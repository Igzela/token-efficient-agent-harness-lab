pub mod cli_node_executor;
pub mod config;

pub use cli_node_executor::CliNodeExecutor;
pub use config::CliConfig;

use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum SpawnWithTimeoutError {
    SpawnFailed {
        kind: std::io::ErrorKind,
        raw_os_error: Option<i32>,
    },
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
    let mut spawn_attempt = 0_u8;
    let mut child: Child = loop {
        match cmd.spawn() {
            Ok(child) => break child,
            Err(error) if is_transient_text_busy(&error) && spawn_attempt < 2 => {
                spawn_attempt += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                return Err(SpawnWithTimeoutError::SpawnFailed {
                    kind: error.kind(),
                    raw_os_error: error.raw_os_error(),
                });
            }
        }
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

pub(crate) fn is_transient_text_busy(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(26)
    }
    #[cfg(not(unix))]
    {
        let _ = error;
        false
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
            SpawnWithTimeoutError::SpawnFailed {
                kind: std::io::ErrorKind::NotFound,
                ..
            }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn spawn_retries_transient_text_busy() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("transient-binary");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .open(&binary)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2));
            drop(writer);
        });

        let result = spawn_with_timeout(&mut Command::new(&binary), 5_000);

        release.join().unwrap();
        assert!(result.unwrap().status.success());
    }
}
