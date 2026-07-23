pub mod cli_node_executor;
pub mod config;

pub use cli_node_executor::CliNodeExecutor;
pub use config::CliConfig;

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
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
///
/// stdout/stderr are forced to pipes and drained on background threads while
/// waiting so a long-running child that emits a large JSONL/trace stream cannot
/// block forever once the OS pipe buffer fills.
///
/// Returns Ok(output) if the child exits within the deadline, or an exact bounded
/// process failure. Timeout and wait failure retain any termination status
/// observed while cleaning up the child.
pub fn spawn_with_timeout(
    cmd: &mut Command,
    timeout_ms: u64,
) -> Result<Output, SpawnWithTimeoutError> {
    // Always capture and drain streams. Callers that previously inherited
    // stdout/stderr still get an Output with empty captured buffers when the
    // child writes nothing.
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

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

    let stdout_reader = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            buffer
        })
    });
    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            buffer
        })
    });

    let deadline = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(100);

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= deadline {
                    let _ = child.kill();
                    let terminated_status = child.wait().ok();
                    // Drain reader threads after kill so pipes close cleanly.
                    let _ = join_pipe(stdout_reader);
                    let _ = join_pipe(stderr_reader);
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
                let _ = join_pipe(stdout_reader);
                let _ = join_pipe(stderr_reader);
                return Err(SpawnWithTimeoutError::WaitFailed {
                    elapsed_ms: start.elapsed().as_millis() as i64,
                    observed_status,
                });
            }
        }
    };

    // Child already exited; collect drained buffers. Do not call wait_with_output
    // because stdout/stderr handles were moved into the drain threads.
    let _ = child.wait();
    let stdout = join_pipe(stdout_reader).unwrap_or_default();
    let stderr = join_pipe(stderr_reader).unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn join_pipe(handle: Option<std::thread::JoinHandle<Vec<u8>>>) -> Option<Vec<u8>> {
    handle.and_then(|thread| thread.join().ok())
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
    fn spawn_with_timeout_drains_large_stdout_without_deadlock() {
        // Without concurrent drain, writing more than the OS pipe capacity while
        // the parent only polls try_wait deadlocks until timeout. 256 KiB is
        // well above a typical 64 KiB pipe buffer.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "dd if=/dev/zero bs=1024 count=256 status=none"]);
        let result = spawn_with_timeout(&mut cmd, 5_000).expect("large stdout should complete");
        assert!(result.status.success());
        assert_eq!(result.stdout.len(), 256 * 1024);
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
