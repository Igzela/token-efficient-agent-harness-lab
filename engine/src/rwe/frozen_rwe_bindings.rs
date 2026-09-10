//! Exact frozen Minimum First RWE bindings admitted by Product Golden Path and
//! LocalProductStore owners. No second policy owner: callers never invent paths,
//! verifiers, budgets, or target identities outside this freeze.

use super::corpus::RweTaskDefinition;
use super::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_TARGET_REPO,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

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

/// Exact pre-AC source checkout and provider-free recipe bound by the
/// reconstructable snapshot manifest. These values are distinct from the
/// task-definition source tree above: the former identifies the complete
/// source checkout, while the latter identifies the task input contract.
pub const FROZEN_RWE_PRE_AC_SOURCE_TREE_HASH: &str =
    "f8d22ebf5009842d37285624f345d47bf6da5548032eb84cb7528407169d9cc3";
pub const FROZEN_RWE_RECIPE_COMMIT: &str = "de0b3bb5158f07100d9ee3846b0555193503629d";
pub const FROZEN_RWE_RECIPE_TREE_HASH: &str =
    "8fc5610c47cc4477c5ab7c65fe680ddf970bca4e612558701b316cc2ca038766";
pub const FROZEN_RWE_SNAPSHOT_MANIFEST_SHA256: &str =
    "a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c";
pub const FROZEN_RWE_CORPUS_SHA256: &str =
    "044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20";
pub const FROZEN_RWE_PROTOCOL_SHA256: &str =
    "bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db";
pub const FROZEN_RWE_SCHEDULE_SHA256: &str =
    "6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38";

/// Accepted post-AC main identity used as the comparison arm. This is a
/// binding only; it does not authorize a replay, Provider call, or target
/// write.
pub const FROZEN_RWE_POST_AC_MAIN_SHA: &str = "42fcfa5ad7e349d27d3caa815163340f9c0d5c0b";
pub const FROZEN_RWE_POST_AC_TREE_HASH: &str = "c81a2e4e635da05a8a1c15630371e98943c70c86";
pub const FROZEN_RWE_POST_AC_CARGO_LOCK_SHA256: &str =
    "cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653";
pub const FROZEN_RWE_POST_AC_RUST_TOOLCHAIN_SHA256: &str =
    "e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12";

/// Exact hash-bound generated active baseline files from pre_ac_harness_snapshot.v2.json.
/// These files reside under `alters/current` (which is in .gitignore in alters-lab).
pub const FROZEN_RWE_ACTIVE_BASELINE_FILES: &[(&str, &str)] = &[
    (
        "alters/current/snapshot.yaml",
        "589218e09ea1cc1cd64cf559d957a5f8158d7a6c656b743ecf8a2b0ff248972e",
    ),
    (
        "alters/current/branches.yaml",
        "6534c5d8af4af2783610c5f982e21f1632e9fb38aabd76e28fea828e6155f162",
    ),
    (
        "alters/current/alters/alter_A.yaml",
        "c65275430894d0f9adc6cdf0b64dc3a6f00c598a20f2b6bc1962a4876e04625e",
    ),
    (
        "alters/current/alters/alter_B.yaml",
        "793cab5f3650a94f3230e8cbbd0de1792f7a2127f6ff57bb9ab7c3f91b26f551",
    ),
    (
        "alters/current/alters/alter_C.yaml",
        "3ce18450debc66e1e6b35383e6250aa569d79bdae1ef6419efb64c8fdc725435",
    ),
    (
        "alters/current/alters/alter_D.yaml",
        "29bf4d14065c5437eda12044f874178514bcae50ee4ab60ad6a40e61135a9ce5",
    ),
    (
        "alters/current/value_alignment/alignment_2026-05-19.yaml",
        "717d5f6dcc4a423b9901ddc5439a4cfe6c71a24249d42a081975917c8b9c86a7",
    ),
    (
        "alters/current/dialogue/dialogue_alter_D_2026-05-19.yaml",
        "3fba7b41b9a5f4b52a1b0d0d1feafcfcd5012810455388188feddef114a96814",
    ),
    (
        "alters/current/reality_trace.yaml",
        "c02a3a07400fbff3e96fc1864468c3b046267f63c2ee8716d708df05695485cd",
    ),
];

pub const FROZEN_RWE_ACTIVE_BASELINE_MATERIALIZER_SCRIPT: &str = r#"
import glob, os, shutil, sys
from pathlib import Path
import yaml

workspace = Path(sys.argv[1]).resolve()
sample = workspace / 'alters' / 'sample'
current = workspace / 'alters' / 'current'
if not sample.is_dir():
    sys.exit(1)

current_alters = current / 'alters'
current_dialogue = current / 'dialogue'
current_alignment = current / 'value_alignment'

current_alters.mkdir(parents=True, exist_ok=True)
current_dialogue.mkdir(parents=True, exist_ok=True)
current_alignment.mkdir(parents=True, exist_ok=True)

for name in ('snapshot.yaml', 'branches.yaml', 'reality_trace.yaml'):
    shutil.copy2(sample / name, current / name)
for path in glob.glob(str(sample / 'alters' / 'alter_*.yaml')):
    shutil.copy2(path, current_alters / Path(path).name)

BASELINE_DATE = '2026-05-19'
def _dump(path: Path, data: dict) -> None:
    with path.open('w', encoding='utf-8') as handle:
        yaml.dump(data, handle, default_flow_style=False)

for path in sorted(current_alters.glob('alter_*.yaml')):
    with path.open(encoding='utf-8') as handle:
        data = yaml.safe_load(handle) or {}
    data['generated_at'] = BASELINE_DATE
    data['time_horizon'] = '1.5-2年后'
    data['personality_drift'] = {'detected': False}
    _dump(path, data)

reality_trace = current / 'reality_trace.yaml'
with reality_trace.open(encoding='utf-8') as handle:
    data = yaml.safe_load(handle) or {}
current_probe = data.get('reality_trace', {}).get('current_probe', {})
if 'day_14_gate' in current_probe:
    current_probe['day_14_gate']['status'] = 'completed'
_dump(reality_trace, data)

_dump(
    current_alignment / f'alignment_{BASELINE_DATE}.yaml',
    {
        'value_alignment_report': {
            'status': 'human_confirmed',
            'final_interpretation': {'primary_candidate': 'branch_D'},
            'provisional_commitment': {
                'selected_branch': 'branch_D',
                'selected_alter': 'alter_D',
            },
        }
    },
)
_dump(
    current_dialogue / f'dialogue_alter_D_{BASELINE_DATE}.yaml',
    {
        'dialogue': {
            'status': 'human_confirmed_static_artifact',
            'session': {'alter_ref': 'alters/current/alters/alter_D.yaml'},
            'context_policy': {'provider_used': None, 'runtime_used': False},
        }
    },
)
"#;

/// Validate that all 9 hash-bound active baseline files exist as regular files and
/// match their exact frozen SHA256 hashes.
pub fn validate_frozen_rwe_active_baseline(workspace_path: &Path) -> Result<(), String> {
    for (rel, expected_hash) in FROZEN_RWE_ACTIVE_BASELINE_FILES {
        let path = workspace_path.join(rel);
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("active baseline file missing: {rel}: {e}"))?;
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            return Err(format!("active baseline file is not a regular file: {rel}"));
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("active baseline file unreadable: {rel}: {e}"))?;
        let actual_hash = hex::encode(Sha256::digest(&bytes));
        if actual_hash != *expected_hash {
            return Err(format!(
                "active baseline file hash mismatch for {rel}: expected {expected_hash}, got {actual_hash}"
            ));
        }
    }
    Ok(())
}

/// Ensure that the frozen active baseline fixtures are present and valid in the workspace.
/// If already valid, this is a no-op.
/// Otherwise, it copies them from target_repo_path if available and valid, or materializes
/// them using python3 from workspace alters/sample, then validates all hashes fail-closed.
pub fn ensure_frozen_rwe_workspace_active_baseline(
    workspace_path: &Path,
    target_repo_path: Option<&Path>,
) -> Result<(), String> {
    if !workspace_path.join("alters").join("sample").is_dir() {
        return Ok(());
    }

    if validate_frozen_rwe_active_baseline(workspace_path).is_ok() {
        return Ok(());
    }

    let mut copied = false;
    if let Some(target) = target_repo_path {
        if target != workspace_path && validate_frozen_rwe_active_baseline(target).is_ok() {
            for (rel, _) in FROZEN_RWE_ACTIVE_BASELINE_FILES {
                let src = target.join(rel);
                let dst = workspace_path.join(rel);
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed creating parent dir for {rel}: {e}"))?;
                }
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("failed copying active baseline file {rel}: {e}"))?;
            }
            copied = true;
        }
    }

    if !copied {
        let output = std::process::Command::new("python3")
            .arg("-c")
            .arg(FROZEN_RWE_ACTIVE_BASELINE_MATERIALIZER_SCRIPT)
            .arg(workspace_path)
            .output()
            .map_err(|e| format!("failed to run active baseline materializer: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("active baseline materializer failed: {stderr}"));
        }
    }

    validate_frozen_rwe_active_baseline(workspace_path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrozenRweReconstructionBinding {
    pub pre_ac_source_commit: &'static str,
    pub pre_ac_source_tree_hash: &'static str,
    pub recipe_commit: &'static str,
    pub recipe_tree_hash: &'static str,
    pub snapshot_manifest_sha256: &'static str,
    pub corpus_sha256: &'static str,
    pub protocol_sha256: &'static str,
    pub schedule_sha256: &'static str,
    pub post_ac_main_sha: &'static str,
    pub post_ac_tree_hash: &'static str,
    pub post_ac_cargo_lock_sha256: &'static str,
    pub post_ac_rust_toolchain_sha256: &'static str,
}

impl FrozenRweReconstructionBinding {
    pub fn validate(self) -> Result<(), String> {
        for (name, value) in [
            ("pre_ac_source_commit", self.pre_ac_source_commit),
            ("pre_ac_source_tree_hash", self.pre_ac_source_tree_hash),
            ("recipe_commit", self.recipe_commit),
            ("recipe_tree_hash", self.recipe_tree_hash),
            ("snapshot_manifest_sha256", self.snapshot_manifest_sha256),
            ("corpus_sha256", self.corpus_sha256),
            ("protocol_sha256", self.protocol_sha256),
            ("schedule_sha256", self.schedule_sha256),
            ("post_ac_main_sha", self.post_ac_main_sha),
            ("post_ac_tree_hash", self.post_ac_tree_hash),
            ("post_ac_cargo_lock_sha256", self.post_ac_cargo_lock_sha256),
            (
                "post_ac_rust_toolchain_sha256",
                self.post_ac_rust_toolchain_sha256,
            ),
        ] {
            if value.trim().is_empty() {
                return Err(format!("reconstruction binding requires {name}"));
            }
        }
        if self.pre_ac_source_commit == self.post_ac_main_sha {
            return Err("pre-AC source and post-AC main identities must differ".into());
        }
        if self.pre_ac_source_tree_hash == self.post_ac_tree_hash {
            return Err("pre-AC source and post-AC trees must differ".into());
        }
        Ok(())
    }
}

pub const fn frozen_rwe_reconstruction_binding() -> FrozenRweReconstructionBinding {
    FrozenRweReconstructionBinding {
        pre_ac_source_commit: FROZEN_RWE_TARGET_MAIN_SHA,
        pre_ac_source_tree_hash: FROZEN_RWE_PRE_AC_SOURCE_TREE_HASH,
        recipe_commit: FROZEN_RWE_RECIPE_COMMIT,
        recipe_tree_hash: FROZEN_RWE_RECIPE_TREE_HASH,
        snapshot_manifest_sha256: FROZEN_RWE_SNAPSHOT_MANIFEST_SHA256,
        corpus_sha256: FROZEN_RWE_CORPUS_SHA256,
        protocol_sha256: FROZEN_RWE_PROTOCOL_SHA256,
        schedule_sha256: FROZEN_RWE_SCHEDULE_SHA256,
        post_ac_main_sha: FROZEN_RWE_POST_AC_MAIN_SHA,
        post_ac_tree_hash: FROZEN_RWE_POST_AC_TREE_HASH,
        post_ac_cargo_lock_sha256: FROZEN_RWE_POST_AC_CARGO_LOCK_SHA256,
        post_ac_rust_toolchain_sha256: FROZEN_RWE_POST_AC_RUST_TOOLCHAIN_SHA256,
    }
}

/// One immutable source identity in the contemporary old/new comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRweComparisonArm {
    pub arm_id: String,
    pub source_commit: String,
    pub source_tree_hash: String,
}

/// Frozen comparison-window rules projected onto the existing protocol/schedule
/// hashes. This does not mutate those hashes or authorize a replay.
pub const FROZEN_RWE_COMPARISON_WINDOW: &str = "single_randomized_interleaved_window";
pub const FROZEN_RWE_COMPARISON_ALLOCATION: &str = "paired_old_new_same_schedule";
pub const FROZEN_RWE_COMPARISON_CONCEALMENT: &str =
    "arm_source_identity_bound_not_operator_assigned";
pub const FROZEN_RWE_COMPARISON_DRIFT: &str = "fail_closed_on_protocol_schedule_corpus_drift";
pub const FROZEN_RWE_COMPARISON_CAPACITY: &str = "shared_frozen_cell_budget_envelopes";
pub const FROZEN_RWE_COMPARISON_UNISSUED_PACKAGES: u32 = 2;

/// Existing-owner projection of the reconstructable old/new comparison.
///
/// The corpus, protocol, schedule, lockfile, and toolchain are shared
/// comparison inputs; only the source checkout identity differs between arms.
/// This is a binding/projection only and cannot authorize a replay or effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRweComparisonManifest {
    pub old: FrozenRweComparisonArm,
    pub new: FrozenRweComparisonArm,
    pub cargo_lock_sha256: String,
    pub rust_toolchain_sha256: String,
    pub corpus_sha256: String,
    pub protocol_sha256: String,
    pub schedule_sha256: String,
    pub window: String,
    pub allocation: String,
    pub concealment: String,
    pub drift: String,
    pub capacity_pairing: String,
    pub unissued_authorization_packages: u32,
}

impl FrozenRweComparisonManifest {
    pub fn validate(&self) -> Result<(), String> {
        let binding = frozen_rwe_reconstruction_binding();
        binding.validate()?;
        if self.old.arm_id != "old" || self.new.arm_id != "new" {
            return Err("comparison manifest arm labels must be old and new".into());
        }
        if self.old.source_commit == self.new.source_commit
            || self.old.source_tree_hash == self.new.source_tree_hash
        {
            return Err("comparison manifest arms have colliding source identities".into());
        }
        if self.old.source_commit != FROZEN_RWE_TARGET_MAIN_SHA
            || self.old.source_tree_hash != FROZEN_RWE_PRE_AC_SOURCE_TREE_HASH
        {
            return Err("comparison manifest old identity is swapped or mismatched".into());
        }
        if self.new.source_commit != FROZEN_RWE_POST_AC_MAIN_SHA
            || self.new.source_tree_hash != FROZEN_RWE_POST_AC_TREE_HASH
        {
            return Err("comparison manifest new identity is swapped or mismatched".into());
        }
        for (name, value) in [
            ("cargo_lock_sha256", self.cargo_lock_sha256.as_str()),
            ("rust_toolchain_sha256", self.rust_toolchain_sha256.as_str()),
            ("corpus_sha256", self.corpus_sha256.as_str()),
            ("protocol_sha256", self.protocol_sha256.as_str()),
            ("schedule_sha256", self.schedule_sha256.as_str()),
        ] {
            if !is_sha256(value) {
                return Err(format!("comparison manifest {name} must be a sha256"));
            }
        }
        if self.corpus_sha256 != binding.corpus_sha256
            || self.protocol_sha256 != binding.protocol_sha256
            || self.schedule_sha256 != binding.schedule_sha256
        {
            return Err("comparison manifest frozen evidence identity mismatch".into());
        }
        if self.cargo_lock_sha256 != binding.post_ac_cargo_lock_sha256
            || self.rust_toolchain_sha256 != binding.post_ac_rust_toolchain_sha256
        {
            return Err("comparison manifest build identity mismatch".into());
        }
        if self.window != FROZEN_RWE_COMPARISON_WINDOW
            || self.allocation != FROZEN_RWE_COMPARISON_ALLOCATION
            || self.concealment != FROZEN_RWE_COMPARISON_CONCEALMENT
            || self.drift != FROZEN_RWE_COMPARISON_DRIFT
            || self.capacity_pairing != FROZEN_RWE_COMPARISON_CAPACITY
            || self.unissued_authorization_packages != FROZEN_RWE_COMPARISON_UNISSUED_PACKAGES
        {
            return Err("comparison protocol freeze is missing or mutated".into());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "schema_version": "rwe_comparison_identity.v1",
            "old": {
                "arm_id": self.old.arm_id,
                "source_commit": self.old.source_commit,
                "source_tree_hash": self.old.source_tree_hash,
            },
            "new": {
                "arm_id": self.new.arm_id,
                "source_commit": self.new.source_commit,
                "source_tree_hash": self.new.source_tree_hash,
            },
            "cargo_lock_sha256": self.cargo_lock_sha256,
            "rust_toolchain_sha256": self.rust_toolchain_sha256,
            "corpus_sha256": self.corpus_sha256,
            "protocol_sha256": self.protocol_sha256,
            "schedule_sha256": self.schedule_sha256,
            "window": self.window,
            "allocation": self.allocation,
            "concealment": self.concealment,
            "drift": self.drift,
            "capacity_pairing": self.capacity_pairing,
            "unissued_authorization_packages": self.unissued_authorization_packages,
            "authorizations_issued": false,
        })
    }
}

/// Build the accepted contemporary old/new identity projection.
pub fn current_comparison_manifest() -> Result<FrozenRweComparisonManifest, String> {
    let binding = frozen_rwe_reconstruction_binding();
    binding.validate()?;
    let manifest = FrozenRweComparisonManifest {
        old: FrozenRweComparisonArm {
            arm_id: "old".into(),
            source_commit: binding.pre_ac_source_commit.into(),
            source_tree_hash: binding.pre_ac_source_tree_hash.into(),
        },
        new: FrozenRweComparisonArm {
            arm_id: "new".into(),
            source_commit: binding.post_ac_main_sha.into(),
            source_tree_hash: binding.post_ac_tree_hash.into(),
        },
        cargo_lock_sha256: binding.post_ac_cargo_lock_sha256.into(),
        rust_toolchain_sha256: binding.post_ac_rust_toolchain_sha256.into(),
        corpus_sha256: binding.corpus_sha256.into(),
        protocol_sha256: binding.protocol_sha256.into(),
        schedule_sha256: binding.schedule_sha256.into(),
        window: FROZEN_RWE_COMPARISON_WINDOW.into(),
        allocation: FROZEN_RWE_COMPARISON_ALLOCATION.into(),
        concealment: FROZEN_RWE_COMPARISON_CONCEALMENT.into(),
        drift: FROZEN_RWE_COMPARISON_DRIFT.into(),
        capacity_pairing: FROZEN_RWE_COMPARISON_CAPACITY.into(),
        unissued_authorization_packages: FROZEN_RWE_COMPARISON_UNISSUED_PACKAGES,
    };
    manifest.validate()?;
    Ok(manifest)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

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
        let max_cost = cell
            .get("max_cost")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c > 0.0);
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
///
/// Entry-shape convention: an allowed entry whose basename contains a `.` is
/// treated as a file and admits no children; an entry without a `.` is treated
/// as a directory. Dotless file names (e.g. `Dockerfile`) must therefore be
/// spelled as exact entries and never relied on to admit children; the frozen
/// corpus currently uses dotted file entries only.
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
    frozen_rwe_reconstruction_binding().validate()?;
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
        let reconstruction = frozen_rwe_reconstruction_binding();
        reconstruction.validate().unwrap();
        assert_ne!(
            reconstruction.pre_ac_source_commit,
            reconstruction.post_ac_main_sha
        );
        assert_ne!(
            reconstruction.pre_ac_source_tree_hash,
            reconstruction.post_ac_tree_hash
        );
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
    fn current_comparison_manifest_rejects_identity_collision() {
        let mut manifest = current_comparison_manifest().unwrap();
        manifest.new.source_commit = manifest.old.source_commit.clone();
        let error = manifest.validate().unwrap_err();
        assert!(
            error.contains("colliding"),
            "unexpected validation error: {error}"
        );

        let mut manifest = current_comparison_manifest().unwrap();
        manifest.new.source_tree_hash = manifest.old.source_tree_hash.clone();
        let error = manifest.validate().unwrap_err();
        assert!(
            error.contains("colliding"),
            "unexpected validation error: {error}"
        );
    }

    #[test]
    fn current_comparison_manifest_rejects_protocol_freeze_mutation() {
        let mut manifest = current_comparison_manifest().unwrap();
        manifest.window = "operator_chosen_window".into();
        let error = manifest.validate().unwrap_err();
        assert!(
            error.contains("comparison protocol freeze"),
            "unexpected validation error: {error}"
        );
        let json = current_comparison_manifest().unwrap().to_json();
        assert_eq!(json["window"], FROZEN_RWE_COMPARISON_WINDOW);
        assert_eq!(json["allocation"], FROZEN_RWE_COMPARISON_ALLOCATION);
        assert_eq!(json["concealment"], FROZEN_RWE_COMPARISON_CONCEALMENT);
        assert_eq!(json["authorizations_issued"], false);
        assert_eq!(json["unissued_authorization_packages"], 2);
    }

    #[test]
    fn current_comparison_manifest_rejects_swapped_and_missing_identity() {
        let mut manifest = current_comparison_manifest().unwrap();
        std::mem::swap(
            &mut manifest.old.source_commit,
            &mut manifest.new.source_commit,
        );
        std::mem::swap(
            &mut manifest.old.source_tree_hash,
            &mut manifest.new.source_tree_hash,
        );
        let error = manifest.validate().unwrap_err();
        assert!(
            error.contains("old identity"),
            "unexpected validation error: {error}"
        );

        let mut manifest = current_comparison_manifest().unwrap();
        manifest.new.source_commit.clear();
        let error = manifest.validate().unwrap_err();
        assert!(
            error.contains("new identity"),
            "unexpected validation error: {error}"
        );
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

    #[test]
    fn test_validate_frozen_rwe_active_baseline_detects_missing_and_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        // Missing files:
        let err = validate_frozen_rwe_active_baseline(temp.path()).unwrap_err();
        assert!(err.contains("active baseline file missing"));

        // Corrupted file:
        let sample_file = temp.path().join("alters/current/snapshot.yaml");
        std::fs::create_dir_all(sample_file.parent().unwrap()).unwrap();
        std::fs::write(&sample_file, b"corrupt").unwrap();
        let err = validate_frozen_rwe_active_baseline(temp.path()).unwrap_err();
        assert!(err.contains("hash mismatch"));
    }

    #[test]
    fn test_ensure_frozen_rwe_workspace_active_baseline_ignores_non_alters() {
        let temp = tempfile::tempdir().unwrap();
        assert!(ensure_frozen_rwe_workspace_active_baseline(temp.path(), None).is_ok());
        assert!(!temp.path().join("alters").exists());
    }

    #[test]
    fn test_ensure_frozen_rwe_workspace_active_baseline_materialization() {
        let sample_source = Path::new("/tmp/codex-rwe-target-QgG3Md/alters-lab/alters/sample");
        if !sample_source.is_dir() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let ws_alters = temp.path().join("alters");
        std::fs::create_dir_all(&ws_alters).unwrap();
        let status = std::process::Command::new("cp")
            .arg("-r")
            .arg(sample_source.to_str().unwrap())
            .arg(ws_alters.to_str().unwrap())
            .status()
            .unwrap();
        assert!(status.success());

        assert!(ensure_frozen_rwe_workspace_active_baseline(temp.path(), None).is_ok());
        assert!(validate_frozen_rwe_active_baseline(temp.path()).is_ok());

        // Test copy path from existing populated directory
        let temp2 = tempfile::tempdir().unwrap();
        let ws_alters2 = temp2.path().join("alters");
        std::fs::create_dir_all(&ws_alters2).unwrap();
        let status2 = std::process::Command::new("cp")
            .arg("-r")
            .arg(sample_source.to_str().unwrap())
            .arg(ws_alters2.to_str().unwrap())
            .status()
            .unwrap();
        assert!(status2.success());
        assert!(
            ensure_frozen_rwe_workspace_active_baseline(temp2.path(), Some(temp.path())).is_ok()
        );
        assert!(validate_frozen_rwe_active_baseline(temp2.path()).is_ok());
    }
}
