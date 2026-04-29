//! Hot-swappable provider holder.
//!
//! Wraps the chat + embed providers behind an async RwLock so the
//! Settings → AI handler can replace them at runtime. Every reader
//! goes through `chat()` / `embed()` which take a read lock and
//! clone the inner `Arc` — fast (just a refcount bump) and never
//! blocks the writer for longer than the clone takes.
//!
//! Why a single holder for both providers: the hot-swap is atomic
//! at the settings-save boundary. When a user flips from Ollama to
//! Claude, both providers refresh in the same operation; readers
//! that snapshot mid-swap either see the old pair or the new pair,
//! never a half-applied mix.

use std::sync::Arc;

use tokio::sync::RwLock;

use super::provider::LlmProvider;

#[derive(Default)]
struct Inner {
    chat: Option<Arc<dyn LlmProvider>>,
    embed: Option<Arc<dyn LlmProvider>>,
}

#[derive(Clone, Default)]
pub struct LlmHolder {
    inner: Arc<RwLock<Inner>>,
}

impl LlmHolder {
    pub fn new(
        chat: Option<Arc<dyn LlmProvider>>,
        embed: Option<Arc<dyn LlmProvider>>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { chat, embed })),
        }
    }

    /// Snapshot the chat provider. None when AI is disabled or the
    /// configured provider failed health check.
    pub async fn chat(&self) -> Option<Arc<dyn LlmProvider>> {
        self.inner.read().await.chat.clone()
    }

    /// Snapshot the embed provider. None when AI is disabled or the
    /// embed backend (Ollama by default) is unreachable.
    pub async fn embed(&self) -> Option<Arc<dyn LlmProvider>> {
        self.inner.read().await.embed.clone()
    }

    /// Atomically replace both providers. Called from the Settings
    /// → AI POST handler after the new config has been persisted
    /// and the providers built + healthchecked.
    pub async fn replace(
        &self,
        chat: Option<Arc<dyn LlmProvider>>,
        embed: Option<Arc<dyn LlmProvider>>,
    ) {
        let mut w = self.inner.write().await;
        w.chat = chat;
        w.embed = embed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::llm::provider::{ChatRequest, ChatResponse, EmbedRequest, EmbedResponse, PrivacyPosture};
    use async_trait::async_trait;

    struct DummyProvider(&'static str);

    #[async_trait]
    impl LlmProvider for DummyProvider {
        fn id(&self) -> &'static str {
            self.0
        }
        fn privacy_posture(&self) -> PrivacyPosture {
            PrivacyPosture::LocalOnly
        }
        async fn health(&self) -> Result<()> {
            Ok(())
        }
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
            unreachable!()
        }
        async fn embed(&self, _req: EmbedRequest) -> Result<EmbedResponse> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn empty_holder_returns_none() {
        let h = LlmHolder::default();
        assert!(h.chat().await.is_none());
        assert!(h.embed().await.is_none());
    }

    #[tokio::test]
    async fn replace_swaps_providers() {
        let h = LlmHolder::new(
            Some(Arc::new(DummyProvider("first")) as Arc<dyn LlmProvider>),
            Some(Arc::new(DummyProvider("first-embed")) as Arc<dyn LlmProvider>),
        );
        assert_eq!(h.chat().await.unwrap().id(), "first");

        h.replace(
            Some(Arc::new(DummyProvider("second")) as Arc<dyn LlmProvider>),
            Some(Arc::new(DummyProvider("second-embed")) as Arc<dyn LlmProvider>),
        )
        .await;
        assert_eq!(h.chat().await.unwrap().id(), "second");
        assert_eq!(h.embed().await.unwrap().id(), "second-embed");
    }

    #[tokio::test]
    async fn replace_with_none_disables_provider() {
        let h = LlmHolder::new(
            Some(Arc::new(DummyProvider("on")) as Arc<dyn LlmProvider>),
            None,
        );
        h.replace(None, None).await;
        assert!(h.chat().await.is_none());
        assert!(h.embed().await.is_none());
    }
}
