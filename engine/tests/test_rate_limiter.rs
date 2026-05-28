use engine::infrastructure::rate_limiter::*;

#[test]
fn test_rate_limiter_allows_within_limit() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    let result = limiter.check("t1", "k1", Some(5), 1000.0);
    assert!(result.allowed);
    assert_eq!(result.remaining, 4);
    assert_eq!(result.limit, 5);
}

#[test]
fn test_rate_limiter_blocks_at_limit() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    for i in 0..5 {
        let r = limiter.check("t1", "k1", Some(5), 1000.0 + i as f64);
        assert!(r.allowed);
    }
    let result = limiter.check("t1", "k1", Some(5), 1005.0);
    assert!(!result.allowed);
    assert_eq!(result.remaining, 0);
    assert!(result.retry_after.is_some());
}

#[test]
fn test_rate_limiter_unlimited_when_no_limit() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    let result = limiter.check("t1", "k1", None, 1000.0);
    assert!(result.allowed);
    assert_eq!(result.limit, -1);
}

#[test]
fn test_rate_limiter_unlimited_when_zero_limit() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    let result = limiter.check("t1", "k1", Some(0), 1000.0);
    assert!(result.allowed);
}

#[test]
fn test_rate_limiter_window_expiry() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    for i in 0..5 {
        limiter.check("t1", "k1", Some(5), 1000.0 + i as f64);
    }
    // After window expires, requests should be allowed again
    let result = limiter.check("t1", "k1", Some(5), 1061.0);
    assert!(result.allowed);
}

#[test]
fn test_rate_limiter_separate_keys() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    for i in 0..3 {
        limiter.check("t1", "k1", Some(3), 1000.0 + i as f64);
    }
    // k1 is at limit, k2 should still work
    let result = limiter.check("t1", "k2", Some(3), 1003.0);
    assert!(result.allowed);
}

#[test]
fn test_rate_limiter_separate_tenants() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    for i in 0..3 {
        limiter.check("t1", "k1", Some(3), 1000.0 + i as f64);
    }
    let result = limiter.check("t2", "k1", Some(3), 1003.0);
    assert!(result.allowed);
}

#[test]
fn test_rate_limiter_cleanup() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    limiter.check("t1", "k1", Some(5), 1000.0);
    limiter.check("t1", "k2", Some(5), 1000.0);
    assert_eq!(limiter.bucket_count(), 2);
    let removed = limiter.cleanup(1061.0);
    assert_eq!(removed, 2);
    assert_eq!(limiter.bucket_count(), 0);
}

#[test]
fn test_rate_limiter_retry_after() {
    let mut limiter = RateLimiter::new(60.0, 1000);
    for i in 0..3 {
        limiter.check("t1", "k1", Some(3), 1000.0 + i as f64);
    }
    let result = limiter.check("t1", "k1", Some(3), 1010.0);
    assert!(!result.allowed);
    let retry = result.retry_after.unwrap();
    assert!(retry > 0.0);
    assert!(retry <= 60.0);
}

#[test]
fn test_rate_limiter_result_serde() {
    let result = RateLimitResult {
        allowed: true,
        remaining: 5,
        limit: 10,
        retry_after: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    let r: RateLimitResult = serde_json::from_str(&json).unwrap();
    assert!(r.allowed);
    assert_eq!(r.remaining, 5);
}
