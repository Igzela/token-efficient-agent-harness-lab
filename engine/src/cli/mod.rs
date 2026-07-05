pub mod claude_code;
pub mod cli_node_executor;
pub mod codex;
pub mod config;
pub mod multi_executor;

pub use claude_code::ClaudeCodeCliExecutor;
pub use cli_node_executor::CliNodeExecutor;
pub use codex::CodexCliExecutor;
pub use config::CliConfig;
pub use multi_executor::MultiExecutor;

use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

/// Apply the common direct-CLI process environment boundary.
///
/// CLI execution may need local account context for tools like Claude Code or Codex,
/// but it must not inherit the full engine environment by default. Keep PATH so the
/// CLI can launch its own helper binaries, then copy only explicit allowlisted keys
/// from `ACP_CLI_ENV_ALLOWLIST`.
pub(crate) fn apply_restricted_cli_env(cmd: &mut Command) {
    cmd.env_clear().env(
        "PATH",
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
    );
    for key in cli_env_allowlist() {
        if let Ok(value) = std::env::var(&key) {
            cmd.env(key, value);
        }
    }
}

fn cli_env_allowlist() -> Vec<String> {
    std::env::var("ACP_CLI_ENV_ALLOWLIST")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Spawn a child process and wait for it with a timeout.
/// Returns Ok(output) if the child exits within the deadline,
/// or Err(elapsed_ms) if the timeout fires (child is killed).
pub fn spawn_with_timeout(cmd: &mut Command, timeout_ms: u64) -> Result<Output, i64> {
    let start = Instant::now();
    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Err(0),
    };

    let deadline = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .map_err(|_| start.elapsed().as_millis() as i64);
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(start.elapsed().as_millis() as i64);
                }
                std::thread::sleep(poll_interval);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(start.elapsed().as_millis() as i64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_cli_env_drops_command_env_and_keeps_path() {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg("test -z \"${ACP_TEST_SECRET:-}\" && test -n \"${PATH:-}\"")
            .env("ACP_TEST_SECRET", "should-not-leak");
        apply_restricted_cli_env(&mut cmd);

        let output = cmd.output().expect("restricted env command should run");
        assert!(
            output.status.success(),
            "restricted env leaked explicit command env or removed PATH: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn test_spawn_with_timeout_fast_exit() {
        let mut cmd = Command::new("true");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_ok());
        assert!(result.unwrap().status.success());
    }

    #[test]
    fn test_spawn_with_timeout_failing_command() {
        let mut cmd = Command::new("false");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_ok());
        assert!(!result.unwrap().status.success());
    }

    #[test]
    fn test_spawn_with_timeout_kills_on_deadline() {
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        let result = spawn_with_timeout(&mut cmd, 200);
        assert!(result.is_err());
        let elapsed = result.unwrap_err();
        assert!(elapsed >= 200, "elapsed {elapsed}ms should be >= 200ms");
        assert!(
            elapsed < 5000,
            "elapsed {elapsed}ms should be well under 60s"
        );
    }

    #[test]
    fn test_spawn_with_timeout_nonexistent_binary() {
        let mut cmd = Command::new("nonexistent_binary_xyz_98765");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), 0);
    }
}
