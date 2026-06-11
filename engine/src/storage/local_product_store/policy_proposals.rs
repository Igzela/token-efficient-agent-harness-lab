use rusqlite::{params, Row};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::{append_audit_locked, collect_values, str_at, DatabaseConnection, LocalProductStore};
use crate::dispatch_decision::{TASK_DOMAINS, TASK_INTENTS};
use crate::model_selector::{is_safe_policy_override_tier, DispatchRoutingPolicy};

const POLICY_PROPOSAL_SCHEMA_VERSION: &str = "controlled_loop_policy_proposal.v1";

#[derive(Debug, Clone)]
struct ProposalFields {
    title: String,
    summary: Option<String>,
    task_domain: String,
    task_intent: String,
    target_tier: String,
    evidence: Value,
    payload: Value,
}

impl LocalProductStore {
    pub fn create_policy_proposal(&self, request: &Value, actor: &str) -> Result<Value, String> {
        let fields = validate_proposal_request(request)?;
        let now = self.now();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_policy_proposal_sequence(conn)?;
                let proposal_id = format!("policy-proposal-{sequence:04}");
                let proposal = proposal_value(&proposal_id, "pending", &now, &now, &fields, None);
                conn.execute(
                    "INSERT INTO controlled_loop_policy_proposals
                     (proposal_sequence, proposal_id, created_at, updated_at, status, title,
                      summary, task_domain, task_intent, target_tier, evidence_json,
                      approval_json, proposal_json)
                     VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11)",
                    params![
                        sequence,
                        proposal_id,
                        now,
                        now,
                        fields.title,
                        fields.summary,
                        fields.task_domain,
                        fields.task_intent,
                        fields.target_tier,
                        fields.evidence.to_string(),
                        proposal.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "policy_proposal.create",
                    &proposal_id,
                    &json!({
                        "scope": "tier_map_override",
                        "task_domain": fields.task_domain,
                        "task_intent": fields.task_intent,
                        "target_tier": fields.target_tier,
                        "requires_human_approval": true,
                    }),
                )?;
                Ok(proposal)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let proposal_id = next_pg_policy_proposal_id(client)?;
                let proposal = proposal_value(&proposal_id, "pending", &now, &now, &fields, None);
                client
                    .execute(
                        "INSERT INTO controlled_loop_policy_proposals
                         (proposal_id, created_at, updated_at, status, title, summary,
                          task_domain, task_intent, target_tier, evidence_json,
                          approval_json, proposal_json)
                         VALUES ($1, $2, $3, 'pending', $4, $5, $6, $7, $8, $9, NULL, $10)",
                        &[
                            &proposal_id,
                            &now,
                            &now,
                            &fields.title,
                            &fields.summary,
                            &fields.task_domain,
                            &fields.task_intent,
                            &fields.target_tier,
                            &fields.evidence.to_string(),
                            &proposal.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({
                    "scope": "tier_map_override",
                    "task_domain": fields.task_domain,
                    "task_intent": fields.task_intent,
                    "target_tier": fields.target_tier,
                    "requires_human_approval": true,
                })
                .to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'policy_proposal.create', $3, $4)",
                        &[&self.now(), &actor, &proposal_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(proposal)
            }),
        }
    }

    pub fn list_policy_proposals(
        &self,
        limit: i64,
        offset: i64,
        status: Option<&str>,
    ) -> Result<Value, String> {
        let limit = limit.clamp(0, 500);
        let offset = offset.max(0);
        let proposals = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                if let Some(status) = clean(status) {
                    let mut stmt = conn
                        .prepare(
                            "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                    status, title, summary, task_domain, task_intent,
                                    target_tier, evidence_json, approval_json, proposal_json
                             FROM controlled_loop_policy_proposals
                             WHERE status = ?1
                             ORDER BY proposal_sequence DESC
                             LIMIT ?2 OFFSET ?3",
                        )
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map(params![status, limit, offset], policy_proposal_row)
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)
                } else {
                    let mut stmt = conn
                        .prepare(
                            "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                    status, title, summary, task_domain, task_intent,
                                    target_tier, evidence_json, approval_json, proposal_json
                             FROM controlled_loop_policy_proposals
                             ORDER BY proposal_sequence DESC
                             LIMIT ?1 OFFSET ?2",
                        )
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map(params![limit, offset], policy_proposal_row)
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)
                }
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = if let Some(status) = clean(status) {
                    client
                        .query(
                            "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                    status, title, summary, task_domain, task_intent,
                                    target_tier, evidence_json, approval_json, proposal_json
                             FROM controlled_loop_policy_proposals
                             WHERE status = $1
                             ORDER BY proposal_sequence DESC
                             LIMIT $2 OFFSET $3",
                            &[&status, &limit, &offset],
                        )
                        .map_err(|e| e.to_string())?
                } else {
                    client
                        .query(
                            "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                    status, title, summary, task_domain, task_intent,
                                    target_tier, evidence_json, approval_json, proposal_json
                             FROM controlled_loop_policy_proposals
                             ORDER BY proposal_sequence DESC
                             LIMIT $1 OFFSET $2",
                            &[&limit, &offset],
                        )
                        .map_err(|e| e.to_string())?
                };
                Ok(rows.iter().map(pg_policy_proposal_row).collect::<Vec<_>>())
            })?,
        };

        Ok(json!({
            "schema_version": "axum_api.v1",
            "proposals": proposals,
            "total": proposals.len(),
            "limit": limit,
            "offset": offset,
        }))
    }

    pub fn get_policy_proposal(&self, proposal_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                status, title, summary, task_domain, task_intent,
                                target_tier, evidence_json, approval_json, proposal_json
                         FROM controlled_loop_policy_proposals
                         WHERE proposal_id = ?1
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![proposal_id], policy_proposal_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(err)) => Err(err.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                status, title, summary, task_domain, task_intent,
                                target_tier, evidence_json, approval_json, proposal_json
                         FROM controlled_loop_policy_proposals
                         WHERE proposal_id = $1
                         LIMIT 1",
                        &[&proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.first().map(pg_policy_proposal_row))
            }),
        }
    }

    pub fn approve_policy_proposal(
        &self,
        proposal_id: &str,
        actor: &str,
        reason: Option<&str>,
        confirm_policy_override: bool,
    ) -> Result<Value, String> {
        if !confirm_policy_override {
            return Err("confirm_policy_override is required".to_string());
        }
        self.transition_policy_proposal(proposal_id, "active", actor, reason, true)
    }

    pub fn reject_policy_proposal(
        &self,
        proposal_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        self.transition_policy_proposal(proposal_id, "rejected", actor, reason, false)
    }

    pub fn deactivate_policy_proposal(
        &self,
        proposal_id: &str,
        actor: &str,
        reason: Option<&str>,
        confirm_policy_override: bool,
    ) -> Result<Value, String> {
        if !confirm_policy_override {
            return Err("confirm_policy_override is required".to_string());
        }
        self.transition_policy_proposal(proposal_id, "inactive", actor, reason, false)
    }

    pub fn rollback_policy_proposal(
        &self,
        proposal_id: &str,
        actor: &str,
        reason: Option<&str>,
        confirm_policy_override: bool,
    ) -> Result<Value, String> {
        if !confirm_policy_override {
            return Err("confirm_policy_override is required".to_string());
        }
        self.transition_policy_proposal(proposal_id, "rolled_back", actor, reason, false)
    }

    pub fn active_routing_policy(&self) -> Result<Option<DispatchRoutingPolicy>, String> {
        let active = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                status, title, summary, task_domain, task_intent,
                                target_tier, evidence_json, approval_json, proposal_json
                         FROM controlled_loop_policy_proposals
                         WHERE status = 'active'
                         ORDER BY proposal_sequence ASC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], policy_proposal_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_sequence, proposal_id, created_at, updated_at,
                                status, title, summary, task_domain, task_intent,
                                target_tier, evidence_json, approval_json, proposal_json
                         FROM controlled_loop_policy_proposals
                         WHERE status = 'active'
                         ORDER BY proposal_sequence ASC",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_policy_proposal_row).collect::<Vec<_>>())
            })?,
        };

        if active.is_empty() {
            return Ok(None);
        }

        let mut overrides = BTreeMap::new();
        for proposal in active {
            let domain = str_at(&proposal, &["task_domain"]).unwrap_or("");
            let intent = str_at(&proposal, &["task_intent"]).unwrap_or("");
            let tier = str_at(&proposal, &["target_tier"]).unwrap_or("");
            validate_policy_override(domain, intent, tier)?;
            overrides.insert(format!("{domain}_{intent}"), tier.to_string());
        }

        let mut policy = DispatchRoutingPolicy::default();
        for (key, tier) in overrides {
            policy.tier_map.insert(key, tier);
        }
        policy.policy_id = "controlled_loop_v1".to_string();
        policy.description =
            "Default routing policy with explicit human-approved safe tier overrides".to_string();
        Ok(Some(policy))
    }

    fn transition_policy_proposal(
        &self,
        proposal_id: &str,
        new_status: &str,
        actor: &str,
        reason: Option<&str>,
        supersede_same_key: bool,
    ) -> Result<Value, String> {
        let current = self
            .get_policy_proposal(proposal_id)?
            .ok_or_else(|| format!("proposal not found: {proposal_id}"))?;
        let current_status = str_at(&current, &["status"]).unwrap_or("");
        if new_status == "active" && current_status != "pending" {
            return Err(format!(
                "proposal {proposal_id} cannot be approved from status {current_status}"
            ));
        }
        if matches!(new_status, "inactive" | "rolled_back") && current_status != "active" {
            return Err(format!(
                "proposal {proposal_id} cannot be deactivated from status {current_status}"
            ));
        }

        let task_domain = str_at(&current, &["task_domain"]).unwrap_or("");
        let task_intent = str_at(&current, &["task_intent"]).unwrap_or("");
        let target_tier = str_at(&current, &["target_tier"]).unwrap_or("");
        validate_policy_override(task_domain, task_intent, target_tier)?;

        let now = self.now();
        let approval = json!({
            "actor": actor,
            "reason": reason,
            "confirmed_human_approval": new_status == "active",
            "status": new_status,
            "created_at": now,
            "scope": "tier_map_override",
        });
        let mut proposal = current.clone();
        if let Some(obj) = proposal.as_object_mut() {
            obj.insert("status".to_string(), json!(new_status));
            obj.insert("updated_at".to_string(), json!(now));
            obj.insert("approval".to_string(), approval.clone());
        }

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                if supersede_same_key {
                    conn.execute(
                        "UPDATE controlled_loop_policy_proposals
                         SET status = 'superseded', updated_at = ?1
                         WHERE status = 'active'
                           AND task_domain = ?2
                           AND task_intent = ?3
                           AND proposal_id <> ?4",
                        params![now, task_domain, task_intent, proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                conn.execute(
                    "UPDATE controlled_loop_policy_proposals
                     SET status = ?1, updated_at = ?2, approval_json = ?3, proposal_json = ?4
                     WHERE proposal_id = ?5",
                    params![
                        new_status,
                        now,
                        approval.to_string(),
                        proposal.to_string(),
                        proposal_id
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    &format!("policy_proposal.{new_status}"),
                    proposal_id,
                    &json!({
                        "task_domain": task_domain,
                        "task_intent": task_intent,
                        "target_tier": target_tier,
                        "reason": reason,
                        "requires_human_approval": new_status == "active",
                    }),
                )?;
                Ok(proposal)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if supersede_same_key {
                    client
                        .execute(
                            "UPDATE controlled_loop_policy_proposals
                             SET status = 'superseded', updated_at = $1
                             WHERE status = 'active'
                               AND task_domain = $2
                               AND task_intent = $3
                               AND proposal_id <> $4",
                            &[&now, &task_domain, &task_intent, &proposal_id],
                        )
                        .map_err(|e| e.to_string())?;
                }
                client
                    .execute(
                        "UPDATE controlled_loop_policy_proposals
                         SET status = $1, updated_at = $2, approval_json = $3, proposal_json = $4
                         WHERE proposal_id = $5",
                        &[
                            &new_status,
                            &now,
                            &approval.to_string(),
                            &proposal.to_string(),
                            &proposal_id,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({
                    "task_domain": task_domain,
                    "task_intent": task_intent,
                    "target_tier": target_tier,
                    "reason": reason,
                    "requires_human_approval": new_status == "active",
                })
                .to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[
                            &self.now(),
                            &actor,
                            &format!("policy_proposal.{new_status}"),
                            &proposal_id,
                            &details,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(proposal)
            }),
        }
    }
}

fn validate_proposal_request(request: &Value) -> Result<ProposalFields, String> {
    let payload = request.get("payload").cloned().unwrap_or_else(|| json!({}));
    let task_domain = first_str(
        request,
        &[
            &["task_domain"],
            &["domain"],
            &["payload", "task_domain"],
            &["payload", "domain"],
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        first_str(request, &[&["task_class"], &["payload", "task_class"]])
            .and_then(|value| value.split_once('_').map(|(domain, _)| domain.to_string()))
    })
    .ok_or_else(|| "task_domain is required".to_string())?;
    let task_intent = first_str(
        request,
        &[
            &["task_intent"],
            &["intent"],
            &["payload", "task_intent"],
            &["payload", "intent"],
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        first_str(request, &[&["task_class"], &["payload", "task_class"]])
            .and_then(|value| value.split_once('_').map(|(_, intent)| intent.to_string()))
    })
    .ok_or_else(|| "task_intent is required".to_string())?;
    let target_tier = first_str(
        request,
        &[
            &["target_tier"],
            &["tier"],
            &["payload", "target_tier"],
            &["payload", "tier"],
        ],
    )
    .ok_or_else(|| "target_tier is required".to_string())?
    .to_string();

    validate_policy_override(&task_domain, &task_intent, &target_tier)?;
    let title = clean(first_str(request, &[&["title"], &["payload", "title"]]))
        .map(str::to_string)
        .unwrap_or_else(|| format!("Override {task_domain}_{task_intent} -> {target_tier}"));
    let summary =
        clean(first_str(request, &[&["summary"], &["payload", "summary"]])).map(str::to_string);
    let evidence = request
        .get("evidence")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(ProposalFields {
        title,
        summary,
        task_domain,
        task_intent,
        target_tier,
        evidence,
        payload,
    })
}

fn validate_policy_override(
    task_domain: &str,
    task_intent: &str,
    target_tier: &str,
) -> Result<(), String> {
    if !TASK_DOMAINS.contains(&task_domain) {
        return Err(format!("unsupported task_domain: {task_domain}"));
    }
    if !TASK_INTENTS.contains(&task_intent) {
        return Err(format!("unsupported task_intent: {task_intent}"));
    }
    if !is_safe_policy_override_tier(target_tier) {
        return Err(format!(
            "target_tier requires explicit unsupported boundary expansion approval: {target_tier}"
        ));
    }
    Ok(())
}

fn proposal_value(
    proposal_id: &str,
    status: &str,
    created_at: &str,
    updated_at: &str,
    fields: &ProposalFields,
    approval: Option<Value>,
) -> Value {
    json!({
        "schema_version": POLICY_PROPOSAL_SCHEMA_VERSION,
        "proposal_id": proposal_id,
        "created_at": created_at,
        "updated_at": updated_at,
        "status": status,
        "title": fields.title,
        "summary": fields.summary,
        "task_domain": fields.task_domain,
        "task_intent": fields.task_intent,
        "task_class": format!("{}_{}", fields.task_domain, fields.task_intent),
        "policy_key": format!("{}_{}", fields.task_domain, fields.task_intent),
        "target_tier": fields.target_tier,
        "tier": fields.target_tier,
        "requires_human_approval": true,
        "scope": "tier_map_override",
        "payload": fields.payload,
        "evidence": fields.evidence,
        "approval": approval,
        "boundaries": {
            "provider_cli_execution_boundary_expansion": "requires_separate_human_approval",
            "auth_security_boundary_changes": "requires_separate_human_approval",
            "db_migrations": "limited_to_policy_proposal_v12",
            "hard_constraint_mutation": "disabled",
            "target_repository_writes": "disabled",
            "destructive_operations": "requires_separate_human_approval"
        }
    })
}

fn policy_proposal_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let evidence_text: String = row.get(10)?;
    let approval_text: Option<String> = row.get(11)?;
    let proposal_text: String = row.get(12)?;
    let mut proposal: Value = serde_json::from_str(&proposal_text).unwrap_or(Value::Null);
    overlay_row_fields(
        &mut proposal,
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        &evidence_text,
        approval_text.as_deref(),
    );
    Ok(proposal)
}

#[cfg(feature = "pg")]
fn pg_policy_proposal_row(row: &postgres::Row) -> Value {
    let evidence_text: String = row.get(10);
    let approval_text: Option<String> = row.get(11);
    let proposal_text: String = row.get(12);
    let mut proposal: Value = serde_json::from_str(&proposal_text).unwrap_or(Value::Null);
    overlay_row_fields(
        &mut proposal,
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        row.get(5),
        row.get(6),
        row.get(7),
        row.get(8),
        row.get(9),
        &evidence_text,
        approval_text.as_deref(),
    );
    proposal
}

#[allow(clippy::too_many_arguments)]
fn overlay_row_fields(
    proposal: &mut Value,
    sequence: i64,
    proposal_id: String,
    created_at: String,
    updated_at: String,
    status: String,
    title: String,
    summary: Option<String>,
    task_domain: String,
    task_intent: String,
    target_tier: String,
    evidence_text: &str,
    approval_text: Option<&str>,
) {
    let evidence = serde_json::from_str(evidence_text).unwrap_or(Value::Null);
    let approval = approval_text
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null);
    if let Some(obj) = proposal.as_object_mut() {
        obj.insert("proposal_sequence".to_string(), json!(sequence));
        obj.insert("proposal_id".to_string(), json!(proposal_id));
        obj.insert("created_at".to_string(), json!(created_at));
        obj.insert("updated_at".to_string(), json!(updated_at));
        obj.insert("status".to_string(), json!(status));
        obj.insert("title".to_string(), json!(title));
        obj.insert("summary".to_string(), json!(summary));
        obj.insert("task_domain".to_string(), json!(task_domain));
        obj.insert("task_intent".to_string(), json!(task_intent));
        obj.insert(
            "task_class".to_string(),
            json!(format!("{}_{}", task_domain, task_intent)),
        );
        obj.insert(
            "policy_key".to_string(),
            json!(format!("{}_{}", task_domain, task_intent)),
        );
        obj.insert("target_tier".to_string(), json!(target_tier));
        obj.insert("tier".to_string(), json!(target_tier));
        obj.insert("evidence".to_string(), evidence);
        obj.insert("approval".to_string(), approval);
    }
}

fn next_policy_proposal_sequence(conn: &rusqlite::Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(proposal_sequence), 0) + 1 FROM controlled_loop_policy_proposals",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn next_pg_policy_proposal_id(client: &mut postgres::Client) -> Result<String, String> {
    let row = client
        .query_one(
            "SELECT COALESCE(MAX(proposal_sequence), 0) + 1 FROM controlled_loop_policy_proposals",
            &[],
        )
        .map_err(|e| e.to_string())?;
    let sequence: i64 = row.get(0);
    Ok(format!("policy-proposal-{sequence:04}"))
}

fn first_str<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths.iter().find_map(|path| str_at(value, path))
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn proposal_lifecycle_builds_active_policy() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
        let proposal = store
            .create_policy_proposal(
                &json!({
                    "task_domain": "docs",
                    "task_intent": "review",
                    "target_tier": "verifier",
                    "payload": {"type": "tier_map_override"},
                }),
                "actor",
            )
            .unwrap();
        let proposal_id = proposal["proposal_id"].as_str().unwrap();
        store
            .approve_policy_proposal(proposal_id, "approver", Some("pilot"), true)
            .unwrap();

        let policy = store.active_routing_policy().unwrap().unwrap();
        assert_eq!(policy.tier_map["docs_review"], "verifier");
    }

    #[test]
    fn proposal_rejects_cli_tier() {
        let request = json!({
            "task_domain": "docs",
            "task_intent": "review",
            "target_tier": "codex_cli",
        });
        assert!(validate_proposal_request(&request).is_err());
    }
}
