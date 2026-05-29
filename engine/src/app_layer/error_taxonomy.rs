use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SCHEMA_VERSION: &str = "error_record.v1";

pub const CANONICAL_DOMAINS: &[&str] = &[
    "tool_contract_error",
    "environment_error",
    "context_error",
    "model_judgment_error",
    "evaluation_error",
    "harness_bug",
    "user_abort",
    "provider_error",
    "timeout",
    "unknown_error",
];

pub const REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "error_id",
    "error_domain",
    "error_class",
    "retryable",
    "counts_against_model",
    "requires_human_triage",
    "tool_name",
    "model_profile_id",
    "context_pack_id",
    "event_id",
    "evidence_refs",
    "created_at",
];

pub const NON_RETRYABLE_DOMAINS: &[&str] = &["user_abort", "harness_bug", "unknown_error"];
pub const MANDATORY_TRIAGE_DOMAINS: &[&str] = &["unknown_error", "harness_bug"];
pub const NON_ADOPTABLE_DOMAINS: &[&str] = &["unknown_error"];

// ---------------------------------------------------------------------------
// ErrorDomain
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorDomain {
    ToolContractError,
    EnvironmentError,
    ContextError,
    ModelJudgmentError,
    EvaluationError,
    HarnessBug,
    UserAbort,
    ProviderError,
    Timeout,
    UnknownError,
}

impl ErrorDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolContractError => "tool_contract_error",
            Self::EnvironmentError => "environment_error",
            Self::ContextError => "context_error",
            Self::ModelJudgmentError => "model_judgment_error",
            Self::EvaluationError => "evaluation_error",
            Self::HarnessBug => "harness_bug",
            Self::UserAbort => "user_abort",
            Self::ProviderError => "provider_error",
            Self::Timeout => "timeout",
            Self::UnknownError => "unknown_error",
        }
    }

    pub fn parse_domain(s: &str) -> Option<Self> {
        match s {
            "tool_contract_error" => Some(Self::ToolContractError),
            "environment_error" => Some(Self::EnvironmentError),
            "context_error" => Some(Self::ContextError),
            "model_judgment_error" => Some(Self::ModelJudgmentError),
            "evaluation_error" => Some(Self::EvaluationError),
            "harness_bug" => Some(Self::HarnessBug),
            "user_abort" => Some(Self::UserAbort),
            "provider_error" => Some(Self::ProviderError),
            "timeout" => Some(Self::Timeout),
            "unknown_error" => Some(Self::UnknownError),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ErrorRecord
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ErrorRecord {
    pub error_id: String,
    pub error_domain: String,
    pub error_class: String,
    pub retryable: bool,
    pub counts_against_model: bool,
    pub requires_human_triage: bool,
    pub tool_name: String,
    pub model_profile_id: String,
    pub context_pack_id: String,
    pub event_id: String,
    pub evidence_refs: Vec<String>,
    pub created_at: String,
    pub schema_version: String,
}

impl Default for ErrorRecord {
    fn default() -> Self {
        Self {
            error_id: String::new(),
            error_domain: String::new(),
            error_class: String::new(),
            retryable: false,
            counts_against_model: false,
            requires_human_triage: false,
            tool_name: String::new(),
            model_profile_id: String::new(),
            context_pack_id: String::new(),
            event_id: String::new(),
            evidence_refs: Vec::new(),
            created_at: String::new(),
            schema_version: SCHEMA_VERSION.to_string(),
        }
    }
}

impl ErrorRecord {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_error_record(data: &serde_json::Value) -> Vec<String> {
    let mut violations = Vec::new();

    let object = match data.as_object() {
        Some(o) => o,
        None => {
            violations.push("error_record must be a JSON object".to_string());
            return violations;
        }
    };

    for field in REQUIRED_FIELDS {
        if !object.contains_key(*field) {
            violations.push(format!("missing required field: {}", field));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    let sv = object["schema_version"].as_str().unwrap_or("");
    if sv != SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {}, got {:?}",
            SCHEMA_VERSION, sv
        ));
    }

    let domain = object["error_domain"].as_str().unwrap_or("");
    if !CANONICAL_DOMAINS.contains(&domain) {
        violations.push(format!(
            "error_domain {:?} not in canonical domains",
            domain
        ));
    }

    match object.get("error_id").and_then(serde_json::Value::as_str) {
        Some(s) if !s.is_empty() => {}
        _ => violations.push("error_id must be a non-empty string".to_string()),
    }

    for bf in &["retryable", "counts_against_model", "requires_human_triage"] {
        if !object[*bf].is_boolean() {
            violations.push(format!(
                "{} must be a bool, got {}",
                bf,
                json_type_name(&object[*bf])
            ));
        }
    }

    if !object["evidence_refs"].is_array() {
        violations.push("evidence_refs must be a list".to_string());
    }

    violations.extend(validate_domain_constraints(object, domain));

    violations
}

fn validate_domain_constraints(
    object: &serde_json::Map<String, serde_json::Value>,
    domain: &str,
) -> Vec<String> {
    let mut violations = Vec::new();

    let retryable = object.get("retryable").and_then(serde_json::Value::as_bool);
    let triage = object
        .get("requires_human_triage")
        .and_then(serde_json::Value::as_bool);

    if domain == "unknown_error" {
        if retryable != Some(false) {
            violations.push("unknown_error must have retryable=false (fail-hard)".to_string());
        }
        if triage != Some(true) {
            violations.push("unknown_error must have requires_human_triage=true".to_string());
        }
    }

    if domain == "user_abort" && retryable != Some(false) {
        violations.push("user_abort must have retryable=false".to_string());
    }

    if domain == "harness_bug" {
        if retryable != Some(false) {
            violations.push("harness_bug must have retryable=false".to_string());
        }
        if triage != Some(true) {
            violations.push("harness_bug must have requires_human_triage=true".to_string());
        }
    }

    if (domain == "provider_error" || domain == "timeout") && retryable != Some(true) {
        violations.push(format!("{} should typically be retryable=true", domain));
    }

    violations
}

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct CreateErrorRecordParams<'a> {
    pub error_domain: &'a str,
    pub error_class: &'a str,
    pub retryable: bool,
    pub counts_against_model: bool,
    pub requires_human_triage: bool,
    pub tool_name: &'a str,
    pub model_profile_id: &'a str,
    pub context_pack_id: &'a str,
    pub event_id: &'a str,
    pub evidence_refs: Option<Vec<String>>,
    pub created_at: Option<&'a str>,
    pub error_id: Option<&'a str>,
}

pub fn create_error_record(params: CreateErrorRecordParams<'_>) -> ErrorRecord {
    ErrorRecord {
        error_id: params
            .error_id
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        error_domain: params.error_domain.to_string(),
        error_class: params.error_class.to_string(),
        retryable: params.retryable,
        counts_against_model: params.counts_against_model,
        requires_human_triage: params.requires_human_triage,
        tool_name: params.tool_name.to_string(),
        model_profile_id: params.model_profile_id.to_string(),
        context_pack_id: params.context_pack_id.to_string(),
        event_id: params.event_id.to_string(),
        evidence_refs: params.evidence_refs.unwrap_or_default(),
        created_at: params
            .created_at
            .map(str::to_string)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Classification helpers
// ---------------------------------------------------------------------------

pub fn is_adoptable(domain: &str) -> bool {
    !NON_ADOPTABLE_DOMAINS.contains(&domain)
}

pub fn is_retryable(domain: &str) -> bool {
    !NON_RETRYABLE_DOMAINS.contains(&domain)
}

pub fn requires_triage(domain: &str) -> bool {
    MANDATORY_TRIAGE_DOMAINS.contains(&domain)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_record_json() -> serde_json::Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "error_id": "err-001",
            "error_domain": "provider_error",
            "error_class": "rate_limit",
            "retryable": true,
            "counts_against_model": true,
            "requires_human_triage": false,
            "tool_name": "code_gen",
            "model_profile_id": "mimo-v2",
            "context_pack_id": "cp-001",
            "event_id": "evt-001",
            "evidence_refs": ["ref-1"],
            "created_at": "2026-01-01T00:00:00Z"
        })
    }

    #[test]
    fn test_valid_record_passes() {
        let violations = validate_error_record(&valid_record_json());
        assert!(
            violations.is_empty(),
            "expected no violations, got {:?}",
            violations
        );
    }

    #[test]
    fn test_missing_required_field() {
        let mut data = valid_record_json();
        data.as_object_mut().unwrap().remove("error_id");
        let violations = validate_error_record(&data);
        assert!(violations.iter().any(|v| v.contains("error_id")));
    }

    #[test]
    fn test_bad_schema_version() {
        let mut data = valid_record_json();
        data["schema_version"] = json!("wrong.v1");
        let violations = validate_error_record(&data);
        assert!(violations.iter().any(|v| v.contains("schema_version")));
    }

    #[test]
    fn test_unknown_error_must_be_non_retryable_and_triage() {
        let mut data = valid_record_json();
        data["error_domain"] = json!("unknown_error");
        data["retryable"] = json!(true);
        data["requires_human_triage"] = json!(false);
        let violations = validate_error_record(&data);
        assert!(violations.iter().any(|v| v.contains("retryable=false")));
        assert!(violations
            .iter()
            .any(|v| v.contains("requires_human_triage=true")));
    }

    #[test]
    fn test_harness_bug_constraints() {
        let mut data = valid_record_json();
        data["error_domain"] = json!("harness_bug");
        data["retryable"] = json!(true);
        data["requires_human_triage"] = json!(false);
        let violations = validate_error_record(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("harness_bug must have retryable=false")));
        assert!(violations
            .iter()
            .any(|v| v.contains("harness_bug must have requires_human_triage=true")));
    }

    #[test]
    fn test_user_abort_not_retryable() {
        let mut data = valid_record_json();
        data["error_domain"] = json!("user_abort");
        data["retryable"] = json!(true);
        let violations = validate_error_record(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("user_abort must have retryable=false")));
    }

    #[test]
    fn test_provider_error_should_be_retryable() {
        let mut data = valid_record_json();
        data["error_domain"] = json!("provider_error");
        data["retryable"] = json!(false);
        let violations = validate_error_record(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("should typically be retryable=true")));
    }

    #[test]
    fn test_domain_classification_helpers() {
        assert!(is_adoptable("provider_error"));
        assert!(!is_adoptable("unknown_error"));

        assert!(is_retryable("provider_error"));
        assert!(!is_retryable("unknown_error"));
        assert!(!is_retryable("user_abort"));
        assert!(!is_retryable("harness_bug"));

        assert!(requires_triage("unknown_error"));
        assert!(requires_triage("harness_bug"));
        assert!(!requires_triage("provider_error"));
    }

    #[test]
    fn test_create_error_record_defaults() {
        let rec = create_error_record(CreateErrorRecordParams {
            error_domain: "timeout",
            error_class: "deadline_exceeded",
            retryable: true,
            counts_against_model: false,
            requires_human_triage: false,
            tool_name: "tool_x",
            model_profile_id: "profile_y",
            context_pack_id: "ctx_z",
            event_id: "evt-1",
            evidence_refs: None,
            created_at: Some("2026-01-01T00:00:00Z"),
            error_id: Some("err-fixed"),
        });
        assert_eq!(rec.error_id, "err-fixed");
        assert_eq!(rec.error_domain, "timeout");
        assert_eq!(rec.schema_version, SCHEMA_VERSION);
        assert!(rec.evidence_refs.is_empty());
    }

    #[test]
    fn test_error_domain_roundtrip() {
        for domain_str in CANONICAL_DOMAINS {
            let domain = ErrorDomain::parse_domain(domain_str).unwrap();
            assert_eq!(domain.as_str(), *domain_str);
        }
    }

    #[test]
    fn test_error_record_to_value() {
        let rec = create_error_record(CreateErrorRecordParams {
            error_domain: "environment_error",
            error_class: "missing_dep",
            retryable: false,
            counts_against_model: true,
            requires_human_triage: false,
            tool_name: "t",
            model_profile_id: "p",
            context_pack_id: "c",
            event_id: "e",
            evidence_refs: Some(vec!["r1".to_string()]),
            created_at: Some("2026-01-01T00:00:00Z"),
            error_id: Some("err-001"),
        });
        let v = rec.to_value();
        assert_eq!(v["error_domain"], "environment_error");
        assert_eq!(v["schema_version"], SCHEMA_VERSION);
        assert_eq!(v["evidence_refs"][0], "r1");
    }
}
