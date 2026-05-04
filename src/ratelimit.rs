use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

enum LimiterKind {
    Bucket(TokenBucket),
    Unlimited,
}

struct TokenBucket {
    tokens: f64,
    max: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    #[allow(clippy::cast_precision_loss)]
    fn new(max: u64, refill_per_sec: f64) -> Self {
        Self {
            tokens: max as f64,
            max: max as f64,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.max);
            self.last_refill = now;
        }
    }

    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn wait_duration(&self) -> std::time::Duration {
        if self.tokens >= 1.0 {
            std::time::Duration::ZERO
        } else {
            let needed = 1.0 - self.tokens;
            let secs = needed / self.refill_per_sec;
            std::time::Duration::from_secs_f64(secs)
        }
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<LimiterKind>>,
}

impl RateLimiter {
    #[allow(clippy::cast_precision_loss)]
    pub fn new(max_rpm: u64) -> Self {
        let refill_per_sec = max_rpm as f64 / 60.0;
        Self {
            inner: Arc::new(Mutex::new(LimiterKind::Bucket(TokenBucket::new(
                max_rpm,
                refill_per_sec,
            )))),
        }
    }

    pub fn unlimited() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LimiterKind::Unlimited)),
        }
    }

    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut inner = self.inner.lock().await;
                match &mut *inner {
                    LimiterKind::Bucket(bucket) => {
                        if bucket.try_acquire() {
                            return;
                        }
                        bucket.wait_duration()
                    }
                    LimiterKind::Unlimited => return,
                }
            };
            if wait.is_zero() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            } else {
                tokio::time::sleep(wait).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_starts_full() {
        let mut bucket = TokenBucket::new(10, 1.0);
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_bucket_depletes() {
        let mut bucket = TokenBucket::new(2, 1.0);
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
        assert!(!bucket.try_acquire());
    }

    #[test]
    fn test_bucket_refills() {
        let mut bucket = TokenBucket::new(10, 100.0);
        for _ in 0..10 {
            assert!(bucket.try_acquire());
        }
        assert!(!bucket.try_acquire());
        bucket.last_refill = Instant::now() - std::time::Duration::from_millis(100);
        bucket.refill();
        assert!(bucket.try_acquire());
    }

    #[test]
    fn test_unlimited_always_acquires() {
        let limiter = RateLimiter::unlimited();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            limiter.acquire().await;
            limiter.acquire().await;
            limiter.acquire().await;
        });
    }
}