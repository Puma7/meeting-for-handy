//! Own a foreground gocryptfs process and identify its exact mount in this namespace.

use crate::Result;
use std::{
    fs,
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileTypeExt, MetadataExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const MOUNTINFO: &str = "/proc/self/mountinfo";
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const START_LIMIT: Duration = Duration::from_secs(45);
const STOP_LIMIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const UNAVAILABLE: &str = "Install gocryptfs and FUSE and run the application as a regular Linux user. / Bitte gocryptfs und FUSE installieren und die App als normaler Linux-Benutzer starten.";
const NOT_MOUNTED: &str = "The encrypted Linux profile is unavailable or its mount changed. / Das verschlüsselte Linux-Profil ist nicht verfügbar oder sein Einhängepunkt hat sich geändert.";
const WRONG_PASSWORD: &str = "The encrypted Linux profile could not be unlocked with this passphrase. / Das verschlüsselte Linux-Profil konnte mit dieser Passphrase nicht entsperrt werden.";
const FUSE_FAILED: &str = "Linux could not mount the encrypted profile. Check that the system FUSE helper and /dev/fuse are usable. / Linux konnte das verschlüsselte Profil nicht einhängen. Bitte das FUSE-Einhängeprogramm und /dev/fuse prüfen.";
const DETACH_FAILED: &str = "The encrypted Linux profile is still mounted. Close applications using the profile before unmounting. / Das verschlüsselte Linux-Profil ist noch eingehängt. Bitte zuerst Programme schließen, die das Profil verwenden.";
const CHILD_STILL_RUNNING: &str = "The encrypted Linux profile is still closing. Close files open from the profile and retry. / Das verschlüsselte Linux-Profil wird noch geschlossen. Bitte geöffnete Dateien aus dem Profil schließen und erneut versuchen.";
const ALREADY_MOUNTED: &str = "This Linux vault is still mounted. Close the existing mount before reopening it. / Dieses Linux-Profil ist noch eingehängt. Bitte den bestehenden Einhängepunkt vor dem erneuten Öffnen aushängen.";

fn system_tool(name: &str) -> Option<PathBuf> {
    SYSTEM_PATH.split(':').find_map(|directory| {
        let path = Path::new(directory).join(name);
        let meta = fs::metadata(&path).ok()?;
        (meta.is_file() && meta.uid() == 0 && meta.mode() & 0o022 == 0 && meta.mode() & 0o111 != 0)
            .then_some(path)
    })
}

fn unmount_tool() -> Option<PathBuf> {
    system_tool("fusermount3").or_else(|| system_tool("fusermount"))
}

fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

pub(super) fn available() -> bool {
    effective_uid().is_some_and(|uid| uid != 0)
        && system_tool("gocryptfs").is_some()
        && unmount_tool().is_some()
        && fs::metadata("/dev/fuse").is_ok_and(|meta| meta.file_type().is_char_device())
        && fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/fuse")
            .is_ok()
        && fs::File::open(MOUNTINFO).is_ok()
}

fn command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .env_clear()
        .env("PATH", SYSTEM_PATH)
        // The helper handles Ctrl-C and SIGTERM by trying a lazy unmount.
        .process_group(0)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn validate_password(password: &str) -> Result<()> {
    if password.is_empty() || password.len() > 2048 || password.contains(['\n', '\r', '\0']) {
        return Err("Enter a single-line profile passphrase. / Bitte eine einzeilige Profil-Passphrase eingeben.".into());
    }
    Ok(())
}

fn start(command: &mut Command, password: &str) -> io::Result<Child> {
    let mut child = command.stdin(Stdio::piped()).spawn()?;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("Could not open profile passphrase pipe"))
        .and_then(|mut input| input.write_all(password.as_bytes()));
    if let Err(error) = written {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

fn wait_until(child: &mut Child, limit: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

pub(super) fn create(vault: &Path, password: &str) -> Result<()> {
    validate_password(password)?;
    if !available() {
        return Err(UNAVAILABLE.into());
    }
    let tool = system_tool("gocryptfs").ok_or(UNAVAILABLE)?;
    let cipher = vault.join("cipher");
    fs::DirBuilder::new().mode(0o700).create(&cipher)?;
    let mut child = start(
        command(&tool).args(["-init", "-q", "--"]).arg(cipher),
        password,
    )?;
    match wait_until(&mut child, START_LIMIT) {
        Ok(Some(status)) if status.success() => Ok(()),
        Ok(Some(_)) => Err("Could not create the encrypted Linux profile. / Das verschlüsselte Linux-Profil konnte nicht erstellt werden.".into()),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("Creating the encrypted Linux profile did not finish. / Die Erstellung des verschlüsselten Linux-Profils wurde nicht abgeschlossen.".into())
        }
    }
}

fn mount_path(path: &Path) -> Vec<u8> {
    let mut encoded = Vec::new();
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b' ' => encoded.extend_from_slice(b"\\040"),
            b'\t' => encoded.extend_from_slice(b"\\011"),
            b'\n' => encoded.extend_from_slice(b"\\012"),
            b'\\' => encoded.extend_from_slice(b"\\134"),
            byte => encoded.push(*byte),
        }
    }
    encoded
}

fn mount_id(table: &[u8], mount: &Path, source: &[u8], uid: u32) -> Result<Option<u64>> {
    let encoded = mount_path(mount);
    let owner = format!("user_id={uid}");
    let mut mounted = None;
    for record in table.split(|byte| *byte == b'\n') {
        let mut fields = record.split(|byte| *byte == b' ');
        let Some(id) = fields.next() else {
            continue;
        };
        let mut fields = fields.skip(2);
        let root = fields.next();
        if fields.next() != Some(encoded.as_slice()) {
            continue;
        }
        let flags = fields.next().ok_or(NOT_MOUNTED)?;
        if mounted.is_some() || root != Some(b"/") || !fields.any(|field| field == b"-") {
            return Err(NOT_MOUNTED.into());
        }
        if fields.next() != Some(b"fuse.gocryptfs") || fields.next() != Some(source) {
            return Err(NOT_MOUNTED.into());
        }
        for required in [b"rw".as_slice(), b"nodev", b"nosuid", b"noexec"] {
            if !flags
                .split(|byte| *byte == b',')
                .any(|flag| flag == required)
            {
                return Err(NOT_MOUNTED.into());
            }
        }
        let super_flags = fields.next().ok_or(NOT_MOUNTED)?;
        let mut super_flags = super_flags.split(|byte| *byte == b',');
        if super_flags
            .clone()
            .any(|flag| flag == b"allow_other" || flag == b"allow_root")
            || !super_flags.any(|flag| flag == owner.as_bytes())
        {
            return Err(NOT_MOUNTED.into());
        }
        mounted = Some(std::str::from_utf8(id)?.parse::<u64>()?);
    }
    Ok(mounted)
}

fn has_source(table: &[u8], source: &[u8]) -> bool {
    table.split(|byte| *byte == b'\n').any(|record| {
        let mut fields = record.split(|byte| *byte == b' ').skip(6);
        fields.any(|field| field == b"-")
            && fields.next() == Some(b"fuse.gocryptfs")
            && fields.next() == Some(source)
    })
}

pub(super) struct Attachment {
    child: Child,
    mount: PathBuf,
    source: String,
    mount_id: Option<u64>,
    uid: u32,
    detached: bool,
}

impl Attachment {
    fn mounted(&self) -> Result<Option<u64>> {
        mount_id(
            &fs::read(MOUNTINFO)?,
            &self.mount,
            self.source.as_bytes(),
            self.uid,
        )
    }

    pub(super) fn check(&self, mount: &Path) -> Result<()> {
        if !self.detached
            && mount == self.mount
            && self.mount_id.is_some()
            && self.mounted()? == self.mount_id
        {
            Ok(())
        } else {
            Err(NOT_MOUNTED.into())
        }
    }

    pub(super) fn detach(&mut self, mount: &Path) -> Result<()> {
        if mount != self.mount {
            return Err(NOT_MOUNTED.into());
        }
        self.unmount(STOP_LIMIT)
    }

    fn unmount(&mut self, child_wait: Duration) -> Result<()> {
        if self.detached {
            return Ok(());
        }
        if let Some(id) = self.mounted()? {
            if self.mount_id.is_some_and(|expected| id != expected) {
                return Err(NOT_MOUNTED.into());
            }
            let tool = unmount_tool().ok_or(DETACH_FAILED)?;
            let mut helper = command(&tool)
                .stdin(Stdio::null())
                .args(["-u", "--"])
                .arg(&self.mount)
                .spawn()?;
            match wait_until(&mut helper, STOP_LIMIT) {
                Ok(Some(status)) if status.success() => {}
                Ok(Some(_)) => return Err(DETACH_FAILED.into()),
                _ => {
                    let _ = helper.kill();
                    let _ = helper.wait();
                    return Err(DETACH_FAILED.into());
                }
            }
            if self.mounted()?.is_some() {
                return Err(DETACH_FAILED.into());
            }
        }
        // Let gocryptfs finish flushing and releasing its keys. An external lazy
        // unmount may still have users even when the mount no longer appears in mountinfo.
        if wait_until(&mut self.child, child_wait)?.is_none() {
            return Err(CHILD_STILL_RUNNING.into());
        }
        self.detached = true;
        Ok(())
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        let _ = self.unmount(STOP_LIMIT);
    }
}

pub(super) fn attach(vault: &Path, mount: &Path, password: &str) -> Result<Attachment> {
    validate_password(password)?;
    if !available() {
        return Err(UNAVAILABLE.into());
    }
    let uid = effective_uid().filter(|uid| *uid != 0).ok_or(UNAVAILABLE)?;
    let tool = system_tool("gocryptfs").ok_or(UNAVAILABLE)?;
    let cipher = vault.join("cipher");
    let directory = fs::symlink_metadata(&cipher)?;
    if !directory.is_dir() || directory.uid() != uid || directory.mode() & 0o022 != 0 {
        return Err(NOT_MOUNTED.into());
    }
    let config = fs::symlink_metadata(cipher.join("gocryptfs.conf"))?;
    if !config.is_file() || config.uid() != uid || config.mode() & 0o022 != 0 {
        return Err(NOT_MOUNTED.into());
    }
    // A leftover daemon can outlive its window or a failed normal unmount.
    // The stable backing identity prevents opening a second mount of that vault.
    let source = format!("handymeeting-{:x}-{:x}", directory.dev(), directory.ino());
    let table = fs::read(MOUNTINFO)?;
    if has_source(&table, source.as_bytes()) {
        return Err(ALREADY_MOUNTED.into());
    }
    // The new mountpoint itself must also have no preexisting mount.
    mount_id(&table, mount, source.as_bytes(), uid)?;
    let child = start(
        command(&tool)
            .args(["-fg", "-q", "-rw", "-nosuid", "-nodev", "-noexec"])
            .arg(format!("-fsname={source}"))
            .arg("--")
            .arg(cipher)
            .arg(mount),
        password,
    )?;
    let mut attached = Attachment {
        child,
        mount: mount.to_owned(),
        source,
        mount_id: None,
        uid,
        detached: false,
    };
    let deadline = Instant::now() + START_LIMIT;
    loop {
        if let Some(status) = attached.child.try_wait()? {
            return Err(if status.code() == Some(12) {
                WRONG_PASSWORD
            } else {
                FUSE_FAILED
            }
            .into());
        }
        if let Some(id) = attached.mounted()? {
            attached.mount_id = Some(id);
            return Ok(attached);
        }
        if Instant::now() >= deadline {
            // No match was found. Stop only our startup child and let its drop
            // guard attempt an ordinary unmount if it mounted in the meantime.
            let _ = attached.child.kill();
            let _ = attached.child.wait();
            return Err(FUSE_FAILED.into());
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_exact_private_mount() {
        let path = Path::new("/tmp/meeting files\\one");
        let owned = b"18 10 0:41 / /tmp/meeting\\040files\\134one rw,nosuid,nodev,noexec shared:2 - fuse.gocryptfs handymeeting-own rw,user_id=1000,group_id=1000\n";
        assert_eq!(
            mount_id(owned, path, b"handymeeting-own", 1000).unwrap(),
            Some(18)
        );
        assert!(mount_id(owned, path, b"handymeeting-other", 1000).is_err());
        assert!(mount_id(owned, path, b"handymeeting-own", 1001).is_err());
        assert_eq!(
            mount_id(
                owned,
                Path::new("/tmp/meeting files"),
                b"handymeeting-own",
                1000
            )
            .unwrap(),
            None
        );
        assert!(mount_id(
            &[owned.as_slice(), owned.as_slice()].concat(),
            path,
            b"handymeeting-own",
            1000
        )
        .is_err());

        let shared = b"18 10 0:41 / /tmp/meeting\\040files\\134one rw,nosuid,nodev,noexec - fuse.gocryptfs handymeeting-own rw,allow_other,user_id=1000\n";
        assert!(mount_id(shared, path, b"handymeeting-own", 1000).is_err());
        assert!(has_source(shared, b"handymeeting-own"));
        assert!(!has_source(shared, b"handymeeting-other"));
    }

    #[test]
    fn absent_mount_cannot_complete_detach_while_its_child_is_alive() {
        let mount = tempfile::tempdir().unwrap();
        // The fixture process waits on a pipe we own. No FUSE mount or signal is needed.
        let mut child = Command::new("/bin/sh")
            .args(["-c", "IFS= read -r line"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut attachment = Attachment {
            child,
            mount: mount.path().to_owned(),
            source: "handymeeting-absent-fixture".into(),
            mount_id: Some(1),
            uid: effective_uid().unwrap(),
            detached: false,
        };
        assert_eq!(attachment.mounted().unwrap(), None);
        let first = attachment.unmount(Duration::ZERO);
        let prematurely_detached = attachment.detached;

        input.write_all(b"\n").unwrap();
        drop(input);
        let retry = attachment.unmount(STOP_LIMIT);
        assert!(attachment.child.wait().unwrap().success());

        assert_eq!(first.unwrap_err().to_string(), CHILD_STILL_RUNNING);
        assert!(!prematurely_detached);
        assert!(retry.is_ok());
        assert!(attachment.detached);
    }
}
