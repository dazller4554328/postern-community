//! Anthropic Claude backend.
//!
//! Wire format: `POST /v1/messages` with `x-api-key` and
//! `anthropic-version` headers. Differences from OpenAI worth knowing:
//!   * `system` is a top-level field, not a role inside `messages[]`.
//!   * `max_tokens` is REQUIRED on every request.
//!   * Streaming is SSE with `event: …\ndata: {…}` framing — distinct
//!     event types per chunk (`message_start`, `content_block_delta`,
//!     `message_delta`, `message_stop`).
//!   * No embeddings API. `embed` returns the trait's default error;
//!     callers fall back to a separate embed provider (we keep
//!     embeddings on Ollama by default — see `llm::build_embed_provider`).
//!
//! Privacy posture is fixed `ThirdPartyCloud`. The cloud-consent
//! gate in the storage layer ensures the user has explicitly accepted
//! that mail content is leaving their box before this provider can
//! be selected.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::provider::{
    ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStream, LlmProvider, PrivacyPosture,
    StreamChunk, Usage,
};
use crate::error::{Error, Result};

const BASE_URL: &str = "https://api.anthropic.com/v1";
/// Pinned API version. Anthropic versions their API by date string in
/// the `anthropic-version` header — bumping lets us pick up new
/// features without touching every call site.
const API_VERSION: &str = "2023-06-01";
/// Anthropic requires max_tokens on every request. Picked to match
/// the OpenAI default behaviour (uncapped → caller's max_tokens) and
/// fall back to a sensible ceiling when the caller doesn't set one.
const DEFAULT_MAX_TOKENS: u32 = 1024;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
}

impl AnthropicProvider {
    /// `bind_iface` should be the VPN interface (e.g. `"wg0"`) when
    /// the kill-switch is active — otherwise the chain blocks the
    /// outbound call. Mirrors the IMAP/SMTP binding approach.
    pub fn new(api_key: impl Into<String>, bind_iface: Option<&str>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(Error::BadRequest(
                "Anthropic provider requires an API key".into(),
            ));
        }
        let mut builder = Client::builder().timeout(Duration::from_secs(180));
        if let Some(iface) = bind_iface {
            builder = builder.interface(iface);
        }
        let client = builder
            .build()
            .map_err(|e| Error::Other(anyhow::anyhow!("build anthropic client: {e}")))?;
        Ok(Self { client, api_key })
    }
}

// ---------- wire types ----------------------------------------------

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    /// `system` is a top-level field, not part of `messages[]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<WireMessage<'a>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct WireResponse {
    model: String,
    content: Vec<WireContentBlock>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireContentBlock {
    Text { text: String },
    /// Anthropic sometimes returns tool-use blocks in tool-aware
    /// responses. We don't request tools, so this is defensive — we
    /// simply ignore non-text blocks rather than erroring.
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Default)]
struct WireUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

/// SSE event payload — Anthropic's stream is line-framed
/// `event: <name>\ndata: <json>\n\n`. We dispatch on event name.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    MessageStart { message: StreamStartMessage },
    ContentBlockDelta { delta: StreamDelta },
    MessageDelta { usage: WireUsage },
    /// Other events (`ping`, `content_block_start`,
    /// `content_block_stop`, `message_stop`) carry no token payload —
    /// we ignore them.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct StreamStartMessage {
    model: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamDelta {
    TextDelta { text: String },
    /// Other delta types (input_json_delta etc) — ignored, we don't
    /// use tool-use streaming.
    #[serde(other)]
    Other,
}

fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<WireMessage<'_>>) {
    // Anthropic puts the system prompt at the top of the request, not
    // in the messages array. We concatenate every System role we get
    // (typically just one) and pass user/assistant turns through as
    // the message list. Empty system → don't send the field at all.
    let mut system_parts: Vec<&str> = Vec::new();
    let mut wire: Vec<WireMessage<'_>> = Vec::with_capacity(messages.len());
    for m in messages {
        match m.role {
            ChatRole::System => system_parts.push(m.content.as_str()),
            ChatRole::User => wire.push(WireMessage {
                role: "user",
                content: m.content.as_str(),
            }),
            ChatRole::Assistant => wire.push(WireMessage {
                role: "assistant",
                content: m.content.as_str(),
            }),
        }
    }
    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };
    (system, wire)
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn privacy_posture(&self) -> PrivacyPosture {
        PrivacyPosture::ThirdPartyCloud
    }

    async fn health(&self) -> Result<()> {
        // Anthropic has no cheap healthcheck endpoint that returns
        // 200 without consuming credits. The accepted pattern is to
        // POST a tiny /messages call (max_tokens=1, single "hi" user
        // message) — burns < 5 input tokens + 1 output token. Costs
        // a fraction of a cent and confirms key + reachability.
        let body = WireRequest {
            model: "claude-haiku-4-5-20251001",
            system: None,
            messages: vec![WireMessage {
                role: "user",
                content: "hi",
            }],
            max_tokens: 1,
            temperature: Some(0.0),
            stop_sequences: vec![],
            stream: None,
        };
        let resp = self
            .client
            .post(format!("{BASE_URL}/messages"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic health: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "anthropic health {}: {}",
                status,
                body
            )));
        }
        Ok(())
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let started = Instant::now();
        let (system, messages) = split_system(&req.messages);
        let body = WireRequest {
            model: &req.model,
            system,
            messages,
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            temperature: req.temperature,
            stop_sequences: req.stop,
            stream: None,
        };
        let resp = self
            .client
            .post(format!("{BASE_URL}/messages"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic chat: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "anthropic chat {}: {}",
                status,
                body_text
            )));
        }
        let parsed: WireResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic parse: {e}")))?;
        let mut content = String::new();
        for block in parsed.content {
            if let WireContentBlock::Text { text } = block {
                content.push_str(&text);
            }
        }
        let usage = parsed.usage.unwrap_or_default();
        Ok(ChatResponse {
            content,
            model_used: parsed.model,
            usage: Usage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let started = Instant::now();
        let (system, messages) = split_system(&req.messages);
        let body = WireRequest {
            model: &req.model,
            system,
            messages,
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            temperature: req.temperature,
            stop_sequences: req.stop,
            stream: Some(true),
        };
        let resp = self
            .client
            .post(format!("{BASE_URL}/messages"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic chat_stream: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "anthropic chat_stream {}: {}",
                status,
                body_text
            )));
        }

        let mut byte_stream = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            let mut model_used: String = String::new();
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;
            while let Some(item) = byte_stream.next().await {
                let bytes = match item {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(Error::Other(anyhow::anyhow!("stream read: {e}")));
                        return;
                    }
                };
                let chunk_str = match std::str::from_utf8(&bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        yield Err(Error::Other(anyhow::anyhow!(
                            "non-utf8 chunk from anthropic"
                        )));
                        return;
                    }
                };
                buf.push_str(chunk_str);
                while let Some(end) = buf.find("\n\n") {
                    let event = buf[..end].to_owned();
                    buf.drain(..=end + 1);
                    // Only the `data:` line carries JSON; the
                    // `event:` line names the type but the payload
                    // re-states it via `"type"`. We just parse data.
                    let mut payload: Option<&str> = None;
                    for line in event.lines() {
                        if let Some(rest) = line.trim().strip_prefix("data:") {
                            payload = Some(rest.trim());
                            break;
                        }
                    }
                    let Some(payload) = payload else { continue };
                    if payload.is_empty() {
                        continue;
                    }
                    let parsed: StreamEvent = match serde_json::from_str(payload) {
                        Ok(p) => p,
                        Err(e) => {
                            yield Err(Error::Other(anyhow::anyhow!(
                                "parse anthropic sse: {e}"
                            )));
                            return;
                        }
                    };
                    match parsed {
                        StreamEvent::MessageStart { message } => {
                            model_used = message.model;
                        }
                        StreamEvent::ContentBlockDelta { delta } => {
                            if let StreamDelta::TextDelta { text } = delta {
                                if !text.is_empty() {
                                    yield Ok(StreamChunk::Token(text));
                                }
                            }
                        }
                        StreamEvent::MessageDelta { usage } => {
                            input_tokens = usage.input_tokens;
                            output_tokens = usage.output_tokens;
                        }
                        StreamEvent::Other => {}
                    }
                }
                // Anthropic's stream terminates when the connection
                // closes (no [DONE] sentinel). Loop back to the
                // byte-stream poll, which returns None when done.
            }
            yield Ok(StreamChunk::Done {
                model_used,
                usage: Usage {
                    prompt_tokens: input_tokens,
                    completion_tokens: output_tokens,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            });
        };
        Ok(Box::pin(stream))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        // Anthropic exposes /v1/models with the same auth headers as
        // /v1/messages. Returns the catalogue visible to this key.
        #[derive(serde::Deserialize)]
        struct ModelEntry {
            id: String,
        }
        #[derive(serde::Deserialize)]
        struct ModelsResponse {
            #[serde(default)]
            data: Vec<ModelEntry>,
        }
        let resp = self
            .client
            .get(format!("{BASE_URL}/models"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic list_models: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "anthropic list_models {}: {}",
                status,
                body
            )));
        }
        let parsed: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("anthropic list_models parse: {e}")))?;
        let mut names: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        names.sort();
        Ok(names)
    }

    // No `embed` override — the trait default returns BadRequest, and
    // callers know to route embeddings to a different provider when
    // chat is on Anthropic.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_posture_are_stable() {
        let p = AnthropicProvider::new("sk-ant-test", None).unwrap();
        assert_eq!(p.id(), "anthropic");
        assert_eq!(p.privacy_posture(), PrivacyPosture::ThirdPartyCloud);
    }

    #[test]
    fn rejects_empty_api_key() {
        assert!(AnthropicProvider::new("", None).is_err());
        assert!(AnthropicProvider::new("   ", None).is_err());
    }

    #[test]
    fn split_system_separates_top_level_system() {
        let msgs = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "be concise".into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "hi".into(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "hello".into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "again".into(),
            },
        ];
        let (system, wire) = split_system(&msgs);
        assert_eq!(system.as_deref(), Some("be concise"));
        assert_eq!(wire.len(), 3);
        assert_eq!(wire[0].role, "user");
        assert_eq!(wire[1].role, "assistant");
        assert_eq!(wire[2].role, "user");
    }

    #[test]
    fn split_system_concatenates_multiple_system_messages() {
        let msgs = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "rule one".into(),
            },
            ChatMessage {
                role: ChatRole::System,
                content: "rule two".into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "go".into(),
            },
        ];
        let (system, _) = split_system(&msgs);
        assert_eq!(system.as_deref(), Some("rule one\n\nrule two"));
    }

    #[test]
    fn chat_response_extracts_text_blocks() {
        let canonical = serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-6",
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "text", "text": " world" }
            ],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        });
        let parsed: WireResponse = serde_json::from_value(canonical).unwrap();
        let mut joined = String::new();
        for b in parsed.content {
            if let WireContentBlock::Text { text } = b {
                joined.push_str(&text);
            }
        }
        assert_eq!(joined, "hello world");
        let u = parsed.usage.unwrap();
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 2);
    }
}
