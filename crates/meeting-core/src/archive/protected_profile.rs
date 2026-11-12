//! Migrate authoritative profile files into storage that encrypts native writes.
use super::{digest_cancellable, metadata, operation_lock, publication_lease, reject_link};
use crate::{storage, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[cfg(test)]
mod tests;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod volume;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use volume::{migrate_volume, migrate_volume_cancellable, open_volume, MountedVolume};

const MARKER: &str = "protected-profile.v1";
const VAULT_MARKER: &str = "protected-volume.v1";
const DIRECTORY_PREFIX: &str = "HandyMeeting-Protected-";
const MARKER_CONTENT: &[u8] = b"HandyMeeting Windows EFS profile v1\n";
const PROFILE_DATA: &[&str] = &[
    "sessions",
    "archives",
    "audio-copies",
    "immediate-deletions",
    "legacy-map",
    "maintenance-log",
    "retention-plans",
    "retention-settings-history",
    "speaker-labels",
    "speaker-model-checks",
    "speaker-model-check-archives",
    "templates",
    "user-metadata",
];
const MAX_ENTRIES: usize = 1_000_000;
const PROFILE_FILES: &[&str] = &["retention-settings.json", "retention-settings.json.bak"];
const MAX_DEPTH: usize = 48;
const UNAVAILABLE: &str = "Windows file encryption is unavailable for this folder. Choose a local EFS-capable NTFS folder. / Windows-Dateiverschlüsselung ist hier nicht verfügbar. Bitte einen lokalen NTFS-Ordner mit EFS wählen.";
const NOT_PROTECTED: &str = "The protected profile is missing or contains unencrypted files. Check the folder and your Windows encryption key. / Das geschützte Profil fehlt oder enthält unverschlüsselte Dateien. Bitte Ordner und Windows-Schlüssel prüfen.";

pub fn supported() -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        volume::available()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        cfg!(target_os = "windows")
    }
}

pub fn provider_name() -> Option<&'static str> {
    match std::env::consts::OS {
        "windows" => Some("windowsEfs"),
        "macos" => Some("macosImage"),
        "linux" => Some("linuxGocryptfs"),
        _ => None,
    }
}

/// GUI processes cooperate even before a recording acquires its individual session lease.
pub fn reserve_runtime(root: &Path) -> Result<File> {
    let file = metadata::open_lock_file(&root.join("desktop-profile.lock"))?;
    metadata::lock_soon(&file).map_err(|_| {
        "This profile is already open. Close its other window first. / Dieses Profil ist bereits geöffnet. Bitte zuerst das andere Fenster schließen."
    })?;
    Ok(file)
}

pub fn needs_verification(root: &Path) -> Result<bool> {
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(DIRECTORY_PREFIX))
    {
        return Ok(true);
    }
    Ok(storage::exists_no_link(&root.join(MARKER))?
        || storage::exists_no_link(&root.join(VAULT_MARKER))?)
}

/// Check a marked profile before the application or its webview writes to it.
pub fn verify(root: &Path) -> Result<()> {
    verify_with(root, &Efs)
}

#[derive(Serialize)]
pub struct Receipt {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug)]
pub struct MigrationCancelled;

impl std::fmt::Display for MigrationCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Profile copy cancelled. The original profile was kept. / Profilkopie abgebrochen. Das Originalprofil bleibt erhalten.")
    }
}

impl std::error::Error for MigrationCancelled {}

/// Keep cancellation atomic with the final publication of a verified copy.
#[derive(Default)]
pub struct MigrationControl {
    cancelled: AtomicBool,
    publishing: Mutex<bool>,
}

impl MigrationControl {
    pub fn cancel(&self) -> bool {
        let Ok(publishing) = self.publishing.lock() else {
            return false;
        };
        if *publishing {
            return false;
        }
        self.cancelled.store(true, Ordering::Release);
        true
    }

    pub fn cancellation_requested(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<()> {
        if self.cancellation_requested() {
            return Err(MigrationCancelled.into());
        }
        Ok(())
    }

    fn begin_publication(&self) -> Result<()> {
        let mut publishing = self
            .publishing
            .lock()
            .map_err(|_| "Profile migration control is unavailable")?;
        self.check()?;
        *publishing = true;
        Ok(())
    }

    fn digest(&self, path: &Path) -> Result<(u64, String)> {
        self.check()?;
        let digest = digest_cancellable(path, Some(&self.cancelled));
        self.check()?;
        digest
    }
}

pub fn migrate(source: &Path, parent: &Path) -> Result<Receipt> {
    migrate_cancellable(source, parent, &MigrationControl::default())
}

pub fn migrate_cancellable(
    source: &Path,
    parent: &Path,
    control: &MigrationControl,
) -> Result<Receipt> {
    migrate_with(source, parent, &Efs, control)
}

trait Protection {
    fn enable_directory(&self, directory: &Path) -> Result<()>;
    fn check(&self, path: &Path) -> Result<()>;
}

struct Efs;

impl Protection for Efs {
    fn enable_directory(&self, directory: &Path) -> Result<()> {
        platform::encrypt(directory)?;
        self.check(directory)
    }

    fn check(&self, path: &Path) -> Result<()> {
        if !platform::encrypted(path)? {
            return Err(NOT_PROTECTED.into());
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    File(u64, String),
}

fn walk(
    path: &Path,
    depth: usize,
    remaining: &mut usize,
    control: Option<&MigrationControl>,
    visit: &mut impl FnMut(&Path, bool) -> Result<bool>,
) -> Result<()> {
    if let Some(control) = control {
        control.check()?;
    }
    if depth > MAX_DEPTH || *remaining == 0 {
        return Err("The profile exceeds the migration inventory limit. / Das Profil überschreitet die Grenze für die Kopierprüfung.".into());
    }
    *remaining -= 1;
    reject_link(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() && !meta.is_file() {
        return Err("The profile contains an unsupported filesystem entry.".into());
    }
    if visit(path, meta.is_dir())? && meta.is_dir() {
        for child in fs::read_dir(path)? {
            walk(&child?.path(), depth + 1, remaining, control, visit)?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn inventory(root: &Path) -> Result<BTreeMap<PathBuf, Entry>> {
    inventory_cancellable(root, &MigrationControl::default())
}

fn inventory_cancellable(
    root: &Path,
    control: &MigrationControl,
) -> Result<BTreeMap<PathBuf, Entry>> {
    let mut entries = BTreeMap::new();
    for name in PROFILE_FILES {
        control.check()?;
        let path = root.join(name);
        if storage::exists_no_link(&path)? {
            let (bytes, hash) = control.digest(&path)?;
            entries.insert(PathBuf::from(name), Entry::File(bytes, hash));
        }
    }
    let mut remaining = MAX_ENTRIES;
    for name in PROFILE_DATA {
        control.check()?;
        let path = root.join(name);
        if !storage::exists_no_link(&path)? {
            continue;
        }
        metadata::checked_directory(&path)?;
        walk(
            &path,
            1,
            &mut remaining,
            Some(control),
            &mut |path, directory| {
                let relative = path.strip_prefix(root)?;
                if relative == Path::new("sessions/.leases")
                    || (!directory
                        && relative.components().count() == 3
                        && relative.starts_with("sessions")
                        && relative
                            .file_name()
                            .is_some_and(|name| name == "session.lock"))
                {
                    return Ok(false);
                }
                let entry = if directory {
                    Entry::Directory
                } else {
                    let (bytes, hash) = control.digest(path)?;
                    Entry::File(bytes, hash)
                };
                entries.insert(relative.to_owned(), entry);
                Ok(true)
            },
        )?;
    }
    Ok(entries)
}

fn verify_with(root: &Path, protection: &impl Protection) -> Result<()> {
    verify_marked(root, protection, MARKER_CONTENT)
}

fn verify_marked(root: &Path, protection: &impl Protection, marker_content: &[u8]) -> Result<()> {
    let root = checked_root(root).map_err(|_| NOT_PROTECTED)?;
    verify_marker(&root, protection, marker_content)?;
    verify_tree(&root, protection, MAX_ENTRIES, None)
}

fn verify_marker(root: &Path, protection: &impl Protection, marker_content: &[u8]) -> Result<()> {
    let marker = root.join(MARKER);
    reject_link(&marker).map_err(|_| NOT_PROTECTED)?;
    protection.check(&marker)?;
    let mut contents = Vec::new();
    File::open(&marker)?
        .take(marker_content.len() as u64 + 1)
        .read_to_end(&mut contents)?;
    if contents != marker_content {
        return Err(NOT_PROTECTED.into());
    }
    Ok(())
}

fn verify_tree(
    root: &Path,
    protection: &impl Protection,
    mut remaining: usize,
    control: Option<&MigrationControl>,
) -> Result<()> {
    walk(root, 0, &mut remaining, control, &mut |path, _| {
        protection.check(path)?;
        Ok(true)
    })
}

fn checked_root(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(
            "Choose an absolute local folder. / Bitte einen absoluten lokalen Ordner wählen."
                .into(),
        );
    }
    metadata::checked_directory(path)?;
    #[cfg(windows)]
    for parent in path.ancestors().skip(1).filter(|part| part.is_absolute()) {
        metadata::checked_directory(parent)?;
    }
    let canonical = path.canonicalize()?;
    #[cfg(windows)]
    if !platform::local_drive(&canonical) {
        return Err("Choose a folder on a local Windows drive. / Bitte einen Ordner auf einem lokalen Windows-Laufwerk wählen.".into());
    }
    Ok(canonical)
}

fn session_leases(
    root: &Path,
    control: &MigrationControl,
) -> Result<Vec<storage::RelocationGuard>> {
    let folder = root.join("sessions");
    metadata::checked_directory(&folder)?;
    let mut leases = Vec::new();
    for entry in fs::read_dir(folder)? {
        control.check()?;
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|id| id.len() == 32 && metadata::valid_id(id))
        {
            leases.push(storage::erasure_lease(&entry.path())?);
        }
    }
    Ok(leases)
}

fn migrate_with(
    source: &Path,
    parent: &Path,
    protection: &impl Protection,
    control: &MigrationControl,
) -> Result<Receipt> {
    control.check()?;
    let (source, parent) = migration_paths(source, parent)?;
    let stage = tempfile::Builder::new()
        .prefix(DIRECTORY_PREFIX)
        .tempdir_in(parent)?;
    let result = (|| {
        let root = stage.path();
        protection.enable_directory(root).map_err(|_| UNAVAILABLE)?;
        let copied = copy_profile(&source, root, protection, control)?;
        control.begin_publication()?;
        write_profile_marker(root, protection, MARKER_CONTENT)?;
        verify_marker(root, protection, MARKER_CONTENT)?;
        Ok(copied)
    })();
    let (stage, (files, bytes)) = finish_stage(stage, result)?;
    Ok(Receipt {
        path: stage.keep(),
        files,
        bytes,
    })
}

/// Called only once the stage is unmounted or was never mounted.
fn finish_stage<T>(stage: tempfile::TempDir, result: Result<T>) -> Result<(tempfile::TempDir, T)> {
    match result {
        Ok(value) => Ok((stage, value)),
        Err(error) => {
            let path = stage.path().to_owned();
            if let Err(cleanup) = stage.close() {
                return Err(format!("{error}\nCould not remove the unfinished encrypted copy at {}. / Die unfertige verschlüsselte Kopie unter {} konnte nicht entfernt werden. ({cleanup})", path.display(), path.display()).into());
            }
            Err(error)
        }
    }
}

fn migration_paths(source: &Path, parent: &Path) -> Result<(PathBuf, PathBuf)> {
    let source = checked_root(source)?;
    let parent = checked_root(parent)?;
    #[cfg(windows)]
    let nested = parent.ancestors().any(|path| {
        path.to_string_lossy()
            .eq_ignore_ascii_case(&source.to_string_lossy())
    });
    #[cfg(not(windows))]
    let nested = parent.starts_with(&source);
    if nested {
        return Err("Choose a folder outside the current profile. / Bitte einen Ordner außerhalb der aktiven Ablage wählen.".into());
    }
    Ok((source, parent))
}

fn copy_profile(
    source: &Path,
    root: &Path,
    protection: &impl Protection,
    control: &MigrationControl,
) -> Result<(usize, u64)> {
    control.check()?;
    let _archive = operation_lock(source)?;
    let _publication = publication_lease(source, true)?;
    let _sessions = session_leases(source, control)?;
    protection.check(root)?;
    let before = inventory_cancellable(source, control)?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    for (name, entry) in &before {
        control.check()?;
        let path = root.join(name);
        match entry {
            Entry::Directory => {
                fs::create_dir(&path)?;
                protection.check(&path)?;
            }
            Entry::File(length, expected) => {
                let original = source.join(name);
                reject_link(&original)?;
                let mut input = File::open(&original)?.take(length.saturating_add(1));
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                protection.check(&path)?;
                if copy_blocks(&mut input, &mut output, control)? != *length {
                    return Err("The source changed during migration. / Die Quelle hat sich beim Kopieren geändert.".into());
                }
                output.sync_all()?;
                drop(output);
                if control.digest(&path)? != (*length, expected.clone()) {
                    return Err("The copied file failed verification. / Eine kopierte Datei konnte nicht verifiziert werden.".into());
                }
                files += 1;
                bytes = bytes.checked_add(*length).ok_or("Profile size overflow")?;
            }
        }
    }
    if inventory_cancellable(source, control)? != before {
        return Err(
            "The source changed during migration. / Die Quelle hat sich beim Kopieren geändert."
                .into(),
        );
    }
    // The marker written at publication is counted when reopening the profile.
    verify_tree(root, protection, MAX_ENTRIES - 1, Some(control))?;
    Ok((files, bytes))
}

fn copy_blocks(
    input: &mut impl Read,
    output: &mut impl Write,
    control: &MigrationControl,
) -> Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        control.check()?;
        let read = match input.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            return Ok(copied);
        }
        control.check()?;
        output.write_all(&buffer[..read])?;
        copied = copied
            .checked_add(read as u64)
            .ok_or("Profile size overflow")?;
    }
}

fn write_profile_marker(
    root: &Path,
    protection: &impl Protection,
    marker_content: &[u8],
) -> Result<()> {
    let marker = root.join(MARKER);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)?;
    protection.check(&marker)?;
    file.write_all(marker_content)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod platform {
    use super::*;
    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn EncryptFileW(name: *const u16) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }

    pub(super) fn local_drive(path: &Path) -> bool {
        use std::path::{Component, Prefix};
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return false;
        };
        let letter = match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => return false,
        };
        let root = [u16::from(letter), u16::from(b':'), u16::from(b'\\'), 0];
        // The four-code-unit drive root is NUL-terminated.
        matches!(unsafe { GetDriveTypeW(root.as_ptr()) }, 2 | 3)
    }

    pub(super) fn encrypt(path: &Path) -> Result<()> {
        reject_link(path)?;
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(UNAVAILABLE.into());
        }
        wide.push(0);
        // The string is NUL-terminated and remains valid through the call.
        if unsafe { EncryptFileW(wide.as_ptr()) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub(super) fn encrypted(path: &Path) -> Result<bool> {
        const FILE_ATTRIBUTE_ENCRYPTED: u32 = 0x4000;
        reject_link(path)?;
        Ok(fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_ENCRYPTED != 0)
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;

    pub(super) fn encrypt(_: &Path) -> Result<()> {
        Err(UNAVAILABLE.into())
    }

    pub(super) fn encrypted(_: &Path) -> Result<bool> {
        Err(UNAVAILABLE.into())
    }
}
