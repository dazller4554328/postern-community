//! Abstract LLM-provider interface.
//!
//! The trait is deliberately minimal: chat completion, optional
//! transcription, and a reachability probe. The audit and
//! vault-locked gating live in decorator wrappers above this layer —
//! keeping the provider trait pure means tests for individual
//! backends don't need to fake out the whole world.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Where this provider sends data. Drives the UI's privacy-budget
/// meter and gates which features can use which provider — e.g. a
/// future "compose RAG" feature that pulls in your prior emails as
/// context refuses to run against `ThirdPartyCloud` unless the user
/// has explicitly opted that feature in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPosture {
    /// Inference runs on this machine (or a host the user controls
    /// directly via Tailscale / LAN). Data never leaves user-owned
    /// infrastructure.
    LocalOnly,
    /// User-controlled remote endpoint — typically a self-hosted
    /// vLLM or llama.cpp-server on the user's own VPS. Leaves the
    /// box but stays in the user's trust domain.
    UserControlledRemote,
    /// Third-party cloud (Anthropic, `OpenAI`, etc.). User has
    /// explicitly opted in per-feature.
    ThirdPartyCloud,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    /// Backend-specific model identifier
    /// (e.g. `"llama3.1:8b-instruct-q4_K_M"` for Ollama,
    /// `"claude-sonnet-4-6"` for Anthropic).
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Stop sequences. Useful for structured outputs (e.g. forcing
    /// a JSON-only summary by setting stop=["}"]).
    pub stop: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatResponse {
    pub content: String,
    /// Exact model identifier the backend reports it served the
    /// request with. May differ from request.model when the backend
    /// aliases (e.g. Anthropic's auto-selected snapshot). Stored
    /// verbatim in the audit log so a future operator can replay.
    pub model_used: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Wall-clock time for the call, including queue + network +
    /// generation. Lets the UI surface "this took 14s" so users
    /// understand what they're paying for in latency.
    pub elapsed_ms: u64,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable identifier used in the audit log
    /// (`"ollama"`, `"openai_compat"`, `"anthropic"`). Renaming an
    /// id orphans existing audit history, so treat these as a
    /// public contract.
    fn id(&self) -> &'static str;

    /// Where data sent to this provider goes. The UI reads this to
    /// render the per-feature privacy-budget badge.
    fn privacy_posture(&self) -> PrivacyPosture;

    /// Cheap reachability + auth probe. Hit once at boot, then
    /// occasionally to update the "AI available" indicator. Should
    /// not call the model — just talk to the provider's healthcheck
    /// or list-models endpoint.
    async fn health(&self) -> Result<()>;

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// List the chat-capable models this provider currently has
    /// available — Ollama returns its installed pull list, OpenAI /
    /// Anthropic return their account-visible model catalogue.
    /// Default impl returns an empty list so the Settings UI can
    /// degrade gracefully (free-text fallback) rather than failing
    /// for providers that don't expose discovery.
    async fn list_models(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Transcribe a chunk of audio to text. Currently only OpenAI
    /// (Whisper) implements this; others return BadRequest so
    /// callers can degrade gracefully when the configured chat
    /// provider doesn't support voice. `audio_bytes` is the raw
    /// container bytes (e.g. webm/opus from MediaRecorder), and
    /// `mime_type` is the MIME from the upload (`audio/webm`,
    /// `audio/m4a`, etc.) — used to set the multipart filename
    /// extension OpenAI's Whisper endpoint sniffs for.
    async fn transcribe(&self, _audio_bytes: Vec<u8>, _mime_type: &str) -> Result<String> {
        Err(crate::error::Error::BadRequest(
            "audio transcription not supported by this provider".into(),
        ))
    }
}
