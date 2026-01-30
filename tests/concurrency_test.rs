//! Concurrency and Rate Limiting Tests
//!
//! Tests to verify the rate limiting and parallel processing functionality
//! for the AI and TikHub API clients.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use glance_mind_agent_rs::concurrency::{RateLimiter, GlobalRateLimiters};
use glance_mind_agent_rs::domain::ConcurrencyConfig;

/// Test that rate limiter correctly limits concurrent access
#[tokio::test]
async fn test_rate_limiter_concurrency_limit() {
    let limiter = Arc::new(RateLimiter::new(3, 0)); // 3 concurrent, no interval
    let counter = Arc::new(AtomicU32::new(0));
    let max_concurrent = Arc::new(AtomicU32::new(0));
    
    let mut handles = vec![];
    
    // Spawn 10 tasks
    for _ in 0..10 {
        let limiter = limiter.clone();
        let counter = counter.clone();
        let max_concurrent = max_concurrent.clone();
        
        handles.push(tokio::spawn(async move {
            let _permit = limiter.acquire().await;
            
            // Increment counter
            let current = counter.fetch_add(1, Ordering::SeqCst) + 1;
            
            // Track max concurrent
            loop {
                let old_max = max_concurrent.load(Ordering::SeqCst);
                if current <= old_max {
                    break;
                }
                if max_concurrent.compare_exchange(
                    old_max, current, Ordering::SeqCst, Ordering::SeqCst
                ).is_ok() {
                    break;
                }
            }
            
            // Simulate work
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            // Decrement counter
            counter.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Max concurrent should be at most 3
    assert!(
        max_concurrent.load(Ordering::SeqCst) <= 3,
        "Max concurrent was {}, expected <= 3",
        max_concurrent.load(Ordering::SeqCst)
    );
}

/// Test that rate limiter enforces minimum interval between calls
#[tokio::test]
async fn test_rate_limiter_interval_enforcement() {
    let limiter = RateLimiter::new(10, 100); // 10 concurrent, 100ms interval
    
    let start = Instant::now();
    
    // Make 3 sequential calls
    let _p1 = limiter.acquire().await;
    drop(_p1);
    
    let _p2 = limiter.acquire().await;
    drop(_p2);
    
    let _p3 = limiter.acquire().await;
    drop(_p3);
    
    let elapsed = start.elapsed();
    
    // Should take at least 200ms (2 intervals after first call)
    assert!(
        elapsed >= Duration::from_millis(180), // Allow some margin
        "Elapsed {:?}, expected >= 180ms",
        elapsed
    );
}

/// Test that try_acquire doesn't block
#[tokio::test]
async fn test_rate_limiter_try_acquire() {
    let limiter = RateLimiter::new(1, 0);
    
    // First acquire should succeed
    let p1 = limiter.try_acquire();
    assert!(p1.is_some(), "First try_acquire should succeed");
    
    // Second should fail (no permits available)
    let p2 = limiter.try_acquire();
    assert!(p2.is_none(), "Second try_acquire should fail");
    
    // After releasing, should succeed again
    drop(p1);
    let p3 = limiter.try_acquire();
    assert!(p3.is_some(), "Third try_acquire should succeed after release");
}

/// Test AI rate limiter with default settings
#[tokio::test]
async fn test_ai_rate_limiter_defaults() {
    let limiter = RateLimiter::ai_default();
    
    // Should have 20 permits available
    assert_eq!(limiter.available_permits(), 20);
}

/// Test TikHub rate limiter with default settings
#[tokio::test]
async fn test_tikhub_rate_limiter_defaults() {
    let limiter = RateLimiter::tikhub_default();
    
    // Should have 3 permits available
    assert_eq!(limiter.available_permits(), 3);
}

/// Test GlobalRateLimiters creation from config
#[tokio::test]
async fn test_global_rate_limiters_from_config() {
    let config = ConcurrencyConfig {
        max_concurrent_tasks: 5,
        max_concurrent_videos: 5,
        ai_concurrency: 15,
        tikhub_concurrency: 5,
        ai_min_interval_ms: 100,
    };
    
    let limiters = GlobalRateLimiters::from_config(&config);
    
    assert_eq!(limiters.ai.available_permits(), 15);
    assert_eq!(limiters.tikhub.available_permits(), 5);
}

/// Test ConcurrencyConfig defaults
#[test]
fn test_concurrency_config_defaults() {
    let config = ConcurrencyConfig::default();
    
    assert_eq!(config.max_concurrent_tasks, 5);
    assert_eq!(config.max_concurrent_videos, 5);
    assert_eq!(config.ai_concurrency, 20);
    assert_eq!(config.tikhub_concurrency, 3);
    assert_eq!(config.ai_min_interval_ms, 50);
}

/// Test ConcurrencyConfig builder methods
#[test]
fn test_concurrency_config_builder() {
    let config = ConcurrencyConfig::default()
        .with_max_concurrent_tasks(10)
        .with_max_concurrent_videos(8)
        .with_ai_concurrency(30)
        .with_tikhub_concurrency(5)
        .with_ai_min_interval_ms(100);
    
    assert_eq!(config.max_concurrent_tasks, 10);
    assert_eq!(config.max_concurrent_videos, 8);
    assert_eq!(config.ai_concurrency, 30);
    assert_eq!(config.tikhub_concurrency, 5);
    assert_eq!(config.ai_min_interval_ms, 100);
}

/// Test parallel task simulation with rate limiting
#[tokio::test]
async fn test_parallel_tasks_with_rate_limiter() {
    use futures::stream::{self, StreamExt};
    
    let limiter = Arc::new(RateLimiter::new(3, 10)); // 3 concurrent, 10ms interval
    let completed = Arc::new(AtomicU32::new(0));
    
    let start = Instant::now();
    
    // Process 9 items with buffer_unordered(3)
    let items: Vec<i32> = (0..9).collect();
    
    let results: Vec<_> = stream::iter(items)
        .map(|item| {
            let limiter = limiter.clone();
            let completed = completed.clone();
            
            async move {
                let _permit = limiter.acquire().await;
                
                // Simulate work
                tokio::time::sleep(Duration::from_millis(20)).await;
                
                completed.fetch_add(1, Ordering::SeqCst);
                item * 2
            }
        })
        .buffer_unordered(3)
        .collect()
        .await;
    
    let elapsed = start.elapsed();
    
    // All items should be completed
    assert_eq!(completed.load(Ordering::SeqCst), 9);
    
    // Results should be doubled values (order may vary due to concurrency)
    assert_eq!(results.len(), 9);
    for result in &results {
        assert!(result % 2 == 0, "Result {} should be even", result);
    }
    
    // Should take roughly 3 batches * 20ms work + some overhead
    // With rate limiting intervals, expect around 90-200ms
    assert!(
        elapsed >= Duration::from_millis(60),
        "Elapsed {:?}, expected >= 60ms",
        elapsed
    );
    
    println!("Parallel tasks completed in {:?}", elapsed);
}

/// Test that rate limiter handles panic in task gracefully
#[tokio::test]
async fn test_rate_limiter_panic_recovery() {
    let limiter = Arc::new(RateLimiter::new(2, 0));
    
    // Spawn a task that panics
    let limiter_clone = limiter.clone();
    let handle = tokio::spawn(async move {
        let _permit = limiter_clone.acquire().await;
        panic!("Intentional panic for testing");
    });
    
    // Wait for panic (catch it)
    let _ = handle.await;
    
    // Give some time for permit to be released
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Limiter should still be usable
    let p1 = limiter.try_acquire();
    assert!(p1.is_some(), "Limiter should be usable after panic");
}

/// Stress test with many concurrent tasks
#[tokio::test]
async fn stress_test_rate_limiter() {
    let limiter = Arc::new(RateLimiter::new(20, 5)); // 20 concurrent, 5ms interval
    let completed = Arc::new(AtomicU32::new(0));
    
    let mut handles = vec![];
    
    // Spawn 100 tasks
    for i in 0..100 {
        let limiter = limiter.clone();
        let completed = completed.clone();
        
        handles.push(tokio::spawn(async move {
            let _permit = limiter.acquire().await;
            
            // Very short work
            tokio::time::sleep(Duration::from_millis(1)).await;
            
            completed.fetch_add(1, Ordering::SeqCst);
            i
        }));
    }
    
    // Wait for all tasks
    for handle in handles {
        let _ = handle.await;
    }
    
    // All tasks should complete
    assert_eq!(
        completed.load(Ordering::SeqCst),
        100,
        "All 100 tasks should complete"
    );
}
