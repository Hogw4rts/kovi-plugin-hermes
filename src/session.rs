use crate::routing::Role;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const MAX_SESSIONS: usize = 500;
const SESSION_TTL_SECS: u64 = 86400;
const FLUSH_INTERVAL_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub(crate) struct ConversationMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct SessionState {
    #[serde(default)]
    pub session_versions: HashMap<String, u64>,
    #[serde(default)]
    pub selected_model: String,
    #[serde(default)]
    pub conversations: HashMap<String, Vec<ConversationMessage>>,
    #[serde(default)]
    pub last_active: HashMap<String, u64>,
}

#[derive(Clone)]
pub(crate) struct SessionStore {
    state: Arc<RwLock<SessionState>>,
    path: PathBuf,
    dirty: Arc<AtomicBool>,
}

impl SessionStore {
    pub(crate) async fn new(data_dir: &Path) -> Self {
        let path = data_dir.join("session-state.json");
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                kovi::log::warn!(
                    "hermes: corrupt session state at {}, resetting: {}",
                    path.display(),
                    e
                );
                SessionState::default()
            }),
            Err(e) => {
                kovi::log::warn!(
                    "hermes: failed to read session state from {}: {}, starting fresh",
                    path.display(),
                    e
                );
                SessionState::default()
            }
        };
        let store = Self {
            state: Arc::new(RwLock::new(state)),
            path,
            dirty: Arc::new(AtomicBool::new(false)),
        };
        store.spawn_flush_task();
        store
    }

    fn spawn_flush_task(&self) {
        let state = self.state.clone();
        let path = self.path.clone();
        let dirty = self.dirty.clone();

        kovi::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(FLUSH_INTERVAL_SECS),
            );
            loop {
                interval.tick().await;
                if dirty.load(Ordering::Relaxed) {
                    let raw = {
                        let s = state.read().await;
                        match serde_json::to_string_pretty(&*s) {
                            Ok(r) => r,
                            Err(e) => {
                                kovi::log::warn!(
                                    "hermes: failed to serialize session state: {e}"
                                );
                                continue;
                            }
                        }
                    };
                    if let Err(e) = tokio::fs::write(&path, &raw).await {
                        kovi::log::warn!("hermes: failed to write session state: {e}");
                    } else {
                        dirty.compare_exchange(
                            true,
                            false,
                            std::sync::atomic::Ordering::AcqRel,
                            std::sync::atomic::Ordering::Acquire,
                        ).ok();
                    }
                }
            }
        });
    }

    pub(crate) async fn flush(&self) {
        self.write_to_disk().await;
        self.dirty.compare_exchange(
            true,
            false,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        ).ok();
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn session_id(&self, base_key: &str) -> String {
        let state = self.state.read().await;
        let version = state.session_versions.get(base_key).copied().unwrap_or(0);
        format!("{base_key}:v{version}")
    }

    pub(crate) async fn bump_session(&self, base_key: &str) -> String {
        let new_id;
        {
            let mut state = self.state.write().await;
            let next = state.session_versions.get(base_key).copied().unwrap_or(0) + 1;
            let old_id = format!("{}:v{}", base_key, next - 1);
            state.session_versions.insert(base_key.to_string(), next);
            state.conversations.remove(&old_id);
            state.last_active.remove(&old_id);
            new_id = format!("{base_key}:v{next}");
        }
        self.flush().await;
        new_id
    }

    pub(crate) async fn selected_model(&self) -> String {
        let state = self.state.read().await;
        state.selected_model.clone()
    }

    pub(crate) async fn set_selected_model(&self, model: &str) {
        {
            let mut state = self.state.write().await;
            state.selected_model = model.to_string();
        }
        self.mark_dirty();
    }

    pub(crate) async fn clear_selected_model(&self) {
        {
            let mut state = self.state.write().await;
            state.selected_model.clear();
        }
        self.mark_dirty();
    }

    pub(crate) async fn get_conversation(&self, session_id: &str) -> Vec<ConversationMessage> {
        let state = self.state.read().await;
        state
            .conversations
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) async fn append_conversation(
        &self,
        session_id: &str,
        messages: &[ConversationMessage],
        max_messages: usize,
    ) {
        {
            let mut state = self.state.write().await;
            let existing = state
                .conversations
                .entry(session_id.to_string())
                .or_default();
            existing.extend(
                messages
                    .iter()
                    .filter(|m| !m.content.is_empty())
                    .cloned(),
            );
            if max_messages > 0 && existing.len() > max_messages {
                let drain_count = existing.len() - max_messages;
                existing.drain(..drain_count);
            }

            let now = now_secs();
            state.last_active.insert(session_id.to_string(), now);

            if state.conversations.len() > MAX_SESSIONS {
                Self::evict_stale(&mut state);
            }
        }
        self.mark_dirty();
    }

    fn evict_stale(state: &mut SessionState) {
        let now = now_secs();
        let cutoff = now.saturating_sub(SESSION_TTL_SECS);

        let stale_keys: Vec<String> = state
            .last_active
            .iter()
            .filter(|&(_, ts)| *ts < cutoff)
            .map(|(k, _)| k.clone())
            .collect();

        for key in &stale_keys {
            state.conversations.remove(key);
            state.last_active.remove(key);
        }

        if state.conversations.len() > MAX_SESSIONS {
            let mut entries: Vec<(String, u64)> = state
                .last_active
                .iter()
                .map(|(k, &ts)| (k.clone(), ts))
                .collect();
            entries.sort_unstable_by_key(|(_, ts)| *ts);
            let to_remove = state.conversations.len() - MAX_SESSIONS;
            for (key, _) in entries.iter().take(to_remove) {
                state.conversations.remove(key);
                state.last_active.remove(key);
            }
        }

        let active_base_keys: std::collections::HashSet<String> = state
            .last_active
            .keys()
            .filter_map(|sid| sid.rsplit_once(":v").map(|(base, _)| base.to_string()))
            .collect();

        state
            .session_versions
            .retain(|base_key, _| active_base_keys.contains(base_key));
    }

    async fn write_to_disk(&self) {
        let raw = {
            let state = self.state.read().await;
            match serde_json::to_string_pretty(&*state) {
                Ok(r) => r,
                Err(e) => {
                    kovi::log::warn!("hermes: failed to serialize session state: {e}");
                    return;
                }
            }
        };
        if let Err(e) = tokio::fs::write(&self.path, raw).await {
            kovi::log::warn!("hermes: failed to write session state: {e}");
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}