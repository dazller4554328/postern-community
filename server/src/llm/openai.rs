//! OpenAI-compatible backend.
//!
//! Covers three things in one impl:
//!   * Real OpenAI (`https://api.openai.com/v1`) — `id() = "openai"`.
//!   * xAI / Grok (`https://api.x.ai/v1`) — `id() = "openai_compat"`,
//!     same wire format.
//!   * Self-hosted vLLM / llama.cpp-server / text-generation-inference,
//!     all of which expose an OpenAI-compatible `/v1/chat/completions`
//!     and (vLLM) `/v1/embeddings` surface.
//!
//! Privacy posture is determined by the constructor:
//!   * `new_openai`  → `ThirdPartyCloud`
//!   * `new_compat`  → `UserControlledRemote` for self-hosted, or
//!                     `ThirdPartyCloud` when the user picks a hosted
//!                     vendor like xAI. We can't tell from a URL alone,
//!                     so the constructor takes the posture explicitly.

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

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    /// Stable id surfaced to audit-log + UI. `"openai"` for the
    /// real provider, `"openai_compat"` for everything that speaks
    /// the same protocol (xAI, vLLM, etc).
    id: &'static str,
    posture: PrivacyPosture,
}

impl OpenAiProvider {
    /// Real OpenAI. Posture is fixed `ThirdPartyCloud`.
    /// `bind_iface` should be the VPN interface (e.g. `"wg0"`) when
    /// the VPN is up — without it, the kill-switch chain blocks
    /// the outbound HTTPS call. None for unbound default-route.
    pub fn new_openai(api_key: impl Into<String>, bind_iface: Option<&str>) -> Result<Self> {
        Self::build(
            OPENAI_BASE_URL.to_owned(),
            api_key.into(),
            "openai",
            PrivacyPosture::ThirdPartyCloud,
            bind_iface,
        )
    }

    /// Generic OpenAI-compatible endpoint. Caller decides the posture
    /// — `ThirdPartyCloud` for hosted vendors (xAI), `UserControlled
    /// Remote` for a self-hosted vLLM on your own VPS.
    pub fn new_compat(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        posture: PrivacyPosture,
        bind_iface: Option<&str>,
    ) -> Result<Self> {
        Self::build(
            base_url.into(),
            api_key.into(),
            "openai_compat",
            posture,
            bind_iface,
        )
    }

    fn build(
        base_url: String,
        api_key: String,
        id: &'static str,
        posture: PrivacyPosture,
        bind_iface: Option<&str>,
    ) -> Result<Self> {
        if api_key.trim().is_empty() {
            return Err(Error::BadRequest(
                "OpenAI-compatible provider requires an API key".into(),
            ));
        }
        let mut builder = Client::builder().timeout(Duration::from_secs(180));
        // SO_BINDTODEVICE on Linux. Mirrors how IMAP/SMTP route
        // through wg0 when the VPN is up; without it the kill-switch
        // chain REJECTs the outbound HTTPS call before it leaves.
        if let Some(iface) = bind_iface {
            builder = builder.interface(iface);
        }
        let client = builder
            .build()
            .map_err(|e| Error::Other(anyhow::anyhow!("build openai client: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
            id,
            posture,
        })
    }
}

// ---------- wire types ----------------------------------------------

#[derive(Serialize)]
struct WireChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// Legacy parameter name — works for gpt-3.5, gpt-4*, gpt-4o*,
    /// and most OpenAI-compat backends. Newer reasoning models
    /// (o1/o3/o4) and the gpt-5 family reject it and require
    /// `max_completion_tokens` instead. Exactly one of the two is
    /// populated per request based on the model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    /// Reasoning-effort knob for the gpt-5 family + o-series. Values
    /// the OpenAI API accepts: "minimal" (gpt-5 only), "low",
    /// "medium", "high". We pin to the lowest viable value so the
    /// model spends most of `max_completion_tokens` on visible output
    /// instead of internal thinking. Older models silently ignore it
    /// — at worst this is harmless on classic chat models, but we
    /// only emit it for models that actually consume it.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    stream: bool,
    /// Streaming usage block — required to get token counts back on
    /// the final SSE event for OpenAI. Compat backends ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<WireStreamOpts>,
}

/// Pick the reasoning-effort value to send for a given model. None
/// for everything that isn't a known reasoning model (so we don't
/// confuse openai_compat backends with a parameter they may reject).
/// gpt-5 family supports "minimal", which is faster than "low" and
/// gives near-direct answers; o-series only goes down to "low".
fn reasoning_effort_for(model: &str) -> Option<&'static str> {
    let m = model.to_ascii_lowercase();
    if m.starts_with("gpt-5") || m.starts_with("gpt5") {
        Some("minimal")
    } else if m.starts_with('o')
        && m.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
    {
        Some("low")
    } else {
        None
    }
}

/// Map an audio MIME type to the file extension Whisper's
/// server-side sniff expects. Falls back to `webm` for unknown
/// types — that's what browser MediaRecorder emits by default and
/// it's the safe choice when we can't tell.
fn mime_to_ext(mime: &str) -> &'static str {
    let m = mime.to_ascii_lowercase();
    if m.contains("mp4") || m.contains("m4a") {
        "m4a"
    } else if m.contains("ogg") || m.contains("opus") {
        "ogg"
    } else if m.contains("wav") {
        "wav"
    } else if m.contains("mp3") || m.contains("mpeg") {
        "mp3"
    } else {
        "webm"
    }
}

/// gpt-5 supports temperature; o1/o3/o4 force it to 1.0 and 400 if
/// you send a non-default value. Easiest safe path: drop temperature
/// entirely for the o-series so the API uses its default. Returns
/// the temperature value to actually send.
fn effective_temperature(model: &str, requested: Option<f32>) -> Option<f32> {
    let m = model.to_ascii_lowercase();
    let is_o_series = m.starts_with('o')
        && m.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
    if is_o_series {
        None
    } else {
        requested
    }
}

/// Pick which output-cap parameter the given model expects. Heuristic
/// based on prefix because OpenAI doesn't expose this in `/v1/models`.
/// Conservative: only models we know need the new name route there;
/// everything else stays on `max_tokens` so OpenAI-compat backends
/// (Grok, Groq, vLLM, etc.) keep working unchanged.
fn uses_max_completion_tokens(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    // Reasoning models: o1, o1-mini, o1-pro, o3, o3-mini, o4-mini, …
    // The "o" family universally rejects max_tokens. Match a digit
    // after the "o" so we don't false-match an arbitrary "o..." name.
    let starts_with_o_digit = m.starts_with('o')
        && m.chars()
            .nth(1)
            .is_some_and(|c| c.is_ascii_digit());
    // gpt-5 family — gpt-5, gpt-5-mini, gpt-5-nano, etc.
    let is_gpt5 = m.starts_with("gpt-5") || m.starts_with("gpt5");
    starts_with_o_digit || is_gpt5
}

#[derive(Serialize)]
struct WireStreamOpts {
    include_usage: bool,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct WireChatResponse {
    model: String,
    choices: Vec<WireChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireResponseMessage,
}

#[derive(Deserialize)]
struct WireResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

/// Per-event shape for SSE streaming. OpenAI emits `data: {json}\n\n`
/// events; the final event is a literal `data: [DONE]`. Each event is
/// a partial completion with one or more `choices[].delta.content`.
#[derive(Deserialize)]
struct WireStreamEvent {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<WireStreamChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct WireStreamChoice {
    #[serde(default)]
    delta: WireStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct WireStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Serialize)]
struct WireEmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct WireEmbedResponse {
    model: String,
    data: Vec<WireEmbedItem>,
}

#[derive(Deserialize)]
struct WireEmbedItem {
    embedding: Vec<f32>,
}

fn role_str(r: ChatRole) -> &'static str {
    match r {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn privacy_posture(&self) -> PrivacyPosture {
        self.posture
    }

    async fn health(&self) -> Result<()> {
        // /v1/models is the cheapest auth+reachability probe — both
        // OpenAI and every compat backend implement it. A 200 means
        // the key is at least the right shape.
        let url = format!("{}/models", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("openai health: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} health {}: {}",
                self.id,
                status,
                body
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
        let (max_tokens, max_completion_tokens) = if uses_max_completion_tokens(&req.model) {
            (None, req.max_tokens)
        } else {
            (req.max_tokens, None)
        };
        let body = WireChatRequest {
            model: &req.model,
            messages,
            temperature: effective_temperature(&req.model, req.temperature),
            max_tokens,
            max_completion_tokens,
            reasoning_effort: reasoning_effort_for(&req.model),
            stop: req.stop,
            stream: false,
            stream_options: None,
        };
        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} chat: {e}", self.id)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} chat {}: {}",
                self.id,
                status,
                body_text
            )));
        }
        let parsed: WireChatResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} parse: {e}", self.id)))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        let usage = parsed.usage.unwrap_or_default();
        Ok(ChatResponse {
            content,
            model_used: parsed.model,
            usage: Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
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
        let (max_tokens, max_completion_tokens) = if uses_max_completion_tokens(&req.model) {
            (None, req.max_tokens)
        } else {
            (req.max_tokens, None)
        };
        let body = WireChatRequest {
            model: &req.model,
            messages,
            temperature: effective_temperature(&req.model, req.temperature),
            max_tokens,
            max_completion_tokens,
            reasoning_effort: reasoning_effort_for(&req.model),
            stop: req.stop,
            stream: true,
            stream_options: Some(WireStreamOpts { include_usage: true }),
        };
        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} chat_stream: {e}", self.id)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} chat_stream {}: {}",
                self.id,
                status,
                body_text
            )));
        }

        let provider_id = self.id;
        let mut byte_stream = resp.bytes_stream();
        let stream = async_stream::stream! {
            // SSE accumulator: raw bytes until a `\n\n` event boundary,
            // then split off `data: …` lines and parse them. Buffer
            // matters: a JSON object can split across reads, and a
            // single read can carry several events.
            let mut buf = String::new();
            let mut last_model: Option<String> = None;
            let mut last_usage = WireUsage::default();
            let mut saw_done = false;
            'reader: while let Some(item) = byte_stream.next().await {
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
                            "non-utf8 chunk from {}",
                            provider_id
                        )));
                        return;
                    }
                };
                buf.push_str(chunk_str);
                // SSE events are terminated by a blank line — `\n\n`.
                while let Some(end) = buf.find("\n\n") {
                    let event = buf[..end].to_owned();
                    buf.drain(..=end + 1);
                    for line in event.lines() {
                        let line = line.trim();
                        let Some(payload) = line.strip_prefix("data:") else {
                            continue;
                        };
                        let payload = payload.trim();
                        if payload.is_empty() {
                            continue;
                        }
                        if payload == "[DONE]" {
                            saw_done = true;
                            break 'reader;
                        }
                        let parsed: WireStreamEvent = match serde_json::from_str(payload) {
                            Ok(p) => p,
                            Err(e) => {
                                yield Err(Error::Other(anyhow::anyhow!(
                                    "parse {} sse: {e}",
                                    provider_id
                                )));
                                return;
                            }
                        };
                        if let Some(m) = parsed.model {
                            last_model = Some(m);
                        }
                        if let Some(u) = parsed.usage {
                            last_usage = u;
                        }
                        for choice in parsed.choices {
                            if let Some(content) = choice.delta.content {
                                if !content.is_empty() {
                                    yield Ok(StreamChunk::Token(content));
                                }
                            }
                            if choice.finish_reason.is_some() {
                                // OpenAI sends finish_reason=stop on
                                // the penultimate event then a usage-
                                // only event then [DONE]. Keep going
                                // to capture the usage.
                            }
                        }
                    }
                }
            }
            if !saw_done {
                // Stream ended without [DONE] — still emit a Done so
                // the client doesn't hang. Token counts may be 0.
            }
            yield Ok(StreamChunk::Done {
                model_used: last_model.unwrap_or_default(),
                usage: Usage {
                    prompt_tokens: last_usage.prompt_tokens,
                    completion_tokens: last_usage.completion_tokens,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            });
        };
        Ok(Box::pin(stream))
    }

    async fn transcribe(
        &self,
        audio_bytes: Vec<u8>,
        mime_type: &str,
    ) -> Result<String> {
        // OpenAI's Whisper endpoint accepts mp3, mp4, mpeg, mpga,
        // m4a, wav, webm. Pick a filename extension matching the
        // upload's MIME so the server-side sniff agrees. Default to
        // .webm since that's what browser MediaRecorder emits.
        let ext = mime_to_ext(mime_type);
        let filename = format!("audio.{ext}");
        let part = reqwest::multipart::Part::bytes(audio_bytes)
            .file_name(filename)
            .mime_str(mime_type)
            .map_err(|e| Error::Other(anyhow::anyhow!("transcribe mime: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            // whisper-1 is OpenAI's only transcription model name
            // as of 2026-04. Hardcoded — the chat-model picker
            // doesn't apply here. If they ever add gpt-4o-transcribe
            // etc. we'd surface a separate setting.
            .text("model", "whisper-1")
            // Force JSON response so we can deserialize the text
            // field cleanly. Default is verbose_json.
            .text("response_format", "json");
        let url = format!("{}/audio/transcriptions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} transcribe: {e}", self.id)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} transcribe {}: {}",
                self.id,
                status,
                body
            )));
        }
        #[derive(serde::Deserialize)]
        struct WireTranscription {
            text: String,
        }
        let parsed: WireTranscription = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} transcribe parse: {e}", self.id)))?;
        Ok(parsed.text)
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        // /v1/models returns everything the account can call — chat,
        // embed, vision, fine-tunes — in a single flat list. We
        // surface the lot and let the user pick; calling an
        // embedding-only model for chat will fail at call time with
        // a clear message, same as if they'd typed the name freehand.
        // Filtering server-side would mean maintaining an allow-list
        // that drifts behind every OpenAI model release.
        #[derive(serde::Deserialize)]
        struct ModelEntry {
            id: String,
        }
        #[derive(serde::Deserialize)]
        struct ModelsResponse {
            #[serde(default)]
            data: Vec<ModelEntry>,
        }
        let url = format!("{}/models", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} list_models: {e}", self.id)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} list_models {}: {}",
                self.id,
                status,
                body
            )));
        }
        let parsed: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} list_models parse: {e}", self.id)))?;
        let mut names: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
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
        let url = format!("{}/embeddings", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} embed: {e}", self.id)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "{} embed {}: {}",
                self.id,
                status,
                body_text
            )));
        }
        let parsed: WireEmbedResponse = resp
            .json()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("{} embed parse: {e}", self.id)))?;
        Ok(EmbedResponse {
            model_used: parsed.model,
            vectors: parsed.data.into_iter().map(|d| d.embedding).collect(),
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

    #[test]
    fn id_and_posture_split_by_constructor() {
        let real = OpenAiProvider::new_openai("sk-test", None).unwrap();
        assert_eq!(real.id(), "openai");
        assert_eq!(real.privacy_posture(), PrivacyPosture::ThirdPartyCloud);

        let compat = OpenAiProvider::new_compat(
            "https://api.x.ai/v1",
            "xai-test",
            PrivacyPosture::ThirdPartyCloud,
            None,
        )
        .unwrap();
        assert_eq!(compat.id(), "openai_compat");
        assert_eq!(compat.privacy_posture(), PrivacyPosture::ThirdPartyCloud);

        let local = OpenAiProvider::new_compat(
            "http://vllm.lan:8000/v1",
            "anything",
            PrivacyPosture::UserControlledRemote,
            None,
        )
        .unwrap();
        assert_eq!(local.privacy_posture(), PrivacyPosture::UserControlledRemote);
    }

    #[test]
    fn rejects_empty_api_key() {
        let r = OpenAiProvider::new_openai("", None);
        assert!(r.is_err());
        let r = OpenAiProvider::new_compat(
            "https://x.test",
            "  ",
            PrivacyPosture::ThirdPartyCloud,
            None,
        );
        assert!(r.is_err());
    }

    #[test]
    fn chat_request_serialises_to_openai_shape() {
        let body = WireChatRequest {
            model: "gpt-4o-mini",
            messages: vec![WireMessage {
                role: "user",
                content: "hi",
            }],
            temperature: Some(0.2),
            max_tokens: Some(64),
            max_completion_tokens: None,
            reasoning_effort: None,
            stop: vec![],
            stream: false,
            stream_options: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "gpt-4o-mini");
        assert_eq!(json["max_tokens"], 64);
        assert!(json.get("max_completion_tokens").is_none());
        assert_eq!(json["stream"], false);
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn newer_models_route_to_max_completion_tokens() {
        // gpt-5 family + the o1/o3/o4 reasoning models reject
        // max_tokens. These cases should return true so the request
        // builder picks the new field name.
        assert!(uses_max_completion_tokens("gpt-5"));
        assert!(uses_max_completion_tokens("gpt-5-mini"));
        assert!(uses_max_completion_tokens("gpt-5-nano"));
        assert!(uses_max_completion_tokens("o1"));
        assert!(uses_max_completion_tokens("o1-mini"));
        assert!(uses_max_completion_tokens("o3-mini"));
        assert!(uses_max_completion_tokens("o4-mini"));
        // Older models keep the legacy parameter so OpenAI-compat
        // backends (Grok, Groq, vLLM) and gpt-3.5/4*/4o* keep working.
        assert!(!uses_max_completion_tokens("gpt-4o"));
        assert!(!uses_max_completion_tokens("gpt-4o-mini"));
        assert!(!uses_max_completion_tokens("gpt-4-turbo"));
        assert!(!uses_max_completion_tokens("gpt-3.5-turbo"));
        assert!(!uses_max_completion_tokens("grok-beta"));
        assert!(!uses_max_completion_tokens("llama3"));
    }

    #[test]
    fn chat_response_parses_canonical_payload() {
        let canonical = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "gpt-4o-mini-2024-07-18",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hello" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12 }
        });
        let parsed: WireChatResponse = serde_json::from_value(canonical).unwrap();
        assert_eq!(parsed.model, "gpt-4o-mini-2024-07-18");
        assert_eq!(parsed.choices[0].message.content.as_deref(), Some("hello"));
        let u = parsed.usage.unwrap();
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.completion_tokens, 2);
    }

    #[test]
    fn embed_response_parses_into_vectors() {
        let canonical = serde_json::json!({
            "object": "list",
            "model": "text-embedding-3-small",
            "data": [
                { "object": "embedding", "embedding": [0.1, 0.2, 0.3], "index": 0 },
                { "object": "embedding", "embedding": [0.4, 0.5, 0.6], "index": 1 }
            ]
        });
        let parsed: WireEmbedResponse = serde_json::from_value(canonical).unwrap();
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].embedding[0], 0.1);
    }
}
