use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::workflow::tool_registry::{
    validate_tool_hook_contract, HookAction, HookEvaluation, HookType, ToolHook,
};

const MAX_TOOL_POLICY_ID_BYTES: usize = 256;
const MAX_SNAPSHOT_ALLOWLIST_TOOLS: usize = 128;
const MAX_SNAPSHOT_HOOKS: usize = 32;

#[derive(Clone, Debug)]
pub(crate) struct ToolExecutionPolicySnapshot {
    pub sha256: String,
    pub allowlist_configured: bool,
    pub tool_allowed: bool,
    capability_registered: bool,
    capability_requires_approval: bool,
    hooks: Vec<ToolHook>,
}

impl ToolExecutionPolicySnapshot {
    pub(crate) fn capability_registered(&self) -> bool {
        self.capability_registered
    }

    pub(crate) fn capability_requires_approval(&self) -> bool {
        self.capability_requires_approval
    }

    pub(crate) fn evaluate_hooks(
        &self,
        hook_type: &HookType,
        tool_name: &str,
        context: &Value,
    ) -> Result<HookEvaluation, String> {
        super::tool_registry::evaluate_tool_hooks(&self.hooks, hook_type, tool_name, context)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ToolExecutionGate {
    AwaitingApproval { approval_id: String },
    Authorized,
    Rejected,
    ConsumedOutcomeUnknown,
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_TOOL_POLICY_ID_BYTES {
        return Err(format!(
            "{field} must contain 1..={MAX_TOOL_POLICY_ID_BYTES} bytes"
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("{field} contains unsupported characters"));
    }
    Ok(())
}

fn validate_action_hash(action_sha256: &str) -> Result<(), String> {
    if action_sha256.len() != 64 || !action_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("action_sha256 must be a 64-character hexadecimal digest".to_string());
    }
    Ok(())
}

fn policy_snapshot_from_value(
    value: Value,
    capability_requires_approval: bool,
    hooks: Vec<ToolHook>,
) -> Result<ToolExecutionPolicySnapshot, String> {
    let allowlist_configured = value
        .pointer("/allowlist/configured")
        .and_then(Value::as_bool)
        .ok_or_else(|| "tool policy snapshot allowlist state is invalid".to_string())?;
    let tool_allowed = value
        .pointer("/allowlist/tool_allowed")
        .and_then(Value::as_bool)
        .ok_or_else(|| "tool policy snapshot allowlist decision is invalid".to_string())?;
    let capability_registered = value
        .get("capability")
        .is_some_and(|capability| !capability.is_null());
    let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    Ok(ToolExecutionPolicySnapshot {
        sha256: hex::encode(Sha256::digest(bytes)),
        allowlist_configured,
        tool_allowed,
        capability_registered,
        capability_requires_approval,
        hooks,
    })
}

fn parse_snapshot_json(raw: Option<String>, field: &str) -> Result<Option<Value>, String> {
    raw.map(|raw| {
        serde_json::from_str(&raw)
            .map_err(|error| format!("invalid stored tool policy {field} JSON: {error}"))
    })
    .transpose()
}

fn sqlite_tool_execution_policy_snapshot(
    conn: &rusqlite::Connection,
    profile_id: &str,
    tool_name: &str,
) -> Result<ToolExecutionPolicySnapshot, String> {
    let configured = conn
        .query_row(
            "SELECT 1 FROM tool_allowlist_profiles WHERE profile_id = ?1",
            params![profile_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    let mut allowlist = Vec::new();
    if configured {
        let mut statement = conn
            .prepare(
                "SELECT tool_name FROM tool_allowlists
                 WHERE profile_id = ?1 ORDER BY tool_name",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![profile_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for row in rows {
            let value = row.map_err(|error| error.to_string())?;
            validate_identifier("allowlisted tool_name", &value)?;
            allowlist.push(value);
        }
    }
    if allowlist.len() > MAX_SNAPSHOT_ALLOWLIST_TOOLS {
        return Err("configured tool allowlist exceeds bounded snapshot size".to_string());
    }
    let tool_allowed = !configured || allowlist.iter().any(|value| value == tool_name);

    let capability = conn
        .query_row(
            "SELECT description, input_schema_json, output_schema_json,
                    requires_approval, risk_level
             FROM tool_capabilities WHERE tool_name = ?1",
            params![tool_name],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .map(
            |(description, input, output, requires_approval, risk_level)| {
                if !matches!(risk_level.as_str(), "low" | "medium" | "high") {
                    return Err(format!(
                        "invalid stored tool capability risk level: {risk_level}"
                    ));
                }
                Ok(json!({
                    "description": description,
                    "input_schema": parse_snapshot_json(input, "input_schema")?,
                    "output_schema": parse_snapshot_json(output, "output_schema")?,
                    "requires_approval": requires_approval != 0,
                    "risk_level": risk_level,
                }))
            },
        )
        .transpose()?;
    let capability_requires_approval = capability
        .as_ref()
        .and_then(|value| value.get("requires_approval"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut hooks = Vec::new();
    let mut policy_hooks = Vec::new();
    let mut statement = conn
        .prepare(
            "SELECT hook_id, hook_type, tool_name, condition_json, action,
                    action_config_json
             FROM tool_hooks
             WHERE enabled = 1 AND (tool_name IS NULL OR tool_name = ?1)
             ORDER BY hook_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![tool_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (hook_id, hook_type, target_tool, condition, action, action_config) =
            row.map_err(|error| error.to_string())?;
        validate_identifier("hook_id", &hook_id)?;
        let condition = parse_snapshot_json(condition, "hook condition")?;
        let action_config = parse_snapshot_json(action_config, "hook action_config")?;
        validate_tool_hook_contract(
            &hook_type,
            condition.as_ref(),
            &action,
            action_config.as_ref(),
        )?;
        let parsed_hook_type = HookType::parse_str(&hook_type)
            .ok_or_else(|| format!("invalid stored tool hook type: {hook_type}"))?;
        let parsed_action = HookAction::parse_str(&action)
            .ok_or_else(|| format!("invalid stored tool hook action: {action}"))?;
        hooks.push(json!({
            "hook_id": &hook_id,
            "hook_type": &hook_type,
            "tool_name": &target_tool,
            "condition": &condition,
            "action": &action,
            "action_config": &action_config,
        }));
        policy_hooks.push(ToolHook {
            hook_id,
            hook_type: parsed_hook_type,
            tool_name: target_tool,
            condition,
            action: parsed_action,
            action_config,
            enabled: true,
            created_at: String::new(),
        });
    }
    if hooks.len() > MAX_SNAPSHOT_HOOKS {
        return Err("enabled tool hook count exceeds bounded snapshot size".to_string());
    }

    policy_snapshot_from_value(
        json!({
            "schema_version": "tool_execution_policy_snapshot.v1",
            "profile_id": profile_id,
            "tool_name": tool_name,
            "allowlist": {
                "configured": configured,
                "tool_names": allowlist,
                "tool_allowed": tool_allowed,
            },
            "capability": capability,
            "hooks": hooks,
        }),
        capability_requires_approval,
        policy_hooks,
    )
}

#[cfg(feature = "pg")]
fn pg_tool_execution_policy_snapshot(
    client: &mut impl postgres::GenericClient,
    profile_id: &str,
    tool_name: &str,
) -> Result<ToolExecutionPolicySnapshot, String> {
    let configured = client
        .query_opt(
            "SELECT 1 FROM tool_allowlist_profiles WHERE profile_id = $1",
            &[&profile_id],
        )
        .map_err(|error| error.to_string())?
        .is_some();
    let allowlist = if configured {
        client
            .query(
                "SELECT tool_name FROM tool_allowlists
                 WHERE profile_id = $1 ORDER BY tool_name",
                &[&profile_id],
            )
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if allowlist.len() > MAX_SNAPSHOT_ALLOWLIST_TOOLS {
        return Err("configured tool allowlist exceeds bounded snapshot size".to_string());
    }
    for value in &allowlist {
        validate_identifier("allowlisted tool_name", value)?;
    }
    let tool_allowed = !configured || allowlist.iter().any(|value| value == tool_name);

    let capability = client
        .query_opt(
            "SELECT description, input_schema_json, output_schema_json,
                    requires_approval, risk_level
             FROM tool_capabilities WHERE tool_name = $1",
            &[&tool_name],
        )
        .map_err(|error| error.to_string())?
        .map(|row| {
            let description = row.get::<_, String>(0);
            let input = row.get::<_, Option<String>>(1);
            let output = row.get::<_, Option<String>>(2);
            let requires_approval = row.get::<_, i32>(3);
            let risk_level = row.get::<_, String>(4);
            if !matches!(risk_level.as_str(), "low" | "medium" | "high") {
                return Err(format!(
                    "invalid stored tool capability risk level: {risk_level}"
                ));
            }
            Ok(json!({
                "description": description,
                "input_schema": parse_snapshot_json(input, "input_schema")?,
                "output_schema": parse_snapshot_json(output, "output_schema")?,
                "requires_approval": requires_approval != 0,
                "risk_level": risk_level,
            }))
        })
        .transpose()?;
    let capability_requires_approval = capability
        .as_ref()
        .and_then(|value| value.get("requires_approval"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let rows = client
        .query(
            "SELECT hook_id, hook_type, tool_name, condition_json, action,
                    action_config_json
             FROM tool_hooks
             WHERE enabled = 1 AND (tool_name IS NULL OR tool_name = $1)
             ORDER BY hook_id",
            &[&tool_name],
        )
        .map_err(|error| error.to_string())?;
    if rows.len() > MAX_SNAPSHOT_HOOKS {
        return Err("enabled tool hook count exceeds bounded snapshot size".to_string());
    }
    let mut hooks = Vec::with_capacity(rows.len());
    let mut policy_hooks = Vec::with_capacity(rows.len());
    for row in rows {
        let hook_id = row.get::<_, String>(0);
        let hook_type = row.get::<_, String>(1);
        let target_tool = row.get::<_, Option<String>>(2);
        let condition = parse_snapshot_json(row.get(3), "hook condition")?;
        let action = row.get::<_, String>(4);
        let action_config = parse_snapshot_json(row.get(5), "hook action_config")?;
        validate_identifier("hook_id", &hook_id)?;
        validate_tool_hook_contract(
            &hook_type,
            condition.as_ref(),
            &action,
            action_config.as_ref(),
        )?;
        let parsed_hook_type = HookType::parse_str(&hook_type)
            .ok_or_else(|| format!("invalid stored tool hook type: {hook_type}"))?;
        let parsed_action = HookAction::parse_str(&action)
            .ok_or_else(|| format!("invalid stored tool hook action: {action}"))?;
        hooks.push(json!({
            "hook_id": &hook_id,
            "hook_type": &hook_type,
            "tool_name": &target_tool,
            "condition": &condition,
            "action": &action,
            "action_config": &action_config,
        }));
        policy_hooks.push(ToolHook {
            hook_id,
            hook_type: parsed_hook_type,
            tool_name: target_tool,
            condition,
            action: parsed_action,
            action_config,
            enabled: true,
            created_at: String::new(),
        });
    }

    policy_snapshot_from_value(
        json!({
            "schema_version": "tool_execution_policy_snapshot.v1",
            "profile_id": profile_id,
            "tool_name": tool_name,
            "allowlist": {
                "configured": configured,
                "tool_names": allowlist,
                "tool_allowed": tool_allowed,
            },
            "capability": capability,
            "hooks": hooks,
        }),
        capability_requires_approval,
        policy_hooks,
    )
}

fn require_policy_snapshot(
    current: &ToolExecutionPolicySnapshot,
    expected_sha256: &str,
) -> Result<(), String> {
    validate_action_hash(expected_sha256)?;
    if current.sha256 != expected_sha256 {
        return Err("tool execution policy changed before authorization claim".to_string());
    }
    Ok(())
}

fn gate_from_status(status: &str, approval_id: String) -> Result<ToolExecutionGate, String> {
    match status {
        "requested" => Ok(ToolExecutionGate::AwaitingApproval { approval_id }),
        "rejected" => Ok(ToolExecutionGate::Rejected),
        "consumed" => Ok(ToolExecutionGate::ConsumedOutcomeUnknown),
        other => Err(format!(
            "unsupported tool execution authorization status: {other}"
        )),
    }
}

impl LocalProductStore {
    pub(crate) fn current_tool_execution_policy_snapshot(
        &self,
        profile_id: &str,
        tool_name: &str,
    ) -> Result<ToolExecutionPolicySnapshot, String> {
        validate_identifier("profile_id", profile_id)?;
        validate_identifier("tool_name", tool_name)?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                sqlite_tool_execution_policy_snapshot(conn, profile_id, tool_name)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_tool_execution_policy_snapshot(client, profile_id, tool_name)
            }),
        }
    }

    pub(crate) fn gate_tool_execution(
        &self,
        run_id: &str,
        node_id: &str,
        tool_name: &str,
        profile_id: &str,
        policy_sha256: &str,
        action_sha256: &str,
        reason: &str,
    ) -> Result<ToolExecutionGate, String> {
        validate_identifier("run_id", run_id)?;
        validate_identifier("node_id", node_id)?;
        validate_identifier("tool_name", tool_name)?;
        validate_identifier("profile_id", profile_id)?;
        validate_action_hash(policy_sha256)?;
        validate_action_hash(action_sha256)?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    let policy =
                        sqlite_tool_execution_policy_snapshot(conn, profile_id, tool_name)?;
                    require_policy_snapshot(&policy, policy_sha256)?;
                    let node_exists: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM workflow_run_nodes
                             WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    if node_exists != 1 {
                        return Err(format!(
                            "workflow node not found for tool authorization: {run_id}/{node_id}"
                        ));
                    }

                    let existing: Option<(String, String, String, String)> = conn
                        .query_row(
                            "SELECT action_sha256, tool_name, profile_id, status
                             FROM tool_execution_authorizations
                             WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                        )
                        .optional()
                        .map_err(|error| error.to_string())?;
                    if let Some((bound_hash, bound_tool, bound_profile, status)) = existing {
                        if bound_hash != action_sha256
                            || bound_tool != tool_name
                            || bound_profile != profile_id
                        {
                            return Err(
                                "tool authorization binding changed for leased node".to_string()
                            );
                        }
                        let approval_id: String = conn
                            .query_row(
                                "SELECT requested_approval_id
                                 FROM tool_execution_authorizations
                                 WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, node_id],
                                |row| row.get(0),
                            )
                            .map_err(|error| error.to_string())?;
                        if status == "approved" {
                            let now = self.now();
                            let changed = conn
                                .execute(
                                    "UPDATE tool_execution_authorizations
                                     SET status = 'consumed', updated_at = ?1
                                     WHERE run_id = ?2 AND node_id = ?3 AND status = 'approved'",
                                    params![now, run_id, node_id],
                                )
                                .map_err(|error| error.to_string())?;
                            if changed != 1 {
                                return Err(
                                    "tool authorization was consumed concurrently".to_string()
                                );
                            }
                            append_audit_locked(
                                conn,
                                &now,
                                "tool-policy",
                                "tool_execution.authorization_consumed",
                                run_id,
                                &json!({
                                    "node_id": node_id,
                                    "tool_name": tool_name,
                                    "profile_id": profile_id,
                                    "policy_sha256": policy_sha256,
                                    "action_sha256": action_sha256,
                                    "approval_id": approval_id,
                                }),
                            )?;
                            return Ok(ToolExecutionGate::Authorized);
                        }
                        return gate_from_status(&status, approval_id);
                    }

                    let sequence: i64 = conn
                        .query_row(
                            "SELECT COALESCE(MAX(approval_sequence), 0) + 1
                             FROM workflow_run_approvals",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    let approval_id = format!("workflow-approval-{sequence:04}");
                    let now = self.now();
                    let approval = json!({
                        "approval_sequence": sequence,
                        "approval_id": approval_id,
                        "run_id": run_id,
                        "node_id": node_id,
                        "decision": "requested",
                        "actor": "tool-policy",
                        "reason": reason,
                        "created_at": now,
                        "approval_kind": "tool_execution",
                        "tool_name": tool_name,
                        "profile_id": profile_id,
                        "policy_sha256": policy_sha256,
                        "action_sha256": action_sha256,
                        "metadata_only": false,
                        "execution_authority": "single_tool_invocation",
                    });
                    conn.execute(
                        "INSERT INTO workflow_run_approvals
                         (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                          created_at, approval_json)
                         VALUES (?1, ?2, ?3, ?4, 'requested', 'tool-policy', ?5, ?6, ?7)",
                        params![
                            sequence,
                            approval_id,
                            run_id,
                            node_id,
                            reason,
                            now,
                            approval.to_string(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    conn.execute(
                        "INSERT INTO tool_execution_authorizations
                         (run_id, node_id, action_sha256, tool_name, profile_id, status,
                          requested_approval_id, resolved_by, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'requested', ?6, NULL, ?7, ?7)",
                        params![
                            run_id,
                            node_id,
                            action_sha256,
                            tool_name,
                            profile_id,
                            approval_id,
                            now,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    append_audit_locked(
                        conn,
                        &now,
                        "tool-policy",
                        "tool_execution.approval_requested",
                        run_id,
                        &json!({
                            "node_id": node_id,
                            "tool_name": tool_name,
                            "profile_id": profile_id,
                            "policy_sha256": policy_sha256,
                            "action_sha256": action_sha256,
                            "approval_id": approval_id,
                        }),
                    )?;
                    Ok(ToolExecutionGate::AwaitingApproval { approval_id })
                })();
                match result {
                    Ok(value) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(value)
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error)
                    }
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::tool_registry::pg_lock_tool_policy_authority(&mut tx)?;
                let policy = pg_tool_execution_policy_snapshot(&mut tx, profile_id, tool_name)?;
                require_policy_snapshot(&policy, policy_sha256)?;
                tx.query_one(
                    "SELECT node_id FROM workflow_run_nodes
                     WHERE run_id = $1 AND node_id = $2 FOR UPDATE",
                    &[&run_id, &node_id],
                )
                .map_err(|_| {
                    format!("workflow node not found for tool authorization: {run_id}/{node_id}")
                })?;
                let existing = tx
                    .query_opt(
                        "SELECT action_sha256, tool_name, profile_id, status, requested_approval_id
                         FROM tool_execution_authorizations
                         WHERE run_id = $1 AND node_id = $2 FOR UPDATE",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(row) = existing {
                    let bound_hash: String = row.get(0);
                    let bound_tool: String = row.get(1);
                    let bound_profile: String = row.get(2);
                    let status: String = row.get(3);
                    let approval_id: String = row.get(4);
                    if bound_hash != action_sha256
                        || bound_tool != tool_name
                        || bound_profile != profile_id
                    {
                        return Err(
                            "tool authorization binding changed for leased node".to_string()
                        );
                    }
                    if status == "approved" {
                        let now = self.now();
                        let changed = tx
                            .execute(
                                "UPDATE tool_execution_authorizations
                                 SET status = 'consumed', updated_at = $1
                                 WHERE run_id = $2 AND node_id = $3 AND status = 'approved'",
                                &[&now, &run_id, &node_id],
                            )
                            .map_err(|error| error.to_string())?;
                        if changed != 1 {
                            return Err("tool authorization was consumed concurrently".to_string());
                        }
                        pg_append_audit(
                            &mut tx,
                            &now,
                            "tool-policy",
                            "tool_execution.authorization_consumed",
                            run_id,
                            &json!({
                                "node_id": node_id,
                                "tool_name": tool_name,
                                "profile_id": profile_id,
                                "policy_sha256": policy_sha256,
                                "action_sha256": action_sha256,
                                "approval_id": approval_id,
                            }),
                        )?;
                        tx.commit().map_err(|error| error.to_string())?;
                        return Ok(ToolExecutionGate::Authorized);
                    }
                    let gate = gate_from_status(&status, approval_id)?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(gate);
                }

                tx.batch_execute("LOCK TABLE workflow_run_approvals IN SHARE ROW EXCLUSIVE MODE")
                    .map_err(|error| error.to_string())?;
                let sequence: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(approval_sequence), 0) + 1
                         FROM workflow_run_approvals",
                        &[],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let approval_id = format!("workflow-approval-{sequence:04}");
                let now = self.now();
                let approval = json!({
                    "approval_sequence": sequence,
                    "approval_id": approval_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "decision": "requested",
                    "actor": "tool-policy",
                    "reason": reason,
                    "created_at": now,
                    "approval_kind": "tool_execution",
                    "tool_name": tool_name,
                    "profile_id": profile_id,
                    "policy_sha256": policy_sha256,
                    "action_sha256": action_sha256,
                    "metadata_only": false,
                    "execution_authority": "single_tool_invocation",
                });
                tx.execute(
                    "INSERT INTO workflow_run_approvals
                     (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                      created_at, approval_json)
                     VALUES ($1, $2, $3, $4, 'requested', 'tool-policy', $5, $6, $7)",
                    &[
                        &sequence,
                        &approval_id,
                        &run_id,
                        &node_id,
                        &reason,
                        &now,
                        &approval.to_string(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.execute(
                    "INSERT INTO tool_execution_authorizations
                     (run_id, node_id, action_sha256, tool_name, profile_id, status,
                      requested_approval_id, resolved_by, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, 'requested', $6, NULL, $7, $7)",
                    &[
                        &run_id,
                        &node_id,
                        &action_sha256,
                        &tool_name,
                        &profile_id,
                        &approval_id,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    "tool-policy",
                    "tool_execution.approval_requested",
                    run_id,
                    &json!({
                        "node_id": node_id,
                        "tool_name": tool_name,
                        "profile_id": profile_id,
                        "policy_sha256": policy_sha256,
                        "action_sha256": action_sha256,
                        "approval_id": approval_id,
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ToolExecutionGate::AwaitingApproval { approval_id })
            }),
        }
    }

    pub(crate) fn claim_tool_execution_without_approval(
        &self,
        run_id: &str,
        node_id: &str,
        tool_name: &str,
        profile_id: &str,
        policy_sha256: &str,
        action_sha256: &str,
    ) -> Result<ToolExecutionGate, String> {
        validate_identifier("run_id", run_id)?;
        validate_identifier("node_id", node_id)?;
        validate_identifier("tool_name", tool_name)?;
        validate_identifier("profile_id", profile_id)?;
        validate_action_hash(policy_sha256)?;
        validate_action_hash(action_sha256)?;
        let receipt_id = format!(
            "implicit-{}",
            hex::encode(Sha256::digest(format!(
                "{run_id}\0{node_id}\0{tool_name}\0{profile_id}\0{action_sha256}"
            )))
        );
        let verify_existing = |bound_hash: String,
                               bound_tool: String,
                               bound_profile: String,
                               status: String,
                               approval_id: String|
         -> Result<ToolExecutionGate, String> {
            if bound_hash != action_sha256 || bound_tool != tool_name || bound_profile != profile_id
            {
                return Err("tool execution receipt binding changed for leased node".to_string());
            }
            if approval_id != receipt_id || status != "consumed" {
                return Err(
                    "existing tool execution requires its explicit approval workflow".to_string(),
                );
            }
            Ok(ToolExecutionGate::ConsumedOutcomeUnknown)
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    let policy =
                        sqlite_tool_execution_policy_snapshot(conn, profile_id, tool_name)?;
                    require_policy_snapshot(&policy, policy_sha256)?;
                    let node_exists: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM workflow_run_nodes
                             WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    if node_exists != 1 {
                        return Err(format!(
                            "workflow node not found for tool receipt: {run_id}/{node_id}"
                        ));
                    }
                    let existing = conn
                        .query_row(
                            "SELECT action_sha256, tool_name, profile_id, status,
                                    requested_approval_id
                             FROM tool_execution_authorizations
                             WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?,
                                    row.get::<_, String>(3)?,
                                    row.get::<_, String>(4)?,
                                ))
                            },
                        )
                        .optional()
                        .map_err(|error| error.to_string())?;
                    if let Some((hash, tool, profile, status, approval_id)) = existing {
                        return verify_existing(hash, tool, profile, status, approval_id);
                    }
                    let now = self.now();
                    conn.execute(
                        "INSERT INTO tool_execution_authorizations
                         (run_id, node_id, action_sha256, tool_name, profile_id, status,
                          requested_approval_id, resolved_by, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'consumed', ?6,
                                 'tool-policy:implicit', ?7, ?7)",
                        params![
                            run_id,
                            node_id,
                            action_sha256,
                            tool_name,
                            profile_id,
                            receipt_id,
                            now,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    append_audit_locked(
                        conn,
                        &now,
                        "tool-policy",
                        "tool_execution.implicit_receipt_claimed",
                        run_id,
                        &json!({
                            "node_id": node_id,
                            "tool_name": tool_name,
                            "profile_id": profile_id,
                            "policy_sha256": policy_sha256,
                            "action_sha256": action_sha256,
                            "receipt_id": receipt_id,
                            "effect_outcome": "unknown_until_node_finalization",
                        }),
                    )?;
                    Ok(ToolExecutionGate::Authorized)
                })();
                match result {
                    Ok(value) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(value)
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error)
                    }
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::tool_registry::pg_lock_tool_policy_authority(&mut tx)?;
                let policy = pg_tool_execution_policy_snapshot(&mut tx, profile_id, tool_name)?;
                require_policy_snapshot(&policy, policy_sha256)?;
                tx.query_one(
                    "SELECT node_id FROM workflow_run_nodes
                     WHERE run_id = $1 AND node_id = $2 FOR UPDATE",
                    &[&run_id, &node_id],
                )
                .map_err(|_| {
                    format!("workflow node not found for tool receipt: {run_id}/{node_id}")
                })?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT action_sha256, tool_name, profile_id, status,
                                requested_approval_id
                         FROM tool_execution_authorizations
                         WHERE run_id = $1 AND node_id = $2 FOR UPDATE",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?
                {
                    let gate = verify_existing(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(gate);
                }
                let now = self.now();
                tx.execute(
                    "INSERT INTO tool_execution_authorizations
                     (run_id, node_id, action_sha256, tool_name, profile_id, status,
                      requested_approval_id, resolved_by, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, 'consumed', $6,
                             'tool-policy:implicit', $7, $7)",
                    &[
                        &run_id,
                        &node_id,
                        &action_sha256,
                        &tool_name,
                        &profile_id,
                        &receipt_id,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    "tool-policy",
                    "tool_execution.implicit_receipt_claimed",
                    run_id,
                    &json!({
                        "node_id": node_id,
                        "tool_name": tool_name,
                        "profile_id": profile_id,
                        "policy_sha256": policy_sha256,
                        "action_sha256": action_sha256,
                        "receipt_id": receipt_id,
                        "effect_outcome": "unknown_until_node_finalization",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ToolExecutionGate::Authorized)
            }),
        }
    }

    pub fn inspect_tool_execution_authorization(
        &self,
        run_id: &str,
        node_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT action_sha256, tool_name, profile_id, status,
                            requested_approval_id, resolved_by, created_at, updated_at
                     FROM tool_execution_authorizations
                     WHERE run_id = ?1 AND node_id = ?2",
                    params![run_id, node_id],
                    |row| {
                        Ok(json!({
                            "run_id": run_id,
                            "node_id": node_id,
                            "action_sha256": row.get::<_, String>(0)?,
                            "tool_name": row.get::<_, String>(1)?,
                            "profile_id": row.get::<_, String>(2)?,
                            "status": row.get::<_, String>(3)?,
                            "requested_approval_id": row.get::<_, String>(4)?,
                            "resolved_by": row.get::<_, Option<String>>(5)?,
                            "created_at": row.get::<_, String>(6)?,
                            "updated_at": row.get::<_, String>(7)?,
                            "content_excluded": true,
                        }))
                    },
                )
                .optional()
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "SELECT action_sha256, tool_name, profile_id, status,
                                requested_approval_id, resolved_by, created_at, updated_at
                         FROM tool_execution_authorizations
                         WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(row.map(|row| {
                    json!({
                        "run_id": run_id,
                        "node_id": node_id,
                        "action_sha256": row.get::<_, String>(0),
                        "tool_name": row.get::<_, String>(1),
                        "profile_id": row.get::<_, String>(2),
                        "status": row.get::<_, String>(3),
                        "requested_approval_id": row.get::<_, String>(4),
                        "resolved_by": row.get::<_, Option<String>>(5),
                        "created_at": row.get::<_, String>(6),
                        "updated_at": row.get::<_, String>(7),
                        "content_excluded": true,
                    })
                }))
            }),
        }
    }

    pub fn tool_execution_approval_requires_execute_scope(
        &self,
        run_id: &str,
        approval_id: &str,
    ) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tool_execution_authorizations
                         WHERE run_id = ?1 AND requested_approval_id = ?2",
                        params![run_id, approval_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(count == 1)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let count: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM tool_execution_authorizations
                         WHERE run_id = $1 AND requested_approval_id = $2",
                        &[&run_id, &approval_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                Ok(count == 1)
            }),
        }
    }
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details.to_string()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
