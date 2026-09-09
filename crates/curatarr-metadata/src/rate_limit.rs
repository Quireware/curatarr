use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket. `per_minute` tokens refill linearly over 60 seconds.
pub struct TokenBucket {
    capacity: f64,
    tokens: Mutex<f64>,
    refill_per_sec: f64,
    last: Mutex<Instant>,
}

impl TokenBucket {
    pub fn new(per_minute: u32) -> Self {
        let capacity = f64::from(per_minute.max(1));
        Self {
            capacity,
            tokens: Mutex::new(capacity),
            refill_per_sec: capacity / 60.0,
            last: Mutex::new(Instant::now()),
        }
    }

    pub async fn acquire(&self) {
        loop {
            if self.try_take() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn try_take(&self) -> bool {
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(*last).as_secs_f64();
        *tokens = (*tokens + elapsed * self.refill_per_sec).min(self.capacity);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[tokio::test]
    async fn first_acquire_on_fresh_bucket_is_immediate() {
        let bucket = TokenBucket::new(60);
        bucket.acquire().await;
    }

    proptest! {
        #[test]
        fn constructor_never_panics(per_minute in 0u32..10_000) {
            let _ = TokenBucket::new(per_minute);
        }
    }
}
