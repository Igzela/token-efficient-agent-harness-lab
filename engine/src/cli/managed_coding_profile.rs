//! Versioned, provider-free admission profile for a managed coding runtime.
//!
//! The profile is an immutable launch input.  It deliberately records the
//! observed binary separately from compatibility policy: a compatible version
//! does not erase the exact path, hash, or capability evidence attached to an
//! attempt.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::config::sha256_file;
use super::{spawn_with_timeout_with_limits, OutputLimits};

pub const MANAGED_CODING_RUNTIME_PROFILE_SCHEMA: &str = "managed_coding_runtime_profile.v1";
const PROBE_TIMEOUT_MS: u64 = 2_000;
const PROBE_LIMITS: OutputLimits = OutputLimits {
    stdout_bytes: 4 * 1024,
    stderr_bytes: 4 * 1024,
    combined_bytes: 8 * 1024,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedCodingExecutorKind {
    CodexCli,
    ProviderNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedCodingProtocolKind {
    OpenaiCompatible,
    AnthropicMessages,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVersionPolicy {
    /// Explicit versions are useful for a short emergency deny/allow list.
    #[serde(default)]
    pub allowed_versions: Vec<String>,
    /// Inclusive lower bound in `major.minor.patch` form.
    pub minimum_inclusive: Option<String>,
    /// Exclusive upper bound in `major.minor.patch` form.
    pub maximum_exclusive: Option<String>,
    #[serde(default)]
    pub denied_versions: Vec<String>,
}

impl RuntimeVersionPolicy {
    fn validate(&self) -> Result<(), String> {
        if self.allowed_versions.is_empty()
            && self.minimum_inclusive.is_none()
            && self.maximum_exclusive.is_none()
        {
            return Err(
                "managed coding runtime version policy requires an allowlist or supported range"
                    .to_string(),
            );
        }
        for version in self
            .allowed_versions
            .iter()
            .chain(self.denied_versions.iter())
        {
            Semver::parse(version)?;
        }
        let minimum = self
            .minimum_inclusive
            .as_deref()
            .map(Semver::parse)
            .transpose()?;
        let maximum = self
            .maximum_exclusive
            .as_deref()
            .map(Semver::parse)
            .transpose()?;
        if minimum.is_some_and(|minimum| maximum.is_some_and(|maximum| minimum >= maximum)) {
            return Err(
                "managed coding runtime version range must have a lower bound before its upper bound"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn admits(&self, observed: &str) -> Result<bool, String> {
        self.validate()?;
        let observed = Semver::parse(observed)?;
        let canonical = observed.render();
        if self
            .denied_versions
            .iter()
            .any(|version| version == &canonical)
        {
            return Ok(false);
        }
        if !self.allowed_versions.is_empty() {
            return Ok(self
                .allowed_versions
                .iter()
                .any(|version| version == &canonical));
        }
        if let Some(minimum) = self.minimum_inclusive.as_deref() {
            if observed < Semver::parse(minimum)? {
                return Ok(false);
            }
        }
        if let Some(maximum) = self.maximum_exclusive.as_deref() {
            if observed >= Semver::parse(maximum)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredCapabilityProbe {
    /// Arguments passed directly to the admitted binary; never a shell string.
    pub argv: Vec<String>,
    /// Required bounded output marker.  Marker values are capability names,
    /// not prompts or provider output, and must not contain secret-shaped data.
    pub required_stdout_fragment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedCodingRuntimeProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub executor_kind: ManagedCodingExecutorKind,
    pub protocol_kind: ManagedCodingProtocolKind,
    pub canonical_executable_path: Option<PathBuf>,
    /// Expected SHA-256 is required for binary-backed executors.
    pub expected_binary_sha256: Option<String>,
    pub version_probe_argv: Vec<String>,
    pub version_policy: RuntimeVersionPolicy,
    pub required_capability_probes: Vec<RequiredCapabilityProbe>,
    pub requested_model: Option<String>,
    pub resolved_model: Option<String>,
    pub thinking_configuration: Option<String>,
    pub provider_identity: String,
    pub credential_reference: Option<String>,
    pub endpoint_allowlist: Vec<String>,
    pub usage_parser_version: String,
    pub pricing_source_version: String,
    pub admission_classification: String,
}

impl ManagedCodingRuntimeProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != MANAGED_CODING_RUNTIME_PROFILE_SCHEMA {
            return Err("managed coding runtime profile schema is unsupported".to_string());
        }
        self.version_policy.validate()?;
        for (field, value) in [
            ("profile_id", self.profile_id.as_str()),
            ("provider_identity", self.provider_identity.as_str()),
            ("usage_parser_version", self.usage_parser_version.as_str()),
            (
                "pricing_source_version",
                self.pricing_source_version.as_str(),
            ),
            (
                "admission_classification",
                self.admission_classification.as_str(),
            ),
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(format!("managed coding runtime profile {field} is invalid"));
            }
        }
        if let Some(path) = self.canonical_executable_path.as_deref() {
            if !path.is_absolute() {
                return Err("managed coding runtime executable path must be absolute".to_string());
            }
            let expected = self.expected_binary_sha256.as_deref().ok_or_else(|| {
                "binary-backed managed coding profile requires expected_binary_sha256".to_string()
            })?;
            validate_sha256(expected)?;
            if self.version_probe_argv.is_empty() {
                return Err(
                    "binary-backed managed coding profile requires version probe argv".to_string(),
                );
            }
            if self.required_capability_probes.is_empty() {
                return Err(
                    "binary-backed managed coding profile requires capability probes".to_string(),
                );
            }
            let canonical = std::fs::canonicalize(path)
                .map_err(|error| format!("managed coding binary is unavailable: {error}"))?;
            if canonical != path {
                return Err("managed coding binary path must already be canonical".to_string());
            }
        } else if self.expected_binary_sha256.is_some()
            || !self.version_probe_argv.is_empty()
            || !self.required_capability_probes.is_empty()
        {
            return Err(
                "provider-native managed coding profile cannot declare binary probes".to_string(),
            );
        }
        for probe in &self.required_capability_probes {
            if probe.argv.is_empty()
                || probe
                    .argv
                    .iter()
                    .any(|arg| arg.is_empty() || arg.len() > 256)
                || probe.required_stdout_fragment.is_empty()
                || probe.required_stdout_fragment.len() > 128
            {
                return Err("managed coding capability probe is invalid".to_string());
            }
        }
        Ok(())
    }

    pub fn profile_sha256(&self) -> Result<String, String> {
        self.validate()?;
        let value = json!({
            "schema_version": self.schema_version,
            "profile_id": self.profile_id,
            "executor_kind": self.executor_kind,
            "protocol_kind": self.protocol_kind,
            "canonical_executable_path": self.canonical_executable_path,
            "expected_binary_sha256": self.expected_binary_sha256,
            "version_probe_argv": self.version_probe_argv,
            "version_policy": self.version_policy,
            "required_capability_probes": self.required_capability_probes,
            "requested_model": self.requested_model,
            "resolved_model": self.resolved_model,
            "thinking_configuration": self.thinking_configuration,
            "provider_identity": self.provider_identity,
            "credential_reference": self.credential_reference,
            "endpoint_allowlist": self.endpoint_allowlist,
            "usage_parser_version": self.usage_parser_version,
            "pricing_source_version": self.pricing_source_version,
            "admission_classification": self.admission_classification,
        });
        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedManagedCodingRuntime {
    pub canonical_executable_path: PathBuf,
    pub observed_version: String,
    pub binary_sha256: String,
    pub capability_probe_sha256: String,
    pub profile_sha256: String,
}

pub fn admit_binary_runtime(
    profile: &ManagedCodingRuntimeProfile,
) -> Result<ObservedManagedCodingRuntime, String> {
    profile.validate()?;
    let path = profile
        .canonical_executable_path
        .as_deref()
        .ok_or_else(|| "managed coding profile has no binary runtime".to_string())?;
    observe_binary_runtime(profile, path)
}

pub fn revalidate_binary_runtime(
    profile: &ManagedCodingRuntimeProfile,
    expected: &ObservedManagedCodingRuntime,
) -> Result<(), String> {
    if profile.profile_sha256()? != expected.profile_sha256 {
        return Err("managed coding runtime profile hash changed".to_string());
    }
    let observed = admit_binary_runtime(profile)?;
    if &observed != expected {
        return Err("managed coding runtime identity changed before spawn".to_string());
    }
    Ok(())
}

fn observe_binary_runtime(
    profile: &ManagedCodingRuntimeProfile,
    path: &Path,
) -> Result<ObservedManagedCodingRuntime, String> {
    let expected_sha = profile
        .expected_binary_sha256
        .as_deref()
        .ok_or_else(|| "managed coding binary profile has no expected SHA-256".to_string())?;
    validate_regular_executable(path)?;
    let binary_sha256 = sha256_file(path)?;
    if binary_sha256 != expected_sha.to_ascii_lowercase() {
        return Err("managed coding binary SHA-256 does not match runtime profile".to_string());
    }
    let observed_version = run_probe(path, &profile.version_probe_argv)?;
    let observed_version = parse_version(&observed_version)?;
    if !profile.version_policy.admits(&observed_version)? {
        return Err(
            "managed coding binary version is outside the runtime profile policy".to_string(),
        );
    }
    let capability_probe_sha256 = probe_capabilities(path, &profile.required_capability_probes)?;
    Ok(ObservedManagedCodingRuntime {
        canonical_executable_path: path.to_path_buf(),
        observed_version,
        binary_sha256,
        capability_probe_sha256,
        profile_sha256: profile.profile_sha256()?,
    })
}

fn validate_regular_executable(path: &Path) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("managed coding binary is unavailable: {error}"))?;
    if canonical != path {
        return Err(
            "managed coding binary path must be canonical and contain no symlink".to_string(),
        );
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("managed coding binary is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("managed coding binary must be a regular file, not a symlink".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("managed coding binary must be executable".to_string());
        }
    }
    Ok(())
}

fn run_probe(path: &Path, argv: &[String]) -> Result<String, String> {
    let mut command = Command::new(path);
    command
        .args(argv)
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir("/");
    let output = spawn_with_timeout_with_limits(&mut command, PROBE_TIMEOUT_MS, PROBE_LIMITS)
        .map_err(|error| format!("managed coding probe failed: {}", error.reason_code()))?;
    if !output.status.success() {
        return Err("managed coding probe returned nonzero status".to_string());
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "managed coding probe output is not UTF-8".to_string())
}

fn probe_capabilities(path: &Path, probes: &[RequiredCapabilityProbe]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for probe in probes {
        let output = run_probe(path, &probe.argv)?;
        if !output.contains(&probe.required_stdout_fragment) {
            return Err("managed coding required capability probe failed".to_string());
        }
        hasher.update(probe.argv.join("\u{1f}").as_bytes());
        hasher.update([0]);
        hasher.update(probe.required_stdout_fragment.as_bytes());
        hasher.update([0]);
        hasher.update(Sha256::digest(output.as_bytes()));
    }
    Ok(hex::encode(hasher.finalize()))
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("managed coding binary SHA-256 must be 64 hexadecimal characters".to_string())
    }
}

fn parse_version(output: &str) -> Result<String, String> {
    output
        .split_whitespace()
        .find_map(|token| Semver::parse(token).ok().map(|version| version.render()))
        .ok_or_else(|| "managed coding version probe did not produce semantic version".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Semver(u64, u64, u64);

impl Semver {
    fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim().trim_start_matches('v');
        let mut parts = value.split('.');
        let parse_part = |part: Option<&str>| {
            part.ok_or_else(|| "managed coding version must be major.minor.patch".to_string())?
                .parse::<u64>()
                .map_err(|_| "managed coding version must be major.minor.patch".to_string())
        };
        let major = parse_part(parts.next())?;
        let minor = parse_part(parts.next())?;
        let patch = parse_part(parts.next())?;
        if parts.next().is_some() {
            return Err("managed coding version must be major.minor.patch".to_string());
        }
        Ok(Self(major, minor, patch))
    }

    fn render(self) -> String {
        format!("{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn script(body: &str) -> (tempfile::TempDir, PathBuf, String) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("codex-real");
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let digest = sha256_file(&path).unwrap();
        (directory, path, digest)
    }

    fn profile(path: PathBuf, sha256: String) -> ManagedCodingRuntimeProfile {
        ManagedCodingRuntimeProfile {
            schema_version: MANAGED_CODING_RUNTIME_PROFILE_SCHEMA.to_string(),
            profile_id: "codex-compatible-test".to_string(),
            executor_kind: ManagedCodingExecutorKind::CodexCli,
            protocol_kind: ManagedCodingProtocolKind::OpenaiCompatible,
            canonical_executable_path: Some(path),
            expected_binary_sha256: Some(sha256),
            version_probe_argv: vec!["--version".to_string()],
            version_policy: RuntimeVersionPolicy {
                allowed_versions: vec![],
                minimum_inclusive: Some("0.146.0".to_string()),
                maximum_exclusive: Some("0.147.0".to_string()),
                denied_versions: vec![],
            },
            required_capability_probes: vec![RequiredCapabilityProbe {
                argv: vec!["exec".to_string(), "--help".to_string()],
                required_stdout_fragment: "--sandbox".to_string(),
            }],
            requested_model: Some("gpt-5.6-luna".to_string()),
            resolved_model: None,
            thinking_configuration: None,
            provider_identity: "openai_compatible".to_string(),
            credential_reference: Some("ACP_CODEX_UPSTREAM_API_KEY".to_string()),
            endpoint_allowlist: vec!["/v1/responses".to_string()],
            usage_parser_version: "codex_usage.v1".to_string(),
            pricing_source_version: "fixture.v1".to_string(),
            admission_classification: "test".to_string(),
        }
    }

    #[test]
    fn admits_codex_0146_patch_without_a_rust_version_constant() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.7'; else echo '--sandbox --json'; fi\n",
        );
        let observed = admit_binary_runtime(&profile(path, sha256)).unwrap();
        assert_eq!(observed.observed_version, "0.146.7");
    }

    #[test]
    fn later_compatible_patch_needs_only_profile_policy_change() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.99'; else echo '--sandbox --json'; fi\n",
        );
        assert_eq!(
            admit_binary_runtime(&profile(path, sha256))
                .unwrap()
                .observed_version,
            "0.146.99"
        );
    }

    #[test]
    fn unconstrained_version_policy_fails_closed() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.1'; else echo '--sandbox'; fi\n",
        );
        let mut profile = profile(path, sha256);
        profile.version_policy = RuntimeVersionPolicy {
            allowed_versions: vec![],
            minimum_inclusive: None,
            maximum_exclusive: None,
            denied_versions: vec![],
        };
        assert!(admit_binary_runtime(&profile).is_err());
    }

    #[test]
    fn missing_required_capability_fails_closed() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.1'; else echo '--json'; fi\n",
        );
        assert!(admit_binary_runtime(&profile(path, sha256)).is_err());
    }

    #[test]
    fn replacement_between_probe_and_spawn_fails_closed() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.1'; else echo '--sandbox'; fi\n",
        );
        let profile = profile(path.clone(), sha256);
        let observed = admit_binary_runtime(&profile).unwrap();
        fs::write(&path, "#!/bin/sh\necho changed\n").unwrap();
        assert!(revalidate_binary_runtime(&profile, &observed).is_err());
    }

    #[test]
    fn symlink_and_profile_mutation_fail_closed() {
        let (_directory, path, sha256) = script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.1'; else echo '--sandbox'; fi\n",
        );
        let profile = profile(path.clone(), sha256);
        let observed = admit_binary_runtime(&profile).unwrap();
        let link = path.with_file_name("codex-link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&path, &link).unwrap();
        #[cfg(unix)]
        assert!(admit_binary_runtime(&ManagedCodingRuntimeProfile {
            canonical_executable_path: Some(link),
            ..profile.clone()
        })
        .is_err());
        let mut mutated = profile.clone();
        mutated.usage_parser_version = "changed.v2".to_string();
        assert!(revalidate_binary_runtime(&mutated, &observed).is_err());
    }
}
