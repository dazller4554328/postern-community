//! OpenPGP support via rPGP.
//!
//! Scope for Sprint 4.2 (receive + discovery):
//!   - Key generation + import + export (keyring)
//!   - Autocrypt header harvesting from incoming mail
//!   - Recipient key discovery: WKD → keyserver fallback
//!   - Decryption of inbound PGP-encrypted messages
//!   - Signature verification surfaced to forensics
//!
//! Send-side encryption + Autocrypt injection arrive in the SMTP sprint.

mod autocrypt;
mod crypto;
mod discover;
mod keyring;
mod publish;

pub use autocrypt::{build_autocrypt_header, parse_autocrypt_header};
pub use crypto::{
    decrypt_armored, encrypt_to_public_keys, generate_keypair, parse_public_key_info,
    parse_secret_key_info,
};
pub use discover::{discover_key, DiscoveryResult, DiscoverySource};
pub use keyring::{KeyRow, KeySource, NewKey};
pub use publish::{check_keyserver, publish_to_hagrid, KeyserverStatus, PublishResult};

use crate::{storage::Db, vault::Vault};
use mail_parser::{HeaderValue, MessageParser};

/// Scan a raw RFC822 for an `Autocrypt:` header and, if found, add
/// the embedded public key to our keyring. Fire-and-forget — any
/// parse failure is silently dropped, harvest is best-effort. Public
/// keys don't go through the secrets table, but pgp_upsert still
/// needs a Vault handle for its interface consistency.
pub fn harvest_autocrypt(raw: &[u8], db: &Db, vault: &Vault) {
    let parser = MessageParser::default();
    let Some(msg) = parser.parse(raw) else { return };
    for h in &msg.parts[0].headers {
        if !h.name.as_str().eq_ignore_ascii_case("autocrypt") {
            continue;
        }
        let value = match &h.value {
            HeaderValue::Text(s) => s.to_string(),
            HeaderValue::TextList(list) => list
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(";"),
            _ => continue,
        };
        let Some(ac) = parse_autocrypt_header(&value) else {
            continue;
        };
        let Ok(info) = parse_public_key_info(&ac.armored_public_key) else {
            continue;
        };
        let _ = db.pgp_upsert(
            &NewKey {
                info: &info,
                armored_public: &ac.armored_public_key,
                armored_secret: None,
                source: KeySource::Autocrypt,
            },
            vault,
        );
        break;
    }
}

/// Attempt to decrypt a PGP-encrypted message embedded in `raw` using
/// any secret keys we hold. Returns the decrypted plaintext on success.
/// Scans both the PGP/MIME structure (multipart/encrypted) and any
/// inline armored blocks.
pub fn try_decrypt(raw: &[u8], db: &Db, vault: &Vault) -> Option<Vec<u8>> {
    use mail_parser::PartType;

    // No secrets available when locked — just skip the decrypt attempt
    // instead of surfacing an error to the caller.
    if !vault.is_unlocked() {
        return None;
    }
    let secrets = db.pgp_all_secrets(vault).ok()?;
    if secrets.is_empty() {
        return None;
    }
    let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();

    let parser = MessageParser::default();
    let msg = parser.parse(raw)?;

    // PGP/MIME: application/octet-stream sub-part inside multipart/encrypted.
    for part in msg.parts.iter() {
        if let PartType::Binary(body) | PartType::InlineBinary(body) = &part.body {
            if let Ok(Some(outcome)) = decrypt_armored(body, &secret_refs) {
                return Some(outcome.plaintext);
            }
        }
        if let PartType::Text(text) = &part.body {
            if text.contains("-----BEGIN PGP MESSAGE-----") {
                if let Ok(Some(outcome)) = decrypt_armored(text.as_bytes(), &secret_refs) {
                    return Some(outcome.plaintext);
                }
            }
        }
    }
    None
}

/// Gather armored public keys for every recipient email, bail if any
/// are missing, then hand off to `encrypt_to_public_keys`. Returns the
/// armored ciphertext ready for the RFC 3156 octet-stream part.
pub fn encrypt_for_recipients(
    db: &Db,
    _vault: &Vault,
    recipient_emails: &[String],
    plaintext: &[u8],
) -> crate::error::Result<String> {
    let mut armored_keys: Vec<String> = Vec::with_capacity(recipient_emails.len());
    let mut missing: Vec<String> = Vec::new();
    for email in recipient_emails {
        match db.pgp_public_armored_for_email(email)? {
            Some(k) => armored_keys.push(k),
            None => missing.push(email.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(crate::error::Error::BadRequest(format!(
            "no public key in keyring for: {}",
            missing.join(", ")
        )));
    }
    let refs: Vec<&str> = armored_keys.iter().map(String::as_str).collect();
    encrypt_to_public_keys(&refs, plaintext)
}
