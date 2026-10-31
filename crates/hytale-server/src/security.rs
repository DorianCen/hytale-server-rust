//! Session abuse limits.
//!
//! The protocol decoder is bounds-checked, but a client can still spend server
//! CPU by flooding small frames, allocate large payloads, request an unbounded
//! view, or leave broadcasts queued behind a slow consumer. These limits are
//! deliberately conservative defaults; tests and unusual deployments can
//! override them through [`crate::SessionOptions`].

use std::time::{Duration, Instant};

/// Per-session resource and rate limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLimits {
    /// Maximum inbound frame payload accepted before decompression.
    pub max_inbound_packet_bytes: u32,
    /// Token-bucket burst for inbound client frames.
    pub inbound_packet_burst: u32,
    /// Sustained inbound client frames per second.
    pub inbound_packets_per_second: u32,
    /// Maximum view radius a client may request, in chunks.
    pub max_view_radius: u32,
    /// Maximum queued player/entity/block messages per session.
    pub max_queued_messages: usize,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            // Supported client packets are tiny. The registry's Java defaults
            // allow multi-megabyte editor packets, which this server rejects
            // before allocating them.
            max_inbound_packet_bytes: 4 * 1024 * 1024,
            inbound_packet_burst: 1024,
            inbound_packets_per_second: 512,
            max_view_radius: 16,
            max_queued_messages: 4096,
        }
    }
}

impl SessionLimits {
    /// Clamp a requested view radius to the configured ceiling.
    pub fn clamp_view_radius(&self, requested: u32) -> u32 {
        requested.min(self.max_view_radius)
    }

    /// Build an inbound frame token bucket from these limits.
    pub fn inbound_bucket(&self) -> TokenBucket {
        TokenBucket::new(self.inbound_packet_burst, self.inbound_packets_per_second)
    }
}

/// A monotonic token bucket.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_second: f64,
    last: Instant,
}

impl TokenBucket {
    /// Create a full bucket. Both values are clamped to at least one so a
    /// misconfiguration cannot lock every client out.
    pub fn new(burst: u32, per_second: u32) -> Self {
        Self::new_at(burst, per_second, Instant::now())
    }

    /// Create a full bucket with an explicit starting instant.
    pub fn new_at(burst: u32, per_second: u32, now: Instant) -> Self {
        let capacity = f64::from(burst.max(1));
        Self {
            capacity,
            tokens: capacity,
            refill_per_second: f64::from(per_second.max(1)),
            last: now,
        }
    }

    /// Take one token for an event at `now`.
    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.capacity);
            self.last = now;
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Time until the next token, for logging and tests.
    pub fn retry_after(&self) -> Duration {
        let missing = (1.0 - self.tokens).max(0.0);
        Duration::from_secs_f64(missing / self.refill_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucket_allows_the_burst_then_refills() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(2, 1, start);
        assert!(bucket.allow(start));
        assert!(bucket.allow(start));
        assert!(!bucket.allow(start));
        assert!(bucket.retry_after() > Duration::ZERO);

        assert!(bucket.allow(start + Duration::from_secs(1)));
        assert!(!bucket.allow(start + Duration::from_secs(1)));
        assert!(bucket.allow(start + Duration::from_secs(3)));
    }

    #[test]
    fn the_bucket_caps_refill_and_tolerates_zero_configuration() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(3, 100, start);
        assert!(bucket.allow(start));
        assert!(bucket.allow(start + Duration::from_secs(10)));
        assert!(bucket.allow(start + Duration::from_secs(10)));
        assert!(bucket.allow(start + Duration::from_secs(10)));
        assert!(!bucket.allow(start + Duration::from_secs(10)));

        let mut zero = TokenBucket::new_at(0, 0, start);
        assert!(zero.allow(start));
        assert!(!zero.allow(start));
    }

    #[test]
    fn the_defaults_bound_the_view_and_packet_ceiling() {
        let limits = SessionLimits::default();
        assert_eq!(limits.clamp_view_radius(0), 0);
        assert_eq!(limits.clamp_view_radius(7), 7);
        assert_eq!(limits.clamp_view_radius(1_000), 16);
        assert!(limits.max_inbound_packet_bytes <= 4 * 1024 * 1024);
        assert!(limits.max_queued_messages >= 1);
    }
}
