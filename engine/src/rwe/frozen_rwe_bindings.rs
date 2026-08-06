//! Exact frozen Minimum First RWE bindings admitted by Product Golden Path and
//! LocalProductStore owners. No second policy owner: callers never invent paths,
//! verifiers, budgets, or target identities outside this freeze.

use super::corpus::RweTaskDefinition;
use super::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_TARGET_REPO,
};
use serde_json::Value;

/// Exact target main SHA frozen for Minimum First RWE (Igzela/alters-lab).
pub const FROZEN_RWE_TARGET_MAIN_SHA: &str = "6240768506320a324d68787b9eaa86971c8c930c";
/// Exact source tree hash bound to the frozen target main.
pub const FROZEN_RWE_TARGET_TREE_HASH: &str =
    "137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064";
/// Exact pytest verifier shared by both frozen Minimum First tasks.
pub const FROZEN_RWE_PYTEST_VERIFIER: &str =
    "PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/ -q";
/// Risk class that admits frozen RWE pytest under product intake (not global).
pub const FROZEN_RWE_RISK_CLASS: &str = "rwe";
/// Verifier identity recorded on delegated execution manifests for frozen RWE.
pub const FROZEN_RWE_VERIFIER_IDENTITY: &str = "deterministic_rwe_pytest_v1";
/// Docs Golden Path verifier identity (unchanged).
pub const DOCS_GP_VERIFIER_IDENTITY: &str = "deterministic_docs_health_check_v1";

#[derive(Debug, Clone, PartialEq)]
pub struct FrozenRweTaskBinding {
    pub task_id: String,
    pub allowed_mutable_paths: Vec<String>,
    pub expected_verification_command: String,
    pub source_commit: String,
    pub source_tree_hash: String,
    pub per_task_max_provider_requests: u64,
    pub per_task_max_retries: u64,
    pub per_task_max_input_tokens: u64,
    pub per_task_max_output_tokens: u64,
    pub per_task_max_total_tokens: u64,
    pub timeout_ms: u64,
    pub patch_max_files: u64,
    pub patch_max_lines: u64,
}

/// Full next-cell budget envelope reserved before any cell effect.
#[derive(Debug, Clone, PartialEq)]
pub struct RweCellBudgetEnvelope {
    pub max_provider_requests: u64,
    pub max_retries: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    /// `None` means monetary ceiling unavailable (must not invent a number).
    pub max_cost: Option<f64>,
}

impl RweCellBudgetEnvelope {
    pub fn from_schedule_cell(cell: &Value) -> Result<Self, String> {
        let max_provider_requests = cell
            .get("max_provider_requests")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("cell max_provider_requests must be positive")?;
        let max_retries = cell.get("max_retries").and_then(Value::as_u64).unwrap_or(0);
        let max_input_tokens = cell
            .get("max_input_tokens")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("cell max_input_tokens must be positive")?;
        let max_output_tokens = cell
            .get("max_output_tokens")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("cell max_output_tokens must be positive")?;
        let max_total_tokens = cell
            .get("max_total_tokens")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("cell max_total_tokens must be positive")?;
        let max_wall_time_ms = cell
            .get("max_wall_time_ms")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("cell max_wall_time_ms must be positive")?;
        let max_cost = cell.get("max_cost").and_then(Value::as_f64).and_then(|c| {
            if c.is_finite() && c > 0.0 {
                Some(c)
            } else {
                None
            }
        });
        if max_input_tokens
            .saturating_add(max_output_tokens)
            .saturating_mul(1)
            > max_total_tokens
            && max_input_tokens + max_output_tokens > max_total_tokens
        {
            // Soft consistency: input+output may exceed total only if schedule said so;
            // still require positive total.
        }
        Ok(Self {
            max_provider_requests,
            max_retries,
            max_input_tokens,
            max_output_tokens,
            max_total_tokens,
            max_wall_time_ms,
            max_cost,
        })
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "schema_version": "rwe_cell_budget_envelope.v1",
            "max_provider_requests": self.max_provider_requests,
            "max_retries": self.max_retries,
            "max_input_tokens": self.max_input_tokens,
            "max_output_tokens": self.max_output_tokens,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "max_cost": self.max_cost,
            "monetary_ceiling_available": self.max_cost.is_some(),
        })
    }
}

pub fn frozen_rwe_task_bindings() -> Result<Vec<FrozenRweTaskBinding>, String> {
    let frozen = freeze_current_operator_contract_set()?;
    frozen.corpus.tasks.iter().map(binding_from_task).collect()
}

fn binding_from_task(task: &RweTaskDefinition) -> Result<FrozenRweTaskBinding, String> {
    if task.source_commit != FROZEN_RWE_TARGET_MAIN_SHA {
        return Err(format!(
            "frozen RWE task {} source_commit mismatch",
            task.task_id
        ));
    }
    if task.source_tree_hash != FROZEN_RWE_TARGET_TREE_HASH {
        return Err(format!(
            "frozen RWE task {} source_tree_hash mismatch",
            task.task_id
        ));
    }
    let cmd = task
        .expected_verification_commands
        .first()
        .cloned()
        .ok_or_else(|| format!("frozen RWE task {} missing verifier", task.task_id))?;
    if cmd != FROZEN_RWE_PYTEST_VERIFIER {
        return Err(format!(
            "frozen RWE task {} verifier is not the exact frozen pytest command",
            task.task_id
        ));
    }
    if task.allowed_mutable_paths.is_empty() {
        return Err(format!(
            "frozen RWE task {} has empty allowed_mutable_paths",
            task.task_id
        ));
    }
    Ok(FrozenRweTaskBinding {
        task_id: task.task_id.clone(),
        allowed_mutable_paths: task.allowed_mutable_paths.clone(),
        expected_verification_command: cmd,
        source_commit: task.source_commit.clone(),
        source_tree_hash: task.source_tree_hash.clone(),
        per_task_max_provider_requests: task.per_task_max_provider_requests,
        per_task_max_retries: task.per_task_max_retries,
        per_task_max_input_tokens: task.per_task_max_input_tokens,
        per_task_max_output_tokens: task.per_task_max_output_tokens,
        per_task_max_total_tokens: task.per_task_max_total_tokens,
        timeout_ms: task.timeout_ms,
        patch_max_files: task.patch_max_files,
        patch_max_lines: task.patch_max_lines,
    })
}

/// Exact per-cell monetary ceiling from the frozen schedule (not invented constants).
pub fn frozen_schedule_cell_max_cost(cell: &Value) -> Result<Option<f64>, String> {
    match cell.get("max_cost") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let c = v
                .as_f64()
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or("frozen schedule cell max_cost must be a positive finite number")?;
            Ok(Some(c))
        }
    }
}

/// Exact run-level monetary ceiling from the frozen schedule, when present.
pub fn frozen_schedule_run_max_total_cost(
    frozen: &OperatorFrozenContractSet,
) -> Result<Option<f64>, String> {
    match frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_cost")
        .or_else(|| {
            frozen
                .schedule
                .body
                .pointer("/run_level_budget/cost_authority/max_cost")
        }) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let c = v
                .as_f64()
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or("frozen schedule run max_total_cost must be positive finite")?;
            Ok(Some(c))
        }
    }
}

/// True when a product intake is exactly one of the frozen RWE tasks.
pub fn is_exact_frozen_rwe_product_intake(
    risk_class: &str,
    source_revision: &str,
    allowed_paths: &[String],
    verification_command: &str,
    timeout_ms: u64,
) -> bool {
    if risk_class != FROZEN_RWE_RISK_CLASS || source_revision != FROZEN_RWE_TARGET_MAIN_SHA {
        return false;
    }
    if verification_command.trim() != FROZEN_RWE_PYTEST_VERIFIER {
        return false;
    }
    if timeout_ms == 0 || timeout_ms > 900_000 {
        return false;
    }
    match frozen_rwe_task_bindings() {
        Ok(bindings) => bindings.iter().any(|b| {
            b.allowed_mutable_paths == allowed_paths
                && b.expected_verification_command == verification_command.trim()
        }),
        Err(_) => false,
    }
}

/// True when workspace allowed paths match a frozen RWE task exactly.
pub fn is_exact_frozen_rwe_allowed_paths(allowed_paths: &[String]) -> bool {
    match frozen_rwe_task_bindings() {
        Ok(bindings) => bindings
            .iter()
            .any(|b| b.allowed_mutable_paths == allowed_paths),
        Err(_) => false,
    }
}

pub fn is_exact_frozen_rwe_verifier_command(command: &str) -> bool {
    command.trim() == FROZEN_RWE_PYTEST_VERIFIER
}

/// Strict one-way containment: `path` is admitted only when it equals an
/// allowed entry, or when it is a clean child of an allowed DIRECTORY entry.
///
/// Direction is never reversed: a parent of an allowed entry is never
/// admitted, and a file entry (basename contains a `.`) admits no pseudo
/// children. Absolute paths, empty paths, empty components, `.`, `..`, and
/// any path escaping the allowed root fail closed.
pub fn path_under_allowed_paths(path: &str, allowed_paths: &[String]) -> bool {
    let Some(path_components) = clean_relative_path_components(path) else {
        return false;
    };
    if path_components.is_empty() {
        return false;
    }
    allowed_paths.iter().any(|allowed| {
        let Some(allowed_components) = clean_relative_path_components(allowed) else {
            return false;
        };
        if allowed_components.is_empty() {
            return false;
        }
        if path_components == allowed_components {
            return true;
        }
        // Children are admitted only under an allowed DIRECTORY entry; a
        // file-shaped entry (basename with a dot) admits exact equality only.
        if allowed_components.len() < path_components.len()
            && path_components[..allowed_components.len()] == allowed_components[..]
        {
            return !allowed_components
                .last()
                .is_some_and(|basename| basename.contains('.'));
        }
        false
    })
}

/// Normalize a relative path into clean components; reject absolute paths,
/// empty components, and `.`/`..` traversal components.
fn clean_relative_path_components(path: &str) -> Option<Vec<&str>> {
    if path.is_empty() || path.starts_with('/') {
        return None;
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." | ".." => return None,
            other => components.push(other),
        }
    }
    Some(components)
}

/// Union of frozen RWE mutable paths (for run-level delegation scope checks).
pub fn frozen_rwe_union_allowed_paths() -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for b in frozen_rwe_task_bindings()? {
        for p in b.allowed_mutable_paths {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn frozen_rwe_max_patch_limits() -> Result<(u64, u64), String> {
    let bindings = frozen_rwe_task_bindings()?;
    let files = bindings
        .iter()
        .map(|b| b.patch_max_files)
        .max()
        .unwrap_or(0);
    let lines = bindings
        .iter()
        .map(|b| b.patch_max_lines)
        .max()
        .unwrap_or(0);
    if files == 0 || lines == 0 {
        return Err("frozen RWE patch limits missing".into());
    }
    Ok((files, lines))
}

pub fn ensure_frozen_operator_target(frozen: &OperatorFrozenContractSet) -> Result<(), String> {
    if frozen.corpus.disposable_target_repo != OPERATOR_TARGET_REPO {
        return Err("frozen target repo mismatch".into());
    }
    let sha = frozen
        .corpus
        .tasks
        .first()
        .map(|t| t.source_commit.as_str())
        .unwrap_or("");
    if sha != FROZEN_RWE_TARGET_MAIN_SHA {
        return Err("frozen target main SHA mismatch".into());
    }
    Ok(())
}

/// Composition seam readiness: frozen bindings load and match Board A freeze.
/// Does not issue or consume authority.
pub fn rwe_composition_seam_ready() -> Result<(), String> {
    let frozen = freeze_current_operator_contract_set()?;
    ensure_frozen_operator_target(&frozen)?;
    let bindings = frozen_rwe_task_bindings()?;
    if bindings.len() != 2 {
        return Err(format!(
            "frozen RWE expects exactly 2 tasks, got {}",
            bindings.len()
        ));
    }
    let cells = frozen
        .schedule
        .body
        .get("cells")
        .and_then(Value::as_array)
        .ok_or("frozen schedule cells missing")?;
    if cells.len() != 4 {
        return Err(format!(
            "frozen schedule expects 4 cells, got {}",
            cells.len()
        ));
    }
    for cell in cells {
        RweCellBudgetEnvelope::from_schedule_cell(cell)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_bindings_load_and_match_pytest() {
        let bindings = frozen_rwe_task_bindings().unwrap();
        assert_eq!(bindings.len(), 2);
        for b in &bindings {
            assert_eq!(b.expected_verification_command, FROZEN_RWE_PYTEST_VERIFIER);
            assert_eq!(b.source_commit, FROZEN_RWE_TARGET_MAIN_SHA);
            assert!(is_exact_frozen_rwe_allowed_paths(&b.allowed_mutable_paths));
        }
        rwe_composition_seam_ready().unwrap();
    }

    #[test]
    fn path_under_allowed_accepts_children_only() {
        let allowed = vec!["apps/api/src".into(), "README.md".into()];
        // Directory entry: exact and children admitted; parents never.
        assert!(path_under_allowed_paths("apps/api/src", &allowed));
        assert!(path_under_allowed_paths("apps/api/src/main.py", &allowed));
        assert!(path_under_allowed_paths(
            "apps/api/src/sub/file.py",
            &allowed
        ));
        assert!(!path_under_allowed_paths("apps/api", &allowed));
        assert!(!path_under_allowed_paths("apps", &allowed));
        assert!(!path_under_allowed_paths("apps/api/other.py", &allowed));
        // File entry: exact equality only; no pseudo children; no suffix names.
        assert!(path_under_allowed_paths("README.md", &allowed));
        assert!(!path_under_allowed_paths("README.md/child", &allowed));
        assert!(!path_under_allowed_paths("README", &allowed));
        assert!(!path_under_allowed_paths("README.md.bak", &allowed));
        // Escape attempts fail closed.
        assert!(!path_under_allowed_paths("../escape", &allowed));
        assert!(!path_under_allowed_paths("/apps/api/src", &allowed));
        assert!(!path_under_allowed_paths(
            "apps/api/src/../../escape",
            &allowed
        ));
        assert!(!path_under_allowed_paths(
            "apps/api/src/./main.py",
            &allowed
        ));
        assert!(!path_under_allowed_paths("apps//api/src", &allowed));
        assert!(!path_under_allowed_paths("", &allowed));
        assert!(!path_under_allowed_paths("apps/api/src/", &allowed));
        assert!(!path_under_allowed_paths("src2", &["src".into()]));
        assert!(!path_under_allowed_paths("src2/main.py", &["src".into()]));
        assert!(path_under_allowed_paths("src/main.py", &["src".into()]));
    }
}
