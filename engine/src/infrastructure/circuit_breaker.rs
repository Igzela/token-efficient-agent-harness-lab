use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(v: u8) -> Self {
        match v {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

/// Error returned by the circuit breaker.
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// The circuit is open; the inner call was not made.
    CircuitOpen,
    /// The inner call was made but returned an error.
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::CircuitOpen => write!(f, "circuit breaker is open"),
            CircuitBreakerError::Inner(e) => write!(f, "inner error: {}", e),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for CircuitBreakerError<E> {}

/// Snapshot of circuit breaker state for status reporting.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerSnapshot {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: u64,
    pub success_count: u64,
    pub total_calls: u64,
    pub failure_threshold: u64,
    pub recovery_timeout_ms: u64,
    pub last_failure_at: Option<String>,
    pub consecutive_successes_in_half_open: u64,
}

/// A circuit breaker that wraps a callable and tracks failures.
///
/// States:
/// - **Closed**: Normal operation. Calls pass through. Failures are counted.
///   Transitions to Open when `failure_threshold` is reached.
/// - **Open**: Calls are rejected immediately. Transitions to HalfOpen after
///   `recovery_timeout_ms` elapses.
/// - **HalfOpen**: A single probe call is allowed. If it succeeds, transitions
///   to Closed. If it fails, transitions back to Open.
pub struct CircuitBreaker {
    name: String,
    state: AtomicU8,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    total_calls: AtomicU64,
    failure_threshold: u64,
    recovery_timeout_ms: u64,
    last_failure_at: Mutex<Option<Instant>>,
    consecutive_successes_in_half_open: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, failure_threshold: u64, recovery_timeout_ms: u64) -> Self {
        Self {
            name: name.into(),
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            total_calls: AtomicU64::new(0),
            failure_threshold,
            recovery_timeout_ms,
            last_failure_at: Mutex::new(None),
            consecutive_successes_in_half_open: AtomicU64::new(0),
        }
    }

    pub fn state(&self) -> CircuitState {
        CircuitState::from(self.state.load(Ordering::SeqCst))
    }

    pub fn snapshot(&self) -> CircuitBreakerSnapshot {
        let last_failure = self.last_failure_at.lock().unwrap().map(|t| {
            let elapsed = t.elapsed();
            format!("{:.1}s ago", elapsed.as_secs_f64())
        });
        CircuitBreakerSnapshot {
            name: self.name.clone(),
            state: self.state(),
            failure_count: self.failure_count.load(Ordering::SeqCst),
            success_count: self.success_count.load(Ordering::SeqCst),
            total_calls: self.total_calls.load(Ordering::SeqCst),
            failure_threshold: self.failure_threshold,
            recovery_timeout_ms: self.recovery_timeout_ms,
            last_failure_at: last_failure,
            consecutive_successes_in_half_open: self
                .consecutive_successes_in_half_open
                .load(Ordering::SeqCst),
        }
    }

    /// Execute a closure through the circuit breaker.
    ///
    /// Returns `CircuitBreakerError::CircuitOpen` if the circuit is open and
    /// the recovery timeout has not elapsed. Otherwise, executes the closure
    /// and tracks the result.
    pub fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let current = self.state();

        match current {
            CircuitState::Open => {
                if self.should_try_reset() {
                    self.transition_to(CircuitState::HalfOpen);
                } else {
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
            CircuitState::HalfOpen => {
                // Allow the probe call through.
            }
            CircuitState::Closed => {
                // Normal operation.
            }
        }

        self.total_calls.fetch_add(1, Ordering::SeqCst);

        match f() {
            Ok(v) => {
                self.record_success();
                Ok(v)
            }
            Err(e) => {
                self.record_failure();
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }

    pub fn record_call(&self) {
        self.total_calls.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);
        let prev_failures = self.failure_count.swap(0, Ordering::SeqCst);

        match self.state() {
            CircuitState::HalfOpen => {
                let consec = self
                    .consecutive_successes_in_half_open
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                // Require 2 consecutive successes to close the circuit.
                if consec >= 2 {
                    self.transition_to(CircuitState::Closed);
                    self.consecutive_successes_in_half_open
                        .store(0, Ordering::SeqCst);
                }
            }
            CircuitState::Closed => {
                // Already closed, just reset failure count (already done via swap above).
                let _ = prev_failures;
            }
            CircuitState::Open => {
                // Shouldn't happen (calls are rejected in Open), but handle gracefully.
            }
        }
    }

    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.success_count.store(0, Ordering::SeqCst);

        {
            let mut last = self.last_failure_at.lock().unwrap();
            *last = Some(Instant::now());
        }

        match self.state() {
            CircuitState::Closed => {
                if count >= self.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open reopens the circuit.
                self.transition_to(CircuitState::Open);
                self.consecutive_successes_in_half_open
                    .store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {
                // Already open.
            }
        }
    }

    pub fn should_try_reset(&self) -> bool {
        let last = self.last_failure_at.lock().unwrap();
        match *last {
            Some(t) => {
                let elapsed_ms = t.elapsed().as_millis() as u64;
                elapsed_ms >= self.recovery_timeout_ms
            }
            None => true,
        }
    }

    pub fn transition_to(&self, new_state: CircuitState) {
        self.state.store(new_state as u8, Ordering::SeqCst);
    }

    /// Manually reset the circuit breaker to Closed state.
    pub fn reset(&self) {
        self.transition_to(CircuitState::Closed);
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.consecutive_successes_in_half_open
            .store(0, Ordering::SeqCst);
        {
            let mut last = self.last_failure_at.lock().unwrap();
            *last = None;
        }
    }
}

/// Registry of circuit breakers for status reporting.
pub struct CircuitBreakerRegistry {
    breakers: std::sync::RwLock<Vec<std::sync::Arc<CircuitBreaker>>>,
}

impl CircuitBreakerRegistry {
    pub fn new() -> Self {
        Self {
            breakers: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn register(&self, breaker: std::sync::Arc<CircuitBreaker>) {
        self.breakers.write().unwrap().push(breaker);
    }

    pub fn snapshots(&self) -> Vec<CircuitBreakerSnapshot> {
        self.breakers
            .read()
            .unwrap()
            .iter()
            .map(|b| b.snapshot())
            .collect()
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn circuit_starts_closed() {
        let cb = CircuitBreaker::new("test", 3, 1000);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_stays_closed_on_success() {
        let cb = CircuitBreaker::new("test", 3, 1000);
        let result = cb.call(|| Ok::<i32, String>(42));
        assert_eq!(result.unwrap(), 42);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_opens_after_threshold_failures() {
        let cb = CircuitBreaker::new("test", 3, 1000);
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail1".into()));
        assert_eq!(cb.state(), CircuitState::Closed);

        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail2".into()));
        assert_eq!(cb.state(), CircuitState::Closed);

        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail3".into()));
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn circuit_rejects_when_open() {
        let cb = Arc::new(CircuitBreaker::new("test", 1, 60_000));
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.state(), CircuitState::Open);

        let result = cb.call(|| Ok::<i32, String>(42));
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
    }

    #[test]
    fn circuit_transitions_to_half_open_after_timeout() {
        let cb = Arc::new(CircuitBreaker::new("test", 1, 1)); // 1ms timeout
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(std::time::Duration::from_millis(5));

        let result = cb.call(|| Ok::<i32, String>(42));
        assert_eq!(result.unwrap(), 42);
        // After one success in half-open, need one more to close.
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn circuit_closes_after_two_successes_in_half_open() {
        let cb = Arc::new(CircuitBreaker::new("test", 1, 1));
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(std::time::Duration::from_millis(5));

        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(1));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(2));
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_reopens_on_failure_in_half_open() {
        let cb = Arc::new(CircuitBreaker::new("test", 1, 1));
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(std::time::Duration::from_millis(5));

        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail again".into()));
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn circuit_resets_failure_count_on_success() {
        let cb = CircuitBreaker::new("test", 3, 1000);
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail1".into()));
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail2".into()));
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 2);

        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(42));
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_manual_reset() {
        let cb = CircuitBreaker::new("test", 1, 60_000);
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn circuit_snapshot_reports_state() {
        let cb = CircuitBreaker::new("my-breaker", 5, 30_000);
        let snap = cb.snapshot();
        assert_eq!(snap.name, "my-breaker");
        assert_eq!(snap.state, CircuitState::Closed);
        assert_eq!(snap.failure_threshold, 5);
        assert_eq!(snap.recovery_timeout_ms, 30_000);
        assert_eq!(snap.total_calls, 0);
    }

    #[test]
    fn circuit_total_calls_tracks_all_attempts() {
        let cb = CircuitBreaker::new("test", 10, 1000);
        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(1));
        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(2));
        let _: Result<(), CircuitBreakerError<String>> = cb.call(|| Err("fail".into()));
        assert_eq!(cb.total_calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn registry_snapshots_all_breakers() {
        let registry = CircuitBreakerRegistry::new();
        let cb1 = Arc::new(CircuitBreaker::new("breaker-1", 3, 1000));
        let cb2 = Arc::new(CircuitBreaker::new("breaker-2", 5, 5000));
        registry.register(cb1);
        registry.register(cb2);

        let snapshots = registry.snapshots();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].name, "breaker-1");
        assert_eq!(snapshots[1].name, "breaker-2");
    }

    #[test]
    fn circuit_success_in_closed_does_not_increase_consecutive() {
        let cb = CircuitBreaker::new("test", 3, 1000);
        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| Ok(1));
        assert_eq!(
            cb.consecutive_successes_in_half_open.load(Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn circuit_multiple_threshold_variations() {
        // Threshold of 1: opens on first failure
        let cb1 = CircuitBreaker::new("t1", 1, 1000);
        let _: Result<(), CircuitBreakerError<String>> = cb1.call(|| Err("f".into()));
        assert_eq!(cb1.state(), CircuitState::Open);

        // Threshold of 5: stays closed through 4 failures
        let cb5 = CircuitBreaker::new("t5", 5, 1000);
        for _ in 0..4 {
            let _: Result<(), CircuitBreakerError<String>> = cb5.call(|| Err("f".into()));
            assert_eq!(cb5.state(), CircuitState::Closed);
        }
        let _: Result<(), CircuitBreakerError<String>> = cb5.call(|| Err("f".into()));
        assert_eq!(cb5.state(), CircuitState::Open);
    }
}
