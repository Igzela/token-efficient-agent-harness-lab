use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::provider::redaction::contains_sensitive_patterns;
use crate::workflow::tool_registry::validate_tool_hook_contract;

const MAX_POLICY_IDENTIFIER_BYTES: usize = 256;
const MAX_TOOL_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_POLICY_JSON_BYTES: usize = 16 * 1024;
const MAX_TOOL_NAMES: usize = 128;
const MAX_ENABLED_HOOKS: i64 = 32;

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_POLICY_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn validate_policy_value(resource_id: &str, value: &Value) -> Result<(), String> {
    if !bounded_identifier(resource_id) {
        return Err("invalid tool policy identifier".to_string());
    }
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_POLICY_JSON_BYTES {
        return Err("tool policy resource exceeds bounded JSON size".to_string());
    }
    if contains_sensitive_patterns(&String::from_utf8_lossy(&bytes)) {
        return Err("tool policy resource contains secret-shaped content".to_string());
    }
    Ok(())
}

fn value_sha256(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn mutation_result(
    kind: &str,
    resource_id: &str,
    value: Value,
    changed: bool,
) -> Result<Value, String> {
    validate_policy_value(resource_id, &value)?;
    Ok(json!({
        "schema_version": "tool_policy_resource.v1",
        "resource_kind": kind,
        "resource_id": resource_id,
        "resource_sha256": value_sha256(&value)?,
        "changed": changed,
        "value": value,
    }))
}

fn require_fresh_or_retry(
    current: Option<&Value>,
    desired: &Value,
    expected_current_sha256: Option<&str>,
) -> Result<bool, String> {
    if let Some(expected) = expected_current_sha256 {
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("expected_current_sha256 is not a hexadecimal SHA-256".to_string());
        }
    }
    if let Some(value) = current {
        let resource_id = value
            .get("tool_name")
            .or_else(|| value.get("profile_id"))
            .or_else(|| value.get("hook_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| "tool policy resource identity is missing".to_string())?;
        validate_policy_value(resource_id, value)?;
    }
    if current.is_some_and(|value| value == desired) {
        return Ok(false);
    }
    match current {
        None if expected_current_sha256.is_none() => Ok(true),
        None => Err("tool policy resource does not exist at expected hash".to_string()),
        Some(_) if expected_current_sha256.is_none() => {
            Err("expected_current_sha256 is required when replacing tool policy".to_string())
        }
        Some(value) => {
            let current_sha256 = value_sha256(value)?;
            if expected_current_sha256 == Some(current_sha256.as_str()) {
                Ok(true)
            } else {
                Err("tool policy resource changed concurrently".to_string())
            }
        }
    }
}

fn capability_value(
    tool_name: &str,
    description: &str,
    input_schema: Option<&Value>,
    output_schema: Option<&Value>,
    requires_approval: bool,
    risk_level: &str,
) -> Value {
    json!({
        "tool_name": tool_name,
        "description": description,
        "input_schema": input_schema,
        "output_schema": output_schema,
        "requires_approval": requires_approval,
        "risk_level": risk_level,
    })
}

fn parse_optional_policy_json(raw: Option<String>, field: &str) -> Result<Option<Value>, String> {
    raw.map(|raw| {
        serde_json::from_str(&raw)
            .map_err(|error| format!("invalid stored tool policy {field} JSON: {error}"))
    })
    .transpose()
}

fn sqlite_current_capability(
    tx: &Transaction<'_>,
    tool_name: &str,
) -> Result<Option<Value>, String> {
    let row = tx
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
        .map_err(|error| error.to_string())?;
    let Some((description, input, output, requires_approval, risk_level)) = row else {
        return Ok(None);
    };
    let input = parse_optional_policy_json(input, "input_schema")?;
    let output = parse_optional_policy_json(output, "output_schema")?;
    Ok(Some(capability_value(
        tool_name,
        &description,
        input.as_ref(),
        output.as_ref(),
        requires_approval != 0,
        &risk_level,
    )))
}

fn allowlist_value(profile_id: &str, tool_names: &[String]) -> Value {
    json!({"profile_id": profile_id, "tool_names": tool_names})
}

fn sqlite_current_allowlist(
    tx: &Transaction<'_>,
    profile_id: &str,
) -> Result<Option<Value>, String> {
    let configured = tx
        .query_row(
            "SELECT 1 FROM tool_allowlist_profiles WHERE profile_id = ?1",
            params![profile_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !configured {
        return Ok(None);
    }
    let mut statement = tx
        .prepare(
            "SELECT tool_name FROM tool_allowlists
             WHERE profile_id = ?1 ORDER BY tool_name",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![profile_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut tool_names = Vec::new();
    for row in rows {
        tool_names.push(row.map_err(|error| error.to_string())?);
    }
    Ok(Some(allowlist_value(profile_id, &tool_names)))
}

fn hook_value(
    hook_id: &str,
    hook_type: &str,
    tool_name: Option<&str>,
    condition: Option<&Value>,
    action: &str,
    action_config: Option<&Value>,
    enabled: bool,
) -> Value {
    json!({
        "hook_id": hook_id,
        "hook_type": hook_type,
        "tool_name": tool_name,
        "condition": condition,
        "action": action,
        "action_config": action_config,
        "enabled": enabled,
    })
}

fn sqlite_current_hook(tx: &Transaction<'_>, hook_id: &str) -> Result<Option<Value>, String> {
    let row = tx
        .query_row(
            "SELECT hook_type, tool_name, condition_json, action, action_config_json, enabled
         FROM tool_hooks WHERE hook_id = ?1",
            params![hook_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((hook_type, tool_name, condition, action, config, enabled)) = row else {
        return Ok(None);
    };
    let condition = parse_optional_policy_json(condition, "condition")?;
    let config = parse_optional_policy_json(config, "action_config")?;
    Ok(Some(hook_value(
        hook_id,
        &hook_type,
        tool_name.as_deref(),
        condition.as_ref(),
        &action,
        config.as_ref(),
        enabled != 0,
    )))
}

impl LocalProductStore {
    #[allow(clippy::too_many_arguments)]
    pub fn configure_tool_capability(
        &self,
        actor: &str,
        tool_name: &str,
        description: &str,
        input_schema: Option<&Value>,
        output_schema: Option<&Value>,
        requires_approval: bool,
        risk_level: &str,
        expected_current_sha256: Option<&str>,
    ) -> Result<Value, String> {
        let desired = capability_value(
            tool_name,
            description,
            input_schema,
            output_schema,
            requires_approval,
            risk_level,
        );
        if description.trim().is_empty()
            || description.len() > MAX_TOOL_DESCRIPTION_BYTES
            || !matches!(risk_level, "low" | "medium" | "high")
        {
            return Err("invalid tool capability policy".to_string());
        }
        validate_policy_value(tool_name, &desired)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let previous = sqlite_current_capability(&tx, tool_name)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("capability", tool_name, desired, false);
                }
                tx.execute(
                    "INSERT INTO tool_capabilities
                     (tool_name, description, input_schema_json, output_schema_json,
                      requires_approval, risk_level, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(tool_name) DO UPDATE SET
                      description=excluded.description,
                      input_schema_json=excluded.input_schema_json,
                      output_schema_json=excluded.output_schema_json,
                      requires_approval=excluded.requires_approval,
                      risk_level=excluded.risk_level",
                    params![
                        tool_name,
                        description,
                        input_schema.map(Value::to_string),
                        output_schema.map(Value::to_string),
                        requires_approval as i64,
                        risk_level,
                        now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "tool_policy.capability_configured",
                    tool_name,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("capability", tool_name, desired, true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::tool_registry::pg_lock_tool_policy_authority(&mut tx)?;
                tx.batch_execute("LOCK TABLE tool_capabilities IN SHARE ROW EXCLUSIVE MODE")
                    .map_err(|error| error.to_string())?;
                let previous = pg_current_capability(&mut tx, tool_name)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("capability", tool_name, desired, false);
                }
                let input = input_schema.map(Value::to_string);
                let output = output_schema.map(Value::to_string);
                let approval = i32::from(requires_approval);
                tx.execute(
                    "INSERT INTO tool_capabilities
                     (tool_name, description, input_schema_json, output_schema_json,
                      requires_approval, risk_level, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     ON CONFLICT(tool_name) DO UPDATE SET
                      description=EXCLUDED.description,
                      input_schema_json=EXCLUDED.input_schema_json,
                      output_schema_json=EXCLUDED.output_schema_json,
                      requires_approval=EXCLUDED.requires_approval,
                      risk_level=EXCLUDED.risk_level",
                    &[
                        &tool_name,
                        &description,
                        &input,
                        &output,
                        &approval,
                        &risk_level,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "tool_policy.capability_configured",
                    tool_name,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("capability", tool_name, desired, true)
            }),
        }
    }

    pub fn configure_tool_allowlist(
        &self,
        actor: &str,
        profile_id: &str,
        tool_names: &[String],
        expected_current_sha256: Option<&str>,
    ) -> Result<Value, String> {
        if tool_names.len() > MAX_TOOL_NAMES
            || tool_names
                .iter()
                .any(|tool_name| !bounded_identifier(tool_name))
            || tool_names.iter().collect::<HashSet<_>>().len() != tool_names.len()
        {
            return Err("invalid tool allowlist policy".to_string());
        }
        let mut sorted = tool_names.to_vec();
        sorted.sort();
        sorted.dedup();
        let desired = allowlist_value(profile_id, &sorted);
        validate_policy_value(profile_id, &desired)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let previous = sqlite_current_allowlist(&tx, profile_id)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("allowlist", profile_id, desired, false);
                }
                require_sqlite_capabilities(&tx, &sorted)?;
                tx.execute(
                    "INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                     VALUES (?1, ?2)
                     ON CONFLICT(profile_id) DO UPDATE SET configured_at=excluded.configured_at",
                    params![profile_id, now],
                )
                .map_err(|error| error.to_string())?;
                tx.execute(
                    "DELETE FROM tool_allowlists WHERE profile_id=?1",
                    params![profile_id],
                )
                .map_err(|error| error.to_string())?;
                for tool_name in &sorted {
                    tx.execute(
                        "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                         VALUES (?1, ?2, ?3)",
                        params![profile_id, tool_name, now],
                    )
                    .map_err(|error| error.to_string())?;
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "tool_policy.allowlist_configured",
                    profile_id,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("allowlist", profile_id, desired, true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::tool_registry::pg_lock_tool_policy_authority(&mut tx)?;
                tx.batch_execute("LOCK TABLE tool_allowlist_profiles IN SHARE ROW EXCLUSIVE MODE")
                    .map_err(|error| error.to_string())?;
                let previous = pg_current_allowlist(&mut tx, profile_id)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("allowlist", profile_id, desired, false);
                }
                require_pg_capabilities(&mut tx, &sorted)?;
                tx.execute(
                    "INSERT INTO tool_allowlist_profiles (profile_id, configured_at)
                     VALUES ($1, $2)
                     ON CONFLICT(profile_id) DO UPDATE SET configured_at=EXCLUDED.configured_at",
                    &[&profile_id, &now],
                )
                .map_err(|error| error.to_string())?;
                tx.execute(
                    "DELETE FROM tool_allowlists WHERE profile_id=$1",
                    &[&profile_id],
                )
                .map_err(|error| error.to_string())?;
                for tool_name in &sorted {
                    tx.execute(
                        "INSERT INTO tool_allowlists (profile_id, tool_name, created_at)
                         VALUES ($1, $2, $3)",
                        &[&profile_id, tool_name, &now],
                    )
                    .map_err(|error| error.to_string())?;
                }
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "tool_policy.allowlist_configured",
                    profile_id,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("allowlist", profile_id, desired, true)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn configure_tool_hook(
        &self,
        actor: &str,
        hook_id: &str,
        hook_type: &str,
        tool_name: Option<&str>,
        condition: Option<&Value>,
        action: &str,
        action_config: Option<&Value>,
        enabled: bool,
        expected_current_sha256: Option<&str>,
    ) -> Result<Value, String> {
        let desired = hook_value(
            hook_id,
            hook_type,
            tool_name,
            condition,
            action,
            action_config,
            enabled,
        );
        if tool_name.is_some_and(|tool_name| !bounded_identifier(tool_name)) {
            return Err("invalid tool hook policy".to_string());
        }
        validate_tool_hook_contract(hook_type, condition, action, action_config)?;
        validate_policy_value(hook_id, &desired)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let previous = sqlite_current_hook(&tx, hook_id)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("hook", hook_id, desired, false);
                }
                if let Some(tool_name) = tool_name {
                    require_sqlite_capabilities(&tx, &[tool_name.to_string()])?;
                }
                tx.execute(
                    "INSERT INTO tool_hooks
                     (hook_id, hook_type, tool_name, condition_json, action,
                      action_config_json, enabled, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(hook_id) DO UPDATE SET
                      hook_type=excluded.hook_type, tool_name=excluded.tool_name,
                      condition_json=excluded.condition_json, action=excluded.action,
                      action_config_json=excluded.action_config_json, enabled=excluded.enabled",
                    params![
                        hook_id,
                        hook_type,
                        tool_name,
                        condition.map(Value::to_string),
                        action,
                        action_config.map(Value::to_string),
                        enabled as i64,
                        now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                let enabled_count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM tool_hooks WHERE enabled=1",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                if enabled_count > MAX_ENABLED_HOOKS {
                    return Err(format!(
                        "enabled tool hook count exceeds bounded maximum {MAX_ENABLED_HOOKS}"
                    ));
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "tool_policy.hook_configured",
                    hook_id,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("hook", hook_id, desired, true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::tool_registry::pg_lock_tool_policy_authority(&mut tx)?;
                tx.batch_execute("LOCK TABLE tool_hooks IN SHARE ROW EXCLUSIVE MODE")
                    .map_err(|error| error.to_string())?;
                let previous = pg_current_hook(&mut tx, hook_id)?;
                if !require_fresh_or_retry(previous.as_ref(), &desired, expected_current_sha256)? {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return mutation_result("hook", hook_id, desired, false);
                }
                if let Some(tool_name) = tool_name {
                    require_pg_capabilities(&mut tx, &[tool_name.to_string()])?;
                }
                let condition_json = condition.map(Value::to_string);
                let action_config_json = action_config.map(Value::to_string);
                let enabled_i32 = i32::from(enabled);
                tx.execute(
                    "INSERT INTO tool_hooks
                     (hook_id, hook_type, tool_name, condition_json, action,
                      action_config_json, enabled, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                     ON CONFLICT(hook_id) DO UPDATE SET
                      hook_type=EXCLUDED.hook_type, tool_name=EXCLUDED.tool_name,
                      condition_json=EXCLUDED.condition_json, action=EXCLUDED.action,
                      action_config_json=EXCLUDED.action_config_json, enabled=EXCLUDED.enabled",
                    &[
                        &hook_id,
                        &hook_type,
                        &tool_name,
                        &condition_json,
                        &action,
                        &action_config_json,
                        &enabled_i32,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                let enabled_count: i64 = tx
                    .query_one("SELECT COUNT(*) FROM tool_hooks WHERE enabled=1", &[])
                    .map_err(|error| error.to_string())?
                    .get(0);
                if enabled_count > MAX_ENABLED_HOOKS {
                    return Err(format!(
                        "enabled tool hook count exceeds bounded maximum {MAX_ENABLED_HOOKS}"
                    ));
                }
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "tool_policy.hook_configured",
                    hook_id,
                    &json!({"previous": previous, "current": desired}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                mutation_result("hook", hook_id, desired, true)
            }),
        }
    }

    pub fn read_tool_capability_policy(&self, tool_name: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = connection
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let current = sqlite_current_capability(&tx, tool_name)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("capability", tool_name, value, false))
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let current = pg_current_capability(&mut tx, tool_name)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("capability", tool_name, value, false))
                    .transpose()
            }),
        }
    }

    pub fn read_tool_allowlist_policy(&self, profile_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = connection
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let current = sqlite_current_allowlist(&tx, profile_id)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("allowlist", profile_id, value, false))
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let current = pg_current_allowlist(&mut tx, profile_id)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("allowlist", profile_id, value, false))
                    .transpose()
            }),
        }
    }

    pub fn read_tool_hook_policy(&self, hook_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let tx = connection
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let current = sqlite_current_hook(&tx, hook_id)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("hook", hook_id, value, false))
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let current = pg_current_hook(&mut tx, hook_id)?;
                tx.rollback().map_err(|error| error.to_string())?;
                current
                    .map(|value| mutation_result("hook", hook_id, value, false))
                    .transpose()
            }),
        }
    }
}

fn require_sqlite_capabilities(tx: &Transaction<'_>, tool_names: &[String]) -> Result<(), String> {
    for tool_name in tool_names {
        let exists = tx
            .query_row(
                "SELECT 1 FROM tool_capabilities WHERE tool_name=?1",
                params![tool_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err(format!("tool capability is not registered: {tool_name}"));
        }
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_current_capability(
    client: &mut impl postgres::GenericClient,
    tool_name: &str,
) -> Result<Option<Value>, String> {
    let row = client
        .query_opt(
            "SELECT description, input_schema_json, output_schema_json,
                    requires_approval, risk_level
             FROM tool_capabilities WHERE tool_name=$1 FOR UPDATE",
            &[&tool_name],
        )
        .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let input = parse_optional_policy_json(row.get(1), "input_schema")?;
    let output = parse_optional_policy_json(row.get(2), "output_schema")?;
    Ok(Some(capability_value(
        tool_name,
        &row.get::<_, String>(0),
        input.as_ref(),
        output.as_ref(),
        row.get::<_, i32>(3) != 0,
        &row.get::<_, String>(4),
    )))
}

#[cfg(feature = "pg")]
fn pg_current_allowlist(
    client: &mut impl postgres::GenericClient,
    profile_id: &str,
) -> Result<Option<Value>, String> {
    if client
        .query_opt(
            "SELECT 1 FROM tool_allowlist_profiles WHERE profile_id=$1 FOR UPDATE",
            &[&profile_id],
        )
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Ok(None);
    }
    let rows = client
        .query(
            "SELECT tool_name FROM tool_allowlists WHERE profile_id=$1 ORDER BY tool_name",
            &[&profile_id],
        )
        .map_err(|error| error.to_string())?;
    let tool_names = rows
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    Ok(Some(allowlist_value(profile_id, &tool_names)))
}

#[cfg(feature = "pg")]
fn pg_current_hook(
    client: &mut impl postgres::GenericClient,
    hook_id: &str,
) -> Result<Option<Value>, String> {
    let row = client
        .query_opt(
            "SELECT hook_type, tool_name, condition_json, action, action_config_json, enabled
             FROM tool_hooks WHERE hook_id=$1 FOR UPDATE",
            &[&hook_id],
        )
        .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let condition = parse_optional_policy_json(row.get(2), "condition")?;
    let config = parse_optional_policy_json(row.get(4), "action_config")?;
    Ok(Some(hook_value(
        hook_id,
        &row.get::<_, String>(0),
        row.get::<_, Option<String>>(1).as_deref(),
        condition.as_ref(),
        &row.get::<_, String>(3),
        config.as_ref(),
        row.get::<_, i32>(5) != 0,
    )))
}

#[cfg(feature = "pg")]
fn require_pg_capabilities(
    client: &mut impl postgres::GenericClient,
    tool_names: &[String],
) -> Result<(), String> {
    for tool_name in tool_names {
        if client
            .query_opt(
                "SELECT 1 FROM tool_capabilities WHERE tool_name=$1",
                &[tool_name],
            )
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Err(format!("tool capability is not registered: {tool_name}"));
        }
    }
    Ok(())
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
    let details_json = details.to_string();
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details_json],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_policy_read_rejects_corrupt_stored_schema_json() {
        let store = LocalProductStore::new(":memory:").expect("store");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO tool_capabilities
                         (tool_name, description, input_schema_json, output_schema_json,
                          requires_approval, risk_level, created_at)
                         VALUES (?1, ?2, ?3, NULL, 0, 'low', ?4)",
                        params!["corrupt-capability", "fixture", "{not-json", "now"],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect("corrupt fixture insert");

        let error = store
            .read_tool_capability_policy("corrupt-capability")
            .expect_err("corrupt schema must fail closed");
        assert!(error.contains("invalid stored tool policy input_schema JSON"));
    }

    #[test]
    fn hook_policy_read_rejects_corrupt_stored_action_config_json() {
        let store = LocalProductStore::new(":memory:").expect("store");
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO tool_hooks
                         (hook_id, hook_type, tool_name, condition_json, action,
                          action_config_json, enabled, created_at)
                         VALUES (?1, 'pre_execution', NULL, NULL, 'block', ?2, 1, ?3)",
                        params!["corrupt-hook", "{not-json", "now"],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect("corrupt fixture insert");

        let error = store
            .read_tool_hook_policy("corrupt-hook")
            .expect_err("corrupt action config must fail closed");
        assert!(error.contains("invalid stored tool policy action_config JSON"));
    }
}
