//! Migrate a local profile by copying its authoritative files to an EFS directory.
//! The operating system protects writes made by the audio workers and SQLite too.
use super::{digest, metadata, operation_lock, publication_lease, reject_link};
use crate::{storage, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

const MARKER: &str = "protected-profile.v1";
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
    cfg!(target_os = "windows")
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
    storage::exists_no_link(&root.join(MARKER))
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

pub fn migrate(source: &Path, parent: &Path) -> Result<Receipt> {
    migrate_with(source, parent, &Efs)
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
    visit: &mut impl FnMut(&Path, bool) -> Result<bool>,
) -> Result<()> {
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
            walk(&child?.path(), depth + 1, remaining, visit)?;
        }
    }
    Ok(())
}

fn inventory(root: &Path) -> Result<BTreeMap<PathBuf, Entry>> {
    let mut entries = BTreeMap::new();
    for name in PROFILE_FILES {
        let path = root.join(name);
        if storage::exists_no_link(&path)? {
            let (bytes, hash) = digest(&path)?;
            entries.insert(PathBuf::from(name), Entry::File(bytes, hash));
        }
    }
    let mut remaining = MAX_ENTRIES;
    for name in PROFILE_DATA {
        let path = root.join(name);
        if !storage::exists_no_link(&path)? {
            continue;
        }
        metadata::checked_directory(&path)?;
        walk(&path, 1, &mut remaining, &mut |path, directory| {
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
                let (bytes, hash) = digest(path)?;
                Entry::File(bytes, hash)
            };
            entries.insert(relative.to_owned(), entry);
            Ok(true)
        })?;
    }
    Ok(entries)
}

fn verify_with(root: &Path, protection: &impl Protection) -> Result<()> {
    let root = checked_root(root).map_err(|_| NOT_PROTECTED)?;
    let marker = root.join(MARKER);
    reject_link(&marker).map_err(|_| NOT_PROTECTED)?;
    protection.check(&marker)?;
    let mut contents = Vec::new();
    File::open(&marker)?
        .take(MARKER_CONTENT.len() as u64 + 1)
        .read_to_end(&mut contents)?;
    if contents != MARKER_CONTENT {
        return Err(NOT_PROTECTED.into());
    }
    let mut remaining = MAX_ENTRIES;
    walk(&root, 0, &mut remaining, &mut |path, _| {
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

fn session_leases(root: &Path) -> Result<Vec<storage::RelocationGuard>> {
    let folder = root.join("sessions");
    metadata::checked_directory(&folder)?;
    let mut leases = Vec::new();
    for entry in fs::read_dir(folder)? {
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

fn migrate_with(source: &Path, parent: &Path, protection: &impl Protection) -> Result<Receipt> {
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
    let _archive = operation_lock(&source)?;
    let _publication = publication_lease(&source, true)?;
    let _sessions = session_leases(&source)?;
    let stage = tempfile::Builder::new()
        .prefix(DIRECTORY_PREFIX)
        .tempdir_in(parent)?;
    let root = stage.path();
    protection.enable_directory(root).map_err(|_| UNAVAILABLE)?;
    let before = inventory(&source)?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    for (name, entry) in &before {
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
                if io::copy(&mut input, &mut output)? != *length {
                    return Err("The source changed during migration. / Die Quelle hat sich beim Kopieren geändert.".into());
                }
                output.sync_all()?;
                drop(output);
                if digest(&path)? != (*length, expected.clone()) {
                    return Err("The copied file failed verification. / Eine kopierte Datei konnte nicht verifiziert werden.".into());
                }
                files += 1;
                bytes = bytes.checked_add(*length).ok_or("Profile size overflow")?;
            }
        }
    }
    if inventory(&source)? != before {
        return Err(
            "The source changed during migration. / Die Quelle hat sich beim Kopieren geändert."
                .into(),
        );
    }
    let marker = root.join(MARKER);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)?;
    protection.check(&marker)?;
    file.write_all(MARKER_CONTENT)?;
    file.sync_all()?;
    drop(file);
    verify_with(root, protection)?;
    Ok(Receipt {
        path: stage.keep(),
        files,
        bytes,
    })
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
