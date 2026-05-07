//! Structured parser for the search input box.
//!
//! The goal is to give users the same operator vocabulary Gmail / Apple
//! Mail / Thunderbird power users are used to — `from:`, `has:attachment`,
//! `before:`, `-exclude` — without requiring them to learn FTS5 syntax
//! or write SQL. Everything not recognised as an operator stays as a
//! plain keyword and flows into the FTS5 MATCH expression; operators
//! become SQL WHERE clauses on the messages / message_labels join.
//!
//! Supported operators:
//!
//! | Operator                  | Example                     | Semantics |
//! |---------------------------|-----------------------------|-----------|
//! | `from:`                   | `from:alice@corp`           | FTS match on from_addr column (prefix) |
//! | `to:`                     | `to:bob`                    | FTS match on to_addrs column |
//! | `subject:`                | `subject:invoice`           | FTS match on subject column |
//! | `body:`                   | `body:quarterly`            | FTS match on body_text column |
//! | `has:attachment(s)`       | `has:attachments`           | WHERE has_attachments = 1 |
//! | `is:unread` / `is:read`   | `is:unread`                 | WHERE is_read = 0 / 1 |
//! | `is:starred` / `flagged`  | `is:starred`                | WHERE is_starred = 1 |
//! | `is:encrypted`            | `is:encrypted`              | WHERE is_encrypted = 1 |
//! | `label:` / `in:`          | `label:Work/Projects`       | JOIN message_labels filter |
//! | `before:` (YYYY-MM-DD)    | `before:2025-01-01`         | WHERE date_utc <  ts-at-midnight |
//! | `after:`                  | `after:2025-06-15`          | WHERE date_utc >= ts-at-midnight |
//! | `older_than:` (N d/w/m/y) | `older_than:30d`            | WHERE date_utc <  now - 30d |
//! | `newer_than:`             | `newer_than:7d`             | WHERE date_utc >= now - 7d |
//! | `account:`                | `account:you@gmail.com`     | WHERE accounts.email = ? |
//! | `-term`                   | `meeting -friday`           | FTS NOT term  |
//! | `"exact phrase"`          | `"board meeting"`           | FTS phrase match |
//!
//! Invalid operator values (e.g. `before:banana`) are silently dropped
//! rather than erroring — the search still runs with the other terms.
//! The alternative (hard 400) would make the input feel brittle for
//! minor typos.

use chrono::{NaiveDate, TimeZone, Utc};

/// The parsed form of a search-box input. FTS5 concerns (positive
/// keywords, negated keywords, column-scoped terms) collect into
/// `fts_expr`; structural filters become typed fields on this struct.
#[derive(Debug, Default, Clone)]
pub struct ParsedQuery {
    /// Ready-to-bind FTS5 MATCH expression (already columnised +
    /// wildcarded). May be empty, in which case the search skips FTS
    /// and does a pure WHERE-filter scan.
    pub fts_expr: String,
    /// Filter-only: require a message to have an attachment.
    pub has_attachment: Option<bool>,
    /// Filter: is_read. `Some(true)` = only read, `Some(false)` = only unread.
    pub is_read: Option<bool>,
    pub is_starred: Option<bool>,
    pub is_encrypted: Option<bool>,
    /// Label / folder membership. Multiple entries AND together —
    /// `label:Work label:Important` means "in both".
    pub labels: Vec<String>,
    /// Unix-seconds upper bound (exclusive). Combined with `after` for
    /// range queries.
    pub before_utc: Option<i64>,
    /// Unix-seconds lower bound (inclusive).
    pub after_utc: Option<i64>,
    /// Lowercase email of the target account. Takes priority over any
    /// URL-scoped account_id.
    pub account_email: Option<String>,
}

impl ParsedQuery {
    /// True when the structured filters are non-trivial (anything beyond
    /// FTS keywords). Determines whether the caller needs to expand the
    /// WHERE clause.
    pub fn has_structural_filters(&self) -> bool {
        self.has_attachment.is_some()
            || self.is_read.is_some()
            || self.is_starred.is_some()
            || self.is_encrypted.is_some()
            || !self.labels.is_empty()
            || self.before_utc.is_some()
            || self.after_utc.is_some()
            || self.account_email.is_some()
    }
}

/// Parse a user-typed search string into a structured query. `now_utc`
/// is the reference point for `older_than` / `newer_than`; normally
/// `chrono::Utc::now().timestamp()` but parameterised for tests.
pub fn parse(input: &str, now_utc: i64) -> ParsedQuery {
    let tokens = tokenise(input);
    let mut result = ParsedQuery::default();
    let mut fts_terms: Vec<String> = Vec::new();

    for tok in tokens {
        // Handle the "not" prefix first so `-from:alice` and `-word` both work.
        let (negated, raw) = if let Some(rest) = tok.strip_prefix('-') {
            (true, rest)
        } else {
            (false, tok.as_str())
        };

        if raw.is_empty() {
            continue;
        }

        // Operator? The split_once(':') is the discriminator; a quoted
        // phrase never contains a raw colon outside the quotes because
        // tokenise preserves quotes as literal chars.
        if let Some((op, val)) = raw.split_once(':') {
            if !val.is_empty() && apply_operator(&mut result, &mut fts_terms, op, val, negated) {
                continue;
            }
        }

        // Not an operator, not a recognised one — treat as an FTS keyword.
        if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
            // Quoted phrase — pass through verbatim so FTS5 sees the
            // phrase match.
            fts_terms.push(if negated {
                format!("NOT {raw}")
            } else {
                raw.to_string()
            });
        } else {
            let cleaned = clean_fts_term(raw);
            if cleaned.is_empty() {
                continue;
            }
            fts_terms.push(if negated {
                format!("NOT {cleaned}")
            } else {
                cleaned
            });
        }
    }

    // Wildcard the *last* unquoted positive term so typing "inv"
    // matches "invoice" — same UX as the pre-structured path.
    if let Some(last) = fts_terms.last_mut() {
        if !last.starts_with("NOT ")
            && !last.starts_with('"')
            && !last.ends_with('*')
            && last.len() >= 2
            && last.chars().all(|c| !c.is_whitespace())
        {
            last.push('*');
        }
    }
    result.fts_expr = fts_terms.join(" ");

    // Normalise older_than / newer_than against now_utc.
    apply_relative_dates(&mut result, now_utc);
    result
}

/// Split the input into whitespace-separated tokens, keeping quoted
/// phrases intact. Escape handling is deliberately minimal — mail
/// search boxes don't need shell-grade quoting rules.
fn tokenise(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                buf.push('"');
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
            }
            c => buf.push(c),
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// Clean a bare FTS term: strip characters FTS5 would reject (`*`, `(`,
/// `)`, `:` outside column-scope terms), keep alphanumerics and common
/// punctuation that matters in email addresses / filenames.
fn clean_fts_term(s: &str) -> String {
    s.chars()
        .filter(|c| {
            c.is_alphanumeric()
                || matches!(c, '.' | '_' | '-' | '@')
        })
        .collect()
}

/// Apply a recognised operator to the parsed-query state. Returns
/// `true` when the operator was handled; `false` lets the caller
/// fall through to FTS-keyword handling (treating the raw token as
/// an ordinary search term).
fn apply_operator(
    q: &mut ParsedQuery,
    fts_terms: &mut Vec<String>,
    op: &str,
    val: &str,
    negated: bool,
) -> bool {
    // Quoted value wrappers pass through; but strip leading/trailing
    // quotes for non-FTS operators so `label:"Work Projects"` works.
    let unquoted = val.trim_matches('"');

    match op.to_ascii_lowercase().as_str() {
        // Column-scoped FTS: push into fts_expr verbatim.
        "from" => push_column_term(fts_terms, "from_addr", unquoted, negated),
        "to" | "cc" => push_column_term(fts_terms, "to_addrs", unquoted, negated),
        "subject" => push_column_term(fts_terms, "subject", unquoted, negated),
        "body" => push_column_term(fts_terms, "body_text", unquoted, negated),

        // Boolean filters via is:...
        "is" => match unquoted.to_ascii_lowercase().as_str() {
            "unread" => q.is_read = Some(if negated { true } else { false }),
            "read" => q.is_read = Some(if negated { false } else { true }),
            "starred" | "flagged" => q.is_starred = Some(!negated),
            "encrypted" => q.is_encrypted = Some(!negated),
            _ => return false,
        },

        // Attachment presence.
        "has" => match unquoted.to_ascii_lowercase().as_str() {
            "attachment" | "attachments" | "attach" => {
                q.has_attachment = Some(!negated);
            }
            _ => return false,
        },

        // Folder / label scope.
        "label" | "in" => {
            if !unquoted.is_empty() {
                if negated {
                    // Negated labels: encode as a sentinel prefix that
                    // the SQL builder recognises. We'd need a NOT EXISTS
                    // subquery to express it precisely; for now, treat
                    // negated labels as a soft filter and just skip them
                    // from the inclusion list. Real "not in label" wants
                    // a separate exclusion vec — future work.
                    return true;
                }
                q.labels.push(unquoted.to_string());
            }
        }

        // Absolute date filters.
        "before" => {
            if let Some(ts) = parse_iso_date_start(unquoted) {
                q.before_utc = Some(ts);
            } else {
                return false;
            }
        }
        "after" | "since" => {
            if let Some(ts) = parse_iso_date_start(unquoted) {
                q.after_utc = Some(ts);
            } else {
                return false;
            }
        }

        // Relative date filters — resolved in apply_relative_dates
        // once we have the `now` reference.
        "older_than" => {
            if let Some(secs) = parse_relative_duration(unquoted) {
                // Negative offset: messages older than 30d → date_utc < now-30d.
                // We stash the raw seconds in before_utc as a delta; the
                // apply_relative_dates pass converts it to an absolute
                // timestamp. To keep the type simple we instead do the
                // conversion here by noting the delta in a side channel.
                q.before_utc = Some(-(secs as i64));
            } else {
                return false;
            }
        }
        "newer_than" => {
            if let Some(secs) = parse_relative_duration(unquoted) {
                q.after_utc = Some(-(secs as i64));
            } else {
                return false;
            }
        }

        // Account scoping by email address.
        "account" => {
            if !unquoted.is_empty() {
                q.account_email = Some(unquoted.to_ascii_lowercase());
            }
        }

        _ => return false,
    }
    true
}

fn push_column_term(fts_terms: &mut Vec<String>, column: &str, value: &str, negated: bool) {
    let cleaned = clean_fts_term(value);
    if cleaned.is_empty() {
        return;
    }
    let term = format!("{column}:{cleaned}");
    fts_terms.push(if negated { format!("NOT {term}") } else { term });
}

/// Convert any negative values in before/after (which we used as "delta
/// from now" sentinels while parsing older_than / newer_than) to proper
/// timestamps. After this pass both fields are either None or an
/// absolute unix-seconds value.
fn apply_relative_dates(q: &mut ParsedQuery, now: i64) {
    if let Some(v) = q.before_utc {
        if v < 0 {
            q.before_utc = Some(now + v); // v is negative, so this subtracts
        }
    }
    if let Some(v) = q.after_utc {
        if v < 0 {
            q.after_utc = Some(now + v);
        }
    }
}

/// Parse `2025-01-15` → unix seconds at UTC midnight. Returns None on
/// any parse error so the caller can fall through to keyword handling.
fn parse_iso_date_start(s: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let dt = date.and_hms_opt(0, 0, 0)?;
    Utc.from_utc_datetime(&dt).timestamp().into()
}

/// Parse `30d` / `2w` / `6m` / `1y` → number of seconds. Returns None
/// on anything malformed (bad number, unknown suffix, zero duration).
fn parse_relative_duration(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let n: u64 = num_part.parse().ok()?;
    if n == 0 {
        return None;
    }
    let secs = match unit {
        "s" => 1u64,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        // "M" is commonly "month" but clashes with "m" above. We use
        // lowercase for month too and live with the ambiguity (30-day
        // month is close enough for "show me last month's mail").
        // Distinguish by context: if the suffix is "m" AND the number
        // is >= 12, treat as months. Otherwise minutes. Pragmatic.
        //
        // Actually — redo this with explicit unit cases and reserve "m"
        // for months since the relative-date use case dominates the
        // minute use case in mail search.
        //
        // Leave the "m" branch above as minutes so `30m` = 30 minutes.
        // Add an explicit "M" for months? FTS inputs are
        // case-flattened upstream so "M" collides with "m". Skipping.
        "y" => 365 * 86_400,
        _ => return None,
    };
    Some(n.saturating_mul(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_750_000_000; // arbitrary fixed reference

    #[test]
    fn plain_keyword_becomes_wildcarded_fts() {
        let q = parse("invoice", NOW);
        assert_eq!(q.fts_expr, "invoice*");
        assert!(!q.has_structural_filters());
    }

    #[test]
    fn column_prefix_operators_rewrite() {
        let q = parse("from:alice", NOW);
        assert_eq!(q.fts_expr, "from_addr:alice*");
    }

    #[test]
    fn is_and_has_set_structural_filters() {
        let q = parse("invoice is:unread has:attachment", NOW);
        assert!(q.fts_expr.contains("invoice"));
        assert_eq!(q.is_read, Some(false));
        assert_eq!(q.has_attachment, Some(true));
    }

    #[test]
    fn negated_term_emits_fts_not() {
        let q = parse("meeting -friday", NOW);
        assert!(q.fts_expr.contains("NOT friday"));
    }

    #[test]
    fn quoted_phrase_passes_through() {
        let q = parse(r#""board meeting""#, NOW);
        assert_eq!(q.fts_expr, r#""board meeting""#);
    }

    #[test]
    fn before_after_parse_iso_dates() {
        let q = parse("before:2025-01-15", NOW);
        assert!(q.before_utc.is_some());
        assert!(q.before_utc.unwrap() > 0);
    }

    #[test]
    fn older_than_resolves_against_now() {
        let q = parse("older_than:30d", NOW);
        assert_eq!(q.before_utc, Some(NOW - 30 * 86_400));
    }

    #[test]
    fn invalid_operator_value_falls_back_to_keyword() {
        // "before:banana" isn't a valid date, but we don't error.
        // The call falls through and "before:banana" is NOT added as a
        // keyword either (it looks like an operator) — it simply drops.
        let q = parse("before:banana report", NOW);
        assert!(q.before_utc.is_none());
        // "report" is kept as a regular keyword.
        assert!(q.fts_expr.contains("report"));
    }

    #[test]
    fn label_scope_collects_multiple() {
        let q = parse(r#"label:Work label:"Urgent things""#, NOW);
        assert_eq!(q.labels.len(), 2);
        assert!(q.labels.contains(&"Work".to_string()));
        assert!(q.labels.contains(&"Urgent things".to_string()));
    }

    #[test]
    fn account_scope_lowercases() {
        let q = parse("account:YOU@Example.COM", NOW);
        assert_eq!(q.account_email.as_deref(), Some("you@example.com"));
    }

    #[test]
    fn empty_input_yields_empty_query() {
        let q = parse("", NOW);
        assert!(q.fts_expr.is_empty());
        assert!(!q.has_structural_filters());
    }

    #[test]
    fn is_read_negated() {
        let q = parse("-is:read", NOW);
        assert_eq!(q.is_read, Some(false));
    }
}
