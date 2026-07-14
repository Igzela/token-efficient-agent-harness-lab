use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

use crate::storage::local_product_store::LocalProductStore;

pub const PROVIDER_AUDIT_EVENT_SCHEMA_VERSION: &str = "provider_audit_event.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderAuditEvent {
    pub schema_version: String,
    pub event_id: String,
    pub dispatch_id: String,
    pub provider_id: String,
    pub event_type: String,
    pub input_token_count: Option<i64>,
    pub output_token_count: Option<i64>,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    pub latency_ms: Option<i64>,
    pub error_domain: Option<String>,
    pub redaction_status: String,
    pub created_at: String,
}

fn utc_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let hours = rem / 3600;
    let minutes = (rem % 3600) / 60;
    let seconds = rem % 60;

    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut rem = days;
    let mut y = 1970u64;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if rem < year_days {
            break;
        }
        rem -= year_days;
        y += 1;
    }
    let month_days: &[u64] = if is_leap(y) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u64;
    for &md in month_days {
        if rem < md {
            break;
        }
        rem -= md;
        m += 1;
    }
    (y, m, rem + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

pub struct ProviderAuditRecorder {
    events: Mutex<Vec<ProviderAuditEvent>>,
    counter: Mutex<u64>,
    instance_id: String,
    store: Option<Arc<LocalProductStore>>,
}

impl Default for ProviderAuditRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAuditRecorder {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            counter: Mutex::new(0),
            instance_id: uuid::Uuid::new_v4().simple().to_string(),
            store: None,
        }
    }

    pub fn with_store(store: Arc<LocalProductStore>) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            counter: Mutex::new(0),
            instance_id: uuid::Uuid::new_v4().simple().to_string(),
            store: Some(store),
        }
    }

    pub fn try_record(&self, event: ProviderAuditEvent) -> Result<(), String> {
        if let Some(store) = &self.store {
            store.record_provider_audit_event(&event)?;
        }
        self.events.lock().unwrap().push(event);
        Ok(())
    }

    pub fn record(&self, event: ProviderAuditEvent) {
        let _ = self.try_record(event);
    }

    fn create_event(
        &self,
        dispatch_id: &str,
        provider_id: &str,
        event_type: &str,
        extra: Option<&Value>,
    ) -> ProviderAuditEvent {
        let event_id = {
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            format!("paudit-{}-{:012x}", self.instance_id, *counter)
        };

        let mut event = ProviderAuditEvent {
            schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
            event_id,
            dispatch_id: dispatch_id.to_string(),
            provider_id: provider_id.to_string(),
            event_type: event_type.to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: None,
            currency: None,
            latency_ms: None,
            error_domain: None,
            redaction_status: "not_applicable".to_string(),
            created_at: utc_now_iso(),
        };

        if let Some(extra) = extra {
            if let Some(obj) = extra.as_object() {
                if let Some(v) = obj.get("input_token_count").and_then(|v| v.as_i64()) {
                    event.input_token_count = Some(v);
                }
                if let Some(v) = obj.get("output_token_count").and_then(|v| v.as_i64()) {
                    event.output_token_count = Some(v);
                }
                if let Some(v) = obj.get("cost").and_then(|v| v.as_f64()) {
                    event.cost = Some(v);
                }
                if let Some(v) = obj.get("currency").and_then(|v| v.as_str()) {
                    event.currency = Some(v.to_string());
                }
                if let Some(v) = obj.get("latency_ms").and_then(|v| v.as_i64()) {
                    event.latency_ms = Some(v);
                }
                if let Some(v) = obj.get("error_domain").and_then(|v| v.as_str()) {
                    event.error_domain = Some(v.to_string());
                }
                if let Some(v) = obj.get("redaction_status").and_then(|v| v.as_str()) {
                    if matches!(v, "redacted" | "not_applicable") {
                        event.redaction_status = v.to_string();
                    }
                }
            }
        }

        event
    }

    pub fn try_create_and_record(
        &self,
        dispatch_id: &str,
        provider_id: &str,
        event_type: &str,
        extra: Option<&Value>,
    ) -> Result<ProviderAuditEvent, String> {
        let event = self.create_event(dispatch_id, provider_id, event_type, extra);
        self.try_record(event.clone())?;
        Ok(event)
    }

    pub fn try_reserve_cost(
        &self,
        dispatch_id: &str,
        provider_id: &str,
        reserved_cost_usd: f64,
        per_call_cap_usd: f64,
        daily_cap_usd: f64,
    ) -> Result<ProviderAuditEvent, String> {
        let store = self.store.as_ref().ok_or_else(|| {
            "persistent provider audit store is required for cost reservation".to_string()
        })?;
        let mut event = self.create_event(
            dispatch_id,
            provider_id,
            "request_reserved",
            Some(&serde_json::json!({
                "cost": reserved_cost_usd,
                "currency": "USD",
                "redaction_status": "redacted",
            })),
        );
        event.event_id = format!(
            "paudit-reservation-{}",
            hex::encode(Sha256::digest(format!(
                "{dispatch_id}\0{provider_id}\0request_reserved"
            )))
        );
        store.reserve_provider_audit_cost(&event, per_call_cap_usd, daily_cap_usd)?;
        self.events.lock().unwrap().push(event.clone());
        Ok(event)
    }

    pub fn create_and_record(
        &self,
        dispatch_id: &str,
        provider_id: &str,
        event_type: &str,
        extra: Option<&Value>,
    ) -> ProviderAuditEvent {
        let event = self.create_event(dispatch_id, provider_id, event_type, extra);
        self.record(event.clone());
        event
    }

    pub fn list_events(&self, dispatch_id: &str) -> Vec<ProviderAuditEvent> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.dispatch_id == dispatch_id)
            .cloned()
            .collect()
    }

    pub fn list_all(&self) -> Vec<ProviderAuditEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_recorder_is_empty() {
        let r = ProviderAuditRecorder::new();
        assert_eq!(r.count(), 0);
        assert!(r.list_all().is_empty());
    }

    #[test]
    fn record_appends_event() {
        let r = ProviderAuditRecorder::new();
        let event = ProviderAuditEvent {
            schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
            event_id: "paudit-000000000001".to_string(),
            dispatch_id: "d1".to_string(),
            provider_id: "p1".to_string(),
            event_type: "request_sent".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: None,
            currency: None,
            latency_ms: None,
            error_domain: None,
            redaction_status: "not_applicable".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        r.record(event.clone());
        assert_eq!(r.count(), 1);
        assert_eq!(r.list_all()[0], event);
    }

    #[test]
    fn create_and_record_generates_event() {
        let r = ProviderAuditRecorder::new();
        let event = r.create_and_record("d1", "p1", "request_sent", None);
        assert_eq!(event.schema_version, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION);
        assert!(event.event_id.starts_with("paudit-"));
        assert_eq!(event.dispatch_id, "d1");
        assert_eq!(event.provider_id, "p1");
        assert_eq!(event.event_type, "request_sent");
        assert_eq!(event.redaction_status, "not_applicable");
        assert!(event.input_token_count.is_none());
        assert_eq!(r.count(), 1);
    }

    #[test]
    fn create_and_record_with_extra() {
        let r = ProviderAuditRecorder::new();
        let extra = json!({
            "input_token_count": 100,
            "output_token_count": 50,
            "cost": 0.0025,
            "currency": "USD",
            "latency_ms": 42,
            "error_domain": null,
            "redaction_status": "redacted"
        });
        let event = r.create_and_record("d1", "p1", "response_received", Some(&extra));
        assert_eq!(event.input_token_count, Some(100));
        assert_eq!(event.output_token_count, Some(50));
        assert_eq!(event.cost, Some(0.0025));
        assert_eq!(event.currency.as_deref(), Some("USD"));
        assert_eq!(event.latency_ms, Some(42));
        assert!(event.error_domain.is_none());
        assert_eq!(event.redaction_status, "redacted");
    }

    #[test]
    fn create_and_record_with_error_domain() {
        let r = ProviderAuditRecorder::new();
        let extra = json!({"error_domain": "provider_rate_limit"});
        let event = r.create_and_record("d1", "p1", "error", Some(&extra));
        assert_eq!(event.error_domain.as_deref(), Some("provider_rate_limit"));
    }

    #[test]
    fn list_events_filters_by_dispatch_id() {
        let r = ProviderAuditRecorder::new();
        r.create_and_record("d1", "p1", "request_sent", None);
        r.create_and_record("d2", "p1", "request_sent", None);
        r.create_and_record("d1", "p2", "response_received", None);
        assert_eq!(r.list_events("d1").len(), 2);
        assert_eq!(r.list_events("d2").len(), 1);
        assert_eq!(r.list_events("d3").len(), 0);
    }

    #[test]
    fn event_ids_are_unique() {
        let r = ProviderAuditRecorder::new();
        let e1 = r.create_and_record("d1", "p1", "request_sent", None);
        let e2 = r.create_and_record("d1", "p1", "request_sent", None);
        assert_ne!(e1.event_id, e2.event_id);
    }

    #[test]
    fn event_ids_are_monotonically_increasing() {
        let r = ProviderAuditRecorder::new();
        let e1 = r.create_and_record("d1", "p1", "request_sent", None);
        let e2 = r.create_and_record("d1", "p1", "request_sent", None);
        assert!(e2.event_id > e1.event_id);
    }

    #[test]
    fn event_roundtrip_json() {
        let r = ProviderAuditRecorder::new();
        let event = r.create_and_record("d1", "p1", "request_sent", None);
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: ProviderAuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn extra_ignores_unknown_fields() {
        let r = ProviderAuditRecorder::new();
        let extra = json!({"unknown_field": "value", "input_token_count": 42});
        let event = r.create_and_record("d1", "p1", "request_sent", Some(&extra));
        assert_eq!(event.input_token_count, Some(42));
    }

    #[test]
    fn extra_non_object_ignored() {
        let r = ProviderAuditRecorder::new();
        let extra = json!("not an object");
        let event = r.create_and_record("d1", "p1", "request_sent", Some(&extra));
        assert!(event.input_token_count.is_none());
    }

    #[test]
    fn utc_now_iso_produces_valid_format() {
        let ts = utc_now_iso();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    #[test]
    fn days_to_ymd_known_date() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_2000_01_01() {
        let (y, m, d) = days_to_ymd(10957);
        assert_eq!((y, m, d), (2000, 1, 1));
    }
}
