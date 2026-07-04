//! Shared token-bucket bandwidth limiter (Phase 5).
//!
//! A single bucket is shared by all active downloads so the *aggregate*
//! throughput is capped. `consume(n)` deducts `n` bytes (allowing the balance
//! to go negative) and sleeps proportionally to any debt, which smooths the
//! combined rate to `rate_bps` regardless of chunk size or concurrency.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TokenBucket {
    inner: Mutex<Inner>,
}

struct Inner {
    /// Bytes per second; `0.0` means unlimited.
    rate: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// `rate_bps` of 0 means unlimited.
    pub fn new(rate_bps: u64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                rate: rate_bps as f64,
                tokens: rate_bps as f64,
                last: Instant::now(),
            }),
        }
    }

    /// Reconfigure the rate live. `0` = unlimited.
    pub fn set_rate(&self, rate_bps: u64) {
        let mut i = self.inner.lock().unwrap();
        i.refill();
        i.rate = rate_bps as f64;
        // Don't carry a huge positive burst across a rate decrease.
        if i.rate > 0.0 && i.tokens > i.rate {
            i.tokens = i.rate;
        }
    }

    /// Wait until `n` bytes may be sent, then account for them.
    pub async fn consume(&self, n: u64) {
        let wait = {
            let mut i = self.inner.lock().unwrap();
            if i.rate <= 0.0 {
                return; // unlimited
            }
            i.refill();
            i.tokens -= n as f64;
            if i.tokens >= 0.0 {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(-i.tokens / i.rate)
            }
        };
        if wait > Duration::ZERO {
            tokio::time::sleep(wait).await;
        }
    }
}

impl Inner {
    fn refill(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        if self.rate > 0.0 {
            // Positive balance is capped at one second of burst.
            self.tokens = (self.tokens + dt * self.rate).min(self.rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn unlimited_returns_immediately() {
        let b = TokenBucket::new(0);
        let start = tokio::time::Instant::now();
        b.consume(10_000_000).await;
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn limited_rate_sleeps_for_debt() {
        // 1000 B/s. Consuming 3000 bytes should take ~3s of (virtual) time.
        let b = TokenBucket::new(1000);
        let start = tokio::time::Instant::now();
        b.consume(1000).await; // empties the initial bucket
        b.consume(3000).await; // 3000 debt -> ~3s
        assert!(start.elapsed() >= Duration::from_secs(3));
    }
}
