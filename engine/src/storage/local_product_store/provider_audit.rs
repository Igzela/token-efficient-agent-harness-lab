use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::Digest;

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

type ProviderReservationBinding = (String, String, String, Option<f64>, Option<String>, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderEmbeddingReceiptState {
    PreflightReserved,
    Reserved,
    Sending,
    NetworkSucceeded,
    Succeeded,
    ResultErased,
    FailedBeforeSend,
    FailedKnownOutcome,
    OutcomeUnknown,
    OutcomeUnknownAcknowledged,
    RetryAuthorized,
}

impl ProviderEmbeddingReceiptState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PreflightReserved => "preflight_reserved",
            Self::Reserved => "reserved",
            Self::Sending => "sending",
            Self::NetworkSucceeded => "network_succeeded",
            Self::Succeeded => "succeeded",
            Self::ResultErased => "result_erased",
            Self::FailedBeforeSend => "failed_before_send",
            Self::FailedKnownOutcome => "failed_known_outcome",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::OutcomeUnknownAcknowledged => "outcome_unknown_acknowledged",
            Self::RetryAuthorized => "retry_authorized",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "preflight_reserved" => Ok(Self::PreflightReserved),
            "reserved" => Ok(Self::Reserved),
            "sending" => Ok(Self::Sending),
            "network_succeeded" => Ok(Self::NetworkSucceeded),
            "succeeded" => Ok(Self::Succeeded),
            "result_erased" => Ok(Self::ResultErased),
            "failed_before_send" => Ok(Self::FailedBeforeSend),
            "failed_known_outcome" => Ok(Self::FailedKnownOutcome),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            "outcome_unknown_acknowledged" => Ok(Self::OutcomeUnknownAcknowledged),
            "retry_authorized" => Ok(Self::RetryAuthorized),
            _ => Err("provider embedding operation state is invalid".to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderEmbeddingErrorDomain {
    FailedBeforeSend,
    OutcomeUnknown,
    OutcomeUnknownRedirect,
    OutcomeUnknownOversized,
    OutcomeUnknownTruncated,
    OutcomeUnknownMalformed,
    OutcomeUnknownTimeout,
    OutcomeUnknownConnection,
    Auth,
    Timeout,
    CircuitOpen,
    KillSwitch,
    Pricing,
    Error,
}

impl ProviderEmbeddingErrorDomain {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FailedBeforeSend => "provider_failed_before_send",
            Self::OutcomeUnknown => "provider_outcome_unknown",
            Self::OutcomeUnknownRedirect => "provider_outcome_unknown_redirect",
            Self::OutcomeUnknownOversized => "provider_outcome_unknown_oversized",
            Self::OutcomeUnknownTruncated => "provider_outcome_unknown_truncated",
            Self::OutcomeUnknownMalformed => "provider_outcome_unknown_malformed",
            Self::OutcomeUnknownTimeout => "provider_outcome_unknown_timeout",
            Self::OutcomeUnknownConnection => "provider_outcome_unknown_connection",
            Self::Auth => "provider_auth",
            Self::Timeout => "provider_timeout",
            Self::CircuitOpen => "provider_circuit_open",
            Self::KillSwitch => "provider_kill_switch",
            Self::Pricing => "provider_pricing",
            Self::Error => "provider_error",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "provider_failed_before_send" => Some(Self::FailedBeforeSend),
            "provider_outcome_unknown" => Some(Self::OutcomeUnknown),
            "provider_outcome_unknown_redirect" => Some(Self::OutcomeUnknownRedirect),
            "provider_outcome_unknown_oversized" => Some(Self::OutcomeUnknownOversized),
            "provider_outcome_unknown_truncated" => Some(Self::OutcomeUnknownTruncated),
            "provider_outcome_unknown_malformed" => Some(Self::OutcomeUnknownMalformed),
            "provider_outcome_unknown_timeout" => Some(Self::OutcomeUnknownTimeout),
            "provider_outcome_unknown_connection" => Some(Self::OutcomeUnknownConnection),
            "provider_auth" => Some(Self::Auth),
            "provider_timeout" => Some(Self::Timeout),
            "provider_circuit_open" => Some(Self::CircuitOpen),
            "provider_kill_switch" => Some(Self::KillSwitch),
            "provider_pricing" => Some(Self::Pricing),
            "provider_error" => Some(Self::Error),
            _ => None,
        }
    }

    pub(crate) fn is_unknown_outcome(self) -> bool {
        matches!(
            self,
            Self::OutcomeUnknown
                | Self::OutcomeUnknownRedirect
                | Self::OutcomeUnknownOversized
                | Self::OutcomeUnknownTruncated
                | Self::OutcomeUnknownMalformed
                | Self::OutcomeUnknownTimeout
                | Self::OutcomeUnknownConnection
        )
    }

    pub(crate) fn is_known_outcome(self) -> bool {
        matches!(
            self,
            Self::Auth
                | Self::Timeout
                | Self::CircuitOpen
                | Self::KillSwitch
                | Self::Pricing
                | Self::Error
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderEmbeddingOperation {
    pub operation_id: String,
    pub operation_kind: String,
    pub target_memory_id: String,
    pub target_version: i64,
    pub tenant_id: String,
    pub workspace_id: String,
    pub agent_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub source_id: String,
    pub source_sha256: String,
    pub node_id: Option<String>,
    pub query_sha256: Option<String>,
    pub request_identity_sha256: String,
    pub operation_binding_sha256: String,
    pub content_sha256: String,
    pub contract_json: String,
    pub contract_sha256: String,
    pub receipt_sha256: String,
    pub provider_id: String,
    pub requested_model_id: String,
    pub resolved_model_id: String,
    pub dimensions: i64,
    pub created_at: String,
}

#[derive(Debug)]
pub(crate) enum ProviderEmbeddingOperationClaim {
    PreflightReserved {
        attempt_count: i64,
    },
    Claimed {
        attempt_count: i64,
    },
    RetryAuthorized {
        attempt_count: i64,
    },
    Completed {
        vector_json: String,
        metadata_json: String,
    },
}

#[derive(Debug)]
struct StoredEmbeddingOperation {
    operation_id: String,
    operation_kind: String,
    tenant_id: String,
    workspace_id: String,
    agent_id: Option<String>,
    run_id: Option<String>,
    task_id: Option<String>,
    source_id: String,
    source_sha256: String,
    node_id: Option<String>,
    query_sha256: Option<String>,
    request_identity_sha256: String,
    operation_binding_sha256: String,
    content_sha256: String,
    contract_json: String,
    contract_sha256: String,
    receipt_sha256: String,
    provider_id: String,
    requested_model_id: String,
    resolved_model_id: String,
    dimensions: i64,
    state: ProviderEmbeddingReceiptState,
    attempt_count: i64,
    vector_json: Option<String>,
    metadata_json: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEmbeddingResolutionAction {
    RetryFailed,
    AcknowledgeUnknown,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEmbeddingResolutionRequest {
    pub target_version: i64,
    pub expected_attempt_count: i64,
    pub scope: super::durable_memory::MemoryScope,
    pub run_id: Option<String>,
    pub action: ProviderEmbeddingResolutionAction,
    pub evidence_source_id: Option<String>,
    pub evidence_sha256: Option<String>,
    pub confirm_resolution: bool,
}

impl LocalProductStore {
    pub fn authorized_provider_embedding_receipt_evidence(
        &self,
        limit: i64,
        visibility: super::ProviderEmbeddingReceiptVisibility,
    ) -> Result<Vec<Value>, String> {
        match visibility {
            super::ProviderEmbeddingReceiptVisibility::TenantOperator { tenant_id } => {
                self.provider_embedding_receipt_evidence(limit, &tenant_id)
            }
            super::ProviderEmbeddingReceiptVisibility::Hidden => Ok(Vec::new()),
        }
    }

    fn provider_embedding_receipt_evidence(
        &self,
        limit: i64,
        tenant_id: &str,
    ) -> Result<Vec<Value>, String> {
        let limit = limit.clamp(1, 100);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn.prepare(
                    "SELECT o.operation_id,o.operation_kind,o.tenant_id,o.workspace_id,o.run_id,
                            o.node_id,o.provider_id,o.requested_model_id,o.resolved_model_id,o.dimensions,
                            o.state,o.attempt_count,o.receipt_sha256,o.request_identity_sha256,
                            o.reservation_event_id,o.send_event_id,o.outcome_event_id,o.result_kind,
                            o.result_id,o.result_sha256,e.error_domain,o.created_at,o.updated_at
                     FROM provider_embedding_operations o
                     LEFT JOIN provider_audit_events e ON e.event_id=o.outcome_event_id
                     WHERE o.tenant_id=?1
                     ORDER BY o.updated_at DESC,o.operation_id DESC LIMIT ?2",
                ).map_err(|error|error.to_string())?;
                let rows = statement.query_map(rusqlite::params![tenant_id, limit], |row| Ok(json!({
                    "operation_id":row.get::<_,String>(0)?,"operation_kind":row.get::<_,String>(1)?,
                    "tenant_id":row.get::<_,String>(2)?,"workspace_id":row.get::<_,String>(3)?,
                    "run_id":row.get::<_,Option<String>>(4)?,"node_id":row.get::<_,Option<String>>(5)?,
                    "provider_id":row.get::<_,String>(6)?,"requested_model_id":row.get::<_,String>(7)?,
                    "resolved_model_id":row.get::<_,String>(8)?,"dimensions":row.get::<_,i64>(9)?,
                    "state":row.get::<_,String>(10)?,"attempt_count":row.get::<_,i64>(11)?,
                    "receipt_sha256":row.get::<_,String>(12)?,"request_identity_sha256":row.get::<_,String>(13)?,
                    "reservation_event_id":row.get::<_,String>(14)?,"send_event_id":row.get::<_,Option<String>>(15)?,
                    "outcome_event_id":row.get::<_,Option<String>>(16)?,"result_kind":row.get::<_,Option<String>>(17)?,
                    "result_id":row.get::<_,Option<String>>(18)?,"result_sha256":row.get::<_,Option<String>>(19)?,
                    "error_domain":row.get::<_,Option<String>>(20)?,"created_at":row.get::<_,String>(21)?,
                    "updated_at":row.get::<_,String>(22)?,"redacted":true,
                }))).map_err(|error|error.to_string())?;
                rows.map(|row|row.map_err(|error|error.to_string())).collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.query(
                    "SELECT o.operation_id,o.operation_kind,o.tenant_id,o.workspace_id,o.run_id,
                            o.node_id,o.provider_id,o.requested_model_id,o.resolved_model_id,o.dimensions,
                            o.state,o.attempt_count,o.receipt_sha256,o.request_identity_sha256,
                            o.reservation_event_id,o.send_event_id,o.outcome_event_id,o.result_kind,
                            o.result_id,o.result_sha256,e.error_domain,o.created_at::TEXT,o.updated_at::TEXT
                     FROM provider_embedding_operations o
                     LEFT JOIN provider_audit_events e ON e.event_id=o.outcome_event_id
                     WHERE o.tenant_id=$1
                     ORDER BY o.updated_at DESC,o.operation_id DESC LIMIT $2", &[&tenant_id, &limit],
                ).map_err(|error|error.to_string()).map(|rows| rows.iter().map(|row| json!({
                    "operation_id":row.get::<_,String>(0),"operation_kind":row.get::<_,String>(1),
                    "tenant_id":row.get::<_,String>(2),"workspace_id":row.get::<_,String>(3),
                    "run_id":row.get::<_,Option<String>>(4),"node_id":row.get::<_,Option<String>>(5),
                    "provider_id":row.get::<_,String>(6),"requested_model_id":row.get::<_,String>(7),
                    "resolved_model_id":row.get::<_,String>(8),"dimensions":row.get::<_,i64>(9),
                    "state":row.get::<_,String>(10),"attempt_count":row.get::<_,i64>(11),
                    "receipt_sha256":row.get::<_,String>(12),"request_identity_sha256":row.get::<_,String>(13),
                    "reservation_event_id":row.get::<_,String>(14),"send_event_id":row.get::<_,Option<String>>(15),
                    "outcome_event_id":row.get::<_,Option<String>>(16),"result_kind":row.get::<_,Option<String>>(17),
                    "result_id":row.get::<_,Option<String>>(18),"result_sha256":row.get::<_,Option<String>>(19),
                    "error_domain":row.get::<_,Option<String>>(20),"created_at":row.get::<_,String>(21),
                    "updated_at":row.get::<_,String>(22),"redacted":true,
                })).collect())
            }),
        }
    }

    pub(crate) fn reserve_provider_embedding_preflight(
        &self,
        operation: &ProviderEmbeddingOperation,
        preflight: &crate::provider::ProviderAuditEvent,
    ) -> Result<ProviderEmbeddingOperationClaim, String> {
        validate_embedding_operation(operation)?;
        if preflight.event_type != "contract_check_reserved"
            || preflight.provider_id != operation.provider_id
            || preflight.cost.is_some()
            || preflight.currency.is_some()
            || preflight.redaction_status != "redacted"
        {
            return Err("invalid provider embedding preflight audit event".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                ).map_err(|error|error.to_string())?;
                if let Some(existing) = sqlite_embedding_operation(&tx, operation)? {
                    let attempt = existing.attempt_count;
                    match validate_stored_embedding_operation(operation, existing)? {
                        ProviderEmbeddingOperationClaim::RetryAuthorized { .. } => {
                            let event = retry_audit_event(preflight, attempt);
                            insert_provider_event_sqlite(&tx, &event)?;
                            let changed=tx.execute(
                                "UPDATE provider_embedding_operations
                                 SET state='preflight_reserved',reservation_event_id=?1,
                                     send_event_id=NULL,outcome_event_id=NULL,updated_at=?2
                                 WHERE operation_id=?3 AND state='retry_authorized' AND attempt_count=?4",
                                params![event.event_id,event.created_at,operation.operation_id,attempt],
                            ).map_err(|error|error.to_string())?;
                            require_single_embedding_operation_update(changed)?;
                            tx.commit().map_err(|error|error.to_string())?;
                            return Ok(ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count: attempt });
                        }
                        existing => return Ok(existing),
                    }
                }
                insert_provider_event_sqlite(&tx, preflight)?;
                insert_embedding_operation_sqlite(
                    &tx,operation,&preflight.event_id,ProviderEmbeddingReceiptState::PreflightReserved,
                )?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count: 1 })
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx=client.transaction().map_err(|error|error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&format!("{}:{}",operation.target_memory_id,operation.target_version)])
                    .map_err(|error|error.to_string())?;
                if let Some(existing)=pg_embedding_operation(&mut tx,operation)? {
                    let attempt=existing.attempt_count;
                    match validate_stored_embedding_operation(operation,existing)? {
                        ProviderEmbeddingOperationClaim::RetryAuthorized { .. } => {
                            let event=retry_audit_event(preflight,attempt);
                            insert_provider_event_pg(&mut tx,&event)?;
                            let changed=tx.execute(
                                "UPDATE provider_embedding_operations
                                 SET state='preflight_reserved',reservation_event_id=$1,
                                     send_event_id=NULL,outcome_event_id=NULL,updated_at=$2
                                 WHERE operation_id=$3 AND state='retry_authorized' AND attempt_count=$4",
                                &[&event.event_id,&event.created_at,&operation.operation_id,&attempt],
                            ).map_err(|error|error.to_string())?;
                            require_single_embedding_operation_update(changed as usize)?;
                            tx.commit().map_err(|error|error.to_string())?;
                            return Ok(ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count: attempt });
                        }
                        existing => return Ok(existing),
                    }
                }
                insert_provider_event_pg(&mut tx,preflight)?;
                insert_embedding_operation_pg(
                    &mut tx,operation,&preflight.event_id,ProviderEmbeddingReceiptState::PreflightReserved,
                )?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count: 1 })
            }),
        }
    }

    pub fn reconcile_provider_embedding_operation(
        &self,
        memory_id: &str,
        request: &ProviderEmbeddingResolutionRequest,
        actor: &str,
    ) -> Result<Value, String> {
        if memory_id.is_empty() || actor.is_empty() || request.target_version <= 0 {
            return Err("invalid provider embedding reconciliation binding".to_string());
        }
        if !request.confirm_resolution {
            return Err(
                "provider embedding reconciliation requires explicit confirmation".to_string(),
            );
        }
        let evidence_sha256 = match request.action {
            ProviderEmbeddingResolutionAction::RetryFailed => {
                if request.evidence_source_id.is_some() || request.evidence_sha256.is_some() {
                    return Err(
                        "known-failure retry must not claim external outcome evidence".to_string(),
                    );
                }
                None
            }
            ProviderEmbeddingResolutionAction::AcknowledgeUnknown => {
                let source = request
                    .evidence_source_id
                    .as_deref()
                    .filter(|value| {
                        !value.is_empty()
                            && value.len() <= 256
                            && !value.chars().any(char::is_control)
                            && !crate::provider::redaction::contains_sensitive_patterns(value)
                    })
                    .ok_or_else(|| {
                        "unknown-outcome acknowledgement requires an evidence source".to_string()
                    })?;
                let hash = request
                    .evidence_sha256
                    .as_deref()
                    .filter(|value| is_sha256(value))
                    .ok_or_else(|| {
                        "unknown-outcome acknowledgement requires a SHA-256 evidence binding"
                            .to_string()
                    })?;
                Some((source, hash))
            }
        };
        let now = self.now();
        let audit_evidence = |operation_id: &str, prior_state: &str, next_attempt: i64| {
            json!({
                "schema_version":"provider_embedding_resolution.v1",
                "operation_id":operation_id,
                "target_memory_id":memory_id,
                "target_version":request.target_version,
                "prior_state":prior_state,
                "action":request.action,
                "next_attempt_count":next_attempt,
                "evidence_source_id":evidence_sha256.map(|value|value.0),
                "evidence_sha256":evidence_sha256.map(|value|value.1),
                "raw_evidence_stored":false,
            })
        };
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx=rusqlite::Transaction::new_unchecked(conn,rusqlite::TransactionBehavior::Immediate)
                    .map_err(|error|error.to_string())?;
                let row=sqlite_embedding_operation_for_resolution(&tx,memory_id,request.target_version)?
                    .ok_or_else(||"provider embedding operation not found".to_string())?;
                validate_resolution_scope(&row,request)?;
                if resolution_is_idempotent(&row,request) {
                    let action=resolution_audit_action(&request.action);
                    let expected=resolution_prior_states(&request.action).iter()
                        .map(|prior_state|audit_evidence(&row.operation_id,prior_state,row.attempt_count))
                        .collect::<Vec<_>>();
                    validate_idempotent_resolution_sqlite(&tx,&format!("provider-embedding/{}",row.operation_id),action,&expected)?;
                    return Ok(json!({"operation_id":row.operation_id,"state":row.state,"attempt_count":row.attempt_count,"idempotent":true}));
                }
                let transition=resolution_transition(&row,request)?;
                let prior_state = row.state.as_str();
                let next_state = transition.next_state.as_str();
                let changed=tx.execute(
                    "UPDATE provider_embedding_operations SET state=?1,attempt_count=?2,updated_at=?3
                     WHERE operation_id=?4 AND state=?5 AND attempt_count=?6",
                    params![next_state,transition.next_attempt,now,row.operation_id,prior_state,row.attempt_count],
                ).map_err(|error|error.to_string())?;
                require_single_embedding_operation_update(changed)?;
                append_audit_locked(&tx,&now,actor,transition.audit_action,
                    &format!("provider-embedding/{}",row.operation_id),&audit_evidence(&row.operation_id,prior_state,transition.next_attempt))?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(json!({"operation_id":row.operation_id,"state":transition.next_state,"attempt_count":transition.next_attempt,"idempotent":false}))
            }),
            #[cfg(feature="pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx=client.transaction().map_err(|error|error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))",&[&format!("{}:{}",memory_id,request.target_version)])
                    .map_err(|error|error.to_string())?;
                let row=pg_embedding_operation_for_resolution(&mut tx,memory_id,request.target_version)?
                    .ok_or_else(||"provider embedding operation not found".to_string())?;
                validate_resolution_scope(&row,request)?;
                if resolution_is_idempotent(&row,request) {
                    let action=resolution_audit_action(&request.action);
                    let expected=resolution_prior_states(&request.action).iter()
                        .map(|prior_state|audit_evidence(&row.operation_id,prior_state,row.attempt_count))
                        .collect::<Vec<_>>();
                    validate_idempotent_resolution_pg(&mut tx,&format!("provider-embedding/{}",row.operation_id),action,&expected)?;
                    return Ok(json!({"operation_id":row.operation_id,"state":row.state,"attempt_count":row.attempt_count,"idempotent":true}));
                }
                let transition=resolution_transition(&row,request)?;
                let prior_state = row.state.as_str();
                let next_state = transition.next_state.as_str();
                let changed=tx.execute(
                    "UPDATE provider_embedding_operations SET state=$1,attempt_count=$2,updated_at=$3
                     WHERE operation_id=$4 AND state=$5 AND attempt_count=$6",
                    &[&next_state,&transition.next_attempt,&now,&row.operation_id,&prior_state,&row.attempt_count],
                ).map_err(|error|error.to_string())?;
                require_single_embedding_operation_update(changed as usize)?;
                append_provider_embedding_resolution_audit_pg(&mut tx,&now,actor,transition.audit_action,
                    &format!("provider-embedding/{}",row.operation_id),&audit_evidence(&row.operation_id,prior_state,transition.next_attempt))?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(json!({"operation_id":row.operation_id,"state":transition.next_state,"attempt_count":transition.next_attempt,"idempotent":false}))
            }),
        }
    }

    pub fn record_provider_audit_event(
        &self,
        event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let changed = conn.execute(
                    "INSERT OR IGNORE INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        event.event_id,
                        event.dispatch_id,
                        event.provider_id,
                        event.event_type,
                        event.input_token_count,
                        event.output_token_count,
                        event.cost,
                        event.currency,
                        event.latency_ms,
                        event.error_domain,
                        event.redaction_status,
                        event.created_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                if changed == 0 {
                    let existing = conn.query_row(
                        "SELECT event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,cost,currency,latency_ms,error_domain,redaction_status,created_at FROM provider_audit_events WHERE event_id=?1",
                        params![event.event_id],
                        provider_event_binding_sqlite,
                    ).map_err(|error|error.to_string())?;
                    if existing != provider_event_binding(event) {
                        return Err("provider audit event id binding conflict".to_string());
                    }
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let eid: &str = &event.event_id;
                let did: &str = &event.dispatch_id;
                let pid: &str = &event.provider_id;
                let etype: &str = &event.event_type;
                let itc: Option<i32> = event.input_token_count.map(|v| v as i32);
                let otc: Option<i32> = event.output_token_count.map(|v| v as i32);
                let cost = event.cost;
                let cur = event.currency.as_deref();
                let lat: Option<i32> = event.latency_ms.map(|v| v as i32);
                let edom = event.error_domain.as_deref();
                let rs: &str = &event.redaction_status;
                let cat: &str = &event.created_at;
                let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
                    &eid, &did, &pid, &etype, &itc, &otc, &cost, &cur, &lat, &edom, &rs, &cat,
                ];
                let changed = client
                    .execute(
                        "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT DO NOTHING",
                        &params,
                    )
                    .map_err(|e| e.to_string())?;
                if changed == 0 {
                    let row=client.query_one("SELECT event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,cost,currency,latency_ms,error_domain,redaction_status,created_at FROM provider_audit_events WHERE event_id=$1",&[&event.event_id]).map_err(|error|error.to_string())?;
                    if provider_event_binding_pg(&row)!=provider_event_binding(event){return Err("provider audit event id binding conflict".to_string())}
                }
                Ok(())
            }),
        }
    }

    pub fn reserve_provider_audit_cost(
        &self,
        event: &crate::provider::ProviderAuditEvent,
        per_call_cap_usd: f64,
        daily_cap_usd: f64,
    ) -> Result<(), String> {
        self.reserve_provider_audit_cost_inner(event, per_call_cap_usd, daily_cap_usd, false)
            .map(|_| ())
    }

    /// Atomically reserves verified-free capacity and creates the durable receipt.
    /// No send ownership is granted and no network operation may occur in this phase.
    pub(crate) fn reserve_verified_free_embedding_operation(
        &self,
        operation: &ProviderEmbeddingOperation,
        reservation: &crate::provider::ProviderAuditEvent,
        per_call_cap_usd: f64,
        daily_cap_usd: f64,
        contract: &crate::provider::embedding::EmbeddingContractEvidence,
    ) -> Result<ProviderEmbeddingOperationClaim, String> {
        validate_embedding_operation(operation)?;
        validate_verified_free_embedding_reservation(reservation, contract)?;
        validate_provider_caps(per_call_cap_usd, daily_cap_usd)?;
        let date_prefix = reservation
            .created_at
            .get(..10)
            .ok_or_else(|| "invalid provider cost reservation audit event".to_string())?;
        let pattern = format!("{date_prefix}%");

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let (preflight_attempt, retry_attempt) = match sqlite_embedding_operation(&tx, operation)? {
                    Some(existing) => match validate_stored_embedding_operation(operation, existing)? {
                        ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count } => {
                            (Some(attempt_count), None)
                        }
                        ProviderEmbeddingOperationClaim::RetryAuthorized { attempt_count } => {
                            (None, Some(attempt_count))
                        }
                        completed => return Ok(completed),
                    },
                    None => (None, None),
                };
                let effective_attempt = preflight_attempt.or(retry_attempt);
                let reservation = effective_attempt
                    .filter(|attempt| *attempt > 1)
                    .map(|attempt| retry_audit_event(reservation, attempt))
                    .unwrap_or_else(|| reservation.clone());
                let dispatch_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)
                         FROM dispatch_history WHERE created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let reservation_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(cost), 0.0) FROM provider_audit_events
                         WHERE event_type='request_reserved' AND created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let adaptive_raw: Option<String> = tx
                    .query_row(
                        "SELECT value_json FROM local_config WHERE key='adaptive_fusion_observations'",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let today_total = dispatch_cost
                    + reservation_cost
                    + adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                if today_total > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                insert_provider_event_sqlite(&tx, &reservation)?;
                if let Some(attempt) = preflight_attempt {
                    let changed = tx.execute(
                        "UPDATE provider_embedding_operations
                         SET state='reserved',reservation_event_id=?1,updated_at=?2
                         WHERE operation_id=?3 AND operation_binding_sha256=?4
                           AND state='preflight_reserved' AND attempt_count=?5",
                        params![reservation.event_id,reservation.created_at,operation.operation_id,
                            operation.operation_binding_sha256,attempt],
                    ).map_err(|error|error.to_string())?;
                    require_single_embedding_operation_update(changed)?;
                } else if let Some(attempt) = retry_attempt {
                    let changed = tx.execute(
                        "UPDATE provider_embedding_operations
                         SET state='reserved',reservation_event_id=?1,send_event_id=NULL,
                             outcome_event_id=NULL,updated_at=?2
                         WHERE operation_id=?3 AND operation_binding_sha256=?4
                           AND state='retry_authorized' AND attempt_count=?5",
                        params![reservation.event_id,reservation.created_at,operation.operation_id,
                            operation.operation_binding_sha256,attempt],
                    ).map_err(|error|error.to_string())?;
                    require_single_embedding_operation_update(changed)?;
                } else {
                    insert_embedding_operation_sqlite(
                        &tx,
                        operation,
                        &reservation.event_id,
                        ProviderEmbeddingReceiptState::Reserved,
                    )?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ProviderEmbeddingOperationClaim::Claimed {
                    attempt_count: effective_attempt.unwrap_or(1),
                })
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(684214091)", &[])
                    .map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "{}:{}",
                        operation.target_memory_id, operation.target_version
                    )],
                )
                .map_err(|error| error.to_string())?;
                let (preflight_attempt, retry_attempt) = match pg_embedding_operation(&mut tx, operation)? {
                    Some(existing) => match validate_stored_embedding_operation(operation, existing)? {
                        ProviderEmbeddingOperationClaim::PreflightReserved { attempt_count } => {
                            (Some(attempt_count), None)
                        }
                        ProviderEmbeddingOperationClaim::RetryAuthorized { attempt_count } => {
                            (None, Some(attempt_count))
                        }
                        completed => return Ok(completed),
                    },
                    None => (None, None),
                };
                let effective_attempt = preflight_attempt.or(retry_attempt);
                let reservation = effective_attempt
                    .filter(|attempt| *attempt > 1)
                    .map(|attempt| retry_audit_event(reservation, attempt))
                    .unwrap_or_else(|| reservation.clone());
                let dispatch_cost: f64 = tx.query_one(
                    "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)::DOUBLE PRECISION
                     FROM dispatch_history WHERE created_at LIKE $1",
                    &[&pattern],
                ).map_err(|error|error.to_string())?.get(0);
                let reservation_cost: f64 = tx.query_one(
                    "SELECT COALESCE(SUM(cost), 0.0)::DOUBLE PRECISION FROM provider_audit_events
                     WHERE event_type='request_reserved' AND created_at LIKE $1",
                    &[&pattern],
                ).map_err(|error|error.to_string())?.get(0);
                let adaptive_raw = tx.query_opt(
                    "SELECT value_json FROM local_config WHERE key='adaptive_fusion_observations' FOR SHARE",
                    &[],
                ).map_err(|error|error.to_string())?.map(|row|row.get::<_,String>(0));
                let today_total = dispatch_cost + reservation_cost
                    + adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                if today_total > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                insert_provider_event_pg(&mut tx, &reservation)?;
                if let Some(attempt) = preflight_attempt {
                    let changed=tx.execute(
                        "UPDATE provider_embedding_operations
                         SET state='reserved',reservation_event_id=$1,updated_at=$2
                         WHERE operation_id=$3 AND operation_binding_sha256=$4
                           AND state='preflight_reserved' AND attempt_count=$5",
                        &[&reservation.event_id,&reservation.created_at,&operation.operation_id,
                          &operation.operation_binding_sha256,&attempt],
                    ).map_err(|error|error.to_string())?;
                    require_single_embedding_operation_update(changed as usize)?;
                } else if let Some(attempt) = retry_attempt {
                    let changed=tx.execute(
                        "UPDATE provider_embedding_operations
                         SET state='reserved',reservation_event_id=$1,send_event_id=NULL,
                             outcome_event_id=NULL,updated_at=$2
                         WHERE operation_id=$3 AND operation_binding_sha256=$4
                           AND state='retry_authorized' AND attempt_count=$5",
                        &[&reservation.event_id,&reservation.created_at,&operation.operation_id,
                          &operation.operation_binding_sha256,&attempt],
                    ).map_err(|error|error.to_string())?;
                    require_single_embedding_operation_update(changed as usize)?;
                } else {
                    insert_embedding_operation_pg(
                        &mut tx,
                        operation,
                        &reservation.event_id,
                        ProviderEmbeddingReceiptState::Reserved,
                    )?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ProviderEmbeddingOperationClaim::Claimed {
                    attempt_count: effective_attempt.unwrap_or(1),
                })
            }),
        }
    }

    /// Atomically grants the only permitted send ownership. The caller may start
    /// the network operation only after this method commits successfully.
    pub(crate) fn claim_verified_free_embedding_send(
        &self,
        operation: &ProviderEmbeddingOperation,
        request_sent: &crate::provider::ProviderAuditEvent,
    ) -> Result<i64, String> {
        validate_embedding_operation(operation)?;
        if request_sent.event_type != "request_sent"
            || !request_sent.dispatch_id.starts_with(&format!(
                "memory-embedding-{}",
                operation.operation_binding_sha256
            ))
            || request_sent.provider_id != operation.provider_id
            || request_sent.redaction_status != "redacted"
        {
            return Err("invalid provider embedding request audit event".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let row = sqlite_embedding_operation(&tx, operation)?
                    .ok_or_else(|| "provider embedding reservation is missing".to_string())?;
                validate_stored_embedding_operation_binding(operation, &row)?;
                if row.state != ProviderEmbeddingReceiptState::Reserved {
                    return Err(
                        "competing provider embedding mutation already owns send authority"
                            .to_string(),
                    );
                }
                insert_provider_event_sqlite(&tx, request_sent)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                         SET state='sending',send_event_id=?1,updated_at=?2
                         WHERE operation_id=?3 AND operation_binding_sha256=?4 AND state='reserved'",
                        params![
                            request_sent.event_id,
                            request_sent.created_at,
                            operation.operation_id,
                            operation.operation_binding_sha256
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed)?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(row.attempt_count)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "{}:{}",
                        operation.target_memory_id, operation.target_version
                    )],
                )
                .map_err(|error| error.to_string())?;
                let row = pg_embedding_operation(&mut tx, operation)?
                    .ok_or_else(|| "provider embedding reservation is missing".to_string())?;
                validate_stored_embedding_operation_binding(operation, &row)?;
                if row.state != ProviderEmbeddingReceiptState::Reserved {
                    return Err(
                        "competing provider embedding mutation already owns send authority"
                            .to_string(),
                    );
                }
                insert_provider_event_pg(&mut tx, request_sent)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                         SET state='sending',send_event_id=$1,updated_at=$2
                         WHERE operation_id=$3 AND operation_binding_sha256=$4 AND state='reserved'",
                        &[
                            &request_sent.event_id,
                            &request_sent.created_at,
                            &operation.operation_id,
                            &operation.operation_binding_sha256,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed as usize)?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(row.attempt_count)
            }),
        }
    }

    pub(crate) fn fail_provider_embedding_preflight(
        &self,
        operation: &ProviderEmbeddingOperation,
        error_event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        validate_embedding_operation(operation)?;
        if error_event.event_type != "error"
            || error_event.provider_id != operation.provider_id
            || error_event.error_domain.as_deref()
                != Some(ProviderEmbeddingErrorDomain::FailedBeforeSend.as_str())
            || error_event.redaction_status != "redacted"
        {
            return Err("invalid provider embedding preflight failure audit event".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_sqlite(&tx, error_event)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                     SET state='failed_before_send',outcome_event_id=?1,updated_at=?2
                     WHERE operation_id=?3 AND operation_binding_sha256=?4
                       AND state='preflight_reserved' AND send_event_id IS NULL",
                        params![
                            error_event.event_id,
                            error_event.created_at,
                            operation.operation_id,
                            operation.operation_binding_sha256
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed)?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "{}:{}",
                        operation.target_memory_id, operation.target_version
                    )],
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_pg(&mut tx, error_event)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                     SET state='failed_before_send',outcome_event_id=$1,updated_at=$2
                     WHERE operation_id=$3 AND operation_binding_sha256=$4
                       AND state='preflight_reserved' AND send_event_id IS NULL",
                        &[
                            &error_event.event_id,
                            &error_event.created_at,
                            &operation.operation_id,
                            &operation.operation_binding_sha256,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed as usize)?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub(crate) fn complete_provider_embedding_operation(
        &self,
        operation: &ProviderEmbeddingOperation,
        vector_json: &str,
        metadata_json: &str,
        response: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        let _: Vec<f64> = serde_json::from_str(vector_json)
            .map_err(|_| "completed embedding vector receipt is malformed".to_string())?;
        let _: crate::provider::embedding::ProviderEmbeddingMetadata =
            serde_json::from_str(metadata_json)
                .map_err(|_| "completed embedding metadata receipt is malformed".to_string())?;
        let expected_response_id = response
            .dispatch_id
            .strip_prefix("memory-embedding-")
            .map(|suffix| format!("paudit-response-{suffix}"));
        if response.event_type != "response_received"
            || !response.dispatch_id.starts_with(&format!(
                "memory-embedding-{}",
                operation.operation_binding_sha256
            ))
            || response.provider_id != operation.provider_id
            || response.redaction_status != "redacted"
            || expected_response_id.as_deref() != Some(response.event_id.as_str())
        {
            return Err("invalid provider embedding response audit event".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_sqlite(&tx, response)?;
                complete_embedding_operation_sqlite(
                    &tx,
                    operation,
                    vector_json,
                    metadata_json,
                    &response.event_id,
                    &response.created_at,
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "{}:{}",
                        operation.target_memory_id, operation.target_version
                    )],
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_pg(&mut tx, response)?;
                complete_embedding_operation_pg(
                    &mut tx,
                    operation,
                    vector_json,
                    metadata_json,
                    &response.event_id,
                    &response.created_at,
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub(crate) fn fail_provider_embedding_operation(
        &self,
        operation: &ProviderEmbeddingOperation,
        failed_before_send: bool,
        outcome_unknown: bool,
        error_event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        if failed_before_send && outcome_unknown {
            return Err("provider embedding failure classification is contradictory".to_string());
        }
        let state = if failed_before_send {
            ProviderEmbeddingReceiptState::FailedBeforeSend
        } else if outcome_unknown {
            ProviderEmbeddingReceiptState::OutcomeUnknown
        } else {
            ProviderEmbeddingReceiptState::FailedKnownOutcome
        };
        let expected_error_id = error_event
            .dispatch_id
            .strip_prefix("memory-embedding-")
            .map(|suffix| format!("paudit-error-{suffix}"));
        if error_event.event_type != "error"
            || !error_event.dispatch_id.starts_with(&format!(
                "memory-embedding-{}",
                operation.operation_binding_sha256
            ))
            || error_event.provider_id != operation.provider_id
            || expected_error_id.as_deref() != Some(error_event.event_id.as_str())
            || error_event.redaction_status != "redacted"
            || error_event
                .error_domain
                .as_deref()
                .and_then(ProviderEmbeddingErrorDomain::parse)
                .is_none_or(|domain| {
                    if failed_before_send {
                        domain != ProviderEmbeddingErrorDomain::FailedBeforeSend
                    } else if outcome_unknown {
                        !domain.is_unknown_outcome()
                    } else {
                        !domain.is_known_outcome()
                    }
                })
        {
            return Err("invalid provider embedding failure audit event".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_sqlite(&tx, error_event)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                         SET state=?1,outcome_event_id=?2,updated_at=?3
                         WHERE operation_id=?4 AND operation_binding_sha256=?5 AND state='sending'",
                        params![
                            state.as_str(),
                            error_event.event_id,
                            error_event.created_at,
                            operation.operation_id,
                            operation.operation_binding_sha256
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed)?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!(
                        "{}:{}",
                        operation.target_memory_id, operation.target_version
                    )],
                )
                .map_err(|error| error.to_string())?;
                insert_provider_event_pg(&mut tx, error_event)?;
                let changed = tx
                    .execute(
                        "UPDATE provider_embedding_operations
                         SET state=$1,outcome_event_id=$2,updated_at=$3
                         WHERE operation_id=$4 AND operation_binding_sha256=$5 AND state='sending'",
                        &[
                            &state.as_str(),
                            &error_event.event_id,
                            &error_event.created_at,
                            &operation.operation_id,
                            &operation.operation_binding_sha256,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                require_single_embedding_operation_update(changed as usize)?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    fn reserve_provider_audit_cost_inner(
        &self,
        event: &crate::provider::ProviderAuditEvent,
        per_call_cap_usd: f64,
        daily_cap_usd: f64,
        allow_verified_zero: bool,
    ) -> Result<bool, String> {
        let reserved_cost = event
            .cost
            .filter(|value| {
                value.is_finite() && (*value > 0.0 || allow_verified_zero && *value == 0.0)
            })
            .ok_or_else(|| "provider cost reservation must be finite and positive".to_string())?;
        if !per_call_cap_usd.is_finite()
            || per_call_cap_usd <= 0.0
            || !daily_cap_usd.is_finite()
            || daily_cap_usd <= 0.0
        {
            return Err(
                "provider cost reservation requires positive per-call and daily caps".to_string(),
            );
        }
        if reserved_cost > per_call_cap_usd {
            return Err("agent decision provider per-call cost cap exceeded".to_string());
        }
        if event.event_type != "request_reserved" || event.created_at.len() < 10 {
            return Err("invalid provider cost reservation audit event".to_string());
        }
        let date_prefix = &event.created_at[..10];
        let pattern = format!("{date_prefix}%");

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let existing: Option<ProviderReservationBinding> = tx
                    .query_row(
                        "SELECT dispatch_id, provider_id, event_type, cost, currency, created_at
                         FROM provider_audit_events WHERE event_id = ?1",
                        params![event.event_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if let Some(existing) = existing {
                    validate_existing_reservation(event, &existing)?;
                    return Ok(false);
                }
                let dispatch_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)
                         FROM dispatch_history WHERE created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let reservation_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(cost), 0.0) FROM provider_audit_events
                         WHERE event_type='request_reserved' AND created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let adaptive_raw: Option<String> = tx
                    .query_row(
                        "SELECT value_json FROM local_config WHERE key='adaptive_fusion_observations'",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let adaptive_cost = adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                let today_total = dispatch_cost + reservation_cost + adaptive_cost;
                if today_total + reserved_cost > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                tx.execute(
                    "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        event.event_id,
                        event.dispatch_id,
                        event.provider_id,
                        event.event_type,
                        event.input_token_count,
                        event.output_token_count,
                        event.cost,
                        event.currency,
                        event.latency_ms,
                        event.error_domain,
                        event.redaction_status,
                        event.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(684214091)", &[])
                    .map_err(|error| error.to_string())?;
                let existing = tx
                    .query_opt(
                        "SELECT dispatch_id, provider_id, event_type,
                                cost::DOUBLE PRECISION, currency, created_at
                         FROM provider_audit_events WHERE event_id = $1",
                        &[&event.event_id],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| {
                        (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, Option<f64>>(3),
                            row.get::<_, Option<String>>(4),
                            row.get::<_, String>(5),
                        )
                    });
                if let Some(existing) = existing {
                    validate_existing_reservation(event, &existing)?;
                    return Ok(false);
                }
                let dispatch_cost: f64 = tx
                    .query_one(
                        "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)::DOUBLE PRECISION
                         FROM dispatch_history WHERE created_at LIKE $1",
                        &[&pattern],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let reservation_cost: f64 = tx
                    .query_one(
                        "SELECT COALESCE(SUM(cost), 0.0)::DOUBLE PRECISION FROM provider_audit_events
                         WHERE event_type='request_reserved' AND created_at LIKE $1",
                        &[&pattern],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let adaptive_raw = tx
                    .query_opt(
                        "SELECT value_json FROM local_config
                         WHERE key='adaptive_fusion_observations' FOR SHARE",
                        &[],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get::<_, String>(0));
                let adaptive_cost = adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                let today_total = dispatch_cost + reservation_cost + adaptive_cost;
                if today_total + reserved_cost > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                let input_tokens = event.input_token_count.map(|value| value as i32);
                let output_tokens = event.output_token_count.map(|value| value as i32);
                let latency_ms = event.latency_ms.map(|value| value as i32);
                tx.execute(
                    "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                    &[
                        &event.event_id,
                        &event.dispatch_id,
                        &event.provider_id,
                        &event.event_type,
                        &input_tokens,
                        &output_tokens,
                        &event.cost,
                        &event.currency,
                        &latency_ms,
                        &event.error_domain,
                        &event.redaction_status,
                        &event.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(true)
            }),
        }
    }

    pub fn provider_audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.provider_audit_events_with_offset(limit, 0)
    }

    pub fn daily_provider_audit_cost_usd(&self, date_prefix: &str) -> Result<f64, String> {
        if date_prefix.len() != 10
            || !date_prefix.chars().enumerate().all(|(index, ch)| {
                matches!(index, 4 | 7) && ch == '-'
                    || !matches!(index, 4 | 7) && ch.is_ascii_digit()
            })
        {
            return Err("date prefix must use YYYY-MM-DD".to_string());
        }
        let pattern = format!("{date_prefix}%");
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT COALESCE(SUM(cost), 0.0)
                     FROM provider_audit_events
                     WHERE event_type = 'response_received'
                       AND cost IS NOT NULL
                       AND created_at LIKE ?1",
                    params![pattern],
                    |row| row.get::<_, f64>(0),
                )
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT COALESCE(SUM(cost), 0.0)::DOUBLE PRECISION
                         FROM provider_audit_events
                         WHERE event_type = 'response_received'
                           AND cost IS NOT NULL
                           AND created_at LIKE $1",
                        &[&pattern],
                    )
                    .map(|row| row.get::<_, f64>(0))
                    .map_err(|e| e.to_string())
            }),
        }
    }

    pub fn provider_audit_events_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         ORDER BY created_at DESC, event_id DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], |row| {
                        Ok(json!({
                            "event_id": row.get::<_, String>(0)?,
                            "dispatch_id": row.get::<_, String>(1)?,
                            "provider_id": row.get::<_, String>(2)?,
                            "event_type": row.get::<_, String>(3)?,
                            "input_token_count": row.get::<_, Option<i64>>(4)?,
                            "output_token_count": row.get::<_, Option<i64>>(5)?,
                            "cost": row.get::<_, Option<f64>>(6)?,
                            "currency": row.get::<_, Option<String>>(7)?,
                            "latency_ms": row.get::<_, Option<i64>>(8)?,
                            "error_domain": row.get::<_, Option<String>>(9)?,
                            "redaction_status": row.get::<_, String>(10)?,
                            "created_at": row.get::<_, String>(11)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         ORDER BY created_at DESC, event_id DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_provider_audit_rows(rows)
            }),
        }
    }

    pub fn provider_audit_events_for_dispatch(
        &self,
        dispatch_id: &str,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         WHERE dispatch_id = ?1
                         ORDER BY created_at DESC, event_id DESC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![dispatch_id], |row| {
                        Ok(json!({
                            "event_id": row.get::<_, String>(0)?,
                            "dispatch_id": row.get::<_, String>(1)?,
                            "provider_id": row.get::<_, String>(2)?,
                            "event_type": row.get::<_, String>(3)?,
                            "input_token_count": row.get::<_, Option<i64>>(4)?,
                            "output_token_count": row.get::<_, Option<i64>>(5)?,
                            "cost": row.get::<_, Option<f64>>(6)?,
                            "currency": row.get::<_, Option<String>>(7)?,
                            "latency_ms": row.get::<_, Option<i64>>(8)?,
                            "error_domain": row.get::<_, Option<String>>(9)?,
                            "redaction_status": row.get::<_, String>(10)?,
                            "created_at": row.get::<_, String>(11)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let p1: &str = dispatch_id;
                let rows = client
                    .query(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         WHERE dispatch_id = $1
                         ORDER BY created_at DESC, event_id DESC",
                        &[&p1],
                    )
                    .map_err(|e| e.to_string())?;
                pg_provider_audit_rows(rows)
            }),
        }
    }
}

fn validate_embedding_operation(operation: &ProviderEmbeddingOperation) -> Result<(), String> {
    let contract: crate::provider::embedding::EmbeddingContractEvidence =
        serde_json::from_str(&operation.contract_json)
            .map_err(|_| "invalid provider embedding operation binding".to_string())?;
    if operation.operation_id.is_empty()
        || !matches!(
            operation.operation_kind.as_str(),
            "memory_version" | "retrieval_query"
        )
        || operation.target_memory_id.is_empty()
        || operation.target_version <= 0
        || operation.tenant_id.is_empty()
        || operation.workspace_id.is_empty()
        || operation.source_id.is_empty()
        || !is_sha256(&operation.source_sha256)
        || !is_sha256(&operation.request_identity_sha256)
        || !is_sha256(&operation.operation_binding_sha256)
        || !is_sha256(&operation.content_sha256)
        || !is_sha256(&operation.contract_sha256)
        || format!(
            "{:x}",
            sha2::Sha256::digest(operation.contract_json.as_bytes())
        ) != operation.contract_sha256
        || operation.receipt_sha256 != provider_embedding_operation_receipt_sha256(operation)?
        || contract.provider_id != operation.provider_id
        || contract.requested_model_id != operation.requested_model_id
        || contract.resolved_model_id != operation.resolved_model_id
        || contract.dimensions as i64 != operation.dimensions
        || (operation.operation_kind == "memory_version"
            && (operation.node_id.is_some() || operation.query_sha256.is_some()))
        || (operation.operation_kind == "retrieval_query"
            && (operation.run_id.is_none()
                || operation.node_id.as_deref().is_none_or(str::is_empty)
                || operation
                    .query_sha256
                    .as_deref()
                    .is_none_or(|value| !is_sha256(value))
                || operation.query_sha256.as_deref() != Some(operation.source_sha256.as_str())))
        || !crate::provider::embedding::is_supported_durable_embedding_contract(&contract)
        || operation.created_at.len() < 10
    {
        return Err("invalid provider embedding operation binding".to_string());
    }
    Ok(())
}

pub(crate) fn provider_embedding_operation_receipt_sha256(
    operation: &ProviderEmbeddingOperation,
) -> Result<String, String> {
    let value = json!({
        "operation_id": operation.operation_id,
        "operation_kind": operation.operation_kind,
        "target_memory_id": operation.target_memory_id,
        "target_version": operation.target_version,
        "tenant_id":operation.tenant_id,
        "workspace_id":operation.workspace_id,
        "agent_id":operation.agent_id,
        "run_id":operation.run_id,
        "task_id":operation.task_id,
        "source_id":operation.source_id,
        "source_sha256":operation.source_sha256,
        "node_id":operation.node_id,
        "query_sha256":operation.query_sha256,
        "request_identity_sha256":operation.request_identity_sha256,
        "operation_binding_sha256": operation.operation_binding_sha256,
        "content_sha256": operation.content_sha256,
        "contract_sha256":operation.contract_sha256,
        "provider_id": operation.provider_id,
        "requested_model_id": operation.requested_model_id,
        "resolved_model_id": operation.resolved_model_id,
        "dimensions": operation.dimensions,
    });
    serde_json::to_vec(&value)
        .map(|bytes| format!("{:x}", sha2::Sha256::digest(bytes)))
        .map_err(|error| error.to_string())
}

fn validate_provider_caps(per_call_cap_usd: f64, daily_cap_usd: f64) -> Result<(), String> {
    if !per_call_cap_usd.is_finite()
        || per_call_cap_usd <= 0.0
        || !daily_cap_usd.is_finite()
        || daily_cap_usd <= 0.0
    {
        return Err(
            "provider cost reservation requires positive per-call and daily caps".to_string(),
        );
    }
    Ok(())
}

fn validate_verified_free_embedding_reservation(
    event: &crate::provider::ProviderAuditEvent,
    contract: &crate::provider::embedding::EmbeddingContractEvidence,
) -> Result<(), String> {
    let pricing = &contract.pricing;
    if event.provider_id != crate::provider::embedding::OPENROUTER_EMBEDDING_PROVIDER_ID
        || event.event_type != "request_reserved"
        || event.cost != Some(0.0)
        || event.currency.as_deref() != Some("USD")
        || pricing.prompt_cost_per_token_usd != Some(0.0)
        || pricing.completion_cost_per_token_usd != Some(0.0)
        || pricing.request_cost_per_request_usd != Some(0.0)
        || pricing
            .image_cost_per_image_usd
            .is_some_and(|price| price != 0.0)
        || pricing
            .web_search_cost_per_request_usd
            .is_some_and(|price| price != 0.0)
        || pricing
            .internal_reasoning_cost_per_token_usd
            .is_some_and(|price| price != 0.0)
        || pricing
            .input_cache_read_cost_per_token_usd
            .is_some_and(|price| price != 0.0)
        || pricing
            .input_cache_write_cost_per_token_usd
            .is_some_and(|price| price != 0.0)
        || pricing.request_max_price
            != crate::provider::embedding::EmbeddingPricingOverrides::zero()
        || pricing.currency != "USD"
        || pricing.source != crate::provider::embedding::OPENROUTER_EMBEDDING_PRICING_SOURCE
        || !crate::provider::embedding::has_current_pricing_dimension_applicability(contract)
    {
        return Err(
            "zero-cost provider reservation requires verified free embedding pricing".to_string(),
        );
    }
    Ok(())
}

fn validate_stored_embedding_operation(
    operation: &ProviderEmbeddingOperation,
    existing: StoredEmbeddingOperation,
) -> Result<ProviderEmbeddingOperationClaim, String> {
    validate_stored_embedding_operation_binding(operation, &existing)?;
    match existing.state {
        ProviderEmbeddingReceiptState::NetworkSucceeded
        | ProviderEmbeddingReceiptState::Succeeded => {
            match (existing.vector_json, existing.metadata_json) {
                (Some(vector_json), Some(metadata_json)) => {
                    Ok(ProviderEmbeddingOperationClaim::Completed {
                        vector_json,
                        metadata_json,
                    })
                }
                _ => Err("successful provider embedding receipt is incomplete".to_string()),
            }
        }
        ProviderEmbeddingReceiptState::PreflightReserved => {
            Ok(ProviderEmbeddingOperationClaim::PreflightReserved {
                attempt_count: existing.attempt_count,
            })
        }
        ProviderEmbeddingReceiptState::Reserved => Ok(ProviderEmbeddingOperationClaim::Claimed {
            attempt_count: existing.attempt_count,
        }),
        ProviderEmbeddingReceiptState::Sending
        | ProviderEmbeddingReceiptState::OutcomeUnknown
        | ProviderEmbeddingReceiptState::OutcomeUnknownAcknowledged => {
            Err("provider embedding outcome is unknown; automatic replay is forbidden".to_string())
        }
        ProviderEmbeddingReceiptState::FailedBeforeSend
        | ProviderEmbeddingReceiptState::FailedKnownOutcome => Err(
            "provider embedding operation failed; explicit operator retry is required".to_string(),
        ),
        ProviderEmbeddingReceiptState::ResultErased => {
            Err("provider embedding result was erased by a durable-memory tombstone".to_string())
        }
        ProviderEmbeddingReceiptState::RetryAuthorized => {
            Ok(ProviderEmbeddingOperationClaim::RetryAuthorized {
                attempt_count: existing.attempt_count,
            })
        }
    }
}

fn validate_stored_embedding_operation_binding(
    operation: &ProviderEmbeddingOperation,
    existing: &StoredEmbeddingOperation,
) -> Result<(), String> {
    if existing.operation_id != operation.operation_id
        || existing.operation_kind != operation.operation_kind
        || existing.tenant_id != operation.tenant_id
        || existing.workspace_id != operation.workspace_id
        || existing.agent_id != operation.agent_id
        || existing.run_id != operation.run_id
        || existing.task_id != operation.task_id
        || existing.source_id != operation.source_id
        || existing.source_sha256 != operation.source_sha256
        || existing.node_id != operation.node_id
        || existing.query_sha256 != operation.query_sha256
        || existing.request_identity_sha256 != operation.request_identity_sha256
        || existing.operation_binding_sha256 != operation.operation_binding_sha256
        || existing.content_sha256 != operation.content_sha256
        || existing.contract_json != operation.contract_json
        || existing.contract_sha256 != operation.contract_sha256
        || existing.receipt_sha256 != operation.receipt_sha256
        || existing.provider_id != operation.provider_id
        || existing.requested_model_id != operation.requested_model_id
        || existing.resolved_model_id != operation.resolved_model_id
        || existing.dimensions != operation.dimensions
    {
        return Err(
            "competing provider embedding mutation already owns this memory version".to_string(),
        );
    }
    Ok(())
}

fn sqlite_embedding_operation(
    tx: &rusqlite::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
) -> Result<Option<StoredEmbeddingOperation>, String> {
    tx.query_row(
        "SELECT operation_id,operation_kind,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,
                source_sha256,node_id,query_sha256,request_identity_sha256,operation_binding_sha256,
                content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                requested_model_id,resolved_model_id,dimensions,state,attempt_count,vector_json,metadata_json
         FROM provider_embedding_operations
         WHERE target_memory_id=?1 AND target_version=?2",
        params![operation.target_memory_id, operation.target_version],
        |row| {
            stored_embedding_operation_sqlite(row)
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn sqlite_embedding_operation_for_resolution(
    tx: &rusqlite::Transaction<'_>,
    memory_id: &str,
    target_version: i64,
) -> Result<Option<StoredEmbeddingOperation>, String> {
    tx.query_row(
        "SELECT operation_id,operation_kind,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,
                source_sha256,node_id,query_sha256,request_identity_sha256,operation_binding_sha256,
                content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                requested_model_id,resolved_model_id,dimensions,state,attempt_count,vector_json,metadata_json
         FROM provider_embedding_operations WHERE target_memory_id=?1 AND target_version=?2",
        params![memory_id,target_version],
        stored_embedding_operation_sqlite,
    ).optional().map_err(|error|error.to_string())
}

fn stored_embedding_operation_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredEmbeddingOperation> {
    Ok(StoredEmbeddingOperation {
        operation_id: row.get(0)?,
        operation_kind: row.get(1)?,
        tenant_id: row.get(2)?,
        workspace_id: row.get(3)?,
        agent_id: row.get(4)?,
        run_id: row.get(5)?,
        task_id: row.get(6)?,
        source_id: row.get(7)?,
        source_sha256: row.get(8)?,
        node_id: row.get(9)?,
        query_sha256: row.get(10)?,
        request_identity_sha256: row.get(11)?,
        operation_binding_sha256: row.get(12)?,
        content_sha256: row.get(13)?,
        contract_json: row.get(14)?,
        contract_sha256: row.get(15)?,
        receipt_sha256: row.get(16)?,
        provider_id: row.get(17)?,
        requested_model_id: row.get(18)?,
        resolved_model_id: row.get(19)?,
        dimensions: row.get(20)?,
        state: ProviderEmbeddingReceiptState::parse(&row.get::<_, String>(21)?).map_err(
            |error| {
                rusqlite::Error::FromSqlConversionFailure(
                    21,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                )
            },
        )?,
        attempt_count: row.get(22)?,
        vector_json: row.get(23)?,
        metadata_json: row.get(24)?,
    })
}

#[cfg(feature = "pg")]
fn pg_embedding_operation(
    tx: &mut postgres::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
) -> Result<Option<StoredEmbeddingOperation>, String> {
    tx.query_opt(
        "SELECT operation_id,operation_kind,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,
                source_sha256,node_id,query_sha256,request_identity_sha256,operation_binding_sha256,
                content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                requested_model_id,resolved_model_id,dimensions,state,attempt_count,vector_json,metadata_json
         FROM provider_embedding_operations
         WHERE target_memory_id=$1 AND target_version=$2 FOR UPDATE",
        &[&operation.target_memory_id, &operation.target_version],
    )
    .map_err(|error| error.to_string())
    .and_then(|row| row.map(|row| stored_embedding_operation_pg(&row)).transpose())
}

#[cfg(feature = "pg")]
fn pg_embedding_operation_for_resolution(
    tx: &mut postgres::Transaction<'_>,
    memory_id: &str,
    target_version: i64,
) -> Result<Option<StoredEmbeddingOperation>, String> {
    tx.query_opt(
        "SELECT operation_id,operation_kind,tenant_id,workspace_id,agent_id,run_id,task_id,source_id,
                source_sha256,node_id,query_sha256,request_identity_sha256,operation_binding_sha256,
                content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                requested_model_id,resolved_model_id,dimensions,state,attempt_count,vector_json,metadata_json
         FROM provider_embedding_operations WHERE target_memory_id=$1 AND target_version=$2 FOR UPDATE",
        &[&memory_id,&target_version],
    ).map_err(|error|error.to_string()).and_then(|row|row.map(|row|stored_embedding_operation_pg(&row)).transpose())
}

#[cfg(feature = "pg")]
fn stored_embedding_operation_pg(row: &postgres::Row) -> Result<StoredEmbeddingOperation, String> {
    Ok(StoredEmbeddingOperation {
        operation_id: row.get(0),
        operation_kind: row.get(1),
        tenant_id: row.get(2),
        workspace_id: row.get(3),
        agent_id: row.get(4),
        run_id: row.get(5),
        task_id: row.get(6),
        source_id: row.get(7),
        source_sha256: row.get(8),
        node_id: row.get(9),
        query_sha256: row.get(10),
        request_identity_sha256: row.get(11),
        operation_binding_sha256: row.get(12),
        content_sha256: row.get(13),
        contract_json: row.get(14),
        contract_sha256: row.get(15),
        receipt_sha256: row.get(16),
        provider_id: row.get(17),
        requested_model_id: row.get(18),
        resolved_model_id: row.get(19),
        dimensions: row.get(20),
        state: ProviderEmbeddingReceiptState::parse(&row.get::<_, String>(21))?,
        attempt_count: row.get(22),
        vector_json: row.get(23),
        metadata_json: row.get(24),
    })
}

fn validate_resolution_scope(
    row: &StoredEmbeddingOperation,
    request: &ProviderEmbeddingResolutionRequest,
) -> Result<(), String> {
    if row.tenant_id != request.scope.tenant_id
        || row.workspace_id != request.scope.workspace_id
        || row.agent_id != request.scope.agent_id
        || row.task_id != request.scope.task_id
        || row.run_id != request.run_id
    {
        return Err("provider embedding reconciliation scope mismatch".to_string());
    }
    Ok(())
}

struct ResolutionTransition {
    next_state: ProviderEmbeddingReceiptState,
    next_attempt: i64,
    audit_action: &'static str,
}

fn resolution_is_idempotent(
    row: &StoredEmbeddingOperation,
    request: &ProviderEmbeddingResolutionRequest,
) -> bool {
    match request.action {
        ProviderEmbeddingResolutionAction::RetryFailed => {
            row.state == ProviderEmbeddingReceiptState::RetryAuthorized
                && row.attempt_count == request.expected_attempt_count + 1
        }
        ProviderEmbeddingResolutionAction::AcknowledgeUnknown => {
            row.state == ProviderEmbeddingReceiptState::OutcomeUnknownAcknowledged
                && row.attempt_count == request.expected_attempt_count
        }
    }
}

fn resolution_audit_action(action: &ProviderEmbeddingResolutionAction) -> &'static str {
    match action {
        ProviderEmbeddingResolutionAction::RetryFailed => "provider_embedding.retry_authorized",
        ProviderEmbeddingResolutionAction::AcknowledgeUnknown => {
            "provider_embedding.outcome_unknown_acknowledged"
        }
    }
}

fn resolution_prior_states(action: &ProviderEmbeddingResolutionAction) -> &'static [&'static str] {
    match action {
        ProviderEmbeddingResolutionAction::RetryFailed => {
            &["failed_before_send", "failed_known_outcome"]
        }
        ProviderEmbeddingResolutionAction::AcknowledgeUnknown => &["outcome_unknown"],
    }
}

fn validate_idempotent_resolution_sqlite(
    tx: &rusqlite::Transaction<'_>,
    resource: &str,
    action: &str,
    expected: &[Value],
) -> Result<(), String> {
    let details: String = tx
        .query_row(
            "SELECT details_json FROM audit_log WHERE resource=?1 AND action=?2 ORDER BY audit_id DESC LIMIT 1",
            params![resource, action],
            |row| row.get(0),
        )
        .map_err(|_| "provider embedding reconciliation audit binding is missing".to_string())?;
    let actual: Value = serde_json::from_str(&details)
        .map_err(|_| "provider embedding reconciliation audit binding is malformed".to_string())?;
    if !expected.contains(&actual) {
        return Err("provider embedding reconciliation idempotency binding mismatch".to_string());
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn validate_idempotent_resolution_pg(
    tx: &mut postgres::Transaction<'_>,
    resource: &str,
    action: &str,
    expected: &[Value],
) -> Result<(), String> {
    let details: String = tx
        .query_opt(
            "SELECT details_json FROM audit_log WHERE resource=$1 AND action=$2 ORDER BY audit_id DESC LIMIT 1",
            &[&resource, &action],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "provider embedding reconciliation audit binding is missing".to_string())?
        .get(0);
    let actual: Value = serde_json::from_str(&details)
        .map_err(|_| "provider embedding reconciliation audit binding is malformed".to_string())?;
    if !expected.contains(&actual) {
        return Err("provider embedding reconciliation idempotency binding mismatch".to_string());
    }
    Ok(())
}

fn resolution_transition(
    row: &StoredEmbeddingOperation,
    request: &ProviderEmbeddingResolutionRequest,
) -> Result<ResolutionTransition, String> {
    if row.attempt_count != request.expected_attempt_count {
        return Err("provider embedding reconciliation attempt conflict".to_string());
    }
    match (&request.action, row.state) {
        (
            ProviderEmbeddingResolutionAction::RetryFailed,
            ProviderEmbeddingReceiptState::FailedBeforeSend
            | ProviderEmbeddingReceiptState::FailedKnownOutcome,
        ) if row.attempt_count < 4 => Ok(ResolutionTransition {
            next_state: ProviderEmbeddingReceiptState::RetryAuthorized,
            next_attempt: row.attempt_count + 1,
            audit_action: "provider_embedding.retry_authorized",
        }),
        (
            ProviderEmbeddingResolutionAction::AcknowledgeUnknown,
            ProviderEmbeddingReceiptState::OutcomeUnknown,
        ) => Ok(ResolutionTransition {
            next_state: ProviderEmbeddingReceiptState::OutcomeUnknownAcknowledged,
            next_attempt: row.attempt_count,
            audit_action: "provider_embedding.outcome_unknown_acknowledged",
        }),
        _ => Err("provider embedding reconciliation state/action conflict".to_string()),
    }
}

fn retry_audit_event(
    event: &crate::provider::ProviderAuditEvent,
    attempt_count: i64,
) -> crate::provider::ProviderAuditEvent {
    let mut event = event.clone();
    let suffix = format!("-attempt-{attempt_count}");
    if !event.event_id.ends_with(&suffix) {
        event.event_id.push_str(&suffix);
    }
    if !event.dispatch_id.ends_with(&suffix) {
        event.dispatch_id.push_str(&suffix);
    }
    event
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn insert_provider_event_sqlite(
    tx: &rusqlite::Transaction<'_>,
    event: &crate::provider::ProviderAuditEvent,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO provider_audit_events
         (event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,
          cost,currency,latency_ms,error_domain,redaction_status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            event.event_id,
            event.dispatch_id,
            event.provider_id,
            event.event_type,
            event.input_token_count,
            event.output_token_count,
            event.cost,
            event.currency,
            event.latency_ms,
            event.error_domain,
            event.redaction_status,
            event.created_at
        ],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn insert_embedding_operation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
    reservation_event_id: &str,
    state: ProviderEmbeddingReceiptState,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO provider_embedding_operations
         (operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,agent_id,
          run_id,task_id,source_id,source_sha256,node_id,query_sha256,request_identity_sha256,
          operation_binding_sha256,content_sha256,contract_json,contract_sha256,receipt_sha256,
          provider_id,requested_model_id,resolved_model_id,dimensions,reservation_event_id,state,
          attempt_count,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                 ?18,?19,?20,?21,?22,?23,?24,?25,1,?26,?26)",
        params![operation.operation_id,operation.operation_kind,operation.target_memory_id,
            operation.target_version,operation.tenant_id,operation.workspace_id,operation.agent_id,
            operation.run_id,operation.task_id,operation.source_id,operation.source_sha256,
            operation.node_id,operation.query_sha256,operation.request_identity_sha256,
            operation.operation_binding_sha256,operation.content_sha256,operation.contract_json,
            operation.contract_sha256,operation.receipt_sha256,operation.provider_id,
            operation.requested_model_id,operation.resolved_model_id,operation.dimensions,
            reservation_event_id,state.as_str(),operation.created_at],
    ).map(|_|()).map_err(map_embedding_claim_conflict)
}

#[cfg(feature = "pg")]
fn append_provider_embedding_resolution_audit_pg(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    evidence: &Value,
) -> Result<(), String> {
    let details = serde_json::to_string(evidence).map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO audit_log (created_at,actor,action,resource,details_json) VALUES ($1,$2,$3,$4,$5)",
        &[&now,&actor,&action,&resource,&details],
    ).map(|_|()).map_err(|error|error.to_string())
}

#[cfg(feature = "pg")]
fn insert_provider_event_pg(
    tx: &mut postgres::Transaction<'_>,
    event: &crate::provider::ProviderAuditEvent,
) -> Result<(), String> {
    let input_tokens = event.input_token_count.map(|value| value as i32);
    let output_tokens = event.output_token_count.map(|value| value as i32);
    let latency_ms = event.latency_ms.map(|value| value as i32);
    tx.execute(
        "INSERT INTO provider_audit_events
         (event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,
          cost,currency,latency_ms,error_domain,redaction_status,created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        &[
            &event.event_id,
            &event.dispatch_id,
            &event.provider_id,
            &event.event_type,
            &input_tokens,
            &output_tokens,
            &event.cost,
            &event.currency,
            &latency_ms,
            &event.error_domain,
            &event.redaction_status,
            &event.created_at,
        ],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn insert_embedding_operation_pg(
    tx: &mut postgres::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
    reservation_event_id: &str,
    state: ProviderEmbeddingReceiptState,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO provider_embedding_operations
         (operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,agent_id,
          run_id,task_id,source_id,source_sha256,node_id,query_sha256,request_identity_sha256,
          operation_binding_sha256,content_sha256,contract_json,contract_sha256,receipt_sha256,
          provider_id,requested_model_id,resolved_model_id,dimensions,reservation_event_id,state,
          attempt_count,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                 $18,$19,$20,$21,$22,$23,$24,$25,1,$26,$26)",
        &[&operation.operation_id,&operation.operation_kind,&operation.target_memory_id,
          &operation.target_version,&operation.tenant_id,&operation.workspace_id,&operation.agent_id,
          &operation.run_id,&operation.task_id,&operation.source_id,&operation.source_sha256,
          &operation.node_id,&operation.query_sha256,&operation.request_identity_sha256,
          &operation.operation_binding_sha256,&operation.content_sha256,&operation.contract_json,
          &operation.contract_sha256,&operation.receipt_sha256,&operation.provider_id,
          &operation.requested_model_id,&operation.resolved_model_id,&operation.dimensions,
          &reservation_event_id,&state.as_str(),&operation.created_at],
    ).map(|_|()).map_err(map_embedding_claim_conflict)
}

fn complete_embedding_operation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
    vector_json: &str,
    metadata_json: &str,
    outcome_event_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    let changed = tx
        .execute(
            "UPDATE provider_embedding_operations
         SET state='network_succeeded',vector_json=?1,metadata_json=?2,
             outcome_event_id=?3,updated_at=?4
         WHERE operation_id=?5 AND operation_binding_sha256=?6 AND state='sending'",
            params![
                vector_json,
                metadata_json,
                outcome_event_id,
                updated_at,
                operation.operation_id,
                operation.operation_binding_sha256
            ],
        )
        .map_err(|error| error.to_string())?;
    require_single_embedding_operation_update(changed)
}

#[cfg(feature = "pg")]
fn complete_embedding_operation_pg(
    tx: &mut postgres::Transaction<'_>,
    operation: &ProviderEmbeddingOperation,
    vector_json: &str,
    metadata_json: &str,
    outcome_event_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    let changed = tx
        .execute(
            "UPDATE provider_embedding_operations
         SET state='network_succeeded',vector_json=$1,metadata_json=$2,
             outcome_event_id=$3,updated_at=$4
         WHERE operation_id=$5 AND operation_binding_sha256=$6 AND state='sending'",
            &[
                &vector_json,
                &metadata_json,
                &outcome_event_id,
                &updated_at,
                &operation.operation_id,
                &operation.operation_binding_sha256,
            ],
        )
        .map_err(|error| error.to_string())?;
    require_single_embedding_operation_update(changed as usize)
}

fn require_single_embedding_operation_update(changed: usize) -> Result<(), String> {
    if changed == 1 {
        Ok(())
    } else {
        Err("provider embedding operation receipt state conflict".to_string())
    }
}

fn map_embedding_claim_conflict(error: impl std::fmt::Display) -> String {
    let rendered = error.to_string();
    if rendered.contains("UNIQUE") || rendered.contains("duplicate key") {
        "competing provider embedding mutation already owns this memory version".to_string()
    } else {
        rendered
    }
}

fn provider_event_binding(event: &crate::provider::ProviderAuditEvent) -> Value {
    json!({"event_id":event.event_id,"dispatch_id":event.dispatch_id,"provider_id":event.provider_id,"event_type":event.event_type,"input_token_count":event.input_token_count,"output_token_count":event.output_token_count,"cost":event.cost,"currency":event.currency,"latency_ms":event.latency_ms,"error_domain":event.error_domain,"redaction_status":event.redaction_status,"created_at":event.created_at})
}

fn provider_event_binding_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"event_id":row.get::<_,String>(0)?,"dispatch_id":row.get::<_,String>(1)?,"provider_id":row.get::<_,String>(2)?,"event_type":row.get::<_,String>(3)?,"input_token_count":row.get::<_,Option<i64>>(4)?,"output_token_count":row.get::<_,Option<i64>>(5)?,"cost":row.get::<_,Option<f64>>(6)?,"currency":row.get::<_,Option<String>>(7)?,"latency_ms":row.get::<_,Option<i64>>(8)?,"error_domain":row.get::<_,Option<String>>(9)?,"redaction_status":row.get::<_,String>(10)?,"created_at":row.get::<_,String>(11)?}),
    )
}

#[cfg(feature = "pg")]
fn provider_event_binding_pg(row: &postgres::Row) -> Value {
    json!({"event_id":row.get::<_,String>(0),"dispatch_id":row.get::<_,String>(1),"provider_id":row.get::<_,String>(2),"event_type":row.get::<_,String>(3),"input_token_count":row.get::<_,Option<i32>>(4).map(i64::from),"output_token_count":row.get::<_,Option<i32>>(5).map(i64::from),"cost":row.get::<_,Option<f64>>(6),"currency":row.get::<_,Option<String>>(7),"latency_ms":row.get::<_,Option<i32>>(8).map(i64::from),"error_domain":row.get::<_,Option<String>>(9),"redaction_status":row.get::<_,String>(10),"created_at":row.get::<_,String>(11)})
}

fn validate_existing_reservation(
    event: &crate::provider::ProviderAuditEvent,
    existing: &ProviderReservationBinding,
) -> Result<(), String> {
    let same_cost = match (event.cost, existing.3) {
        (Some(expected), Some(actual)) => (expected - actual).abs() <= f64::EPSILON,
        (None, None) => true,
        _ => false,
    };
    if existing.0 == event.dispatch_id
        && existing.1 == event.provider_id
        && existing.2 == event.event_type
        && same_cost
        && existing.4 == event.currency
        && existing.5 == event.created_at
    {
        Ok(())
    } else {
        Err("provider cost reservation identity conflicts with persisted evidence".to_string())
    }
}

fn adaptive_daily_cost(raw: Option<&str>, date_prefix: &str) -> Result<f64, String> {
    let Some(raw) = raw else {
        return Ok(0.0);
    };
    let values: Value = serde_json::from_str(raw)
        .map_err(|error| format!("invalid adaptive observation cost evidence: {error}"))?;
    let observations = values
        .as_array()
        .ok_or_else(|| "adaptive observation cost evidence must be an array".to_string())?;
    observations.iter().try_fold(0.0, |total, observation| {
        let created_at = observation
            .get("created_at")
            .and_then(Value::as_str)
            .ok_or_else(|| "adaptive observation is missing created_at".to_string())?;
        let cost = observation
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| "adaptive observation has invalid cost_usd".to_string())?;
        Ok(if created_at.starts_with(date_prefix) {
            total + cost
        } else {
            total
        })
    })
}

#[cfg(feature = "pg")]
fn pg_provider_audit_rows(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for row in &rows {
        let itc: Option<i32> = row.get(4);
        let otc: Option<i32> = row.get(5);
        let lat: Option<i32> = row.get(8);
        result.push(json!({
            "event_id": row.get::<_, String>(0),
            "dispatch_id": row.get::<_, String>(1),
            "provider_id": row.get::<_, String>(2),
            "event_type": row.get::<_, String>(3),
            "input_token_count": itc.map(|v| v as i64),
            "output_token_count": otc.map(|v| v as i64),
            "cost": row.get::<_, Option<f64>>(6),
            "currency": row.get::<_, Option<String>>(7),
            "latency_ms": lat.map(|v| v as i64),
            "error_domain": row.get::<_, Option<String>>(9),
            "redaction_status": row.get::<_, String>(10),
            "created_at": row.get::<_, String>(11),
        }));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_reservation() -> crate::provider::ProviderAuditEvent {
        crate::provider::ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: "paudit-reservation-test".to_string(),
            dispatch_id: "memory-embedding-test".to_string(),
            provider_id: crate::provider::embedding::OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            event_type: "request_reserved".to_string(),
            input_token_count: None,
            output_token_count: None,
            cost: Some(0.0),
            currency: Some("USD".to_string()),
            latency_ms: None,
            error_domain: None,
            redaction_status: "redacted".to_string(),
            created_at: "2026-07-16T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn free_embedding_reservation_owner_checks_every_modeled_charge_dimension() {
        let event = zero_reservation();
        let mut contract = crate::provider::embedding::pinned_free_embedding_contract_evidence();
        assert!(validate_verified_free_embedding_reservation(&event, &contract).is_ok());
        for mutate in [
            |value: &mut crate::provider::embedding::EmbeddingPricingEvidence| {
                value.web_search_cost_per_request_usd = Some(0.000_001);
            },
            |value: &mut crate::provider::embedding::EmbeddingPricingEvidence| {
                value.internal_reasoning_cost_per_token_usd = Some(0.000_001);
            },
            |value: &mut crate::provider::embedding::EmbeddingPricingEvidence| {
                value.input_cache_read_cost_per_token_usd = Some(0.000_001);
            },
            |value: &mut crate::provider::embedding::EmbeddingPricingEvidence| {
                value.input_cache_write_cost_per_token_usd = Some(0.000_001);
            },
        ] {
            let original = contract.clone();
            mutate(&mut contract.pricing);
            assert!(validate_verified_free_embedding_reservation(&event, &contract).is_err());
            contract = original;
        }
        contract.pricing_dimension_applicability.remove("request");
        assert!(validate_verified_free_embedding_reservation(&event, &contract).is_err());
    }
}
