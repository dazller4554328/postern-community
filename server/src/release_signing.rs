//! Compile-time-embedded Ed25519 public key for release-tarball
//! verification.
//!
//! The signing private key lives on the build host (`postern-email`)
//! at `~/.config/postern-release-signing/priv.pem` and never leaves
//! that machine. The post-receive hook on each
//! host produces a detached `.sig` next to every released tarball.
//!
//! Two verification points consume the artefact:
//!   * The host-side updater (`deploy/auto-deploy/postern-updater.sh`)
//!     verifies the tarball bytes against the `.sig` before extracting.
//!     **This is the strong anti-piracy gate** — a cracker who clones
//!     the update server still can't mint signed releases without the
//!     private key.
//!   * The Rust client (this module) validates that the public key is
//!     well-formed at startup, so a build-chain corruption surfaces
//!     loudly before users hit it. Phase 1b will use the same key to
//!     verify activation tokens minted by the WHMCS license module.
//!
//! Rotation: regenerate, copy private key to every build host, replace
//! `postern-release.pub` here, ship a release. See
//! `deploy/release-signing/README.md` for the full procedure.

use ed25519_dalek::{pkcs8::DecodePublicKey, VerifyingKey};

/// PEM bytes of the release-signing public key. Embedded at compile
/// time so a tampered file on disk cannot redirect verification to a
/// different keypair.
pub const RELEASE_PUBLIC_KEY_PEM: &[u8] =
    include_bytes!("../../deploy/release-signing/postern-release.pub");

/// Parse the embedded PEM into a verifying key. Errors are
/// programmer errors — they only fire if someone replaces the .pub
/// file with malformed content.
pub fn release_verifying_key() -> Result<VerifyingKey, ed25519_dalek::pkcs8::spki::Error> {
    let pem = std::str::from_utf8(RELEASE_PUBLIC_KEY_PEM)
        .map_err(|_| ed25519_dalek::pkcs8::spki::Error::KeyMalformed)?;
    VerifyingKey::from_public_key_pem(pem)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build-time sanity: the embedded pubkey must parse. If this
    /// fails the post-receive hook's signing key is mismatched with
    /// what the binary will check against, and every release would
    /// be rejected by the host updater as soon as it shipped.
    #[test]
    fn embedded_release_key_parses() {
        let key = release_verifying_key().expect("embedded pubkey should parse");
        // VerifyingKey::as_bytes returns the 32-byte raw key — Ed25519
        // pubkeys are always exactly 32 bytes, so anything else means
        // the curve type is wrong.
        assert_eq!(key.as_bytes().len(), 32);
    }
}
