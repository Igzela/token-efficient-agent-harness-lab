pub mod cli_node_executor;
pub mod config;

pub use cli_node_executor::CliNodeExecutor;
pub use config::CliConfig;

use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum SpawnWithTimeoutError {
    SpawnFailed,
    TimedOut {
        elapsed_ms: i64,
        terminated_status: Option<std::process::ExitStatus>,
    },
    WaitFailed {
        elapsed_ms: i64,
        observed_status: Option<std::process::ExitStatus>,
    },
}

/// Spawn a child process and wait for it with a timeout.
/// Returns Ok(output) if the child exits within the deadline,
/// or an exact bounded process failure. Timeout and wait failure retain any
/// termination status observed while cleaning up the child.
pub fn spawn_with_timeout(
    cmd: &mut Command,
    timeout_ms: u64,
) -> Result<Output, SpawnWithTimeoutError> {
    let start = Instant::now();
    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Err(SpawnWithTimeoutError::SpawnFailed),
    };

    let deadline = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return child
                    .wait_with_output()
                    .map_err(|_| SpawnWithTimeoutError::WaitFailed {
                        elapsed_ms: start.elapsed().as_millis() as i64,
                        observed_status: Some(status),
                    });
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    let _ = child.kill();
                    let terminated_status = child.wait().ok();
                    return Err(SpawnWithTimeoutError::TimedOut {
                        elapsed_ms: start.elapsed().as_millis() as i64,
                        terminated_status,
                    });
                }
                std::thread::sleep(poll_interval);
            }
            Err(_) => {
                let _ = child.kill();
                let observed_status = child.wait().ok();
                return Err(SpawnWithTimeoutError::WaitFailed {
                    elapsed_ms: start.elapsed().as_millis() as i64,
                    observed_status,
                });
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
        let SpawnWithTimeoutError::TimedOut { elapsed_ms, .. } = result.unwrap_err() else {
            panic!("expected timeout");
        };
        assert!(
            elapsed_ms >= 200,
            "elapsed {elapsed_ms}ms should be >= 200ms"
        );
        assert!(
            elapsed_ms < 5000,
            "elapsed {elapsed_ms}ms should be well under 60s"
        );
    }

    #[test]
    fn test_spawn_with_timeout_nonexistent_binary() {
        let mut cmd = Command::new("nonexistent_binary_xyz_98765");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SpawnWithTimeoutError::SpawnFailed
        ));
    }
}
