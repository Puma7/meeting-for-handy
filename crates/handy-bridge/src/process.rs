use crate::{BridgeError, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    let mut child = command.spawn().map_err(|_| BridgeError::QueryFailed)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().ok_or(BridgeError::QueryFailed)?;
    let stderr = child.stderr.take().ok_or(BridgeError::QueryFailed)?;
    let stdout_overflow = overflow.clone();
    let stderr_overflow = overflow.clone();
    let out =
        std::thread::spawn(move || read_bounded(stdout, limits.stdout_bytes, stdout_overflow));
    let err =
        std::thread::spawn(move || read_bounded(stderr, limits.stderr_bytes, stderr_overflow));
    let started = Instant::now();
    let result = loop {
        if cancelled() {
            break Err(BridgeError::Cancelled);
        }
        if overflow.load(Ordering::Acquire) {
            break Err(BridgeError::OutputLimit);
        }
        if started.elapsed() >= limits.timeout {
            break Err(BridgeError::Timeout);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(BridgeError::QueryFailed)
                }
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break Err(BridgeError::QueryFailed),
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let stdout = out.join().map_err(|_| BridgeError::QueryFailed)??;
    let _ = err.join().map_err(|_| BridgeError::QueryFailed)??;
    result?;
    if cancelled() {
        return Err(BridgeError::Cancelled);
    }
    if overflow.load(Ordering::Acquire) {
        return Err(BridgeError::OutputLimit);
    }
    Ok(stdout)
}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(output);
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
        if count > remaining {
            overflow.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_pipe_output_is_consumed_without_unbounded_storage() {
        let overflow = Arc::new(AtomicBool::new(false));
        let output = read_bounded(
            std::io::Cursor::new(vec![b'x'; 1024 * 1024]),
            4096,
            overflow.clone(),
        )
        .unwrap();
        assert_eq!(output.len(), 4096);
        assert!(overflow.load(Ordering::Acquire));
        let exact = Arc::new(AtomicBool::new(false));
        assert_eq!(
            read_bounded(std::io::Cursor::new(b"[]"), 2, exact.clone()).unwrap(),
            b"[]"
        );
        assert!(!exact.load(Ordering::Acquire));
    }
}
