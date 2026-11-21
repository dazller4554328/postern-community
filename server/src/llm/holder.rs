//! Hot-swappable provider holder.
//!
//! Wraps the chat provider behind an async `RwLock` so the Settings →
//! AI handler can replace it at runtime. Every reader goes through
//! `chat()` which takes a read lock and clones the inner `Arc` —
//! fast (just a refcount bump) and never blocks the writer for
//! longer than the clone takes.

use std::sync::Arc;

use tokio::sync::RwLock;

use super::provider::LlmProvider;

#[derive(Default)]
struct Inner {
    chat: Option<Arc<dyn LlmProvider>>,
}

#[derive(Clone, Default)]
pub struct LlmHolder {
    inner: Arc<RwLock<Inner>>,
}

impl LlmHolder {
    pub fn new(chat: Option<Arc<dyn LlmProvider>>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { chat })),
        }
    }

    /// Snapshot the chat provider. None when AI is disabled or the
    /// configured provider failed health check.
    pub async fn chat(&self) -> Option<Arc<dyn LlmProvider>> {
        self.inner.read().await.chat.clone()
    }

    /// Atomically replace the provider. Called from the Settings →
    /// AI POST handler after the new config has been persisted and
    /// the provider built + healthchecked.
    pub async fn replace(&self, chat: Option<Arc<dyn LlmProvider>>) {
        let mut w = self.inner.write().await;
        w.chat = chat;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::llm::provider::{ChatRequest, ChatResponse, PrivacyPosture};
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
    }

    #[tokio::test]
    async fn empty_holder_returns_none() {
        let h = LlmHolder::default();
        assert!(h.chat().await.is_none());
    }

    #[tokio::test]
    async fn replace_swaps_providers() {
        let h = LlmHolder::new(Some(
            Arc::new(DummyProvider("first")) as Arc<dyn LlmProvider>
        ));
        assert_eq!(h.chat().await.unwrap().id(), "first");

        h.replace(Some(
            Arc::new(DummyProvider("second")) as Arc<dyn LlmProvider>
        ))
        .await;
        assert_eq!(h.chat().await.unwrap().id(), "second");
    }

    #[tokio::test]
    async fn replace_with_none_disables_provider() {
        let h = LlmHolder::new(Some(Arc::new(DummyProvider("on")) as Arc<dyn LlmProvider>));
        h.replace(None).await;
        assert!(h.chat().await.is_none());
    }
}
