pub mod claude_code;
pub mod codex;
pub mod config;
pub mod multi_executor;

pub use claude_code::ClaudeCodeCliExecutor;
pub use codex::CodexCliExecutor;
pub use config::CliConfig;
pub use multi_executor::MultiExecutor;

use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

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
