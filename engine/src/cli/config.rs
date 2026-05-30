use std::process::Command;

pub const DEFAULT_CLI_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_COMPLEXITY_THRESHOLD: f64 = 0.7;

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub enabled: bool,
    pub claude_code_bin: Option<String>,
    pub claude_code_enabled: bool,
    pub codex_bin: Option<String>,
    pub codex_enabled: bool,
    pub timeout_ms: u64,
    pub complexity_threshold: f64,
}

impl CliConfig {
    pub fn from_env() -> Self {
        let enabled = env_bool("ACP_ENABLE_CLI_EXECUTION", true);
        let timeout_ms = env_u64("ACP_CLI_TIMEOUT_MS", DEFAULT_CLI_TIMEOUT_MS);
        let complexity_threshold =
            env_f64("ACP_CLI_COMPLEXITY_THRESHOLD", DEFAULT_COMPLEXITY_THRESHOLD);

        if !enabled {
            return Self {
                enabled: false,
                claude_code_bin: None,
                claude_code_enabled: false,
                codex_bin: None,
                codex_enabled: false,
                timeout_ms,
                complexity_threshold,
            };
        }

        let claude_code_bin = env_opt("ACP_CLAUDE_CODE_BIN").or_else(|| detect_binary("claude"));
        let claude_code_enabled = claude_code_bin.is_some();

        let codex_bin = env_opt("ACP_CODEX_BIN").or_else(|| detect_binary("codex"));
        let codex_enabled = codex_bin.is_some();

        if !claude_code_enabled {
            eprintln!("[acp-cli] claude binary not found; claude_code_cli executor disabled");
        }
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
            complexity_threshold,
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

fn env_f64(key: &str, default: f64) -> f64 {
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
        assert!(config.complexity_threshold > 0.0);
        assert!(config.complexity_threshold <= 1.0);
    }

    #[test]
    fn test_detect_binary_found() {
        let result = detect_binary("sh");
        assert!(result.is_some(), "sh binary should be detected on any Unix system");
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
