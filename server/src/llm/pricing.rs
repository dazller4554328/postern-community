//! Static cost table for AI providers.
//!
//! Hardcoded so it ships with the binary — no external API call to
//! a price oracle, no Postern-hosted price feed (which would create
//! a phone-home Postern doesn't otherwise need). When OpenAI /
//! Anthropic / xAI change prices we bump these constants in a new
//! release; the user always sees what their build was compiled with.
//!
//! Returned as USD per 1,000,000 tokens because that's the unit the
//! vendors quote in. The frontend multiplies actual token counts to
//! get a real-currency estimate.
//!
//! Unknown models return a `(None, None)` rate pair — the UI then
//! shows "—" for cost rather than lying with a $0 value.

use serde::Serialize;

/// Per-million-token rates in USD. Both fields are optional so a
/// provider that doesn't bill per-token (e.g. local Ollama) can
/// declare itself with `(Some(0.0), Some(0.0))` and chat-only
/// providers can leave the embed side `None`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelRate {
    pub prompt_per_1m_usd: Option<f64>,
    pub completion_per_1m_usd: Option<f64>,
}

/// Look up the rate for a (provider_id, model_id) pair. Matching
/// is exact + case-sensitive on `model` to avoid rate confusion
/// between e.g. `gpt-4o` and `gpt-4o-mini`. Provider ids match
/// `LlmProvider::id()` strings.
///
/// Models not in the table return `None` so the UI knows to render
/// "—" instead of $0.00.
pub fn rate_for(provider: &str, model: &str) -> Option<ModelRate> {
    // Strip provider-side date / version suffixes that would
    // otherwise miss the lookup. OpenAI's snapshot ids look like
    // `gpt-4o-mini-2024-07-18`; the user picked `gpt-4o-mini`,
    // and that's what should drive the cost row. Same approach
    // for Ollama's `:latest` tag and Anthropic's snapshot dates.
    let normalised = normalise_model_id(model);
    let key = (provider, normalised.as_str());
    let mapped = match key {
        // ── Ollama (local; everything is "free") ──────────────
        ("ollama", _) => Some(ModelRate {
            prompt_per_1m_usd: Some(0.0),
            completion_per_1m_usd: Some(0.0),
        }),

        // ── OpenAI ──────────────────────────────────────────
        // Chat (Sept 2026 prices; bump these on rate changes).
        ("openai", "gpt-4o") => Some(ModelRate {
            prompt_per_1m_usd: Some(2.50),
            completion_per_1m_usd: Some(10.00),
        }),
        ("openai", "gpt-4o-mini") => Some(ModelRate {
            prompt_per_1m_usd: Some(0.15),
            completion_per_1m_usd: Some(0.60),
        }),
        ("openai", "gpt-4-turbo") => Some(ModelRate {
            prompt_per_1m_usd: Some(10.00),
            completion_per_1m_usd: Some(30.00),
        }),
        ("openai", "o1") => Some(ModelRate {
            prompt_per_1m_usd: Some(15.00),
            completion_per_1m_usd: Some(60.00),
        }),
        ("openai", "o1-mini") => Some(ModelRate {
            prompt_per_1m_usd: Some(3.00),
            completion_per_1m_usd: Some(12.00),
        }),
        // Embeddings.
        ("openai", "text-embedding-3-small") => Some(ModelRate {
            prompt_per_1m_usd: Some(0.020),
            completion_per_1m_usd: Some(0.0),
        }),
        ("openai", "text-embedding-3-large") => Some(ModelRate {
            prompt_per_1m_usd: Some(0.130),
            completion_per_1m_usd: Some(0.0),
        }),
        ("openai", "text-embedding-ada-002") => Some(ModelRate {
            prompt_per_1m_usd: Some(0.100),
            completion_per_1m_usd: Some(0.0),
        }),

        // ── Anthropic ───────────────────────────────────────
        ("anthropic", "claude-opus-4-7") => Some(ModelRate {
            prompt_per_1m_usd: Some(15.00),
            completion_per_1m_usd: Some(75.00),
        }),
        ("anthropic", "claude-sonnet-4-6") => Some(ModelRate {
            prompt_per_1m_usd: Some(3.00),
            completion_per_1m_usd: Some(15.00),
        }),
        ("anthropic", "claude-haiku-4-5") => Some(ModelRate {
            prompt_per_1m_usd: Some(1.00),
            completion_per_1m_usd: Some(5.00),
        }),

        // ── xAI / Grok (via openai_compat) ──────────────────
        ("openai_compat", "grok-beta") => Some(ModelRate {
            prompt_per_1m_usd: Some(5.00),
            completion_per_1m_usd: Some(15.00),
        }),
        ("openai_compat", "grok-2") => Some(ModelRate {
            prompt_per_1m_usd: Some(2.00),
            completion_per_1m_usd: Some(10.00),
        }),
        // Self-hosted vLLM on user infra is "free" in marginal
        // terms — the user is paying for the box. Match openai_
        // compat models the user might point at and don't have
        // explicit rates for via the empty-string default below.

        _ => None,
    };
    // Last-resort default for openai_compat: zero (self-hosted).
    // Tells the UI "no per-token cost" rather than "unknown".
    if mapped.is_none() && provider == "openai_compat" {
        // Heuristic: a self-hosted compat target. The base URL
        // analysis for hosted-vendor classification lives in
        // `llm::mod`; here we just trust that unknown openai_
        // compat = local-ish.
        return Some(ModelRate {
            prompt_per_1m_usd: Some(0.0),
            completion_per_1m_usd: Some(0.0),
        });
    }
    mapped
}

/// Strip date/version suffixes the vendor adds in API responses
/// so a `gpt-4o-mini-2024-07-18` (Open AI snapshot) maps to
/// `gpt-4o-mini`. Conservative: only strips when the suffix matches
/// a known shape, never breaks an unfamiliar model id.
fn normalise_model_id(model: &str) -> String {
    let m = model;
    // Ollama tag: drop everything after the first ':'. So
    // `nomic-embed-text:latest` → `nomic-embed-text`.
    if let Some((before, _)) = m.split_once(':') {
        return before.to_owned();
    }
    // OpenAI snapshot: strip a trailing -YYYY-MM-DD if present.
    let bytes = m.as_bytes();
    if bytes.len() > 11
        && bytes[bytes.len() - 11] == b'-'
        && bytes[bytes.len() - 10..bytes.len() - 6]
            .iter()
            .all(u8::is_ascii_digit)
        && bytes[bytes.len() - 6] == b'-'
        && bytes[bytes.len() - 5..bytes.len() - 3]
            .iter()
            .all(u8::is_ascii_digit)
        && bytes[bytes.len() - 3] == b'-'
        && bytes[bytes.len() - 2..]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return m[..m.len() - 11].to_owned();
    }
    m.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_canonical_openai_models() {
        let r = rate_for("openai", "gpt-4o-mini").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(0.15));
        assert_eq!(r.completion_per_1m_usd, Some(0.60));
    }

    #[test]
    fn strips_openai_snapshot_dates() {
        let r = rate_for("openai", "gpt-4o-mini-2024-07-18").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(0.15));
    }

    #[test]
    fn strips_ollama_tag() {
        let r = rate_for("ollama", "nomic-embed-text:latest").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(0.0));
    }

    #[test]
    fn ollama_is_always_free() {
        let r = rate_for("ollama", "some-future-model:7b").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(0.0));
        assert_eq!(r.completion_per_1m_usd, Some(0.0));
    }

    #[test]
    fn unknown_openai_returns_none() {
        // Don't lie about prices for a model we don't recognise.
        assert!(rate_for("openai", "gpt-9000").is_none());
    }

    #[test]
    fn unknown_compat_assumed_self_hosted_free() {
        // Self-hosted vLLM users shouldn't see a $-cost when the
        // box is theirs.
        let r = rate_for("openai_compat", "my-private-model").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(0.0));
    }

    #[test]
    fn known_compat_grok_priced() {
        let r = rate_for("openai_compat", "grok-beta").unwrap();
        assert_eq!(r.prompt_per_1m_usd, Some(5.00));
    }
}
