//! Store-owned RWE campaign package descriptor and driver seam.
//!
//! Research evidence on the common RWE basis requires finite frozen canonical
//! experiments binding immutable task/corpus, protocol, schedule, provider,
//! model, binary, budget, target, and rollback identities before any execution.
//!
//! A campaign package binds these identities into a verifiable, tamper-evident
//! schema (`rwe_campaign_package.v1`). Live execution requires explicit Store-owned
//! authorization and standing owner approval. Provider-free evaluation composes
//! through this seam without live provider POSTs or target repository writes.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::frozen_rwe_bindings::FROZEN_RWE_TARGET_MAIN_SHA;
use super::operator_corpus::{
    freeze_current_operator_contract_set, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL,
    OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL, OPERATOR_CORPUS_ID, OPERATOR_TARGET_REPO,
};
use crate::storage::local_product_store::{AuthenticatedPrincipal, LocalProductStore};

pub const RWE_CAMPAIGN_PACKAGE_SCHEMA: &str = "rwe_campaign_package.v1";
pub const RWE_DEEPSEEK_V2_PACKAGE_ID: &str = "rwe-campaign-deepseek-v2";
pub const RWE_AGY_V1_PACKAGE_ID: &str = "rwe-campaign-agy-v1";
pub const FROZEN_PROVIDER_EXECUTION_BINDING_SCHEMA: &str =
    "rwe_frozen_provider_execution_binding.v1";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The provider route that a frozen campaign actually admits.  This is an
/// execution identity, not a display hint: every field is copied into the
/// Store-owned manifest and compared again immediately before transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenProviderExecutionBinding {
    pub schema_version: String,
    pub provider_identity: String,
    pub provider_kind: String,
    pub protocol: String,
    pub host: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub credential_reference: String,
    pub request_schema_version: String,
    pub response_schema_version: String,
    pub usage_parser_version: String,
    pub pricing_identity: Option<String>,
    pub cost_unavailable: bool,
    pub admitted_model: String,
}

impl FrozenProviderExecutionBinding {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != FROZEN_PROVIDER_EXECUTION_BINDING_SCHEMA {
            return Err("frozen provider binding schema is not canonical".into());
        }
        for (name, value) in [
            ("provider_identity", &self.provider_identity),
            ("provider_kind", &self.provider_kind),
            ("protocol", &self.protocol),
            ("host", &self.host),
            ("base_url", &self.base_url),
            ("endpoint_path", &self.endpoint_path),
            ("credential_reference", &self.credential_reference),
            ("request_schema_version", &self.request_schema_version),
            ("response_schema_version", &self.response_schema_version),
            ("usage_parser_version", &self.usage_parser_version),
            ("admitted_model", &self.admitted_model),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(format!("frozen provider binding {name} is malformed"));
            }
        }
        if self.protocol != "openai_compatible" {
            return Err("frozen provider binding protocol is not admitted".into());
        }
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| "frozen provider binding base URL is malformed".to_string())?;
        if url.scheme() != "https" || url.host_str() != Some(self.host.as_str()) {
            return Err("frozen provider binding host and base URL disagree".into());
        }
        if !self.endpoint_path.starts_with('/')
            || self.endpoint_path.contains('?')
            || self.endpoint_path.contains('#')
        {
            return Err("frozen provider binding endpoint path is malformed".into());
        }
        match (&self.pricing_identity, self.cost_unavailable) {
            (Some(identity), false) if !identity.trim().is_empty() => {}
            (None, true) => {}
            _ => return Err("frozen provider binding pricing identity is ambiguous".into()),
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "provider_identity": self.provider_identity,
            "provider_kind": self.provider_kind,
            "protocol": self.protocol,
            "host": self.host,
            "base_url": self.base_url,
            "endpoint_path": self.endpoint_path,
            "credential_reference": self.credential_reference,
            "request_schema_version": self.request_schema_version,
            "response_schema_version": self.response_schema_version,
            "usage_parser_version": self.usage_parser_version,
            "pricing_identity": self.pricing_identity,
            "cost_unavailable": self.cost_unavailable,
            "admitted_model": self.admitted_model,
        })
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        let binding: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("frozen provider binding is malformed: {error}"))?;
        binding.validate()?;
        Ok(binding)
    }
}

/// The only provider binding admitted by the current managed DeepSeek route.
/// Keeping this constructor independent of the corpus freeze lets every
/// Store-owned boundary compare the same exact binding without reconstructing
/// a route from a provider kind or a user-supplied URL.
pub fn canonical_deepseek_provider_binding() -> FrozenProviderExecutionBinding {
    FrozenProviderExecutionBinding {
        schema_version: FROZEN_PROVIDER_EXECUTION_BINDING_SCHEMA.into(),
        provider_identity: crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_ID.into(),
        provider_kind: crate::provider::managed_deepseek::DEEPSEEK_PROVIDER_KIND.into(),
        protocol: "openai_compatible".into(),
        host: "api.deepseek.com".into(),
        base_url: crate::provider::managed_deepseek::DEEPSEEK_OPENAI_BASE_URL.into(),
        endpoint_path: crate::provider::managed_deepseek::DEEPSEEK_OPENAI_PATH.into(),
        credential_reference: crate::provider::managed_deepseek::DEEPSEEK_CREDENTIAL_REFERENCE
            .into(),
        request_schema_version: crate::provider::managed_deepseek::MANAGED_PROVIDER_CALL_SCHEMA
            .into(),
        response_schema_version:
            crate::provider::managed_deepseek::MANAGED_PROVIDER_RESPONSE_SCHEMA.into(),
        usage_parser_version: crate::provider::managed_deepseek::DEEPSEEK_USAGE_PARSER_VERSION
            .into(),
        pricing_identity: Some("deepseek-v4-usd-2026-07-31".into()),
        cost_unavailable: false,
        admitted_model: OPERATOR_ADMITTED_MODEL.into(),
    }
}

/// Immutable descriptor for one frozen RWE campaign package.
///
/// Binds provider, model, binary, corpus, protocol, schedule, target, and
/// rollback identities into a verifiable tamper-evident envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenCampaignPackage {
    pub schema_version: String,
    pub package_id: String,
    pub provider_kind: String,
    pub provider_execution_binding: Option<FrozenProviderExecutionBinding>,
    pub admitted_model: String,
    pub planner_reviewer_model: String,
    pub admitted_binary_path: String,
    pub admitted_binary_version: String,
    pub admitted_binary_sha256: String,
    pub target_repo: String,
    pub target_main_sha: String,
    pub corpus_id: String,
    pub corpus_sha256: String,
    pub protocol_sha256: String,
    pub schedule_sha256: String,
    pub cell_count: usize,
    pub auto_merge_disabled: bool,
    pub draft_pr_only: bool,
    pub requires_owner_approval: bool,
    pub live_authorization_required: bool,
    pub max_provider_requests_per_cell: u64,
    pub max_total_tokens_per_cell: u64,
    pub timeout_ms_per_cell: u64,
    pub max_cost_usd_per_cell: Option<f64>,
    pub rollback_reference: String,
    pub rollback_strategy: String,
    pub notes: String,
}

impl FrozenCampaignPackage {
    /// Resolve the exact provider binding for one package-owned model role.
    ///
    /// A package may expose distinct implementer and planner/reviewer model
    /// identities while sharing one frozen provider route.  The returned value
    /// is still the package binding in every field; only the model is selected
    /// from the package's admitted role identities.  Arbitrary caller strings
    /// are never admitted here.
    pub fn provider_execution_binding_for_model(
        &self,
        requested_model: &str,
    ) -> Result<FrozenProviderExecutionBinding, String> {
        let mut binding = self
            .provider_execution_binding
            .clone()
            .ok_or("frozen campaign package lacks a direct provider binding")?;
        if requested_model != self.admitted_model && requested_model != self.planner_reviewer_model
        {
            return Err("requested model is not an exact model identity admitted by the frozen campaign package".into());
        }
        binding.admitted_model = requested_model.to_string();
        binding.validate()?;
        Ok(binding)
    }

    /// Strict validation of package invariants.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RWE_CAMPAIGN_PACKAGE_SCHEMA {
            return Err(format!(
                "invalid package schema_version: expected {RWE_CAMPAIGN_PACKAGE_SCHEMA}, got {}",
                self.schema_version
            ));
        }
        if self.package_id.trim().is_empty() {
            return Err("package_id must not be empty".into());
        }
        if self.provider_kind.trim().is_empty() {
            return Err("provider_kind must not be empty".into());
        }
        if self.admitted_model.trim().is_empty() {
            return Err("admitted_model must not be empty".into());
        }
        match (
            &self.provider_execution_binding,
            self.provider_kind.as_str(),
        ) {
            (Some(binding), _) => {
                binding.validate()?;
                if binding.admitted_model != self.admitted_model {
                    return Err("frozen provider binding model differs from package model".into());
                }
            }
            (None, "agy") => {}
            (None, _) => {
                return Err("frozen campaign package lacks a direct provider binding".into())
            }
        }
        if self.planner_reviewer_model.trim().is_empty() {
            return Err("planner_reviewer_model must not be empty".into());
        }
        if self.admitted_binary_path.trim().is_empty() {
            return Err("admitted_binary_path must not be empty".into());
        }
        if self.admitted_binary_version.trim().is_empty() {
            return Err("admitted_binary_version must not be empty".into());
        }
        if !is_sha256(&self.admitted_binary_sha256) {
            return Err("admitted_binary_sha256 must be a valid 64-character hex SHA-256".into());
        }
        if self.target_repo != OPERATOR_TARGET_REPO {
            return Err(format!(
                "target_repo mismatch: expected {OPERATOR_TARGET_REPO}, got {}",
                self.target_repo
            ));
        }
        if self.target_main_sha != FROZEN_RWE_TARGET_MAIN_SHA {
            return Err(format!(
                "target_main_sha mismatch: expected {FROZEN_RWE_TARGET_MAIN_SHA}, got {}",
                self.target_main_sha
            ));
        }
        if !is_sha256(&self.corpus_sha256) {
            return Err("corpus_sha256 must be a valid 64-character hex SHA-256".into());
        }
        if !is_sha256(&self.protocol_sha256) {
            return Err("protocol_sha256 must be a valid 64-character hex SHA-256".into());
        }
        if !is_sha256(&self.schedule_sha256) {
            return Err("schedule_sha256 must be a valid 64-character hex SHA-256".into());
        }
        if self.cell_count != 4 {
            return Err(format!(
                "cell_count mismatch: expected 4 cells, got {}",
                self.cell_count
            ));
        }
        if !self.auto_merge_disabled {
            return Err("auto_merge_disabled must be true for all RWE campaign packages".into());
        }
        if !self.draft_pr_only {
            return Err("draft_pr_only must be true for all RWE campaign packages".into());
        }
        if self.max_provider_requests_per_cell == 0 {
            return Err("max_provider_requests_per_cell must be greater than 0".into());
        }
        if self.max_total_tokens_per_cell == 0 {
            return Err("max_total_tokens_per_cell must be greater than 0".into());
        }
        if self.timeout_ms_per_cell == 0 {
            return Err("timeout_ms_per_cell must be greater than 0".into());
        }
        if self.rollback_reference.trim().is_empty() {
            return Err("rollback_reference must not be empty".into());
        }
        if self.rollback_strategy.trim().is_empty() {
            return Err("rollback_strategy must not be empty".into());
        }
        Ok(())
    }

    /// Canonical SHA-256 digest of this package descriptor.
    pub fn canonical_sha256(&self) -> Result<String, String> {
        let val = self.to_json();
        let sorted = sort_value(&val);
        let canonical_str = serde_json::to_string(&sorted)
            .map_err(|e| format!("canonical json serialization error: {e}"))?;
        Ok(sha256_hex(canonical_str.as_bytes()))
    }

    /// Serialize to JSON Value.
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "package_id": self.package_id,
            "provider_kind": self.provider_kind,
            "provider_execution_binding": self.provider_execution_binding,
            "admitted_model": self.admitted_model,
            "planner_reviewer_model": self.planner_reviewer_model,
            "admitted_binary_path": self.admitted_binary_path,
            "admitted_binary_version": self.admitted_binary_version,
            "admitted_binary_sha256": self.admitted_binary_sha256,
            "target_repo": self.target_repo,
            "target_main_sha": self.target_main_sha,
            "corpus_id": self.corpus_id,
            "corpus_sha256": self.corpus_sha256,
            "protocol_sha256": self.protocol_sha256,
            "schedule_sha256": self.schedule_sha256,
            "cell_count": self.cell_count,
            "auto_merge_disabled": self.auto_merge_disabled,
            "draft_pr_only": self.draft_pr_only,
            "requires_owner_approval": self.requires_owner_approval,
            "live_authorization_required": self.live_authorization_required,
            "max_provider_requests_per_cell": self.max_provider_requests_per_cell,
            "max_total_tokens_per_cell": self.max_total_tokens_per_cell,
            "timeout_ms_per_cell": self.timeout_ms_per_cell,
            "max_cost_usd_per_cell": self.max_cost_usd_per_cell,
            "rollback_reference": self.rollback_reference,
            "rollback_strategy": self.rollback_strategy,
            "notes": self.notes,
        })
    }

    /// Deserialize from JSON Value.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        serde_json::from_value(value.clone())
            .map_err(|e| format!("package deserialization error: {e}"))
    }
}

/// The canonical operator-approved Decision B DeepSeek v2 frozen RWE baseline package.
pub fn canonical_deepseek_v2_package() -> Result<FrozenCampaignPackage, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let binary_sha256 = sha256_hex(
        format!(
            "rwe.in_process_binary.v1:{}:{}",
            OPERATOR_ADMITTED_BINARY_PATH, OPERATOR_ADMITTED_BINARY_VERSION
        )
        .as_bytes(),
    );
    let pkg = FrozenCampaignPackage {
        schema_version: RWE_CAMPAIGN_PACKAGE_SCHEMA.into(),
        package_id: RWE_DEEPSEEK_V2_PACKAGE_ID.into(),
        provider_kind: "managed_deepseek".into(),
        provider_execution_binding: Some(canonical_deepseek_provider_binding()),
        admitted_model: OPERATOR_ADMITTED_MODEL.into(),
        planner_reviewer_model: OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL.into(),
        admitted_binary_path: OPERATOR_ADMITTED_BINARY_PATH.into(),
        admitted_binary_version: OPERATOR_ADMITTED_BINARY_VERSION.into(),
        admitted_binary_sha256: binary_sha256,
        target_repo: OPERATOR_TARGET_REPO.into(),
        target_main_sha: FROZEN_RWE_TARGET_MAIN_SHA.into(),
        corpus_id: OPERATOR_CORPUS_ID.into(),
        corpus_sha256: frozen.corpus.corpus_sha256,
        protocol_sha256: frozen.protocol.body_sha256,
        schedule_sha256: frozen.schedule.schedule_sha256,
        cell_count: 4,
        auto_merge_disabled: true,
        draft_pr_only: true,
        requires_owner_approval: false, // pre-approved under Decision B / Stage 3
        live_authorization_required: true,
        max_provider_requests_per_cell: 3,
        max_total_tokens_per_cell: 16_000,
        timeout_ms_per_cell: 900_000,
        max_cost_usd_per_cell: Some(0.10),
        rollback_reference: format!("accepted-main:{}", frozen.accepted_main_sha),
        rollback_strategy: "restore_target_main".into(),
        notes: "Canonical operator-approved Decision B DeepSeek v2 frozen RWE baseline package."
            .into(),
    };
    pkg.validate()?;
    Ok(pkg)
}

/// Candidate AGY provider package specification.
///
/// Represents the prospective bounded AGY worker integration package.
/// In accordance with Rule 4, this package explicitly enforces:
/// - `requires_owner_approval: true`: Cannot be activated without authentic owner approval.
/// - `live_authorization_required: true`: Cannot perform live calls without Store-issued authorization.
/// - Distinct package ID and binary identity from DeepSeek.
pub fn canonical_agy_v1_candidate_package() -> Result<FrozenCampaignPackage, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let binary_sha256 = sha256_hex(b"rwe.external_binary.v1:/usr/local/bin/agy:0.4.0");
    let pkg = FrozenCampaignPackage {
        schema_version: RWE_CAMPAIGN_PACKAGE_SCHEMA.into(),
        package_id: RWE_AGY_V1_PACKAGE_ID.into(),
        provider_kind: "agy".into(),
        provider_execution_binding: None,
        admitted_model: "gemini-2.5-flash".into(),
        planner_reviewer_model: "gemini-2.5-pro".into(),
        admitted_binary_path: "/usr/local/bin/agy".into(),
        admitted_binary_version: "0.4.0".into(),
        admitted_binary_sha256: binary_sha256,
        target_repo: OPERATOR_TARGET_REPO.into(),
        target_main_sha: FROZEN_RWE_TARGET_MAIN_SHA.into(),
        corpus_id: OPERATOR_CORPUS_ID.into(),
        corpus_sha256: frozen.corpus.corpus_sha256,
        protocol_sha256: frozen.protocol.body_sha256,
        schedule_sha256: frozen.schedule.schedule_sha256,
        cell_count: 4,
        auto_merge_disabled: true,
        draft_pr_only: true,
        requires_owner_approval: true, // MUST have explicit owner approval
        live_authorization_required: true, // MUST have Store-issued live authorization
        max_provider_requests_per_cell: 3,
        max_total_tokens_per_cell: 16_000,
        timeout_ms_per_cell: 900_000,
        max_cost_usd_per_cell: Some(0.10),
        rollback_reference: format!("accepted-main:{}", frozen.accepted_main_sha),
        rollback_strategy: "restore_target_main".into(),
        notes: "Candidate AGY provider package; requires authentic owner approval and Store live authorization before activation.".into(),
    };
    pkg.validate()?;
    Ok(pkg)
}

/// Resolve a frozen campaign package by its canonical identifier.
pub fn resolve_frozen_campaign_package(package_id: &str) -> Result<FrozenCampaignPackage, String> {
    match package_id {
        RWE_DEEPSEEK_V2_PACKAGE_ID => canonical_deepseek_v2_package(),
        RWE_AGY_V1_PACKAGE_ID => canonical_agy_v1_candidate_package(),
        other => Err(format!("unknown frozen campaign package id: {other}")),
    }
}

/// Record an audit trail entry in LocalProductStore verifying and registering a campaign package.
pub fn record_campaign_package_audit(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    package: &FrozenCampaignPackage,
) -> Result<Value, String> {
    package.validate()?;
    let canonical_hash = package.canonical_sha256()?;
    let details = json!({
        "schema_version": "rwe_campaign_package_audit.v1",
        "package_id": package.package_id,
        "package_canonical_sha256": canonical_hash,
        "provider_kind": package.provider_kind,
        "provider_execution_binding": package.provider_execution_binding,
        "admitted_model": package.admitted_model,
        "requires_owner_approval": package.requires_owner_approval,
        "live_authorization_required": package.live_authorization_required,
        "target_repo": package.target_repo,
        "target_main_sha": package.target_main_sha,
        "registered_by_principal": principal.principal_id(),
        "tenant_id": principal.tenant_id(),
    });
    store.append_audit(
        principal.principal_id(),
        "rwe_campaign_package_verified",
        &format!("rwe:campaign_package:{}", package.package_id),
        &details,
    )?;
    Ok(details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_v2_package_validates_cleanly() {
        let pkg = canonical_deepseek_v2_package().expect("deepseek v2 package must validate");
        assert_eq!(pkg.package_id, RWE_DEEPSEEK_V2_PACKAGE_ID);
        assert_eq!(pkg.provider_kind, "managed_deepseek");
        assert_eq!(pkg.cell_count, 4);
        assert!(pkg.auto_merge_disabled);
        assert!(pkg.draft_pr_only);
        assert!(!pkg.requires_owner_approval);
        assert!(pkg.live_authorization_required);

        let hash = pkg.canonical_sha256().expect("hash must compute");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn package_model_binding_is_exact_and_rejects_arbitrary_models() {
        let pkg = canonical_deepseek_v2_package().unwrap();
        assert_eq!(
            pkg.provider_execution_binding_for_model("deepseek-v4-pro")
                .unwrap()
                .admitted_model,
            "deepseek-v4-pro"
        );
        assert_eq!(
            pkg.provider_execution_binding_for_model("deepseek-v4-flash")
                .unwrap()
                .admitted_model,
            "deepseek-v4-flash"
        );
        assert!(pkg
            .provider_execution_binding_for_model("arbitrary-model")
            .is_err());
    }

    #[test]
    fn agy_v1_candidate_package_validates_cleanly() {
        let pkg =
            canonical_agy_v1_candidate_package().expect("agy candidate package must validate");
        assert_eq!(pkg.package_id, RWE_AGY_V1_PACKAGE_ID);
        assert_eq!(pkg.provider_kind, "agy");
        assert_eq!(pkg.admitted_binary_path, "/usr/local/bin/agy");
        assert!(pkg.requires_owner_approval);
        assert!(pkg.live_authorization_required);

        let hash = pkg.canonical_sha256().expect("hash must compute");
        assert_eq!(hash.len(), 64);

        // Disallow collision with DeepSeek package
        let ds_pkg = canonical_deepseek_v2_package().unwrap();
        assert_ne!(pkg.package_id, ds_pkg.package_id);
        assert_ne!(hash, ds_pkg.canonical_sha256().unwrap());
    }

    #[test]
    fn package_tampering_is_rejected() {
        let mut pkg = canonical_deepseek_v2_package().unwrap();

        // Mutated cell count
        pkg.cell_count = 3;
        assert!(pkg.validate().is_err());
        pkg.cell_count = 4;

        // Mutated target repo
        pkg.target_repo = "unauthorized/repo".into();
        assert!(pkg.validate().is_err());
        pkg.target_repo = OPERATOR_TARGET_REPO.into();

        // Mutated target main sha
        pkg.target_main_sha = "0000000000000000000000000000000000000000".into();
        assert!(pkg.validate().is_err());
        pkg.target_main_sha = FROZEN_RWE_TARGET_MAIN_SHA.into();

        // Mutated auto merge
        pkg.auto_merge_disabled = false;
        assert!(pkg.validate().is_err());
        pkg.auto_merge_disabled = true;

        // Invalid sha256
        pkg.corpus_sha256 = "not-a-sha".into();
        assert!(pkg.validate().is_err());
    }

    #[test]
    fn package_json_roundtrip_is_exact() {
        let pkg = canonical_deepseek_v2_package().unwrap();
        let json = pkg.to_json();
        let restored = FrozenCampaignPackage::from_json(&json).expect("from_json must succeed");
        assert_eq!(pkg, restored);
    }
}
