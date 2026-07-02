//! AI HTTP surface — slim build (post-Datas removal).
//!
//! AI is now used for two things only:
//!   * **Polish** — compose-pane "rewrite this selection" button.
//!     Hits `/api/ai/rewrite`.
//!   * **Dictate** — voice-to-text in compose. Hits
//!     `/api/ai/transcribe`.
//!
//! The rest of the surface is provider configuration: pick a backend
//! (Ollama / `OpenAI` / Anthropic / OpenAI-compatible), supply a key
//! and optionally a base URL, and verify it's reachable.

use axum::{
    extract::{Multipart, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{
    error::{Error, Result},
    llm::{ChatMessage, ChatRequest, ChatRole, PrivacyPosture},
    storage::{AiSettings, UpdateAiSettings},
};

/// Default chat model when the operator hasn't picked one and the
/// active provider doesn't claim a known default. Sized to run on a
/// 16 GB VPS; only sensible when the provider is actually Ollama-
/// shaped.
const DEFAULT_CHAT_MODEL: &str = "llama3.1:8b-instruct-q4_K_M";

/// Provider-appropriate default chat model. Without this the Polish
/// endpoint sent the Ollama-flavoured fallback to OpenAI/Anthropic
/// when neither `polish_chat_model` nor `chat_model` was set, which
/// the cloud APIs reject with 404 `model_not_found`.
fn default_chat_model_for(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "claude-sonnet-4-6",
        "openai" => "gpt-4o-mini",
        _ => DEFAULT_CHAT_MODEL,
    }
}

/// Cap on the input text the rewrite endpoint will accept. Bigger
/// inputs need to be split client-side; the model context window
/// would still take them, but the latency + token cost rises non-
/// linearly past this point and the UI becomes unusable.
const REWRITE_MAX_INPUT_CHARS: usize = 4000;

/// Per-call rewrite parameters. Kept low and short — the rewrite
/// task is not creative; we want a deterministic re-phrasing of the
/// same facts, not a new draft.
const REWRITE_TEMPERATURE: f32 = 0.3;
const REWRITE_MAX_TOKENS: u32 = 1500;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ai/status", get(status))
        .route("/ai/settings", get(get_settings).post(post_settings))
        .route("/ai/enabled", post(post_enabled))
        .route("/ai/test", post(test_settings))
        .route("/ai/rewrite", post(rewrite))
        .route("/ai/models", get(list_models))
        .route("/ai/transcribe", post(transcribe))
}

// ─────────────────────────── status ───────────────────────────

#[derive(Serialize)]
struct StatusResponse {
    enabled: bool,
    provider: Option<String>,
    privacy_posture: Option<PrivacyPosture>,
    chat_model: String,
}

async fn status(State(s): State<AppState>) -> Json<StatusResponse> {
    let settings = s.db.get_ai_settings().ok();
    let chat = s.llm.chat().await;
    let provider_id = chat.as_ref().map(|p| p.id());
    let chat_model = settings
        .as_ref()
        .map(|s| s.chat_model.clone())
        .filter(|m| !m.is_empty())
        .or_else(|| std::env::var("POSTERN_CHAT_MODEL").ok())
        .unwrap_or_else(|| default_chat_model_for(provider_id.unwrap_or("")).to_owned());
    Json(StatusResponse {
        enabled: chat.is_some(),
        provider: provider_id.map(str::to_owned),
        privacy_posture: chat.as_ref().map(|p| p.privacy_posture()),
        chat_model,
    })
}

// ─────────────────────── settings GET / POST ───────────────────

/// Public DTO for `GET /api/ai/settings`. Mirrors the persisted row,
/// minus the encrypted API key — we only surface whether one is set.
#[derive(Serialize)]
struct AiSettingsDto {
    enabled: bool,
    provider_kind: String,
    chat_model: String,
    base_url: Option<String>,
    api_key_set: bool,
    cloud_consent: bool,
    auto_start: bool,
    polish_chat_model: Option<String>,
    updated_at: i64,
}

impl From<AiSettings> for AiSettingsDto {
    fn from(s: AiSettings) -> Self {
        let api_key_set = s.api_key_ref.is_some();
        Self {
            enabled: s.enabled,
            provider_kind: s.provider_kind,
            chat_model: s.chat_model,
            base_url: s.base_url,
            api_key_set,
            cloud_consent: s.cloud_consent,
            auto_start: s.auto_start,
            polish_chat_model: s.polish_chat_model,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct UpdateSettingsBody {
    enabled: bool,
    provider_kind: String,
    chat_model: Option<String>,
    base_url: Option<String>,
    /// Three states: missing/null = leave existing key alone; ""
    /// = clear; non-empty = rotate.
    api_key: Option<String>,
    /// Required to be `true` when the chosen provider resolves to a
    /// cloud vendor.
    #[serde(default)]
    cloud_consent: bool,
    #[serde(default = "default_auto_start")]
    auto_start: bool,
    /// Polish-feature model override. Three states:
    /// missing = leave; "" = clear (inherit `chat_model`); non-empty
    /// = replace.
    polish_chat_model: Option<String>,
}

fn default_auto_start() -> bool {
    true
}

async fn get_settings(State(s): State<AppState>) -> Result<Json<AiSettingsDto>> {
    let row = s.db.get_ai_settings()?;
    Ok(Json(row.into()))
}

async fn post_settings(
    State(s): State<AppState>,
    Json(body): Json<UpdateSettingsBody>,
) -> Result<Json<AiSettingsDto>> {
    s.vault.require_unlocked()?;
    if !crate::tier::ALLOW_AI {
        return Err(Error::BadRequest(
            "AI features disabled in this build".into(),
        ));
    }
    validate_provider_kind(&body.provider_kind)?;

    let cloud = is_cloud_provider(&body.provider_kind, body.base_url.as_deref());
    if cloud && !body.cloud_consent {
        return Err(Error::BadRequest(
            "Cloud provider selected but cloud_consent was not granted. Tick the confirmation in Settings → AI before saving.".into(),
        ));
    }

    let prior = s.db.get_ai_settings().ok();
    let resolved_polish_chat_model = match body.polish_chat_model.as_ref() {
        None => prior.as_ref().and_then(|p| p.polish_chat_model.clone()),
        Some(v) if v.trim().is_empty() => None,
        Some(v) => Some(v.clone()),
    };

    let update = UpdateAiSettings {
        enabled: body.enabled,
        provider_kind: &body.provider_kind,
        chat_model: body.chat_model.as_deref().unwrap_or(""),
        base_url: body.base_url.as_deref(),
        cloud_consent: body.cloud_consent,
        auto_start: body.auto_start,
        polish_chat_model: resolved_polish_chat_model.as_deref(),
    };
    let saved =
        s.db.set_ai_settings(&update, body.api_key.as_deref(), &s.vault)?;

    // Rebuild the chat provider from the new settings.
    let api_key = s.db.ai_api_key(&s.vault)?;
    let bind_iface = s.vpn.bind_iface();
    let chat = crate::llm::build_chat_provider(&saved, api_key.as_deref(), bind_iface.as_deref())?;

    if let Some(p) = chat.as_ref() {
        if let Err(e) = p.health().await {
            tracing::warn!(error = %e, provider = p.id(), "ai/settings: chat provider failed health probe (still installed)");
        }
    }
    s.llm.replace(chat).await;

    Ok(Json(saved.into()))
}

// ─────────────────────── enabled toggle ────────────────────────

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn post_enabled(
    State(s): State<AppState>,
    Json(body): Json<EnabledBody>,
) -> Result<Json<AiSettingsDto>> {
    s.vault.require_unlocked()?;
    if !crate::tier::ALLOW_AI {
        return Err(Error::BadRequest(
            "AI features disabled in this build".into(),
        ));
    }
    let current = s.db.get_ai_settings()?;
    if current.enabled == body.enabled {
        return Ok(Json(current.into()));
    }
    let update = UpdateAiSettings {
        enabled: body.enabled,
        provider_kind: &current.provider_kind,
        chat_model: &current.chat_model,
        base_url: current.base_url.as_deref(),
        cloud_consent: current.cloud_consent,
        auto_start: current.auto_start,
        polish_chat_model: current.polish_chat_model.as_deref(),
    };
    let saved = s.db.set_ai_settings(&update, None, &s.vault)?;

    if body.enabled {
        let api_key = s.db.ai_api_key(&s.vault)?;
        let bind_iface = s.vpn.bind_iface();
        let chat =
            crate::llm::build_chat_provider(&saved, api_key.as_deref(), bind_iface.as_deref())?;
        s.llm.replace(chat).await;
    } else {
        s.llm.replace(None).await;
    }
    Ok(Json(saved.into()))
}

// ─────────────────────────── test ───────────────────────────

#[derive(Deserialize)]
struct TestBody {
    provider_kind: String,
    chat_model: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct TestResponse {
    ok: bool,
    provider: String,
    privacy_posture: String,
    message: Option<String>,
}

async fn test_settings(
    State(s): State<AppState>,
    Json(body): Json<TestBody>,
) -> Result<Json<TestResponse>> {
    s.vault.require_unlocked()?;
    if !crate::tier::ALLOW_AI {
        return Err(Error::BadRequest(
            "AI features disabled in this build".into(),
        ));
    }
    validate_provider_kind(&body.provider_kind)?;

    let api_key = match body.api_key.as_deref() {
        Some(k) if !k.is_empty() => Some(k.to_owned()),
        _ => s.db.ai_api_key(&s.vault)?,
    };

    let candidate = AiSettings {
        enabled: true,
        provider_kind: body.provider_kind.clone(),
        chat_model: body.chat_model.clone().unwrap_or_default(),
        base_url: body.base_url.clone(),
        api_key_ref: api_key.as_ref().map(|_| "candidate".to_owned()),
        cloud_consent: true,
        auto_start: false,
        polish_chat_model: None,
        updated_at: 0,
    };

    let bind_iface = s.vpn.bind_iface();
    let provider = match crate::llm::build_chat_provider(
        &candidate,
        api_key.as_deref(),
        bind_iface.as_deref(),
    )? {
        Some(p) => p,
        None => {
            return Ok(Json(TestResponse {
                ok: false,
                provider: candidate.provider_kind,
                privacy_posture: "unknown".into(),
                message: Some("provider build returned None (AI disabled?)".into()),
            }));
        }
    };
    let posture = posture_str(provider.privacy_posture()).to_owned();
    let id = provider.id().to_owned();
    match provider.health().await {
        Ok(()) => Ok(Json(TestResponse {
            ok: true,
            provider: id,
            privacy_posture: posture,
            message: None,
        })),
        Err(e) => Ok(Json(TestResponse {
            ok: false,
            provider: id,
            privacy_posture: posture,
            message: Some(e.to_string()),
        })),
    }
}

// ─────────────────────────── rewrite ───────────────────────────

#[derive(Deserialize)]
struct RewriteBody {
    text: String,
    #[serde(default)]
    tone: Option<String>,
}

#[derive(Serialize)]
struct RewriteResponse {
    rewritten: String,
    provider: String,
    chat_model: String,
    privacy_posture: PrivacyPosture,
    elapsed_ms: u64,
    prompt_tokens: u32,
    completion_tokens: u32,
}

async fn rewrite(
    State(s): State<AppState>,
    Json(body): Json<RewriteBody>,
) -> Result<Json<RewriteResponse>> {
    s.vault.require_unlocked()?;
    if !crate::tier::ALLOW_AI {
        return Err(Error::BadRequest(
            "AI features disabled in this build".into(),
        ));
    }
    let chat_provider = s.llm.chat().await.ok_or_else(|| {
        Error::BadRequest("AI is not configured — open Settings → AI to pick a provider.".into())
    })?;

    let text = body.text.trim();
    if text.is_empty() {
        return Err(Error::BadRequest("text is required".into()));
    }
    if text.chars().count() > REWRITE_MAX_INPUT_CHARS {
        return Err(Error::BadRequest(format!(
            "text too long for rewrite (max {REWRITE_MAX_INPUT_CHARS} characters)"
        )));
    }

    let tone = match body.tone.as_deref().map(str::trim) {
        Some("concise") => "concise",
        Some("friendly") => "friendly",
        _ => "professional",
    };

    let settings = s.db.get_ai_settings().ok();
    let chat_model = settings
        .as_ref()
        .and_then(|x| x.polish_chat_model.as_deref())
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .or_else(|| settings.as_ref().map(|x| x.chat_model.clone()))
        .filter(|m| !m.is_empty())
        .or_else(|| std::env::var("POSTERN_CHAT_MODEL").ok())
        .unwrap_or_else(|| default_chat_model_for(chat_provider.id()).to_owned());

    let started = std::time::Instant::now();
    let system = format!(
        "You are a writing assistant for a single email draft. Rewrite the \
         user's text in a {tone} tone. Preserve their meaning, intent, and \
         every concrete fact they mentioned (names, dates, amounts, links, \
         attachments, addresses). Do not invent new facts. Do not add a \
         greeting or sign-off unless one was already present. Match the \
         user's language. Return only the rewritten text — no preamble, \
         no commentary, no surrounding quotes. If the draft is already \
         well-written, return it unchanged."
    );

    let chat = chat_provider
        .chat(ChatRequest {
            model: chat_model.clone(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: system,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: text.to_owned(),
                },
            ],
            temperature: Some(REWRITE_TEMPERATURE),
            max_tokens: Some(REWRITE_MAX_TOKENS),
            stop: vec![],
        })
        .await?;

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let posture = chat_provider.privacy_posture();
    let cleaned = strip_rewrite_preamble(&chat.content);

    Ok(Json(RewriteResponse {
        rewritten: cleaned,
        provider: chat_provider.id().to_owned(),
        chat_model: chat.model_used,
        privacy_posture: posture,
        elapsed_ms,
        prompt_tokens: chat.usage.prompt_tokens,
        completion_tokens: chat.usage.completion_tokens,
    }))
}

/// Strip the wrapper artefacts smaller chat models add despite being
/// told not to.
fn strip_rewrite_preamble(s: &str) -> String {
    let mut t = s.trim();

    let pairs: &[(char, char)] = &[
        ('"', '"'),
        ('\'', '\''),
        ('\u{201C}', '\u{201D}'),
        ('\u{2018}', '\u{2019}'),
    ];
    for (open, close) in pairs {
        let mut chars = t.chars();
        if chars.next() == Some(*open) && t.ends_with(*close) {
            let inner = &t[open.len_utf8()..t.len() - close.len_utf8()];
            if !inner.contains(*open) && !inner.contains(*close) {
                t = inner.trim();
                break;
            }
        }
    }

    let lower_head: String = t.chars().take(60).collect::<String>().to_lowercase();
    let preambles = [
        "here is the rewritten",
        "here's the rewritten",
        "here is the polished",
        "here's the polished",
        "here is a rewritten",
        "here's a rewritten",
        "here is a polished",
        "here's a polished",
        "sure, here",
        "sure! here",
        "rewritten:",
        "polished:",
    ];
    for p in &preambles {
        if lower_head.starts_with(p) {
            if let Some(nl) = t.find('\n') {
                t = t[nl + 1..].trim_start();
            }
            break;
        }
    }

    t.to_owned()
}

// ─────────────────────────── transcribe ───────────────────────────

#[derive(Serialize)]
struct TranscribeResponse {
    text: String,
    provider: String,
    elapsed_ms: u64,
    audio_bytes: u64,
}

const TRANSCRIBE_MAX_BYTES: usize = 20 * 1024 * 1024;

async fn transcribe(
    State(s): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<TranscribeResponse>> {
    s.vault.require_unlocked()?;
    if !crate::tier::ALLOW_AI {
        return Err(Error::BadRequest(
            "AI features disabled in this build".into(),
        ));
    }
    let chat_provider = s.llm.chat().await.ok_or_else(|| {
        Error::BadRequest("AI is not configured — open Settings → AI to pick a provider.".into())
    })?;

    let mut audio_bytes: Option<Vec<u8>> = None;
    let mut mime_type = String::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::BadRequest(format!("multipart read: {e}")))?
    {
        if field.name() == Some("file") {
            mime_type = field.content_type().unwrap_or("audio/webm").to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| Error::BadRequest(format!("multipart bytes: {e}")))?;
            if bytes.len() > TRANSCRIBE_MAX_BYTES {
                return Err(Error::BadRequest(format!(
                    "audio too large ({} bytes; max {})",
                    bytes.len(),
                    TRANSCRIBE_MAX_BYTES
                )));
            }
            audio_bytes = Some(bytes.to_vec());
        }
    }
    let audio_bytes = audio_bytes
        .ok_or_else(|| Error::BadRequest("missing 'file' part in multipart upload".into()))?;
    if audio_bytes.is_empty() {
        return Err(Error::BadRequest("empty audio upload".into()));
    }
    let n_bytes = audio_bytes.len() as u64;

    let started = std::time::Instant::now();
    let text = chat_provider.transcribe(audio_bytes, &mime_type).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    Ok(Json(TranscribeResponse {
        text,
        provider: chat_provider.id().to_owned(),
        elapsed_ms,
        audio_bytes: n_bytes,
    }))
}

// ─────────────────────────── models ───────────────────────────

#[derive(Serialize)]
struct ModelsResponse {
    provider: String,
    models: Vec<String>,
    error: Option<String>,
}

async fn list_models(State(s): State<AppState>) -> Json<ModelsResponse> {
    let chat = s.llm.chat().await;
    let Some(provider) = chat else {
        return Json(ModelsResponse {
            provider: String::new(),
            models: Vec::new(),
            error: Some("AI is not configured — open Settings → AI to pick a provider.".into()),
        });
    };
    let provider_id = provider.id().to_owned();
    match provider.list_models().await {
        Ok(models) => Json(ModelsResponse {
            provider: provider_id,
            models,
            error: None,
        }),
        Err(e) => Json(ModelsResponse {
            provider: provider_id,
            models: Vec::new(),
            error: Some(e.to_string()),
        }),
    }
}

// ─────────────────────────── helpers ───────────────────────────

fn validate_provider_kind(kind: &str) -> Result<()> {
    match kind {
        "ollama" | "anthropic" | "openai" | "openai_compat" => Ok(()),
        other => Err(Error::BadRequest(format!(
            "unknown provider kind '{other}' (expected one of: ollama, anthropic, openai, openai_compat)"
        ))),
    }
}

fn is_cloud_provider(kind: &str, base_url: Option<&str>) -> bool {
    match kind {
        "anthropic" | "openai" => true,
        "openai_compat" => {
            let lower = base_url.unwrap_or("").to_ascii_lowercase();
            [
                "api.x.ai",
                "api.groq.com",
                "api.together.xyz",
                "api.perplexity.ai",
                "api.deepseek.com",
                "api.mistral.ai",
            ]
            .iter()
            .any(|m| lower.contains(m))
        }
        _ => false,
    }
}

fn posture_str(p: PrivacyPosture) -> &'static str {
    match p {
        PrivacyPosture::LocalOnly => "local_only",
        PrivacyPosture::UserControlledRemote => "user_controlled_remote",
        PrivacyPosture::ThirdPartyCloud => "third_party_cloud",
    }
}
