pub mod cli_node_executor;
pub mod codex_budget_authority;
pub mod codex_mediation_admission;
pub mod codex_residual_admission;
pub mod codex_session_usage;
pub mod codex_usage_journal;
pub mod config;

pub use cli_node_executor::CliNodeExecutor;
pub use config::CliConfig;

use std::io::{self, Read};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Versioned bounded-capture contract for every managed CLI process.
pub const CLI_OUTPUT_LIMITS_SCHEMA_VERSION: &str = "managed_cli_output_limits.v1";
pub const DEFAULT_CLI_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_CLI_STDERR_LIMIT_BYTES: usize = 1024 * 1024;
pub const DEFAULT_CLI_COMBINED_LIMIT_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_CONFIGURED_CLI_OUTPUT_LIMIT_BYTES: usize = 16 * 1024 * 1024;
pub const CLI_PROCESS_CLEANUP_TIMEOUT_MS: u64 = 1_000;
const OUTPUT_READ_CHUNK_BYTES: usize = 8 * 1024;

/// Per-stream and combined byte ceilings. Bytes beyond any ceiling are never
/// retained or returned to a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLimits {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub combined_bytes: usize,
}

pub const DEFAULT_CLI_OUTPUT_LIMITS: OutputLimits = OutputLimits {
    stdout_bytes: DEFAULT_CLI_STDOUT_LIMIT_BYTES,
    stderr_bytes: DEFAULT_CLI_STDERR_LIMIT_BYTES,
    combined_bytes: DEFAULT_CLI_COMBINED_LIMIT_BYTES,
};

impl OutputLimits {
    fn validate(self) -> Result<(), String> {
        if self.stdout_bytes == 0 || self.stderr_bytes == 0 || self.combined_bytes == 0 {
            return Err("CLI output limits must be positive".to_string());
        }
        if self.stdout_bytes > MAX_CONFIGURED_CLI_OUTPUT_LIMIT_BYTES
            || self.stderr_bytes > MAX_CONFIGURED_CLI_OUTPUT_LIMIT_BYTES
            || self.combined_bytes > MAX_CONFIGURED_CLI_OUTPUT_LIMIT_BYTES
        {
            return Err(format!(
                "CLI output limits exceed the {}-byte maximum",
                MAX_CONFIGURED_CLI_OUTPUT_LIMIT_BYTES
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
    Combined,
}

impl OutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Combined => "combined",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLimitDetails {
    pub stream: OutputStream,
    pub trigger_stream: OutputStream,
    pub configured_limit_bytes: usize,
    pub observed_bytes: usize,
    pub combined_observed_bytes: usize,
    pub elapsed_ms: i64,
}

impl OutputLimitDetails {
    pub fn summary(&self) -> String {
        format!(
            "schema={CLI_OUTPUT_LIMITS_SCHEMA_VERSION};stream={};trigger_stream={};configured_limit_bytes={};observed_bytes={};combined_observed_bytes={};elapsed_ms={}",
            self.stream.as_str(),
            self.trigger_stream.as_str(),
            self.configured_limit_bytes,
            self.observed_bytes,
            self.combined_observed_bytes,
            self.elapsed_ms,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderFailureDetails {
    pub stream: OutputStream,
    pub kind: io::ErrorKind,
    pub raw_os_error: Option<i32>,
}

impl ReaderFailureDetails {
    fn summary(&self) -> String {
        format!(
            "stream={};kind={:?};raw_os_error={:?}",
            self.stream.as_str(),
            self.kind,
            self.raw_os_error,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupStep {
    NotRequired,
    Requested,
    AlreadyGone,
    AlreadyExited,
    Reaped,
    Completed,
    TimedOut,
    Failed {
        kind: io::ErrorKind,
        raw_os_error: Option<i32>,
    },
    Unsupported,
}

impl CleanupStep {
    fn failed(&self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::Failed { .. } | Self::Unsupported
        )
    }

    fn summary(&self) -> String {
        match self {
            Self::NotRequired => "not_required".to_string(),
            Self::Requested => "requested".to_string(),
            Self::AlreadyGone => "already_gone".to_string(),
            Self::AlreadyExited => "already_exited".to_string(),
            Self::Reaped => "reaped".to_string(),
            Self::Completed => "completed".to_string(),
            Self::TimedOut => "timed_out".to_string(),
            Self::Failed { kind, raw_os_error } => {
                format!("failed:kind={kind:?};raw_os_error={raw_os_error:?}")
            }
            Self::Unsupported => "unsupported".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTerminationOutcome {
    pub process_group: CleanupStep,
    pub process: CleanupStep,
    pub wait: CleanupStep,
    pub readers: CleanupStep,
}

impl Default for ProcessTerminationOutcome {
    fn default() -> Self {
        Self {
            process_group: CleanupStep::NotRequired,
            process: CleanupStep::NotRequired,
            wait: CleanupStep::NotRequired,
            readers: CleanupStep::NotRequired,
        }
    }
}

impl ProcessTerminationOutcome {
    fn failed(&self) -> bool {
        self.process_group.failed()
            || self.process.failed()
            || self.wait.failed()
            || self.readers.failed()
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "process_group={};process={};wait={};readers={}",
            self.process_group.summary(),
            self.process.summary(),
            self.wait.summary(),
            self.readers.summary(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnWithTimeoutError {
    SpawnFailed {
        kind: std::io::ErrorKind,
        raw_os_error: Option<i32>,
    },
    TimedOut {
        elapsed_ms: i64,
        terminated_status: Option<std::process::ExitStatus>,
        termination: ProcessTerminationOutcome,
    },
    WaitFailed {
        elapsed_ms: i64,
        observed_status: Option<std::process::ExitStatus>,
        kind: io::ErrorKind,
        raw_os_error: Option<i32>,
        termination: ProcessTerminationOutcome,
    },
    ReaderFailed {
        stream: OutputStream,
        kind: io::ErrorKind,
        raw_os_error: Option<i32>,
        termination: ProcessTerminationOutcome,
    },
    OutputLimitExceeded {
        details: OutputLimitDetails,
        termination: ProcessTerminationOutcome,
    },
    ProcessTreeCleanupFailed {
        elapsed_ms: i64,
        primary_reason: String,
        termination: ProcessTerminationOutcome,
    },
    ProcessTreeContainmentUnsupported,
    InvalidOutputLimits {
        reason: String,
    },
}

impl SpawnWithTimeoutError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::SpawnFailed { .. } => "spawn_failed",
            Self::TimedOut { .. } => "timeout",
            Self::WaitFailed { .. } => "wait_failed",
            Self::ReaderFailed {
                stream: OutputStream::Stdout,
                ..
            } => "stdout_reader_failed",
            Self::ReaderFailed {
                stream: OutputStream::Stderr,
                ..
            } => "stderr_reader_failed",
            Self::ReaderFailed {
                stream: OutputStream::Combined,
                ..
            } => "combined_reader_failed",
            Self::OutputLimitExceeded { .. } => "output_limit_exceeded",
            Self::ProcessTreeCleanupFailed { .. } => "process_tree_cleanup_failed",
            Self::ProcessTreeContainmentUnsupported => "process_tree_containment_unsupported",
            Self::InvalidOutputLimits { .. } => "invalid_output_limits",
        }
    }
}

/// Spawn a child process and wait for it with a timeout.
///
/// stdout/stderr are forced to pipes and drained on background threads while
/// waiting so a long-running child that emits a large JSONL/trace stream cannot
/// block forever once the OS pipe buffer fills.
/// On Unix the child starts its own session so a timed-out or early-exiting CLI
/// cannot leave descendants holding the captured pipes open.
///
/// Returns Ok(output) if the child exits within the deadline, or an exact bounded
/// process failure. Timeout and wait failure retain any termination status
/// observed while cleaning up the child.
pub fn spawn_with_timeout(
    cmd: &mut Command,
    timeout_ms: u64,
) -> Result<Output, SpawnWithTimeoutError> {
    spawn_with_timeout_with_limits(cmd, timeout_ms, DEFAULT_CLI_OUTPUT_LIMITS)
}

/// Spawn a managed process with bounded concurrent stdout/stderr capture.
///
/// A reader records only bytes that remain below both the stream and combined
/// limits. It atomically signals the owner when a limit or reader failure is
/// observed. The owner then terminates the contained process group, reaps the
/// parent, and waits for readers only until the bounded cleanup deadline.
pub fn spawn_with_timeout_with_limits(
    cmd: &mut Command,
    timeout_ms: u64,
    limits: OutputLimits,
) -> Result<Output, SpawnWithTimeoutError> {
    if !process_tree_containment_supported() {
        return Err(SpawnWithTimeoutError::ProcessTreeContainmentUnsupported);
    }
    limits
        .validate()
        .map_err(|reason| SpawnWithTimeoutError::InvalidOutputLimits { reason })?;

    // Always capture and drain streams. Callers that previously inherited
    // stdout/stderr still get an Output with empty captured buffers when the
    // child writes nothing.
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_containment(cmd);

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

    let capture = Arc::new(SharedCapture::new(limits, start));
    let stdout_reader = spawn_pipe_reader(OutputStream::Stdout, child.stdout.take(), &capture);
    let stderr_reader = spawn_pipe_reader(OutputStream::Stderr, child.stderr.take(), &capture);

    let deadline = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(10);

    let mut primary = PrimaryFailure::None;
    let status = loop {
        if let Some(details) = capture.limit_event() {
            primary = PrimaryFailure::OutputLimit(details);
            break None;
        }
        if let Some(details) = capture.reader_failure() {
            primary = PrimaryFailure::Reader(details);
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= deadline {
                    primary = PrimaryFailure::TimedOut;
                    break None;
                }
                std::thread::sleep(poll_interval);
            }
            Err(error) => {
                primary = PrimaryFailure::WaitFailed {
                    kind: error.kind(),
                    raw_os_error: error.raw_os_error(),
                };
                break None;
            }
        }
    };

    let cleanup_deadline = Instant::now() + Duration::from_millis(CLI_PROCESS_CLEANUP_TIMEOUT_MS);
    let mut termination = cleanup_process_tree(&mut child, status.is_some(), cleanup_deadline);
    termination.readers = wait_for_readers(&capture, cleanup_deadline);
    let readers_joined = termination.readers == CleanupStep::Completed;
    if readers_joined
        && stdout_reader
            .into_iter()
            .chain(stderr_reader)
            .any(|reader| reader.join().is_err())
    {
        termination.readers = CleanupStep::Failed {
            kind: io::ErrorKind::Other,
            raw_os_error: None,
        };
    }

    if termination.failed() {
        return Err(SpawnWithTimeoutError::ProcessTreeCleanupFailed {
            elapsed_ms: start.elapsed().as_millis() as i64,
            primary_reason: primary.reason(&capture),
            termination,
        });
    }

    if let Some(details) = capture.limit_event() {
        return Err(SpawnWithTimeoutError::OutputLimitExceeded {
            details,
            termination,
        });
    }
    if let Some(details) = capture.reader_failure() {
        return Err(SpawnWithTimeoutError::ReaderFailed {
            stream: details.stream,
            kind: details.kind,
            raw_os_error: details.raw_os_error,
            termination,
        });
    }

    match primary {
        PrimaryFailure::None => {
            let status = status.expect("successful managed process has an exit status");
            let (stdout, stderr) = capture.take_output();
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        PrimaryFailure::OutputLimit(details) => Err(SpawnWithTimeoutError::OutputLimitExceeded {
            details,
            termination,
        }),
        PrimaryFailure::Reader(details) => Err(SpawnWithTimeoutError::ReaderFailed {
            stream: details.stream,
            kind: details.kind,
            raw_os_error: details.raw_os_error,
            termination,
        }),
        PrimaryFailure::TimedOut => Err(SpawnWithTimeoutError::TimedOut {
            elapsed_ms: start.elapsed().as_millis() as i64,
            terminated_status: status,
            termination,
        }),
        PrimaryFailure::WaitFailed { kind, raw_os_error } => {
            Err(SpawnWithTimeoutError::WaitFailed {
                elapsed_ms: start.elapsed().as_millis() as i64,
                observed_status: status,
                kind,
                raw_os_error,
                termination,
            })
        }
    }
}

#[derive(Debug)]
struct SharedCapture {
    state: Mutex<CaptureState>,
    stop_signal: AtomicBool,
    limits: OutputLimits,
    started: Instant,
}

#[derive(Debug)]
struct CaptureState {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_observed: usize,
    stderr_observed: usize,
    combined_observed: usize,
    stdout_finished: bool,
    stderr_finished: bool,
    limit_event: Option<OutputLimitDetails>,
    reader_failure: Option<ReaderFailureDetails>,
}

impl SharedCapture {
    fn new(limits: OutputLimits, started: Instant) -> Self {
        Self {
            state: Mutex::new(CaptureState {
                stdout: Vec::with_capacity(limits.stdout_bytes.min(64 * 1024)),
                stderr: Vec::with_capacity(limits.stderr_bytes.min(64 * 1024)),
                stdout_observed: 0,
                stderr_observed: 0,
                combined_observed: 0,
                stdout_finished: false,
                stderr_finished: false,
                limit_event: None,
                reader_failure: None,
            }),
            stop_signal: AtomicBool::new(false),
            limits,
            started,
        }
    }

    fn record_chunk(&self, stream: OutputStream, bytes: &[u8]) {
        if self.stop_signal.load(Ordering::SeqCst) {
            return;
        }
        let mut state = self.state.lock().expect("CLI capture state lock");
        if state.limit_event.is_some() || state.reader_failure.is_some() {
            return;
        }
        let stream_limit = match stream {
            OutputStream::Stdout => self.limits.stdout_bytes,
            OutputStream::Stderr => self.limits.stderr_bytes,
            OutputStream::Combined => self.limits.combined_bytes,
        };
        let observed_bytes = match stream {
            OutputStream::Stdout => {
                state.stdout_observed =
                    bounded_count(state.stdout_observed, bytes.len(), self.limits.stdout_bytes);
                state.stdout_observed
            }
            OutputStream::Stderr => {
                state.stderr_observed =
                    bounded_count(state.stderr_observed, bytes.len(), self.limits.stderr_bytes);
                state.stderr_observed
            }
            OutputStream::Combined => {
                state.combined_observed = bounded_count(
                    state.combined_observed,
                    bytes.len(),
                    self.limits.combined_bytes,
                );
                state.combined_observed
            }
        };
        state.combined_observed = bounded_count(
            state.combined_observed,
            bytes.len(),
            self.limits.combined_bytes,
        );
        if observed_bytes > stream_limit {
            self.record_limit_locked(&mut state, stream, stream, stream_limit, observed_bytes);
            return;
        }
        if state.combined_observed > self.limits.combined_bytes {
            let combined_observed = state.combined_observed;
            self.record_limit_locked(
                &mut state,
                OutputStream::Combined,
                stream,
                self.limits.combined_bytes,
                combined_observed,
            );
            return;
        }
        match stream {
            OutputStream::Stdout => state.stdout.extend_from_slice(bytes),
            OutputStream::Stderr => state.stderr.extend_from_slice(bytes),
            OutputStream::Combined => unreachable!("combined is not a pipe reader"),
        }
    }

    fn record_limit_locked(
        &self,
        state: &mut CaptureState,
        stream: OutputStream,
        trigger_stream: OutputStream,
        configured_limit_bytes: usize,
        observed_bytes: usize,
    ) {
        state.limit_event = Some(OutputLimitDetails {
            stream,
            trigger_stream,
            configured_limit_bytes,
            observed_bytes,
            combined_observed_bytes: state.combined_observed,
            elapsed_ms: self.started.elapsed().as_millis() as i64,
        });
        self.stop_signal.store(true, Ordering::SeqCst);
    }

    fn record_reader_failure(&self, stream: OutputStream, error: &io::Error) {
        let mut state = self.state.lock().expect("CLI capture state lock");
        if state.limit_event.is_none() && state.reader_failure.is_none() {
            state.reader_failure = Some(ReaderFailureDetails {
                stream,
                kind: error.kind(),
                raw_os_error: error.raw_os_error(),
            });
            self.stop_signal.store(true, Ordering::SeqCst);
        }
    }

    fn mark_finished(&self, stream: OutputStream) {
        let mut state = self.state.lock().expect("CLI capture state lock");
        match stream {
            OutputStream::Stdout => state.stdout_finished = true,
            OutputStream::Stderr => state.stderr_finished = true,
            OutputStream::Combined => {}
        }
    }

    fn readers_finished(&self) -> bool {
        let state = self.state.lock().expect("CLI capture state lock");
        state.stdout_finished && state.stderr_finished
    }

    fn limit_event(&self) -> Option<OutputLimitDetails> {
        self.state
            .lock()
            .expect("CLI capture state lock")
            .limit_event
            .clone()
    }

    fn reader_failure(&self) -> Option<ReaderFailureDetails> {
        self.state
            .lock()
            .expect("CLI capture state lock")
            .reader_failure
            .clone()
    }

    fn take_output(&self) -> (Vec<u8>, Vec<u8>) {
        let mut state = self.state.lock().expect("CLI capture state lock");
        (
            std::mem::take(&mut state.stdout),
            std::mem::take(&mut state.stderr),
        )
    }
}

fn bounded_count(current: usize, added: usize, limit: usize) -> usize {
    current.saturating_add(added).min(limit.saturating_add(1))
}

fn spawn_pipe_reader(
    stream: OutputStream,
    pipe: Option<impl Read + Send + 'static>,
    capture: &Arc<SharedCapture>,
) -> Option<JoinHandle<()>> {
    let Some(mut pipe) = pipe else {
        capture.mark_finished(stream);
        return None;
    };
    let capture = Arc::clone(capture);
    Some(thread::spawn(move || {
        let mut chunk = [0_u8; OUTPUT_READ_CHUNK_BYTES];
        loop {
            if capture.stop_signal.load(Ordering::SeqCst) {
                break;
            }
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => capture.record_chunk(stream, &chunk[..read]),
                Err(error) => {
                    if !capture.stop_signal.load(Ordering::SeqCst) {
                        capture.record_reader_failure(stream, &error);
                    }
                    break;
                }
            }
        }
        capture.mark_finished(stream);
    }))
}

fn wait_for_readers(capture: &SharedCapture, deadline: Instant) -> CleanupStep {
    while !capture.readers_finished() {
        if Instant::now() >= deadline {
            return CleanupStep::TimedOut;
        }
        thread::sleep(Duration::from_millis(5));
    }
    CleanupStep::Completed
}

#[derive(Debug)]
enum PrimaryFailure {
    None,
    OutputLimit(OutputLimitDetails),
    Reader(ReaderFailureDetails),
    TimedOut,
    WaitFailed {
        kind: io::ErrorKind,
        raw_os_error: Option<i32>,
    },
}

impl PrimaryFailure {
    fn reason(&self, capture: &SharedCapture) -> String {
        if let Some(details) = capture.limit_event() {
            return format!("output_limit_exceeded:{}", details.summary());
        }
        if let Some(details) = capture.reader_failure() {
            return format!("reader_failed:{}", details.summary());
        }
        match self {
            Self::None => "normal_exit".to_string(),
            Self::OutputLimit(details) => format!("output_limit_exceeded:{}", details.summary()),
            Self::Reader(details) => format!("reader_failed:{}", details.summary()),
            Self::TimedOut => "timeout".to_string(),
            Self::WaitFailed { kind, raw_os_error } => {
                format!("wait_failed:kind={kind:?};raw_os_error={raw_os_error:?}")
            }
        }
    }
}

fn cleanup_process_tree(
    child: &mut Child,
    child_exited: bool,
    deadline: Instant,
) -> ProcessTerminationOutcome {
    let process_group = kill_process_group(child);
    let process = if child_exited {
        CleanupStep::AlreadyExited
    } else {
        kill_parent(child)
    };
    let wait = reap_child(child, child_exited, deadline);
    ProcessTerminationOutcome {
        process_group,
        process,
        wait,
        readers: CleanupStep::NotRequired,
    }
}

fn reap_child(child: &mut Child, already_exited: bool, deadline: Instant) -> CleanupStep {
    if already_exited {
        return match child.wait() {
            Ok(_) => CleanupStep::Reaped,
            Err(error) => CleanupStep::Failed {
                kind: error.kind(),
                raw_os_error: error.raw_os_error(),
            },
        };
    }
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return CleanupStep::Reaped,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => return CleanupStep::TimedOut,
            Err(error) => {
                return CleanupStep::Failed {
                    kind: error.kind(),
                    raw_os_error: error.raw_os_error(),
                }
            }
        }
    }
}

fn kill_parent(child: &mut Child) -> CleanupStep {
    match child.kill() {
        Ok(()) => CleanupStep::Requested,
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => CleanupStep::AlreadyExited,
        Err(error) if error.kind() == io::ErrorKind::NotFound => CleanupStep::AlreadyGone,
        Err(error) => CleanupStep::Failed {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
        },
    }
}

fn configure_process_containment(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // `setsid` is async-signal-safe and runs between fork and exec. The
        // resulting session/process-group id is the child pid, allowing cleanup
        // to terminate CLI descendants without touching unrelated processes.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

fn kill_process_group(child: &Child) -> CleanupStep {
    #[cfg(unix)]
    {
        let process_group_id = child.id() as i32;
        if process_group_id > 0 {
            let result = unsafe { libc::kill(-process_group_id, libc::SIGKILL) };
            if result == 0 {
                return CleanupStep::Requested;
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                return CleanupStep::AlreadyGone;
            }
            return CleanupStep::Failed {
                kind: error.kind(),
                raw_os_error: error.raw_os_error(),
            };
        }
        CleanupStep::Failed {
            kind: io::ErrorKind::InvalidInput,
            raw_os_error: None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child;
        CleanupStep::Unsupported
    }
}

pub fn process_tree_containment_supported() -> bool {
    cfg!(unix)
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

    fn limits(stdout_bytes: usize, stderr_bytes: usize, combined_bytes: usize) -> OutputLimits {
        OutputLimits {
            stdout_bytes,
            stderr_bytes,
            combined_bytes,
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_spawn_with_timeout_fast_exit() {
        let mut cmd = Command::new("true");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_ok());
        assert!(result.unwrap().status.success());
    }

    #[cfg(unix)]
    #[test]
    fn test_spawn_with_timeout_failing_command() {
        let mut cmd = Command::new("false");
        let result = spawn_with_timeout(&mut cmd, 5000);
        assert!(result.is_ok());
        assert!(!result.unwrap().status.success());
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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
    fn spawn_with_timeout_retains_stdout_below_limit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf stdout-ok"]);
        let output = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(32, 32, 64))
            .expect("stdout below limit");
        assert_eq!(output.stdout, b"stdout-ok");
        assert!(output.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_retains_stderr_below_limit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf stderr-ok >&2"]);
        let output = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(32, 32, 64))
            .expect("stderr below limit");
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"stderr-ok");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_accepts_exact_stream_boundary() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 1234"]);
        let output = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(4, 4, 8))
            .expect("exact stream boundary");
        assert_eq!(output.stdout, b"1234");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_rejects_stdout_over_limit_without_partial_output() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf secret-output"]);
        let error = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(4, 32, 64))
            .expect_err("stdout over limit");
        let SpawnWithTimeoutError::OutputLimitExceeded { details, .. } = error else {
            panic!("expected stdout output-limit error");
        };
        assert_eq!(details.stream, OutputStream::Stdout);
        assert_eq!(details.configured_limit_bytes, 4);
        assert_eq!(details.observed_bytes, 5);
        assert_eq!(details.combined_observed_bytes, 13);
        assert!(!details.summary().contains("secret-output"));
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_rejects_stderr_over_limit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf stderr-overflow >&2"]);
        let error = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(32, 4, 64))
            .expect_err("stderr over limit");
        let SpawnWithTimeoutError::OutputLimitExceeded { details, .. } = error else {
            panic!("expected stderr output-limit error");
        };
        assert_eq!(details.stream, OutputStream::Stderr);
        assert_eq!(details.configured_limit_bytes, 4);
        assert_eq!(details.observed_bytes, 5);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_rejects_combined_limit_overflow() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf abc; printf def >&2"]);
        let error = spawn_with_timeout_with_limits(&mut cmd, 5000, limits(16, 16, 5))
            .expect_err("combined output over limit");
        let SpawnWithTimeoutError::OutputLimitExceeded { details, .. } = error else {
            panic!("expected combined output-limit error");
        };
        assert_eq!(details.stream, OutputStream::Combined);
        assert_eq!(details.configured_limit_bytes, 5);
        assert_eq!(details.observed_bytes, 6);
        assert_eq!(details.combined_observed_bytes, 6);
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
    fn spawn_with_timeout_terminates_descendants_after_parent_exit() {
        // The shell exits immediately, while the background process keeps the
        // inherited stdout pipe open. The process-group cleanup must close it
        // instead of making reader joins wait for the background process.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 60 & exit 0"]);
        let started = Instant::now();
        let result = spawn_with_timeout(&mut cmd, 2_000).expect("parent should exit cleanly");
        assert!(result.status.success());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "descendant containment must not extend the bounded wait"
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_kills_descendants_on_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("descendant.pid");
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            &format!("sleep 60 & echo $! > '{}'; wait", pid_path.display()),
        ]);
        let error = spawn_with_timeout_with_limits(&mut cmd, 250, limits(32, 32, 64))
            .expect_err("timeout should terminate the process tree");
        assert!(matches!(error, SpawnWithTimeoutError::TimedOut { .. }));
        let pid = std::fs::read_to_string(&pid_path)
            .expect("descendant pid was written")
            .trim()
            .parse::<libc::pid_t>()
            .expect("valid descendant pid");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && unsafe { libc::kill(pid, 0) } == 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "descendant still alive");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_with_timeout_large_stderr_does_not_deadlock() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "dd if=/dev/zero bs=1024 count=256 status=none >&2"]);
        let output = spawn_with_timeout_with_limits(
            &mut cmd,
            5000,
            limits(512 * 1024, 512 * 1024, 1024 * 1024),
        )
        .expect("large stderr should complete");
        assert_eq!(output.stderr.len(), 256 * 1024);
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

    #[cfg(unix)]
    #[test]
    fn process_tree_containment_is_admitted_on_unix() {
        assert!(process_tree_containment_supported());
    }

    #[test]
    fn reader_failure_reason_is_not_timeout() {
        struct FailingReader;

        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected reader failure",
                ))
            }
        }

        let capture = Arc::new(SharedCapture::new(limits(32, 32, 64), Instant::now()));
        let reader = spawn_pipe_reader(OutputStream::Stdout, Some(FailingReader), &capture)
            .expect("reader thread");
        reader.join().expect("reader thread should finish");
        let details = capture
            .reader_failure()
            .expect("reader failure should be recorded");
        assert_eq!(details.stream, OutputStream::Stdout);

        let error = SpawnWithTimeoutError::ReaderFailed {
            stream: OutputStream::Stdout,
            kind: details.kind,
            raw_os_error: details.raw_os_error,
            termination: ProcessTerminationOutcome::default(),
        };
        assert_eq!(error.reason_code(), "stdout_reader_failed");
        assert_ne!(error.reason_code(), "timeout");
    }

    #[cfg(not(unix))]
    #[test]
    fn process_tree_containment_is_rejected_before_spawn_on_unsupported_platform() {
        assert!(!process_tree_containment_supported());
        let mut cmd = Command::new("definitely-not-started");
        assert!(matches!(
            spawn_with_timeout(&mut cmd, 5000),
            Err(SpawnWithTimeoutError::ProcessTreeContainmentUnsupported)
        ));
    }
}
