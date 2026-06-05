use serde_json::{json, Value};

use super::{boundaries::local_boundaries, LocalProductStore, LOCAL_TEAM_EXPORT_SCHEMA_VERSION};

pub const LOCAL_IMPORT_SCHEMA_VERSION: &str = "local_team_export.v1";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportCounts {
    pub dispatches: i64,
    pub plans: i64,
    pub config: i64,
    pub team: i64,
    pub audit: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub imported: ImportCounts,
    pub errors: Vec<String>,
}

impl LocalProductStore {
    pub fn import_snapshot(&self, snapshot: &Value) -> Result<ImportResult, String> {
        let schema_version = snapshot
            .get("schema_version")
            .and_then(Value::as_str)
            .unwrap_or("");
        if schema_version != LOCAL_IMPORT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schema version: {schema_version} (expected {LOCAL_IMPORT_SCHEMA_VERSION})"
            ));
        }

        let mut errors = Vec::new();
        let mut counts = ImportCounts::default();

        if let Some(config) = snapshot.get("config").and_then(Value::as_object) {
            for (key, value) in config {
                match self.set_config_value(key, value.clone(), "import") {
                    Ok(_) => counts.config += 1,
                    Err(e) => errors.push(format!("config.{key}: {e}")),
                }
            }
        }

        if let Some(team) = snapshot.get("team") {
            if let Some(members) = team.get("members").and_then(Value::as_array) {
                for member in members {
                    let user_id = member.get("user_id").and_then(Value::as_str).unwrap_or("");
                    let display_name = member
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let role = member
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("member");
                    if user_id.is_empty() {
                        errors.push("team member missing user_id".to_string());
                        continue;
                    }
                    match self.upsert_team_member(user_id, display_name, role) {
                        Ok(_) => counts.team += 1,
                        Err(e) => errors.push(format!("team.{user_id}: {e}")),
                    }
                }
            }
        }

        if let Some(audit) = snapshot.get("audit").and_then(Value::as_array) {
            for event in audit {
                let actor = event
                    .get("actor")
                    .and_then(Value::as_str)
                    .unwrap_or("import");
                let action = event
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let resource = event
                    .get("resource")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let details = event.get("details").cloned().unwrap_or(Value::Null);
                match self.append_audit(actor, action, resource, &details) {
                    Ok(_) => counts.audit += 1,
                    Err(e) => errors.push(format!("audit.{action}: {e}")),
                }
            }
        }

        if let Some(dispatches) = snapshot.get("dispatches").and_then(Value::as_array) {
            for dispatch in dispatches {
                let raw_request = dispatch
                    .get("raw_request")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let request_source = dispatch
                    .get("request_source")
                    .and_then(Value::as_str)
                    .unwrap_or("import");
                let bundle = dispatch.get("bundle").cloned().unwrap_or(Value::Null);
                let dispatch_id =
                    dispatch
                        .get("dispatch_id")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            bundle
                                .pointer("/record/dispatch_id")
                                .and_then(Value::as_str)
                        });
                if let Some(dispatch_id) = dispatch_id {
                    if self.get_dispatch(dispatch_id)?.is_some() {
                        continue;
                    }
                }
                match self.record_dispatch(raw_request, request_source, &bundle, "import") {
                    Ok(_) => counts.dispatches += 1,
                    Err(e) => errors.push(format!("dispatch: {e}")),
                }
            }
        }

        if let Some(plans) = snapshot.get("plans").and_then(Value::as_array) {
            for plan in plans {
                match self.import_workflow_plan(plan) {
                    Ok(true) => counts.plans += 1,
                    Ok(false) => {}
                    Err(e) => errors.push(format!("plan: {e}")),
                }
            }
        }

        Ok(ImportResult {
            imported: counts,
            errors,
        })
    }

    pub fn export_snapshot(
        &self,
        executor_type: &str,
        provider_enabled: bool,
    ) -> Result<Value, String> {
        Ok(json!({
            "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
            "generated_at": self.now(),
            "dispatches": self.list_dispatches(10_000)?,
            "plans": self.search_workflow_plans(10_000, 0, None)?,
            "config": self.config_snapshot()?,
            "team": self.team_snapshot()?,
            "costs": self.cost_summary()?,
            "audit": self.audit_events(10_000)?,
            "boundaries": local_boundaries(executor_type, provider_enabled),
        }))
    }
}
