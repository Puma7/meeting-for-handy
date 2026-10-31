//! Explicit erasure of one inactive, app-owned meeting, including attributed retained copies.
//! No retention age rule, trash copy, transport or external-path deletion is involved.
use super::{
    deletion::{add_tree, annotation_locks, optional_entries, push_delete_file, DeleteFile},
    *,
};
use std::collections::{BTreeMap, BTreeSet};
mod copies;
mod records;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErasurePreview {
    pub meeting_id: String,
    pub fingerprint: String,
    pub files: usize,
    pub bytes: u64,
    pub rebuild_search: bool,
    /// Unassignable atomic-write remnants are preserved and disclosed before confirmation.
    pub preserved_atomic_remnants: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErasureResult {
    pub removed_files: usize,
    pub complete: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingErasure {
    pub meeting_id: String,
    pub confirmed_at: String,
    /// Identity-only retry authority, never titles, paths, quotes or file inventories.
    #[serde(default)]
    pub archive_ids: Vec<String>,
}
struct Inventory {
    files: Vec<DeleteFile>,
    directories: Vec<PathBuf>,
    archive_ids: Vec<String>,
    preserved_atomic_remnants: usize,
}

/// Reads bounded ordinary JSON without interpreting any embedded instructions or paths.
fn read_record(path: &Path) -> Result<serde_json::Value> {
    regular(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("Oversized retained record; erasure blocked".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
/// Collects complete structured meetingId fields, never quoted title/path text.
fn references(value: &serde_json::Value, ids: &mut BTreeSet<String>, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err("Retained record nesting exceeds erasure bound".into());
    }
    match value {
        serde_json::Value::Array(rows) => {
            for row in rows {
                references(row, ids, depth + 1)?;
            }
        }
        serde_json::Value::Object(rows) => {
            for (key, row) in rows {
                if key == "meetingId" {
                    if let Some(id) = row.as_str() {
                        ids.insert(id.to_owned());
                        if ids.len() > MAX_FILES {
                            return Err("Too many retained record identities".into());
                        }
                    }
                }
                references(row, ids, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}
/// Plans are rebuildable shared previews. Mixed maintenance logs are ambiguous
/// authority: neither erase another meeting's evidence nor silently retain known
/// selected-meeting content while claiming success. Block before any removal.
fn selected_record(parent: &str, id: &str, ids: &BTreeSet<String>) -> Result<bool> {
    if !ids.contains(id) {
        return Ok(false);
    }
    if parent == "maintenance-log" && ids.len() != 1 {
        return Err("Maintenance record names multiple meetings; erasure blocked and all files preserved. Review the conflicting record before creating another preview.".into());
    }
    Ok(true)
}
/// Attributes legacy pending records before allowing a retry whose source manifest is gone.
fn pending(root: &Path, id: &str) -> Result<Option<PendingErasure>> {
    let folder = root.join("immediate-deletions");
    metadata::reject_link_if_present(&folder)?;
    if !folder.exists() {
        return Ok(None);
    }
    metadata::checked_directory(&folder)?;
    let path = folder.join(format!("{id}.json"));
    metadata::reject_link_if_present(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    let value: PendingErasure = serde_json::from_value(read_record(&path)?)?;
    if value.meeting_id != id
        || value.archive_ids.len() > MAX_FILES
        || value.archive_ids.iter().any(|aid| !native_id(aid))
        || chrono::DateTime::parse_from_rfc3339(&value.confirmed_at).is_err()
    {
        return Err("Invalid pending erasure identity".into());
    }
    Ok(Some(value))
}
/// Minimal retry markers contain no title, quote, source path or file inventory.
pub fn pending_erasures(root: &Path) -> Result<Vec<PendingErasure>> {
    let mut result = Vec::new();
    for entry in optional_entries(&root.join("immediate-deletions"))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return Err("Invalid pending erasure name".into());
        };
        // An orphan staging file is retained, never promoted to erasure authority.
        // It must not hide valid markers belonging to other interrupted operations.
        if name.starts_with('.') || name.ends_with(".json.bak") || !name.ends_with(".json") {
            continue;
        }
        regular(&path)?;
        let id = name
            .strip_suffix(".json")
            .ok_or("Invalid pending erasure record")?;
        if !metadata::valid_id(id) || id.starts_with("legacy-") {
            return Err("Invalid pending erasure identity".into());
        }
        result.push(pending(root, id)?.ok_or("Pending erasure vanished")?);
    }
    Ok(result)
}
/// Includes content-bearing maintenance receipts and previews referring to this meeting.
fn retained_records(
    root: &Path,
    id: &str,
    cancel: &AtomicBool,
    files: &mut Vec<DeleteFile>,
    preserved: &mut usize,
) -> Result<()> {
    for parent in ["retention-plans", "maintenance-log"] {
        for entry in optional_entries(&root.join(parent))? {
            if cancel.load(Ordering::Acquire) {
                return Err("Erasure preview cancelled".into());
            }
            let path = entry?.path();
            if records::is_temporary(&path)? {
                match records::temporary_identities(&path, false)? {
                    Some(ids) => {
                        if selected_record(parent, id, &ids)? {
                            push_delete_file(root, &path, cancel, files)?;
                        }
                    }
                    None => *preserved += 1,
                }
                continue;
            }
            let mut ids = BTreeSet::new();
            references(&read_record(&path)?, &mut ids, 0)?;
            if selected_record(parent, id, &ids)? {
                push_delete_file(root, &path, cancel, files)?;
            }
        }
    }
    Ok(())
}
/// Keeps the checked archive targets alongside the source leases. The caller already
/// holds the exclusive archive/publication leases, so pre-removal inventory can reuse
/// this attribution without decompressing the same ZIPs a second time.
fn source_leases(
    root: &Path,
    id: &str,
    cancel: &AtomicBool,
) -> Result<(Vec<crate::storage::RelocationGuard>, Vec<String>)> {
    let known = pending(root, id)?
        .map(|p| p.archive_ids)
        .unwrap_or_default();
    let targets = copies::targets(root, id, &known, cancel)?;
    let mut leases = Vec::new();
    for path in std::iter::once(root.join("sessions").join(id)).chain(
        targets
            .iter()
            .map(|aid| root.join("archives").join(format!("{aid}.source"))),
    ) {
        metadata::reject_link_if_present(&path)?;
        if path.exists() {
            // A corrupt/empty owned meeting is erasable; a valid manifest naming
            // a different meeting is an attribution conflict, never erasure authority.
            copies::reject_conflicting_identity(&path, id)?;
            leases.push(crate::storage::erasure_lease(&path)?);
        }
    }
    Ok((leases, targets))
}
/// Erasure authority accepts only fixed native identities, never external legacy references.
fn native_id(id: &str) -> bool {
    metadata::valid_id(id) && !id.starts_with("legacy-")
}
/// Reuses targets only within the unchanged, exclusively leased pre-removal state.
/// Passing None after removal performs fresh attribution of all remaining copies.
fn inventory_for(
    root: &Path,
    id: &str,
    cancel: &AtomicBool,
    checked_targets: Option<Vec<String>>,
) -> Result<Inventory> {
    let prior = pending(root, id)?;
    let known = prior
        .as_ref()
        .map(|p| p.archive_ids.as_slice())
        .unwrap_or_default();
    let targets = match checked_targets {
        Some(targets) => targets,
        None => copies::targets(root, id, known, cancel)?,
    };
    let mut data = copies::inventory(root, id, targets, cancel)?;
    if data.files.is_empty() && data.directories.is_empty() && prior.is_none() {
        return Err("No app-owned meeting or attributed copy found".into());
    }
    retained_records(
        root,
        id,
        cancel,
        &mut data.files,
        &mut data.preserved_atomic_remnants,
    )?;
    let unique: BTreeMap<_, _> = data
        .files
        .into_iter()
        .map(|f| (f.name.clone(), f))
        .collect();
    data.files = unique.into_values().collect();
    data.directories.sort();
    data.directories.dedup();
    Ok(data)
}
/// Only the documented current SQLite files and retained unreadable cache copies are caches.
fn cache_inventory(root: &Path, cancel: &AtomicBool) -> Result<Inventory> {
    let folder = root.join("library-index");
    let mut files = Vec::new();
    let mut directories = Vec::new();
    fn walk(
        root: &Path,
        folder: &Path,
        cancel: &AtomicBool,
        files: &mut Vec<DeleteFile>,
        dirs: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if depth > 2 {
            return Err("Unexpected search-cache nesting; erasure blocked".into());
        }
        for entry in optional_entries(folder)? {
            let path = entry?.path();
            reject_link(&path)?;
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid cache name")?;
            if path.is_dir() {
                if depth != 0 || !name.starts_with("unreadable-") {
                    return Err("Unknown search-cache directory; erasure blocked".into());
                }
                walk(root, &path, cancel, files, dirs, depth + 1)?;
            } else if [
                "library.sqlite3",
                "library.sqlite3-wal",
                "library.sqlite3-shm",
                "library.sqlite3-journal",
            ]
            .contains(&name)
            {
                push_delete_file(root, &path, cancel, files)?;
            } else {
                return Err("Unknown search-cache file; erasure blocked".into());
            }
        }
        if folder.exists() {
            dirs.push(folder.to_owned());
        }
        Ok(())
    }
    walk(root, &folder, cancel, &mut files, &mut directories, 0)?;
    Ok(Inventory {
        files,
        directories,
        archive_ids: Vec::new(),
        preserved_atomic_remnants: 0,
    })
}
/// The preview authority covers meeting data/retained copies, never mutable search-cache pages.
fn fingerprint(id: &str, files: &[DeleteFile], preserved: usize) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            "explicit-meeting-erasure-v2",
            id,
            files,
            preserved
        ))?)
    ))
}
/// Prepares one immediate erasure without removing any data or creating a replacement copy.
pub fn preview_erasure(root: &Path, id: &str, cancel: &AtomicBool) -> Result<ErasurePreview> {
    if !metadata::valid_id(id) || id.starts_with("legacy-") {
        return Err(
            "Imported originals are outside app-owned storage and cannot be erased here".into(),
        );
    }
    let _operation = operation_lock(root)?;
    let _publication = publication_lease(root, true)?;
    let _index = crate::library::erasure_index_lock(root)?;
    let _annotations = annotation_locks(root, id)?;
    let (_leases, targets) = source_leases(root, id, cancel)?;
    let data = inventory_for(root, id, cancel, Some(targets))?;
    cache_inventory(root, cancel)?;
    Ok(ErasurePreview {
        meeting_id: id.into(),
        fingerprint: fingerprint(id, &data.files, data.preserved_atomic_remnants)?,
        files: data.files.len(),
        bytes: data.files.iter().map(|f| f.bytes).sum(),
        rebuild_search: true,
        preserved_atomic_remnants: data.preserved_atomic_remnants,
    })
}
/// Rechecks each ordinary parent and exact bytes before removing a confirmed owned file.
fn remove_file(root: &Path, file: &DeleteFile, cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err("Erasure interrupted; already removed files cannot be restored. Confirm a new preview to finish remaining data.".into());
    }
    let relative = Path::new(&file.name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Unsafe erasure path".into());
    }
    let mut parent = root.to_path_buf();
    for part in relative
        .parent()
        .ok_or("Missing erasure parent")?
        .components()
    {
        parent.push(part);
        metadata::checked_directory(&parent)?;
    }
    let path = root.join(relative);
    let (size, hash) = digest_cancellable(&path, Some(cancel))?;
    if size != file.bytes || hash != file.sha256 {
        return Err(
            "Erasure inventory changed; remaining files retained. Create a new preview.".into(),
        );
    }
    fs::remove_file(path)?;
    Ok(())
}
/// Permanently removes exactly the confirmed inactive meeting. Partial failures keep a minimal
/// retry marker, not content-bearing audit copies. Success requires a final empty inventory.
pub fn erase_meeting(
    root: &Path,
    id: &str,
    expected: &str,
    confirmation: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<ErasureResult> {
    if !metadata::valid_id(id)
        || id.starts_with("legacy-")
        || confirmation != format!("DELETE {id}")
    {
        return Err("Erasure requires confirmation of exactly one native meeting".into());
    }
    let _operation = operation_lock(root)?;
    let _publication = publication_lease(root, true)?;
    let _index = crate::library::erasure_index_lock(root)?;
    let _annotations = annotation_locks(root, id)?;
    let (_leases, targets) = source_leases(root, id, cancel)?;
    let mut data = inventory_for(root, id, cancel, Some(targets))?;
    if fingerprint(id, &data.files, data.preserved_atomic_remnants)? != expected {
        return Err(
            "Meeting or copies changed since preview; nothing deleted. Create a new preview."
                .into(),
        );
    }
    let cache = cache_inventory(root, cancel)?;
    data.files.extend(cache.files);
    data.directories.extend(cache.directories);
    if cancel.load(Ordering::Acquire) {
        return Err("Erasure cancelled before file removal".into());
    }
    let folder = root.join("immediate-deletions");
    match fs::create_dir(&folder) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    metadata::checked_directory(&folder)?;
    let marker = folder.join(format!("{id}.json"));
    let record = PendingErasure {
        meeting_id: id.into(),
        confirmed_at: pending(root, id)?
            .map(|p| p.confirmed_at)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        archive_ids: data.archive_ids.clone(),
    };
    metadata::reject_link_if_present(&marker.with_extension("json.bak"))?;
    atomic_json(&marker, &record)?;
    // Leave primary manifests until late so an interrupted operation remains discoverable.
    data.files.sort_by_key(|f| {
        (
            f.name.starts_with(&format!("sessions/{id}/"))
                && (f.name.ends_with("session.json") || f.name.ends_with("session.json.bak")),
            f.name.clone(),
        )
    });
    let total = data.files.len();
    progress(0, total as u64);
    for (n, file) in data.files.iter().enumerate() {
        remove_file(root, file, cancel)?;
        progress(n as u64 + 1, total as u64);
    }
    data.directories
        .sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    data.directories.dedup();
    for directory in data.directories {
        metadata::checked_directory(&directory)?;
        fs::remove_dir(directory)?;
    }
    let remaining = inventory_for(root, id, cancel, None)?;
    if !remaining.files.is_empty()
        || !remaining.directories.is_empty()
        || remaining.preserved_atomic_remnants != data.preserved_atomic_remnants
        || root.join("library-index").exists()
    {
        return Err("Erasure incomplete; remaining data requires a new confirmed preview".into());
    }
    let backup = marker.with_extension("json.bak");
    metadata::reject_link_if_present(&backup)?;
    if backup.exists() {
        regular(&backup)?;
        fs::remove_file(backup)?;
    }
    regular(&marker)?;
    fs::remove_file(marker)?;
    Ok(ErasureResult {
        removed_files: total,
        complete: true,
    })
}
