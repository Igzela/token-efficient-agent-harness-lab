use std::process::Command;

pub const DEFAULT_CLI_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_CLI_EXECUTION_ENABLED: bool = false;

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub enabled: bool,
    pub claude_code_bin: Option<String>,
    pub claude_code_enabled: bool,
    pub codex_bin: Option<String>,
    pub codex_enabled: bool,
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
                codex_bin: None,
                codex_enabled: false,
                timeout_ms,
            };
        }

        // Claude Code exposes nested Edit/Write/Bash tools without an enforceable
        // app-owned filesystem sandbox or per-tool mediation contract. Keep it
        // unavailable until such a contract exists; Codex workspace-write is the
        // only managed CLI adapter registered by this runtime.
        let claude_code_bin = None;
        let claude_code_enabled = false;
        if env_opt("ACP_CLAUDE_CODE_BIN").is_some() {
            eprintln!(
                "[acp-cli] ACP_CLAUDE_CODE_BIN is ignored: claude_code_cli lacks the required managed workspace sandbox"
            );
        }

        let codex_bin = env_opt("ACP_CODEX_BIN").or_else(|| detect_binary("codex"));
        let codex_enabled = codex_bin.is_some();

        eprintln!(
            "[acp-cli] claude_code_cli is unavailable: nested tools lack app-owned mediation"
        );
        if !codex_enabled {
            eprintln!("[acp-cli] codex binary not found; codex_cli executor disabled");
        }

        Self {
            enabled,
            claude_code_bin,
            claude_code_enabled,
            codex_bin,
            codex_enabled,
            timeout_ms,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
