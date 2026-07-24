//! RWE live-gate evaluation and provider-free validation.

use serde_json::{json, Value};

use super::corpus::{freeze_first_rwe_corpus, FirstRweCorpus};

pub const RWE_RUN_AUTH_SCHEMA: &str = "rwe_run_authorization.v1";

/// Separately persisted operator spend envelope for multi-task RWE.
#[derive(Debug, Clone, PartialEq)]
pub struct RweRunAuthorization {
    pub schema_version: String,
    pub authorization_id: String,
    pub corpus_sha256: String,
    pub principal_id: String,
    pub principal_kind: String,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority_kind: String,
    pub expires_at: String,
    pub active: bool,
}

impl RweRunAuthorization {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "authorization_id": self.authorization_id,
            "corpus_sha256": self.corpus_sha256,
            "principal_id": self.principal_id,
            "principal_kind": self.principal_kind,
            "max_total_provider_requests": self.max_total_provider_requests,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "cost_authority_kind": self.cost_authority_kind,
            "expires_at": self.expires_at,
            "active": self.active,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RweLiveGateResult {
    ReadyAuthorized,
    BlockedMissingRweSpendAuthorization,
    BlockedCorpusMismatch,
    BlockedPrincipal,
    BlockedExpired,
}

impl RweLiveGateResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadyAuthorized => "ready_authorized",
            Self::BlockedMissingRweSpendAuthorization => "blocked_missing_rwe_spend_authorization",
            Self::BlockedCorpusMismatch => "blocked_corpus_mismatch",
            Self::BlockedPrincipal => "blocked_principal",
            Self::BlockedExpired => "blocked_expired",
        }
    }

    pub fn allows_live_rwe(&self) -> bool {
        matches!(self, Self::ReadyAuthorized)
    }
}

/// Evaluate whether live RWE may run. Golden Path auth alone is insufficient.
pub fn evaluate_rwe_live_gate(
    corpus: &FirstRweCorpus,
    auth: Option<&RweRunAuthorization>,
    now: &str,
) -> RweLiveGateResult {
    let Some(auth) = auth else {
        return RweLiveGateResult::BlockedMissingRweSpendAuthorization;
    };
    if !auth.active {
        return RweLiveGateResult::BlockedMissingRweSpendAuthorization;
    }
    if auth.corpus_sha256 != corpus.corpus_sha256 {
        return RweLiveGateResult::BlockedCorpusMismatch;
    }
    if auth.principal_kind == "fixture_principal" {
        return RweLiveGateResult::BlockedPrincipal;
    }
    if auth.expires_at.as_str() < now {
        return RweLiveGateResult::BlockedExpired;
    }
    if auth.max_total_provider_requests == 0 || auth.max_retries_invalid() {
        return RweLiveGateResult::BlockedMissingRweSpendAuthorization;
    }
    RweLiveGateResult::ReadyAuthorized
}

impl RweRunAuthorization {
    fn max_retries_invalid(&self) -> bool {
        // Placeholder: envelope must be positive and finite.
        self.max_total_tokens == 0 || self.max_wall_time_ms == 0
    }
}

/// Provider-free validation of corpus + gate without live calls.
pub fn validate_rwe_provider_free_prep() -> Value {
    let corpus = freeze_first_rwe_corpus();
    let gate = evaluate_rwe_live_gate(&corpus, None, "2026-07-25T12:00:00Z");
    json!({
        "schema_version": "rwe_provider_free_prep.v1",
        "corpus_sha256": corpus.corpus_sha256,
        "task_count": corpus.tasks.len(),
        "live_gate": gate.as_str(),
        "allows_live_rwe": gate.allows_live_rwe(),
        "live_provider_request": false,
        "manual_gate": "persist separate RweRunAuthorization with operator principal and spend envelope",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rwe_gate_blocks_without_separate_authorization() {
        let corpus = freeze_first_rwe_corpus();
        let gate = evaluate_rwe_live_gate(&corpus, None, "2026-07-25T12:00:00Z");
        assert_eq!(gate, RweLiveGateResult::BlockedMissingRweSpendAuthorization);
        assert!(!gate.allows_live_rwe());
        let prep = validate_rwe_provider_free_prep();
        assert_eq!(prep["live_provider_request"], false);
        assert_eq!(prep["allows_live_rwe"], false);
    }

    #[test]
    fn rwe_gate_rejects_fixture_principal_and_corpus_mismatch() {
        let corpus = freeze_first_rwe_corpus();
        let auth = RweRunAuthorization {
            schema_version: RWE_RUN_AUTH_SCHEMA.into(),
            authorization_id: "rwe-auth-1".into(),
            corpus_sha256: corpus.corpus_sha256.clone(),
            principal_id: "fixture-principal-x".into(),
            principal_kind: "fixture_principal".into(),
            max_total_provider_requests: 5,
            max_total_tokens: 100_000,
            max_wall_time_ms: 3_600_000,
            cost_authority_kind: "cost_unavailable".into(),
            expires_at: "2026-08-01T00:00:00Z".into(),
            active: true,
        };
        assert_eq!(
            evaluate_rwe_live_gate(&corpus, Some(&auth), "2026-07-25T12:00:00Z"),
            RweLiveGateResult::BlockedPrincipal
        );
        let mut bad = auth.clone();
        bad.principal_kind = "operator_api_key".into();
        bad.principal_id = "key-real".into();
        bad.corpus_sha256 = "00".repeat(32);
        assert_eq!(
            evaluate_rwe_live_gate(&corpus, Some(&bad), "2026-07-25T12:00:00Z"),
            RweLiveGateResult::BlockedCorpusMismatch
        );
    }
}
