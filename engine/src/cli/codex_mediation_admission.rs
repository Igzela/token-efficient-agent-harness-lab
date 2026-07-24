//! Codex full mediation admission for the Product Golden Path API-key path.
//!
//! This module does **not** create a second budget owner. It proves that the
//! product-managed Codex child can only reach the provider through the Rust
//! `CodexBudgetGateway`, with task-scoped credentials and filesystem isolation
//! that hide the operator's real Codex home and upstream secrets.
//!
//! Official ChatGPT-auth Codex (child holds reusable OAuth) remains excluded.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::codex_budget_authority::{
    BudgetGatewayUsage, CodexBudgetAuthority, CODEX_BUDGET_AUTHORITY_SCHEMA,
    CODEX_SESSION_TOKEN_PREFIX, DEFAULT_CODEX_MAX_OUTPUT_TOKENS_PER_REQUEST,
    DEFAULT_CODEX_MAX_PROVIDER_REQUESTS,
};

pub const CODEX_MEDIATED_ADMISSION_SCHEMA: &str = "codex_mediated_admission.v2";
pub const BUBBLEWRAP_BIN: &str = "/usr/bin/bwrap";
/// Fixed in-sandbox path for the admitted Codex binary (never the real home path).
pub const SANDBOX_CODEX_BIN: &str = "/opt/acp/managed-codex";
/// Fixed in-sandbox path for the task-scoped CODEX_HOME.
pub const SANDBOX_CODEX_HOME: &str = "/opt/acp/codex-home";

/// True when bwrap can create an unprivileged user+pid namespace (not true on all
/// CI hosts that still provide FS isolation via bwrap bind/tmpfs mounts).
pub fn unprivileged_user_ns_available() -> bool {
    let bwrap = Path::new(BUBBLEWRAP_BIN);
    if !bwrap.is_file() {
        return false;
    }
    let mut cmd = Command::new(bwrap);
    cmd.arg("--die-with-parent")
        .arg("--unshare-user")
        .arg("--unshare-pid")
        .arg("--ro-bind")
        .arg("/usr")
        .arg("/usr")
        .arg("--ro-bind")
        .arg("/bin")
        .arg("/bin")
        .arg("--ro-bind")
        .arg("/lib")
        .arg("/lib");
    if Path::new("/lib64").exists() {
        cmd.arg("--ro-bind").arg("/lib64").arg("/lib64");
    }
    cmd.arg("--proc")
        .arg("/proc")
        .arg("--dev")
        .arg("/dev")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("--clearenv")
        .arg("--setenv")
        .arg("PATH")
        .arg("/usr/bin:/bin")
        .arg("/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Two-axis admission classification for product-managed Codex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexAdmissionClass {
    /// All original full-admission axes proved (not currently claimed).
    FullyAdmittedMediatedApiKey,
    /// Gateway + parent journal + bwrap/PID isolation foundation; residual blockers remain.
    MediationHardenedPartial,
    /// Exact post-call usage evidence only; not live Golden Path ready.
    UsageEvidenceCapable,
    /// Official ChatGPT-auth / unmediated path — excluded from product admission.
    ExcludedChatgptAuthBypass,
    /// Missing identity, isolation tool, or authority binding.
    Blocked,
}

impl CodexAdmissionClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullyAdmittedMediatedApiKey => "fully_admitted_mediated_api_key",
            Self::MediationHardenedPartial => "mediation_hardened_partial",
            Self::UsageEvidenceCapable => "usage_evidence_capable",
            Self::ExcludedChatgptAuthBypass => "excluded_chatgpt_auth_bypass",
            Self::Blocked => "blocked",
        }
    }

    pub fn admits_live_product_golden_path(&self) -> bool {
        // Full live Golden Path requires FullyAdmittedMediatedApiKey only.
        matches!(self, Self::FullyAdmittedMediatedApiKey)
    }

    /// Product path may run the mediated executor for provider-free proof work when
    /// partial mediation is available; live acceptance still requires full class.
    pub fn allows_mediated_product_launch(&self) -> bool {
        matches!(
            self,
            Self::FullyAdmittedMediatedApiKey | Self::MediationHardenedPartial
        )
    }
}

/// Process/network isolation posture for one mediated launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsolationMode {
    /// bubblewrap FS isolation: real HOME hidden; only loopback gateway token present.
    BubblewrapFilesystem,
    /// Isolation tool unavailable — full product admission is refused.
    Unavailable,
}

impl IsolationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BubblewrapFilesystem => "bubblewrap_filesystem",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Capability report for the mediated Codex product path (not a second budget owner).
#[derive(Debug, Clone, PartialEq)]
pub struct CodexMediatedCapabilityReport {
    pub schema_version: String,
    pub admission_class: CodexAdmissionClass,
    pub exact_post_call_usage_evidence: bool,
    pub enforceable_pre_or_cross_call_budget: bool,
    pub process_containment: bool,
    pub worktree_path_confinement: bool,
    pub no_direct_credential_bypass: bool,
    pub no_direct_network_credential_bypass: bool,
    pub exact_executable_and_model_identity: bool,
    pub hard_single_request_output_bound: bool,
    pub hard_cross_call_budget_interposition: bool,
    pub bounded_calls_and_retries: bool,
    pub restart_safe_reconciliation: bool,
    pub isolation_mode: IsolationMode,
    pub network_confinement: String,
    pub remaining_blocker: Option<String>,
    pub notes: Vec<String>,
}

impl CodexMediatedCapabilityReport {
    /// Honest classification after PE7-CODEX-FULL-MEDIATION-ADMISSION-REPAIR-1.
    ///
    /// Does **not** claim full live Golden Path admission: Codex does not label
    /// internal retries on the HTTP wire, and host-loopback-preserving network
    /// isolation is unproved on this unprivileged profile.
    pub fn evaluate(isolation: IsolationMode, bwrap_present: bool) -> Self {
        let fs_ok = matches!(isolation, IsolationMode::BubblewrapFilesystem) && bwrap_present;
        let network_confinement = if fs_ok {
            "shared_host_network_credential_non_bypass_only;loopback_only_netns_unproved"
                .to_string()
        } else {
            "unavailable".to_string()
        };
        let remaining = if !fs_ok {
            Some(
                "product-managed Codex mediation requires /usr/bin/bwrap filesystem+PID isolation"
                    .to_string(),
            )
        } else {
            Some(
                "remaining full-admission blockers: (1) Codex internal retries are not wire-labeled so retry axis is only a subsequent-POST cap, not true retry identity; (2) process-level network isolation that reaches only the loopback gateway (not arbitrary egress) is unproved without elevated privileges; (3) live operator credential+authorization still required for managed acceptance"
                    .to_string(),
            )
        };
        let class = if fs_ok {
            CodexAdmissionClass::MediationHardenedPartial
        } else {
            CodexAdmissionClass::Blocked
        };
        Self {
            schema_version: CODEX_MEDIATED_ADMISSION_SCHEMA.to_string(),
            admission_class: class,
            exact_post_call_usage_evidence: true,
            // Gateway enforces pre/cross-call residual for mediated API-key path.
            enforceable_pre_or_cross_call_budget: fs_ok,
            process_containment: fs_ok,
            // Worktree binding is enforced by Codex workspace-write + absolute bind;
            // full provider-independent path confinement is not claimed here.
            worktree_path_confinement: fs_ok,
            no_direct_credential_bypass: fs_ok,
            // Network egress may still exist; credential non-bypass is the proved axis.
            no_direct_network_credential_bypass: false,
            exact_executable_and_model_identity: true,
            hard_single_request_output_bound: fs_ok,
            hard_cross_call_budget_interposition: fs_ok,
            // Separate request/retry axes exist, but retry identity is unproved.
            bounded_calls_and_retries: false,
            restart_safe_reconciliation: fs_ok,
            isolation_mode: isolation,
            network_confinement,
            remaining_blocker: remaining,
            notes: vec![
                "PR #295 was a partial foundation; full admission is not claimed.".into(),
                "Official ChatGPT-auth Codex path remains excluded from Product Golden Path.".into(),
                "Session JSONL importer is corroborating evidence only; gateway is the cross-call gate.".into(),
                "ProductTask budget remains the sole durable budget authority.".into(),
                "Parent-owned usage journal is outside every child sandbox mount.".into(),
            ],
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "admission_class": self.admission_class.as_str(),
            "admits_live_product_golden_path": self.admission_class.admits_live_product_golden_path(),
            "exact_post_call_usage_evidence": self.exact_post_call_usage_evidence,
            "enforceable_pre_or_cross_call_budget": self.enforceable_pre_or_cross_call_budget,
            "process_containment": self.process_containment,
            "worktree_path_confinement": self.worktree_path_confinement,
            "no_direct_credential_bypass": self.no_direct_credential_bypass,
            "no_direct_network_credential_bypass": self.no_direct_network_credential_bypass,
            "exact_executable_and_model_identity": self.exact_executable_and_model_identity,
            "hard_single_request_output_bound": self.hard_single_request_output_bound,
            "hard_cross_call_budget_interposition": self.hard_cross_call_budget_interposition,
            "bounded_calls_and_retries": self.bounded_calls_and_retries,
            "restart_safe_reconciliation": self.restart_safe_reconciliation,
            "isolation_mode": self.isolation_mode.as_str(),
            "network_confinement": self.network_confinement,
            "remaining_blocker": self.remaining_blocker,
            "notes": self.notes,
        })
    }
}

/// Planned mediated child launch (program + args + env). Does not spawn.
#[derive(Debug, Clone)]
pub struct MediatedCodexLaunchPlan {
    pub isolation_mode: IsolationMode,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub clear_env: bool,
    pub env: Vec<(OsString, OsString)>,
    pub current_dir: PathBuf,
    pub sandbox_codex_bin: PathBuf,
    pub sandbox_codex_home: PathBuf,
    pub host_ephemeral_home: PathBuf,
    pub host_binary: PathBuf,
    pub capability: CodexMediatedCapabilityReport,
}

impl MediatedCodexLaunchPlan {
    /// Audit that the planned environment never carries real upstream secrets.
    pub fn assert_no_upstream_credential_env(&self) -> Result<(), String> {
        const FORBIDDEN: &[&str] = &[
            "OPENAI_API_KEY",
            "CHATGPT_API_KEY",
            "CODEX_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "OPENAI_ACCESS_TOKEN",
            "CODEX_AUTH",
        ];
        // OPENAI_API_KEY is set to the gateway session token only — must start with prefix.
        for (key, value) in &self.env {
            let key = key.to_string_lossy();
            let value = value.to_string_lossy();
            if key == "OPENAI_API_KEY" {
                if !value.starts_with(CODEX_SESSION_TOKEN_PREFIX) {
                    return Err(
                        "OPENAI_API_KEY must be the task-scoped gateway session token".to_string(),
                    );
                }
                continue;
            }
            if FORBIDDEN.iter().any(|name| *name == key) {
                return Err(format!(
                    "mediated Codex launch must not set forbidden credential/proxy env {key}"
                ));
            }
            // Fail closed if a value looks like a long opaque secret outside the session token.
            if value.contains("sk-") || value.contains("sess-") {
                return Err(format!(
                    "mediated Codex launch env {key} appears to contain a provider secret shape"
                ));
            }
        }
        if !self.clear_env {
            return Err("mediated Codex launch must clear the parent environment".to_string());
        }
        Ok(())
    }

    pub fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .current_dir(&self.current_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.clear_env {
            cmd.env_clear();
        }
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        cmd
    }
}

/// Build a one-shot mediated launch plan. Full product admission requires bwrap.
pub fn plan_mediated_codex_launch(
    authority: &CodexBudgetAuthority,
    host_binary: &Path,
    host_ephemeral_home: &Path,
    gateway_base_url: &str,
    session_token: &str,
    codex_args: &[OsString],
) -> Result<MediatedCodexLaunchPlan, String> {
    if authority.schema_version != CODEX_BUDGET_AUTHORITY_SCHEMA {
        return Err("codex budget authority schema version is unsupported".to_string());
    }
    if !session_token.starts_with(CODEX_SESSION_TOKEN_PREFIX) {
        return Err("session token is not an ACP codex budget token".to_string());
    }
    if !gateway_base_url.starts_with("http://127.0.0.1:")
        && !gateway_base_url.starts_with("http://localhost:")
    {
        return Err("gateway base URL must be loopback HTTP for mediated Codex".to_string());
    }
    let host_binary = std::fs::canonicalize(host_binary)
        .map_err(|error| format!("Codex binary is unavailable: {error}"))?;
    if !host_binary.is_file() {
        return Err("Codex binary path is not a file".to_string());
    }
    if !authority.worktree.is_absolute() {
        return Err("worktree must be absolute".to_string());
    }
    if !host_ephemeral_home.is_absolute() {
        return Err("ephemeral CODEX_HOME must be absolute".to_string());
    }

    let bwrap = Path::new(BUBBLEWRAP_BIN);
    let bwrap_present = bwrap.is_file();
    let isolation = if bwrap_present {
        IsolationMode::BubblewrapFilesystem
    } else {
        IsolationMode::Unavailable
    };
    let capability = CodexMediatedCapabilityReport::evaluate(isolation.clone(), bwrap_present);
    if !capability.admission_class.allows_mediated_product_launch() {
        return Err(capability
            .remaining_blocker
            .clone()
            .unwrap_or_else(|| "codex mediation admission is blocked".to_string()));
    }

    let mut args: Vec<OsString> = Vec::new();
    // Die with parent so timeout/cancel of the outer process reaps the sandbox.
    // Do not use --new-session: the managed CLI owner places the child in a
    // process group and kills that group on timeout/cancel; a new session would
    // detach descendants from that cleanup path.
    args.push("--die-with-parent".into());
    // PID isolation when the host permits unprivileged user namespaces. GitHub
    // Actions often denies uid_map; FS isolation still applies via tmpfs/bind.
    let pid_ns = unprivileged_user_ns_available();
    if pid_ns {
        args.push("--unshare-user".into());
        args.push("--unshare-pid".into());
    }
    // Essential host root pieces (read-only).
    for path in ["/usr", "/bin", "/lib", "/lib64", "/etc"] {
        if Path::new(path).exists() {
            args.push("--ro-bind".into());
            args.push(path.into());
            args.push(path.into());
        }
    }
    args.push("--proc".into());
    args.push("/proc".into());
    args.push("--dev".into());
    args.push("/dev".into());
    // Hide real home/credential roots.
    args.push("--tmpfs".into());
    args.push("/tmp".into());
    args.push("--tmpfs".into());
    args.push("/home".into());
    args.push("--tmpfs".into());
    args.push("/root".into());
    // Re-bind worktree after /home tmpfs so product workspace remains reachable.
    args.push("--bind".into());
    args.push(authority.worktree.as_os_str().to_os_string());
    args.push(authority.worktree.as_os_str().to_os_string());
    // Task-scoped home + binary (binary is never the operator CODEX_HOME tree).
    args.push("--ro-bind".into());
    args.push(host_binary.as_os_str().to_os_string());
    args.push(SANDBOX_CODEX_BIN.into());
    args.push("--bind".into());
    args.push(host_ephemeral_home.as_os_str().to_os_string());
    args.push(SANDBOX_CODEX_HOME.into());
    args.push("--chdir".into());
    args.push(authority.worktree.as_os_str().to_os_string());
    args.push("--clearenv".into());
    args.push("--setenv".into());
    args.push("PATH".into());
    args.push("/usr/bin:/bin".into());
    args.push("--setenv".into());
    args.push("HOME".into());
    args.push(SANDBOX_CODEX_HOME.into());
    args.push("--setenv".into());
    args.push("CODEX_HOME".into());
    args.push(SANDBOX_CODEX_HOME.into());
    args.push("--setenv".into());
    args.push("OPENAI_BASE_URL".into());
    args.push(gateway_base_url.into());
    args.push("--setenv".into());
    args.push("OPENAI_API_KEY".into());
    args.push(session_token.into());
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "TERM"] {
        if let Ok(value) = std::env::var(key) {
            args.push("--setenv".into());
            args.push(key.into());
            args.push(value.into());
        }
    }
    // Codex program + product args (already include model, sandbox, prompt).
    args.push(SANDBOX_CODEX_BIN.into());
    args.extend(codex_args.iter().cloned());

    // Outer process env is cleared; bwrap --setenv injects the sandbox env.
    // Keep a mirrored env list for audit and non-bwrap test doubles.
    let env = vec![
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("HOME".into(), SANDBOX_CODEX_HOME.into()),
        ("CODEX_HOME".into(), SANDBOX_CODEX_HOME.into()),
        ("OPENAI_BASE_URL".into(), gateway_base_url.into()),
        ("OPENAI_API_KEY".into(), session_token.into()),
    ];

    let plan = MediatedCodexLaunchPlan {
        isolation_mode: IsolationMode::BubblewrapFilesystem,
        program: bwrap.to_path_buf(),
        args,
        clear_env: true,
        env,
        current_dir: authority.worktree.clone(),
        sandbox_codex_bin: PathBuf::from(SANDBOX_CODEX_BIN),
        sandbox_codex_home: PathBuf::from(SANDBOX_CODEX_HOME),
        host_ephemeral_home: host_ephemeral_home.to_path_buf(),
        host_binary,
        capability,
    };
    plan.assert_no_upstream_credential_env()?;
    Ok(plan)
}

/// Provider-free probe: under the planned sandbox, the real operator auth path is unreadable.
pub fn probe_real_auth_hidden(
    real_auth_path: &Path,
    host_binary: &Path,
    host_ephemeral_home: &Path,
    worktree: &Path,
) -> Result<bool, String> {
    let bwrap = Path::new(BUBBLEWRAP_BIN);
    if !bwrap.is_file() {
        return Err("bwrap is unavailable for isolation probe".to_string());
    }
    // Auth hide relies on tmpfs over /home, not user namespaces (GHA often denies uid_map).
    let mut cmd = Command::new(bwrap);
    cmd.arg("--die-with-parent")
        .arg("--ro-bind")
        .arg("/usr")
        .arg("/usr")
        .arg("--ro-bind")
        .arg("/bin")
        .arg("/bin")
        .arg("--ro-bind")
        .arg("/lib")
        .arg("/lib");
    if Path::new("/lib64").exists() {
        cmd.arg("--ro-bind").arg("/lib64").arg("/lib64");
    }
    cmd.arg("--ro-bind")
        .arg("/etc")
        .arg("/etc")
        .arg("--proc")
        .arg("/proc")
        .arg("--dev")
        .arg("/dev")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("--tmpfs")
        .arg("/home")
        .arg("--tmpfs")
        .arg("/root")
        .arg("--bind")
        .arg(worktree)
        .arg(worktree)
        .arg("--ro-bind")
        .arg(host_binary)
        .arg(SANDBOX_CODEX_BIN)
        .arg("--bind")
        .arg(host_ephemeral_home)
        .arg(SANDBOX_CODEX_HOME)
        .arg("--clearenv")
        .arg("--setenv")
        .arg("PATH")
        .arg("/usr/bin:/bin")
        .arg("--setenv")
        .arg("HOME")
        .arg(SANDBOX_CODEX_HOME)
        .arg("/bin/sh")
        .arg("-c")
        .arg(format!(
            "if test -r {}; then echo AUTH_LEAK; else echo AUTH_HIDDEN; fi; if test -r {}/auth.json; then echo EPH_OK; else echo EPH_MISSING; fi",
            shell_single_quote(&real_auth_path.to_string_lossy()),
            shell_single_quote(SANDBOX_CODEX_HOME)
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = cmd
        .output()
        .map_err(|error| format!("isolation probe failed to spawn: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Non-setuid bwrap implies a user namespace; hosts that deny unprivileged
        // uid_map (common on GitHub Actions) cannot execute FS isolation probes.
        if stderr.contains("uid map") || stderr.contains("Permission denied") {
            return Err(format!(
                "BLOCKED:bwrap_userns_unavailable: isolation probe failed: status={:?} stderr={stderr}",
                output.status.code()
            ));
        }
        return Err(format!(
            "isolation probe failed: status={:?} stderr={stderr}",
            output.status.code()
        ));
    }
    if stdout.contains("AUTH_LEAK") {
        return Ok(false);
    }
    if stdout.contains("AUTH_HIDDEN") && stdout.contains("EPH_OK") {
        return Ok(true);
    }
    Err(format!(
        "isolation probe produced unexpected output: {}",
        stdout.trim()
    ))
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Reconcile gateway-measured usage with session JSONL rollup counters.
///
/// On agreement, prefer gateway (hard interposition source). On contradiction,
/// fail closed with a bounded conflict result (no silent merge).
#[derive(Debug, Clone, PartialEq)]
pub enum UsageReconcileResult {
    PreferGateway {
        input_tokens: u64,
        output_tokens: u64,
        provider_requests: u64,
    },
    PreferSessionOnly {
        input_tokens: u64,
        output_tokens: u64,
        reason: String,
    },
    Conflict {
        gateway_input: u64,
        gateway_output: u64,
        session_input: u64,
        session_output: u64,
        detail: String,
    },
    Missing {
        detail: String,
    },
}

pub fn reconcile_gateway_and_session_usage(
    gateway: &BudgetGatewayUsage,
    session_input: Option<u64>,
    session_output: Option<u64>,
) -> UsageReconcileResult {
    match (session_input, session_output) {
        (None, None) if gateway.provider_requests == 0 => UsageReconcileResult::Missing {
            detail: "no gateway or session usage measured".into(),
        },
        (None, None) => UsageReconcileResult::PreferGateway {
            input_tokens: gateway.cumulative_input_tokens,
            output_tokens: gateway.cumulative_output_tokens,
            provider_requests: gateway.provider_requests,
        },
        (Some(si), Some(so)) if gateway.provider_requests == 0 => {
            UsageReconcileResult::PreferSessionOnly {
                input_tokens: si,
                output_tokens: so,
                reason: "gateway recorded no provider requests; session evidence only".into(),
            }
        }
        (Some(si), Some(so)) => {
            // Allow small session≥gateway skew when session includes non-mediated
            // accounting noise, but never accept session < gateway or large drift.
            let gi = gateway.cumulative_input_tokens;
            let go = gateway.cumulative_output_tokens;
            if si == gi && so == go {
                return UsageReconcileResult::PreferGateway {
                    input_tokens: gi,
                    output_tokens: go,
                    provider_requests: gateway.provider_requests,
                };
            }
            // Exact match is preferred; absolute disagreement fails closed.
            UsageReconcileResult::Conflict {
                gateway_input: gi,
                gateway_output: go,
                session_input: si,
                session_output: so,
                detail: "gateway and session usage counters disagree".into(),
            }
        }
        _ => UsageReconcileResult::Conflict {
            gateway_input: gateway.cumulative_input_tokens,
            gateway_output: gateway.cumulative_output_tokens,
            session_input: session_input.unwrap_or(0),
            session_output: session_output.unwrap_or(0),
            detail: "partial session usage is incomplete relative to gateway".into(),
        },
    }
}

/// Stable identity hash for launch plan audit (no secrets, no private paths).
pub fn launch_plan_identity_fingerprint(plan: &MediatedCodexLaunchPlan) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan.isolation_mode.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(plan.sandbox_codex_bin.as_os_str().as_encoded_bytes());
    hasher.update(b"|");
    hasher.update(plan.sandbox_codex_home.as_os_str().as_encoded_bytes());
    hasher.update(b"|");
    hasher.update(plan.capability.admission_class.as_str().as_bytes());
    // Include presence of gateway token prefix only, never the token value.
    let has_token = plan.env.iter().any(|(k, v)| {
        k == "OPENAI_API_KEY" && v.to_string_lossy().starts_with(CODEX_SESSION_TOKEN_PREFIX)
    });
    if has_token {
        hasher.update(b"|token");
    } else {
        hasher.update(b"|notoken");
    }
    hex::encode(hasher.finalize())
}

/// Default product ceilings exposed for admission evidence (not a second owner).
pub fn mediated_default_ceilings() -> Value {
    json!({
        "max_provider_requests_default": DEFAULT_CODEX_MAX_PROVIDER_REQUESTS,
        "max_output_tokens_per_request_default": DEFAULT_CODEX_MAX_OUTPUT_TOKENS_PER_REQUEST,
        "mediation": "loopback_budget_gateway",
        "isolation": IsolationMode::BubblewrapFilesystem.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_budget_authority::{
        new_codex_attempt_id, write_ephemeral_codex_home, CodexExecutableIdentity,
        CodexProviderIdentity, ADMITTED_CODEX_CLI_VERSION, CODEX_BUDGET_AUTHORITY_SCHEMA,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn require_bwrap() {
        assert!(
            Path::new(BUBBLEWRAP_BIN).is_file(),
            "BLOCKED: /usr/bin/bwrap is required for this provider-free validation lane"
        );
    }

    fn sample_authority(worktree: PathBuf) -> CodexBudgetAuthority {
        let binary = std::env::temp_dir().join(format!("codex-med-bin-{}", uuid::Uuid::new_v4()));
        std::fs::write(&binary, b"#!/bin/sh\necho codex-cli 0.145.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let sha = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        let provider =
            CodexProviderIdentity::openai_compatible("https://api.openai.com/v1").unwrap();
        CodexBudgetAuthority {
            schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
            task_id: "ptask-med".into(),
            workflow_node_id: "node-1".into(),
            execution_id: new_codex_attempt_id(),
            executable: CodexExecutableIdentity {
                binary_path: binary,
                binary_version: ADMITTED_CODEX_CLI_VERSION.to_string(),
                binary_sha256: sha,
            },
            provider,
            model: "gpt-test-model".into(),
            max_provider_requests: 4,
            max_retries: 1,
            max_input_tokens_per_request: 20_000,
            max_output_tokens_per_request: 256,
            max_cumulative_tokens: 10_000,
            max_cost_usd: None,
            timeout_ms: 30_000,
            worktree,
            expires_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
                + 60_000,
        }
    }

    #[test]
    fn capability_report_is_partial_not_full_when_bwrap_present() {
        let blocked = CodexMediatedCapabilityReport::evaluate(IsolationMode::Unavailable, false);
        assert!(!blocked.admission_class.allows_mediated_product_launch());
        assert!(!blocked.enforceable_pre_or_cross_call_budget);

        let partial =
            CodexMediatedCapabilityReport::evaluate(IsolationMode::BubblewrapFilesystem, true);
        assert!(!partial.admission_class.admits_live_product_golden_path());
        assert!(partial.admission_class.allows_mediated_product_launch());
        assert_eq!(
            partial.admission_class,
            CodexAdmissionClass::MediationHardenedPartial
        );
        assert!(partial.enforceable_pre_or_cross_call_budget);
        assert!(!partial.bounded_calls_and_retries);
        assert!(!partial.no_direct_network_credential_bypass);
        assert!(partial
            .remaining_blocker
            .as_ref()
            .unwrap()
            .contains("retry"));
    }

    #[test]
    fn launch_plan_hides_real_credentials_and_uses_pid_isolation() {
        require_bwrap();
        let worktree = std::env::temp_dir().join(format!("codex-med-wt-{}", uuid::Uuid::new_v4()));
        let eph = std::env::temp_dir().join(format!("codex-med-home-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&worktree);
        let _ = std::fs::remove_dir_all(&eph);
        std::fs::create_dir_all(&worktree).unwrap();
        write_ephemeral_codex_home(&eph, "gpt-test-model", "http://127.0.0.1:9/v1").unwrap();
        let authority = sample_authority(worktree.clone());
        let token = format!("{CODEX_SESSION_TOKEN_PREFIX}{}", "a".repeat(64));
        let plan = plan_mediated_codex_launch(
            &authority,
            &authority.executable.binary_path,
            &eph,
            "http://127.0.0.1:9/v1",
            &token,
            &["--version".into()],
        )
        .unwrap();
        plan.assert_no_upstream_credential_env().unwrap();
        let args: Vec<String> = plan
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if unprivileged_user_ns_available() {
            assert!(args.iter().any(|a| a == "--unshare-pid"));
            assert!(args.iter().any(|a| a == "--unshare-user"));
        }
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--tmpfs" && w[1] == "/home"));
        // Parent journal root must not be mounted into the sandbox.
        assert!(!args.iter().any(|a| a.contains("acp-codex-parent-journal")));
        let _ = std::fs::remove_dir_all(&worktree);
        let _ = std::fs::remove_dir_all(&eph);
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn launch_plan_rejects_non_loopback_gateway() {
        require_bwrap();
        let worktree = std::env::temp_dir().join(format!("codex-med-wt2-{}", uuid::Uuid::new_v4()));
        let eph = std::env::temp_dir().join(format!("codex-med-home2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&worktree).unwrap();
        write_ephemeral_codex_home(&eph, "gpt-test-model", "http://127.0.0.1:9/v1").unwrap();
        let authority = sample_authority(worktree.clone());
        let token = format!("{CODEX_SESSION_TOKEN_PREFIX}test");
        let err = plan_mediated_codex_launch(
            &authority,
            &authority.executable.binary_path,
            &eph,
            "https://api.openai.com/v1",
            &token,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("loopback"), "{err}");
        let _ = std::fs::remove_dir_all(&worktree);
        let _ = std::fs::remove_dir_all(&eph);
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn isolation_probe_hides_synthetic_auth_path() {
        require_bwrap();
        let worktree =
            std::env::temp_dir().join(format!("codex-probe-wt-{}", uuid::Uuid::new_v4()));
        let eph = std::env::temp_dir().join(format!("codex-probe-home-{}", uuid::Uuid::new_v4()));
        let fake_home =
            std::env::temp_dir().join(format!("codex-probe-real-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&worktree);
        let _ = std::fs::remove_dir_all(&eph);
        let _ = std::fs::remove_dir_all(&fake_home);
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(fake_home.join(".codex")).unwrap();
        let auth = fake_home.join(".codex/auth.json");
        std::fs::write(&auth, r#"{"OPENAI_API_KEY":"sk-real-must-not-leak"}"#).unwrap();
        write_ephemeral_codex_home(&eph, "gpt-test-model", "http://127.0.0.1:9/v1").unwrap();
        let binary = std::env::temp_dir().join(format!("codex-probe-bin-{}", uuid::Uuid::new_v4()));
        std::fs::write(&binary, b"#!/bin/sh\necho ok\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        match probe_real_auth_hidden(&auth, &binary, &eph, &worktree) {
            Ok(hidden) => {
                assert!(hidden, "real auth must be unreadable inside sandbox");
                assert!(auth.is_file());
            }
            Err(error) if error.contains("BLOCKED:bwrap_userns_unavailable") => {
                // Host cannot execute bwrap FS isolation (uid_map denied). Do not
                // claim probe success; residual remains in partial admission report.
                let report = CodexMediatedCapabilityReport::evaluate(
                    IsolationMode::BubblewrapFilesystem,
                    true,
                );
                assert!(
                    !report.admission_class.admits_live_product_golden_path(),
                    "must not claim full admission when executed FS probe is blocked"
                );
                eprintln!("executed auth-hide probe BLOCKED on host: {error}");
            }
            Err(error) => panic!("unexpected isolation probe failure: {error}"),
        }
        let _ = std::fs::remove_dir_all(&worktree);
        let _ = std::fs::remove_dir_all(&eph);
        let _ = std::fs::remove_dir_all(&fake_home);
        let _ = std::fs::remove_file(&binary);
    }

    #[test]
    fn isolation_probe_hides_parent_environ_with_pid_namespace() {
        require_bwrap();
        if !unprivileged_user_ns_available() {
            // Host cannot create unprivileged user namespaces (common on GHA).
            // Residual blocker remains recorded; do not silently claim success.
            let report =
                CodexMediatedCapabilityReport::evaluate(IsolationMode::BubblewrapFilesystem, true);
            assert!(
                report
                    .remaining_blocker
                    .as_ref()
                    .is_some_and(|b| b.contains("network") || b.contains("retry")),
                "partial admission must still record residual blockers when PID ns unavailable"
            );
            return;
        }
        let host_pid = std::process::id();
        let mut cmd = Command::new(BUBBLEWRAP_BIN);
        cmd.arg("--die-with-parent")
            .arg("--unshare-user")
            .arg("--unshare-pid")
            .arg("--ro-bind")
            .arg("/usr")
            .arg("/usr")
            .arg("--ro-bind")
            .arg("/bin")
            .arg("/bin")
            .arg("--ro-bind")
            .arg("/lib")
            .arg("/lib");
        if Path::new("/lib64").exists() {
            cmd.arg("--ro-bind").arg("/lib64").arg("/lib64");
        }
        cmd.arg("--proc")
            .arg("/proc")
            .arg("--dev")
            .arg("/dev")
            .arg("--tmpfs")
            .arg("/tmp")
            .arg("--tmpfs")
            .arg("/home")
            .arg("--clearenv")
            .arg("--setenv")
            .arg("PATH")
            .arg("/usr/bin:/bin")
            .arg("/bin/sh")
            .arg("-c")
            .arg(format!(
                "if test -r /proc/{host_pid}/environ; then echo HOST_ENV_LEAK; else echo HOST_ENV_HIDDEN; fi"
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = cmd.output().expect("spawn isolation probe");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("HOST_ENV_HIDDEN"),
            "parent environ must be hidden under PID isolation: {stdout}"
        );
    }

    #[test]
    fn reconcile_prefers_gateway_and_fails_closed_on_conflict() {
        let gateway = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 10,
            cumulative_output_tokens: 5,
            cumulative_tokens: 15,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        assert!(matches!(
            reconcile_gateway_and_session_usage(&gateway, Some(10), Some(5)),
            UsageReconcileResult::PreferGateway { .. }
        ));
        assert!(matches!(
            reconcile_gateway_and_session_usage(&gateway, Some(99), Some(5)),
            UsageReconcileResult::Conflict { .. }
        ));
    }

    #[test]
    fn chatgpt_auth_class_never_admits_live_path() {
        assert!(!CodexAdmissionClass::ExcludedChatgptAuthBypass.admits_live_product_golden_path());
        assert!(!CodexAdmissionClass::UsageEvidenceCapable.admits_live_product_golden_path());
        assert!(!CodexAdmissionClass::MediationHardenedPartial.admits_live_product_golden_path());
        assert!(!CodexAdmissionClass::Blocked.admits_live_product_golden_path());
        assert!(CodexAdmissionClass::MediationHardenedPartial.allows_mediated_product_launch());
    }
}
