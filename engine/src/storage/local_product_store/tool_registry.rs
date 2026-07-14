use rusqlite::{params, types::Type};
use serde_json::Value;

use super::{DatabaseConnection, LocalProductStore};
use crate::workflow::tool_registry::{
    validate_tool_hook_contract, HookAction, HookEvaluation, HookResult, HookType, RiskLevel,
    ToolCapability, ToolDescriptor, ToolHook,
};

const MAX_EVALUATED_TOOL_HOOKS: usize = 32;

/// Stable PostgreSQL transaction-lock namespace for authoritative tool-policy
/// mutations and execution-receipt claims. Every production writer and claim
/// acquires this lock before reading or mutating capability, allowlist, or hook
/// state so a receipt cannot be minted from a stale policy snapshot.
#[cfg(feature = "pg")]
pub(super) const TOOL_POLICY_AUTHORITY_LOCK_KEY: i64 = 0x4150_435f_5450_4f4c;

#[cfg(feature = "pg")]
pub(super) fn pg_lock_tool_policy_authority(
    client: &mut impl postgres::GenericClient,
) -> Result<(), String> {
    client
        .execute(
            "SELECT pg_advisory_xact_lock($1)",
            &[&TOOL_POLICY_AUTHORITY_LOCK_KEY],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

fn capability_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCapability> {
    let tool_name: String = row.get(0)?;
    let description: String = row.get(1)?;
    let input_schema_json: Option<String> = row.get(2)?;
    let output_schema_json: Option<String> = row.get(3)?;
    let requires_approval: i64 = row.get(4)?;
    let risk_level_str: String = row.get(5)?;
    let created_at: String = row.get(6)?;

    let input_schema = parse_optional_capability_json(input_schema_json, 2)?;
    let output_schema = parse_optional_capability_json(output_schema_json, 3)?;
    let risk_level = RiskLevel::parse_str(&risk_level_str).ok_or_else(|| {
        invalid_capability_row(5, format!("invalid risk_level: {risk_level_str}"))
    })?;

    Ok(ToolCapability {
        tool_name,
        description,
        input_schema,
        output_schema,
        requires_approval: requires_approval != 0,
        risk_level,
        created_at,
    })
}

fn invalid_capability_row(index: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn parse_optional_capability_json(
    raw: Option<String>,
    index: usize,
) -> rusqlite::Result<Option<Value>> {
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|error| invalid_capability_row(index, error.to_string()))
    })
    .transpose()
}

fn hook_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolHook> {
    let hook_id: String = row.get(0)?;
    let hook_type_str: String = row.get(1)?;
    let tool_name: Option<String> = row.get(2)?;
    let condition_json: Option<String> = row.get(3)?;
    let action_str: String = row.get(4)?;
    let action_config_json: Option<String> = row.get(5)?;
    let enabled: i64 = row.get(6)?;
    let created_at: String = row.get(7)?;

    let hook_type = HookType::parse_str(&hook_type_str)
        .ok_or_else(|| invalid_hook_row(1, format!("invalid hook_type: {hook_type_str}")))?;
    let action = HookAction::parse_str(&action_str)
        .ok_or_else(|| invalid_hook_row(4, format!("invalid hook action: {action_str}")))?;
    let condition = parse_optional_hook_json(condition_json, 3)?;
    let action_config = parse_optional_hook_json(action_config_json, 5)?;
    validate_tool_hook_contract(
        &hook_type_str,
        condition.as_ref(),
        &action_str,
        action_config.as_ref(),
    )
    .map_err(|error| invalid_hook_row(4, error))?;

    Ok(ToolHook {
        hook_id,
        hook_type,
        tool_name,
        condition,
        action,
        action_config,
        enabled: enabled != 0,
        created_at,
    })
}

fn invalid_hook_row(index: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn parse_optional_hook_json(raw: Option<String>, index: usize) -> rusqlite::Result<Option<Value>> {
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|error| invalid_hook_row(index, error.to_string()))
    })
    .transpose()
}

// ---------------------------------------------------------------------------
// Storage methods
// ---------------------------------------------------------------------------

impl LocalProductStore {
    #[cfg(test)]
    pub(crate) fn register_tool_capability(
        &self,
        name: &str,
        description: &str,
        input_schema: Option<&Value>,
        output_schema: Option<&Value>,
        requires_approval: bool,
        risk_level: &str,
    ) -> Result<(), String> {
        let input_json = input_schema.map(|v| v.to_string());
        let output_json = output_schema.map(|v| v.to_string());
        let now = self.now();
        let approval_i64 = requires_approval as i64;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO tool_capabilities
                     (tool_name, description, input_schema_json, output_schema_json,
                      requires_approval, risk_level, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(tool_name) DO UPDATE SET
                      description = excluded.description,
                      input_schema_json = excluded.input_schema_json,
                      output_schema_json = excluded.output_schema_json,
                      requires_approval = excluded.requires_approval,
                      risk_level = excluded.risk_level",
                    params![
                        name,
                        description,
                        input_json,
                        output_json,
                        approval_i64,
                        risk_level,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let approval_i32 = i32::from(requires_approval);
                client
                    .execute(
                        "INSERT INTO tool_capabilities
                         (tool_name, description, input_schema_json, output_schema_json,
                          requires_approval, risk_level, created_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7)
                         ON CONFLICT(tool_name) DO UPDATE SET
                          description = EXCLUDED.description,
                          input_schema_json = EXCLUDED.input_schema_json,
                          output_schema_json = EXCLUDED.output_schema_json,
                          requires_approval = EXCLUDED.requires_approval,
                          risk_level = EXCLUDED.risk_level",
                        &[
                            &name,
                            &description,
                            &input_json,
                            &output_json,
                            &approval_i32,
                            &risk_level,
                            &now,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn get_tool_capability(&self, name: &str) -> Result<Option<ToolCapability>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT tool_name, description, input_schema_json, output_schema_json,
                                requires_approval, risk_level, created_at
                         FROM tool_capabilities WHERE tool_name = ?1 LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![name], capability_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT tool_name, description, input_schema_json, output_schema_json,
                                requires_approval, risk_level, created_at
                         FROM tool_capabilities WHERE tool_name = $1 LIMIT 1",
                        &[&name],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_capability_row(&row)?)),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn list_tool_capabilities(&self) -> Result<Vec<ToolCapability>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT tool_name, description, input_schema_json, output_schema_json,
                                requires_approval, risk_level, created_at
                         FROM tool_capabilities ORDER BY tool_name",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], capability_row)
                    .map_err(|e| e.to_string())?;
                let mut caps = Vec::new();
                for row in rows {
                    caps.push(row.map_err(|e| e.to_string())?);
                }
                Ok(caps)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT tool_name, description, input_schema_json, output_schema_json,
                                requires_approval, risk_level, created_at
                         FROM tool_capabilities ORDER BY tool_name",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                rows.iter().map(pg_capability_row).collect()
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn set_tool_allowlist(
        &self,
        profile_id: &str,
        tool_names: &[String],
    ) -> Result<(), String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                         VALUES (?1, ?2)
                         ON CONFLICT(profile_id) DO UPDATE SET
                          configured_at = excluded.configured_at",
                        params![profile_id, now],
                    )
                    .map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "DELETE FROM tool_allowlists WHERE profile_id = ?1",
                        params![profile_id],
                    )
                    .map_err(|e| e.to_string())?;
                let mut stmt = transaction
                    .prepare(
                        "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                         VALUES (?1, ?2, ?3)",
                    )
                    .map_err(|e| e.to_string())?;
                for tool in tool_names {
                    stmt.execute(params![profile_id, tool, now])
                        .map_err(|e| e.to_string())?;
                }
                drop(stmt);
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                         VALUES ($1, $2)
                         ON CONFLICT(profile_id) DO UPDATE SET
                          configured_at = EXCLUDED.configured_at",
                        &[&profile_id, &now],
                    )
                    .map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "DELETE FROM tool_allowlists WHERE profile_id = $1",
                        &[&profile_id],
                    )
                    .map_err(|e| e.to_string())?;
                for tool in tool_names {
                    transaction
                        .execute(
                            "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                             VALUES ($1, $2, $3)",
                            &[&profile_id, tool, &now],
                        )
                        .map_err(|e| e.to_string())?;
                }
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn check_tool_allowed(&self, profile_id: &str, tool_name: &str) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let configured: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tool_allowlist_profiles WHERE profile_id = ?1",
                        params![profile_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;

                // An absent profile preserves the legacy unconfigured behavior. Once a
                // profile is configured, its entries are authoritative; an explicit
                // empty allowlist therefore denies every tool.
                if configured == 0 {
                    return Ok(true);
                }

                let found: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tool_allowlists
                         WHERE profile_id = ?1 AND tool_name = ?2",
                        params![profile_id, tool_name],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;

                Ok(found > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let configured: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM tool_allowlist_profiles WHERE profile_id = $1",
                        &[&profile_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);

                if configured == 0 {
                    return Ok(true);
                }

                let found: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM tool_allowlists
                         WHERE profile_id = $1 AND tool_name = $2",
                        &[&profile_id, &tool_name],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);

                Ok(found > 0)
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn add_tool_hook(
        &self,
        hook_id: &str,
        hook_type: &str,
        tool_name: Option<&str>,
        condition: Option<&Value>,
        action: &str,
        action_config: Option<&Value>,
    ) -> Result<(), String> {
        let condition_json = condition.map(|v| v.to_string());
        let config_json = action_config.map(|v| v.to_string());
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO tool_hooks
                     (hook_id, hook_type, tool_name, condition_json, action,
                      action_config_json, enabled, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)
                     ON CONFLICT(hook_id) DO UPDATE SET
                      hook_type = excluded.hook_type,
                      tool_name = excluded.tool_name,
                      condition_json = excluded.condition_json,
                      action = excluded.action,
                      action_config_json = excluded.action_config_json",
                    params![
                        hook_id,
                        hook_type,
                        tool_name,
                        condition_json,
                        action,
                        config_json,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO tool_hooks
                         (hook_id, hook_type, tool_name, condition_json, action,
                          action_config_json, enabled, created_at)
                         VALUES ($1, $2, $3, $4, $5, $6, 1, $7)
                         ON CONFLICT(hook_id) DO UPDATE SET
                          hook_type = EXCLUDED.hook_type,
                          tool_name = EXCLUDED.tool_name,
                          condition_json = EXCLUDED.condition_json,
                          action = EXCLUDED.action,
                          action_config_json = EXCLUDED.action_config_json",
                        &[
                            &hook_id,
                            &hook_type,
                            &tool_name,
                            &condition_json,
                            &action,
                            &config_json,
                            &now,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn evaluate_hooks(
        &self,
        hook_type: &HookType,
        tool_name: &str,
        context: &Value,
    ) -> Result<HookResult, String> {
        Ok(self
            .evaluate_hooks_with_provenance(hook_type, tool_name, context)?
            .result)
    }

    pub fn evaluate_hooks_with_provenance(
        &self,
        hook_type: &HookType,
        tool_name: &str,
        context: &Value,
    ) -> Result<HookEvaluation, String> {
        let hooks = self.list_enabled_hooks()?;
        evaluate_tool_hooks(&hooks, hook_type, tool_name, context)
    }

    #[cfg(test)]
    pub(crate) fn set_hook_enabled(&self, hook_id: &str, enabled: bool) -> Result<bool, String> {
        let enabled_i64 = enabled as i64;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let affected = conn
                    .execute(
                        "UPDATE tool_hooks SET enabled = ?1 WHERE hook_id = ?2",
                        params![enabled_i64, hook_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(affected > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let enabled_i32 = i32::from(enabled);
                let affected = client
                    .execute(
                        "UPDATE tool_hooks SET enabled = $1 WHERE hook_id = $2",
                        &[&enabled_i32, &hook_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(affected > 0)
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn delete_all_hooks(&self) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute("DELETE FROM tool_hooks", [])
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute("DELETE FROM tool_hooks", &[])
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    fn list_enabled_hooks(&self) -> Result<Vec<ToolHook>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT hook_id, hook_type, tool_name, condition_json, action,
                                action_config_json, enabled, created_at
                         FROM tool_hooks WHERE enabled = 1 ORDER BY hook_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt.query_map([], hook_row).map_err(|e| e.to_string())?;
                let mut hooks = Vec::new();
                for row in rows {
                    hooks.push(row.map_err(|e| e.to_string())?);
                }
                Ok(hooks)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT hook_id, hook_type, tool_name, condition_json, action,
                                action_config_json, enabled, created_at
                         FROM tool_hooks WHERE enabled = 1 ORDER BY hook_id",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                rows.iter().map(pg_hook_row).collect()
            }),
        }
    }

    pub fn get_mcp_descriptors(&self) -> Result<Vec<ToolDescriptor>, String> {
        let capabilities = self.list_tool_capabilities()?;
        Ok(capabilities
            .into_iter()
            .map(|cap| {
                let annotations = if cap.requires_approval || cap.risk_level != RiskLevel::Low {
                    Some(serde_json::json!({
                        "requires_approval": cap.requires_approval,
                        "risk_level": cap.risk_level.as_str(),
                    }))
                } else {
                    None
                };
                ToolDescriptor {
                    name: cap.tool_name,
                    description: cap.description,
                    input_schema: cap.input_schema,
                    annotations,
                }
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Condition evaluation — simple JSON path equality check
// ---------------------------------------------------------------------------

pub(super) fn evaluate_tool_hooks(
    hooks: &[ToolHook],
    hook_type: &HookType,
    tool_name: &str,
    context: &Value,
) -> Result<HookEvaluation, String> {
    if hooks.len() > MAX_EVALUATED_TOOL_HOOKS {
        return Err(format!(
            "enabled tool hook count exceeds bounded maximum {MAX_EVALUATED_TOOL_HOOKS}"
        ));
    }
    let type_str = hook_type.as_str();
    let mut enrichments: Vec<Value> = Vec::new();
    let mut matched_hook_ids: Vec<String> = Vec::new();
    let mut block_reason: Option<String> = None;
    let mut approval_reason: Option<String> = None;

    for hook in hooks {
        if !hook.enabled || hook.hook_type.as_str() != type_str {
            continue;
        }
        if hook
            .tool_name
            .as_ref()
            .is_some_and(|hook_tool| hook_tool != tool_name)
        {
            continue;
        }
        if hook
            .condition
            .as_ref()
            .is_some_and(|condition| !evaluate_condition(condition, context))
        {
            continue;
        }
        matched_hook_ids.push(hook.hook_id.clone());

        match hook.action {
            HookAction::Block => {
                if block_reason.is_none() {
                    block_reason = Some(
                        hook.action_config
                            .as_ref()
                            .and_then(|config| config.get("reason"))
                            .and_then(Value::as_str)
                            .unwrap_or("blocked by hook")
                            .to_string(),
                    );
                }
            }
            HookAction::RequestApproval => {
                if approval_reason.is_none() {
                    approval_reason = Some(
                        hook.action_config
                            .as_ref()
                            .and_then(|config| config.get("reason"))
                            .and_then(Value::as_str)
                            .unwrap_or("approval required by hook")
                            .to_string(),
                    );
                }
            }
            HookAction::Enrich => {
                if let Some(enrichment) = hook
                    .action_config
                    .as_ref()
                    .and_then(|config| config.get("enrichment"))
                {
                    enrichments.push(enrichment.clone());
                }
            }
            HookAction::Log => {}
        }
    }

    if let Some(reason) = block_reason {
        return Ok(HookEvaluation {
            result: HookResult::Block(reason),
            matched_hook_ids,
        });
    }
    if let Some(reason) = approval_reason {
        return Ok(HookEvaluation {
            result: HookResult::RequestApproval(reason),
            matched_hook_ids,
        });
    }
    if !enrichments.is_empty() {
        let mut merged = context.clone();
        for enrichment in enrichments {
            if let (Some(object), Some(enrichment)) =
                (merged.as_object_mut(), enrichment.as_object())
            {
                for (key, value) in enrichment {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        return Ok(HookEvaluation {
            result: HookResult::Enrich(merged),
            matched_hook_ids,
        });
    }

    Ok(HookEvaluation {
        result: HookResult::Allow,
        matched_hook_ids,
    })
}

fn evaluate_condition(condition: &Value, context: &Value) -> bool {
    if let Some(path) = condition.get("path").and_then(Value::as_str) {
        if let Some(expected) = condition.get("equals") {
            let actual = resolve_json_path(context, path);
            return actual == Some(expected);
        }
    }
    // Unrecognized condition format -> treat as non-match (safe default)
    false
}

fn resolve_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

#[cfg(feature = "pg")]
fn pg_capability_row(row: &postgres::Row) -> Result<ToolCapability, String> {
    let tool_name: String = row.get(0);
    let description: String = row.get(1);
    let input_schema_json: Option<String> = row.get(2);
    let output_schema_json: Option<String> = row.get(3);
    let requires_approval: i32 = row.get(4);
    let risk_level_str: String = row.get(5);
    let created_at: String = row.get(6);

    let input_schema = input_schema_json
        .map(|raw| {
            serde_json::from_str(&raw)
                .map_err(|error| format!("invalid stored input_schema JSON: {error}"))
        })
        .transpose()?;
    let output_schema = output_schema_json
        .map(|raw| {
            serde_json::from_str(&raw)
                .map_err(|error| format!("invalid stored output_schema JSON: {error}"))
        })
        .transpose()?;
    let risk_level = RiskLevel::parse_str(&risk_level_str)
        .ok_or_else(|| format!("invalid stored risk_level: {risk_level_str}"))?;

    Ok(ToolCapability {
        tool_name,
        description,
        input_schema,
        output_schema,
        requires_approval: requires_approval != 0,
        risk_level,
        created_at,
    })
}

#[cfg(feature = "pg")]
fn pg_hook_row(row: &postgres::Row) -> Result<ToolHook, String> {
    let hook_id: String = row.get(0);
    let hook_type_str: String = row.get(1);
    let tool_name: Option<String> = row.get(2);
    let condition_json: Option<String> = row.get(3);
    let action_str: String = row.get(4);
    let action_config_json: Option<String> = row.get(5);
    let enabled: i32 = row.get(6);
    let created_at: String = row.get(7);

    let hook_type = HookType::parse_str(&hook_type_str)
        .ok_or_else(|| format!("invalid stored hook_type: {hook_type_str}"))?;
    let action = HookAction::parse_str(&action_str)
        .ok_or_else(|| format!("invalid stored hook action: {action_str}"))?;
    let condition = condition_json
        .map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
        .transpose()?;
    let action_config = action_config_json
        .map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
        .transpose()?;
    validate_tool_hook_contract(
        &hook_type_str,
        condition.as_ref(),
        &action_str,
        action_config.as_ref(),
    )?;
    Ok(ToolHook {
        hook_id,
        hook_type,
        tool_name,
        condition,
        action,
        action_config,
        enabled: enabled != 0,
        created_at,
    })
}

// ---------------------------------------------------------------------------
// Export support
// ---------------------------------------------------------------------------

impl LocalProductStore {
    pub fn export_tool_capabilities(&self) -> Result<Vec<Value>, String> {
        let caps = self.list_tool_capabilities()?;
        Ok(caps
            .into_iter()
            .map(|cap| {
                serde_json::json!({
                    "tool_name": cap.tool_name,
                    "description": cap.description,
                    "input_schema": cap.input_schema,
                    "output_schema": cap.output_schema,
                    "requires_approval": cap.requires_approval,
                    "risk_level": cap.risk_level.as_str(),
                })
            })
            .collect())
    }

    pub fn import_tool_capability_entry(&self, entry: &Value) -> Result<bool, String> {
        let name = entry.get("tool_name").and_then(Value::as_str).unwrap_or("");
        if name.is_empty() {
            return Ok(false);
        }
        let description = entry
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        let input_schema = entry.get("input_schema").filter(|v| !v.is_null());
        let output_schema = entry.get("output_schema").filter(|v| !v.is_null());
        let requires_approval = entry
            .get("requires_approval")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let risk_level = entry
            .get("risk_level")
            .and_then(Value::as_str)
            .unwrap_or("low");
        let current = self.read_tool_capability_policy(name)?;
        let expected_current_sha256 = current
            .as_ref()
            .and_then(|value| value.get("resource_sha256"))
            .and_then(Value::as_str);
        self.configure_tool_capability(
            "tool-capability-importer",
            name,
            description,
            input_schema,
            output_schema,
            requires_approval,
            risk_level,
            expected_current_sha256,
        )?;
        Ok(true)
    }
}

#[cfg(test)]
mod strict_hook_row_tests {
    use super::*;

    #[test]
    fn corrupt_enabled_hook_row_is_rejected_instead_of_downgraded() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE hooks (
                    hook_id TEXT, hook_type TEXT, tool_name TEXT, condition_json TEXT,
                    action TEXT, action_config_json TEXT, enabled INTEGER, created_at TEXT
                );
                INSERT INTO hooks VALUES (
                    'corrupt', 'pre_execution', 'echo', '{not-json',
                    'block', NULL, 1, 'now'
                );",
            )
            .unwrap();
        let error = connection
            .query_row(
                "SELECT hook_id, hook_type, tool_name, condition_json, action,
                        action_config_json, enabled, created_at FROM hooks",
                [],
                hook_row,
            )
            .expect_err("corrupt hook must fail closed");

        assert!(matches!(
            error,
            rusqlite::Error::FromSqlConversionFailure(..)
        ));
    }

    fn corrupt_capability_error(input_schema: &str, output_schema: &str, risk_level: &str) {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE capabilities (
                    tool_name TEXT, description TEXT, input_schema_json TEXT,
                    output_schema_json TEXT, requires_approval INTEGER,
                    risk_level TEXT, created_at TEXT
                );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO capabilities VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
                rusqlite::params![
                    "corrupt",
                    "fixture",
                    input_schema,
                    output_schema,
                    risk_level,
                    "now"
                ],
            )
            .unwrap();
        let error = connection
            .query_row(
                "SELECT tool_name, description, input_schema_json, output_schema_json,
                        requires_approval, risk_level, created_at FROM capabilities",
                [],
                capability_row,
            )
            .expect_err("corrupt capability must fail closed");

        assert!(matches!(
            error,
            rusqlite::Error::FromSqlConversionFailure(..)
        ));
    }

    #[test]
    fn corrupt_capability_input_schema_is_rejected() {
        corrupt_capability_error("{not-json", "{}", "low");
    }

    #[test]
    fn corrupt_capability_output_schema_is_rejected() {
        corrupt_capability_error("{}", "{not-json", "low");
    }

    #[test]
    fn unknown_capability_risk_is_rejected() {
        corrupt_capability_error("{}", "{}", "unknown");
    }
}
