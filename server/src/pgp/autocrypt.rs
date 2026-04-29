//! Parse the `Autocrypt:` header per
//! <https://autocrypt.org/level1.html#autocrypt-header>.
//!
//! Format: `addr=<email>; [prefer-encrypt=mutual|nopreference;] keydata=<base64>`
//!
//! We return the armored public key (decoded base64, re-armored) plus
//! the claimed email and prefer-encrypt pref. Caller feeds it to the
//! keyring with source='autocrypt'.

use base64::{engine::general_purpose::STANDARD as B64, Engine};

#[derive(Debug, Clone)]
pub struct ParsedAutocrypt {
    pub addr: String,
    pub prefer_encrypt: Option<String>,
    pub armored_public_key: String,
}

pub fn parse_autocrypt_header(value: &str) -> Option<ParsedAutocrypt> {
    let mut addr: Option<String> = None;
    let mut prefer_encrypt: Option<String> = None;
    let mut keydata: Option<String> = None;

    // Split on ';' but be resilient to multi-line folded headers — the
    // mail parser usually unfolds, but keydata= can still contain '\r\n'
    // remnants or spaces.
    for part in value.split(';') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim().trim_matches('"');
        match key.as_str() {
            "addr" => addr = Some(val.to_ascii_lowercase()),
            "prefer-encrypt" => prefer_encrypt = Some(val.to_ascii_lowercase()),
            "keydata" => {
                // Strip internal whitespace from the base64 blob.
                let cleaned: String = val.chars().filter(|c| !c.is_whitespace()).collect();
                keydata = Some(cleaned);
            }
            _ => {}
        }
    }

    let (addr, keydata) = (addr?, keydata?);
    let raw_key = B64.decode(keydata.as_bytes()).ok()?;
    // Re-armor so the downstream keyring path is uniform with pasted keys.
    let armored = armor_public_key(&raw_key);

    Some(ParsedAutocrypt {
        addr,
        prefer_encrypt,
        armored_public_key: armored,
    })
}

fn armor_public_key(raw: &[u8]) -> String {
    let b64 = B64.encode(raw);
    let mut out = String::with_capacity(b64.len() + 128);
    out.push_str("-----BEGIN PGP PUBLIC KEY BLOCK-----\n");
    out.push_str("Comment: Postern — harvested from Autocrypt header\n\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push('\n');
    }
    out.push_str("-----END PGP PUBLIC KEY BLOCK-----\n");
    out
}

/// Build an `Autocrypt:` header line from our armored public key so
/// receiving clients can harvest it the same way we harvest theirs.
/// Returns the full `Autocrypt: addr=...; keydata=...` string (header
/// name included) with the keydata folded per RFC 5322 continuation
/// rules. Returns None if the armor can't be decoded.
pub fn build_autocrypt_header(addr: &str, armored_public_key: &str) -> Option<String> {
    let binary = dearmor_public_key(armored_public_key)?;
    let encoded = B64.encode(&binary);

    // Autocrypt Level 1 ships `prefer-encrypt=mutual` when the sender
    // has committed to always encrypting when they can. That matches
    // Postern's stance — we've already auto-enabled encryption for
    // every recipient whose key we have.
    let mut out = String::from("Autocrypt: addr=");
    out.push_str(addr);
    out.push_str("; prefer-encrypt=mutual; keydata=");

    // Fold the base64 across continuation lines. Each continuation
    // starts with a leading space (required by RFC 5322). 72 chars
    // per chunk keeps the physical line under 78 incl. CRLF.
    for chunk in encoded.as_bytes().chunks(72) {
        out.push_str("\r\n ");
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    Some(out)
}

/// Extract the binary OpenPGP public-key packet from a PGP armor
/// block. Handles armors with optional headers (`Version:`, `Comment:`),
/// blank separator, base64 body, and the trailing `=CRC24` line.
fn dearmor_public_key(armored: &str) -> Option<Vec<u8>> {
    let lines: Vec<&str> = armored.lines().collect();
    let begin = lines
        .iter()
        .position(|l| l.trim().starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"))?;
    let end = lines
        .iter()
        .position(|l| l.trim().starts_with("-----END PGP PUBLIC KEY BLOCK-----"))?;
    if end <= begin + 1 {
        return None;
    }
    let inside = &lines[begin + 1..end];

    // Body starts after the first blank line when armor headers are
    // present. If the first non-empty line is already base64 (no
    // colon), body starts there. Colons never appear in base64 so
    // this test is unambiguous.
    let first_nonempty = inside.iter().position(|l| !l.trim().is_empty())?;
    let body_start = if inside[first_nonempty].contains(':') {
        inside.iter().position(|l| l.trim().is_empty())? + 1
    } else {
        first_nonempty
    };

    let mut collected = String::new();
    for line in &inside[body_start..] {
        let l = line.trim();
        // CRC24 footer — single `=` followed by exactly 4 base64
        // chars. That's the canonical terminator and we stop here.
        if l.starts_with('=') && l.len() <= 5 {
            break;
        }
        collected.extend(l.chars().filter(|c| !c.is_whitespace()));
    }
    B64.decode(collected.as_bytes()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_header() {
        // Fake non-validating base64 — we're testing the parser, not crypto.
        let h = "addr=alice@example.com; prefer-encrypt=mutual; keydata=QUJDREVG";
        let p = parse_autocrypt_header(h).unwrap();
        assert_eq!(p.addr, "alice@example.com");
        assert_eq!(p.prefer_encrypt.as_deref(), Some("mutual"));
        assert!(p.armored_public_key.contains("BEGIN PGP PUBLIC KEY BLOCK"));
    }

    #[test]
    fn strips_whitespace_from_keydata() {
        let h = "addr=bob@example.com; keydata= AAAA BBBB CCCC ";
        let p = parse_autocrypt_header(h).unwrap();
        assert!(
            p.armored_public_key.contains("AAAABBBBCCCC") || p.armored_public_key.contains("AAAA")
        );
    }

    #[test]
    fn rejects_missing_keydata() {
        assert!(parse_autocrypt_header("addr=x@y.com").is_none());
    }

    #[test]
    fn build_round_trips_through_parse() {
        // Synthesize a minimal armored public key block with the
        // base64 "QUJDREVG" (= "ABCDEF" raw). Build an Autocrypt
        // header from it, then parse it back and confirm the
        // payload matches.
        let armored = "\
-----BEGIN PGP PUBLIC KEY BLOCK-----\n\
Version: Postern\n\
\n\
QUJDREVG\n\
-----END PGP PUBLIC KEY BLOCK-----\n";
        let header = build_autocrypt_header("alice@example.com", armored).unwrap();
        assert!(header.starts_with("Autocrypt: addr=alice@example.com"));
        assert!(header.contains("prefer-encrypt=mutual"));
        assert!(header.contains("keydata="));

        // Parser expects the *value* (everything after "Autocrypt: ")
        // and works over an already-unfolded single line.
        let value = header
            .strip_prefix("Autocrypt: ")
            .unwrap()
            .replace("\r\n ", "");
        let parsed = parse_autocrypt_header(&value).unwrap();
        assert_eq!(parsed.addr, "alice@example.com");
        assert_eq!(parsed.prefer_encrypt.as_deref(), Some("mutual"));
        assert!(parsed.armored_public_key.contains("QUJDREVG"));
    }

    #[test]
    fn dearmor_handles_headerless_armor() {
        let armored =
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\nQUJDREVG\n-----END PGP PUBLIC KEY BLOCK-----\n";
        let out = dearmor_public_key(armored).unwrap();
        assert_eq!(out, b"ABCDEF");
    }

    #[test]
    fn dearmor_skips_crc_line() {
        let armored = "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nQUJDREVG\n=AbCd\n-----END PGP PUBLIC KEY BLOCK-----\n";
        let out = dearmor_public_key(armored).unwrap();
        assert_eq!(out, b"ABCDEF");
    }
}
