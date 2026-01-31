//! Concurrency control utilities
//!
//! Provides rate limiters and semaphores for controlling concurrent API calls
//! to prevent overwhelming external services.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, SemaphorePermit};
use tracing::{debug, trace};

/// Rate limiter for API calls with both concurrency and interval control
///
/// Combines a semaphore for concurrency limiting with a minimum interval
/// between calls to prevent bursting.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use glance_mind_agent_rs::concurrency::RateLimiter;
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = Arc::new(RateLimiter::new(20, 50)); // 20 concurrent, 50ms interval
///     
///     let permit = limiter.acquire().await;
///     // Make API call here
///     drop(permit); // Release when done
/// }
/// ```
pub struct RateLimiter {
    /// Semaphore to control max concurrent calls
    semaphore: Arc<Semaphore>,
    /// Minimum interval between calls in milliseconds
    min_interval_ms: u64,
    /// Timestamp of last call (for interval enforcement)
    last_call: Mutex<Instant>,
    /// Name for logging purposes
    name: String,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    ///
    /// * `concurrency` - Maximum number of concurrent calls allowed
    /// * `min_interval_ms` - Minimum milliseconds between calls (0 to disable)
    pub fn new(concurrency: usize, min_interval_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            min_interval_ms,
            last_call: Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_millis(min_interval_ms))
                    .unwrap_or_else(Instant::now),
            ),
            name: "RateLimiter".to_string(),
        }
    }

    /// Create a named rate limiter (for better logging)
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Get the current number of available permits
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Acquire a permit, waiting if necessary
    ///
    /// This method will:
    /// 1. Wait for a semaphore permit (if at concurrency limit)
    /// 2. Enforce minimum interval between calls (if configured)
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        // First, acquire the semaphore permit
        let permit = self.semaphore.acquire().await.expect("Semaphore closed");

        // Then, enforce minimum interval
        if self.min_interval_ms > 0 {
            let mut last = self.last_call.lock().await;
            let elapsed = last.elapsed().as_millis() as u64;

            if elapsed < self.min_interval_ms {
                let wait_ms = self.min_interval_ms - elapsed;
                trace!(
                    name = %self.name,
                    wait_ms = wait_ms,
                    "Rate limiter: waiting for interval"
                );
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }

            *last = Instant::now();
        }

        debug!(
            name = %self.name,
            available = self.semaphore.available_permits(),
            "Rate limiter: permit acquired"
        );

        permit
    }

    /// Try to acquire a permit without waiting
    ///
    /// Returns None if no permit is available immediately
    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>> {
        self.semaphore.try_acquire().ok()
    }

    /// Acquire an owned permit (can be moved across tasks)
    pub async fn acquire_owned(self: &Arc<Self>) -> OwnedSemaphorePermit {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore closed");

        // Enforce minimum interval
        if self.min_interval_ms > 0 {
            let mut last = self.last_call.lock().await;
            let elapsed = last.elapsed().as_millis() as u64;

            if elapsed < self.min_interval_ms {
                let wait_ms = self.min_interval_ms - elapsed;
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }

            *last = Instant::now();
        }

        permit
    }
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("name", &self.name)
            .field("available_permits", &self.semaphore.available_permits())
            .field("min_interval_ms", &self.min_interval_ms)
            .finish()
    }
}

/// AI-specific rate limiter type alias
pub type AiRateLimiter = RateLimiter;

/// TikHub-specific rate limiter type alias
pub type TikHubRateLimiter = RateLimiter;

impl RateLimiter {
    /// Create an AI rate limiter with default settings
    ///
    /// Default: 20 concurrent calls, 50ms minimum interval
    pub fn ai_default() -> Self {
        Self::new(20, 50).with_name("AI")
    }

    /// Create an AI rate limiter from configuration
    pub fn ai_from_config(concurrency: usize, min_interval_ms: u64) -> Self {
        Self::new(concurrency, min_interval_ms).with_name("AI")
    }

    /// Create a TikHub rate limiter with default settings
    ///
    /// Default: 3 concurrent calls, 500ms minimum interval
    pub fn tikhub_default() -> Self {
        Self::new(3, 500).with_name("TikHub")
    }

    /// Create a TikHub rate limiter from configuration
    pub fn tikhub_from_config(concurrency: usize, min_interval_ms: u64) -> Self {
        Self::new(concurrency, min_interval_ms).with_name("TikHub")
    }
}

/// Global rate limiters container
///
/// Provides thread-safe access to shared rate limiters
#[derive(Clone)]
pub struct GlobalRateLimiters {
    /// AI API rate limiter
    pub ai: Arc<RateLimiter>,
    /// TikHub API rate limiter
    pub tikhub: Arc<RateLimiter>,
}

impl GlobalRateLimiters {
    /// Create global rate limiters with default configuration
    pub fn new() -> Self {
        Self {
            ai: Arc::new(AiRateLimiter::ai_default()),
            tikhub: Arc::new(TikHubRateLimiter::tikhub_default()),
        }
    }

    /// Create global rate limiters from concurrency config
    pub fn from_config(config: &crate::domain::ConcurrencyConfig) -> Self {
        Self {
            ai: Arc::new(RateLimiter::ai_from_config(
                config.ai_concurrency,
                config.ai_min_interval_ms,
            )),
            tikhub: Arc::new(RateLimiter::tikhub_from_config(
                config.tikhub_concurrency,
                500, // Default TikHub interval
            )),
        }
    }
}

impl Default for GlobalRateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for GlobalRateLimiters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalRateLimiters")
            .field("ai", &self.ai)
            .field("tikhub", &self.tikhub)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_rate_limiter_concurrency() {
        let limiter = Arc::new(RateLimiter::new(2, 0));

        // Acquire 2 permits
        let p1 = limiter.acquire().await;
        let p2 = limiter.acquire().await;

        // Third should not be available
        assert!(limiter.try_acquire().is_none());

        // Release one
        drop(p1);

        // Now one should be available
        assert!(limiter.try_acquire().is_some());

        drop(p2);
    }

    #[tokio::test]
    async fn test_rate_limiter_interval() {
        let limiter = RateLimiter::new(10, 100); // 100ms interval

        let start = Instant::now();

        // First call should be immediate
        let _p1 = limiter.acquire().await;
        drop(_p1);

        // Second call should wait ~100ms
        let _p2 = limiter.acquire().await;

        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(90),
            "Elapsed: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_ai_rate_limiter() {
        let limiter = RateLimiter::ai_default();
        assert_eq!(limiter.semaphore.available_permits(), 20);
    }

    #[tokio::test]
    async fn test_global_rate_limiters() {
        let config = crate::domain::ConcurrencyConfig {
            ai_concurrency: 10,
            ai_min_interval_ms: 100,
            tikhub_concurrency: 5,
            ..Default::default()
        };

        let limiters = GlobalRateLimiters::from_config(&config);

        assert_eq!(limiters.ai.available_permits(), 10);
        assert_eq!(limiters.tikhub.available_permits(), 5);
    }
}
