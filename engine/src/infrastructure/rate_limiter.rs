use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const RATE_LIMITER_SCHEMA_VERSION: &str = "rate_limiter.v1";
pub const DEFAULT_WINDOW_SECONDS: f64 = 60.0;
pub const DEFAULT_MAX_BUCKETS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: i64,
    pub limit: i64,
    pub retry_after: Option<f64>,
}

pub struct RateLimiter {
    window_seconds: f64,
    max_buckets: usize,
    buckets: HashMap<(String, String), Vec<f64>>,
}

impl RateLimiter {
    pub fn new(window_seconds: f64, max_buckets: usize) -> Self {
        Self {
            window_seconds,
            max_buckets,
            buckets: HashMap::new(),
        }
    }

    pub fn window_seconds(&self) -> f64 {
        self.window_seconds
    }

    pub fn check(
        &mut self,
        tenant_id: &str,
        api_key_id: &str,
        rate_limit: Option<i64>,
        now: f64,
    ) -> RateLimitResult {
        let limit = match rate_limit {
            Some(l) if l > 0 => l,
            _ => {
                return RateLimitResult {
                    allowed: true,
                    remaining: -1,
                    limit: -1,
                    retry_after: None,
                }
            }
        };

        let window_start = now - self.window_seconds;
        let key = (tenant_id.to_string(), api_key_id.to_string());

        if !self.buckets.contains_key(&key) && self.buckets.len() >= self.max_buckets {
            if let Some(oldest_key) = self
                .buckets
                .iter()
                .min_by(|a, b| {
                    let a_first = a.1.first().copied().unwrap_or(f64::INFINITY);
                    let b_first = b.1.first().copied().unwrap_or(f64::INFINITY);
                    a_first
                        .partial_cmp(&b_first)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(k, _)| k.clone())
            {
                self.buckets.remove(&oldest_key);
            }
        }

        let timestamps = self.buckets.entry(key).or_default();
        prune(timestamps, window_start);
        let current_count = timestamps.len() as i64;

        if current_count >= limit {
            let oldest_in_window = timestamps[0];
            let retry_after = (oldest_in_window + self.window_seconds - now).max(0.0);
            return RateLimitResult {
                allowed: false,
                remaining: 0,
                limit,
                retry_after: Some(retry_after),
            };
        }

        timestamps.push(now);
        RateLimitResult {
            allowed: true,
            remaining: limit - current_count - 1,
            limit,
            retry_after: None,
        }
    }

    pub fn cleanup(&mut self, now: f64) -> usize {
        let window_start = now - self.window_seconds;
        let mut removed = 0;
        let mut empty_keys = Vec::new();

        for (key, timestamps) in &mut self.buckets {
            let before = timestamps.len();
            prune(timestamps, window_start);
            removed += before - timestamps.len();
            if timestamps.is_empty() {
                empty_keys.push(key.clone());
            }
        }
        for key in empty_keys {
            self.buckets.remove(&key);
        }
        removed
    }

    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }
}

fn prune(timestamps: &mut Vec<f64>, window_start: f64) {
    let cutoff = timestamps
        .binary_search_by(|t| {
            t.partial_cmp(&window_start)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(|i| i);
    // binary_search gives us the index where window_start would be inserted.
    // We want to remove all timestamps <= window_start.
    let cutoff = match timestamps[..cutoff]
        .iter()
        .rposition(|t| *t <= window_start)
    {
        Some(i) => i + 1,
        None => 0,
    };
    if cutoff > 0 {
        timestamps.drain(..cutoff);
    }
}
