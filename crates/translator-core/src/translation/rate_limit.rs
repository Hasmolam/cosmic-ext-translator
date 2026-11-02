use std::time::{Duration, Instant};

/// Manages exponential backoff for translation requests encountering HTTP 429 rate limits.
#[derive(Debug, Clone)]
pub struct RateLimitManager {
    consecutive_errors: u32,
    pause_until: Option<Instant>,
    base_pause: Duration,
    max_exponent: u32,
}

impl Default for RateLimitManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(30), 6)
    }
}

impl RateLimitManager {
    /// Creates a new `RateLimitManager` with a custom base pause duration and maximum exponent.
    pub fn new(base_pause: Duration, max_exponent: u32) -> Self {
        Self {
            consecutive_errors: 0,
            pause_until: None,
            base_pause,
            max_exponent,
        }
    }

    /// Records an HTTP 429 rate limit encounter.
    ///
    /// Computes exponential pause: `base_pause * 2^(min(consecutive_errors, max_exponent))`.
    /// Sets the pause deadline and increments the consecutive error counter.
    /// Returns the calculated pause duration.
    pub fn record_rate_limit(&mut self) -> Duration {
        let shift = self.consecutive_errors.min(self.max_exponent);
        let multiplier = 1u64 << shift;
        let pause_duration = self.base_pause.saturating_mul(multiplier as u32);

        self.pause_until = Some(Instant::now() + pause_duration);
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        pause_duration
    }

    /// Returns the remaining pause duration if currently throttled.
    pub fn remaining_pause(&self) -> Option<Duration> {
        if let Some(until) = self.pause_until {
            let now = Instant::now();
            if now < until {
                return Some(until.duration_since(now));
            }
        }
        None
    }

    /// Checks if requests are currently throttled.
    pub fn is_throttled(&self) -> bool {
        self.remaining_pause().is_some()
    }

    /// Resets consecutive error streak and clears pause timer upon successful request.
    pub fn record_success(&mut self) {
        self.consecutive_errors = 0;
        self.pause_until = None;
    }

    /// Returns the number of consecutive rate limit errors recorded.
    pub fn consecutive_errors(&self) -> u32 {
        self.consecutive_errors
    }

    /// Returns the configured base pause duration.
    pub fn base_pause(&self) -> Duration {
        self.base_pause
    }

    /// Returns the configured maximum backoff exponent.
    pub fn max_exponent(&self) -> u32 {
        self.max_exponent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff_progression() {
        let mut manager = RateLimitManager::new(Duration::from_secs(10), 4);
        assert!(!manager.is_throttled());
        assert_eq!(manager.consecutive_errors(), 0);

        // 1st error: 10 * 2^0 = 10s
        let d1 = manager.record_rate_limit();
        assert_eq!(d1, Duration::from_secs(10));
        assert_eq!(manager.consecutive_errors(), 1);
        assert!(manager.is_throttled());

        // 2nd error: 10 * 2^1 = 20s
        let d2 = manager.record_rate_limit();
        assert_eq!(d2, Duration::from_secs(20));
        assert_eq!(manager.consecutive_errors(), 2);

        // 3rd error: 10 * 2^2 = 40s
        let d3 = manager.record_rate_limit();
        assert_eq!(d3, Duration::from_secs(40));

        // 4th error: 10 * 2^3 = 80s
        let d4 = manager.record_rate_limit();
        assert_eq!(d4, Duration::from_secs(80));

        // 5th error: 10 * 2^4 = 160s (capped exponent)
        let d5 = manager.record_rate_limit();
        assert_eq!(d5, Duration::from_secs(160));

        // 6th error: 10 * 2^4 = 160s (still capped)
        let d6 = manager.record_rate_limit();
        assert_eq!(d6, Duration::from_secs(160));

        // Recovery
        manager.record_success();
        assert!(!manager.is_throttled());
        assert_eq!(manager.consecutive_errors(), 0);
        assert!(manager.remaining_pause().is_none());
    }
}
