use crate::routing::UserId;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const COOLDOWN_SECS: i64 = 10;
const MAX_ENTRIES: usize = 10000;
const EVICT_THRESHOLD: usize = 8000;

pub struct NotificationGuard {
    cooldowns: RwLock<HashMap<i64, i64>>,
}

impl NotificationGuard {
    pub fn new() -> Self {
        Self {
            cooldowns: RwLock::new(HashMap::new()),
        }
    }

    pub async fn should_notify(&self, user_id: UserId) -> bool {
        let now = now_timestamp();

        let map = self.cooldowns.read().await;
        if let Some(&last) = map.get(&user_id.0)
            && now - last < COOLDOWN_SECS
        {
            return false;
        }
        drop(map);

        let mut map = self.cooldowns.write().await;
        if let Some(&last) = map.get(&user_id.0)
            && now - last < COOLDOWN_SECS
        {
            return false;
        }
        map.insert(user_id.0, now);

        if map.len() > MAX_ENTRIES {
            Self::evict(&mut map, now);
        }

        true
    }

    fn evict(map: &mut HashMap<i64, i64>, now: i64) {
        let cutoff = now - COOLDOWN_SECS * 2;
        map.retain(|_, &mut ts| ts > cutoff);

        if map.len() > EVICT_THRESHOLD {
            let mut entries: Vec<(i64, i64)> = map.iter().map(|(&k, &v)| (k, v)).collect();
            entries.sort_unstable_by_key(|(_, ts)| *ts);
            let to_remove = map.len() - EVICT_THRESHOLD;
            for (k, _) in entries.iter().take(to_remove) {
                map.remove(k);
            }
        }
    }
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().try_into().unwrap_or(i64::MAX))
}