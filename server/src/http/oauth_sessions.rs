//! In-memory OAuth state-token registry — used by the GDrive
//! authorize/callback flow to defend against CSRF.
//!
//! Keyed by a random `state` token sent to Google's authorize
//! endpoint; the value is the human label the operator typed for the
//! destination, plus a `created_at` so expired entries can be swept.
//!
//! Sweep is best-effort on every write rather than a separate
//! background task — at v1 traffic levels (one-shot OAuth dances
//! during destination setup) that's enough. If other OAuth flows
//! land later, promote the cleanup to a periodic tick.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::gdrive::PendingOauth;

#[derive(Clone, Default)]
pub struct OauthSessions {
    inner: Arc<Mutex<std::collections::HashMap<String, PendingOauth>>>,
}

impl OauthSessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn put(&self, state: String, pending: PendingOauth) {
        let mut g = self.inner.lock().await;
        // Drop entries older than an hour while we're holding the
        // lock anyway. Keeps the map bounded without a dedicated
        // sweeper task.
        let cutoff = chrono::Utc::now().timestamp() - 3600;
        g.retain(|_, v| v.created_at >= cutoff);
        g.insert(state, pending);
    }

    pub async fn take(&self, state: &str) -> Option<PendingOauth> {
        self.inner.lock().await.remove(state)
    }
}
