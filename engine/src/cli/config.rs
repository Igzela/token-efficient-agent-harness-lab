use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

use super::{spawn_with_timeout_with_limits, OutputLimits, SpawnWithTimeoutError};

pub const DEFAULT_CLI_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_CLI_EXECUTION_ENABLED: bool = false;
pub const CLAUDE_VERSION_PROBE_TIMEOUT_MS: u64 = 2_000;
pub const CLAUDE_VERSION_PROBE_OUTPUT_LIMITS: OutputLimits = OutputLimits {
    stdout_bytes: 1_024,
    stderr_bytes: 1_024,
    combined_bytes: 2_048,
};

pub const ADMITTED_CLAUDE_CODE_VERSION: &str = "2.1.217";
pub const ADMITTED_CLAUDE_CODE_MODEL: &str = "claude-haiku-4-5-20251001";
/// Exact product-managed Codex CLI version pin. Do not silently upgrade.
pub const ADMITTED_CODEX_VERSION: &str = "0.145.0";
/// Default exact model pin for product-managed Codex budget mediation.
pub const ADMITTED_CODEX_MODEL: &str = "gpt-5.6-luna";
pub const ADMITTED_CLAUDE_CODE_CONTEXT_TOKENS: u64 = 200_000;
pub const ADMITTED_CLAUDE_CODE_MAX_OUTPUT_TOKENS: u64 = 64_000;
pub const ADMITTED_CLAUDE_CODE_MAX_TURNS: u64 = 3;
pub const ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_TOKENS: u64 = (ADMITTED_CLAUDE_CODE_CONTEXT_TOKENS
    + ADMITTED_CLAUDE_CODE_MAX_OUTPUT_TOKENS)
    * ADMITTED_CLAUDE_CODE_MAX_TURNS;
pub const ADMITTED_CLAUDE_CODE_INPUT_USD_PER_MTOK: f64 = 1.0;
pub const ADMITTED_CLAUDE_CODE_CACHE_WRITE_5M_USD_PER_MTOK: f64 = 1.25;
pub const ADMITTED_CLAUDE_CODE_CACHE_WRITE_1H_USD_PER_MTOK: f64 = 2.0;
pub const ADMITTED_CLAUDE_CODE_CACHE_READ_USD_PER_MTOK: f64 = 0.10;
pub const ADMITTED_CLAUDE_CODE_OUTPUT_USD_PER_MTOK: f64 = 5.0;
pub const ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_COST_USD: f64 = 2.16;
pub const ADMITTED_CLAUDE_CODE_PRICING_SOURCE: &str =
    "https://platform.claude.com/docs/en/build-with-claude/prompt-caching";
pub const ADMITTED_CLAUDE_CODE_PRICING_VERIFIED_AT: &str = "2026-07-22";
const MAX_ADMITTED_CLI_BINARY_BYTES: u64 = 512 * 1024 * 1024;

// Claude Code 2.1.217 exposes permission/settings controls, but this repository
// has not proved a provider-independent worktree-only filesystem boundary for
// the real binary. Keep runtime admission fail-closed until such a mediation
// owner is separately implemented and reviewed. Identity/probe tests remain
// useful contract tests; they are not managed admission evidence.
fn claude_worktree_confinement_proven() -> bool {
    false
}

/// Exact Claude Code runtime admission for the managed product executor.
///
/// Model limits and prices are pinned from Anthropic's model overview:
/// https://platform.claude.com/docs/en/about-claude/models/overview
/// Claude Code's exact-model and bounded-turn flags are documented at:
/// https://code.claude.com/docs/en/cli-usage#cli-flags
///
/// The model may be bound in two ways:
/// - `Some(_)`: the exact admitted model snapshot is passed with `--model` and the
///   CLI response must prove that exact identity.
/// - `None`: no `--model` flag is passed and the admitted CLI resolves its own
///   configured default (for example an operator subscription import through the
///   first-party `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_MODEL`
///   environment). The resolved identity must still be proven from the
///   owner-reported single-entry `modelUsage` evidence and is recorded as
///   `resolved_model`; absent or ambiguous model identity fails closed.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaudeCodeAdmission {
    pub binary_path: PathBuf,
    pub binary_version: String,
    pub binary_sha256: String,
    pub model: Option<String>,
    pub max_turns: u64,
    pub max_budget_usd: f64,
    pub max_attempt_tokens: u64,
    pub context_tokens: u64,
    pub max_output_tokens: u64,
    pub input_usd_per_mtok: f64,
    pub cache_write_5m_usd_per_mtok: f64,
    pub cache_write_1h_usd_per_mtok: f64,
    pub cache_read_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
    pub pricing_source: String,
    pub pricing_verified_at: String,
}

impl ClaudeCodeAdmission {
    /// Model-resolution mode recorded in the managed executor identity.
    pub fn model_resolution(&self) -> &'static str {
        if self.model.is_some() {
            "exact_admitted_pin"
        } else {
            "cli_subscription_default"
        }
    }

    pub fn validate(
        binary_path: &Path,
        expected_version: &str,
        expected_sha256: &str,
        model: Option<&str>,
        max_turns: u64,
        max_budget_usd: f64,
    ) -> Result<Self, String> {
        if !binary_path.is_absolute() {
            return Err("Claude Code binary path must be absolute".to_string());
        }
        let canonical_path = std::fs::canonicalize(binary_path)
            .map_err(|error| format!("Claude Code binary is unavailable: {error}"))?;
        if canonical_path != binary_path {
            return Err(
                "Claude Code binary path must already be canonical with no symlink components"
                    .to_string(),
            );
        }
        let expected_sha256 = expected_sha256.trim().to_ascii_lowercase();
        validate_binary_file_identity(binary_path, &expected_sha256)?;

        let expected_version = expected_version.trim();
        if expected_version != ADMITTED_CLAUDE_CODE_VERSION {
            return Err("Claude Code binary version is not the admitted version".to_string());
        }
        let version_output =
            probe_claude_code_version(binary_path).map_err(|error| error.summary().to_string())?;
        let observed_version = String::from_utf8(version_output.stdout)
            .map_err(|_| "claude_version_probe_malformed: non-UTF-8 output".to_string())?;
        let observed_version = observed_version
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if observed_version != expected_version {
            return Err("claude_version_probe_malformed: version identity mismatch".to_string());
        }
        // Revalidate after the probe as well as before it. The version process
        // must never be allowed to establish identity for a file that changed
        // while it was running.
        let binary_sha256 = validate_binary_file_identity(binary_path, &expected_sha256)?;
        if let Some(model) = model {
            if model != ADMITTED_CLAUDE_CODE_MODEL {
                return Err("Claude Code model is not the exact admitted snapshot".to_string());
            }
        }
        if max_turns != ADMITTED_CLAUDE_CODE_MAX_TURNS {
            return Err(format!(
                "Claude Code managed admission requires max_turns={ADMITTED_CLAUDE_CODE_MAX_TURNS}"
            ));
        }
        if !max_budget_usd.is_finite()
            || (max_budget_usd - ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_COST_USD).abs() > f64::EPSILON
        {
            return Err(format!(
                "Claude Code max budget must equal the ${ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_COST_USD:.2} worst-case admitted attempt"
            ));
        }

        Ok(Self {
            binary_path: binary_path.to_path_buf(),
            binary_version: expected_version.to_string(),
            binary_sha256,
            model: model.map(str::to_string),
            max_turns,
            max_budget_usd,
            max_attempt_tokens: ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_TOKENS,
            context_tokens: ADMITTED_CLAUDE_CODE_CONTEXT_TOKENS,
            max_output_tokens: ADMITTED_CLAUDE_CODE_MAX_OUTPUT_TOKENS,
            input_usd_per_mtok: ADMITTED_CLAUDE_CODE_INPUT_USD_PER_MTOK,
            cache_write_5m_usd_per_mtok: ADMITTED_CLAUDE_CODE_CACHE_WRITE_5M_USD_PER_MTOK,
            cache_write_1h_usd_per_mtok: ADMITTED_CLAUDE_CODE_CACHE_WRITE_1H_USD_PER_MTOK,
            cache_read_usd_per_mtok: ADMITTED_CLAUDE_CODE_CACHE_READ_USD_PER_MTOK,
            output_usd_per_mtok: ADMITTED_CLAUDE_CODE_OUTPUT_USD_PER_MTOK,
            pricing_source: ADMITTED_CLAUDE_CODE_PRICING_SOURCE.to_string(),
            pricing_verified_at: ADMITTED_CLAUDE_CODE_PRICING_VERIFIED_AT.to_string(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ClaudeCodeVersionProbeError {
    Process(SpawnWithTimeoutError),
    NonZeroExit { code: Option<i32> },
    MalformedOutput,
}

impl ClaudeCodeVersionProbeError {
    fn summary(&self) -> &'static str {
        match self {
            Self::Process(error) => error.reason_code(),
            Self::NonZeroExit { .. } => "claude_version_probe_nonzero_exit",
            Self::MalformedOutput => "claude_version_probe_malformed",
        }
    }
}

fn probe_claude_code_version(
    binary_path: &Path,
) -> Result<std::process::Output, ClaudeCodeVersionProbeError> {
    let mut command = Command::new(binary_path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir("/");
    let output = spawn_with_timeout_with_limits(
        &mut command,
        CLAUDE_VERSION_PROBE_TIMEOUT_MS,
        CLAUDE_VERSION_PROBE_OUTPUT_LIMITS,
    )
    .map_err(ClaudeCodeVersionProbeError::Process)?;
    if !output.status.success() {
        return Err(ClaudeCodeVersionProbeError::NonZeroExit {
            code: output.status.code(),
        });
    }
    let output_text = std::str::from_utf8(&output.stdout)
        .map_err(|_| ClaudeCodeVersionProbeError::MalformedOutput)?;
    let version_token = output_text.split_whitespace().next().unwrap_or_default();
    if version_token.is_empty()
        || version_token.split('.').count() != 3
        || version_token.split('.').any(|component| {
            component.is_empty() || !component.chars().all(|value| value.is_ascii_digit())
        })
    {
        return Err(ClaudeCodeVersionProbeError::MalformedOutput);
    }
    Ok(output)
}

pub(crate) fn validate_binary_file_identity(
    binary_path: &Path,
    expected_sha256: &str,
) -> Result<String, String> {
    let canonical_path = std::fs::canonicalize(binary_path)
        .map_err(|error| format!("managed CLI binary is unavailable: {error}"))?;
    if canonical_path != binary_path {
        return Err(
            "managed CLI binary path must already be canonical with no symlink components"
                .to_string(),
        );
    }
    let metadata = std::fs::symlink_metadata(binary_path)
        .map_err(|error| format!("managed CLI binary is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("managed CLI binary must be an exact regular file, not a symlink".to_string());
    }
    if metadata.len() == 0 || metadata.len() > MAX_ADMITTED_CLI_BINARY_BYTES {
        return Err("managed CLI binary size is outside the admitted bound".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("managed CLI binary must be executable".to_string());
        }
    }
    if expected_sha256.len() != 64
        || !expected_sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err("managed CLI binary SHA-256 must be 64 hexadecimal characters".to_string());
    }
    let binary_sha256 = sha256_file(binary_path)?;
    if binary_sha256 != expected_sha256 {
        return Err("managed CLI binary SHA-256 does not match the admitted identity".to_string());
    }
    Ok(binary_sha256)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("managed CLI binary could not be opened: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total_read = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("managed CLI binary could not be hashed: {error}"))?;
        if read == 0 {
            break;
        }
        total_read = total_read.saturating_add(read as u64);
        if total_read > MAX_ADMITTED_CLI_BINARY_BYTES {
            return Err("managed CLI binary size is outside the admitted bound".to_string());
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Exact Codex CLI identity for product-managed budget mediation.
#[derive(Clone, Debug, PartialEq)]
pub struct CodexAdmission {
    pub binary_path: PathBuf,
    pub binary_version: String,
    pub binary_sha256: String,
    pub model: String,
}

impl CodexAdmission {
    pub fn validate(
        binary_path: &Path,
        expected_version: &str,
        expected_sha256: &str,
        model: &str,
    ) -> Result<Self, String> {
        if !binary_path.is_absolute() {
            return Err("Codex binary path must be absolute".to_string());
        }
        let canonical = std::fs::canonicalize(binary_path)
            .map_err(|error| format!("Codex binary is unavailable: {error}"))?;
        if canonical != binary_path {
            return Err(
                "Codex binary path must already be canonical with no symlink components"
                    .to_string(),
            );
        }
        let expected_version = expected_version.trim();
        if expected_version != ADMITTED_CODEX_VERSION {
            return Err(format!(
                "Codex binary version is not the admitted version {ADMITTED_CODEX_VERSION}"
            ));
        }
        let model = model.trim();
        if model.is_empty() {
            return Err("Codex model identity is required".to_string());
        }
        let expected_sha256 = expected_sha256.trim().to_ascii_lowercase();
        let _ = validate_binary_file_identity(binary_path, &expected_sha256)?;
        let version_output = probe_codex_version(binary_path)?;
        if version_output != expected_version {
            return Err(format!(
                "codex version probe mismatch: observed={version_output} expected={expected_version}"
            ));
        }
        // Re-hash after probe so a swapped binary cannot retain the pre-probe identity.
        let binary_sha256 = validate_binary_file_identity(binary_path, &expected_sha256)?;
        Ok(Self {
            binary_path: binary_path.to_path_buf(),
            binary_version: expected_version.to_string(),
            binary_sha256,
            model: model.to_string(),
        })
    }
}

fn probe_codex_version(binary_path: &Path) -> Result<String, String> {
    let mut command = Command::new(binary_path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir("/");
    let output = spawn_with_timeout_with_limits(
        &mut command,
        CLAUDE_VERSION_PROBE_TIMEOUT_MS,
        CLAUDE_VERSION_PROBE_OUTPUT_LIMITS,
    )
    .map_err(|error| error.reason_code().to_string())?;
    if !output.status.success() {
        return Err("codex_version_probe_nonzero_exit".to_string());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "codex_version_probe_malformed: non-UTF-8 output".to_string())?;
    // Observed forms: "codex-cli 0.145.0" or "0.145.0".
    let version = stdout
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .trim()
        .to_string();
    if version.is_empty() {
        return Err("codex_version_probe_malformed: missing version token".to_string());
    }
    Ok(version)
}

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub enabled: bool,
    pub claude_code_bin: Option<String>,
    pub claude_code_enabled: bool,
    pub claude_code_admission: Option<ClaudeCodeAdmission>,
    pub codex_bin: Option<String>,
    pub codex_enabled: bool,
    /// Exact product-managed Codex admission. Required for Golden Path mediation.
    pub codex_admission: Option<CodexAdmission>,
    pub timeout_ms: u64,
}

impl CliConfig {
    pub fn from_env() -> Self {
        let requested = env_bool("ACP_ENABLE_CLI_EXECUTION", DEFAULT_CLI_EXECUTION_ENABLED);
        let kill_switch = env_bool("ACP_CLI_EXECUTION_KILL_SWITCH", false);
        let enabled = requested && !kill_switch;
        let timeout_ms = env_u64("ACP_CLI_TIMEOUT_MS", DEFAULT_CLI_TIMEOUT_MS);

        if requested && kill_switch {
            eprintln!("[acp-cli] CLI execution kill switch is active");
        }

        if !enabled {
            return Self {
                enabled: false,
                claude_code_bin: None,
                claude_code_enabled: false,
                claude_code_admission: None,
                codex_bin: None,
                codex_enabled: false,
                codex_admission: None,
                timeout_ms,
            };
        }

        let claude_requested = env_bool("ACP_ENABLE_CLAUDE_CODE_EXECUTION", false);
        let claude_code_admission = if claude_requested {
            match admit_claude_code_from_env() {
                Ok(admission) => Some(admission),
                Err(error) => {
                    eprintln!("[acp-cli] Claude Code admission refused: {error}");
                    None
                }
            }
        } else {
            None
        };
        let claude_code_bin = claude_code_admission
            .as_ref()
            .and_then(|admission| admission.binary_path.to_str())
            .map(str::to_string);
        let claude_code_enabled = claude_code_admission.is_some();

        let codex_bin = env_opt("ACP_CODEX_BIN").or_else(|| detect_binary("codex"));
        // Presence of a binary enables the generic codex_cli adapter. Product
        // Golden Path mediation additionally requires exact CodexAdmission.
        let codex_admission = match admit_codex_from_env(codex_bin.as_deref()) {
            Ok(Some(admission)) => Some(admission),
            Ok(None) => None,
            Err(error) => {
                eprintln!("[acp-cli] Codex product admission refused: {error}");
                None
            }
        };
        let codex_enabled = codex_bin.is_some();

        if !claude_code_enabled {
            eprintln!("[acp-cli] claude_code_cli executor disabled");
        }
        if !codex_enabled {
            eprintln!("[acp-cli] codex binary not found; codex_cli executor disabled");
        } else if codex_admission.is_none() {
            eprintln!(
                "[acp-cli] codex binary present but product budget mediation is not admitted (set ACP_CODEX_BIN to a canonical file, ACP_CODEX_SHA256, ACP_CODEX_VERSION={ADMITTED_CODEX_VERSION}, ACP_CODEX_MODEL)"
            );
        }

        Self {
            enabled,
            claude_code_bin,
            claude_code_enabled,
            claude_code_admission,
            codex_bin,
            codex_enabled,
            codex_admission,
            timeout_ms,
        }
    }
}

fn admit_codex_from_env(detected_bin: Option<&str>) -> Result<Option<CodexAdmission>, String> {
    // Exact product admission is opt-in via identity env. Without it, the generic
    // codex_cli adapter may still exist for non-product paths, but product apply
    // fails closed at execution/graph compile.
    let Some(sha256) = env_opt("ACP_CODEX_SHA256") else {
        return Ok(None);
    };
    let binary = env_opt("ACP_CODEX_BIN")
        .or_else(|| detected_bin.map(str::to_string))
        .ok_or_else(|| "ACP_CODEX_BIN is required for product Codex admission".to_string())?;
    let version =
        env_opt("ACP_CODEX_VERSION").unwrap_or_else(|| ADMITTED_CODEX_VERSION.to_string());
    let model = env_opt("ACP_CODEX_MODEL").unwrap_or_else(|| ADMITTED_CODEX_MODEL.to_string());
    Ok(Some(CodexAdmission::validate(
        Path::new(&binary),
        &version,
        &sha256,
        &model,
    )?))
}

fn admit_claude_code_from_env() -> Result<ClaudeCodeAdmission, String> {
    if !claude_worktree_confinement_proven() {
        return Err(
            "Claude Code managed admission is disabled: provider-independent worktree-only filesystem confinement is not proven"
                .to_string(),
        );
    }
    let binary_path = env_opt("ACP_CLAUDE_CODE_BIN")
        .map(PathBuf::from)
        .ok_or_else(|| "ACP_CLAUDE_CODE_BIN is required".to_string())?;
    let version = env_opt("ACP_CLAUDE_CODE_VERSION")
        .ok_or_else(|| "ACP_CLAUDE_CODE_VERSION is required".to_string())?;
    let sha256 = env_opt("ACP_CLAUDE_CODE_SHA256")
        .ok_or_else(|| "ACP_CLAUDE_CODE_SHA256 is required".to_string())?;
    // Optional: when unset, the admitted CLI resolves its own configured default
    // model (subscription import) and must prove the resolved identity in its
    // owner-reported usage evidence.
    let model = env_opt("ACP_CLAUDE_MODEL");
    let max_turns = env_required_u64("ACP_CLAUDE_MAX_TURNS")?;
    let max_budget_usd = env_required_f64("ACP_CLAUDE_MAX_BUDGET_USD")?;
    ClaudeCodeAdmission::validate(
        &binary_path,
        &version,
        &sha256,
        model.as_deref(),
        max_turns,
        max_budget_usd,
    )
}

fn detect_binary(name: &str) -> Option<String> {
    let output = Command::new("which").arg(name).output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_required_u64(key: &str) -> Result<u64, String> {
    env_opt(key)
        .ok_or_else(|| format!("{key} is required"))?
        .parse()
        .map_err(|_| format!("{key} must be an unsigned integer"))
}

fn env_required_f64(key: &str) -> Result<f64, String> {
    env_opt(key)
        .ok_or_else(|| format!("{key} is required"))?
        .parse()
        .map_err(|_| format!("{key} must be a number"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputStream;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_claude_admission_env() {
        for key in [
            "ACP_ENABLE_CLI_EXECUTION",
            "ACP_CLI_EXECUTION_KILL_SWITCH",
            "ACP_ENABLE_CLAUDE_CODE_EXECUTION",
            "ACP_CLAUDE_CODE_BIN",
            "ACP_CLAUDE_CODE_VERSION",
            "ACP_CLAUDE_CODE_SHA256",
            "ACP_CLAUDE_MODEL",
            "ACP_CLAUDE_MAX_TURNS",
            "ACP_CLAUDE_MAX_BUDGET_USD",
        ] {
            std::env::remove_var(key);
        }
    }

    #[cfg(unix)]
    fn fake_claude_binary(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let binary = root.join("claude-2.1.217");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\nexit 2\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        binary
    }

    #[cfg(unix)]
    fn fake_version_probe_binary(root: &std::path::Path, version_body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let binary = root.join("claude-version-probe");
        std::fs::write(
            &binary,
            format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then {version_body}; fi\nexit 0\n"),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        binary
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_hang_is_typed_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_version_probe_binary(dir.path(), "sleep 5");
        let error = probe_claude_code_version(&binary).expect_err("probe should time out");
        assert!(matches!(
            error,
            ClaudeCodeVersionProbeError::Process(SpawnWithTimeoutError::TimedOut { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_stdout_flood_is_typed_without_raw_output() {
        let dir = tempfile::tempdir().unwrap();
        let binary =
            fake_version_probe_binary(dir.path(), "dd if=/dev/zero bs=1024 count=2 status=none");
        let error = probe_claude_code_version(&binary).expect_err("probe should reject flood");
        let ClaudeCodeVersionProbeError::Process(SpawnWithTimeoutError::OutputLimitExceeded {
            details,
            ..
        }) = error
        else {
            panic!("expected typed stdout flood");
        };
        assert_eq!(details.stream, OutputStream::Stdout);
        assert!(!details.summary().contains("\0"));
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_stderr_flood_is_typed() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_version_probe_binary(
            dir.path(),
            "dd if=/dev/zero bs=1024 count=2 status=none >&2",
        );
        let error = probe_claude_code_version(&binary).expect_err("probe should reject flood");
        let ClaudeCodeVersionProbeError::Process(SpawnWithTimeoutError::OutputLimitExceeded {
            details,
            ..
        }) = error
        else {
            panic!("expected typed stderr flood");
        };
        assert_eq!(details.stream, OutputStream::Stderr);
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_nonzero_exit_is_typed() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_version_probe_binary(dir.path(), "exit 9");
        let error = probe_claude_code_version(&binary).expect_err("probe should reject exit");
        assert!(matches!(
            error,
            ClaudeCodeVersionProbeError::NonZeroExit { code: Some(9) }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_malformed_version_is_typed() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_version_probe_binary(dir.path(), "printf 'not-a-version\\n'");
        let digest = hex::encode(sha2::Sha256::digest(std::fs::read(&binary).unwrap()));
        let error = ClaudeCodeAdmission::validate(
            &binary,
            ADMITTED_CLAUDE_CODE_VERSION,
            &digest,
            None,
            ADMITTED_CLAUDE_CODE_MAX_TURNS,
            ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_COST_USD,
        )
        .expect_err("malformed version must not be admitted");
        assert!(error.contains("malformed"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn claude_version_probe_uses_cleared_environment_and_eof_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_version_probe_binary(
            dir.path(),
            "if [ -n \"$HOME\" ]; then exit 17; fi; if read value; then exit 18; fi; printf '2.1.217 (Claude Code)\\n'",
        );
        let digest = hex::encode(sha2::Sha256::digest(std::fs::read(&binary).unwrap()));
        let admission = ClaudeCodeAdmission::validate(
            &binary,
            ADMITTED_CLAUDE_CODE_VERSION,
            &digest,
            None,
            ADMITTED_CLAUDE_CODE_MAX_TURNS,
            ADMITTED_CLAUDE_CODE_MAX_ATTEMPT_COST_USD,
        )
        .expect("cleared environment and EOF probe");
        assert_eq!(admission.binary_path, binary);
    }

    #[cfg(not(unix))]
    #[test]
    fn claude_version_probe_is_unavailable_before_process_start_on_unsupported_platform() {
        let binary = PathBuf::from("/not/started/claude");
        assert!(matches!(
            probe_claude_code_version(&binary),
            Err(ClaudeCodeVersionProbeError::Process(
                SpawnWithTimeoutError::ProcessTreeContainmentUnsupported
            ))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn exact_pinned_claude_binary_and_model_are_admitted() {
        use sha2::{Digest, Sha256};

        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));

        let admission = ClaudeCodeAdmission::validate(
            &binary,
            "2.1.217",
            &digest,
            Some("claude-haiku-4-5-20251001"),
            3,
            2.16,
        )
        .expect("exact Claude admission");

        assert_eq!(admission.binary_path, binary);
        assert_eq!(admission.binary_version, "2.1.217");
        assert_eq!(admission.binary_sha256, digest);
        assert_eq!(
            admission.model.as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        assert_eq!(admission.model_resolution(), "exact_admitted_pin");
        assert_eq!(admission.max_turns, 3);
        assert_eq!(admission.max_attempt_tokens, 792_000);
        assert!((admission.max_budget_usd - 2.16).abs() < f64::EPSILON);
        assert_eq!(admission.cache_write_5m_usd_per_mtok, 1.25);
        assert_eq!(admission.cache_write_1h_usd_per_mtok, 2.0);
        assert_eq!(admission.cache_read_usd_per_mtok, 0.10);
        assert_eq!(
            admission.pricing_source,
            ADMITTED_CLAUDE_CODE_PRICING_SOURCE
        );
        assert_eq!(
            admission.pricing_verified_at,
            ADMITTED_CLAUDE_CODE_PRICING_VERIFIED_AT
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_admission_rejects_changed_model_turn_or_cost_contract() {
        use sha2::{Digest, Sha256};

        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        let validate = |model: Option<&str>, max_turns: u64, max_budget_usd: f64| {
            ClaudeCodeAdmission::validate(
                &binary,
                "2.1.217",
                &digest,
                model,
                max_turns,
                max_budget_usd,
            )
        };

        assert!(validate(Some("haiku"), 3, 2.16).is_err());
        assert!(validate(Some(ADMITTED_CLAUDE_CODE_MODEL), 2, 2.16).is_err());
        assert!(validate(Some(ADMITTED_CLAUDE_CODE_MODEL), 3, 0.72).is_err());
        assert!(ClaudeCodeAdmission::validate(
            &binary,
            "2.1.218",
            &digest,
            Some(ADMITTED_CLAUDE_CODE_MODEL),
            3,
            2.16,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unpinned_claude_model_is_admitted_in_subscription_default_mode() {
        use sha2::{Digest, Sha256};

        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));

        let admission = ClaudeCodeAdmission::validate(&binary, "2.1.217", &digest, None, 3, 2.16)
            .expect("subscription-default Claude admission");

        assert_eq!(admission.model, None);
        assert_eq!(admission.model_resolution(), "cli_subscription_default");
        assert_eq!(admission.max_turns, 3);
        assert!((admission.max_budget_usd - 2.16).abs() < f64::EPSILON);
    }

    #[cfg(unix)]
    #[test]
    fn claude_admission_rejects_symlinked_binary_identity() {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let linked = dir.path().join("claude-linked");
        symlink(&binary, &linked).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));

        let error = ClaudeCodeAdmission::validate(
            &linked,
            "2.1.217",
            &digest,
            Some(ADMITTED_CLAUDE_CODE_MODEL),
            3,
            2.16,
        )
        .unwrap_err();

        assert!(error.contains("canonical"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn cli_config_keeps_claude_disabled_without_filesystem_confinement() {
        use sha2::{Digest, Sha256};

        let _guard = env_lock().lock().unwrap();
        clear_claude_admission_env();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "1");
        std::env::set_var("ACP_ENABLE_CLAUDE_CODE_EXECUTION", "1");
        std::env::set_var("ACP_CLAUDE_CODE_BIN", &binary);
        std::env::set_var("ACP_CLAUDE_CODE_VERSION", "2.1.217");
        std::env::set_var("ACP_CLAUDE_CODE_SHA256", &digest);
        std::env::set_var("ACP_CLAUDE_MODEL", ADMITTED_CLAUDE_CODE_MODEL);
        std::env::set_var("ACP_CLAUDE_MAX_TURNS", "3");
        std::env::set_var("ACP_CLAUDE_MAX_BUDGET_USD", "2.16");

        let config = CliConfig::from_env();

        assert!(config.enabled);
        assert!(!config.claude_code_enabled);
        assert!(config.claude_code_admission.is_none());
        assert!(config.claude_code_bin.is_none());
        clear_claude_admission_env();
    }

    #[cfg(unix)]
    #[test]
    fn cli_config_keeps_subscription_default_claude_disabled_without_confinement() {
        use sha2::{Digest, Sha256};

        let _guard = env_lock().lock().unwrap();
        clear_claude_admission_env();
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_claude_binary(dir.path());
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "1");
        std::env::set_var("ACP_ENABLE_CLAUDE_CODE_EXECUTION", "1");
        std::env::set_var("ACP_CLAUDE_CODE_BIN", &binary);
        std::env::set_var("ACP_CLAUDE_CODE_VERSION", "2.1.217");
        std::env::set_var("ACP_CLAUDE_CODE_SHA256", &digest);
        std::env::set_var("ACP_CLAUDE_MAX_TURNS", "3");
        std::env::set_var("ACP_CLAUDE_MAX_BUDGET_USD", "2.16");

        let config = CliConfig::from_env();

        assert!(config.enabled);
        assert!(!config.claude_code_enabled);
        assert!(config.claude_code_admission.is_none());
        clear_claude_admission_env();
    }

    #[test]
    fn test_cli_config_defaults() {
        let config = CliConfig::from_env();
        assert!(config.timeout_ms > 0);
    }

    #[test]
    fn test_cli_execution_defaults_off() {
        const { assert!(!DEFAULT_CLI_EXECUTION_ENABLED) };
        assert!(!env_bool(
            "NONEXISTENT_KEY_DEFAULT_FALSE",
            DEFAULT_CLI_EXECUTION_ENABLED
        ));
    }

    #[test]
    fn test_detect_binary_found() {
        let result = detect_binary("sh");
        assert!(
            result.is_some(),
            "sh binary should be detected on any Unix system"
        );
    }

    #[test]
    fn test_detect_binary_nonexistent() {
        let result = detect_binary("nonexistent_binary_xyz_12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_env_bool_parsing() {
        assert!(env_bool("NONEXISTENT_KEY_DEFAULT_TRUE", true));
        assert!(!env_bool("NONEXISTENT_KEY_DEFAULT_FALSE", false));
    }
}
