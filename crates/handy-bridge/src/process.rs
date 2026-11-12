use crate::{BridgeError, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const QUERY_POLL: Duration = Duration::from_millis(10);
const CHILD_REAP_GRACE: Duration = Duration::from_millis(100);
const MAX_PIPE_QUERIES: usize = 16;
static PIPE_QUERIES: AtomicUsize = AtomicUsize::new(0);

/// A descendant can hold our anonymous pipes open. Such a query still consumes one
/// slot until its blocked readers close, bounding outstanding buffers and threads.
#[derive(Default)]
struct QueryPipes {
    stop: AtomicBool,
    overflow: AtomicBool,
}

struct PipePermit(QueryPipes);

impl PipePermit {
    fn reserve() -> Result<Arc<Self>> {
        PIPE_QUERIES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_PIPE_QUERIES).then_some(count + 1)
            })
            .map_err(|_| BridgeError::QueryFailed)?;
        Ok(Arc::new(Self(QueryPipes::default())))
    }
}

impl Drop for PipePermit {
    fn drop(&mut self) {
        PIPE_QUERIES.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(45),
            stdout_bytes: 8 * 1024 * 1024,
            stderr_bytes: 64 * 1024,
        }
    }
}

pub trait ModelListRunner {
    fn list_models(&self, executable: &Path, limits: CommandLimits) -> Result<Vec<u8>>;
}

#[derive(Debug)]
pub struct SystemModelListRunner;

impl ModelListRunner for SystemModelListRunner {
    fn list_models(&self, executable: &Path, limits: CommandLimits) -> Result<Vec<u8>> {
        run_query(executable, &["--list-models", "--json"], limits)
    }
}

pub(crate) fn run_query(
    executable: &Path,
    arguments: &[&str],
    limits: CommandLimits,
) -> Result<Vec<u8>> {
    run_query_cancellable(executable, arguments, limits, &|| false)
}

pub(crate) fn run_query_cancellable(
    executable: &Path,
    arguments: &[&str],
    limits: CommandLimits,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<u8>> {
    if cancelled() {
        return Err(BridgeError::Cancelled);
    }
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW for this owned headless query only.
    }
    run_command_cancellable(&mut command, limits, cancelled)
}

fn run_command_cancellable(
    command: &mut Command,
    limits: CommandLimits,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<u8>> {
    let permit = PipePermit::reserve()?;
    if cancelled() {
        return Err(BridgeError::Cancelled);
    }
    let mut child = command.spawn().map_err(|_| BridgeError::QueryFailed)?;
    let started = Instant::now();
    let result = (|| {
        let stdout = child.stdout.take().ok_or(BridgeError::QueryFailed)?;
        let stderr = child.stderr.take().ok_or(BridgeError::QueryFailed)?;
        let out = spawn_reader(stdout, limits.stdout_bytes, Arc::clone(&permit))?;
        let err = spawn_reader(stderr, limits.stderr_bytes, Arc::clone(&permit))?;
        let mut exited = false;
        loop {
            if cancelled() {
                return Err(BridgeError::Cancelled);
            }
            if permit.0.overflow.load(Ordering::Acquire) {
                return Err(BridgeError::OutputLimit);
            }
            if started.elapsed() >= limits.timeout {
                return Err(BridgeError::Timeout);
            }
            if !exited {
                match child.try_wait() {
                    Ok(Some(status)) if status.success() => exited = true,
                    Ok(None) => {}
                    _ => return Err(BridgeError::QueryFailed),
                }
            }
            if exited && out.is_finished() && err.is_finished() {
                break;
            }
            thread::sleep(QUERY_POLL);
        }
        let stdout = out.join().map_err(|_| BridgeError::QueryFailed)??;
        let _ = err.join().map_err(|_| BridgeError::QueryFailed)??;
        if cancelled() {
            return Err(BridgeError::Cancelled);
        }
        if permit.0.overflow.load(Ordering::Acquire) {
            return Err(BridgeError::OutputLimit);
        }
        Ok(stdout)
    })();
    if result.is_err() {
        permit.0.stop.store(true, Ordering::Release);
        stop_owned_child(child, permit);
    }
    result
}

fn stop_owned_child(mut child: Child, permit: Arc<PipePermit>) {
    if child.try_wait().is_ok_and(|status| status.is_some()) {
        return;
    }
    let _ = child.kill();
    let started = Instant::now();
    while started.elapsed() < CHILD_REAP_GRACE {
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        thread::sleep(QUERY_POLL);
    }
    // Keep the same query permit while the OS finishes reaping our direct child.
    let _ = thread::Builder::new().spawn(move || {
        let _ = child.wait();
        drop(permit);
    });
}

fn spawn_reader(
    reader: impl Read + Send + 'static,
    limit: usize,
    permit: Arc<PipePermit>,
) -> Result<thread::JoinHandle<std::io::Result<Vec<u8>>>> {
    thread::Builder::new()
        .spawn(move || read_bounded(reader, limit, &permit.0))
        .map_err(|_| BridgeError::QueryFailed)
}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    pipes: &QueryPipes,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0; 8192];
    while !pipes.stop.load(Ordering::Acquire) {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(output);
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
        if count > remaining {
            pipes.overflow.store(true, Ordering::Release);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const PIPE_FIXTURE_DIR: &str = "HANDYMEETING_PIPE_QUERY_FIXTURE_DIR";
    const PIPE_FIXTURE_MODE: &str = "HANDYMEETING_PIPE_QUERY_FIXTURE_MODE";

    #[test]
    fn cancelled_timeout_and_overflow_queries_do_not_join_inherited_pipes() {
        for (mode, expected) in [
            ("cancel", BridgeError::Cancelled),
            ("timeout", BridgeError::Timeout),
            ("overflow", BridgeError::OutputLimit),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let ready = directory.path().join("ready");
            let finished = directory.path().join("finished");
            let mut command = pipe_fixture_command(directory.path(), mode);
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let started = Instant::now();
            let result = run_command_cancellable(
                &mut command,
                CommandLimits {
                    timeout: if mode == "timeout" {
                        Duration::from_secs(2)
                    } else {
                        Duration::from_secs(5)
                    },
                    stdout_bytes: 4096,
                    stderr_bytes: 4096,
                },
                &|| mode == "cancel" && ready.is_file(),
            );
            let elapsed = started.elapsed();
            let descendant_was_waiting = ready.is_file() && !finished.is_file();
            // Give the fixture's descendant a normal exit; the query must not
            // have terminated it or depended on this release to return.
            std::fs::write(directory.path().join("release"), []).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !finished.is_file() && Instant::now() < deadline {
                thread::sleep(QUERY_POLL);
            }
            assert!(descendant_was_waiting && finished.is_file(), "{mode}");
            assert!(elapsed < Duration::from_secs(5), "{mode}: {elapsed:?}");
            assert_eq!(
                std::mem::discriminant(&result.unwrap_err()),
                std::mem::discriminant(&expected),
                "{mode}"
            );
        }
    }

    fn pipe_fixture_command(directory: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process::tests::inherited_pipe_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env(PIPE_FIXTURE_DIR, directory)
            .env(PIPE_FIXTURE_MODE, mode)
            .stdin(Stdio::null());
        command
    }

    #[test]
    #[ignore = "Run only as the owned inherited-pipe test child"]
    fn inherited_pipe_fixture() {
        let Some(directory) = std::env::var_os(PIPE_FIXTURE_DIR) else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        let mode = std::env::var(PIPE_FIXTURE_MODE).unwrap();
        if mode == "descendant" {
            std::fs::write(directory.join("ready"), []).unwrap();
            let deadline = Instant::now() + Duration::from_secs(8);
            while !directory.join("release").is_file() && Instant::now() < deadline {
                thread::sleep(QUERY_POLL);
            }
            let _ = std::fs::write(directory.join("finished"), []);
            return;
        }
        let mut child = pipe_fixture_command(&directory, "descendant")
            .spawn()
            .unwrap();
        if mode == "timeout" {
            std::process::exit(0);
        }
        if mode == "overflow" {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !directory.join("ready").is_file() && Instant::now() < deadline {
                thread::sleep(QUERY_POLL);
            }
            std::io::stderr().write_all(&[b'x'; 8192]).unwrap();
        }
        child.wait().unwrap();
    }

    #[test]
    fn oversized_pipe_output_is_consumed_without_unbounded_storage() {
        let overflow = QueryPipes::default();
        let output = read_bounded(
            std::io::Cursor::new(vec![b'x'; 1024 * 1024]),
            4096,
            &overflow,
        )
        .unwrap();
        assert_eq!(output.len(), 4096);
        assert!(overflow.overflow.load(Ordering::Acquire));
        let exact = QueryPipes::default();
        assert_eq!(
            read_bounded(std::io::Cursor::new(b"[]"), 2, &exact).unwrap(),
            b"[]"
        );
        assert!(!exact.overflow.load(Ordering::Acquire));
    }
}
