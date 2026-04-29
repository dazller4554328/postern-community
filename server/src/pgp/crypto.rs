//! rPGP 0.14 wrappers. API notes:
//!   - `sign(&mut rng, passphrase_fn)` for secret, 3-arg for public.
//!   - `PublicKeyTrait` in scope is required for `fingerprint()`,
//!     `created_at()`, `expiration()`, `algorithm()`.
//!   - `to_armored_string(ArmorOptions::default())` instead of Option.

use std::io::Cursor;

use pgp::composed::{
    ArmorOptions, Deserializable, KeyType, Message, SecretKeyParamsBuilder, SignedPublicKey,
    SignedSecretKey, SubkeyParamsBuilder,
};
use pgp::crypto::{ecc_curve::ECCCurve, hash::HashAlgorithm, sym::SymmetricKeyAlgorithm};
use pgp::types::{CompressionAlgorithm, Fingerprint, PublicKeyTrait, SecretKeyTrait};
use rand::rngs::OsRng;
use serde::Serialize;
use smallvec::smallvec;

use crate::error::{Error, Result};

const SECS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Serialize)]
pub struct KeyInfo {
    pub fingerprint: String,
    pub user_id: String,
    pub primary_email: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub algorithm: String,
    pub has_secret: bool,
}

#[derive(Debug)]
pub struct GeneratedKey {
    pub fingerprint: String,
    pub user_id: String,
    pub primary_email: Option<String>,
    pub armored_public: String,
    pub armored_secret: String,
    pub created_at: i64,
}

pub fn generate_keypair(user_id: &str) -> Result<GeneratedKey> {
    if user_id.trim().is_empty() {
        return Err(Error::BadRequest(
            "user_id (e.g. 'Alice <alice@example.com>') is required".into(),
        ));
    }

    let mut rng = OsRng;

    let key_params = SecretKeyParamsBuilder::default()
        .key_type(KeyType::EdDSALegacy)
        .can_certify(true)
        .can_sign(true)
        .primary_user_id(user_id.to_string())
        .preferred_symmetric_algorithms(smallvec![
            SymmetricKeyAlgorithm::AES256,
            SymmetricKeyAlgorithm::AES128,
        ])
        .preferred_hash_algorithms(smallvec![HashAlgorithm::SHA2_512, HashAlgorithm::SHA2_256,])
        .preferred_compression_algorithms(smallvec![
            CompressionAlgorithm::ZLIB,
            CompressionAlgorithm::ZIP,
        ])
        .subkey(
            SubkeyParamsBuilder::default()
                // Curve25519 ECDH subkey for encryption. In rPGP 0.14 the
                // KeyType::ECDH variant takes an ECC curve argument.
                .key_type(KeyType::ECDH(ECCCurve::Curve25519))
                .can_encrypt(true)
                .build()
                .map_err(|e| Error::Other(anyhow::anyhow!("subkey params: {e}")))?,
        )
        .build()
        .map_err(|e| Error::Other(anyhow::anyhow!("key params: {e}")))?;

    let secret = key_params
        .generate(&mut rng)
        .map_err(|e| Error::Other(anyhow::anyhow!("generate: {e}")))?;

    let signed_secret = secret
        .sign(&mut rng, String::new)
        .map_err(|e| Error::Other(anyhow::anyhow!("sign secret: {e}")))?;

    let signed_public = signed_secret
        .public_key()
        .sign(&mut rng, &signed_secret, String::new)
        .map_err(|e| Error::Other(anyhow::anyhow!("sign public: {e}")))?;

    let armored_secret = signed_secret
        .to_armored_string(ArmorOptions::default())
        .map_err(|e| Error::Other(anyhow::anyhow!("armor secret: {e}")))?;
    let armored_public = signed_public
        .to_armored_string(ArmorOptions::default())
        .map_err(|e| Error::Other(anyhow::anyhow!("armor public: {e}")))?;

    let info = public_key_info(&signed_public);
    Ok(GeneratedKey {
        fingerprint: info.fingerprint,
        user_id: info.user_id,
        primary_email: info.primary_email,
        armored_public,
        armored_secret,
        created_at: info.created_at,
    })
}

pub fn parse_public_key_info(armored: &str) -> Result<KeyInfo> {
    let (pubkey, _) = SignedPublicKey::from_armor_single(Cursor::new(armored))
        .map_err(|e| Error::BadRequest(format!("not a valid armored public key: {e}")))?;
    Ok(public_key_info(&pubkey))
}

pub fn parse_secret_key_info(armored: &str) -> Result<KeyInfo> {
    let (secret, _) = SignedSecretKey::from_armor_single(Cursor::new(armored))
        .map_err(|e| Error::BadRequest(format!("not a valid armored secret key: {e}")))?;
    // For a SignedSecretKey the embedded public key is already signed.
    // We don't need to re-sign, just derive the info via the same trait methods.
    let mut info = secret_key_info(&secret);
    info.has_secret = true;
    Ok(info)
}

fn public_key_info(key: &SignedPublicKey) -> KeyInfo {
    let fingerprint = fingerprint_hex(&key.fingerprint());
    let primary_user_id = key
        .details
        .users
        .first()
        .map(|u| u.id.id().to_string())
        .unwrap_or_else(|| "(no user id)".to_string());
    let primary_email = extract_email(&primary_user_id);
    let created_at = key.created_at().timestamp();
    let expires_at = key
        .expiration()
        .map(|days| created_at + i64::from(days) * SECS_PER_DAY);
    let algorithm = format!("{:?}", key.algorithm());

    KeyInfo {
        fingerprint,
        user_id: primary_user_id,
        primary_email,
        created_at,
        expires_at,
        algorithm,
        has_secret: false,
    }
}

fn secret_key_info(key: &SignedSecretKey) -> KeyInfo {
    let fingerprint = fingerprint_hex(&key.fingerprint());
    let primary_user_id = key
        .details
        .users
        .first()
        .map(|u| u.id.id().to_string())
        .unwrap_or_else(|| "(no user id)".to_string());
    let primary_email = extract_email(&primary_user_id);
    let created_at = key.created_at().timestamp();
    let expires_at = key
        .expiration()
        .map(|days| created_at + i64::from(days) * SECS_PER_DAY);
    let algorithm = format!("{:?}", key.algorithm());

    KeyInfo {
        fingerprint,
        user_id: primary_user_id,
        primary_email,
        created_at,
        expires_at,
        algorithm,
        has_secret: false,
    }
}

/// Render a `Fingerprint` as a hex string. Different rPGP versions of
/// `Fingerprint` wrap the bytes in different ways; `as_bytes()` is the
/// stable accessor across them.
fn fingerprint_hex(fp: &Fingerprint) -> String {
    hex::encode_upper(fp.as_bytes())
}

fn extract_email(user_id: &str) -> Option<String> {
    if let Some(open) = user_id.find('<') {
        if let Some(close) = user_id[open..].find('>') {
            let e = &user_id[open + 1..open + close];
            if e.contains('@') {
                return Some(e.trim().to_ascii_lowercase());
            }
        }
    }
    if user_id.contains('@') && !user_id.contains(' ') {
        return Some(user_id.trim().to_ascii_lowercase());
    }
    None
}

#[derive(Debug, Serialize)]
pub struct DecryptOutcome {
    pub plaintext: Vec<u8>,
    pub decrypted_with: Option<String>,
    pub signature: SignatureStatus,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SignatureStatus {
    Absent,
    Good,
    UnknownSigner,
    Bad,
}

/// Encrypt `plaintext` to every supplied recipient key, returning an
/// ASCII-armored PGP message ready to drop into the `application/
/// octet-stream` part of an RFC 3156 multipart/encrypted MIME.
///
/// Parses each armored blob, walks the primary + subkeys to pick the
/// first that `is_encryption_key`, and hands the bundle to rpgp's
/// `encrypt_to_keys_seipdv1` with AES-128. Compressed with ZLIB first
/// so long bodies stay reasonable.
pub fn encrypt_to_public_keys(
    recipient_pubkeys_armored: &[&str],
    plaintext: &[u8],
) -> Result<String> {
    if recipient_pubkeys_armored.is_empty() {
        return Err(Error::BadRequest("no recipient keys provided".into()));
    }

    let mut primaries: Vec<SignedPublicKey> = Vec::with_capacity(recipient_pubkeys_armored.len());
    for armored in recipient_pubkeys_armored {
        let (key, _) = SignedPublicKey::from_armor_single(Cursor::new(*armored))
            .map_err(|e| Error::Other(anyhow::anyhow!("parse public key: {e}")))?;
        primaries.push(key);
    }

    // rpgp's encrypt_to_keys_seipdv1 calls `pkey.encrypt()` directly on
    // whatever we hand it. Passing a SignedPublicKey whose primary is
    // EdDSA (signing-only) fails — rpgp doesn't auto-resolve to the
    // ECDH subkey. We extract the encryption subkey ourselves.
    let mut enc_keys: Vec<&pgp::composed::SignedPublicSubKey> = Vec::new();
    for pk in &primaries {
        let sub = pk.public_subkeys.iter().find(|sk| sk.is_encryption_key());
        if let Some(sk) = sub {
            enc_keys.push(sk);
        } else if pk.is_encryption_key() {
            // Rare: primary itself is encryption-capable (e.g. RSA key).
            // Can't push a SignedPublicKey as SignedPublicSubKey; fall
            // back to the old path which works for RSA primaries.
            let key_refs: Vec<&SignedPublicKey> = vec![pk];
            let mut rng = OsRng;
            let literal = Message::new_literal_bytes("", plaintext);
            let compressed = literal
                .compress(CompressionAlgorithm::ZLIB)
                .map_err(|e| Error::Other(anyhow::anyhow!("compress: {e}")))?;
            let encrypted = compressed
                .encrypt_to_keys_seipdv1(&mut rng, SymmetricKeyAlgorithm::AES128, &key_refs)
                .map_err(|e| Error::Other(anyhow::anyhow!("encrypt: {e}")))?;
            return encrypted
                .to_armored_string(ArmorOptions::default())
                .map_err(|e| Error::Other(anyhow::anyhow!("armor: {e}")));
        } else {
            return Err(Error::BadRequest(format!(
                "key {} has no encryption-capable subkey",
                fingerprint_hex(&pk.fingerprint())
            )));
        }
    }

    let key_refs: Vec<&pgp::composed::SignedPublicSubKey> = enc_keys.iter().copied().collect();
    let mut rng = OsRng;
    let literal = Message::new_literal_bytes("", plaintext);
    let compressed = literal
        .compress(CompressionAlgorithm::ZLIB)
        .map_err(|e| Error::Other(anyhow::anyhow!("compress: {e}")))?;
    let encrypted = compressed
        .encrypt_to_keys_seipdv1(&mut rng, SymmetricKeyAlgorithm::AES128, &key_refs)
        .map_err(|e| Error::Other(anyhow::anyhow!("encrypt: {e}")))?;

    encrypted
        .to_armored_string(ArmorOptions::default())
        .map_err(|e| Error::Other(anyhow::anyhow!("armor: {e}")))
}

pub fn decrypt_armored(
    ciphertext: &[u8],
    secret_keys_armored: &[&str],
) -> Result<Option<DecryptOutcome>> {
    let message = Message::from_armor_single(Cursor::new(ciphertext))
        .map(|(m, _)| m)
        .or_else(|_| Message::from_bytes(Cursor::new(ciphertext)))
        .map_err(|e| Error::Other(anyhow::anyhow!("parse pgp message: {e}")))?;

    for armored in secret_keys_armored {
        let (secret, _) = match SignedSecretKey::from_armor_single(Cursor::new(*armored)) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let decrypted = match message.decrypt(String::new, &[&secret]) {
            Ok((d, _key_ids)) => d,
            Err(_) => continue,
        };

        let plaintext = extract_literal(decrypted)?;
        return Ok(Some(DecryptOutcome {
            plaintext,
            decrypted_with: Some(fingerprint_hex(&secret.fingerprint())),
            signature: SignatureStatus::Absent,
        }));
    }
    Ok(None)
}

fn extract_literal(msg: Message) -> Result<Vec<u8>> {
    match msg {
        Message::Literal(lit) => Ok(lit.data().to_vec()),
        Message::Compressed(_) => {
            let decompressed = msg
                .decompress()
                .map_err(|e| Error::Other(anyhow::anyhow!("decompress: {e}")))?;
            extract_literal(decompressed)
        }
        other => {
            // Signed/encrypted-nested — for Sprint 4.2 MVP we surface the
            // armored re-export as text rather than recursing further.
            let s = other
                .to_armored_string(ArmorOptions::default())
                .map_err(|e| Error::Other(anyhow::anyhow!("re-armor: {e}")))?;
            Ok(s.into_bytes())
        }
    }
}
