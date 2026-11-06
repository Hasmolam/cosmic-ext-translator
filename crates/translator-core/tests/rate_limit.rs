use std::sync::Arc;
use std::time::Duration;
use translator_core::translation::RateLimitManager;

// -----------------------------------------------------------------------------
// Rate limit exponential backoff math and pause duration
// -----------------------------------------------------------------------------
#[test]
fn test_rate_limit_exponential_backoff_progression_and_caps() {
    let mut rlm = RateLimitManager::default(); // 30s base, max exponent 6
    assert_eq!(rlm.base_pause(), Duration::from_secs(30));
    assert_eq!(rlm.max_exponent(), 6);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);

    // Consecutive errors: 30 * 2^k
    let expected_durations = [
        30,   // 30 * 2^0
        60,   // 30 * 2^1
        120,  // 30 * 2^2
        240,  // 30 * 2^3
        480,  // 30 * 2^4
        960,  // 30 * 2^5
        1920, // 30 * 2^6
        1920, // capped at 2^6
        1920, // capped at 2^6
    ];

    for (idx, &expected_secs) in expected_durations.iter().enumerate() {
        let pause = rlm.record_rate_limit();
        assert_eq!(
            pause,
            Duration::from_secs(expected_secs),
            "Failed at error index {}",
            idx
        );
        assert!(rlm.is_throttled());
        assert!(rlm.remaining_pause().is_some());
        assert!(rlm.remaining_pause().unwrap() <= Duration::from_secs(expected_secs));
    }

    assert_eq!(rlm.consecutive_errors(), 9);

    // Recovery upon successful translation
    rlm.record_success();
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);
    assert_eq!(rlm.remaining_pause(), None);
}

// -----------------------------------------------------------------------------
// RateLimitManager high-count stress and cap guarantees
// -----------------------------------------------------------------------------
#[test]
fn test_rate_limit_manager_stress_and_caps() {
    let mut rlm = RateLimitManager::new(Duration::from_secs(5), 4); // 5s * 2^4 = 80s cap

    // 100 consecutive errors should not overflow or exceed 80s
    for _ in 0..100 {
        let pause = rlm.record_rate_limit();
        assert!(pause <= Duration::from_secs(80));
    }
    assert_eq!(rlm.consecutive_errors(), 100);
    assert!(rlm.is_throttled());

    // Remaining pause must be <= 80s
    let rem = rlm.remaining_pause().unwrap();
    assert!(rem <= Duration::from_secs(80));

    // Recovery
    rlm.record_success();
    assert_eq!(rlm.consecutive_errors(), 0);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.remaining_pause(), None);
}

// -----------------------------------------------------------------------------
// Adversarial RateLimitManager exponential progression (k = 0..=20) and recovery
// -----------------------------------------------------------------------------
#[test]
fn test_adversarial_rate_limit_exponential_overflow_and_recovery() {
    let mut rlm = RateLimitManager::default(); // 30s base, max exponent 6
    assert_eq!(rlm.base_pause(), Duration::from_secs(30));
    assert_eq!(rlm.max_exponent(), 6);
    assert!(!rlm.is_throttled());
    assert_eq!(rlm.consecutive_errors(), 0);

    // Test consecutive errors k = 0, 1, 2, ..., 20
    for k in 0..=20u32 {
        let expected_exponent = k.min(6);
        let expected_secs = 30u64 * (1u64 << expected_exponent);
        let expected_duration = Duration::from_secs(expected_secs);

        let pause = rlm.record_rate_limit();

        // 1. Assert exact mathematical formula: 30s * 2^(min(k, 6))
        assert_eq!(
            pause, expected_duration,
            "Rate limit pause mismatch at k={k}: got {:?}, expected {:?}",
            pause, expected_duration
        );

        // 2. Assert capped at 1920s for all k >= 6
        if k >= 6 {
            assert_eq!(
                pause,
                Duration::from_secs(1920),
                "Pause must remain capped at 1920s for k >= 6 (k={k})"
            );
        }

        // 3. Assert consecutive errors tracking
        assert_eq!(rlm.consecutive_errors(), k + 1);

        // 4. Assert throttling status
        assert!(rlm.is_throttled());
        let remaining = rlm
            .remaining_pause()
            .expect("Expected active remaining pause");
        assert!(remaining <= expected_duration);
    }

    // Stress beyond k=20 up to k=100: ensure no integer overflow or unexpected multipliers
    for k in 21..=100u32 {
        let pause = rlm.record_rate_limit();
        assert_eq!(
            pause,
            Duration::from_secs(1920),
            "Cap at 1920s must hold without overflow up to k={k}"
        );
        assert_eq!(rlm.consecutive_errors(), k + 1);
        assert!(rlm.is_throttled());
    }

    // Recovery test: record_success must reset error count to 0 and clear pause
    rlm.record_success();
    assert!(
        !rlm.is_throttled(),
        "Must not be throttled after record_success"
    );
    assert_eq!(
        rlm.consecutive_errors(),
        0,
        "consecutive_errors must be 0 after record_success"
    );
    assert!(
        rlm.remaining_pause().is_none(),
        "remaining_pause must be None after record_success"
    );

    // Immediately trigger rate limit again -> must restart at base pause 30s (30 * 2^0)
    let fresh_pause = rlm.record_rate_limit();
    assert_eq!(fresh_pause, Duration::from_secs(30));
    assert_eq!(rlm.consecutive_errors(), 1);
    assert!(rlm.is_throttled());
}

// -----------------------------------------------------------------------------
// Concurrent race condition and recovery stress on RateLimitManager
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_rate_limit_concurrent_stress_and_recovery() {
    let rlm = Arc::new(tokio::sync::Mutex::new(RateLimitManager::default()));
    let num_tasks = 20;
    let mut handles = Vec::new();

    // 20 concurrent tasks recording rate limits
    for _ in 0..num_tasks {
        let rlm_clone = Arc::clone(&rlm);
        handles.push(tokio::spawn(async move {
            let mut mgr = rlm_clone.lock().await;
            mgr.record_rate_limit()
        }));
    }

    for h in handles {
        let pause = h.await.unwrap();
        assert!(pause >= Duration::from_secs(30));
        assert!(pause <= Duration::from_secs(1920));
    }

    {
        let mgr = rlm.lock().await;
        assert_eq!(mgr.consecutive_errors(), num_tasks);
        assert!(mgr.is_throttled());
    }

    // One thread records success
    {
        let mut mgr = rlm.lock().await;
        mgr.record_success();
        assert_eq!(mgr.consecutive_errors(), 0);
        assert!(!mgr.is_throttled());
        assert!(mgr.remaining_pause().is_none());
    }

    // Subsequent call starts fresh
    {
        let mut mgr = rlm.lock().await;
        let p = mgr.record_rate_limit();
        assert_eq!(p, Duration::from_secs(30));
        assert_eq!(mgr.consecutive_errors(), 1);
    }
}

// -----------------------------------------------------------------------------
// RateLimitManager edge-case configurations & truncation behavior
// -----------------------------------------------------------------------------
#[test]
fn test_adversarial_rate_limit_edge_configurations() {
    // 1. max_exponent = 0: pause is always base_pause
    let mut rlm_zero_exp = RateLimitManager::new(Duration::from_millis(500), 0);
    for _ in 0..10 {
        assert_eq!(rlm_zero_exp.record_rate_limit(), Duration::from_millis(500));
    }

    // 2. Millisecond base pause with exponential scaling
    let mut rlm_ms = RateLimitManager::new(Duration::from_millis(100), 3);
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(100)); // 100 * 2^0
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(200)); // 100 * 2^1
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(400)); // 100 * 2^2
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(800)); // 100 * 2^3
    assert_eq!(rlm_ms.record_rate_limit(), Duration::from_millis(800)); // capped

    // 3. Documented architectural vulnerability observation:
    // If max_exponent >= 32, (1u64 << 32 as u32) truncates to 0!
    let mut rlm_32 = RateLimitManager::new(Duration::from_secs(1), 32);
    for _ in 0..32 {
        rlm_32.record_rate_limit();
    }
    let truncated_pause = rlm_32.record_rate_limit();
    // Multiplier truncates to 0, producing Duration::ZERO
    assert_eq!(
        truncated_pause,
        Duration::ZERO,
        "Confirmed u32 truncation to 0 when exponent >= 32"
    );

    // 4. In debug mode, max_exponent >= 64 triggers shift overflow panic on 1u64 << 64
    let panic_result = std::panic::catch_unwind(|| {
        let mut rlm_64 = RateLimitManager::new(Duration::from_secs(1), 64);
        for _ in 0..64 {
            rlm_64.record_rate_limit();
        }
        rlm_64.record_rate_limit();
    });
    assert!(
        panic_result.is_err(),
        "Confirmed shift overflow panic when max_exponent >= 64"
    );
}
