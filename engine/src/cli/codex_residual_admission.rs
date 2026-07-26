//! PE7-CODEX-RESIDUAL-ADMISSION-CLOSURE-1
//!
//! Provider-free investigation of the remaining full-admission blockers for
//! product-managed Codex (API-key mediated path, CLI 0.145.0):
//!
//! 1. True Codex internal retry identity on the HTTP wire
//! 2. Enforceable loopback-only network confinement to the parent gateway
//! 3. Host-dependent unprivileged user-namespace and PID isolation
//!
//! This module does **not** create a second budget, runtime, store, or
//! admission authority. It records typed evidence and an honest final
//! classification. Full admission is claimed only when every axis is proved.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::codex_mediation_admission::{
    unprivileged_user_ns_available, CodexAdmissionClass, BUBBLEWRAP_BIN,
};
use super::config::ADMITTED_CODEX_VERSION;

/// Versioned residual-admission finding contract.
pub const CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA: &str = "codex_residual_admission_finding.v1";

/// Final packet classification (provider-free).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidualAdmissionVerdict {
    /// Every residual axis closed with executed proof. Not currently claimed.
    FullProviderFreeMediationAdmission,
    /// One or more residual axes remain; full admission is refused.
    ResidualAdmissionNoGo,
}

impl ResidualAdmissionVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullProviderFreeMediationAdmission => "full_provider_free_mediation_admission",
            Self::ResidualAdmissionNoGo => "residual_admission_no_go",
        }
    }
}

/// Typed host/process capability evidence (never a silent downgrade).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityEvidenceClass {
    /// Executed probe succeeded and the property is enforced or enforceable.
    Proved,
    /// Capability is missing and the path fails closed (no silent weaker mode).
    UnavailableFailClosed,
    /// Platform/tooling does not support the capability.
    Unsupported,
    /// Probe could not determine the outcome.
    OutcomeUnknown,
}

impl CapabilityEvidenceClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proved => "proved",
            Self::UnavailableFailClosed => "unavailable_and_fail_closed",
            Self::Unsupported => "unsupported",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub fn is_proved(&self) -> bool {
        matches!(self, Self::Proved)
    }
}

/// Retry-axis investigation result for admitted Codex CLI 0.145.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryIdentityFinding {
    pub admitted_cli_version: String,
    pub classification: CapabilityEvidenceClass,
    /// True only when an internal retry is wire-distinguishable from a new
    /// logical request / transport replay / resumed stream / second tool round.
    pub enforceable_retry_identity: bool,
    pub reason: String,
    pub non_claims: Vec<String>,
    pub evidence_notes: Vec<String>,
}

/// Network-confinement investigation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfinementFinding {
    pub classification: CapabilityEvidenceClass,
    /// True only when the child can reach the admitted gateway and cannot reach
    /// arbitrary external TCP under the supported host profile.
    pub loopback_only_enforced: bool,
    pub unshare_net_available: CapabilityEvidenceClass,
    pub unix_socket_bridge_feasible: CapabilityEvidenceClass,
    pub external_egress_blocked_under_unshare_net: CapabilityEvidenceClass,
    pub host_loopback_tcp_isolated_under_unshare_net: CapabilityEvidenceClass,
    pub product_launch_enforces_loopback_only: bool,
    pub reason: String,
    pub design_notes: Vec<String>,
    pub non_claims: Vec<String>,
}

/// User + PID namespace investigation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPidNamespaceFinding {
    pub classification: CapabilityEvidenceClass,
    pub unprivileged_user_ns: CapabilityEvidenceClass,
    pub pid_namespace_with_user_ns: CapabilityEvidenceClass,
    pub reason: String,
    pub non_claims: Vec<String>,
}

/// Aggregate residual-admission finding for one evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidualAdmissionFinding {
    pub schema_version: String,
    pub verdict: ResidualAdmissionVerdict,
    pub product_admission_class: String,
    pub retry: RetryIdentityFinding,
    pub network: NetworkConfinementFinding,
    pub user_pid_ns: UserPidNamespaceFinding,
    pub remaining_blockers: Vec<String>,
    pub closed_axes: Vec<String>,
    pub notes: Vec<String>,
}

impl ResidualAdmissionFinding {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "verdict": self.verdict.as_str(),
            "product_admission_class": self.product_admission_class,
            "retry_identity": {
                "admitted_cli_version": self.retry.admitted_cli_version,
                "classification": self.retry.classification.as_str(),
                "enforceable_retry_identity": self.retry.enforceable_retry_identity,
                "reason": self.retry.reason,
                "non_claims": self.retry.non_claims,
                "evidence_notes": self.retry.evidence_notes,
            },
            "network_confinement": {
                "classification": self.network.classification.as_str(),
                "loopback_only_enforced": self.network.loopback_only_enforced,
                "unshare_net_available": self.network.unshare_net_available.as_str(),
                "unix_socket_bridge_feasible": self.network.unix_socket_bridge_feasible.as_str(),
                "external_egress_blocked_under_unshare_net":
                    self.network.external_egress_blocked_under_unshare_net.as_str(),
                "host_loopback_tcp_isolated_under_unshare_net":
                    self.network.host_loopback_tcp_isolated_under_unshare_net.as_str(),
                "product_launch_enforces_loopback_only":
                    self.network.product_launch_enforces_loopback_only,
                "reason": self.network.reason,
                "design_notes": self.network.design_notes,
                "non_claims": self.network.non_claims,
            },
            "user_pid_namespace": {
                "classification": self.user_pid_ns.classification.as_str(),
                "unprivileged_user_ns": self.user_pid_ns.unprivileged_user_ns.as_str(),
                "pid_namespace_with_user_ns":
                    self.user_pid_ns.pid_namespace_with_user_ns.as_str(),
                "reason": self.user_pid_ns.reason,
                "non_claims": self.user_pid_ns.non_claims,
            },
            "remaining_blockers": self.remaining_blockers,
            "closed_axes": self.closed_axes,
            "notes": self.notes,
        })
    }
}

/// Investigate Codex 0.145.0 retry identity without live provider calls.
///
/// A trustworthy retry identity must be present on the mediated gateway wire
/// (headers or body fields with documented semantics) such that an internal
/// HTTP retry is distinguishable from a new logical provider request, a
/// transport replay, a resumed stream, or a second tool/model round.
///
/// Heuristics based on timing, body similarity, HTTP status alone, or
/// undocumented inference are rejected by policy.
pub fn investigate_retry_identity() -> RetryIdentityFinding {
    // Evidence sources inspected for this packet (provider-free):
    // - admitted version pin ADMITTED_CODEX_VERSION = 0.145.0
    // - existing gateway policy: max_retries counts subsequent POSTs only
    //   (codex_budget_authority: "codex_internal_retries_not_wire_labeled")
    // - binary string inventory of the installed standalone Codex 0.145.0 image
    //   (when present): internal modules `codex-client/src/retry.rs`,
    //   `core/src/responses_retry.rs`; client headers such as
    //   `x-client-request-id` / response `x-oai-request-id` / body field
    //   `previous_response_id` / `idempotencyKey` exist for correlation or
    //   conversation continuity, not as a documented "this POST is retry of
    //   prior POST N" mediation contract enforced by the gateway.
    //
    // None of those fields is an enforceable retry-axis identity that separates:
    //   new logical request | transport replay | resumed stream | tool/model round.
    let non_claims = vec![
        "Do not treat unique x-client-request-id values as retry linkage (new id ≠ retry-of).".into(),
        "Do not treat previous_response_id as HTTP retry identity (conversation continuity).".into(),
        "Do not treat idempotencyKey presence alone as retry detection without a proved reuse contract.".into(),
        "Do not infer retries from timing, repeated bodies, token similarity, or HTTP status alone.".into(),
        "Do not claim max_retries enforcement is true internal-retry identity; it is a subsequent-POST cap.".into(),
    ];
    let evidence_notes = vec![
        format!("admitted_cli_version={ADMITTED_CODEX_VERSION}"),
        "gateway_retry_axis_note=codex_internal_retries_not_wire_labeled".into(),
        "no_documented_wire_field_marks_internal_http_retry_for_mediation".into(),
        "binary_contains_internal_retry_modules_without_gateway_visible_retry_of_linkage".into(),
    ];
    RetryIdentityFinding {
        admitted_cli_version: ADMITTED_CODEX_VERSION.to_string(),
        classification: CapabilityEvidenceClass::UnavailableFailClosed,
        enforceable_retry_identity: false,
        reason: format!(
            "Codex CLI {ADMITTED_CODEX_VERSION} does not expose a trustworthy, \
             gateway-visible identity that distinguishes an internal HTTP retry \
             from a new logical provider request, transport replay, resumed \
             stream, or second tool/model round. Residual NO-GO for true retry \
             accounting; keep max_retries as a subsequent-POST ceiling only."
        ),
        non_claims,
        evidence_notes,
    }
}

fn bwrap_present() -> bool {
    Path::new(BUBBLEWRAP_BIN).is_file()
}

/// Probe whether bwrap can create a network namespace (no external routes).
pub fn probe_unshare_net_available() -> CapabilityEvidenceClass {
    if !bwrap_present() {
        return CapabilityEvidenceClass::Unsupported;
    }
    let mut cmd = Command::new(BUBBLEWRAP_BIN);
    cmd.arg("--die-with-parent")
        .arg("--unshare-net")
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
        .stderr(Stdio::piped());
    match cmd.output() {
        Ok(output) if output.status.success() => CapabilityEvidenceClass::Proved,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("uid map")
                || stderr.contains("Permission denied")
                || stderr.contains("Operation not permitted")
            {
                CapabilityEvidenceClass::UnavailableFailClosed
            } else {
                CapabilityEvidenceClass::OutcomeUnknown
            }
        }
        Err(_) => CapabilityEvidenceClass::OutcomeUnknown,
    }
}

/// Executed probe: under `--unshare-net`, external TCP fails and a parent Unix
/// gateway socket mounted into the sandbox remains reachable. Host loopback TCP
/// (parent TCP gateway) is not reachable from the netns.
///
/// This proves a *feasible unprivileged design* (unix bridge + unshare-net). It
/// does **not** by itself claim the product launch path currently enforces it.
pub fn probe_loopback_only_unix_bridge_design() -> Result<NetworkBridgeProbeResult, String> {
    if !bwrap_present() {
        return Err("bwrap unavailable".into());
    }
    if !matches!(
        probe_unshare_net_available(),
        CapabilityEvidenceClass::Proved
    ) {
        return Err("unshare-net unavailable on this host".into());
    }

    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let sock_path = dir.path().join("gateway.sock");
    let _ = fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).map_err(|e| format!("unix bind failed: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("unix nonblocking: {e}"))?;

    let accept_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let accept_flag_thread = std::sync::Arc::clone(&accept_flag);
    let accept_handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 256];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
                    );
                    accept_flag_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });

    // Child: unshare-net — external blocked; host TCP loopback isolated; unix ok.
    let script = format!(
        r#"python3 - <<'PY'
import socket
path = {sock_path:?}
# external
try:
    s = socket.socket(); s.settimeout(1.0); s.connect(("1.1.1.1", 443)); print("EXTERNAL_OK")
except Exception as e:
    print("EXTERNAL_BLOCKED", type(e).__name__)
# host loopback TCP (should not reach parent services in a fresh netns)
try:
    s = socket.socket(); s.settimeout(1.0); s.connect(("127.0.0.1", 9)); print("HOST_LOOPBACK_TCP_OK")
except Exception as e:
    print("HOST_LOOPBACK_TCP_BLOCKED", type(e).__name__)
# parent unix gateway
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.settimeout(2.0)
    s.connect(path)
    s.sendall(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    data = s.recv(64)
    print("UNIX_GATEWAY_OK" if data.startswith(b"HTTP/1.1 200") else "UNIX_GATEWAY_BAD")
except Exception as e:
    print("UNIX_GATEWAY_FAIL", type(e).__name__, e)
PY"#,
        sock_path = sock_path
    );

    let mut cmd = Command::new(BUBBLEWRAP_BIN);
    cmd.arg("--die-with-parent")
        .arg("--unshare-net")
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
        .arg("--ro-bind")
        .arg(&sock_path)
        .arg(&sock_path)
        .arg("--clearenv")
        .arg("--setenv")
        .arg("PATH")
        .arg("/usr/bin:/bin")
        .arg("/bin/sh")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| format!("bridge probe spawn failed: {e}"))?;
    let _ = accept_handle.join();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        if stderr.contains("uid map") || stderr.contains("Permission denied") {
            return Err(format!("BLOCKED:bwrap_userns_unavailable: {stderr}"));
        }
        return Err(format!(
            "bridge probe failed: status={:?} stderr={stderr} stdout={stdout}",
            output.status.code()
        ));
    }

    let external_blocked = stdout.contains("EXTERNAL_BLOCKED");
    let host_loopback_isolated = stdout.contains("HOST_LOOPBACK_TCP_BLOCKED");
    let unix_ok =
        stdout.contains("UNIX_GATEWAY_OK") && accept_flag.load(std::sync::atomic::Ordering::SeqCst);

    Ok(NetworkBridgeProbeResult {
        external_egress_blocked: external_blocked,
        host_loopback_tcp_isolated: host_loopback_isolated,
        unix_gateway_reachable: unix_ok,
        raw_stdout_summary: summarize_probe_stdout(&stdout),
    })
}

fn summarize_probe_stdout(stdout: &str) -> String {
    // Redact paths; keep only classification tokens.
    stdout
        .lines()
        .filter(|line| {
            line.contains("EXTERNAL_")
                || line.contains("HOST_LOOPBACK_")
                || line.contains("UNIX_GATEWAY_")
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkBridgeProbeResult {
    pub external_egress_blocked: bool,
    pub host_loopback_tcp_isolated: bool,
    pub unix_gateway_reachable: bool,
    pub raw_stdout_summary: String,
}

impl NetworkBridgeProbeResult {
    pub fn design_feasible(&self) -> bool {
        self.external_egress_blocked
            && self.host_loopback_tcp_isolated
            && self.unix_gateway_reachable
    }
}

/// Investigate loopback-only network confinement under the supported host profile.
///
/// Product launch currently shares the host network and relies on credential
/// non-bypass (session token only). A feasible unprivileged design exists
/// (`bwrap --unshare-net` + parent Unix gateway socket + optional in-sandbox
/// TCP→Unix bridge for OPENAI_BASE_URL). Closing the product residual requires
/// shipping that enforcement on the launch path without elevated privileges,
/// global firewall mutation, or a second privileged runtime owner.
pub fn investigate_network_confinement(
    product_launch_enforces_loopback_only: bool,
) -> NetworkConfinementFinding {
    let unshare = probe_unshare_net_available();
    let mut design_notes = vec![
        "feasible_unprivileged_design=bwrap_unshare_net+parent_unix_gateway_socket+optional_tcp_unix_bridge"
            .into(),
        "credential_hidden_alone_is_not_network_confinement".into(),
        "no_global_host_firewall_or_sysctl_mutation_authorized".into(),
    ];
    let non_claims = vec![
        "Do not claim loopback-only confinement merely because the child lacks the upstream API key."
            .into(),
        "Do not claim host TCP loopback gateway reachability under --unshare-net without a unix bridge."
            .into(),
        "Do not claim product launch enforces loopback-only unless product_launch_enforces_loopback_only is true."
            .into(),
    ];

    let (unix_bridge, external, host_lo, design_feasible) =
        match probe_loopback_only_unix_bridge_design() {
            Ok(result) => {
                design_notes.push(format!("probe_stdout={}", result.raw_stdout_summary));
                let unix = if result.unix_gateway_reachable {
                    CapabilityEvidenceClass::Proved
                } else {
                    CapabilityEvidenceClass::UnavailableFailClosed
                };
                let ext = if result.external_egress_blocked {
                    CapabilityEvidenceClass::Proved
                } else {
                    CapabilityEvidenceClass::UnavailableFailClosed
                };
                let hlo = if result.host_loopback_tcp_isolated {
                    CapabilityEvidenceClass::Proved
                } else {
                    CapabilityEvidenceClass::UnavailableFailClosed
                };
                (unix, ext, hlo, result.design_feasible())
            }
            Err(err) if err.contains("BLOCKED:bwrap_userns_unavailable") => {
                design_notes.push(format!("probe_blocked={err}"));
                (
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    false,
                )
            }
            Err(err) if err.contains("unshare-net unavailable") => {
                design_notes.push(format!("probe_skipped={err}"));
                (
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    CapabilityEvidenceClass::UnavailableFailClosed,
                    false,
                )
            }
            Err(err) if err.contains("bwrap unavailable") => {
                design_notes.push(format!("probe_skipped={err}"));
                (
                    CapabilityEvidenceClass::Unsupported,
                    CapabilityEvidenceClass::Unsupported,
                    CapabilityEvidenceClass::Unsupported,
                    false,
                )
            }
            Err(err) => {
                design_notes.push(format!("probe_outcome_unknown={err}"));
                (
                    CapabilityEvidenceClass::OutcomeUnknown,
                    CapabilityEvidenceClass::OutcomeUnknown,
                    CapabilityEvidenceClass::OutcomeUnknown,
                    false,
                )
            }
        };

    // Product residual: design may be feasible, but full admission requires the
    // product launch path to enforce loopback-only. Current product path uses
    // shared host network (credential non-bypass only) unless explicitly wired.
    let loopback_only_enforced = product_launch_enforces_loopback_only && design_feasible;
    let classification = if loopback_only_enforced {
        CapabilityEvidenceClass::Proved
    } else if design_feasible {
        // Design proved on this host; product launch still open → residual.
        CapabilityEvidenceClass::UnavailableFailClosed
    } else if matches!(unshare, CapabilityEvidenceClass::Unsupported) {
        CapabilityEvidenceClass::Unsupported
    } else if matches!(
        unshare,
        CapabilityEvidenceClass::UnavailableFailClosed | CapabilityEvidenceClass::OutcomeUnknown
    ) {
        unshare.clone()
    } else {
        CapabilityEvidenceClass::UnavailableFailClosed
    };

    let reason = if loopback_only_enforced {
        "Loopback-only network confinement is enforced on the product launch path and proved by executed bypass probes.".into()
    } else if design_feasible {
        "Executed probes prove an unprivileged design (unshare-net + parent Unix gateway) can block external egress while preserving gateway reachability, but the product mediated launch path still shares the host network (credential non-bypass only). Residual NO-GO until product launch enforces loopback-only with fail-closed host-capability gating.".into()
    } else {
        format!(
            "Loopback-only network confinement is not proved on this host profile (unshare_net={}, design_feasible={design_feasible}). Residual NO-GO.",
            unshare.as_str()
        )
    };

    NetworkConfinementFinding {
        classification,
        loopback_only_enforced,
        unshare_net_available: unshare,
        unix_socket_bridge_feasible: unix_bridge,
        external_egress_blocked_under_unshare_net: external,
        host_loopback_tcp_isolated_under_unshare_net: host_lo,
        product_launch_enforces_loopback_only,
        reason,
        design_notes,
        non_claims,
    }
}

/// Investigate unprivileged user + PID namespace capability with executed probes.
pub fn investigate_user_pid_namespace() -> UserPidNamespaceFinding {
    if !bwrap_present() {
        return UserPidNamespaceFinding {
            classification: CapabilityEvidenceClass::Unsupported,
            unprivileged_user_ns: CapabilityEvidenceClass::Unsupported,
            pid_namespace_with_user_ns: CapabilityEvidenceClass::Unsupported,
            reason:
                "bwrap is unavailable; user/PID namespace isolation is unsupported on this host."
                    .into(),
            non_claims: vec!["Do not silently claim PID isolation when bwrap is missing.".into()],
        };
    }

    let user_pid = if unprivileged_user_ns_available() {
        CapabilityEvidenceClass::Proved
    } else {
        // Distinguish hard fail vs unknown via a direct probe message.
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
            .arg("/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        match cmd.output() {
            Ok(output) if output.status.success() => CapabilityEvidenceClass::Proved,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("uid map")
                    || stderr.contains("Permission denied")
                    || stderr.contains("Operation not permitted")
                {
                    CapabilityEvidenceClass::UnavailableFailClosed
                } else {
                    CapabilityEvidenceClass::OutcomeUnknown
                }
            }
            Err(_) => CapabilityEvidenceClass::OutcomeUnknown,
        }
    };

    let classification = user_pid.clone();
    let reason = match &classification {
        CapabilityEvidenceClass::Proved => {
            "Unprivileged user+PID namespaces are available and probed successfully on this host."
                .into()
        }
        CapabilityEvidenceClass::UnavailableFailClosed => {
            "Unprivileged user+PID namespaces are unavailable (e.g. uid_map denied). Product path must fail closed for this axis and must not silently claim PID isolation.".into()
        }
        CapabilityEvidenceClass::Unsupported => {
            "User/PID namespace isolation is unsupported without bwrap.".into()
        }
        CapabilityEvidenceClass::OutcomeUnknown => {
            "User/PID namespace probe outcome is unknown.".into()
        }
    };

    UserPidNamespaceFinding {
        classification,
        unprivileged_user_ns: user_pid.clone(),
        pid_namespace_with_user_ns: user_pid,
        reason,
        non_claims: vec![
            "Do not silently downgrade isolation when userns is denied.".into(),
            "FS bind/tmpfs isolation may still apply without user/PID namespaces; that is a weaker process axis.".into(),
            "Host-wide sysctl or security-policy changes are not authorized to force this capability.".into(),
        ],
    }
}

/// Evaluate residual admission for the current host and product launch posture.
///
/// `product_launch_enforces_loopback_only` must reflect the actual mediated
/// launch path (currently false on main after PE7-CODEX-FULL-MEDIATION-ADMISSION-REPAIR-1).
pub fn evaluate_residual_admission(
    product_launch_enforces_loopback_only: bool,
) -> ResidualAdmissionFinding {
    let retry = investigate_retry_identity();
    let network = investigate_network_confinement(product_launch_enforces_loopback_only);
    let user_pid_ns = investigate_user_pid_namespace();

    let mut remaining = Vec::new();
    let mut closed = Vec::new();

    if retry.enforceable_retry_identity && retry.classification.is_proved() {
        closed.push("retry_identity".into());
    } else {
        remaining.push(format!("retry_identity: {}", retry.reason));
    }

    if network.loopback_only_enforced && network.classification.is_proved() {
        closed.push("network_confinement_loopback_only".into());
    } else {
        remaining.push(format!("network_confinement: {}", network.reason));
    }

    if user_pid_ns.classification.is_proved() {
        closed.push("user_pid_namespace".into());
    } else {
        remaining.push(format!("user_pid_namespace: {}", user_pid_ns.reason));
    }

    let all_closed = remaining.is_empty();
    let verdict = if all_closed {
        ResidualAdmissionVerdict::FullProviderFreeMediationAdmission
    } else {
        ResidualAdmissionVerdict::ResidualAdmissionNoGo
    };

    let product_admission_class = if matches!(
        verdict,
        ResidualAdmissionVerdict::FullProviderFreeMediationAdmission
    ) {
        CodexAdmissionClass::FullyAdmittedMediatedApiKey.as_str()
    } else {
        // Retain honest partial class; do not upgrade.
        CodexAdmissionClass::MediationHardenedPartial.as_str()
    }
    .to_string();

    ResidualAdmissionFinding {
        schema_version: CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA.to_string(),
        verdict,
        product_admission_class,
        retry,
        network,
        user_pid_ns,
        remaining_blockers: remaining,
        closed_axes: closed,
        notes: vec![
            "Usage-evidence closure and mediation_hardened_partial foundation remain accepted.".into(),
            "Residual closure does not authorize live Golden Path, RWE, Level-2, Meta, or OpenCode admission.".into(),
            "Live credential and operator authorization remain separate from this provider-free finding.".into(),
            "No second usage database, budget ledger, or CC Switch dependency is introduced.".into(),
        ],
    }
}

/// Convenience: current product launch does not enforce loopback-only netns.
pub fn evaluate_residual_admission_for_current_product() -> ResidualAdmissionFinding {
    evaluate_residual_admission(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn residual_verdict_is_no_go_while_retry_identity_unproved() {
        let finding = evaluate_residual_admission_for_current_product();
        assert_eq!(
            finding.schema_version,
            CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA
        );
        assert_eq!(
            finding.verdict,
            ResidualAdmissionVerdict::ResidualAdmissionNoGo
        );
        assert_eq!(
            finding.product_admission_class,
            CodexAdmissionClass::MediationHardenedPartial.as_str()
        );
        assert!(!finding.retry.enforceable_retry_identity);
        assert!(!finding.retry.classification.is_proved());
        assert!(finding
            .remaining_blockers
            .iter()
            .any(|b| b.starts_with("retry_identity:")));
        // Must not claim full admission.
        assert_ne!(
            finding.verdict,
            ResidualAdmissionVerdict::FullProviderFreeMediationAdmission
        );
        let json = finding.to_json();
        assert_eq!(json["verdict"], "residual_admission_no_go");
        assert_eq!(json["retry_identity"]["enforceable_retry_identity"], false);
        // Redaction: no secret material.
        let rendered = json.to_string();
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("OPENAI_API_KEY=sk"));
    }

    #[test]
    fn retry_identity_rejects_heuristic_accounting() {
        let retry = investigate_retry_identity();
        assert_eq!(retry.admitted_cli_version, ADMITTED_CODEX_VERSION);
        assert!(!retry.enforceable_retry_identity);
        assert!(retry.non_claims.iter().any(|c| c.contains("timing")));
        assert!(retry
            .non_claims
            .iter()
            .any(|c| c.contains("idempotencyKey")));
        assert!(retry
            .evidence_notes
            .iter()
            .any(|n| n.contains("not_wire_labeled") || n.contains("no_documented_wire_field")));
    }

    #[test]
    fn network_investigation_does_not_claim_product_enforcement_by_default() {
        let net = investigate_network_confinement(false);
        assert!(!net.product_launch_enforces_loopback_only);
        assert!(!net.loopback_only_enforced);
        assert!(
            !net.classification.is_proved(),
            "shared-host product path must not be classified proved for loopback-only"
        );
        assert!(net.non_claims.iter().any(|c| c.contains("API key")));
    }

    #[test]
    fn network_bridge_design_probe_is_executed_when_unshare_net_available() {
        if !bwrap_present() {
            eprintln!("skip: bwrap missing");
            return;
        }
        match probe_unshare_net_available() {
            CapabilityEvidenceClass::Proved => {
                let result = probe_loopback_only_unix_bridge_design()
                    .expect("design probe should run when unshare-net works");
                assert!(
                    result.external_egress_blocked,
                    "external egress must be blocked under unshare-net: {}",
                    result.raw_stdout_summary
                );
                assert!(
                    result.host_loopback_tcp_isolated,
                    "host loopback TCP must be isolated under unshare-net: {}",
                    result.raw_stdout_summary
                );
                assert!(
                    result.unix_gateway_reachable,
                    "parent unix gateway must remain reachable: {}",
                    result.raw_stdout_summary
                );
                assert!(result.design_feasible());
            }
            other => {
                eprintln!(
                    "unshare-net not proved on this host ({}); residual retained",
                    other.as_str()
                );
                let net = investigate_network_confinement(false);
                assert!(!net.loopback_only_enforced);
            }
        }
    }

    #[test]
    fn user_pid_namespace_is_typed_never_silent_success_on_failure() {
        let finding = investigate_user_pid_namespace();
        match finding.classification {
            CapabilityEvidenceClass::Proved => {
                assert!(finding.unprivileged_user_ns.is_proved());
                assert!(finding.pid_namespace_with_user_ns.is_proved());
            }
            CapabilityEvidenceClass::UnavailableFailClosed
            | CapabilityEvidenceClass::Unsupported
            | CapabilityEvidenceClass::OutcomeUnknown => {
                assert!(
                    !finding.classification.is_proved(),
                    "failed userns must not be marked proved"
                );
                assert!(
                    finding.reason.contains("unavailable")
                        || finding.reason.contains("unsupported")
                        || finding.reason.contains("unknown")
                        || finding.reason.contains("bwrap")
                );
            }
        }
        assert!(finding.non_claims.iter().any(|c| c.contains("silently")));
    }

    #[test]
    fn full_admission_requires_all_axes_including_product_network_enforcement() {
        // Even if we hypothetically mark product network enforced, retry still blocks.
        let finding = evaluate_residual_admission(true);
        assert_eq!(
            finding.verdict,
            ResidualAdmissionVerdict::ResidualAdmissionNoGo
        );
        assert!(finding
            .remaining_blockers
            .iter()
            .any(|b| b.starts_with("retry_identity:")));
    }

    #[test]
    fn unix_stream_smoke_without_bwrap_is_local_only() {
        // Sanity: UnixListener works on this host (used by design probe).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let path2 = path.clone();
        let handle = thread::spawn(move || {
            let mut stream = UnixStream::connect(path2).unwrap();
            stream.write_all(b"ping").unwrap();
        });
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4];
        conn.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        handle.join().unwrap();
    }
}
