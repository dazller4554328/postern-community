//! Ollama backend — the canonical local-only provider.
//!
//! Talks to `http://localhost:11434` (or wherever `OLLAMA_HOST`
//! points) over the Ollama HTTP API. Privacy posture is
//! `LocalOnly` — operator-relocated installs (e.g. an Ollama
//! daemon on a different LAN host) should be configured as a
//! separate provider entry to surface that nuance in the UI.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::provider::{
    ChatRequest, ChatResponse, ChatRole, ChatStream, EmbedRequest, EmbedResponse, LlmProvider,
    PrivacyPosture, StreamChunk, Usage,
};
use crate::error::{Error, Result};

/// Default base URL when `OLLAMA_HOST` isn't set. Matches Ollama's
/// own default, so a fresh `ollama serve` on the same machine just
/// works without configuration.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    /// New provider against `base_url`. The 15-minute timeout is
    /// generous because real-world RAG on a 6-vCPU box can spend
    /// 5–8 minutes on prompt processing alone before the first
    /// token (4 retrieved emails ≈ 3 k input tokens at ~10 tok/s
    /// prompt-eval). Beyond 15 minutes the model is genuinely
    /// hung; aborting is kinder than holding the socket forever.
    /// Operators on GPU never come close — the timeout is there
    /// for the CPU edge case.
    /// Ollama is intentionally unbound: the canonical install runs on
    /// `localhost:11434` and `SO_BINDTODEVICE(wg0)` would break that
    /// because the kernel would try to route 127.0.0.1 over wg0
    /// (no such route → EHOSTUNREACH). Operators with a remote
    /// Ollama on a non-localhost host get default-route egress —
    /// fine when the kill-switch isn't on, fine when there's no
    /// VPN. The remote-Ollama-with-kill-switch corner case is
    /// rare; if it bites, we can add per-target detection later.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|e| Error::Other(anyhow::anyhow!("build ollama client: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.into(),
        })
    }
}

// ---------- wire types ----------------------------------------------
//
// Kept as private, file-local types rather than re-using the public
// `provider::*` shapes so an Ollama API tweak (rename a field,
// promote an option) only touches this file.

#[derive(Serialize)]
struct WireChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    /// Always false here — streaming is handled in a separate
    /// `chat_stream` method (not on the base trait yet) so the
    /// non-streaming path stays simple.
    stream: bool,
    options: WireOptions,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize, Default)]
struct WireOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

#[derive(Deserialize)]
struct WireChatResponse {
    model: String,
    message: WireResponseMessage,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct WireResponseMessage {
    content: String,
}

/// Per-chunk shape from Ollama's streaming /api/chat. Each line of
/// the response body is one of these. The final line has `done=true`
/// and the cumulative token counts.
#[derive(Deserialize)]
struct WireStreamChunk {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    message: Option<WireResponseMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Serialize)]
struct WireEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct WireEmbedResponse {
    model: String,
    embeddings: Vec<Vec<f32>>,
}

fn role_str(r: ChatRole) -> &'static str {
    match r {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn privacy_posture(&self) -> PrivacyPosture {
        // The trait can't tell from base_url alone whether the
        // operator pointed Ollama at localhost or at a remote
        // machine. Default to LocalOnly because the canonical
        // config is localhost:11434; operators with a remote Ollama
        // should configure it through the future `OpenAICompatible`
        // provider with an explicit `UserControlledRemote` posture.
        PrivacyPosture::LocalOnly
    }

    async fn health(&self) -> Result<()> {
        // /api/tags is Ollama's list-installed-models endpoint —
        // returns 200 cheaply and confirms both reachability and
        // that the daemon is functional. Cheaper than /api/version
        // for our needs because list-models is the data the UI also
        // consumes.
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama health: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::Other(anyhow::anyhow!(
                "ollama returned {} on /api/tags",
                resp.status()
            )));
        }
        Ok(())
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let started = Instant::now();
        let messages: Vec<WireMessage<'_>> = req
            .messages
            .iter()
            .map(|m| WireMessage {
                role: role_str(m.role),
                content: &m.content,
            })
            .collect();
        let body = WireChatRequest {
            model: &req.model,
            messages,
            stream: false,
            options: WireOptions {
                temperature: req.temperature,
                num_predict: req.max_tokens,
                stop: req.stop,
            },
        };
        let url = format!("{}/api/chat", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama chat: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "ollama chat {}: {}",
                status,
                body_text
            )));
        }
        let parsed: WireChatResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama parse: {e}")))?;
        Ok(ChatResponse {
            content: parsed.message.content,
            model_used: parsed.model,
            usage: Usage {
                prompt_tokens: parsed.prompt_eval_count.unwrap_or(0),
                completion_tokens: parsed.eval_count.unwrap_or(0),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let started = Instant::now();
        let messages: Vec<WireMessage<'_>> = req
            .messages
            .iter()
            .map(|m| WireMessage {
                role: role_str(m.role),
                content: &m.content,
            })
            .collect();
        let body = WireChatRequest {
            model: &req.model,
            messages,
            stream: true,
            options: WireOptions {
                temperature: req.temperature,
                num_predict: req.max_tokens,
                stop: req.stop,
            },
        };
        let url = format!("{}/api/chat", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama chat_stream: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "ollama chat_stream {}: {}",
                status,
                body_text
            )));
        }

        // Ollama streams newline-delimited JSON objects. Buffer
        // partial chunks across reqwest::Bytes boundaries — a single
        // JSON object can span multiple network reads, and a single
        // read can carry several complete objects + a partial.
        let mut byte_stream = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
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
                        yield Err(Error::Other(anyhow::anyhow!("non-utf8 chunk from ollama")));
                        return;
                    }
                };
                buf.push_str(chunk_str);
                while let Some(nl) = buf.find('\n') {
                    let line = buf[..nl].to_owned();
                    buf.drain(..=nl);
                    if line.trim().is_empty() {
                        continue;
                    }
                    let parsed: WireStreamChunk = match serde_json::from_str(&line) {
                        Ok(p) => p,
                        Err(e) => {
                            yield Err(Error::Other(anyhow::anyhow!("parse ollama chunk: {e}")));
                            return;
                        }
                    };
                    if let Some(msg) = parsed.message {
                        if !msg.content.is_empty() {
                            yield Ok(StreamChunk::Token(msg.content));
                        }
                    }
                    if parsed.done {
                        yield Ok(StreamChunk::Done {
                            model_used: parsed.model.unwrap_or_default(),
                            usage: Usage {
                                prompt_tokens: parsed.prompt_eval_count.unwrap_or(0),
                                completion_tokens: parsed.eval_count.unwrap_or(0),
                                elapsed_ms: started.elapsed().as_millis() as u64,
                            },
                        });
                        return;
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        // /api/tags returns every model the daemon has pulled — chat,
        // embed, vision, all in one flat list. The Settings UI lets
        // the user pick from the lot; non-chat models will fail at
        // call time, which is the same failure mode they'd hit if
        // they typed the model name freehand. Filtering server-side
        // would require parsing model metadata that Ollama doesn't
        // expose reliably.
        #[derive(serde::Deserialize)]
        struct TagsModel {
            name: String,
        }
        #[derive(serde::Deserialize)]
        struct TagsResponse {
            #[serde(default)]
            models: Vec<TagsModel>,
        }
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama list_models: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::Other(anyhow::anyhow!(
                "ollama list_models {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let parsed: TagsResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama list_models parse: {e}")))?;
        let mut names: Vec<String> = parsed.models.into_iter().map(|m| m.name).collect();
        names.sort();
        Ok(names)
    }

    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse> {
        let started = Instant::now();
        let inputs: Vec<&str> = req.inputs.iter().map(String::as_str).collect();
        let body = WireEmbedRequest {
            model: &req.model,
            input: inputs,
        };
        let url = format!("{}/api/embed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama embed: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "ollama embed {}: {}",
                status,
                body_text
            )));
        }
        let parsed: WireEmbedResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ollama embed parse: {e}")))?;
        Ok(EmbedResponse {
            model_used: parsed.model,
            vectors: parsed.embeddings,
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `id()` is stored verbatim in the audit log; renaming would
    /// orphan existing history. Pin it.
    #[test]
    fn id_and_privacy_posture_are_stable() {
        let p = OllamaProvider::new(DEFAULT_BASE_URL).unwrap();
        assert_eq!(p.id(), "ollama");
        assert_eq!(p.privacy_posture(), PrivacyPosture::LocalOnly);
    }

    /// Lock the wire format. An Ollama API rename on their side
    /// would break us silently in production; surface it here.
    #[test]
    fn chat_request_serializes_to_ollama_shape() {
        let body = WireChatRequest {
            model: "llama3.1:8b",
            messages: vec![WireMessage {
                role: "user",
                content: "hi",
            }],
            stream: false,
            options: WireOptions {
                temperature: Some(0.2),
                num_predict: None,
                stop: vec![],
            },
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "llama3.1:8b");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hi");
        assert_eq!(json["stream"], false);
        // f32 → JSON serialises as the lossy f32-as-f64, so compare
        // approximately rather than against the literal 0.2.
        let temp = json["options"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.2).abs() < 1e-5);
        // num_predict and stop omitted when default — keeps the
        // payload small and respects Ollama's own defaults.
        assert!(json["options"].get("num_predict").is_none());
        assert!(json["options"].get("stop").is_none());
    }

    #[test]
    fn role_serialisation_matches_ollama_strings() {
        assert_eq!(role_str(ChatRole::System), "system");
        assert_eq!(role_str(ChatRole::User), "user");
        assert_eq!(role_str(ChatRole::Assistant), "assistant");
    }

    /// Embed request shape. Lock the field name (`input`, not
    /// `inputs`) — Ollama's API uses singular even when batched.
    #[test]
    fn embed_request_serializes_to_ollama_shape() {
        let body = WireEmbedRequest {
            model: "nomic-embed-text",
            input: vec!["one", "two"],
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "nomic-embed-text");
        assert_eq!(json["input"][0], "one");
        assert_eq!(json["input"][1], "two");
    }

    /// Parser accepts the documented chat response shape.
    #[test]
    fn chat_response_parses_canonical_payload() {
        let canonical = serde_json::json!({
            "model": "llama3.1:8b-instruct-q4_K_M",
            "created_at": "2026-04-26T10:00:00Z",
            "message": { "role": "assistant", "content": "hello" },
            "done": true,
            "prompt_eval_count": 12,
            "eval_count": 7
        });
        let parsed: WireChatResponse = serde_json::from_value(canonical).unwrap();
        assert_eq!(parsed.model, "llama3.1:8b-instruct-q4_K_M");
        assert_eq!(parsed.message.content, "hello");
        assert_eq!(parsed.prompt_eval_count, Some(12));
        assert_eq!(parsed.eval_count, Some(7));
    }
}
