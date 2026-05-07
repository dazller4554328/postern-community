//! Embedding indexer.
//!
//! Runs as a single tokio task spawned from `main::serve`. Wakes
//! every `TICK_SECS`, checks `vault.is_unlocked() && llm.is_some()`,
//! and if both, calls `db.messages_missing_embedding(model, BATCH)`
//! and embeds whichever messages haven't been indexed against the
//! configured model yet.
//!
//! Why a tick rather than a tight loop:
//!   • The pool isn't pinned for long stretches — a 50-message
//!     batch (the default BATCH) takes maybe 5–10s on CPU + Ollama;
//!     the rest of the minute the indexer is idle and the pool is
//!     free for sync, send, and HTTP handlers.
//!   • Failures (Ollama crashed, model mid-pull) just retry next
//!     tick. No exponential backoff, no error storms — the slow
//!     cadence is its own rate-limiter.
//!
//! Why one task and not per-account:
//!   • Embeddings are model-specific, not account-specific. A
//!     single walker is the right granularity. Ordering inside the
//!     query (date_utc DESC) means recent mail gets indexed first,
//!     so the user can ask about today's email before the backfill
//!     of 2018's email finishes.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::{info, warn};

use super::holder::LlmHolder;
use super::provider::{EmbedRequest, LlmProvider};
use crate::storage::Db;
use crate::vault::Vault;

/// How often the indexer checks for work. 60s matches the backup
/// scheduler — a 1-minute lag on indexing newly-arrived mail is
/// fine because the user-perceived path is "open inbox, see
/// message, ask question about it later", not "ask immediately".
const TICK_SECS: u64 = 60;

/// Max messages embedded per tick. Caps the time the pool can be
/// busy with indexing inserts and the time Ollama is busy with
/// embed calls. 50 × ~150ms (CPU) ≈ 7.5s. Easy to tune via env.
const DEFAULT_BATCH: usize = 50;

/// Default embedding model. Mirrors the http::ai handler default —
/// kept in sync via env overrides. If they drift, the indexer
/// embeds against model A and the handler queries with model B,
/// and top_k_similar returns nothing because no rows match.
const DEFAULT_EMBED_MODEL: &str = "nomic-embed-text";

/// Holder-aware spawn — every tick re-reads the embed provider from
/// the live `LlmHolder` AND the embed model from `ai_settings`, so
/// a Settings-→-AI swap (Ollama → OpenAI, or AI → off, or just a
/// model rename) is observed automatically without a restart. When
/// the holder reports `embed = None`, the tick is a cheap no-op.
pub fn spawn_with_holder(db: Arc<Db>, vault: Vault, holder: LlmHolder) -> JoinHandle<()> {
    let batch = std::env::var("POSTERN_EMBED_BATCH")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0 && *n <= 500)
        .unwrap_or(DEFAULT_BATCH);
    info!(
        batch,
        "ai-indexer: spawned (holder-aware), will read provider + model from settings each tick"
    );
    tokio::spawn(async move {
        // Initial small delay so we don't compete with the boot-time
        // backfills (body_text / subject_key / threads).
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            if let Some(provider) = holder.embed().await {
                let model = current_embed_model(&db, &provider);
                tick(&db, &vault, &provider, &model, batch).await;
            }
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        }
    })
}

/// Resolve which embedding model to use for this tick. Order:
///   1. `ai_settings.embed_model` from the DB (the user's explicit
///      Settings → AI choice — this is the authoritative source now
///      that the UI exposes it).
///   2. `POSTERN_EMBED_MODEL` env var (back-compat for installs
///      that pre-date the settings UI).
///   3. Provider-specific recommended default — `text-embedding-
///      3-small` on OpenAI, `nomic-embed-text` everywhere else.
///
/// Lookups are cheap (one indexed row read) so we redo this every
/// tick. The win: changing the model in the UI is observed within
/// 60s and the indexer immediately starts embedding any rows
/// missing against the new model id.
fn current_embed_model(db: &Db, provider: &Arc<dyn LlmProvider>) -> String {
    if let Ok(settings) = db.get_ai_settings() {
        if !settings.embed_model.is_empty() {
            return settings.embed_model;
        }
    }
    if let Ok(env) = std::env::var("POSTERN_EMBED_MODEL") {
        if !env.is_empty() {
            return env;
        }
    }
    // Provider-aware fallback. `LlmProvider::id()` is stable, so
    // matching on it is safe and survives provider renames.
    match provider.id() {
        "openai" => "text-embedding-3-small".to_owned(),
        _ => DEFAULT_EMBED_MODEL.to_owned(),
    }
}

async fn tick(
    db: &Arc<Db>,
    vault: &Vault,
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    batch: usize,
) {
    if vault.require_unlocked().is_err() {
        // Vault sealed — DB key not in scope. Wait for the operator
        // to unlock; our tick will pick up next time around.
        return;
    }

    // Load exclusions on every tick. Lookups are tiny (one row,
    // two TEXT columns) and the cost is negligible compared with
    // the embed call itself. The win: edits in Settings → AI →
    // Exclusions are observed within 60s without a restart.
    let (sender_likes, labels) = match db.get_ai_settings() {
        Ok(s) => {
            let sender_pat = crate::storage::parse_exclusion_list(s.excluded_senders.as_deref());
            let label = crate::storage::parse_exclusion_list(s.excluded_labels.as_deref());
            (
                crate::storage::sender_patterns_to_like(&sender_pat),
                label,
            )
        }
        Err(e) => {
            warn!(error = %e, "ai-indexer: exclusions query failed; defaulting to no exclusions");
            (Vec::new(), Vec::new())
        }
    };

    let pending = match db.messages_missing_embedding(model, batch, &sender_likes, &labels) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "ai-indexer: pending-list query failed");
            return;
        }
    };
    if pending.is_empty() {
        return;
    }

    // Embed one-at-a-time so a single oversize message can't sink
    // the whole batch — Ollama returns 400 on the entire request if
    // any single input exceeds the model's context window. We'd
    // rather lose one row than 50.
    //
    // The provider trait still takes a Vec<String> so we wrap each
    // call in a 1-element batch. Cheap from the trait's perspective;
    // the cost is request overhead × batch_size, which on a local
    // Ollama is single-digit ms per call.
    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut total_elapsed_ms: u64 = 0;
    for row in &pending {
        let input = build_input(
            row.from_addr.as_deref(),
            row.to_addrs.as_deref(),
            row.cc_addrs.as_deref(),
            row.subject.as_deref(),
            row.snippet.as_deref(),
            row.body_text.as_deref(),
        );
        if input.is_empty() {
            // Nothing to embed. Mark as skipped so the candidate
            // pool advances, but don't write a row — empty input
            // would be useless for retrieval anyway.
            skipped += 1;
            continue;
        }
        let req = EmbedRequest {
            model: model.to_owned(),
            inputs: vec![input],
        };
        match provider.embed(req).await {
            Ok(r) => {
                total_elapsed_ms += r.usage.elapsed_ms;
                if let Some(v) = r.vectors.first() {
                    match db.upsert_embedding(row.id, &r.model_used, v) {
                        Ok(()) => indexed += 1,
                        Err(e) => {
                            warn!(message_id = row.id, error = %e, "ai-indexer: upsert failed");
                            skipped += 1;
                        }
                    }
                }
            }
            Err(e) => {
                // Per-message failure (most often "input too long").
                // Log the id so an operator can grep + investigate
                // a specific outlier without losing the rest of
                // the batch.
                warn!(message_id = row.id, error = %e, "ai-indexer: embed failed for one message; continuing");
                skipped += 1;
            }
        }
    }
    if indexed > 0 || skipped > 0 {
        info!(
            indexed,
            skipped,
            elapsed_ms = total_elapsed_ms,
            "ai-indexer: batch finished"
        );
    }
}

fn build_input(
    from_addr: Option<&str>,
    to_addrs: Option<&str>,
    cc_addrs: Option<&str>,
    subject: Option<&str>,
    snippet: Option<&str>,
    body: Option<&str>,
) -> String {
    // 2000 chars is comfortably under any embedding model's context
    // window (nomic-embed-text reports 2048 tokens; 2000 chars ≈
    // 500 tokens of English, leaving generous headroom for non-Latin
    // scripts and base64-leakage). Embeddings lose signal past the
    // first few hundred tokens anyway, so we're not throwing away
    // useful semantic information by capping aggressively.
    const CAP: usize = 2000;
    // Cap the header block so a 50-recipient mailing list doesn't
    // crowd out the body — header text matters for "emails from
    // Joe" / "to the support team" but the body still needs room.
    const HEADER_CAP: usize = 400;

    let mut header = String::new();
    if let Some(f) = from_addr.filter(|s| !s.trim().is_empty()) {
        header.push_str("From: ");
        header.push_str(f.trim());
        header.push('\n');
    }
    if let Some(t) = to_addrs.filter(|s| !s.trim().is_empty()) {
        header.push_str("To: ");
        header.push_str(t.trim());
        header.push('\n');
    }
    if let Some(c) = cc_addrs.filter(|s| !s.trim().is_empty()) {
        header.push_str("Cc: ");
        header.push_str(c.trim());
        header.push('\n');
    }
    if let Some(s) = subject.filter(|s| !s.trim().is_empty()) {
        header.push_str("Subject: ");
        header.push_str(s.trim());
        header.push('\n');
    }
    let header = truncate_at_boundary(&header, HEADER_CAP);

    // Strip quoted reply history from the body before embedding.
    // Mailpile-imported mail (and any heavy reply chain) is mostly
    // `>`-quoted history of prior messages — embedding that drowns
    // the new content in stale context, so retrieval keeps
    // surfacing old emails. We keep only the new reply at the top.
    let body_owned = body.map(strip_quoted_reply);
    let main = body_owned
        .as_deref()
        .filter(|b| !b.trim().is_empty())
        .or(snippet)
        .unwrap_or("");

    let combined = if header.is_empty() {
        main.to_owned()
    } else if main.is_empty() {
        // Header-only is fine — sender + subject still encodes
        // useful semantics for retrieval.
        header.into_owned()
    } else {
        format!("{header}\n{main}")
    };
    truncate_at_boundary(&combined, CAP).into_owned()
}

/// Drop the quoted-reply trail from an email body so we embed the
/// NEW content, not the cumulative thread history. Heuristic — runs
/// over plain-text bodies post HTML-strip:
///
/// 1. Walk lines from the top, keeping each one until we hit:
///    - any `>`-prefixed line (single or nested: `>`, `>>`, `> >`),
///    - an "On <date>, X wrote:" attribution line,
///    - "-----Original Message-----" or "From: ...\nSent: ..." Outlook header,
///    - a `____________________________` Outlook separator.
/// 2. Stop. Everything from that point on is quoted history.
///
/// Top-posting (reply above the trail) is the common case and the
/// only one we serve well. Bottom-posting / interleaved replies lose
/// some signal here, but those are rare on modern mail and the
/// alternative — embedding the whole thread — is strictly worse for
/// retrieval quality. If a body has no quote markers at all, the
/// function returns it untouched.
fn strip_quoted_reply(body: &str) -> String {
    let mut out = String::with_capacity(body.len().min(4096));
    for line in body.lines() {
        let trimmed = line.trim_start();
        // Quoted line — single or nested `>`, possibly preceded by spaces.
        if trimmed.starts_with('>') {
            break;
        }
        // Outlook full-width separator that precedes a quoted block.
        if trimmed.chars().filter(|&c| c == '_').count() >= 20
            && trimmed.chars().all(|c| c == '_' || c.is_whitespace())
        {
            break;
        }
        // "-----Original Message-----" or "----- Forwarded message -----"
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("-----")
            && (lower.contains("original message") || lower.contains("forwarded message"))
        {
            break;
        }
        // "On <date>, X wrote:" attribution.
        if lower.starts_with("on ") && lower.trim_end().ends_with("wrote:") {
            break;
        }
        // Outlook-style "From: ...\nSent: ..." reply header.
        if lower.starts_with("from:") && body[body.find(line).unwrap_or(0)..].contains("\nSent:") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    let trimmed_end = out.trim_end();
    if trimmed_end.is_empty() {
        // Whole body looked like a quote (e.g. forwarded with no
        // intro). Fall back to the original so we still get *some*
        // signal rather than embedding a header-only stub.
        body.to_string()
    } else {
        trimmed_end.to_string()
    }
}

/// Truncate `s` to at most `cap` bytes, walking back to a UTF-8
/// char boundary so we never split a multi-byte codepoint.
fn truncate_at_boundary(s: &str, cap: usize) -> std::borrow::Cow<'_, str> {
    if s.len() <= cap {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(s[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_input_includes_from_subject_and_body() {
        let s = build_input(
            Some("joe@example.com"),
            Some("me@example.com"),
            None,
            Some("Receipt #4"),
            None,
            Some("Thanks for your payment of £12."),
        );
        // Sender goes in as a labelled header so retrieval queries
        // like "emails from Joe" can match on the From line even
        // when the body doesn't echo the sender's name.
        assert!(s.contains("From: joe@example.com"));
        assert!(s.contains("To: me@example.com"));
        assert!(s.contains("Subject: Receipt #4"));
        assert!(s.contains("£12"));
    }

    #[test]
    fn build_input_falls_back_to_snippet_when_body_empty() {
        let s = build_input(None, None, None, Some("Subj"), Some("snip"), None);
        assert!(s.contains("Subject: Subj"));
        assert!(s.contains("snip"));
        let s = build_input(None, None, None, Some("Subj"), Some("snip"), Some(""));
        assert!(s.contains("snip"));
    }

    #[test]
    fn build_input_caps_on_char_boundary() {
        let big = "é".repeat(5000); // 10000 bytes (each 'é' is 2 bytes)
        let s = build_input(None, None, None, None, None, Some(&big));
        assert!(s.len() <= 2000);
        // Must still be valid UTF-8 — implicit because String can't
        // hold invalid UTF-8, but assert no panics on round-trip.
        let _ = s.chars().count();
    }

    #[test]
    fn build_input_handles_all_none() {
        assert_eq!(build_input(None, None, None, None, None, None), "");
    }

    #[test]
    fn build_input_header_only_is_useful() {
        // A message with no body but a clear sender + subject still
        // produces a usable embedding input. Common for promotional
        // mail where strip_html scrubbed everything away.
        let s = build_input(
            Some("noreply@bank.com"),
            None,
            None,
            Some("Statement ready"),
            None,
            None,
        );
        assert!(s.contains("From: noreply@bank.com"));
        assert!(s.contains("Subject: Statement ready"));
    }

    #[test]
    fn build_input_header_truncation_does_not_eat_body() {
        // Pathological case: a long Cc list. The header section
        // gets capped so the body still gets room within the
        // 2000-char total.
        let huge_cc = (0..500)
            .map(|i| format!("user{i}@example.com"))
            .collect::<Vec<_>>()
            .join(", ");
        let s = build_input(
            Some("alice@example.com"),
            Some("bob@example.com"),
            Some(&huge_cc),
            Some("hi"),
            None,
            Some("the body content that should not be lost"),
        );
        assert!(s.contains("From: alice@example.com"));
        assert!(s.contains("the body content"));
    }

    #[test]
    fn strip_quoted_reply_drops_chevron_block() {
        let body = "New reply text here.\n\nOn Mon, Jan 1, 2024 at 10:00 AM, Joe wrote:\n> Old quoted line.\n> Another quoted line.\n>> Even older.\n";
        let stripped = strip_quoted_reply(body);
        assert_eq!(stripped.trim(), "New reply text here.");
    }

    #[test]
    fn strip_quoted_reply_drops_outlook_separator() {
        let body = "Reply at the top.\n\n________________________________\nFrom: someone\nSent: yesterday\n";
        let stripped = strip_quoted_reply(body);
        assert_eq!(stripped.trim(), "Reply at the top.");
    }

    #[test]
    fn strip_quoted_reply_drops_original_message() {
        let body = "Latest content.\n\n-----Original Message-----\nFrom: prior\nSubject: foo\nstuff stuff\n";
        let stripped = strip_quoted_reply(body);
        assert_eq!(stripped.trim(), "Latest content.");
    }

    #[test]
    fn strip_quoted_reply_passes_through_clean_body() {
        let body = "Just a normal email with no replies.\nNothing to strip.";
        let stripped = strip_quoted_reply(body);
        assert_eq!(stripped, body);
    }

    #[test]
    fn strip_quoted_reply_falls_back_when_only_quotes() {
        // Edge case: forward-only message with no intro line. Don't
        // produce an empty embedding input — keep the original.
        let body = "> Original line 1\n> Original line 2\n";
        let stripped = strip_quoted_reply(body);
        assert!(stripped.contains("Original line 1"));
    }

    #[test]
    fn build_input_strips_mailpile_style_thread() {
        // Real-world Mailpile import: tiny new reply on top of a
        // huge thread. The embedding input must reflect the new
        // content, not the buried history.
        let body = "Got it, thanks.\n\nOn Tue, Mar 5, 2024, Alice wrote:\n> The invoice total is $42.\n>> Forwarded earlier...\n";
        let s = build_input(
            Some("me@example.com"),
            Some("alice@example.com"),
            None,
            Some("Re: Invoice"),
            None,
            Some(body),
        );
        assert!(s.contains("Got it, thanks."));
        assert!(!s.contains("$42"));
        assert!(!s.contains("Forwarded earlier"));
    }
}
