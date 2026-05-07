//! Abstract LLM-provider interface.
//!
//! The trait is deliberately minimal: just chat completion,
//! embeddings, and a reachability probe. The audit and vault-locked
//! gating live in decorator wrappers above this layer — keeping the
//! provider trait pure means tests for individual backends don't
//! need to fake out the whole world.

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// One token-or-final-event from a streaming chat call.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Incremental output from the model. Append to the running
    /// answer and surface to the UI as it arrives.
    Token(String),
    /// Generation finished. Carries the final token-count totals
    /// and the canonical model identifier the backend served the
    /// request with — both are persisted to ai_chat_log.
    Done {
        model_used: String,
        usage: Usage,
    },
}

/// Boxed-stream alias keeps the trait bounds short at call sites.
/// `Send` is mandatory: streams cross await points spanning HTTP
/// handlers + audit decorators + the runtime task boundary.
pub type ChatStream =
    Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

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
    /// Third-party cloud (Anthropic, OpenAI, etc.). User has
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

#[derive(Debug, Clone, Serialize)]
pub struct EmbedRequest {
    pub model: String,
    /// Batched — all backends accept multi-input requests, and
    /// batching matters for the semantic-search initial-index pass
    /// (10k messages = many batches not many requests).
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EmbedResponse {
    pub model_used: String,
    pub vectors: Vec<Vec<f32>>,
    pub usage: Usage,
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

    /// Streaming chat — returns a stream of [`StreamChunk`] values.
    /// Default implementation errors so backends that genuinely
    /// can't stream (a hypothetical synchronous-only provider) can
    /// opt out. Callers fall back to the buffered `chat()` when
    /// streaming is unavailable.
    ///
    /// Why this matters in practice: Cloudflare's free-plan origin
    /// timeout is 100 s. A cold-load CPU answer can take 75–115 s.
    /// Streaming keeps the connection alive as long as bytes are
    /// flowing — Cloudflare doesn't abort, the user sees tokens
    /// appear in real time.
    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream> {
        Err(crate::error::Error::BadRequest(
            "streaming not supported by this provider".into(),
        ))
    }

    /// Embeddings. Default implementation errors so callers can
    /// fall back to a different provider when the active one
    /// doesn't speak embeddings (e.g. Anthropic chat-only). Backends
    /// that do support embeddings override this.
    async fn embed(&self, _req: EmbedRequest) -> Result<EmbedResponse> {
        Err(crate::error::Error::BadRequest(
            "embeddings not supported by this provider".into(),
        ))
    }

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
    async fn transcribe(
        &self,
        _audio_bytes: Vec<u8>,
        _mime_type: &str,
    ) -> Result<String> {
        Err(crate::error::Error::BadRequest(
            "audio transcription not supported by this provider".into(),
        ))
    }
}
