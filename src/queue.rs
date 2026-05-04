use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct SessionQueue {
    chains: RwLock<HashMap<String, Arc<Mutex<()>>>>,
}

impl SessionQueue {
    pub fn new() -> Self {
        Self {
            chains: RwLock::new(HashMap::new()),
        }
    }

    pub async fn enqueue<F, Fut>(&self, key: &str, task: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let mutex = {
            let map = self.chains.read().await;
            if let Some(m) = map.get(key) {
                m.clone()
            } else {
                drop(map);
                let mut map = self.chains.write().await;
                map.entry(key.to_string())
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            }
        };

        let _guard = mutex.lock().await;
        task().await;
    }
}
