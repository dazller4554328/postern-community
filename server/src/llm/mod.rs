//! AI/LLM integration for Postern.
//!
//! Two layers:
//!
//!   - [`provider::LlmProvider`] — the abstract interface every
//!     backend must satisfy. Chat completion + health probe + a
//!     [`provider::PrivacyPosture`] declaration so the UI can render
//!     an honest "data exposure" indicator before the user invokes a
//!     feature.
//!
//!   - Backend implementations: Ollama, `OpenAI`, Anthropic, plus an
//!     OpenAI-compatible adapter for hosted vendors (xAI, Groq,
//!     Together, vLLM).
//!
//! Wired into [`crate::http::AppState`] as `LlmHolder`. Used by the
//! compose-pane Polish + Dictate features only — the Datas RAG path
//! that previously dominated this module was removed 2026-05-08.

pub mod anthropic;
pub mod holder;
pub mod ollama;
pub mod openai;
pub mod provider;

pub use anthropic::AnthropicProvider;
pub use holder::LlmHolder;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use provider::{ChatMessage, ChatRequest, ChatRole, LlmProvider, PrivacyPosture};

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::storage::AiSettings;

/// Build the chat provider for the given settings. Returns `Ok(None)`
/// when AI is disabled — callers branch on `None` to grey out AI
/// surfaces. Errors when the configured provider can't be constructed
/// (missing key, malformed base URL).
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
            let key = api_key
                .ok_or_else(|| Error::BadRequest("OpenAI provider requires an API key".into()))?;
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
            let posture = classify_compat_posture(base);
            let p = OpenAiProvider::new_compat(base, key, posture, bind_iface)?;
            Ok(Some(Arc::new(p)))
        }
        other => Err(Error::BadRequest(format!(
            "unknown AI provider kind: {other}"
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
        PrivacyPosture::UserControlledRemote
    }
}
