use rusqlite::{params, Row};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::{append_audit_locked, collect_values, str_at, DatabaseConnection, LocalProductStore};
use crate::feedback::{OutcomeAttributor, PatternDetector, RunTraceRecorder};

impl LocalProductStore {
    pub fn record_dispatch(
        &self,
        raw_request: &str,
        request_source: &str,
        bundle: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let dispatch_id = str_at(bundle, &["record", "dispatch_id"]).unwrap_or("unknown");
        let default_created_at = self.now();
        let created_at = str_at(bundle, &["record", "created_at"]).unwrap_or(&default_created_at);
        let final_status = str_at(bundle, &["record", "final_status"]).unwrap_or("unknown");
        let selected_tier = str_at(bundle, &["decision", "selected_tier"]).unwrap_or("unknown");
        let risk_level = str_at(bundle, &["analysis", "risk_level"]).unwrap_or("unknown");
        let reserved_cost = bundle
            .pointer("/decision/budget_reservation/reserved_cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let bundle_json = serde_json::to_string(bundle).map_err(|e| e.to_string())?;

        let input_tokens = bundle
            .pointer("/execution_result/input_tokens")
            .and_then(Value::as_i64);
        let output_tokens = bundle
            .pointer("/execution_result/output_tokens")
            .and_then(Value::as_i64);
        let estimated_cost_usd = bundle
            .pointer("/execution_result/estimated_cost")
            .and_then(Value::as_f64);
        let executor_type = bundle
            .pointer("/execution_result/executor_type")
            .and_then(Value::as_str)
            .unwrap_or("noop");
        let latency_ms = bundle
            .pointer("/execution_result/latency_ms")
            .and_then(Value::as_i64);

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO dispatch_history
                     (dispatch_id, created_at, raw_request, request_source, final_status,
                      selected_tier, risk_level, reserved_cost, bundle_json,
                      input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        dispatch_id,
                        created_at,
                        raw_request,
                        request_source,
                        final_status,
                        selected_tier,
                        risk_level,
                        reserved_cost,
                        bundle_json,
                        input_tokens,
                        output_tokens,
                        estimated_cost_usd,
                        executor_type,
                        latency_ms,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let history_id = conn.last_insert_rowid();
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "dispatch.record",
                    dispatch_id,
                    &json!({"history_id": history_id, "request_source": request_source}),
                )?;
                Ok(json!({
                    "history_id": history_id,
                    "dispatch_id": dispatch_id,
                    "created_at": created_at,
                    "raw_request": raw_request,
                    "request_source": request_source,
                    "final_status": final_status,
                    "selected_tier": selected_tier,
                    "risk_level": risk_level,
                    "reserved_cost": reserved_cost,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "estimated_cost_usd": estimated_cost_usd,
                    "executor_type": executor_type,
                    "latency_ms": latency_ms,
                    "bundle": bundle,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_one(
                        "INSERT INTO dispatch_history
                         (dispatch_id, created_at, raw_request, request_source, final_status,
                          selected_tier, risk_level, reserved_cost, bundle_json,
                          input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                         RETURNING history_id",
                        &[
                            &dispatch_id,
                            &created_at,
                            &raw_request,
                            &request_source,
                            &final_status,
                            &selected_tier,
                            &risk_level,
                            &reserved_cost,
                            &bundle_json,
                            &input_tokens,
                            &output_tokens,
                            &estimated_cost_usd,
                            &executor_type,
                            &latency_ms,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let history_id: i64 = row.get(0);
                let now = self.now();
                let details = json!({"history_id": history_id, "request_source": request_source}).to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"dispatch.record", &dispatch_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(json!({
                    "history_id": history_id,
                    "dispatch_id": dispatch_id,
                    "created_at": created_at,
                    "raw_request": raw_request,
                    "request_source": request_source,
                    "final_status": final_status,
                    "selected_tier": selected_tier,
                    "risk_level": risk_level,
                    "reserved_cost": reserved_cost,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "estimated_cost_usd": estimated_cost_usd,
                    "executor_type": executor_type,
                    "latency_ms": latency_ms,
                    "bundle": bundle,
                }))
            }),
        }
    }

    pub fn list_dispatches(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.list_dispatches_with_offset(limit, 0)
    }

    pub fn list_dispatches_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         ORDER BY history_id DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], dispatch_history_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         ORDER BY history_id DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_dispatch_history_rows(rows)
            }),
        }
    }

    pub fn search_dispatches(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
            return self.list_dispatches_with_offset(limit, offset);
        };
        let pattern = format!("%{}%", escape_like(&search.to_lowercase()));

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         WHERE lower(dispatch_id) LIKE ?1 ESCAPE '\\'
                            OR lower(raw_request) LIKE ?1 ESCAPE '\\'
                            OR lower(request_source) LIKE ?1 ESCAPE '\\'
                            OR lower(final_status) LIKE ?1 ESCAPE '\\'
                            OR lower(selected_tier) LIKE ?1 ESCAPE '\\'
                            OR lower(risk_level) LIKE ?1 ESCAPE '\\'
                         ORDER BY history_id DESC
                         LIMIT ?2 OFFSET ?3",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![pattern, limit, offset], dispatch_history_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         WHERE lower(dispatch_id) LIKE $1 ESCAPE '\\'
                            OR lower(raw_request) LIKE $1 ESCAPE '\\'
                            OR lower(request_source) LIKE $1 ESCAPE '\\'
                            OR lower(final_status) LIKE $1 ESCAPE '\\'
                            OR lower(selected_tier) LIKE $1 ESCAPE '\\'
                            OR lower(risk_level) LIKE $1 ESCAPE '\\'
                         ORDER BY history_id DESC
                         LIMIT $2 OFFSET $3",
                        &[&pattern, &limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_dispatch_history_rows(rows)
            }),
        }
    }

    pub fn get_dispatch(&self, dispatch_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         WHERE dispatch_id = ?1
                         ORDER BY history_id DESC
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![dispatch_id], dispatch_history_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(val)) => Ok(Some(val)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         WHERE dispatch_id = $1
                         ORDER BY history_id DESC
                         LIMIT 1",
                        &[&dispatch_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_dispatch_history_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn dispatch_metrics(&self, limit: i64) -> Result<Value, String> {
        let dispatches = self.dispatches_for_read_models(limit.clamp(0, 500), 0)?;
        let mut totals = MetricBucket::default();
        let mut by_tier: BTreeMap<String, MetricBucket> = BTreeMap::new();
        let mut by_task_class: BTreeMap<String, MetricBucket> = BTreeMap::new();
        let mut by_final_status: BTreeMap<String, MetricBucket> = BTreeMap::new();
        let mut by_evaluation_status: BTreeMap<String, MetricBucket> = BTreeMap::new();

        for dispatch in &dispatches {
            let bundle = dispatch.get("bundle").unwrap_or(&Value::Null);
            let selected_tier = selected_tier(dispatch, bundle);
            let task_class = task_class(bundle);
            let final_status = final_status(dispatch, bundle);
            let evaluation_status = evaluation_status(bundle);
            let success = dispatch_success(bundle, &final_status, &evaluation_status);

            totals.ingest(dispatch, bundle, success);
            by_tier
                .entry(selected_tier)
                .or_default()
                .ingest(dispatch, bundle, success);
            by_task_class
                .entry(task_class)
                .or_default()
                .ingest(dispatch, bundle, success);
            by_final_status
                .entry(final_status)
                .or_default()
                .ingest(dispatch, bundle, success);
            by_evaluation_status
                .entry(evaluation_status)
                .or_default()
                .ingest(dispatch, bundle, success);
        }

        Ok(json!({
            "schema_version": "dispatch_metrics.v1",
            "limit": limit.clamp(0, 500),
            "totals": metric_bucket_json(&totals),
            "by_tier": metric_bucket_entries(by_tier, "selected_tier"),
            "by_task_class": metric_bucket_entries(by_task_class, "task_class"),
            "by_final_status": metric_bucket_entries(by_final_status, "final_status"),
            "by_evaluation_status": metric_bucket_entries(by_evaluation_status, "evaluation_status"),
        }))
    }

    pub fn feedback_traces(
        &self,
        limit: i64,
        offset: i64,
        task_class_filter: Option<&str>,
        tier_filter: Option<&str>,
        status_filter: Option<&str>,
    ) -> Result<Value, String> {
        let limit = limit.clamp(0, 500) as usize;
        let offset = offset.max(0);
        let traces: Vec<Value> = self
            .dispatches_for_read_models(500, offset)?
            .into_iter()
            .map(|dispatch| {
                let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
                let attribution = OutcomeAttributor::attribute(&trace);
                let attribution_value = serde_json::to_value(&attribution).unwrap_or(Value::Null);
                let mut json = RunTraceRecorder::to_feedback_trace_json(&trace, attribution_value);
                // Preserve dispatch-level fields not captured by RunTrace
                if let Value::Object(ref mut map) = json {
                    map.insert(
                        "raw_request".to_string(),
                        dispatch.get("raw_request").cloned().unwrap_or(Value::Null),
                    );
                    map.insert(
                        "request_source".to_string(),
                        dispatch
                            .get("request_source")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                }
                json
            })
            .filter(|trace| trace_matches(trace, "task_class", task_class_filter))
            .filter(|trace| trace_matches(trace, "tier", tier_filter))
            .filter(|trace| trace_matches(trace, "status", status_filter))
            .take(limit)
            .collect();

        Ok(json!({
            "schema_version": "feedback_traces.v1",
            "total": traces.len(),
            "limit": limit,
            "offset": offset,
            "traces": traces,
        }))
    }

    pub fn feedback_patterns(
        &self,
        task_class_filter: Option<&str>,
        tier_filter: Option<&str>,
    ) -> Result<Value, String> {
        let dispatches = self.dispatches_for_read_models(500, 0)?;
        let traces: Vec<crate::feedback::run_trace_recorder::RunTrace> = dispatches
            .iter()
            .map(RunTraceRecorder::record_from_dispatch)
            .filter(|trace| {
                task_class_filter
                    .map(|tc| trace.task_class.eq_ignore_ascii_case(tc.trim()))
                    .unwrap_or(true)
            })
            .filter(|trace| {
                tier_filter
                    .map(|tier| trace.selected_tier.eq_ignore_ascii_case(tier.trim()))
                    .unwrap_or(true)
            })
            .collect();

        let detector = PatternDetector::default();
        let mut patterns = detector.detect(&traces);

        if let Some(tc) = task_class_filter {
            let tc = tc.trim();
            if !tc.is_empty() {
                patterns.retain(|p| {
                    p.affected_task_class
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case(tc))
                        .unwrap_or(true)
                });
            }
        }
        if let Some(tier) = tier_filter {
            let tier = tier.trim();
            if !tier.is_empty() {
                patterns.retain(|p| {
                    p.affected_tier
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case(tier))
                        .unwrap_or(true)
                });
            }
        }

        Ok(json!({
            "schema_version": "feedback_patterns.v1",
            "total": patterns.len(),
            "patterns": patterns,
        }))
    }

    pub fn cost_of_pass(
        &self,
        task_class_filter: Option<&str>,
        tier_filter: Option<&str>,
    ) -> Result<Value, String> {
        let mut groups: BTreeMap<(String, String), CostOfPassBucket> = BTreeMap::new();

        for dispatch in self.dispatches_for_read_models(500, 0)? {
            let bundle = dispatch.get("bundle").unwrap_or(&Value::Null);
            let task_class = task_class(bundle);
            let selected_tier = selected_tier(&dispatch, bundle);
            if !string_filter_matches(&task_class, task_class_filter)
                || !string_filter_matches(&selected_tier, tier_filter)
            {
                continue;
            }
            let final_status = final_status(&dispatch, bundle);
            let evaluation_status = evaluation_status(bundle);
            let success = dispatch_success(bundle, &final_status, &evaluation_status);
            groups
                .entry((task_class, selected_tier))
                .or_default()
                .ingest(&dispatch, bundle, success);
        }

        let groups: Vec<Value> = groups
            .into_iter()
            .map(|((task_class, selected_tier), bucket)| {
                json!({
                    "task_class": task_class,
                    "tier": selected_tier,
                    "selected_tier": selected_tier,
                    "total_count": bucket.dispatch_count,
                    "pass_count": bucket.success_count,
                    "pass_rate": success_rate(bucket.success_count, bucket.dispatch_count),
                    "dispatch_count": bucket.dispatch_count,
                    "success_count": bucket.success_count,
                    "total_cost": bucket.total_cost,
                    "total_reserved_cost": bucket.total_reserved_cost,
                    "total_estimated_cost_usd": bucket.total_estimated_cost_usd,
                    "estimated_cost_available": bucket.estimated_cost_rows > 0,
                    "cost_source": "estimated_cost_usd_with_reserved_cost_fallback",
                    "average_cost_usd": if bucket.success_count > 0 {
                        json!(bucket.total_cost / bucket.success_count as f64)
                    } else {
                        Value::Null
                    },
                })
            })
            .collect();

        Ok(json!({
            "schema_version": "feedback_cost_of_pass.v1",
            "rows": groups,
        }))
    }

    pub fn simulation_report(&self, limit: i64) -> Result<Value, String> {
        let limit = limit.clamp(0, 500) as usize;
        let mut shadow_route_count = 0_i64;
        let mut by_shadow_tier: BTreeMap<String, i64> = BTreeMap::new();
        let mut dispatch_reports = Vec::new();

        for dispatch in self.dispatches_for_read_models(limit as i64, 0)? {
            let bundle = dispatch.get("bundle").unwrap_or(&Value::Null);
            let decision = bundle.get("decision").cloned().unwrap_or(Value::Null);
            let shadow_routes = decision
                .get("shadow_routes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            for route in &shadow_routes {
                let tier = str_at(route, &["tier"]).unwrap_or("unknown").to_string();
                *by_shadow_tier.entry(tier).or_insert(0) += 1;
            }
            shadow_route_count += shadow_routes.len() as i64;

            dispatch_reports.push(json!({
                "scenario_id": format!(
                    "shadow-{}",
                    dispatch.get("dispatch_id").and_then(Value::as_str).unwrap_or("unknown")
                ),
                "status": "shadow_only",
                "task_class": task_class(bundle),
                "tier": selected_tier(&dispatch, bundle),
                "recommendation": "diagnostic_only_no_routing_effect",
                "dispatch_id": dispatch.get("dispatch_id").cloned().unwrap_or(Value::Null),
                "history_id": dispatch.get("history_id").cloned().unwrap_or(Value::Null),
                "selected_tier": selected_tier(&dispatch, bundle),
                "budget_reservation": decision.get("budget_reservation").cloned().unwrap_or(Value::Null),
                "executor_type": executor_type(&dispatch, bundle),
                "decision_status": str_at(&decision, &["decision_status"]).unwrap_or("unknown"),
                "routing_mode": str_at(&decision, &["routing_mode"]).unwrap_or("unknown"),
                "shadow_routes": shadow_routes,
                "shadow_influence": shadow_influence_disabled(),
            }));
        }

        let by_shadow_tier: Vec<Value> = by_shadow_tier
            .into_iter()
            .map(|(shadow_tier, route_count)| {
                json!({
                    "shadow_tier": shadow_tier,
                    "route_count": route_count,
                })
            })
            .collect();

        Ok(json!({
            "schema_version": "dispatch_simulation_report.v1",
            "mode": "read_only_shadow_report",
            "generated_from": "dispatch_history.bundle_json.decision.shadow_routes",
            "boundaries": shadow_influence_disabled(),
            "totals": {
                "dispatch_count": dispatch_reports.len(),
                "shadow_route_count": shadow_route_count,
            },
            "summary": {
                "dispatch_count": dispatch_reports.len(),
                "shadow_route_count": shadow_route_count,
                "active_routing_allowed": false,
            },
            "by_shadow_tier": by_shadow_tier,
            "report": dispatch_reports,
            "dispatches": dispatch_reports,
        }))
    }

    fn dispatches_for_read_models(&self, limit: i64, offset: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         ORDER BY history_id DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], dispatch_history_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                                final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                                input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                         FROM dispatch_history
                         ORDER BY history_id DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_dispatch_history_rows(rows)
            }),
        }
    }
}

#[derive(Default)]
struct MetricBucket {
    dispatch_count: i64,
    success_count: i64,
    total_reserved_cost: f64,
    total_estimated_cost_usd: f64,
    estimated_cost_rows: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
}

impl MetricBucket {
    fn ingest(&mut self, dispatch: &Value, bundle: &Value, success: bool) {
        self.dispatch_count += 1;
        if success {
            self.success_count += 1;
        }
        self.total_reserved_cost += reserved_cost(dispatch, bundle);
        if let Some(cost) = estimated_cost(dispatch, bundle) {
            self.total_estimated_cost_usd += cost;
            self.estimated_cost_rows += 1;
        }
        self.total_input_tokens += input_tokens(dispatch, bundle).unwrap_or(0);
        self.total_output_tokens += output_tokens(dispatch, bundle).unwrap_or(0);
    }
}

#[derive(Default)]
struct CostOfPassBucket {
    dispatch_count: i64,
    success_count: i64,
    total_cost: f64,
    total_reserved_cost: f64,
    total_estimated_cost_usd: f64,
    estimated_cost_rows: i64,
}

impl CostOfPassBucket {
    fn ingest(&mut self, dispatch: &Value, bundle: &Value, success: bool) {
        self.dispatch_count += 1;
        if success {
            self.success_count += 1;
        }
        let reserved = reserved_cost(dispatch, bundle);
        self.total_reserved_cost += reserved;
        if let Some(estimated) = estimated_cost(dispatch, bundle) {
            self.total_estimated_cost_usd += estimated;
            self.total_cost += estimated;
            self.estimated_cost_rows += 1;
        } else {
            self.total_cost += reserved;
        }
    }
}

fn metric_bucket_entries(buckets: BTreeMap<String, MetricBucket>, key_name: &str) -> Vec<Value> {
    buckets
        .into_iter()
        .map(|(key, bucket)| {
            let mut value = metric_bucket_json(&bucket);
            if let Value::Object(ref mut object) = value {
                object.insert(key_name.to_string(), json!(key));
            }
            value
        })
        .collect()
}

fn metric_bucket_json(bucket: &MetricBucket) -> Value {
    json!({
        "dispatch_count": bucket.dispatch_count,
        "success_count": bucket.success_count,
        "failure_count": (bucket.dispatch_count - bucket.success_count).max(0),
        "success_rate": success_rate(bucket.success_count, bucket.dispatch_count),
        "total_reserved_cost": bucket.total_reserved_cost,
        "total_estimated_cost_usd": bucket.total_estimated_cost_usd,
        "estimated_cost_available": bucket.estimated_cost_rows > 0,
        "estimated_cost_rows": bucket.estimated_cost_rows,
        "total_input_tokens": bucket.total_input_tokens,
        "total_output_tokens": bucket.total_output_tokens,
    })
}

fn shadow_influence_disabled() -> Value {
    json!({
        "selected_tier": false,
        "budget_reservation": false,
        "executor_selection": false,
        "retry_path": false,
        "decision_status": false,
        "routing_mode": false,
    })
}

fn dispatch_history_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let bundle_text: String = row.get(9)?;
    let bundle: Value = serde_json::from_str(&bundle_text).unwrap_or(Value::Null);
    Ok(json!({
        "history_id": row.get::<_, i64>(0)?,
        "dispatch_id": row.get::<_, String>(1)?,
        "created_at": row.get::<_, String>(2)?,
        "raw_request": row.get::<_, String>(3)?,
        "request_source": row.get::<_, String>(4)?,
        "final_status": row.get::<_, String>(5)?,
        "selected_tier": row.get::<_, String>(6)?,
        "risk_level": row.get::<_, String>(7)?,
        "reserved_cost": row.get::<_, f64>(8)?,
        "bundle": bundle,
        "input_tokens": row.get::<_, Option<i64>>(10)?,
        "output_tokens": row.get::<_, Option<i64>>(11)?,
        "estimated_cost_usd": row.get::<_, Option<f64>>(12)?,
        "executor_type": row.get::<_, String>(13)?,
        "latency_ms": row.get::<_, Option<i64>>(14)?,
    }))
}

#[cfg(feature = "pg")]
fn pg_dispatch_history_row(row: &postgres::Row) -> Value {
    let bundle_text: String = row.get(9);
    let bundle: Value = serde_json::from_str(&bundle_text).unwrap_or(Value::Null);
    json!({
        "history_id": row.get::<_, i64>(0),
        "dispatch_id": row.get::<_, String>(1),
        "created_at": row.get::<_, String>(2),
        "raw_request": row.get::<_, String>(3),
        "request_source": row.get::<_, String>(4),
        "final_status": row.get::<_, String>(5),
        "selected_tier": row.get::<_, String>(6),
        "risk_level": row.get::<_, String>(7),
        "reserved_cost": row.get::<_, f64>(8),
        "bundle": bundle,
        "input_tokens": row.get::<_, Option<i64>>(10),
        "output_tokens": row.get::<_, Option<i64>>(11),
        "estimated_cost_usd": row.get::<_, Option<f64>>(12),
        "executor_type": row.get::<_, String>(13),
        "latency_ms": row.get::<_, Option<i64>>(14),
    })
}

#[cfg(feature = "pg")]
fn pg_dispatch_history_rows(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    Ok(rows.iter().map(pg_dispatch_history_row).collect())
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

fn task_class(bundle: &Value) -> String {
    first_str(
        bundle,
        &[
            &["analysis", "task_class"],
            &["analysis", "task_domain"],
            &["decision", "analysis_snapshot", "task_class"],
            &["decision", "analysis_snapshot", "task_domain"],
        ],
    )
    .unwrap_or("unknown")
    .to_string()
}

fn selected_tier(dispatch: &Value, bundle: &Value) -> String {
    dispatch
        .get("selected_tier")
        .and_then(Value::as_str)
        .or_else(|| str_at(bundle, &["decision", "selected_tier"]))
        .unwrap_or("unknown")
        .to_string()
}

fn final_status(dispatch: &Value, bundle: &Value) -> String {
    dispatch
        .get("final_status")
        .and_then(Value::as_str)
        .or_else(|| str_at(bundle, &["record", "final_status"]))
        .unwrap_or("unknown")
        .to_string()
}

fn evaluation_status(bundle: &Value) -> String {
    str_at(bundle, &["evaluation_result", "status"])
        .unwrap_or("unknown")
        .to_string()
}

fn executor_type(dispatch: &Value, bundle: &Value) -> String {
    dispatch
        .get("executor_type")
        .and_then(Value::as_str)
        .or_else(|| str_at(bundle, &["execution_result", "executor_type"]))
        .unwrap_or("unknown")
        .to_string()
}

fn dispatch_success(bundle: &Value, final_status: &str, evaluation_status: &str) -> bool {
    if bundle
        .pointer("/execution_result/success")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }

    let final_status = final_status.to_ascii_lowercase();
    let evaluation_status = evaluation_status.to_ascii_lowercase();

    if matches!(
        final_status.as_str(),
        "failed" | "fail" | "error" | "cancelled" | "timeout" | "timed_out"
    ) || matches!(evaluation_status.as_str(), "failed" | "fail" | "error")
    {
        return false;
    }

    matches!(
        final_status.as_str(),
        "completed" | "success" | "succeeded" | "passed"
    ) || matches!(
        evaluation_status.as_str(),
        "pass" | "passed" | "success" | "succeeded"
    )
}

fn reserved_cost(dispatch: &Value, bundle: &Value) -> f64 {
    dispatch
        .get("reserved_cost")
        .and_then(Value::as_f64)
        .or_else(|| {
            bundle
                .pointer("/decision/budget_reservation/reserved_cost")
                .and_then(Value::as_f64)
        })
        .unwrap_or(0.0)
}

fn estimated_cost(dispatch: &Value, bundle: &Value) -> Option<f64> {
    dispatch
        .get("estimated_cost_usd")
        .and_then(Value::as_f64)
        .or_else(|| {
            bundle
                .pointer("/execution_result/estimated_cost")
                .and_then(Value::as_f64)
        })
}

fn input_tokens(dispatch: &Value, bundle: &Value) -> Option<i64> {
    dispatch
        .get("input_tokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            bundle
                .pointer("/execution_result/input_tokens")
                .and_then(Value::as_i64)
        })
}

fn output_tokens(dispatch: &Value, bundle: &Value) -> Option<i64> {
    dispatch
        .get("output_tokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            bundle
                .pointer("/execution_result/output_tokens")
                .and_then(Value::as_i64)
        })
}

fn success_rate(success_count: i64, dispatch_count: i64) -> f64 {
    if dispatch_count > 0 {
        success_count as f64 / dispatch_count as f64
    } else {
        0.0
    }
}

fn first_str<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths.iter().find_map(|path| str_at(value, path))
}

fn first_f64(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths
        .iter()
        .find_map(|path| value_at(value, path).and_then(Value::as_f64))
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

fn trace_matches(trace: &Value, key: &str, expected: Option<&str>) -> bool {
    let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    trace
        .get(key)
        .and_then(Value::as_str)
        .map(|actual| actual.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn string_filter_matches(actual: &str, expected: Option<&str>) -> bool {
    let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    actual.eq_ignore_ascii_case(expected)
}
