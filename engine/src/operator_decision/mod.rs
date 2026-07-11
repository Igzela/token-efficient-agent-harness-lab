use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::event_schema::canonical_event_json;

pub const OPERATOR_DECISION_SOURCE_SCHEMA_VERSION: &str = "operator_decision_source.v1";
pub const OPERATOR_DECISION_ITEM_SCHEMA_VERSION: &str = "operator_decision_item.v1";
pub const OPERATOR_DECISION_QUEUE_SCHEMA_VERSION: &str = "operator_decision_queue.v1";

const MAX_IDENTIFIER_BYTES: usize = 160;
const MAX_REASON_CODES: usize = 32;
const MAX_EVIDENCE_REFERENCES: usize = 64;
const MAX_CONTRACT_BYTES: usize = 64 * 1024;
const MIN_ACTIONABLE_CONFIDENCE: f64 = 0.5;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecisionSourceKind {
    Approval,
    Recovery,
    Rollback,
    Budget,
    Policy,
    Workflow,
    Scheduler,
    Benchmark,
}

impl OperatorDecisionSourceKind {
    pub fn precedence(&self) -> u16 {
        match self {
            Self::Approval => 800,
            Self::Recovery => 700,
            Self::Rollback => 650,
            Self::Budget => 600,
            Self::Policy => 500,
            Self::Workflow => 400,
            Self::Scheduler => 300,
            Self::Benchmark => 200,
        }
    }

    pub fn as_identifier(&self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Recovery => "recovery",
            Self::Rollback => "rollback",
            Self::Budget => "budget",
            Self::Policy => "policy",
            Self::Workflow => "workflow",
            Self::Scheduler => "scheduler",
            Self::Benchmark => "benchmark",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecisionAction {
    Approve,
    Reject,
    Pause,
    Resume,
    Retry,
    Rollback,
    Inspect,
    Acknowledge,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecisionSeverity {
    Info,
    Warning,
    Critical,
}

impl OperatorDecisionSeverity {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecisionSourceState {
    Actionable,
    Informational,
    Resolved,
    InsufficientEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecisionOutcome {
    Ready,
    Conflict,
    Expired,
    InsufficientEvidence,
    Resolved,
}

impl OperatorDecisionOutcome {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Ready => 5,
            Self::Conflict => 4,
            Self::Expired => 3,
            Self::InsufficientEvidence => 2,
            Self::Resolved => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct OperatorDecisionEvidenceReference {
    pub evidence_type: String,
    pub evidence_id: String,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OperatorDecisionSource {
    pub schema_version: String,
    pub source_kind: OperatorDecisionSourceKind,
    pub source_id: String,
    pub resource_id: String,
    pub conflict_key: String,
    pub action: OperatorDecisionAction,
    pub state: OperatorDecisionSourceState,
    pub severity: OperatorDecisionSeverity,
    pub confidence: f64,
    pub observed_at: String,
    pub expires_at: Option<String>,
    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<OperatorDecisionEvidenceReference>,
    pub evidence_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OperatorDecisionItem {
    pub schema_version: String,
    pub decision_id: String,
    pub conflict_key: String,
    pub resource_id: String,
    pub outcome: OperatorDecisionOutcome,
    pub recommended_action: Option<OperatorDecisionAction>,
    pub severity: OperatorDecisionSeverity,
    pub confidence: f64,
    pub generated_at: String,
    pub freshness_seconds: u64,
    pub expires_at: Option<String>,
    pub reason_codes: Vec<String>,
    pub selected_source: Option<OperatorDecisionEvidenceReference>,
    pub evidence_references: Vec<OperatorDecisionEvidenceReference>,
    pub content_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OperatorDecisionQueue {
    pub schema_version: String,
    pub generated_at: String,
    pub maximum_freshness_seconds: u64,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub source_counts: BTreeMap<String, usize>,
    pub items: Vec<OperatorDecisionItem>,
    pub queue_sha256: String,
}

impl OperatorDecisionSource {
    pub fn seal(&mut self) -> Result<(), String> {
        self.reason_codes.sort();
        self.reason_codes.dedup();
        self.evidence_references.sort();
        self.evidence_references.dedup();
        self.evidence_sha256.clear();
        self.evidence_sha256 = canonical_hash(self)?;
        self.validate()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != OPERATOR_DECISION_SOURCE_SCHEMA_VERSION {
            return Err("unsupported operator decision source schema version".to_string());
        }
        validate_identifier("source_id", &self.source_id)?;
        validate_identifier("resource_id", &self.resource_id)?;
        validate_identifier("conflict_key", &self.conflict_key)?;
        validate_confidence(self.confidence)?;
        let observed = parse_time("observed_at", &self.observed_at)?;
        if let Some(expires_at) = &self.expires_at {
            if parse_time("expires_at", expires_at)? <= observed {
                return Err("operator decision source expiry must follow observation".to_string());
            }
        }
        validate_reason_codes(&self.reason_codes, &self.state)?;
        validate_references(&self.evidence_references)?;
        validate_hash(&self.evidence_sha256)?;
        let mut unhashed = self.clone();
        unhashed.evidence_sha256.clear();
        if canonical_hash(&unhashed)? != self.evidence_sha256 {
            return Err("operator decision source hash mismatch".to_string());
        }
        validate_size(self)
    }
}

impl OperatorDecisionItem {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != OPERATOR_DECISION_ITEM_SCHEMA_VERSION {
            return Err("unsupported operator decision item schema version".to_string());
        }
        validate_identifier("decision_id", &self.decision_id)?;
        validate_identifier("resource_id", &self.resource_id)?;
        validate_identifier("conflict_key", &self.conflict_key)?;
        validate_confidence(self.confidence)?;
        parse_time("generated_at", &self.generated_at)?;
        if self.freshness_seconds > 30 * 24 * 60 * 60 {
            return Err("operator decision freshness exceeds contract bound".to_string());
        }
        match self.outcome {
            OperatorDecisionOutcome::Ready => {
                if self.recommended_action.is_none() || self.selected_source.is_none() {
                    return Err("ready decision requires an action and selected source".to_string());
                }
            }
            OperatorDecisionOutcome::Conflict
            | OperatorDecisionOutcome::Expired
            | OperatorDecisionOutcome::InsufficientEvidence
            | OperatorDecisionOutcome::Resolved => {
                if self.recommended_action.is_some() || self.selected_source.is_some() {
                    return Err(
                        "non-ready decision must not recommend an action or selected source"
                            .to_string(),
                    );
                }
            }
        }
        if self.reason_codes.is_empty() {
            return Err("operator decision item requires reason codes".to_string());
        }
        validate_references(&self.evidence_references)?;
        if let Some(selected) = &self.selected_source {
            validate_reference(selected)?;
            if !self.evidence_references.contains(selected) {
                return Err("selected source must be present in evidence references".to_string());
            }
        }
        validate_hash(&self.content_sha256)?;
        let mut unhashed = self.clone();
        unhashed.content_sha256.clear();
        if canonical_hash(&unhashed)? != self.content_sha256 {
            return Err("operator decision item hash mismatch".to_string());
        }
        validate_size(self)
    }
}

impl OperatorDecisionQueue {
    pub fn seal(&mut self) -> Result<(), String> {
        self.queue_sha256.clear();
        self.queue_sha256 = canonical_hash(self)?;
        self.validate()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != OPERATOR_DECISION_QUEUE_SCHEMA_VERSION {
            return Err("unsupported operator decision queue schema version".to_string());
        }
        parse_time("generated_at", &self.generated_at)?;
        if self.maximum_freshness_seconds == 0
            || self.maximum_freshness_seconds > 30 * 24 * 60 * 60
        {
            return Err(
                "operator decision queue freshness is outside the contract bound".to_string(),
            );
        }
        if self.limit == 0
            || self.limit > 100
            || self.offset > 10_000
            || self.items.len() > self.limit
        {
            return Err("operator decision queue pagination is outside bounds".to_string());
        }
        if self.total < self.items.len() || self.source_counts.len() > 8 {
            return Err("operator decision queue counts are invalid".to_string());
        }
        for item in &self.items {
            item.validate()?;
        }
        validate_hash(&self.queue_sha256)?;
        let mut unhashed = self.clone();
        unhashed.queue_sha256.clear();
        if canonical_hash(&unhashed)? != self.queue_sha256 {
            return Err("operator decision queue hash mismatch".to_string());
        }
        validate_size(self)
    }
}

pub fn derive_operator_decision_item(
    conflict_key: &str,
    sources: &[OperatorDecisionSource],
    generated_at: &str,
    maximum_freshness_seconds: u64,
) -> Result<OperatorDecisionItem, String> {
    validate_identifier("conflict_key", conflict_key)?;
    let generated = parse_time("generated_at", generated_at)?;
    if maximum_freshness_seconds == 0 || maximum_freshness_seconds > 30 * 24 * 60 * 60 {
        return Err("maximum decision freshness is outside the contract bound".to_string());
    }

    let mut candidates = sources
        .iter()
        .filter(|source| source.conflict_key == conflict_key)
        .cloned()
        .collect::<Vec<_>>();
    for source in &candidates {
        source.validate()?;
    }

    let resources = candidates
        .iter()
        .map(|source| source.resource_id.as_str())
        .collect::<BTreeSet<_>>();
    if resources.len() > 1 {
        return Err("operator decision conflict key spans multiple resources".to_string());
    }

    let mut identities = BTreeMap::new();
    for source in &candidates {
        let identity = (source.source_kind.clone(), source.source_id.clone());
        if identities
            .insert(identity, source.evidence_sha256.clone())
            .is_some_and(|existing| existing != source.evidence_sha256)
        {
            return Err("conflicting duplicate operator decision source".to_string());
        }
    }

    candidates.sort_by(source_order);
    candidates.dedup_by(|left, right| {
        left.source_kind == right.source_kind && left.source_id == right.source_id
    });

    let references = candidates
        .iter()
        .map(source_reference)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(MAX_EVIDENCE_REFERENCES)
        .collect::<Vec<_>>();
    let resource_id = candidates
        .first()
        .map(|source| source.resource_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let severity = candidates
        .iter()
        .map(|source| source.severity.clone())
        .max()
        .unwrap_or(OperatorDecisionSeverity::Info);
    let newest_observed = candidates
        .iter()
        .filter_map(|source| parse_time("observed_at", &source.observed_at).ok())
        .max();
    let freshness_seconds = newest_observed
        .map(|observed| (generated - observed).num_seconds().max(0) as u64)
        .unwrap_or(0);

    let active = candidates
        .iter()
        .filter(|source| {
            matches!(source.state, OperatorDecisionSourceState::Actionable)
                && source.confidence >= MIN_ACTIONABLE_CONFIDENCE
                && source.expires_at.as_ref().is_none_or(|expires| {
                    parse_time("expires_at", expires).is_ok_and(|time| time > generated)
                })
                && parse_time("observed_at", &source.observed_at).is_ok_and(|observed| {
                    generated >= observed
                        && (generated - observed).num_seconds() as u64
                            <= maximum_freshness_seconds
                })
        })
        .collect::<Vec<_>>();

    let (outcome, action, selected, reason_codes) = if candidates.is_empty() {
        (
            OperatorDecisionOutcome::InsufficientEvidence,
            None,
            None,
            vec!["no_matching_sources".to_string()],
        )
    } else if candidates
        .iter()
        .all(|source| matches!(source.state, OperatorDecisionSourceState::Resolved))
    {
        (
            OperatorDecisionOutcome::Resolved,
            None,
            None,
            vec!["all_sources_resolved".to_string()],
        )
    } else if active.is_empty() {
        let any_expired = candidates.iter().any(|source| {
            source.expires_at.as_ref().is_some_and(|expires| {
                parse_time("expires_at", expires).is_ok_and(|time| time <= generated)
            })
        });
        if any_expired {
            (
                OperatorDecisionOutcome::Expired,
                None,
                None,
                vec!["all_actionable_sources_expired_or_stale".to_string()],
            )
        } else {
            (
                OperatorDecisionOutcome::InsufficientEvidence,
                None,
                None,
                vec!["no_fresh_confident_actionable_source".to_string()],
            )
        }
    } else {
        let top = active[0];
        let conflicting = active.iter().skip(1).any(|candidate| {
            candidate.severity == top.severity
                && candidate.source_kind.precedence() == top.source_kind.precedence()
                && candidate.confidence == top.confidence
                && candidate.action != top.action
        });
        if conflicting {
            (
                OperatorDecisionOutcome::Conflict,
                None,
                None,
                vec!["equal_precedence_action_conflict".to_string()],
            )
        } else {
            (
                OperatorDecisionOutcome::Ready,
                Some(top.action.clone()),
                Some(source_reference(top)),
                vec!["highest_precedence_fresh_source_selected".to_string()],
            )
        }
    };

    let selected_source = selected.as_ref().and_then(|reference| {
        candidates
            .iter()
            .find(|source| source_matches_reference(source, reference))
    });
    let confidence = selected_source.map(|source| source.confidence).unwrap_or(0.0);
    let expires_at = selected_source.and_then(|source| source.expires_at.clone());
    let decision_id = decision_id(conflict_key, &references)?;
    let mut item = OperatorDecisionItem {
        schema_version: OPERATOR_DECISION_ITEM_SCHEMA_VERSION.to_string(),
        decision_id,
        conflict_key: conflict_key.to_string(),
        resource_id,
        outcome,
        recommended_action: action,
        severity,
        confidence,
        generated_at: generated_at.to_string(),
        freshness_seconds,
        expires_at,
        reason_codes,
        selected_source: selected,
        evidence_references: references,
        content_sha256: String::new(),
    };
    item.content_sha256 = canonical_hash(&item)?;
    item.validate()?;
    Ok(item)
}

fn source_matches_reference(
    source: &OperatorDecisionSource,
    reference: &OperatorDecisionEvidenceReference,
) -> bool {
    source.source_kind.as_identifier() == reference.evidence_type
        && source.source_id == reference.evidence_id
}

fn source_order(left: &OperatorDecisionSource, right: &OperatorDecisionSource) -> Ordering {
    right
        .severity
        .rank()
        .cmp(&left.severity.rank())
        .then_with(|| {
            right
                .source_kind
                .precedence()
                .cmp(&left.source_kind.precedence())
        })
        .then_with(|| right.confidence.total_cmp(&left.confidence))
        .then_with(|| right.observed_at.cmp(&left.observed_at))
        .then_with(|| left.source_kind.cmp(&right.source_kind))
        .then_with(|| left.source_id.cmp(&right.source_id))
}

fn source_reference(source: &OperatorDecisionSource) -> OperatorDecisionEvidenceReference {
    OperatorDecisionEvidenceReference {
        evidence_type: source.source_kind.as_identifier().to_string(),
        evidence_id: source.source_id.clone(),
        content_sha256: Some(source.evidence_sha256.clone()),
    }
}

fn decision_id(
    conflict_key: &str,
    references: &[OperatorDecisionEvidenceReference],
) -> Result<String, String> {
    let encoded = serde_json::to_vec(&(conflict_key, references)).map_err(|error| error.to_string())?;
    Ok(format!("operator-decision-{:x}", Sha256::digest(encoded)))
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let canonical = canonical_event_json(&value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}

fn parse_time(field: &str, value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value).map_err(|_| format!("{field} must be RFC3339"))
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.chars().any(char::is_whitespace)
    {
        return Err(format!(
            "{field} must be a bounded non-whitespace identifier"
        ));
    }
    Ok(())
}

fn validate_confidence(value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("operator decision confidence must be within [0,1]".to_string());
    }
    Ok(())
}

fn validate_reason_codes(
    codes: &[String],
    state: &OperatorDecisionSourceState,
) -> Result<(), String> {
    if codes.len() > MAX_REASON_CODES
        || codes
            .iter()
            .any(|code| code.is_empty() || code.len() > 80 || code.chars().any(char::is_whitespace))
        || codes.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(
            "operator decision reason codes must be sorted unique bounded identifiers".to_string(),
        );
    }
    if matches!(state, OperatorDecisionSourceState::InsufficientEvidence) && codes.is_empty() {
        return Err("insufficient decision evidence requires reason codes".to_string());
    }
    Ok(())
}

fn validate_reference(reference: &OperatorDecisionEvidenceReference) -> Result<(), String> {
    validate_identifier("evidence_type", &reference.evidence_type)?;
    validate_identifier("evidence_id", &reference.evidence_id)?;
    if let Some(hash) = &reference.content_sha256 {
        validate_hash(hash)?;
    }
    Ok(())
}

fn validate_references(references: &[OperatorDecisionEvidenceReference]) -> Result<(), String> {
    if references.len() > MAX_EVIDENCE_REFERENCES {
        return Err("operator decision evidence references exceed bound".to_string());
    }
    for reference in references {
        validate_reference(reference)?;
    }
    if references.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(
            "operator decision evidence references must be sorted and unique".to_string(),
        );
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("operator decision hash must be 64 hexadecimal characters".to_string());
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T) -> Result<(), String> {
    if serde_json::to_vec(value)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_CONTRACT_BYTES
    {
        return Err("operator decision contract exceeds size bound".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(
        kind: OperatorDecisionSourceKind,
        id: &str,
        action: OperatorDecisionAction,
    ) -> OperatorDecisionSource {
        let mut source = OperatorDecisionSource {
            schema_version: OPERATOR_DECISION_SOURCE_SCHEMA_VERSION.to_string(),
            source_kind: kind,
            source_id: id.to_string(),
            resource_id: "run-1".to_string(),
            conflict_key: "run-1:control".to_string(),
            action,
            state: OperatorDecisionSourceState::Actionable,
            severity: OperatorDecisionSeverity::Warning,
            confidence: 0.9,
            observed_at: "2026-07-11T00:00:00Z".to_string(),
            expires_at: Some("2026-07-11T01:00:00Z".to_string()),
            reason_codes: vec!["operator_attention_required".to_string()],
            evidence_references: vec![],
            evidence_sha256: String::new(),
        };
        source.seal().unwrap();
        source
    }

    #[test]
    fn source_contract_round_trips_and_rejects_tamper() {
        let source = source(
            OperatorDecisionSourceKind::Approval,
            "approval-1",
            OperatorDecisionAction::Approve,
        );
        source.validate().unwrap();
        let encoded = serde_json::to_string(&source).unwrap();
        let mut decoded: OperatorDecisionSource = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, source);
        decoded.confidence = 0.8;
        assert!(decoded.validate().unwrap_err().contains("hash mismatch"));
    }

    #[test]
    fn precedence_deduplication_and_ordering_are_deterministic() {
        let approval = source(
            OperatorDecisionSourceKind::Approval,
            "approval-1",
            OperatorDecisionAction::Approve,
        );
        let budget = source(
            OperatorDecisionSourceKind::Budget,
            "budget-1",
            OperatorDecisionAction::Pause,
        );
        let first = derive_operator_decision_item(
            "run-1:control",
            &[budget.clone(), approval.clone(), approval.clone()],
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap();
        let second = derive_operator_decision_item(
            "run-1:control",
            &[approval, budget],
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.outcome, OperatorDecisionOutcome::Ready);
        assert_eq!(
            first.recommended_action,
            Some(OperatorDecisionAction::Approve)
        );
        assert_eq!(first.evidence_references.len(), 2);
    }

    #[test]
    fn shared_source_id_across_kinds_uses_exact_selected_identity() {
        let mut approval = source(
            OperatorDecisionSourceKind::Approval,
            "shared-1",
            OperatorDecisionAction::Approve,
        );
        approval.confidence = 0.71;
        approval.seal().unwrap();
        let mut budget = source(
            OperatorDecisionSourceKind::Budget,
            "shared-1",
            OperatorDecisionAction::Pause,
        );
        budget.confidence = 0.99;
        budget.seal().unwrap();
        let item = derive_operator_decision_item(
            "run-1:control",
            &[budget, approval],
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap();
        assert_eq!(item.recommended_action, Some(OperatorDecisionAction::Approve));
        assert_eq!(item.confidence, 0.71);
        assert_eq!(
            item.selected_source.as_ref().unwrap().evidence_type,
            "approval"
        );
    }

    #[test]
    fn equal_precedence_action_conflict_fails_closed() {
        let approve = source(
            OperatorDecisionSourceKind::Approval,
            "approval-a",
            OperatorDecisionAction::Approve,
        );
        let reject = source(
            OperatorDecisionSourceKind::Approval,
            "approval-b",
            OperatorDecisionAction::Reject,
        );
        let item = derive_operator_decision_item(
            "run-1:control",
            &[approve, reject],
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap();
        assert_eq!(item.outcome, OperatorDecisionOutcome::Conflict);
        assert!(item.recommended_action.is_none());
        assert!(item.selected_source.is_none());
    }

    #[test]
    fn conflicting_duplicate_identity_is_rejected_in_every_input_order() {
        let approve = source(
            OperatorDecisionSourceKind::Approval,
            "approval-1",
            OperatorDecisionAction::Approve,
        );
        let mut reject = approve.clone();
        reject.action = OperatorDecisionAction::Reject;
        reject.seal().unwrap();
        for inputs in [
            vec![approve.clone(), reject.clone()],
            vec![reject.clone(), approve.clone()],
        ] {
            assert!(derive_operator_decision_item(
                "run-1:control",
                &inputs,
                "2026-07-11T00:05:00Z",
                600
            )
            .unwrap_err()
            .contains("conflicting duplicate"));
        }
    }

    #[test]
    fn expiry_staleness_low_confidence_and_missing_sources_are_explicit() {
        let mut expired = source(
            OperatorDecisionSourceKind::Workflow,
            "workflow-1",
            OperatorDecisionAction::Retry,
        );
        expired.expires_at = Some("2026-07-11T00:02:00Z".to_string());
        expired.seal().unwrap();
        assert_eq!(
            derive_operator_decision_item("run-1:control", &[expired], "2026-07-11T00:05:00Z", 600)
                .unwrap()
                .outcome,
            OperatorDecisionOutcome::Expired
        );
        let mut low = source(
            OperatorDecisionSourceKind::Budget,
            "budget-low",
            OperatorDecisionAction::Pause,
        );
        low.confidence = 0.4;
        low.seal().unwrap();
        assert_eq!(
            derive_operator_decision_item("run-1:control", &[low], "2026-07-11T00:05:00Z", 600)
                .unwrap()
                .outcome,
            OperatorDecisionOutcome::InsufficientEvidence
        );
        assert_eq!(
            derive_operator_decision_item("other:control", &[], "2026-07-11T00:05:00Z", 600)
                .unwrap()
                .outcome,
            OperatorDecisionOutcome::InsufficientEvidence
        );
    }

    #[test]
    fn resolved_sources_never_recommend_actions() {
        let mut resolved = source(
            OperatorDecisionSourceKind::Recovery,
            "recovery-1",
            OperatorDecisionAction::Resume,
        );
        resolved.state = OperatorDecisionSourceState::Resolved;
        resolved.seal().unwrap();
        let item = derive_operator_decision_item(
            "run-1:control",
            &[resolved],
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap();
        assert_eq!(item.outcome, OperatorDecisionOutcome::Resolved);
        assert!(item.recommended_action.is_none());
    }
}
