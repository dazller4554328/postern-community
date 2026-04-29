//! Activity-logging decorator for `LlmProvider`.
//!
//! Wraps any provider so chat / chat_stream / embed calls write a
//! row to `ai_activity_log` automatically. Health probes are NOT
//! logged — they fire constantly in the background and would drown
//! the table.
//!
//! Stored payloads are truncated to 4 KB each side. The *true*
//! sizes are recorded in `input_bytes` / `output_bytes` columns so
//! the UI can show "you sent 47 KB, sample below". Truncation
//! happens in this layer to keep the underlying providers
//! decoupled from any storage-side cap policy.
//!
//! Activity insert errors are deliberately swallowed (`let _ =`).
//! A failed log write must never break a successful chat call —
//! the user would rather have their answer than an empty row in
//! a debug table.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures_util::StreamExt;

use super::provider::{
    ChatRequest, ChatResponse, ChatStream, EmbedRequest, EmbedResponse, LlmProvider,
    PrivacyPosture, StreamChunk, Usage,
};
use crate::error::Result;
use crate::storage::{Db, NewAiActivity};

const MAX_SAMPLE_BYTES: usize = 4 * 1024;

pub struct ActivityLoggedProvider {
    inner: Arc<dyn LlmProvider>,
    db: Arc<Db>,
}

impl ActivityLoggedProvider {
    pub fn wrap(inner: Arc<dyn LlmProvider>, db: Arc<Db>) -> Arc<dyn LlmProvider> {
        Arc::new(Self { inner, db })
    }

    /// Truncate a JSON-serialized payload to `MAX_SAMPLE_BYTES`.
    /// Char-boundary safe — finds the last UTF-8 boundary at or
    /// below the cap. Adds a trailing "…" when truncated so a
    /// reader knows the sample is incomplete.
    fn sample(s: &str) -> String {
        if s.len() <= MAX_SAMPLE_BYTES {
            return s.to_owned();
        }
        let mut end = MAX_SAMPLE_BYTES;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        let mut out = s[..end].to_owned();
        out.push('…');
        out
    }

    fn write(&self, a: NewAiActivity<'_>) {
        let _ = self.db.insert_ai_activity(&a);
    }
}

#[async_trait]
impl LlmProvider for ActivityLoggedProvider {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn privacy_posture(&self) -> PrivacyPosture {
        self.inner.privacy_posture()
    }

    /// Health probes are noisy (one every Settings status fetch
    /// plus boot probe plus settings test). Pass through with no
    /// activity write.
    async fn health(&self) -> Result<()> {
        self.inner.health().await
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let started = Instant::now();
        let req_json = serde_json::to_string(&req).unwrap_or_else(|_| String::new());
        let input_bytes = req_json.len() as u64;
        let req_sample = Self::sample(&req_json);
        let model_req = req.model.clone();

        let result = self.inner.chat(req).await;
        let elapsed = started.elapsed().as_millis() as u64;
        let now = chrono::Utc::now().timestamp();

        match &result {
            Ok(resp) => {
                let resp_json = serde_json::to_string(resp).unwrap_or_else(|_| String::new());
                let output_bytes = resp_json.len() as u64;
                let resp_sample = Self::sample(&resp_json);
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "chat",
                    provider: self.inner.id(),
                    model: &resp.model_used,
                    status: "ok",
                    elapsed_ms: elapsed,
                    prompt_tokens: resp.usage.prompt_tokens,
                    completion_tokens: resp.usage.completion_tokens,
                    input_bytes,
                    output_bytes,
                    request_sample: Some(&req_sample),
                    response_sample: Some(&resp_sample),
                    error_message: None,
                });
            }
            Err(e) => {
                let msg = e.to_string();
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "chat",
                    provider: self.inner.id(),
                    model: &model_req,
                    status: "error",
                    elapsed_ms: elapsed,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes: 0,
                    request_sample: Some(&req_sample),
                    response_sample: None,
                    error_message: Some(&msg),
                });
            }
        }
        result
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream> {
        let started = Instant::now();
        let req_json = serde_json::to_string(&req).unwrap_or_else(|_| String::new());
        let input_bytes = req_json.len() as u64;
        let req_sample = Self::sample(&req_json);
        let model_req = req.model.clone();

        let inner_stream = match self.inner.chat_stream(req).await {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                self.write(NewAiActivity {
                    ts_utc: chrono::Utc::now().timestamp(),
                    kind: "chat_stream",
                    provider: self.inner.id(),
                    model: &model_req,
                    status: "error",
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes: 0,
                    request_sample: Some(&req_sample),
                    response_sample: None,
                    error_message: Some(&msg),
                });
                return Err(e);
            }
        };

        // Wrap the stream so we accumulate the full answer + final
        // usage as tokens arrive, then write the activity row when
        // the stream terminates. The async-stream! crate gives us a
        // Stream we can yield into while keeping the accumulator in
        // local state.
        let provider_id = self.inner.id();
        let db = self.db.clone();
        let wrapped = async_stream::stream! {
            let mut answer_buf = String::new();
            let mut model_used = String::new();
            let mut final_usage = Usage::default();
            let mut errored: Option<String> = None;
            let mut inner = inner_stream;

            while let Some(chunk) = inner.next().await {
                match &chunk {
                    Ok(StreamChunk::Token(t)) => {
                        answer_buf.push_str(t);
                    }
                    Ok(StreamChunk::Done { model_used: m, usage }) => {
                        if !m.is_empty() {
                            model_used = m.clone();
                        }
                        final_usage = usage.clone();
                    }
                    Err(e) => {
                        errored = Some(e.to_string());
                    }
                }
                yield chunk;
            }

            // Stream has terminated (gracefully or not). Persist
            // the activity row now.
            let now = chrono::Utc::now().timestamp();
            let elapsed = started.elapsed().as_millis() as u64;
            let answer_sample = Self::sample(&answer_buf);
            let output_bytes = answer_buf.len() as u64;
            let model_for_log = if model_used.is_empty() {
                model_req.as_str()
            } else {
                model_used.as_str()
            };
            let activity = if let Some(msg) = errored.as_deref() {
                NewAiActivity {
                    ts_utc: now,
                    kind: "chat_stream",
                    provider: provider_id,
                    model: model_for_log,
                    status: "error",
                    elapsed_ms: elapsed,
                    prompt_tokens: final_usage.prompt_tokens,
                    completion_tokens: final_usage.completion_tokens,
                    input_bytes,
                    output_bytes,
                    request_sample: Some(&req_sample),
                    response_sample: if answer_buf.is_empty() {
                        None
                    } else {
                        Some(&answer_sample)
                    },
                    error_message: Some(msg),
                }
            } else {
                NewAiActivity {
                    ts_utc: now,
                    kind: "chat_stream",
                    provider: provider_id,
                    model: model_for_log,
                    status: "ok",
                    elapsed_ms: elapsed,
                    prompt_tokens: final_usage.prompt_tokens,
                    completion_tokens: final_usage.completion_tokens,
                    input_bytes,
                    output_bytes,
                    request_sample: Some(&req_sample),
                    response_sample: Some(&answer_sample),
                    error_message: None,
                }
            };
            let _ = db.insert_ai_activity(&activity);
        };

        Ok(Box::pin(wrapped))
    }

    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse> {
        let started = Instant::now();
        // Embed payloads are noisy (the email body!). Sample but
        // truncate aggressively — 4 KB is enough to debug a single
        // request without storing every email in cleartext-flat
        // form (it's still SQLCipher-encrypted at rest).
        let req_json = serde_json::to_string(&req).unwrap_or_else(|_| String::new());
        let input_bytes = req_json.len() as u64;
        let req_sample = Self::sample(&req_json);
        let model_req = req.model.clone();

        let result = self.inner.embed(req).await;
        let elapsed = started.elapsed().as_millis() as u64;
        let now = chrono::Utc::now().timestamp();

        match &result {
            Ok(resp) => {
                // Don't store the vector itself — pointless, big.
                // A summary suffices: model_used + vector count +
                // dimension. Useful when debugging "why did embed
                // return 768 dims when I expected 1536?"
                let vector_dim = resp.vectors.first().map(Vec::len).unwrap_or(0);
                let summary = serde_json::json!({
                    "model_used": resp.model_used,
                    "vectors_count": resp.vectors.len(),
                    "vector_dim": vector_dim,
                    "usage": {
                        "elapsed_ms": resp.usage.elapsed_ms,
                    },
                });
                let resp_json = summary.to_string();
                let output_bytes = (vector_dim as u64) * (resp.vectors.len() as u64) * 4;
                let resp_sample = Self::sample(&resp_json);
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "embed",
                    provider: self.inner.id(),
                    model: &resp.model_used,
                    status: "ok",
                    elapsed_ms: elapsed,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes,
                    request_sample: Some(&req_sample),
                    response_sample: Some(&resp_sample),
                    error_message: None,
                });
            }
            Err(e) => {
                let msg = e.to_string();
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "embed",
                    provider: self.inner.id(),
                    model: &model_req,
                    status: "error",
                    elapsed_ms: elapsed,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes: 0,
                    request_sample: Some(&req_sample),
                    response_sample: None,
                    error_message: Some(&msg),
                });
            }
        }
        result
    }

    /// Pass through to the inner provider — model discovery doesn't
    /// produce an audit-worthy event (no user content leaves the
    /// box), and without this delegation the trait default returns
    /// an empty Vec, which silently breaks the Settings → AI Polish
    /// model dropdown.
    async fn list_models(&self) -> Result<Vec<String>> {
        self.inner.list_models().await
    }

    /// Pass through transcription with a thin activity-log wrapper
    /// so the user can see how often + how big their voice uploads
    /// went out. Records audio byte size + transcript length only —
    /// the audio itself isn't kept anywhere.
    async fn transcribe(
        &self,
        audio_bytes: Vec<u8>,
        mime_type: &str,
    ) -> Result<String> {
        let started = Instant::now();
        let input_bytes = audio_bytes.len() as u64;
        let result = self.inner.transcribe(audio_bytes, mime_type).await;
        let elapsed = started.elapsed().as_millis() as u64;
        let now = chrono::Utc::now().timestamp();
        match &result {
            Ok(text) => {
                let summary = serde_json::json!({
                    "mime_type": mime_type,
                    "transcript_chars": text.chars().count(),
                });
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "transcribe",
                    provider: self.inner.id(),
                    model: "whisper-1",
                    status: "ok",
                    elapsed_ms: elapsed,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes: text.len() as u64,
                    request_sample: Some(&format!("(audio bytes: {input_bytes}, mime: {mime_type})")),
                    response_sample: Some(&summary.to_string()),
                    error_message: None,
                });
            }
            Err(e) => {
                let msg = e.to_string();
                self.write(NewAiActivity {
                    ts_utc: now,
                    kind: "transcribe",
                    provider: self.inner.id(),
                    model: "whisper-1",
                    status: "error",
                    elapsed_ms: elapsed,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    input_bytes,
                    output_bytes: 0,
                    request_sample: Some(&format!("(audio bytes: {input_bytes}, mime: {mime_type})")),
                    response_sample: None,
                    error_message: Some(&msg),
                });
            }
        }
        result
    }
}
