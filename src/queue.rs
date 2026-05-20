use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

const MAX_CHAINS: usize = 2000;
const EVICT_THRESHOLD: usize = 1600;
const IDLE_SECS: u64 = 300;

struct ChainEntry {
    mutex: Arc<Mutex<()>>,
    last_used: Instant,
}

pub(crate) struct SessionQueue {
    chains: RwLock<HashMap<String, ChainEntry>>,
}

impl SessionQueue {
    pub(crate) fn new() -> Self {
        Self {
            chains: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) async fn enqueue<F, Fut>(&self, key: &str, task: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let mutex = {
            let mut map = self.chains.write().await;
            let entry = map
                .entry(key.to_string())
                .or_insert_with(|| ChainEntry {
                    mutex: Arc::new(Mutex::new(())),
                    last_used: Instant::now(),
                });
            entry.last_used = Instant::now();
            entry.mutex.clone()
        };

        let _guard = mutex.lock().await;
        task().await;

        if let Ok(mut map) = self.chains.try_write() {
            if let Some(entry) = map.get_mut(key) {
                entry.last_used = Instant::now();
            }
            if map.len() > MAX_CHAINS {
                Self::evict(&mut map);
            }
        }
    }

    fn evict(map: &mut HashMap<String, ChainEntry>) {
        let now = Instant::now();
        map.retain(|_, entry| now.duration_since(entry.last_used).as_secs() < IDLE_SECS);

        if map.len() > EVICT_THRESHOLD {
            let mut entries: Vec<(String, Instant)> = map
                .iter()
                .map(|(k, e)| (k.clone(), e.last_used))
                .collect();
            entries.sort_unstable_by_key(|(_, ts)| *ts);
            let to_remove = map.len() - EVICT_THRESHOLD;
            for (k, _) in entries.iter().take(to_remove) {
                map.remove(k);
            }
        }
    }
}