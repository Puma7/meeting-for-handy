//! Startup status over private pipes between the standard app and its protected child.

use crate::AppResult;
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Duration,
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use {std::io::BufRead, zeroize::Zeroizing};

pub(super) const FLAG: &str = "--startup-ack";
const READY: &[u8; 8] = b"HM-READY";
const FAILED: &[u8; 8] = b"HM-ERROR";
const ACCEPTED: &[u8; 7] = b"HM-OPEN";
const STARTUP_LIMIT: Duration = Duration::from_secs(90);
const ACK_LIMIT: Duration = Duration::from_secs(5);
#[cfg(any(target_os = "linux", target_os = "macos"))]
const START_FAILED: &str = "Das geschützte Profil konnte nicht geöffnet werden. Prüfe die Passphrase und ob das Profil bereits geöffnet ist. / The protected profile could not open. Check the passphrase and whether the profile is already open.";
#[cfg(windows)]
const START_FAILED: &str = "Das geschützte Profil konnte nicht geöffnet werden. Prüfe den Ordner und ob das Profil bereits geöffnet ist. / The protected profile could not open. Check the folder and whether the profile is already open.";
const START_TIMEOUT: &str = "Das geschützte Profil hat den Start nicht rechtzeitig bestätigt. Versuche es erneut, sobald der vorige Startversuch beendet ist. / The protected profile did not confirm startup in time. Retry once the previous attempt has ended.";

pub(crate) fn managed_startup(args: &[OsString]) -> bool {
    #[cfg(windows)]
    {
        matches!(args, [launch, _, startup] if launch == "--protected-profile" && startup == FLAG)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        matches!(args, [launch, _, password, startup] if launch == "--protected-profile" && password == "--password-stdin" && startup == FLAG)
    }
}

pub(super) fn launch(mut command: Command, password: Option<&str>) -> AppResult<()> {
    let mut child = command
        .arg(FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| START_FAILED)?;
    let pipes = (child.stdin.take(), child.stdout.take());
    let (Some(mut input), Some(mut output)) = pipes else {
        // Dropping the input cancels a managed startup. Never kill a child that may own a mount.
        drop(pipes);
        thread::spawn(move || {
            let _ = child.wait();
        });
        return Err(START_FAILED.into());
    };
    let (state, confirmation) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = state.send(read_status(&mut output));
        // Keep draining the child's stdout until it exits, even when the parent timed out.
        let _ = io::copy(&mut output, &mut io::sink());
        let _ = child.wait();
    });
    if let Some(password) = password {
        input
            .write_all(password.as_bytes())
            .and_then(|()| input.write_all(b"\n"))
            .map_err(|_| START_FAILED)?;
    }
    complete_startup(&mut input, confirmation, STARTUP_LIMIT)
}

fn read_status(mut output: impl Read) -> bool {
    let mut status = [0; READY.len()];
    output.read_exact(&mut status).is_ok() && &status == READY
}

fn complete_startup(
    mut input: impl Write,
    confirmation: Receiver<bool>,
    timeout: Duration,
) -> AppResult<()> {
    match confirmation.recv_timeout(timeout) {
        Ok(true) => input.write_all(ACCEPTED).map_err(|_| START_FAILED.into()),
        Err(RecvTimeoutError::Timeout) => Err(START_TIMEOUT.into()),
        _ => Err(START_FAILED.into()),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn read_passphrase(input: impl BufRead, managed: bool) -> AppResult<Zeroizing<String>> {
    const MAX_BYTES: usize = 1024;
    let mut passphrase = Zeroizing::new(String::new());
    if managed {
        input
            .take((MAX_BYTES + 1) as u64)
            .read_line(&mut passphrase)
            .map_err(|_| START_FAILED)?;
        if !passphrase.ends_with('\n') {
            return Err(START_FAILED.into());
        }
        passphrase.pop();
    } else {
        // Read one byte beyond the largest CRLF-framed input so it cannot be truncated
        // into a valid passphrase while leaving trailing bytes undiscovered.
        input
            .take((MAX_BYTES + 3) as u64)
            .read_to_string(&mut passphrase)
            .map_err(|_| START_FAILED)?;
        if passphrase.ends_with('\n') {
            passphrase.pop();
            if passphrase.ends_with('\r') {
                passphrase.pop();
            }
        }
        if passphrase.is_empty()
            || passphrase.len() > MAX_BYTES
            || passphrase.contains(['\0', '\r', '\n'])
        {
            return Err(START_FAILED.into());
        }
    }
    Ok(passphrase)
}

fn write_status(status: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(status)?;
    stdout.flush()
}

pub(crate) fn report_failure() {
    let _ = write_status(FAILED);
}

pub(crate) fn acknowledge_window() -> bool {
    if write_status(READY).is_err() {
        return false;
    }
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut response = [0; ACCEPTED.len()];
        let accepted =
            io::stdin().lock().read_exact(&mut response).is_ok() && &response == ACCEPTED;
        let _ = send.send(accepted);
    });
    receive.recv_timeout(ACK_LIMIT) == Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn managed_startup_consumes_only_the_bounded_passphrase_frame() {
        let mut input = Cursor::new(b"synthetic passphrase\nHM-OPEN");
        let phrase = read_passphrase(&mut input, true).unwrap();
        assert_eq!(phrase.as_str(), "synthetic passphrase");
        let mut ack = [0; ACCEPTED.len()];
        input.read_exact(&mut ack).unwrap();
        assert_eq!(&ack, ACCEPTED);
        assert!(read_passphrase(Cursor::new(b"incomplete"), true).is_err());
        assert!(read_passphrase(Cursor::new(vec![b'x'; 1025]), true).is_err());
        assert_eq!(
            read_passphrase(Cursor::new(b"direct passphrase"), false)
                .unwrap()
                .as_str(),
            "direct passphrase"
        );
    }

    #[cfg(unix)]
    #[test]
    fn direct_passphrase_accepts_at_most_one_trailing_line_ending() {
        for input in [
            b" exact spaces ".as_slice(),
            b" exact spaces \n",
            b" exact spaces \r\n",
        ] {
            assert_eq!(
                read_passphrase(Cursor::new(input), false).unwrap().as_str(),
                " exact spaces "
            );
        }
        for input in [
            b"".as_slice(),
            b"\n",
            b"abc\n\n",
            b"abc\r",
            b"abc\r\n\n",
            b"abc\rdef\n",
            b"abc\nnext",
            b"abc\0def",
            b"\xff\n",
        ] {
            assert!(read_passphrase(Cursor::new(input), false).is_err());
        }
        let max = "ü".repeat(512);
        for delimiter in ["", "\n", "\r\n"] {
            assert_eq!(
                read_passphrase(Cursor::new(format!("{max}{delimiter}")), false)
                    .unwrap()
                    .as_str(),
                max
            );
            assert!(read_passphrase(Cursor::new(format!("{max}x{delimiter}")), false).is_err());
        }
        assert!(read_passphrase(Cursor::new(format!("{max}\r\ntrailing bytes")), false).is_err());
    }

    #[test]
    fn startup_succeeds_only_after_receiving_the_full_ready_frame() {
        for (frame, ready) in [
            (READY.as_slice(), true),
            (FAILED.as_slice(), false),
            (&READY[..4], false),
            (b"unrelated", false),
        ] {
            let (send, receive) = mpsc::sync_channel(1);
            send.send(read_status(Cursor::new(frame))).unwrap();
            let mut input = Vec::new();
            assert_eq!(
                complete_startup(&mut input, receive, Duration::ZERO).is_ok(),
                ready
            );
            assert_eq!(
                input.as_slice(),
                if ready { ACCEPTED.as_slice() } else { &[] }
            );
        }
        let (_waiting_child, receive) = mpsc::sync_channel(1);
        let mut input = Vec::new();
        assert_eq!(
            complete_startup(&mut input, receive, Duration::ZERO).unwrap_err(),
            START_TIMEOUT
        );
        assert!(input.is_empty());
    }
}
