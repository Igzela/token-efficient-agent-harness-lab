use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};
use serde_json::{json, Value};

use crate::read_only_planner::WorkflowPlanIds;

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub fn create_workflow_plan<F>(
        &self,
        raw_request: &str,
        request_source: &str,
        actor: &str,
        build_plan: F,
    ) -> Result<Value, String>
    where
        F: FnOnce(&WorkflowPlanIds, &str) -> Result<Value, String>,
    {
        let mut build_plan = Some(build_plan);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let ids = WorkflowPlanIds::for_sequence(sequence);
                let created_at = self.now();
                let plan = build_plan.take().unwrap()(&ids, &created_at)?;
                let status = plan
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("planned_read_only");
                let graph = required_object(&plan, "graph")?;
                let analysis = required_object(&plan, "analysis")?;
                let boundaries = required_object(&plan, "boundaries")?;

                conn.execute(
                    "INSERT INTO workflow_plans
                     (plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                      status, workflow_id, dispatch_id, graph_json, analysis_json, boundaries_json, plan_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        sequence,
                        ids.plan_id,
                        created_at,
                        created_at,
                        raw_request,
                        request_source,
                        status,
                        ids.workflow_id,
                        ids.dispatch_id,
                        graph.to_string(),
                        analysis.to_string(),
                        boundaries.to_string(),
                        plan.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "workflow_plan.create",
                    &ids.plan_id,
                    &json!({
                        "request_source": request_source,
                        "status": status,
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                    }),
                )?;
                Ok(workflow_plan_value(
                    sequence,
                    &ids.plan_id,
                    &created_at,
                    &created_at,
                    raw_request,
                    request_source,
                    status,
                    &ids.workflow_id,
                    &ids.dispatch_id,
                    &plan,
                ))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence: i64 = client
                    .query_one(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let ids = WorkflowPlanIds::for_sequence(sequence);
                let created_at = self.now();
                let plan = build_plan.take().unwrap()(&ids, &created_at)?;
                let status = plan
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("planned_read_only");
                let graph = required_object(&plan, "graph")?;
                let analysis = required_object(&plan, "analysis")?;
                let boundaries = required_object(&plan, "boundaries")?;

                client
                    .execute(
                        "INSERT INTO workflow_plans
                         (plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                          status, workflow_id, dispatch_id, graph_json, analysis_json, boundaries_json, plan_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &sequence,
                            &ids.plan_id,
                            &created_at,
                            &created_at,
                            &raw_request,
                            &request_source,
                            &status,
                            &ids.workflow_id,
                            &ids.dispatch_id,
                            &graph.to_string(),
                            &analysis.to_string(),
                            &boundaries.to_string(),
                            &plan.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({
                    "request_source": request_source,
                    "status": status,
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                })
                .to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&created_at, &actor, &"workflow_plan.create", &ids.plan_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(workflow_plan_value(
                    sequence,
                    &ids.plan_id,
                    &created_at,
                    &created_at,
                    raw_request,
                    request_source,
                    status,
                    &ids.workflow_id,
                    &ids.dispatch_id,
                    &plan,
                ))
            }),
        }
    }

    /// Atomically recover or create the one delegated plan owned by a
    /// ProductTask and bind that plan to the task in the same transaction.
    ///
    /// The v36 unique owner index is the durable last line of defense. The
    /// transaction also removes the crash window between plan insertion and
    /// ProductTask binding, while returning the already-owned plan on replay.
    pub(crate) fn create_or_recover_delegated_workflow_plan<F>(
        &self,
        task_id: &str,
        plan_owner_id: &str,
        raw_request: &str,
        actor: &str,
        build_plan: F,
    ) -> Result<Value, String>
    where
        F: FnOnce(&WorkflowPlanIds, &str) -> Result<Value, String>,
    {
        if task_id.trim().is_empty()
            || plan_owner_id.len() != 64
            || !plan_owner_id
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err("delegated plan owner identity is invalid".into());
        }
        let mut build_plan = Some(build_plan);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                let transaction =
                    Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
                        .map_err(|error| error.to_string())?;
                let task_plan_id: Option<String> = transaction
                    .query_row(
                        "SELECT plan_id FROM product_tasks WHERE task_id=?1",
                        params![task_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("product task not found: {task_id}"))?;
                let owned_plan = sqlite_delegated_plan_by_owner(&transaction, plan_owner_id)?;
                if let Some(plan_id) = task_plan_id.as_deref() {
                    let bound_plan = sqlite_workflow_plan_by_id(&transaction, plan_id)?
                        .ok_or("delegated ProductTask plan is missing")?;
                    require_delegated_plan_owner(&bound_plan, task_id, plan_owner_id)?;
                    if owned_plan
                        .as_ref()
                        .is_some_and(|plan| plan.get("plan_id") != bound_plan.get("plan_id"))
                    {
                        return Err(
                            "delegated ProductTask owner conflicts with its bound plan".into()
                        );
                    }
                    transaction.commit().map_err(|error| error.to_string())?;
                    return Ok(bound_plan);
                }
                if let Some(plan) = owned_plan {
                    bind_delegated_plan_sqlite(
                        &transaction,
                        task_id,
                        plan["plan_id"]
                            .as_str()
                            .ok_or("recovered delegated plan missing plan_id")?,
                        actor,
                        &self.now(),
                    )?;
                    transaction.commit().map_err(|error| error.to_string())?;
                    return Ok(plan);
                }

                let sequence: i64 = transaction
                    .query_row(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let ids = WorkflowPlanIds::for_sequence(sequence);
                let created_at = self.now();
                let plan = build_plan.take().unwrap()(&ids, &created_at)?;
                require_delegated_plan_owner(&plan, task_id, plan_owner_id)?;
                let status = plan
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("planned_executable");
                let graph = required_object(&plan, "graph")?;
                let analysis = required_object(&plan, "analysis")?;
                let boundaries = required_object(&plan, "boundaries")?;
                transaction
                    .execute(
                        "INSERT INTO workflow_plans (
                            plan_sequence, plan_id, created_at, updated_at, raw_request,
                            request_source, status, workflow_id, dispatch_id, graph_json,
                            analysis_json, boundaries_json, plan_json,
                            delegated_plan_owner_id
                         ) VALUES (
                            ?1,?2,?3,?4,?5,'product_golden_path_delegated',?6,?7,?8,
                            ?9,?10,?11,?12,?13
                         )",
                        params![
                            sequence,
                            ids.plan_id,
                            created_at,
                            created_at,
                            raw_request,
                            status,
                            ids.workflow_id,
                            ids.dispatch_id,
                            graph.to_string(),
                            analysis.to_string(),
                            boundaries.to_string(),
                            plan.to_string(),
                            plan_owner_id,
                        ],
                    )
                    .map_err(|error| format!("delegated plan insert failed: {error}"))?;
                append_audit_locked(
                    &transaction,
                    &created_at,
                    actor,
                    "workflow_plan.create",
                    &ids.plan_id,
                    &json!({
                        "request_source": "product_golden_path_delegated",
                        "status": status,
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "delegated_plan_owner_id": plan_owner_id,
                    }),
                )?;
                bind_delegated_plan_sqlite(
                    &transaction,
                    task_id,
                    &ids.plan_id,
                    actor,
                    &created_at,
                )?;
                let value = workflow_plan_value(
                    sequence,
                    &ids.plan_id,
                    &created_at,
                    &created_at,
                    raw_request,
                    "product_golden_path_delegated",
                    status,
                    &ids.workflow_id,
                    &ids.dispatch_id,
                    &plan,
                );
                transaction.commit().map_err(|error| error.to_string())?;
                Ok(value)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|error| error.to_string())?;
                transaction
                    .query_one(
                        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                        &[&plan_owner_id],
                    )
                    .map_err(|error| error.to_string())?;
                let task_row = transaction
                    .query_opt(
                        "SELECT plan_id FROM product_tasks WHERE task_id=$1 FOR UPDATE",
                        &[&task_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("product task not found: {task_id}"))?;
                let task_plan_id: Option<String> = task_row.get(0);
                let owned_plan = pg_delegated_plan_by_owner(&mut transaction, plan_owner_id)?;
                if let Some(plan_id) = task_plan_id.as_deref() {
                    let bound_plan = pg_workflow_plan_by_id(&mut transaction, plan_id)?
                        .ok_or("delegated ProductTask plan is missing")?;
                    require_delegated_plan_owner(&bound_plan, task_id, plan_owner_id)?;
                    if owned_plan
                        .as_ref()
                        .is_some_and(|plan| plan.get("plan_id") != bound_plan.get("plan_id"))
                    {
                        return Err(
                            "delegated ProductTask owner conflicts with its bound plan".into()
                        );
                    }
                    transaction.commit().map_err(|error| error.to_string())?;
                    return Ok(bound_plan);
                }
                if let Some(plan) = owned_plan {
                    bind_delegated_plan_postgres(
                        &mut transaction,
                        task_id,
                        plan["plan_id"]
                            .as_str()
                            .ok_or("recovered delegated plan missing plan_id")?,
                        actor,
                        &self.now(),
                    )?;
                    transaction.commit().map_err(|error| error.to_string())?;
                    return Ok(plan);
                }

                // Existing plan IDs are derived from MAX(plan_sequence)+1.
                // Serialize that legacy allocator for this rare delegated path.
                transaction
                    .batch_execute("LOCK TABLE workflow_plans IN SHARE ROW EXCLUSIVE MODE")
                    .map_err(|error| error.to_string())?;
                let sequence: i64 = transaction
                    .query_one(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        &[],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let ids = WorkflowPlanIds::for_sequence(sequence);
                let created_at = self.now();
                let plan = build_plan.take().unwrap()(&ids, &created_at)?;
                require_delegated_plan_owner(&plan, task_id, plan_owner_id)?;
                let status = plan
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("planned_executable");
                let graph = required_object(&plan, "graph")?;
                let analysis = required_object(&plan, "analysis")?;
                let boundaries = required_object(&plan, "boundaries")?;
                transaction
                    .execute(
                        "INSERT INTO workflow_plans (
                            plan_sequence, plan_id, created_at, updated_at, raw_request,
                            request_source, status, workflow_id, dispatch_id, graph_json,
                            analysis_json, boundaries_json, plan_json,
                            delegated_plan_owner_id
                         ) VALUES (
                            $1,$2,$3,$4,$5,'product_golden_path_delegated',$6,$7,$8,
                            $9,$10,$11,$12,$13
                         )",
                        &[
                            &sequence,
                            &ids.plan_id,
                            &created_at,
                            &created_at,
                            &raw_request,
                            &status,
                            &ids.workflow_id,
                            &ids.dispatch_id,
                            &graph.to_string(),
                            &analysis.to_string(),
                            &boundaries.to_string(),
                            &plan.to_string(),
                            &plan_owner_id,
                        ],
                    )
                    .map_err(|error| format!("delegated plan insert failed: {error}"))?;
                let details = json!({
                    "request_source": "product_golden_path_delegated",
                    "status": status,
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "delegated_plan_owner_id": plan_owner_id,
                })
                .to_string();
                transaction
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1,$2,'workflow_plan.create',$3,$4)",
                        &[&created_at, &actor, &ids.plan_id, &details],
                    )
                    .map_err(|error| error.to_string())?;
                bind_delegated_plan_postgres(
                    &mut transaction,
                    task_id,
                    &ids.plan_id,
                    actor,
                    &created_at,
                )?;
                let value = workflow_plan_value(
                    sequence,
                    &ids.plan_id,
                    &created_at,
                    &created_at,
                    raw_request,
                    "product_golden_path_delegated",
                    status,
                    &ids.workflow_id,
                    &ids.dispatch_id,
                    &plan,
                );
                transaction.commit().map_err(|error| error.to_string())?;
                Ok(value)
            }),
        }
    }

    pub fn search_workflow_plans(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
            return self.list_workflow_plans_with_offset(limit, offset);
        };
        let pattern = format!("%{}%", escape_like(&search.to_lowercase()));
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         WHERE lower(plan_id) LIKE ?1 ESCAPE '\\'
                            OR lower(raw_request) LIKE ?1 ESCAPE '\\'
                            OR lower(request_source) LIKE ?1 ESCAPE '\\'
                            OR lower(status) LIKE ?1 ESCAPE '\\'
                            OR lower(workflow_id) LIKE ?1 ESCAPE '\\'
                            OR lower(dispatch_id) LIKE ?1 ESCAPE '\\'
                         ORDER BY plan_sequence DESC
                         LIMIT ?2 OFFSET ?3",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![pattern, limit, offset], workflow_plan_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         WHERE lower(plan_id) LIKE $1 ESCAPE '\\'
                            OR lower(raw_request) LIKE $1 ESCAPE '\\'
                            OR lower(request_source) LIKE $1 ESCAPE '\\'
                            OR lower(status) LIKE $1 ESCAPE '\\'
                            OR lower(workflow_id) LIKE $1 ESCAPE '\\'
                            OR lower(dispatch_id) LIKE $1 ESCAPE '\\'
                         ORDER BY plan_sequence DESC
                         LIMIT $2 OFFSET $3",
                        &[&pattern, &limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_workflow_plans(rows)
            }),
        }
    }

    pub fn list_workflow_plans_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         ORDER BY plan_sequence DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], workflow_plan_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         ORDER BY plan_sequence DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_workflow_plans(rows)
            }),
        }
    }

    pub fn get_workflow_plan(&self, plan_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         WHERE plan_id = ?1
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![plan_id], workflow_plan_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                                status, workflow_id, dispatch_id, plan_json
                         FROM workflow_plans
                         WHERE plan_id = $1
                         LIMIT 1",
                        &[&plan_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_workflow_plan_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn update_workflow_plan_status(
        &self,
        plan_id: &str,
        new_status: &str,
    ) -> Result<bool, String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let updated = conn
                    .execute(
                        "UPDATE workflow_plans SET status = ?1, updated_at = ?2 WHERE plan_id = ?3",
                        params![new_status, now, plan_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(updated > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let updated = client
                    .execute(
                        "UPDATE workflow_plans SET status = $1, updated_at = $2 WHERE plan_id = $3",
                        &[&new_status, &now, &plan_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(updated > 0)
            }),
        }
    }

    /// Bind a previously deferred managed-DeepSeek graph exactly once before a
    /// workflow run exists. This is intentionally a plan-owner operation: it
    /// cannot admit spend, create a lease, or schedule execution.
    pub fn bind_delegated_managed_deepseek_plan(
        &self,
        plan_id: &str,
        binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        actor: &str,
    ) -> Result<Value, String> {
        binding.validate()?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|e| e.to_string())?;
                let (workflow_id, plan_json): (String, String) = tx
                    .query_row(
                        "SELECT workflow_id, plan_json FROM workflow_plans WHERE plan_id=?1",
                        params![plan_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;
                let run_exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE plan_id=?1)",
                        params![plan_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if workflow_id != binding.workflow_id {
                    return Err("delegated managed graph binding workflow is stale".into());
                }
                let mut plan: Value = serde_json::from_str(&plan_json)
                    .map_err(|_| "delegated managed plan JSON is invalid".to_string())?;
                let (graph, changed) =
                    bind_deferred_managed_deepseek_graph(&mut plan, binding)?;
                if run_exists && changed {
                    return Err("delegated managed graph became runnable before binding".into());
                }
                if !changed {
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(());
                }
                let updated = tx
                    .execute(
                        "UPDATE workflow_plans SET graph_json=?1, plan_json=?2, updated_at=?3 WHERE plan_id=?4 AND workflow_id=?5 AND NOT EXISTS(SELECT 1 FROM workflow_runs WHERE plan_id=?4)",
                        params![graph.to_string(), plan.to_string(), now, plan_id, binding.workflow_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("delegated managed graph binding lost its compare-and-set".into());
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "workflow_plan.bind_delegated_managed_deepseek",
                    plan_id,
                    &json!({
                        "workflow_id": binding.workflow_id,
                        "attempt_id": binding.attempt_id,
                        "spend_authorization_id": binding.spend_authorization_id,
                        "attempt_lease_id": binding.attempt_lease_id,
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT workflow_id, plan_json FROM workflow_plans WHERE plan_id=$1 FOR UPDATE",
                        &[&plan_id],
                    )
                    .map_err(|e| e.to_string())?;
                let workflow_id: String = row.get(0);
                let plan_json: String = row.get(1);
                let run_exists: bool = tx
                    .query_one(
                        "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE plan_id=$1)",
                        &[&plan_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if workflow_id != binding.workflow_id {
                    return Err("delegated managed graph binding workflow is stale".into());
                }
                let mut plan: Value = serde_json::from_str(&plan_json)
                    .map_err(|_| "delegated managed plan JSON is invalid".to_string())?;
                let (graph, changed) =
                    bind_deferred_managed_deepseek_graph(&mut plan, binding)?;
                if run_exists && changed {
                    return Err("delegated managed graph became runnable before binding".into());
                }
                if !changed {
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(());
                }
                let updated = tx
                    .execute(
                        "UPDATE workflow_plans SET graph_json=$1, plan_json=$2, updated_at=$3 WHERE plan_id=$4 AND workflow_id=$5 AND NOT EXISTS(SELECT 1 FROM workflow_runs WHERE plan_id=$4)",
                        &[&graph.to_string(), &plan.to_string(), &now, &plan_id, &binding.workflow_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("delegated managed graph binding lost its compare-and-set".into());
                }
                let details = json!({
                    "workflow_id": binding.workflow_id,
                    "attempt_id": binding.attempt_id,
                    "spend_authorization_id": binding.spend_authorization_id,
                    "attempt_lease_id": binding.attempt_lease_id,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1,$2,$3,$4,$5)",
                    &[&now, &actor, &"workflow_plan.bind_delegated_managed_deepseek", &plan_id, &details],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(())
            }),
        }?;
        self.get_workflow_plan(plan_id)?
            .ok_or_else(|| "bound workflow plan disappeared".to_string())
    }

    pub fn import_workflow_plan(&self, plan: &Value) -> Result<bool, String> {
        let plan_id = plan
            .get("plan_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "plan missing plan_id".to_string())?;
        if self.get_workflow_plan(plan_id)?.is_some() {
            return Ok(false);
        }
        let workflow_id = plan
            .get("workflow_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("plan {plan_id} missing workflow_id"))?;
        let dispatch_id = plan
            .get("dispatch_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("plan {plan_id} missing dispatch_id"))?;
        let status = plan
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("planned_read_only");
        let raw_request = plan
            .get("raw_request")
            .and_then(Value::as_str)
            .unwrap_or("");
        let request_source = plan
            .get("request_source")
            .and_then(Value::as_str)
            .unwrap_or("import");
        let graph = required_object(plan, "graph")?;
        let analysis = required_object(plan, "analysis")?;
        let boundaries = required_object(plan, "boundaries")?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let created_at = plan
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = plan
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                conn.execute(
                    "INSERT INTO workflow_plans
                     (plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                      status, workflow_id, dispatch_id, graph_json, analysis_json, boundaries_json, plan_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        sequence,
                        plan_id,
                        created_at,
                        updated_at,
                        raw_request,
                        request_source,
                        status,
                        workflow_id,
                        dispatch_id,
                        graph.to_string(),
                        analysis.to_string(),
                        boundaries.to_string(),
                        plan.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    "import",
                    "workflow_plan.import",
                    plan_id,
                    &json!({"workflow_id": workflow_id, "dispatch_id": dispatch_id}),
                )?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence: i64 = client
                    .query_one(
                        "SELECT COALESCE(MAX(plan_sequence), 0) + 1 FROM workflow_plans",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let created_at = plan
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = plan
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                client
                    .execute(
                        "INSERT INTO workflow_plans
                         (plan_sequence, plan_id, created_at, updated_at, raw_request, request_source,
                          status, workflow_id, dispatch_id, graph_json, analysis_json, boundaries_json, plan_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &sequence,
                            &plan_id,
                            &created_at,
                            &updated_at,
                            &raw_request,
                            &request_source,
                            &status,
                            &workflow_id,
                            &dispatch_id,
                            &graph.to_string(),
                            &analysis.to_string(),
                            &boundaries.to_string(),
                            &plan.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let now = self.now();
                let details = json!({"workflow_id": workflow_id, "dispatch_id": dispatch_id}).to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &"import", &"workflow_plan.import", &plan_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(true)
            }),
        }
    }
}

fn workflow_plan_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let plan_text: String = row.get(9)?;
    let plan: Value = serde_json::from_str(&plan_text).unwrap_or(Value::Null);
    Ok(workflow_plan_value(
        row.get::<_, i64>(0)?,
        &row.get::<_, String>(1)?,
        &row.get::<_, String>(2)?,
        &row.get::<_, String>(3)?,
        &row.get::<_, String>(4)?,
        &row.get::<_, String>(5)?,
        &row.get::<_, String>(6)?,
        &row.get::<_, String>(7)?,
        &row.get::<_, String>(8)?,
        &plan,
    ))
}

#[cfg(feature = "pg")]
fn pg_workflow_plan_row(row: &postgres::Row) -> Value {
    let plan_text: String = row.get(9);
    let plan: Value = serde_json::from_str(&plan_text).unwrap_or(Value::Null);
    workflow_plan_value(
        row.get::<_, i64>(0),
        &row.get::<_, String>(1),
        &row.get::<_, String>(2),
        &row.get::<_, String>(3),
        &row.get::<_, String>(4),
        &row.get::<_, String>(5),
        &row.get::<_, String>(6),
        &row.get::<_, String>(7),
        &row.get::<_, String>(8),
        &plan,
    )
}

#[cfg(feature = "pg")]
fn pg_collect_workflow_plans(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    Ok(rows.iter().map(pg_workflow_plan_row).collect())
}

fn workflow_plan_value(
    sequence: i64,
    plan_id: &str,
    created_at: &str,
    updated_at: &str,
    raw_request: &str,
    request_source: &str,
    status: &str,
    workflow_id: &str,
    dispatch_id: &str,
    plan: &Value,
) -> Value {
    let mut value = plan.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("plan_sequence".to_string(), json!(sequence));
        obj.insert("plan_id".to_string(), json!(plan_id));
        obj.insert("created_at".to_string(), json!(created_at));
        obj.insert("updated_at".to_string(), json!(updated_at));
        obj.insert("raw_request".to_string(), json!(raw_request));
        obj.insert("request_source".to_string(), json!(request_source));
        obj.insert("status".to_string(), json!(status));
        obj.insert("workflow_id".to_string(), json!(workflow_id));
        obj.insert("dispatch_id".to_string(), json!(dispatch_id));
        value
    } else {
        json!({
            "plan_sequence": sequence,
            "plan_id": plan_id,
            "created_at": created_at,
            "updated_at": updated_at,
            "raw_request": raw_request,
            "request_source": request_source,
            "status": status,
            "workflow_id": workflow_id,
            "dispatch_id": dispatch_id,
        })
    }
}

fn require_delegated_plan_owner(
    plan: &Value,
    task_id: &str,
    plan_owner_id: &str,
) -> Result<(), String> {
    if plan
        .get("request_source")
        .and_then(Value::as_str)
        .is_some_and(|source| source != "product_golden_path_delegated")
        || plan
            .pointer("/advisory/product_task_id")
            .and_then(Value::as_str)
            != Some(task_id)
        || plan
            .pointer("/advisory/delegated_plan_owner_id")
            .and_then(Value::as_str)
            != Some(plan_owner_id)
    {
        return Err("delegated workflow plan owner binding is invalid".into());
    }
    Ok(())
}

fn sqlite_workflow_plan_by_id(
    connection: &rusqlite::Connection,
    plan_id: &str,
) -> Result<Option<Value>, String> {
    connection
        .query_row(
            "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request,
                    request_source, status, workflow_id, dispatch_id, plan_json
             FROM workflow_plans WHERE plan_id=?1",
            params![plan_id],
            workflow_plan_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn sqlite_delegated_plan_by_owner(
    connection: &rusqlite::Connection,
    plan_owner_id: &str,
) -> Result<Option<Value>, String> {
    connection
        .query_row(
            "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request,
                    request_source, status, workflow_id, dispatch_id, plan_json
             FROM workflow_plans WHERE delegated_plan_owner_id=?1",
            params![plan_owner_id],
            workflow_plan_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn bind_delegated_plan_sqlite(
    transaction: &Transaction<'_>,
    task_id: &str,
    plan_id: &str,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let updated = transaction
        .execute(
            "UPDATE product_tasks SET plan_id=?1, updated_at=?2
             WHERE task_id=?3 AND plan_id IS NULL",
            params![plan_id, now, task_id],
        )
        .map_err(|error| error.to_string())?;
    if updated != 1 {
        return Err("delegated plan binding is stale or conflicting".into());
    }
    append_audit_locked(
        transaction,
        now,
        actor,
        "product_task.bind_delegated_plan",
        task_id,
        &json!({"plan_id": plan_id}),
    )
    .map(|_| ())
}

#[cfg(feature = "pg")]
fn pg_workflow_plan_by_id(
    client: &mut impl postgres::GenericClient,
    plan_id: &str,
) -> Result<Option<Value>, String> {
    client
        .query_opt(
            "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request,
                    request_source, status, workflow_id, dispatch_id, plan_json
             FROM workflow_plans WHERE plan_id=$1",
            &[&plan_id],
        )
        .map(|row| row.map(|row| pg_workflow_plan_row(&row)))
        .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn pg_delegated_plan_by_owner(
    client: &mut impl postgres::GenericClient,
    plan_owner_id: &str,
) -> Result<Option<Value>, String> {
    client
        .query_opt(
            "SELECT plan_sequence, plan_id, created_at, updated_at, raw_request,
                    request_source, status, workflow_id, dispatch_id, plan_json
             FROM workflow_plans WHERE delegated_plan_owner_id=$1",
            &[&plan_owner_id],
        )
        .map(|row| row.map(|row| pg_workflow_plan_row(&row)))
        .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn bind_delegated_plan_postgres(
    transaction: &mut postgres::Transaction<'_>,
    task_id: &str,
    plan_id: &str,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let updated = transaction
        .execute(
            "UPDATE product_tasks SET plan_id=$1, updated_at=$2
             WHERE task_id=$3 AND plan_id IS NULL",
            &[&plan_id, &now, &task_id],
        )
        .map_err(|error| error.to_string())?;
    if updated != 1 {
        return Err("delegated plan binding is stale or conflicting".into());
    }
    let details = json!({"plan_id": plan_id}).to_string();
    transaction
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1,$2,'product_task.bind_delegated_plan',$3,$4)",
            &[&now, &actor, &task_id, &details],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn bind_deferred_managed_deepseek_graph(
    plan: &mut Value,
    binding: &crate::provider::managed_deepseek::ManagedCallBinding,
) -> Result<(Value, bool), String> {
    let graph = plan
        .get_mut("graph")
        .and_then(Value::as_object_mut)
        .ok_or("delegated managed plan graph is missing")?;
    if graph.get("workflow_id").and_then(Value::as_str) != Some(binding.workflow_id.as_str()) {
        return Err("delegated managed graph workflow identity mismatch".into());
    }
    let nodes = graph
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or("delegated managed graph nodes are missing")?;
    let mut bound_roles = std::collections::BTreeSet::new();
    let mut deferred_count = 0usize;
    let mut already_bound_count = 0usize;
    for node in nodes {
        let node_id = node
            .get("node_id")
            .and_then(Value::as_str)
            .ok_or("delegated managed graph node_id is missing")?
            .to_string();
        let Some(managed) = node
            .get_mut("managed_deepseek")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let role = managed
            .get("role")
            .and_then(Value::as_str)
            .ok_or("delegated managed graph role is missing")?;
        if !matches!(role, "planner" | "implementer" | "reviewer")
            || !bound_roles.insert(role.to_string())
        {
            return Err("delegated managed graph is not an exact deferred route".into());
        }
        let mut stage_binding = serde_json::to_value(binding)
            .map_err(|_| "delegated managed binding cannot be serialized".to_string())?;
        stage_binding["node_id"] = json!(node_id);
        match managed.get("binding_status").and_then(Value::as_str) {
            Some("deferred")
                if managed
                    .get("binding")
                    .and_then(Value::as_object)
                    .and_then(|value| value.get("node_id"))
                    .and_then(Value::as_str)
                    == Some(node_id.as_str()) =>
            {
                deferred_count += 1;
                managed.insert("binding".to_string(), stage_binding);
                managed.insert("binding_status".to_string(), json!("bound"));
            }
            Some("bound") if managed.get("binding") == Some(&stage_binding) => {
                already_bound_count += 1;
            }
            _ => return Err("delegated managed graph binding is stale or conflicting".into()),
        }
    }
    if bound_roles
        != [
            "planner".to_string(),
            "implementer".to_string(),
            "reviewer".to_string(),
        ]
        .into_iter()
        .collect()
    {
        return Err("delegated managed graph must contain exactly three managed stages".into());
    }
    if !matches!((deferred_count, already_bound_count), (3, 0) | (0, 3)) {
        return Err("delegated managed graph has a partial binding".into());
    }
    Ok((Value::Object(graph.clone()), deferred_count == 3))
}

fn required_object<'a>(plan: &'a Value, field: &str) -> Result<&'a Value, String> {
    let value = plan
        .get(field)
        .ok_or_else(|| format!("plan missing {field}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!("plan {field} must be an object"))
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

#[cfg(test)]
mod delegated_managed_plan_tests {
    use super::bind_deferred_managed_deepseek_graph;
    use crate::provider::managed_deepseek::ManagedCallBinding;
    use crate::storage::LocalProductStore;
    use serde_json::json;
    use std::sync::{Arc, Barrier};

    fn binding() -> ManagedCallBinding {
        ManagedCallBinding {
            product_task_id: "product-task".into(),
            workflow_id: "workflow-1".into(),
            node_id: "workflow-1-planning".into(),
            attempt_id: "attempt-1".into(),
            spend_authorization_id: "spend-1".into(),
            attempt_lease_id: "lease-1".into(),
        }
    }

    #[test]
    fn deferred_managed_graph_binds_each_stage_once() {
        let mut plan = json!({
            "graph": {
                "workflow_id": "workflow-1",
                "nodes": [
                    {"node_id":"workflow-1-planning","managed_deepseek":{"role":"planner","binding_status":"deferred","binding":{"node_id":"workflow-1-planning"}}},
                    {"node_id":"workflow-1-implementation","managed_deepseek":{"role":"implementer","binding_status":"deferred","binding":{"node_id":"workflow-1-implementation"}}},
                    {"node_id":"workflow-1-deterministic_verification","command":"true"},
                    {"node_id":"workflow-1-review","managed_deepseek":{"role":"reviewer","binding_status":"deferred","binding":{"node_id":"workflow-1-review"}}}
                ]
            }
        });
        let (graph, changed) = bind_deferred_managed_deepseek_graph(&mut plan, &binding()).unwrap();
        assert!(changed);
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["managed_deepseek"]["binding_status"], "bound");
        assert_eq!(
            nodes[1]["managed_deepseek"]["binding"]["node_id"],
            "workflow-1-implementation"
        );
        let (_, changed) = bind_deferred_managed_deepseek_graph(&mut plan, &binding()).unwrap();
        assert!(!changed);
        let mut conflicting = binding();
        conflicting.attempt_id = "attempt-2".into();
        assert!(bind_deferred_managed_deepseek_graph(&mut plan, &conflicting).is_err());
    }

    #[test]
    fn concurrent_delegated_plan_prepare_has_one_restart_safe_execution_identity() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("delegated-plan.db");
        let store = Arc::new(LocalProductStore::new(&database_path).unwrap());
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            'task-concurrent', 'product_task.v1', 'tenant-a', 'workspace-a',
                            'delegated-plan-concurrency', 'workspace_bound', 1, ?1,
                            'target-a', '/redacted/target', ?2, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?3, '{}', '{}', NULL, NULL, NULL, NULL, NULL,
                            '2026-07-31T00:00:00Z', '2026-07-31T00:00:00Z', 'test'
                         )",
                        rusqlite::params!["a".repeat(64), "b".repeat(40), "c".repeat(64)],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        let owner_id = "d".repeat(64);
        let barrier = Arc::new(Barrier::new(8));
        let plans = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for index in 0..8 {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                let owner_id = owner_id.clone();
                handles.push(scope.spawn(move || {
                    barrier.wait();
                    store.create_or_recover_delegated_workflow_plan(
                        "task-concurrent",
                        &owner_id,
                        "bounded delegated task",
                        &format!("concurrent-{index}"),
                        |ids, created_at| {
                            Ok(json!({
                                "schema_version": "read_only_plan.v1",
                                "status": "planned_executable",
                                "graph": {
                                    "nodes": [],
                                    "edges": [],
                                    "workflow_id": ids.workflow_id,
                                    "dispatch_id": ids.dispatch_id
                                },
                                "analysis": {},
                                "advisory": {
                                    "product_task_id": "task-concurrent",
                                    "delegated_plan_owner_id": owner_id
                                },
                                "boundaries": {
                                    "execution_authority": "delegated_product_golden_path"
                                },
                                "created_at": created_at
                            }))
                        },
                    )
                }));
            }
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap().unwrap())
                .collect::<Vec<_>>()
        });
        let plan_id = plans[0]["plan_id"].as_str().unwrap();
        assert!(plans
            .iter()
            .all(|plan| plan["plan_id"].as_str() == Some(plan_id)));
        assert_eq!(
            store.list_workflow_plans_with_offset(100, 0).unwrap().len(),
            1
        );
        assert_eq!(
            store.get_product_task("task-concurrent").unwrap().unwrap()["plan_id"],
            plan_id
        );

        drop(store);
        let restarted = LocalProductStore::new(&database_path).unwrap();
        let replay = restarted
            .create_or_recover_delegated_workflow_plan(
                "task-concurrent",
                &owner_id,
                "bounded delegated task",
                "restart",
                |_, _| Err("restart must recover without rebuilding".into()),
            )
            .unwrap();
        assert_eq!(replay["plan_id"], plan_id);
        assert_eq!(
            restarted
                .list_workflow_plans_with_offset(100, 0)
                .unwrap()
                .len(),
            1
        );
    }
}
