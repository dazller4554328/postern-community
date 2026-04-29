//! AI/LLM integration for Postern.
//!
//! Two layers:
//!
//!   - [`provider::LlmProvider`] — the abstract interface every
//!     backend must satisfy. Chat completion, embeddings, health
//!     probe, plus a [`provider::PrivacyPosture`] declaration so the
//!     UI can render an honest "data exposure" indicator before the
//!     user invokes a feature.
//!
//!   - Backend implementations: [`ollama::OllamaProvider`] today,
//!     `OpenAICompatible` and `AnthropicProvider` to follow.
//!
//! Wired into [`crate::http::AppState`] later as
//! `Option<Arc<dyn LlmProvider>>` — `None` when no backend is
//! reachable, in which case AI surfaces grey out in the UI rather
//! than 500 on every click. Probe + install happens once at boot.
//!
//! Audit: every chat/embed call goes through an `AuditedProvider`
//! decorator that writes a row to the existing `audit_log` table
//! before returning, so users can grep their own history of "what
//! did the AI see, when, and what did it say."

pub mod activity;
pub mod anthropic;
pub mod holder;
pub mod indexer;
pub mod ollama;
pub mod openai;
pub mod pricing;
pub mod provider;

pub use activity::ActivityLoggedProvider;
pub use anthropic::AnthropicProvider;
pub use holder::LlmHolder;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use pricing::{rate_for, ModelRate};
pub use provider::{
    ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStream, EmbedRequest, EmbedResponse,
    LlmProvider, PrivacyPosture, StreamChunk, Usage,
};

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::storage::AiSettings;

/// Build the chat provider for the given settings. Returns `Ok(None)`
/// when AI is disabled — callers branch on `None` to grey out AI
/// surfaces. Errors when the configured provider can't be constructed
/// (missing key, malformed base URL).
///
/// `bind_iface` should be `vpn.bind_iface()` from the live VpnManager.
/// Cloud providers (Anthropic, OpenAI, hosted compat) get bound to
/// the VPN interface so the kill-switch chain doesn't REJECT their
/// outbound HTTPS — same approach as IMAP/SMTP. Ollama is left
/// unbound because the canonical install is localhost (binding to
/// wg0 there would break it).
pub fn build_chat_provider(
    settings: &AiSettings,
    api_key: Option<&str>,
    bind_iface: Option<&str>,
) -> Result<Option<Arc<dyn LlmProvider>>> {
    if !settings.enabled {
        return Ok(None);
    }
    match settings.provider_kind.as_str() {
        "ollama" => {
            let host = settings
                .base_url
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ollama::DEFAULT_BASE_URL.to_owned());
            let p = OllamaProvider::new(host)?;
            Ok(Some(Arc::new(p)))
        }
        "anthropic" => {
            let key = api_key.ok_or_else(|| {
                Error::BadRequest("Anthropic provider requires an API key".into())
            })?;
            let p = AnthropicProvider::new(key, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        "openai" => {
            let key = api_key.ok_or_else(|| {
                Error::BadRequest("OpenAI provider requires an API key".into())
            })?;
            let p = OpenAiProvider::new_openai(key, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        "openai_compat" => {
            let key = api_key.ok_or_else(|| {
                Error::BadRequest("OpenAI-compatible provider requires an API key".into())
            })?;
            let base = settings
                .base_url
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    Error::BadRequest(
                        "OpenAI-compatible provider requires a base URL (e.g. https://api.x.ai/v1)"
                            .into(),
                    )
                })?;
            // Heuristic posture: a public hosted vendor (anything
            // matching api.x.ai / api.groq.com / similar) is
            // ThirdPartyCloud; a private/LAN URL is UserControlled
            // Remote. Users on a self-hosted vLLM still get the
            // honest "your remote box" badge.
            let posture = classify_compat_posture(base);
            let p = OpenAiProvider::new_compat(base, key, posture, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        other => Err(Error::BadRequest(format!(
            "unknown AI provider kind: {other}"
        ))),
    }
}

/// Build the embed provider from the explicit `embed_provider_kind`
/// setting — independent of the chat provider. Common pairings:
///   * chat=openai + embed=ollama (recommended) — best chat quality
///     while keeping every email body off the cloud during indexing.
///   * chat=ollama + embed=ollama — fully local, free, slower.
///   * chat=anthropic + embed=ollama — same privacy win as the
///     OpenAI case (Anthropic has no embeddings API anyway).
///   * chat=openai + embed=openai — accepts the cost + privacy
///     tradeoff for marginal retrieval quality.
///
/// `api_key` is the chat key (reused when embed_provider == chat
/// provider); `embed_api_key` is the optional embed-specific key
/// for the cross-provider corner case (chat=Anthropic +
/// embed=OpenAI). Anthropic is rejected as embed because it has no
/// embeddings API.
pub fn build_embed_provider(
    settings: &AiSettings,
    api_key: Option<&str>,
    embed_api_key: Option<&str>,
    bind_iface: Option<&str>,
) -> Result<Option<Arc<dyn LlmProvider>>> {
    if !settings.enabled {
        return Ok(None);
    }
    let kind = settings.embed_provider_kind.as_str();
    // Same-provider pairings reuse the chat key. Different-provider
    // pairings need the embed-specific key (or fall back to the
    // chat key, which probably won't authorize but at least lets
    // the build succeed so the user gets a clear test failure).
    let key = if kind == settings.provider_kind {
        api_key
    } else {
        embed_api_key.or(api_key)
    };

    match kind {
        "ollama" => {
            let host = settings
                .embed_base_url
                .clone()
                .filter(|s| !s.is_empty())
                // If chat is also Ollama and embed_base_url isn't
                // set, honour the chat base_url so a single Ollama
                // override config covers both.
                .or_else(|| {
                    if settings.provider_kind == "ollama" {
                        settings.base_url.clone().filter(|s| !s.is_empty())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| ollama::DEFAULT_BASE_URL.to_owned());
            let p = OllamaProvider::new(host)?;
            Ok(Some(Arc::new(p)))
        }
        "openai" => {
            let key = key.ok_or_else(|| {
                Error::BadRequest("OpenAI embed provider requires an API key".into())
            })?;
            let p = OpenAiProvider::new_openai(key, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        "openai_compat" => {
            let key = key.ok_or_else(|| {
                Error::BadRequest("OpenAI-compatible embed provider requires an API key".into())
            })?;
            let base = settings
                .embed_base_url
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    Error::BadRequest(
                        "OpenAI-compatible embed provider requires a base URL".into(),
                    )
                })?;
            let posture = classify_compat_posture(base);
            let p = OpenAiProvider::new_compat(base, key, posture, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        "anthropic" => Err(Error::BadRequest(
            "Anthropic does not offer an embeddings API — pick Ollama (free, local) or OpenAI for embeddings.".into(),
        )),
        other => Err(Error::BadRequest(format!(
            "unknown AI embed provider kind: {other}"
        ))),
    }
}

fn classify_compat_posture(base_url: &str) -> PrivacyPosture {
    let lower = base_url.to_ascii_lowercase();
    let hosted_markers = [
        "api.x.ai",
        "api.groq.com",
        "api.together.xyz",
        "api.perplexity.ai",
        "api.deepseek.com",
        "api.mistral.ai",
    ];
    if hosted_markers.iter().any(|m| lower.contains(m)) {
        PrivacyPosture::ThirdPartyCloud
    } else {
        // Private / LAN URLs default to user-controlled-remote — the
        // user pointed Postern at their own box.
        PrivacyPosture::UserControlledRemote
    }
}
