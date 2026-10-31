//! Lossless local archives. Verification and restoration never overwrite existing sessions.
use crate::{
    metadata,
    storage::{atomic_json, reject_link},
    Result, SessionData, SessionStore,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
mod audio;
mod deletion;
mod immediate;
mod restore;
pub use audio::{archive_audio, read_archived_transcript, restore_audio};
pub use deletion::{
    delete_archived_meeting, deletion_fingerprint, resume_deletion, unfinished_deletions,
    UnfinishedDeletion,
};
pub use immediate::{
    erase_meeting, pending_erasures, preview_erasure, ErasurePreview, ErasureResult, PendingErasure,
};
use restore::placement_attempt;
pub use restore::{move_source_into_archive, restore_new};

const MAX_FILES: usize = 100000;
const MAX_TOTAL: u64 = 64 * 1024 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;
/// Serializes archive moves, restoration, compaction and deletion within this storage root.
fn operation_lock(root: &Path) -> Result<File> {
    metadata::checked_directory(root)?;
    let file = metadata::open_lock_file(&root.join("archive-maintenance.lock"))?;
    metadata::lock_soon(&file)?;
    Ok(file)
}
/// Holds process-shared erasure exclusion across a source snapshot, annotation commit
/// and derived-index refresh. Callers must keep this lease until their entire write ends.
pub fn source_mutation_lease(root: &Path) -> Result<File> {
    operation_lock(root)
}
/// Content publishers share this process-shared lease from their first source read
/// through their final write. Parallel analysis/live/speaker work can coexist; no
/// capture transition or existing archive/annotation lock is reacquired by a writer.
pub fn source_publication_lease(root: &Path) -> Result<File> {
    publication_lease(root, false)
}
/// Erasure alone takes exclusive publication admission before inspecting any source.
/// The stable lock is outside erased namespaces, so another process cannot recreate
/// a second lock inode while a publisher still owns the original one.
fn publication_lease(root: &Path, exclusive: bool) -> Result<File> {
    metadata::checked_directory(root)?;
    let file = metadata::open_lock_file(&root.join("content-publication.lock"))?;
    if exclusive {
        if let Err(error) = metadata::lock_soon(&file) {
            let contended = error.kind() == std::io::ErrorKind::WouldBlock
                || matches!(
                    (error.raw_os_error(), fs2::lock_contended_error().raw_os_error()),
                    (Some(actual), Some(expected)) if actual == expected
                );
            if contended {
                return Err("Meeting content is being analysed, exported or published; wait for it to finish and try again. / Meetinginhalte werden gerade verarbeitet; bitte den Abschluss abwarten und erneut versuchen.".into());
            }
            // An unrelated I/O failure is not evidence of a competing publisher.
            return Err(error.into());
        }
    } else {
        fs2::FileExt::try_lock_shared(&file)?;
    }
    Ok(file)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub meeting_id: String,
    pub kind: String,
    pub session: SessionData,
    pub files: Vec<ArchivedFile>,
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub retained_sidecars: Vec<Sidecar>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sidecar {
    pub name: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}
/// Accepts only the current annotation file or a bounded revision filename for this meeting.
fn sidecar_name(name: &str, id: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    if !matches!(
        parts.first().copied(),
        Some("user-metadata" | "speaker-labels")
    ) {
        return false;
    }
    if parts.len() == 2 {
        return parts[1] == format!("{id}.json");
    }
    if parts.len() != 3 || parts[1] != format!("{id}-history") {
        return false;
    }
    let file = parts[2];
    file.len() == 58
        && file.as_bytes()[..20].iter().all(u8::is_ascii_digit)
        && file.as_bytes()[20] == b'-'
        && file.as_bytes()[21..53].iter().all(u8::is_ascii_hexdigit)
        && &file[53..] == ".json"
}
/// Collects bounded, ordinary annotation revisions and their hashes without rewriting them.
fn sidecars(root: &Path, id: &str) -> Result<Vec<Sidecar>> {
    let mut paths = Vec::new();
    for parent in ["user-metadata", "speaker-labels"] {
        let folder = root.join(parent);
        if !folder.exists() {
            continue;
        }
        metadata::checked_directory(&folder)?;
        let current = folder.join(format!("{id}.json"));
        if current.exists() {
            paths.push(current);
        }
        let history = folder.join(format!("{id}-history"));
        if history.exists() {
            metadata::checked_directory(&history)?;
            for entry in fs::read_dir(history)? {
                paths.push(entry?.path());
                if paths.len() > MAX_FILES {
                    return Err("Too many retained annotation revisions".into());
                }
            }
        }
    }
    paths.sort();
    let mut total = 0u64;
    let mut actual = 0usize;
    let mut output = Vec::new();
    for path in paths {
        let name = path
            .strip_prefix(root)?
            .to_str()
            .ok_or("Invalid annotation path")?
            .replace('\\', "/");
        if !sidecar_name(&name, id) {
            return Err(
                "Unknown annotation history file was preserved; archive requires review".into(),
            );
        }
        let meta = regular(&path)?;
        total = total
            .checked_add(meta.len())
            .ok_or("Annotation size overflow")?;
        if total > 16 * 1024 * 1024 {
            return Err("Annotation history exceeds archive bound; originals retained".into());
        }
        let mut bytes = Vec::new();
        File::open(path)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Annotation changed beyond its archive bound; preserved".into());
        }
        actual = actual
            .checked_add(bytes.len())
            .ok_or("Annotation size overflow")?;
        if actual > 16 * 1024 * 1024 {
            return Err("Annotation history exceeds archive bound; originals retained".into());
        }
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        output.push(Sidecar {
            name,
            bytes,
            sha256,
        });
    }
    Ok(output)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub archive_id: String,
    pub meeting_id: String,
    pub kind: String,
    pub archive_bytes: u64,
    pub original_bytes: u64,
    pub archive_sha256: String,
    pub files: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub created_at: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AudioArchive {
    receipt: Receipt,
    session: SessionData,
    originals: Vec<ArchivedFile>,
    restored: bool,
}
const TRANSCRIPT_FILES: &[&str] = &[
    "session.json",
    "session.json.bak",
    "audio-journal.jsonl",
    "transcript-journal.jsonl",
];
/// Compares recovered session data while ignoring transient warnings, read-only state and part order.
fn same_original_text(a: &SessionData, b: &SessionData) -> Result<bool> {
    let encode = |value: &SessionData| -> Result<Vec<u8>> {
        let mut data = value.clone();
        data.warnings.clear();
        data.read_only = false;
        data.parts.sort_by(|x, y| x.file.cmp(&y.file));
        Ok(serde_json::to_vec(&data)?)
    };
    Ok(encode(a)? == encode(b)?)
}
/// Rejects linked or non-file archive inputs before reading their metadata.
fn regular(path: &Path) -> Result<fs::Metadata> {
    reject_link(path)?;
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err("Archive input must be an ordinary file".into());
    }
    Ok(meta)
}
/// Checks a session-relative archive name against traversal and cross-platform filename hazards.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        && name.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.chars().any(char::is_control)
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(&stem.as_str())
        })
        && name.starts_with("session/")
}
/// Hashes an ordinary archive input with the shared decoded-size limit.
fn digest(path: &Path) -> Result<(u64, String)> {
    digest_cancellable(path, None)
}
/// Hashes an ordinary input in bounded blocks and stops at cancellation without changing it.
fn digest_cancellable(path: &Path, cancel: Option<&AtomicBool>) -> Result<(u64, String)> {
    regular(path)?;
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err("Archive hashing cancelled; existing data retained".into());
        }
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes.checked_add(n as u64).ok_or("Archive size overflow")?;
        if bytes > MAX_TOTAL {
            return Err("Archive input exceeds size limit".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}
/// Lists ordinary session files and empty directories; the live lease is excluded from payloads.
fn inventory(
    folder: &Path,
    base: &Path,
    files: &mut Vec<(String, PathBuf)>,
    directories: &mut Vec<String>,
) -> Result<()> {
    metadata::checked_directory(folder)?;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        reject_link(&path)?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            let relative = path
                .strip_prefix(base)?
                .to_str()
                .ok_or("Non-Unicode archive directory")?
                .replace('\\', "/");
            let name = format!("session/{relative}");
            if !safe_name(&name) {
                return Err("Unsafe archive directory".into());
            }
            directories.push(name);
            if directories.len() > MAX_FILES {
                return Err("Too many archive directories".into());
            }
            inventory(&path, base, files, directories)?;
        } else if meta.is_file() {
            let relative = path
                .strip_prefix(base)?
                .to_str()
                .ok_or("Non-Unicode archive input")?
                .replace('\\', "/");
            if relative == "session.lock" {
                continue;
            }
            let name = format!("session/{relative}");
            if !safe_name(&name) {
                return Err("Unsafe archive input path".into());
            }
            files.push((name, path));
            if files.len() > MAX_FILES {
                return Err("Too many archive input files".into());
            }
        } else {
            return Err("Unsupported archive input type".into());
        }
    }
    Ok(())
}
/// Creates only normal relative directories and rejects linked parents along the restore path.
fn safe_directories(base: &Path, relative: &Path) -> Result<()> {
    let mut current = metadata::checked_directory(base)?;
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err("Unsafe restore directory component".into());
        }
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
        metadata::checked_directory(&current)?;
    }
    Ok(())
}
/// Writes and verifies a lossless archive under source and metadata leases; originals remain in place.
pub fn create(
    root: &Path,
    id: &str,
    expected_metadata: u64,
    kind: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<Receipt> {
    let _operation = operation_lock(root)?;
    if !metadata::valid_id(id)
        || id.starts_with("legacy-")
        || !["whole-meeting", "audio-only"].contains(&kind)
    {
        return Err("Invalid archive request".into());
    }
    let labels = metadata::directory(root)?;
    let lock = metadata::open_lock_file(&labels.join(format!("{id}.lock")))?;
    metadata::lock_soon(&lock)?;
    let meta = metadata::read(root, id)?;
    if meta.revision != expected_metadata || meta.pinned {
        return Err("Meeting labels changed or this meeting is VIP protected".into());
    }
    let _speaker_lock = if kind == "whole-meeting" {
        let folder = root.join("speaker-labels");
        match fs::create_dir(&folder) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
        metadata::checked_directory(&folder)?;
        let lock = metadata::open_lock_file(&folder.join(format!("{id}.lock")))?;
        metadata::lock_soon(&lock)?;
        Some(lock)
    } else {
        None
    };
    let sessions = metadata::checked_directory(&root.join("sessions"))?;
    let folder = metadata::checked_directory(&sessions.join(id))?;
    let store = SessionStore::open(&folder)?;
    let data = store.snapshot()?;
    if data.id != id
        || data.read_only
        || data.status != "complete"
        || data.capture_incomplete
        || data
            .parts
            .iter()
            .any(|p| !p.audio_closed || p.status != "complete")
    {
        return Err("Only complete, unpinned, inactive meetings may be archived".into());
    }
    let mut input = Vec::new();
    let mut directories = Vec::new();
    inventory(&folder, &folder, &mut input, &mut directories)?;
    if kind == "audio-only" {
        let names: HashSet<_> = data
            .parts
            .iter()
            .map(|p| format!("session/audio/{}", p.file))
            .collect();
        input.retain(|(name, _)| names.contains(name));
        directories.retain(|name| name == "session/audio");
        if input.len() != names.len() {
            return Err("Original audio inventory is incomplete".into());
        }
    }
    input.sort_by(|a, b| a.0.cmp(&b.0));
    let total = input.len() as u64;
    let archives = root.join("archives");
    match fs::create_dir(&archives) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    metadata::checked_directory(&archives)?;
    let archive_id = uuid::Uuid::new_v4().simple().to_string();
    // Written before the ZIP so an interrupted copy stays attributable to its meeting,
    // and permanent deletion can find it even without a receipt.
    atomic_json(
        &archives.join(format!("{archive_id}.pending.json")),
        &serde_json::json!({"meetingId": id, "kind": kind}),
    )?;
    let path = archives.join(format!("{archive_id}.zip"));
    let output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&path)?;
    let mut writer = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(3))
        .large_file(true);
    let mut files = Vec::new();
    let mut original_bytes = 0u64;
    let mut seen = HashSet::new();
    directories.sort();
    for name in &directories {
        if !seen.insert(name.to_lowercase()) {
            return Err("Archive directory names collide".into());
        }
        writer.add_directory(format!("{name}/"), options)?;
    }
    for (number, (name, file)) in input.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            return Err(
                "Archiving cancelled; original files and partial evidence are retained".into(),
            );
        }
        if !seen.insert(name.to_lowercase()) {
            return Err("Archive paths collide across platforms".into());
        }
        regular(file)?;
        writer.start_file(name, options)?;
        let mut stream = File::open(file)?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Archiving cancelled; originals are retained".into());
            }
            let n = stream.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            original_bytes = original_bytes
                .checked_add(n as u64)
                .ok_or("Archive size overflow")?;
            if original_bytes > MAX_TOTAL {
                return Err("Archive exceeds size limit".into());
            }
            writer.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        files.push(ArchivedFile {
            name: name.clone(),
            bytes,
            sha256: format!("{:x}", hash.finalize()),
        });
        progress(number as u64 + 1, total);
    }
    let manifest = Manifest {
        schema: 1,
        meeting_id: id.into(),
        kind: kind.into(),
        session: data,
        files,
        directories,
        retained_sidecars: if kind == "whole-meeting" {
            sidecars(root, id)?
        } else {
            vec![]
        },
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("Archive manifest exceeds size limit".into());
    }
    writer.start_file("ARCHIVE_MANIFEST.json", options)?;
    writer.write_all(&bytes)?;
    let output = writer.finish()?;
    output.sync_all()?;
    drop(output);
    verify(&path, Some(cancel), |_, _| {})?;
    // A concurrent external edit must not become eligible for later source removal.
    for entry in &manifest.files {
        let relative = entry
            .name
            .strip_prefix("session/")
            .ok_or("Invalid archive name")?;
        let (len, hash) = digest_cancellable(&folder.join(relative), Some(cancel))?;
        if len != entry.bytes || hash != entry.sha256 {
            return Err(
                "Original files changed while archiving; originals and archive are retained".into(),
            );
        }
    }
    let (archive_bytes, archive_sha256) = digest_cancellable(&path, Some(cancel))?;
    let receipt = Receipt {
        archive_id,
        meeting_id: id.into(),
        kind: kind.into(),
        archive_bytes,
        original_bytes,
        archive_sha256,
        files: manifest.files.len(),
        title: manifest.session.title.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    atomic_json(
        &archives.join(format!("{}.receipt.json", receipt.archive_id)),
        &receipt,
    )?;
    Ok(receipt)
}
/// Checks archive identities, safe paths, bounded sizes and every manifest hash without extracting files.
pub fn verify(
    path: &Path,
    cancel: Option<&AtomicBool>,
    mut progress: impl FnMut(u64, u64),
) -> Result<Manifest> {
    regular(path)?;
    let mut reader = zip::ZipArchive::new(File::open(path)?)?;
    if reader.len() > MAX_FILES * 2 + 1 {
        return Err("Too many archive entries".into());
    }
    let manifest: Manifest = {
        let file = reader.by_name("ARCHIVE_MANIFEST.json")?;
        if file.size() > MAX_MANIFEST {
            return Err("Oversized archive manifest".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_MANIFEST + 1).read_to_end(&mut bytes)?;
        serde_json::from_slice(&bytes)?
    };
    if manifest.schema != 1
        || !metadata::valid_id(&manifest.meeting_id)
        || manifest.session.id != manifest.meeting_id
        || manifest.files.len() > MAX_FILES
        || manifest.directories.len() > MAX_FILES
        || reader.len() != manifest.files.len() + manifest.directories.len() + 1
        || !["whole-meeting", "audio-only"].contains(&manifest.kind.as_str())
    {
        return Err("Archive manifest identity or inventory mismatch".into());
    }
    let mut seen = HashSet::new();
    let mut sidecar_names = HashSet::new();
    let mut sidecar_bytes = 0usize;
    for sidecar in &manifest.retained_sidecars {
        sidecar_bytes = sidecar_bytes
            .checked_add(sidecar.bytes.len())
            .ok_or("Annotation archive overflow")?;
        if sidecar_bytes > 16 * 1024 * 1024
            || !sidecar_name(&sidecar.name, &manifest.meeting_id)
            || !sidecar_names.insert(sidecar.name.to_lowercase())
            || format!("{:x}", Sha256::digest(&sidecar.bytes)) != sidecar.sha256
        {
            return Err("Invalid retained annotation evidence".into());
        }
    }
    for directory in &manifest.directories {
        if !safe_name(directory) || !seen.insert(directory.to_lowercase()) {
            return Err("Unsafe or duplicate archive directory".into());
        }
        let entry = reader.by_name(&format!("{directory}/"))?;
        if !entry.is_dir()
            || entry.size() != 0
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Invalid archive directory entry".into());
        }
    }
    let mut total = 0u64;
    for (number, item) in manifest.files.iter().enumerate() {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err("Archive verification cancelled; originals retained".into());
        }
        if !safe_name(&item.name)
            || !seen.insert(item.name.to_lowercase())
            || item.sha256.len() != 64
        {
            return Err("Unsafe or duplicate archive path".into());
        }
        total = total
            .checked_add(item.bytes)
            .ok_or("Archive size overflow")?;
        if total > MAX_TOTAL {
            return Err("Archive decoded size exceeds limit".into());
        }
        let mut input = reader.by_name(&item.name)?;
        if input.size() != item.bytes
            || input.is_dir()
            || input.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Archive entry type or size mismatch".into());
        }
        let mut hasher = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
                return Err("Archive verification cancelled; existing data retained".into());
            }
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > item.bytes {
                return Err("Archive expanded beyond manifest".into());
            }
            hasher.update(&buffer[..n]);
        }
        if bytes != item.bytes || format!("{:x}", hasher.finalize()) != item.sha256 {
            return Err("Archive content SHA256 mismatch".into());
        }
        progress(number as u64 + 1, manifest.files.len() as u64);
    }
    Ok(manifest)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePage {
    pub items: Vec<ArchiveEntry>,
    pub more: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub archive_id: String,
    /// Absent when the receipt itself could not be read; its files are still preserved.
    pub receipt: Option<Receipt>,
    pub title: String,
    pub session_present: bool,
    pub source_copy_retained: bool,
    /// Set when part of this entry could not be read. Such entries offer no actions.
    pub unreadable: Option<Unreadable>,
}
/// Which record of an archive entry could not be read. All files stay unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Unreadable {
    /// Missing, linked, oversized, corrupt or from a newer version (unknown fields).
    Receipt,
    /// The meeting's saved labels could not be read.
    Labels,
}
/// Reads one bounded ordinary receipt and checks that its IDs match the requested archive.
fn receipt_at(folder: &Path, id: &str) -> Result<Receipt> {
    if !metadata::valid_id(id) || id.starts_with("legacy-") {
        return Err("Invalid archive identity".into());
    }
    let path = folder.join(format!("{id}.receipt.json"));
    regular(&path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("Oversized archive receipt".into());
    }
    let value: Receipt = serde_json::from_slice(&bytes)?;
    if value.archive_id != id
        || !metadata::valid_id(&value.meeting_id)
        || value.meeting_id.starts_with("legacy-")
        || !["whole-meeting", "audio-only"].contains(&value.kind.as_str())
    {
        return Err("Archive receipt identity mismatch".into());
    }
    Ok(value)
}
/// Pages through archive receipts, retaining unreadable entries as explicit protected results.
pub fn list(root: &Path, offset: usize, limit: usize) -> Result<ArchivePage> {
    if !(1..=50).contains(&limit) || offset > 1000000 {
        return Err("Invalid archive page".into());
    }
    metadata::checked_directory(root)?;
    let folder = root.join("archives");
    if !folder.exists() {
        return Ok(ArchivePage {
            items: Vec::new(),
            more: false,
        });
    }
    metadata::checked_directory(&folder)?;
    let mut ids = Vec::new();
    for entry in fs::read_dir(&folder)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix(".receipt.json") {
            if metadata::valid_id(id) && !id.starts_with("legacy-") {
                ids.push(id.to_owned());
            }
        }
        if ids.len() > 1000000 {
            return Err("Archive catalog exceeds browsing bound".into());
        }
    }
    ids.sort();
    let more = ids.len() > offset.saturating_add(limit);
    let mut items = Vec::new();
    for id in ids.iter().skip(offset).take(limit) {
        // One unreadable entry (e.g. a receipt from a newer version) must not hide the page.
        let source_copy_retained = folder.join(format!("{id}.source")).exists();
        let Ok(receipt) = receipt_at(&folder, id) else {
            items.push(ArchiveEntry {
                archive_id: id.clone(),
                receipt: None,
                title: id.clone(),
                session_present: false,
                source_copy_retained,
                unreadable: Some(Unreadable::Receipt),
            });
            continue;
        };
        let fallback = if receipt.title.is_empty() {
            receipt.meeting_id.clone()
        } else {
            receipt.title.clone()
        };
        let (title, unreadable) = match metadata::read(root, &receipt.meeting_id) {
            Ok(labels) => (labels.title.unwrap_or(fallback), None),
            Err(_) => (fallback, Some(Unreadable::Labels)),
        };
        items.push(ArchiveEntry {
            archive_id: id.clone(),
            title,
            session_present: root.join("sessions").join(&receipt.meeting_id).exists(),
            source_copy_retained,
            receipt: Some(receipt),
            unreadable,
        });
    }
    Ok(ArchivePage { items, more })
}
/// Checks a receipt's ZIP size and hash before selecting whole-session or audio-only restoration.
pub fn restore(root: &Path, archive_id: &str, cancel: &AtomicBool) -> Result<PathBuf> {
    let folder = metadata::checked_directory(&root.join("archives"))?;
    let receipt = receipt_at(&folder, archive_id)?;
    let zip = folder.join(format!("{archive_id}.zip"));
    let (size, hash) = digest_cancellable(&zip, Some(cancel))?;
    if size != receipt.archive_bytes || hash != receipt.archive_sha256 {
        return Err("Archive changed; restoration blocked".into());
    }
    if receipt.kind == "whole-meeting" {
        restore_new(root, archive_id, cancel)
    } else {
        restore_audio(root, &receipt.meeting_id, cancel)?;
        Ok(root.join("sessions").join(receipt.meeting_id))
    }
}
/// Reclaim the redundant raw copy only after fully verifying its retained lossless ZIP.
/// Missing files from an interrupted previous compaction are safe; unknown/changed files block it.
pub fn compact_source_copy(
    root: &Path,
    archive_id: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let _operation = operation_lock(root)?;
    let archives = metadata::checked_directory(&root.join("archives"))?;
    let receipt = receipt_at(&archives, archive_id)?;
    if receipt.kind != "whole-meeting" {
        return Err("Only whole-meeting source copies can be compacted".into());
    }
    let labels = metadata::directory(root)?;
    let lock = metadata::open_lock_file(&labels.join(format!("{}.lock", receipt.meeting_id)))?;
    metadata::lock_soon(&lock)?;
    if metadata::read(root, &receipt.meeting_id)?.pinned {
        return Err("VIP meeting is protected from archive compaction".into());
    }
    let zip = archives.join(format!("{archive_id}.zip"));
    let (size, hash) = digest_cancellable(&zip, Some(cancel))?;
    if size != receipt.archive_bytes || hash != receipt.archive_sha256 {
        return Err("Archive changed; raw copy retained".into());
    }
    let manifest = verify(&zip, Some(cancel), |_, _| {})?;
    if manifest.meeting_id != receipt.meeting_id || manifest.kind != "whole-meeting" {
        return Err("Archive identity mismatch".into());
    }
    let source = metadata::checked_directory(&archives.join(format!("{archive_id}.source")))?;
    let mut actual = Vec::new();
    let mut directories = Vec::new();
    inventory(&source, &source, &mut actual, &mut directories)?;
    if directories
        .iter()
        .any(|d| !manifest.directories.contains(d))
    {
        return Err("Unknown directory in retained source; raw copy preserved".into());
    }
    let mut selected = Vec::new();
    for (name, path) in actual {
        let expected = manifest
            .files
            .iter()
            .find(|f| f.name == name)
            .ok_or("New user file in retained copy; preserved")?;
        let (bytes, sha) = digest_cancellable(&path, Some(cancel))?;
        if bytes != expected.bytes || sha != expected.sha256 {
            return Err("Retained source changed; raw copy preserved".into());
        }
        selected.push((path, bytes, sha));
    }
    let session_lock = source.join("session.lock");
    if session_lock.exists() {
        let (bytes, sha) = digest_cancellable(&session_lock, Some(cancel))?;
        if bytes != 0 {
            return Err("Changed lease file in retained copy; preserved".into());
        }
        selected.push((session_lock, bytes, sha));
    }
    let logs = root.join("maintenance-log");
    match fs::create_dir(&logs) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    };
    metadata::checked_directory(&logs)?;
    let log = logs.join(format!("compact-{}.json", uuid::Uuid::new_v4().simple()));
    let mut record = serde_json::json!({"archiveId":archive_id,"meetingId":receipt.meeting_id,"archiveSHA256":receipt.archive_sha256,"phase":"verified","removedDuplicateFiles":0,"totalDuplicateFiles":selected.len(),"originalsInLosslessZIP":true});
    atomic_json(&log, &record)?;
    for (n, (path, bytes, sha)) in selected.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            return Err("Compaction cancelled; the complete lossless ZIP and remaining raw copies are retained. Repeat compaction to finish.".into());
        }
        let mut parent = source.clone();
        for part in path
            .strip_prefix(&source)?
            .parent()
            .ok_or("Invalid compaction parent")?
            .components()
        {
            parent.push(part);
            metadata::checked_directory(&parent)?;
        }
        let (current_size, current_hash) = digest_cancellable(path, Some(cancel))?;
        if current_size != *bytes || current_hash != *sha {
            return Err(
                "Raw copy changed during compaction; remaining files and complete ZIP retained"
                    .into(),
            );
        }
        fs::remove_file(path)?;
        record["removedDuplicateFiles"] = serde_json::json!(n + 1);
        atomic_json(&log, &record)?;
        progress(n as u64 + 1, selected.len() as u64);
    }
    directories.sort_by_key(|p| std::cmp::Reverse(p.split('/').count()));
    for directory in directories {
        fs::remove_dir(
            source.join(
                directory
                    .strip_prefix("session/")
                    .ok_or("Invalid source directory")?,
            ),
        )?;
    }
    fs::remove_dir(&source)?;
    let placement = archives.join(format!("{archive_id}.placement.json"));
    if placement.exists() {
        regular(&placement)?;
        atomic_json(
            &placement,
            &serde_json::json!({"archiveId":archive_id,"meetingId":receipt.meeting_id,"phase":"compacted","sourceCopyRetained":false,"archiveSHA256":receipt.archive_sha256}),
        )?;
    }
    record["phase"] = serde_json::json!("complete");
    atomic_json(&log, &record)?;
    Ok(())
}
#[cfg(test)]
mod tests;
