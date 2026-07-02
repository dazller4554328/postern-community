//! X-GM-LABELS support.
//!
//! Gmail's IMAP extension returns the full label set for every message —
//! including labels the user has *not* opted into exposing as IMAP
//! folders. That's the whole point of this module: the default Gmail
//! IMAP view hides categories (Updates, Promotions, Social, Forums)
//! and most user labels, and the only way to see them without making
//! the user toggle "Show in IMAP" in Gmail settings is to read
//! X-GM-LABELS off messages we already have access to.
//!
//! The `imap` 2.4 crate doesn't model the extension, so we issue the
//! FETCH as a raw command and parse the untagged response ourselves.
//! The parser is intentionally minimal: it recognises atoms, quoted
//! strings, and the parenthesised label list — enough for real Gmail
//! responses. Modified UTF-7 decoding is out of scope for v1; labels
//! with mUTF-7 segments fall through as-is and still work as unique
//! label identifiers, they just look ugly in the UI until the user
//! renames them.

/// A system-label translation. `\Inbox` → `INBOX`, and so on, so the
/// label names match the folder names the rest of the code already
/// knows about. Unknown `\`-prefixed labels (e.g. `\Muted`, `\Phished`)
/// are intentionally dropped — they're Gmail internals, not things the
/// user wants as a folder.
pub fn translate_system_label(raw: &str) -> Option<&'static str> {
    match raw {
        "\\Inbox" => Some("INBOX"),
        "\\Sent" => Some("[Gmail]/Sent Mail"),
        "\\Draft" | "\\Drafts" => Some("[Gmail]/Drafts"),
        "\\Starred" => Some("[Gmail]/Starred"),
        "\\Important" => Some("[Gmail]/Important"),
        "\\Trash" => Some("[Gmail]/Trash"),
        "\\Spam" | "\\Junk" => Some("[Gmail]/Spam"),
        _ => None,
    }
}

/// Normalise a raw X-GM-LABELS token to the folder name Postern stores
/// it under. Handles four cases:
///
/// 1. System labels — translated via `translate_system_label`.
///    Ignored `\`-prefixed tokens return None.
/// 2. `CATEGORY_*` — kept verbatim; the folders API knows how to
///    display-name them (Updates, Promotions, etc.).
/// 3. Everything else — treated as a user label and kept verbatim.
pub fn normalise_label(raw: &str) -> Option<String> {
    if raw.starts_with('\\') {
        return translate_system_label(raw).map(std::borrow::ToOwned::to_owned);
    }
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_owned())
}

/// Return `"system"` / `"gmail_category"` / `"user"` — matches the
/// `kind` column on the labels table so the folders API can bucket
/// them correctly in the sidebar.
pub fn kind_for_label(name: &str) -> &'static str {
    if name.starts_with("[Gmail]/") || name == "INBOX" {
        "system"
    } else if name.starts_with("CATEGORY_") {
        "gmail_category"
    } else {
        "user"
    }
}

/// Fold a Gmail-namespaced label name onto its plain-IMAP equivalent
/// when the target account isn't a Gmail account. Defends user-input
/// boundaries — rule actions and folder-create requests — against
/// inserting a "[Gmail]/Trash" row on a regular IMAP account, which
/// would later cause the folder-action `empty` flow to dispatch a
/// SELECT the IMAP server rejects ("Name must not have '/'
/// characters").
///
/// Returns a borrowed `&str` when no rewrite is needed so the common
/// path stays allocation-free.
///
/// **Do not** call this in sync paths — those pass real folder names
/// the server actually advertises; rewriting would mis-target the
/// subsequent IMAP operations.
pub fn canonicalise_label_for_kind(
    name: &str,
    kind: crate::storage::AccountKind,
) -> std::borrow::Cow<'_, str> {
    if !matches!(kind, crate::storage::AccountKind::Imap) {
        return std::borrow::Cow::Borrowed(name);
    }
    match name {
        "[Gmail]/Trash" => std::borrow::Cow::Borrowed("Trash"),
        "[Gmail]/Spam" => std::borrow::Cow::Borrowed("Spam"),
        "[Gmail]/Sent Mail" => std::borrow::Cow::Borrowed("Sent"),
        "[Gmail]/Drafts" => std::borrow::Cow::Borrowed("Drafts"),
        _ => std::borrow::Cow::Borrowed(name),
    }
}

/// Split the inside of the label list into atoms and quoted strings.
/// Whitespace separates tokens. Quoted strings may contain escaped
/// quotes and backslashes — we unescape them.
fn tokenise(inside: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = inside.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'"' {
            // Quoted string.
            let mut j = i + 1;
            let mut buf = Vec::new();
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'\\' && j + 1 < bytes.len() {
                    buf.push(bytes[j + 1]);
                    j += 2;
                } else if c == b'"' {
                    j += 1;
                    break;
                } else {
                    buf.push(c);
                    j += 1;
                }
            }
            if let Ok(s) = String::from_utf8(buf) {
                out.push(s);
            }
            i = j;
        } else {
            // Atom — read until whitespace.
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                out.push(s.to_owned());
            }
        }
    }
    out
}

/// Split a raw FETCH response (bytes from `run_command_and_read_response`)
/// into one chunk per `* N FETCH (...)` block. Each returned chunk
/// starts at `FETCH (` and runs to the matching closing `)`. Used by
/// the rescan path so we can regex-extract attributes inside a single
/// message's attribute list without confusing it with a neighbour's.
///
/// Literal-aware: `{N}` markers consume N bytes of data before the
/// parser resumes looking for the matching `)`. Mismatched parens
/// inside literal bodies are therefore ignored correctly.
pub fn split_fetch_blocks(raw: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut i = 0;
    let needle = b" FETCH (";
    while i < raw.len() {
        // Find next "* <digits> FETCH (" — we search for " FETCH (" and
        // then verify the preceding token is "* <digits>".
        let Some(rel) = find_subslice(&raw[i..], needle) else {
            break;
        };
        let start = i + rel + needle.len();
        // Walk from `start` matching parens, tolerating literals.
        let Some(end) = find_matching_close_paren(&raw[start..]) else {
            break;
        };
        out.push(raw[start..start + end].to_vec());
        i = start + end + 1;
    }
    out
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Given bytes starting just AFTER an open paren, return the offset
/// of the matching close paren, accounting for IMAP literals (`{N}`)
/// and quoted strings. Returns None if no matching paren is found.
fn find_matching_close_paren(bytes: &[u8]) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut i = 0;
    let mut in_quote = false;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if in_quote {
            match b {
                b'\\' => escape = true,
                b'"' => in_quote = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => in_quote = true,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'{' => {
                // Literal: {N}\r\n followed by N bytes we must skip
                // wholesale or the parser will find spurious parens
                // inside an RFC 2822 header block.
                let rel_close = bytes[i + 1..].iter().position(|&c| c == b'}')?;
                let n_end = i + 1 + rel_close;
                let num_str = std::str::from_utf8(&bytes[i + 1..n_end]).ok()?;
                let n: usize = num_str.parse().ok()?;
                // After `}` we expect `\r\n` then N bytes of literal.
                let after_brace = n_end + 1;
                let literal_start = if bytes.get(after_brace) == Some(&b'\r')
                    && bytes.get(after_brace + 1) == Some(&b'\n')
                {
                    after_brace + 2
                } else if bytes.get(after_brace) == Some(&b'\n') {
                    after_brace + 1
                } else {
                    after_brace
                };
                i = literal_start.saturating_add(n);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Extract the first `UID <n>` from a fetch attribute block.
pub fn extract_uid(block: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(block).ok()?;
    let idx = text.find("UID ")?;
    let rest = &text[idx + 4..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract the X-GM-LABELS list from a fetch attribute block as a list
/// of raw label tokens (backslash-prefixed for system labels, quoted
/// or atom for everything else).
pub fn extract_labels(block: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(block);
    let Some(idx) = text.find("X-GM-LABELS ") else {
        return Vec::new();
    };
    let rest = &text[idx + "X-GM-LABELS ".len()..];
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Vec::new();
    }
    let Some(inside_end) = find_matching_close_paren(&bytes[1..]) else {
        return Vec::new();
    };
    let inside = &rest[1..=inside_end];
    tokenise(inside)
}

/// Extract the Message-ID value from a fetch attribute block. Looks for
/// a `BODY[HEADER.FIELDS (MESSAGE-ID)] {N}\r\n<literal>` pattern and
/// pulls the value from the literal.
pub fn extract_message_id(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    // Find the literal marker right after the section selector. Being
    // lenient about case — some servers normalise to lowercase.
    let needle_variants = [
        "BODY[HEADER.FIELDS (MESSAGE-ID)]",
        "BODY[HEADER.FIELDS (Message-ID)]",
    ];
    let marker = needle_variants
        .iter()
        .find_map(|n| text.find(n).map(|i| i + n.len()))?;
    let after = text[marker..].trim_start();
    let literal_bytes = if after.starts_with('{') {
        let close = after.find('}')?;
        let n: usize = after[1..close].parse().ok()?;
        // Skip CRLF/LF after the closing brace.
        let body_start = close + 1;
        let body_start = if after[body_start..].starts_with("\r\n") {
            body_start + 2
        } else if after[body_start..].starts_with('\n') {
            body_start + 1
        } else {
            body_start
        };
        &after[body_start..body_start + n.min(after.len() - body_start)]
    } else if let Some(body) = after.strip_prefix('"') {
        // Quoted string alternative. Gmail typically uses literals but
        // let's not bet on it.
        let end = body.find('"')?;
        &body[..end]
    } else {
        return None;
    };

    for line in literal_bytes.lines() {
        let mut parts = line.splitn(2, ':');
        let name = parts.next()?.trim();
        if !name.eq_ignore_ascii_case("message-id") {
            continue;
        }
        let val = parts.next()?.trim();
        let val = val.trim_start_matches('<').trim_end_matches('>').trim();
        if !val.is_empty() {
            // Postern's regular sync normalises Message-IDs to
            // `<id>` via parser::normalize_mid and stores the wrapped
            // form in messages.message_id. The rescan's local lookup
            // has to use the same convention or it will miss every
            // row.
            return Some(format!("<{val}>"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_system_label() {
        assert_eq!(normalise_label("\\Inbox").as_deref(), Some("INBOX"));
        assert_eq!(
            normalise_label("\\Sent").as_deref(),
            Some("[Gmail]/Sent Mail")
        );
        assert_eq!(normalise_label("\\Muted"), None);
        assert_eq!(
            normalise_label("CATEGORY_UPDATES").as_deref(),
            Some("CATEGORY_UPDATES")
        );
        assert_eq!(
            normalise_label("Receipts/2026").as_deref(),
            Some("Receipts/2026")
        );
    }

    #[test]
    fn kind_classifies_by_shape() {
        assert_eq!(kind_for_label("INBOX"), "system");
        assert_eq!(kind_for_label("[Gmail]/Trash"), "system");
        assert_eq!(kind_for_label("CATEGORY_UPDATES"), "gmail_category");
        assert_eq!(kind_for_label("Receipts"), "user");
    }

    // ── extract_uid — pulls UID off a fetch attribute block ──────────

    #[test]
    fn extract_uid_finds_first_uid_token() {
        let block = b"UID 12345 FLAGS (\\Seen) X-GM-LABELS (\\Inbox)";
        assert_eq!(extract_uid(block), Some(12345));
    }

    #[test]
    fn extract_uid_stops_at_non_digit() {
        let block = b"UID 999 RFC822.SIZE 12";
        assert_eq!(extract_uid(block), Some(999));
    }

    #[test]
    fn extract_uid_returns_none_when_absent() {
        let block = b"FLAGS (\\Seen) X-GM-LABELS (\\Inbox)";
        assert_eq!(extract_uid(block), None);
    }

    // ── extract_labels — token list from X-GM-LABELS ─────────────────

    #[test]
    fn extract_labels_parses_mixed_atom_and_quoted() {
        let block = br#"UID 1 X-GM-LABELS (\Inbox \Important "User Label" "weird (label) name")"#;
        let labels = extract_labels(block);
        assert!(labels.contains(&"\\Inbox".to_owned()));
        assert!(labels.contains(&"\\Important".to_owned()));
        assert!(labels.contains(&"User Label".to_owned()));
        // Nested-paren-in-quoted-string is the case the matching-paren
        // walker has to get right — getting it wrong cuts off the
        // labels list at the first `)` and silently truncates labels.
        assert!(labels.contains(&"weird (label) name".to_owned()));
    }

    #[test]
    fn extract_labels_returns_empty_when_no_xgm_labels() {
        let block = b"UID 1 FLAGS (\\Seen)";
        assert!(extract_labels(block).is_empty());
    }

    #[test]
    fn extract_labels_handles_empty_list() {
        let block = b"UID 1 X-GM-LABELS ()";
        assert!(extract_labels(block).is_empty());
    }

    // ── split_fetch_blocks — splits FETCH responses into per-msg blocks

    #[test]
    fn split_fetch_blocks_splits_on_multiple_fetches() {
        let raw =
            b"* 1 FETCH (UID 10 X-GM-LABELS (\\Inbox))\r\n* 2 FETCH (UID 11 X-GM-LABELS (\\Sent))\r\nA001 OK\r\n";
        let blocks = split_fetch_blocks(raw);
        assert_eq!(blocks.len(), 2);
        assert_eq!(extract_uid(&blocks[0]), Some(10));
        assert_eq!(extract_uid(&blocks[1]), Some(11));
    }

    /// IMAP literal markers `{N}` consume N raw bytes — the data block
    /// can contain a `)` byte that must NOT close the FETCH parens.
    /// Getting this wrong splits message boundaries inside the data.
    #[test]
    fn split_fetch_blocks_respects_imap_literals() {
        // Two messages, the first carries a 5-byte literal whose body
        // contains a `)` byte mid-literal. The walker must consume the
        // 5 bytes verbatim, see the *outer* `)`, then resume scanning.
        let raw = b"* 1 FETCH (UID 1 BODY[HEADER] {5}\r\na)b)c)\r\n* 2 FETCH (UID 2)\r\n";
        let blocks = split_fetch_blocks(raw);
        assert_eq!(blocks.len(), 2, "got {} blocks", blocks.len());
        assert_eq!(extract_uid(&blocks[0]), Some(1));
        assert_eq!(extract_uid(&blocks[1]), Some(2));
    }

    // ── canonicalise_label_for_kind — user-input boundary guard ──────

    use crate::storage::AccountKind;

    #[test]
    fn canonicalise_folds_gmail_namespace_on_imap_accounts() {
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/Trash", AccountKind::Imap).as_ref(),
            "Trash"
        );
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/Spam", AccountKind::Imap).as_ref(),
            "Spam"
        );
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/Sent Mail", AccountKind::Imap).as_ref(),
            "Sent"
        );
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/Drafts", AccountKind::Imap).as_ref(),
            "Drafts"
        );
    }

    #[test]
    fn canonicalise_preserves_gmail_namespace_on_gmail_accounts() {
        // Gmail accounts genuinely use these label names — must pass
        // through unchanged or sync routing breaks.
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/Trash", AccountKind::Gmail).as_ref(),
            "[Gmail]/Trash"
        );
    }

    #[test]
    fn canonicalise_passes_unrelated_names_through() {
        assert_eq!(
            canonicalise_label_for_kind("INBOX", AccountKind::Imap).as_ref(),
            "INBOX"
        );
        assert_eq!(
            canonicalise_label_for_kind("Work/2026", AccountKind::Imap).as_ref(),
            "Work/2026"
        );
        // No mapping for All Mail / Important / Starred — they pass
        // through; the migration handles dropping them from existing
        // data, but a stray future write isn't catastrophic.
        assert_eq!(
            canonicalise_label_for_kind("[Gmail]/All Mail", AccountKind::Imap).as_ref(),
            "[Gmail]/All Mail"
        );
    }
}
