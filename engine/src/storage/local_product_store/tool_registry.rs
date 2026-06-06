use rusqlite::params;
use serde_json::Value;

use super::LocalProductStore;
use crate::workflow::tool_registry::{
    HookAction, HookResult, HookType, RiskLevel, ToolCapability, ToolDescriptor, ToolHook,
};

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

    Ok(ToolCapability {
        tool_name,
        description,
        input_schema: input_schema_json.and_then(|s| serde_json::from_str(&s).ok()),
        output_schema: output_schema_json.and_then(|s| serde_json::from_str(&s).ok()),
        requires_approval: requires_approval != 0,
        risk_level: RiskLevel::parse_str(&risk_level_str).unwrap_or(RiskLevel::Low),
        created_at,
    })
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

    Ok(ToolHook {
        hook_id,
        hook_type: HookType::parse_str(&hook_type_str).unwrap_or(HookType::PreExecution),
        tool_name,
        condition: condition_json.and_then(|s| serde_json::from_str(&s).ok()),
        action: HookAction::parse_str(&action_str).unwrap_or(HookAction::Log),
        action_config: action_config_json.and_then(|s| serde_json::from_str(&s).ok()),
        enabled: enabled != 0,
        created_at,
    })
}

// ---------------------------------------------------------------------------
// Storage methods
// ---------------------------------------------------------------------------

impl LocalProductStore {
    pub fn register_tool_capability(
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
        self.with_conn(|conn| {
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
                    requires_approval as i64,
                    risk_level,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn get_tool_capability(&self, name: &str) -> Result<Option<ToolCapability>, String> {
        self.with_conn(|conn| {
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
        })
    }

    pub fn list_tool_capabilities(&self) -> Result<Vec<ToolCapability>, String> {
        self.with_conn(|conn| {
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
        })
    }

    pub fn set_tool_allowlist(
        &self,
        profile_id: &str,
        tool_names: &[String],
    ) -> Result<(), String> {
        let now = self.now();
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM tool_allowlists WHERE profile_id = ?1",
                params![profile_id],
            )
            .map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(|e| e.to_string())?;
            for tool in tool_names {
                stmt.execute(params![profile_id, tool, now])
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    pub fn check_tool_allowed(&self, profile_id: &str, tool_name: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tool_allowlists WHERE profile_id = ?1",
                    params![profile_id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            // No allowlist for this profile -> everything allowed
            if count == 0 {
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
        })
    }

    pub fn add_tool_hook(
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
        self.with_conn(|conn| {
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
        })
    }

    pub fn evaluate_hooks(
        &self,
        hook_type: &HookType,
        tool_name: &str,
        context: &Value,
    ) -> Result<HookResult, String> {
        let hooks = self.list_enabled_hooks()?;
        let type_str = hook_type.as_str();

        let mut enrichments: Vec<Value> = Vec::new();

        for hook in &hooks {
            if hook.hook_type.as_str() != type_str {
                continue;
            }
            // Match: hook.tool_name is None (all-tools) or matches the tool_name
            if let Some(ref hook_tool) = hook.tool_name {
                if hook_tool != tool_name {
                    continue;
                }
            }

            // Evaluate condition if present
            if let Some(ref condition) = hook.condition {
                if !evaluate_condition(condition, context) {
                    continue;
                }
            }

            match hook.action {
                HookAction::Block => {
                    let reason = hook
                        .action_config
                        .as_ref()
                        .and_then(|c| c.get("reason"))
                        .and_then(Value::as_str)
                        .unwrap_or("blocked by hook")
                        .to_string();
                    return Ok(HookResult::Block(reason));
                }
                HookAction::RequestApproval => {
                    let reason = hook
                        .action_config
                        .as_ref()
                        .and_then(|c| c.get("reason"))
                        .and_then(Value::as_str)
                        .unwrap_or("approval required by hook")
                        .to_string();
                    return Ok(HookResult::RequestApproval(reason));
                }
                HookAction::Enrich => {
                    if let Some(ref config) = hook.action_config {
                        if let Some(enrichment) = config.get("enrichment") {
                            enrichments.push(enrichment.clone());
                        }
                    }
                }
                HookAction::Log => {
                    // Log is a no-op in evaluation (audit trail is external)
                }
            }
        }

        if !enrichments.is_empty() {
            let mut merged = context.clone();
            for enrichment in enrichments {
                if let (Some(obj), Some(enrich_obj)) =
                    (merged.as_object_mut(), enrichment.as_object())
                {
                    for (k, v) in enrich_obj {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
            return Ok(HookResult::Enrich(merged));
        }

        Ok(HookResult::Allow)
    }

    pub fn set_hook_enabled(&self, hook_id: &str, enabled: bool) -> Result<bool, String> {
        self.with_conn(|conn| {
            let affected = conn
                .execute(
                    "UPDATE tool_hooks SET enabled = ?1 WHERE hook_id = ?2",
                    params![enabled as i64, hook_id],
                )
                .map_err(|e| e.to_string())?;
            Ok(affected > 0)
        })
    }

    pub fn delete_all_hooks(&self) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM tool_hooks", [])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    fn list_enabled_hooks(&self) -> Result<Vec<ToolHook>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT hook_id, hook_type, tool_name, condition_json, action,
                            action_config_json, enabled, created_at
                     FROM tool_hooks WHERE enabled = 1 ORDER BY rowid",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], hook_row).map_err(|e| e.to_string())?;
            let mut hooks = Vec::new();
            for row in rows {
                hooks.push(row.map_err(|e| e.to_string())?);
            }
            Ok(hooks)
        })
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
        self.register_tool_capability(
            name,
            description,
            input_schema,
            output_schema,
            requires_approval,
            risk_level,
        )?;
        Ok(true)
    }
}
