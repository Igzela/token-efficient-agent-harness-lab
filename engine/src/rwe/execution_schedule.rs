//! execution_schedule.v1: deterministic, frozen, replayable RWE cell schedule.
//!
//! Option 2 (accepted planning decision): repetition × budget-point grid,
//! seeds paired to repetitions in pre-registered order (no seed multiplier),
//! sequential execution. The schedule binds the runtime budgets (requests,
//! retries, input/output/total tokens, wall time, provider monetary ceiling)
//! that protocol v1 intentionally does not carry. Replay re-derives the exact
//! schedule from the frozen artifact and compares `schedule_sha256`.

use serde_json::{Map, Value};

use super::corpus::{FirstRweCorpus, RweTaskDefinition};
use super::economic_protocol::FrozenEvidenceDocument;
use super::operator_corpus::operator_corpus_root;

pub const EXECUTION_SCHEDULE_SCHEMA: &str = "execution_schedule.v1";
pub const SCHEDULE_GENERATOR_VERSION: &str = "execution_schedule.v1:repetition-x-budget-point:v1";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), sort_value(&map[&key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

/// Frozen execution schedule with its self-consistency proof.
#[derive(Debug, Clone)]
pub struct FrozenExecutionSchedule {
    pub schema_version: String,
    pub schedule_id: String,
    pub schedule_sha256: String,
    pub body: Value,
}

impl FrozenExecutionSchedule {
    pub fn body_sha256(&self) -> &str {
        &self.schedule_sha256
    }
}

fn required_str(body: &Value, key: &str) -> Result<String, String> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} required"))
}

fn required_u64(body: &Value, key: &str) -> Result<u64, String> {
    body.get(key)
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("{key} must be a positive integer"))
}

fn required_u64_or_zero(body: &Value, key: &str) -> Result<u64, String> {
    body.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an integer"))
}

fn required_array<'a>(body: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    body.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} required array"))
}

/// Load and freeze the operator-approved schedule artifact, verifying every
/// binding (corpus, protocol, cells, seeds, budgets, totals) and recomputing
/// the canonical hash from the artifact body without its stored hash field.
pub fn freeze_operator_execution_schedule(
    corpus: &FirstRweCorpus,
    protocol: &FrozenEvidenceDocument,
) -> Result<FrozenExecutionSchedule, String> {
    freeze_operator_execution_schedule_from_root(
        corpus,
        protocol,
        &operator_corpus_root().join("schedule/execution_schedule.v1.json"),
    )
}

/// Test/verification variant bound to an explicit artifact path so tampering
/// can be exercised provider-free against mutated copies.
pub fn freeze_operator_execution_schedule_from_root(
    corpus: &FirstRweCorpus,
    protocol: &FrozenEvidenceDocument,
    path: &std::path::Path,
) -> Result<FrozenExecutionSchedule, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let body: Value =
        serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    if body.get("schema_version").and_then(Value::as_str) != Some(EXECUTION_SCHEDULE_SCHEMA) {
        return Err("schedule schema_version is not execution_schedule.v1".into());
    }

    // Stored hash is the canonical hash of the body without the hash field.
    let stored_sha = required_str(&body, "schedule_sha256")?;
    let mut hash_input = body.clone();
    if let Value::Object(ref mut m) = hash_input {
        m.remove("schedule_sha256");
    }
    let schedule_sha256 = sha256_hex(sort_value(&hash_input).to_string().as_bytes());
    if schedule_sha256 != stored_sha {
        return Err("schedule_sha256 does not match canonical schedule body".into());
    }

    if required_str(&body, "corpus_id")? != corpus.corpus_id {
        return Err("schedule corpus_id does not match frozen corpus".into());
    }
    if required_str(&body, "corpus_sha256")? != corpus.corpus_sha256 {
        return Err("schedule corpus_sha256 does not match frozen corpus".into());
    }
    if required_str(&body, "protocol_id")?
        != protocol
            .body
            .get("protocol_id")
            .and_then(Value::as_str)
            .unwrap_or("")
    {
        return Err("schedule protocol_id does not match frozen protocol".into());
    }
    if required_str(&body, "protocol_sha256")? != protocol.body_sha256 {
        return Err("schedule protocol_sha256 does not match frozen protocol".into());
    }

    let allocation = body
        .get("allocation")
        .and_then(Value::as_object)
        .ok_or("schedule allocation object required")?;
    let rule = allocation
        .get("rule")
        .and_then(Value::as_str)
        .ok_or("schedule allocation.rule required")?;
    if rule != "repetition_x_budget_point" {
        return Err("schedule allocation rule is not repetition_x_budget_point".into());
    }
    let sequential = allocation
        .get("sequential")
        .and_then(Value::as_bool)
        .ok_or("schedule allocation.sequential required")?;
    if !sequential {
        return Err("schedule allocation.sequential must be true (Minimum First RWE)".into());
    }
    if allocation.get("seed_pairing").and_then(Value::as_str) != Some("pre_registered_order") {
        return Err("schedule seed_pairing must be pre_registered_order".into());
    }
    if allocation.get("generator_version").and_then(Value::as_str)
        != Some(SCHEDULE_GENERATOR_VERSION)
    {
        return Err("schedule generator_version mismatch".into());
    }

    let repetitions = required_u64(&body, "repetitions")?;
    let protocol_repetitions = protocol
        .body
        .get("minimum_repetitions_per_task")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if repetitions != protocol_repetitions {
        return Err("schedule repetitions do not match frozen protocol".into());
    }
    let protocol_seeds: Vec<u64> = protocol
        .body
        .get("seeds")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
        .unwrap_or_default();
    let protocol_budget_points: Vec<String> = protocol
        .body
        .get("budget_points")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    p.get("budget_point_id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if protocol_seeds.len() != repetitions as usize {
        return Err("schedule seed pairing requires one seed per repetition".into());
    }

    let cells = required_array(&body, "cells")?;
    let expected_cells =
        corpus.tasks.len() as u64 * repetitions * protocol_budget_points.len() as u64;
    if cells.len() as u64 != expected_cells {
        return Err(format!(
            "schedule must contain exactly {expected_cells} cells (tasks × repetitions × budget points)"
        ));
    }
    let mut seen_cell_ids = std::collections::HashSet::new();
    let mut seen_orders = std::collections::HashSet::new();
    let mut task_rep_cells: std::collections::HashMap<
        (String, u64),
        std::collections::HashSet<String>,
    > = Default::default();
    let mut task_rep_seed: std::collections::HashMap<(String, u64), u64> = Default::default();
    let mut cell_total_requests = 0_u64;
    let mut cell_total_tokens = 0_u64;
    let mut cell_total_wall_ms = 0_u64;
    let mut cell_total_cost = 0.0_f64;
    for cell in cells {
        let cell_id = required_str(cell, "cell_id")?;
        if !seen_cell_ids.insert(cell_id.clone()) {
            return Err(format!("duplicate cell_id {cell_id}"));
        }
        let task_id = required_str(cell, "task_id")?;
        let task = corpus
            .tasks
            .iter()
            .find(|t| t.task_id == task_id)
            .ok_or_else(|| format!("cell {cell_id} references unknown task {task_id}"))?;
        let repetition = required_u64(cell, "repetition")?;
        if repetition < 1 || repetition > repetitions {
            return Err(format!(
                "cell {cell_id} repetition outside 1..={repetitions}"
            ));
        }
        let point_id = required_str(cell, "budget_point_id")?;
        if !protocol_budget_points.contains(&point_id) {
            return Err(format!(
                "cell {cell_id} references unknown budget point {point_id}"
            ));
        }
        let seed = required_u64(cell, "seed")?;
        if !protocol_seeds.contains(&seed) {
            return Err(format!(
                "cell {cell_id} seed {seed} not in frozen seed list"
            ));
        }
        // Seeds pair to repetitions in pre-registered order (no multiplier):
        // repetition r must use protocol.seeds[r-1].
        if seed != protocol_seeds[(repetition - 1) as usize] {
            return Err(format!(
                "cell {cell_id} seed {seed} does not pair to pre-registered seed for repetition {repetition}"
            ));
        }
        // Same repetition shares one seed across budget points (paired analysis).
        if let Some(previous) = task_rep_seed.get(&(task_id.clone(), repetition)) {
            if *previous != seed {
                return Err(format!(
                    "cell {cell_id} seed differs from the repetition's paired seed"
                ));
            }
        } else {
            task_rep_seed.insert((task_id.clone(), repetition), seed);
        }
        let order = required_u64(cell, "sequential_order")?;
        if !seen_orders.insert(order) {
            return Err(format!("duplicate sequential_order {order}"));
        }
        validate_cell_budget(cell, cell_id.as_str(), task)?;
        cell_total_requests = cell_total_requests
            .checked_add(required_u64(cell, "max_provider_requests")?)
            .ok_or("schedule request total overflow")?;
        cell_total_tokens = cell_total_tokens
            .checked_add(required_u64(cell, "max_total_tokens")?)
            .ok_or("schedule token total overflow")?;
        cell_total_wall_ms = cell_total_wall_ms
            .checked_add(required_u64(cell, "max_wall_time_ms")?)
            .ok_or("schedule wall total overflow")?;
        if let Some(cost) = cell.get("max_cost").and_then(Value::as_f64) {
            if cost <= 0.0 {
                return Err(format!("cell {cell_id} max_cost must be positive"));
            }
            cell_total_cost += cost;
        }
        task_rep_cells
            .entry((task_id.clone(), repetition))
            .or_default()
            .insert(point_id);
    }
    // Every task × repetition must appear exactly once per budget point.
    for task in &corpus.tasks {
        for rep in 1..=repetitions {
            match task_rep_cells.get(&(task.task_id.clone(), rep)) {
                Some(points) if points.len() == protocol_budget_points.len() => {}
                _ => {
                    return Err(format!(
                        "schedule does not cover task {} repetition {rep} for every budget point",
                        task.task_id
                    ))
                }
            }
        }
    }
    // Sequential order must be exactly 1..=N.
    let n = cells.len() as u64;
    for order in 1..=n {
        if !seen_orders.contains(&order) {
            return Err(format!("sequential_order {order} missing"));
        }
    }

    // Run-level budget must equal the cell sums (symbolic, no real spend here).
    let run_level = body
        .get("run_level_budget")
        .and_then(Value::as_object)
        .ok_or("schedule run_level_budget object required")?;
    let run_requests = run_level
        .get("max_total_provider_requests")
        .and_then(Value::as_u64)
        .ok_or("run_level_budget.max_total_provider_requests required")?;
    let run_tokens = run_level
        .get("max_total_tokens")
        .and_then(Value::as_u64)
        .ok_or("run_level_budget.max_total_tokens required")?;
    let run_wall = run_level
        .get("max_wall_time_ms")
        .and_then(Value::as_u64)
        .ok_or("run_level_budget.max_wall_time_ms required")?;
    if run_requests != cell_total_requests
        || run_tokens != cell_total_tokens
        || run_wall != cell_total_wall_ms
    {
        return Err("run_level_budget does not equal the sum of cell budgets".into());
    }
    if let Some(run_cost) = run_level.get("max_total_cost").and_then(Value::as_f64) {
        if (run_cost - cell_total_cost).abs() > f64::EPSILON {
            return Err("run_level_budget.max_total_cost does not equal cell sums".into());
        }
    }

    Ok(FrozenExecutionSchedule {
        schema_version: EXECUTION_SCHEDULE_SCHEMA.into(),
        schedule_id: required_str(&body, "schedule_id")?,
        schedule_sha256,
        body,
    })
}

fn validate_cell_budget(
    cell: &Value,
    cell_id: &str,
    task: &RweTaskDefinition,
) -> Result<(), String> {
    let requests = required_u64(cell, "max_provider_requests")?;
    let input = required_u64(cell, "max_input_tokens")?;
    let output = required_u64(cell, "max_output_tokens")?;
    let total = required_u64(cell, "max_total_tokens")?;
    let wall = required_u64(cell, "max_wall_time_ms")?;
    let retries = required_u64_or_zero(cell, "max_retries")?;
    if requests != task.per_task_max_provider_requests
        || total != task.per_task_max_total_tokens
        || wall != task.timeout_ms
        || input != task.per_task_max_input_tokens
        || output != task.per_task_max_output_tokens
        || input.saturating_add(output) != total
        || retries != task.per_task_max_retries
    {
        return Err(format!(
            "cell {cell_id} budget does not exactly match corpus task {}",
            task.task_id
        ));
    }
    Ok(())
}
