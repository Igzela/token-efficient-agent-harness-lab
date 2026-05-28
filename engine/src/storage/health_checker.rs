use super::durable_store::DurableStore;
use serde::{Deserialize, Serialize};

pub const HEALTH_CHECKER_SCHEMA_VERSION: &str = "health_checker.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: String,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: String,
    pub checks: Vec<HealthCheck>,
    pub timestamp: f64,
}

pub struct HealthChecker<'a> {
    store: Option<&'a DurableStore>,
}

impl<'a> HealthChecker<'a> {
    pub fn new(store: Option<&'a DurableStore>) -> Self {
        Self { store }
    }

    pub fn check_storage(&self, now: f64) -> HealthCheck {
        match self.store {
            None => HealthCheck {
                name: "storage".to_string(),
                status: "unhealthy".to_string(),
                message: "no store configured".to_string(),
                latency_ms: 0.0,
            },
            Some(store) => match store.stats() {
                Ok(stats) => HealthCheck {
                    name: "storage".to_string(),
                    status: "healthy".to_string(),
                    message: format!(
                        "plans={}, repos={}, events={}",
                        stats["plans"], stats["repos"], stats["events"]
                    ),
                    latency_ms: now * 1000.0, // simplified latency
                },
                Err(e) => HealthCheck {
                    name: "storage".to_string(),
                    status: "unhealthy".to_string(),
                    message: e,
                    latency_ms: 0.0,
                },
            },
        }
    }

    pub fn check_events(&self, now: f64) -> HealthCheck {
        match self.store {
            None => HealthCheck {
                name: "events".to_string(),
                status: "unhealthy".to_string(),
                message: "no store configured".to_string(),
                latency_ms: 0.0,
            },
            Some(store) => match store.get_events(None, 1) {
                Ok(events) => HealthCheck {
                    name: "events".to_string(),
                    status: "healthy".to_string(),
                    message: format!("accessible, latest_count={}", events.len()),
                    latency_ms: now * 1000.0,
                },
                Err(e) => HealthCheck {
                    name: "events".to_string(),
                    status: "unhealthy".to_string(),
                    message: e,
                    latency_ms: 0.0,
                },
            },
        }
    }

    pub fn check_plans(&self, now: f64) -> HealthCheck {
        match self.store {
            None => HealthCheck {
                name: "plans".to_string(),
                status: "unhealthy".to_string(),
                message: "no store configured".to_string(),
                latency_ms: 0.0,
            },
            Some(store) => match store.stats() {
                Ok(stats) => HealthCheck {
                    name: "plans".to_string(),
                    status: "healthy".to_string(),
                    message: format!("accessible, count={}", stats["plans"]),
                    latency_ms: now * 1000.0,
                },
                Err(e) => HealthCheck {
                    name: "plans".to_string(),
                    status: "unhealthy".to_string(),
                    message: e,
                    latency_ms: 0.0,
                },
            },
        }
    }

    pub fn health(&self, now: f64) -> HealthReport {
        let checks = vec![
            self.check_storage(now),
            self.check_events(now),
            self.check_plans(now),
        ];
        let statuses: Vec<&str> = checks.iter().map(|c| c.status.as_str()).collect();
        let overall = if statuses.iter().all(|s| *s == "healthy") {
            "healthy"
        } else if statuses.contains(&"unhealthy") {
            "unhealthy"
        } else {
            "degraded"
        };
        HealthReport {
            status: overall.to_string(),
            checks,
            timestamp: now,
        }
    }

    pub fn readiness(&self, now: f64) -> HealthReport {
        let checks = vec![
            self.check_storage(now),
            self.check_events(now),
            self.check_plans(now),
        ];
        let ready = checks.iter().all(|c| c.status == "healthy");
        HealthReport {
            status: if ready { "ready" } else { "not_ready" }.to_string(),
            checks,
            timestamp: now,
        }
    }
}
