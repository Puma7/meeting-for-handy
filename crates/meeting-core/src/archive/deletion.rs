//! Explicit, inventory-bound permanent deletion and retry logs.
use super::restore::{absent, MAX_PLACEMENT_ATTEMPTS};
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteFile {
    pub(super) name: String,
    pub(super) bytes: u64,
    pub(super) sha256: String,
}
pub(super) struct DeleteInventory {
    pub(super) files: Vec<DeleteFile>,
    pub(super) directories: Vec<PathBuf>,
    pub(super) newest: String,
    legacy_age_evidence: bool,
}

/// Legacy receipts had no archive creation date. A meeting's end alone is not
/// conservative: an old meeting may have been archived or copied only today.
/// Use the newest available ZIP/receipt filesystem provenance and verified
/// meeting date, while keeping a nonempty malformed receipt date fail-closed.
fn receipt_age(
    receipt: &Receipt,
    manifest: &Manifest,
    zip: &Path,
    receipt_path: &Path,
) -> Result<(chrono::DateTime<chrono::Utc>, bool)> {
    if !receipt.created_at.is_empty() {
        return Ok((
            chrono::DateTime::parse_from_rfc3339(&receipt.created_at)?.with_timezone(&chrono::Utc),
            false,
        ));
    }
    let mut newest = chrono::DateTime::parse_from_rfc3339(&manifest.session.started_at)?
        .with_timezone(&chrono::Utc);
    if let Some(ended) = manifest.session.ended_at.as_deref() {
        // Imported/clock-rollback metadata can have an end before the start.
        // Never discard a newer known date when deciding whether deletion is old enough.
        newest =
            newest.max(chrono::DateTime::parse_from_rfc3339(ended)?.with_timezone(&chrono::Utc));
    }
    for path in [zip, receipt_path] {
        let metadata = regular(path)?;
        newest = newest.max(chrono::DateTime::<chrono::Utc>::from(metadata.modified()?));
        // Some Unix filesystems do not expose birth time; their modified time
        // remains mandatory. A recent copy with birth time extends protection.
        if let Ok(created) = metadata.created() {
            newest = newest.max(chrono::DateTime::<chrono::Utc>::from(created));
        }
    }
    Ok((newest, true))
}

/// Binds deletion to every retained file and, for undated receipts, conservative age provenance.
fn inventory_hash(inventory: &DeleteInventory) -> Result<String> {
    let bytes = if inventory.legacy_age_evidence {
        // Date fallback is part of the preview authority. Touching/replacing an
        // otherwise byte-identical legacy archive must invalidate an older plan.
        serde_json::to_vec(&("legacy-archive-age-v1", &inventory.files, &inventory.newest))?
    } else {
        // Preserve the fingerprint contract of already-dated receipts.
        serde_json::to_vec(&inventory.files)?
    };
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
/// Builds a complete deletion group or refuses ambiguous archives, changed sources and active sessions.
pub(super) fn deletion_inventory(
    root: &Path,
    id: &str,
    cancel: &AtomicBool,
) -> Result<DeleteInventory> {
    if !metadata::valid_id(id)
        || id.starts_with("legacy-")
        || root.join("sessions").join(id).exists()
    {
        return Err(
            "Only a fully archived meeting absent from normal history can be permanently deleted"
                .into(),
        );
    }
    let archives = metadata::checked_directory(&root.join("archives"))?;
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut newest = None;
    let mut legacy_age_evidence = false;
    let mut whole = false;
    for entry in fs::read_dir(&archives)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(aid) = name.strip_suffix(".receipt.json") else {
            continue;
        };
        // Any unreadable catalog entry blocks deletion: it might be another copy of this meeting.
        let receipt = receipt_at(&archives, aid)?;
        if receipt.meeting_id != id {
            continue;
        }
        whole |= receipt.kind == "whole-meeting";
        let zip = archives.join(format!("{aid}.zip"));
        let (size, hash) = digest_cancellable(&zip, Some(cancel))?;
        if size != receipt.archive_bytes || hash != receipt.archive_sha256 {
            return Err("Archive changed; permanent deletion blocked".into());
        }
        let manifest = verify(&zip, Some(cancel), |_, _| {})?;
        if manifest.meeting_id != id || manifest.kind != receipt.kind {
            return Err("Archive identity mismatch; retained".into());
        }
        let (at, legacy) = receipt_age(&receipt, &manifest, &zip, &entry.path())?;
        newest = Some(newest.map_or(at, |prior: chrono::DateTime<chrono::Utc>| prior.max(at)));
        legacy_age_evidence |= legacy;
        for path in [zip, entry.path()] {
            push_delete_file(root, &path, cancel, &mut files)?;
        }
        // atomic_json keeps the previous placement phase as a .bak next to the record.
        for placement in [
            archives.join(format!("{aid}.placement.json")),
            archives.join(format!("{aid}.placement.json.bak")),
        ] {
            if placement.exists() {
                push_delete_file(root, &placement, cancel, &mut files)?;
            }
        }
        for number in 1..=MAX_PLACEMENT_ATTEMPTS {
            let attempt = placement_attempt(&archives, aid, number);
            if !absent(&attempt)? {
                push_delete_file(root, &attempt, cancel, &mut files)?;
            }
        }
        let source = archives.join(format!("{aid}.source"));
        if source.exists() {
            metadata::checked_directory(&source)?;
            let mut actual = Vec::new();
            let mut dirs = Vec::new();
            inventory(&source, &source, &mut actual, &mut dirs)?;
            let mut expected_dirs = manifest.directories.clone();
            expected_dirs.sort();
            dirs.sort();
            if actual.len() != manifest.files.len() || dirs != expected_dirs {
                return Err("Retained source inventory changed; permanent deletion blocked".into());
            }
            for (name, path) in actual {
                let expected = manifest
                    .files
                    .iter()
                    .find(|f| f.name == name)
                    .ok_or("Unarchived file in retained source")?;
                let (size, hash) = digest_cancellable(&path, Some(cancel))?;
                if size != expected.bytes || hash != expected.sha256 {
                    return Err("Retained source changed; permanent deletion blocked".into());
                }
                push_delete_file(root, &path, cancel, &mut files)?;
            }
            let lease = source.join("session.lock");
            if lease.exists() {
                push_delete_file(root, &lease, cancel, &mut files)?;
            }
            for dir in dirs {
                directories.push(
                    source.join(
                        dir.strip_prefix("session/")
                            .ok_or("Invalid archive directory")?,
                    ),
                );
            }
            directories.push(source);
        }
    }
    if !whole {
        return Err("No complete whole-meeting archive; permanent deletion blocked".into());
    }
    // Copies outside the receipt catalog: interrupted archive writes and restore staging.
    for entry in fs::read_dir(&archives)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(aid) = name.strip_suffix(".pending.json") {
            if pending_meeting(&entry.path())? != id {
                continue;
            }
            push_delete_file(root, &entry.path(), cancel, &mut files)?;
            let zip = archives.join(format!("{aid}.zip"));
            if zip.exists() && !archives.join(format!("{aid}.receipt.json")).exists() {
                regular(&zip)?;
                push_delete_file(root, &zip, cancel, &mut files)?;
            }
        } else if let Some(aid) = name.strip_suffix(".zip") {
            if archives.join(format!("{aid}.receipt.json")).exists()
                || archives.join(format!("{aid}.pending.json")).exists()
            {
                continue;
            }
            // Older or foreign copy without attribution: only its own manifest can identify it.
            match verify(&entry.path(), Some(cancel), |_, _| {}) {
                Ok(manifest) if manifest.meeting_id != id => (),
                Ok(_) => push_delete_file(root, &entry.path(), cancel, &mut files)?,
                Err(_) => {
                    return Err(format!(
                        "Unidentified archive copy archives/{name} found; permanent deletion blocked until it is inspected"
                    )
                    .into())
                }
            }
        } else if name.contains(".restoring-") {
            let staging = metadata::checked_directory(&entry.path())?;
            let inner = staging.join(id);
            if !inner.exists() {
                continue;
            }
            metadata::checked_directory(&inner)?;
            let mut found = Vec::new();
            let mut dirs = Vec::new();
            inventory(&inner, &inner, &mut found, &mut dirs)?;
            for (_, path) in found {
                push_delete_file(root, &path, cancel, &mut files)?;
            }
            for dir in dirs {
                directories.push(
                    inner.join(
                        dir.strip_prefix("session/")
                            .ok_or("Invalid staging directory")?,
                    ),
                );
            }
            directories.push(inner);
            directories.push(staging);
        }
    }
    for sidecar in sidecars(root, id)? {
        push_delete_file(root, &root.join(sidecar.name), cancel, &mut files)?;
    }
    // atomic_json keeps the previous label/speaker revision as .json.bak; it can hold the
    // title, speaker names and quotes, so it belongs to the confirmed deletion too.
    for parent in ["user-metadata", "speaker-labels"] {
        let backup = root.join(parent).join(format!("{id}.json.bak"));
        if fs::symlink_metadata(&backup).is_ok() {
            regular(&backup)?;
            push_delete_file(root, &backup, cancel, &mut files)?;
        }
    }
    let copies = root.join("audio-copies");
    if copies.exists() {
        metadata::checked_directory(&copies)?;
        for entry in fs::read_dir(&copies)? {
            let path = entry?.path();
            regular(&path)?;
            let mut bytes = Vec::new();
            File::open(&path)?.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return Err("Unreadable audio-copy catalog; deletion blocked".into());
            }
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if value["meetingId"].as_str() == Some(id) {
                let run = value["runId"]
                    .as_str()
                    .ok_or("Invalid audio-copy identity")?;
                if !metadata::valid_id(run)
                    || path.file_name().and_then(|s| s.to_str())
                        != Some(format!("{run}.json").as_str())
                {
                    return Err("Invalid audio-copy catalog; deletion blocked".into());
                }
                push_delete_file(root, &path, cancel, &mut files)?;
            }
        }
    }
    for parent in ["user-metadata", "speaker-labels"] {
        let history = root.join(parent).join(format!("{id}-history"));
        if history.exists() {
            metadata::checked_directory(&history)?;
            directories.push(history);
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    directories.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    Ok(DeleteInventory {
        files,
        directories,
        newest: newest
            .ok_or("No verified archive date; permanent deletion blocked")?
            .to_rfc3339(),
        legacy_age_evidence,
    })
}
/// Missing optional catalogs are empty; linked, unreadable and non-directory catalogs fail closed.
pub(super) fn optional_entries(path: &Path) -> Result<Vec<std::io::Result<fs::DirEntry>>> {
    metadata::reject_link_if_present(path)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    metadata::checked_directory(path)?;
    let rows: Vec<_> = fs::read_dir(path)?.take(500001).collect();
    if rows.len() > 500000 {
        return Err("Catalog exceeds deletion inventory bound".into());
    }
    Ok(rows)
}
/// Includes every ordinary file and directory of a specifically attributed owned tree.
pub(super) fn add_tree(
    root: &Path,
    folder: &Path,
    cancel: &AtomicBool,
    files: &mut Vec<DeleteFile>,
    directories: &mut Vec<PathBuf>,
) -> Result<()> {
    metadata::checked_directory(folder)?;
    if folder.strip_prefix(root)?.components().count() > 64 {
        return Err("Deletion tree nesting exceeds bound".into());
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Deletion preview cancelled".into());
    }
    for entry in optional_entries(folder)? {
        let path = entry?.path();
        reject_link(&path)?;
        let meta = fs::metadata(&path)?;
        if meta.is_dir() {
            add_tree(root, &path, cancel, files, directories)?;
        } else if meta.is_file() {
            push_delete_file(root, &path, cancel, files)?;
        } else {
            return Err("Non-ordinary meeting file; deletion blocked".into());
        }
    }
    directories.push(folder.to_owned());
    if directories.len() > 500000 {
        return Err("Deletion directory bound exceeded".into());
    }
    Ok(())
}
/// Reads the bounded meeting identity from an unfinished archive record.
pub(super) fn pending_meeting(path: &Path) -> Result<String> {
    regular(path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err("Oversized pending archive record".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let id = value["meetingId"]
        .as_str()
        .ok_or("Invalid pending archive record")?;
    if !metadata::valid_id(id) {
        return Err("Invalid pending archive record".into());
    }
    Ok(id.to_owned())
}
/// Adds one ordinary owned file with its size and hash to the bounded deletion inventory.
pub(super) fn push_delete_file(
    root: &Path,
    path: &Path,
    cancel: &AtomicBool,
    files: &mut Vec<DeleteFile>,
) -> Result<()> {
    let (bytes, sha256) = digest_cancellable(path, Some(cancel))?;
    let canonical = path.canonicalize()?;
    let base = root.canonicalize()?;
    let name = canonical
        .strip_prefix(base)?
        .to_str()
        .ok_or("Invalid deletion path")?
        .replace('\\', "/");
    files.push(DeleteFile {
        name,
        bytes,
        sha256,
    });
    if files.len() > 500000 {
        return Err("Deletion inventory exceeds bound; data retained".into());
    }
    Ok(())
}
/// Fingerprint covers every owned archive version, retained source and user annotation.
pub fn deletion_fingerprint(
    root: &Path,
    id: &str,
    cancel: &AtomicBool,
) -> Result<(String, String)> {
    let inventory = deletion_inventory(root, id, cancel)?;
    Ok((inventory_hash(&inventory)?, inventory.newest))
}
/// Label and speaker-annotation locks for one meeting, held across a destructive operation.
pub(super) fn annotation_locks(root: &Path, id: &str) -> Result<Vec<File>> {
    let metadata_dir = metadata::directory(root)?;
    let mut locks = Vec::new();
    for parent in [metadata_dir, root.join("speaker-labels")] {
        if !parent.exists() {
            fs::create_dir(&parent)?;
        }
        metadata::checked_directory(&parent)?;
        let lock = metadata::open_lock_file(&parent.join(format!("{id}.lock")))?;
        metadata::lock_soon(&lock)?;
        locks.push(lock);
    }
    Ok(locks)
}
/// Deletes only an unchanged verified group after exact confirmation, retaining a resumable progress log.
pub fn delete_archived_meeting(
    root: &Path,
    id: &str,
    expected_meta: u64,
    expected_hash: &str,
    confirmation: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    if !metadata::valid_id(id) || id.starts_with("legacy-") {
        return Err("Invalid archived meeting identity".into());
    }
    let _operation = operation_lock(root)?;
    let _publication = publication_lease(root, true)?;
    if confirmation != format!("DELETE {id}") {
        return Err("Permanent deletion requires the exact meeting confirmation".into());
    }
    let _locks = annotation_locks(root, id)?;
    let labels = metadata::read(root, id)?;
    if labels.pinned || labels.revision != expected_meta {
        return Err("VIP protection or labels changed; permanent deletion blocked".into());
    }
    let mut inventory = deletion_inventory(root, id, cancel)?;
    let hash = inventory_hash(&inventory)?;
    if hash != expected_hash {
        return Err("Archive inventory changed since preview; create a new preview".into());
    }
    let logs = root.join("maintenance-log");
    match fs::create_dir(&logs) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    metadata::checked_directory(&logs)?;
    let log = logs.join(format!("delete-{}.json", uuid::Uuid::new_v4().simple()));
    inventory.files.sort_by_key(|item| {
        (
            item.name.starts_with("user-metadata/") || item.name.starts_with("speaker-labels/"),
            item.name.ends_with(".receipt.json"),
        )
    });
    let mut record = serde_json::json!({"meetingId":id,"confirmation":confirmation,"at":chrono::Utc::now().to_rfc3339(),"inventorySHA256":hash,"inventory":inventory.files,"totalFiles":inventory.files.len(),"deletedFiles":0,"phase":"confirmed"});
    atomic_json(&log, &record)?;
    for (n, item) in inventory.files.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            return Err("Permanent deletion stopped. Some confirmed files may already be deleted; consult the local maintenance log.".into());
        }
        let path = root.join(&item.name);
        let mut parent = root.to_path_buf();
        for part in Path::new(&item.name)
            .parent()
            .ok_or("Invalid deletion parent")?
            .components()
        {
            parent.push(part);
            metadata::checked_directory(&parent)?;
        }
        let (size, hash) = digest_cancellable(&path, Some(cancel))?;
        if size != item.bytes || hash != item.sha256 {
            return Err("A file changed during deletion; remaining files retained. Consult the maintenance log.".into());
        }
        fs::remove_file(path)?;
        record["deletedFiles"] = serde_json::json!(n + 1);
        record["phase"] = serde_json::json!("deleting");
        atomic_json(&log, &record)?;
        progress(n as u64 + 1, inventory.files.len() as u64);
    }
    // Never recursively remove directories: late or unknown files must survive.
    for directory in inventory.directories {
        fs::remove_dir(directory)?;
    }
    record["phase"] = serde_json::json!("complete");
    atomic_json(&log, &record)?;
    Ok(())
}

/// A permanent deletion that stopped before completion, from its own maintenance log.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnfinishedDeletion {
    pub log_id: String,
    pub meeting_id: String,
    pub deleted_files: u64,
    pub total_files: u64,
    pub confirmed_at: String,
}
/// Reads a bounded ordinary deletion log selected by a validated delete-log identifier.
pub(super) fn deletion_log(root: &Path, log_id: &str) -> Result<(PathBuf, serde_json::Value)> {
    if !log_id
        .strip_prefix("delete-")
        .is_some_and(metadata::valid_id)
    {
        return Err("Invalid deletion log identity".into());
    }
    let logs = metadata::checked_directory(&root.join("maintenance-log"))?;
    let path = logs.join(format!("{log_id}.json"));
    regular(&path)?;
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("Oversized deletion log".into());
    }
    Ok((path, serde_json::from_slice(&bytes)?))
}
/// Lists deletions whose log ended before `complete` (cancelled, failed or interrupted).
pub fn unfinished_deletions(root: &Path) -> Result<Vec<UnfinishedDeletion>> {
    let logs = root.join("maintenance-log");
    if !logs.exists() {
        return Ok(Vec::new());
    }
    metadata::checked_directory(&logs)?;
    let mut result = Vec::new();
    for entry in fs::read_dir(&logs)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let Some(log_id) = name.strip_suffix(".json") else {
            continue;
        };
        if !log_id.starts_with("delete-") {
            continue;
        }
        let Ok((_, record)) = deletion_log(root, log_id) else {
            continue;
        };
        if !matches!(record["phase"].as_str(), Some("confirmed" | "deleting")) {
            continue;
        }
        let Some(meeting_id) = record["meetingId"]
            .as_str()
            .filter(|m| metadata::valid_id(m))
        else {
            continue;
        };
        result.push(UnfinishedDeletion {
            log_id: log_id.to_owned(),
            meeting_id: meeting_id.to_owned(),
            deleted_files: record["deletedFiles"].as_u64().unwrap_or(0),
            total_files: record["totalFiles"].as_u64().unwrap_or(0),
            confirmed_at: record["at"].as_str().unwrap_or_default().to_owned(),
        });
        if result.len() > 1000 {
            break;
        }
    }
    result.sort_by(|a, b| b.confirmed_at.cmp(&a.confirmed_at));
    Ok(result)
}
/// Continue a confirmed permanent deletion from its own log. Only the logged files are touched:
/// absent ones count as done, present ones must still match their logged size and SHA-256.
pub fn resume_deletion(
    root: &Path,
    log_id: &str,
    confirmation: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<String> {
    let _operation = operation_lock(root)?;
    let _publication = publication_lease(root, true)?;
    let (log, mut record) = deletion_log(root, log_id)?;
    if !matches!(record["phase"].as_str(), Some("confirmed" | "deleting")) {
        return Err("This deletion is not unfinished".into());
    }
    let id = record["meetingId"]
        .as_str()
        .filter(|m| metadata::valid_id(m) && !m.starts_with("legacy-"))
        .ok_or("Invalid deletion log identity")?
        .to_owned();
    if confirmation != format!("DELETE {id}") {
        return Err("Permanent deletion requires the exact meeting confirmation".into());
    }
    if root.join("sessions").join(&id).exists() {
        return Err("The meeting is back in normal history; deletion stays stopped".into());
    }
    let _locks = annotation_locks(root, &id)?;
    let files: Vec<DeleteFile> = serde_json::from_value(record["inventory"].clone())?;
    let labels = format!("user-metadata/{id}.json");
    match metadata::read(root, &id) {
        Ok(meta) if meta.pinned => {
            return Err("VIP protection was set; deletion stays stopped".into());
        }
        Ok(_) => (),
        // The confirmed run already removed the current labels; their history may remain.
        Err(_) if !root.join(&labels).exists() && files.iter().any(|f| f.name == labels) => (),
        Err(e) => return Err(e),
    }
    let base = root.canonicalize()?;
    let total = files.len() as u64;
    let mut parents = HashSet::new();
    for (n, item) in files.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            return Err("Permanent deletion stopped again. Remaining confirmed files are retained; it can be continued.".into());
        }
        let relative = Path::new(&item.name);
        if relative.components().count() < 2
            || !relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Invalid path in deletion log; remaining files retained".into());
        }
        let mut parent = root.to_path_buf();
        let mut missing_parent = false;
        for part in relative
            .parent()
            .ok_or("Invalid deletion parent")?
            .components()
        {
            parent.push(part);
            if !parent.exists() {
                missing_parent = true;
                break;
            }
            metadata::checked_directory(&parent)?;
        }
        let path = root.join(relative);
        if !missing_parent && fs::symlink_metadata(&path).is_ok() {
            regular(&path)?;
            if !path.canonicalize()?.starts_with(&base) {
                return Err("Deletion path left the storage folder; retained".into());
            }
            let (size, hash) = digest_cancellable(&path, Some(cancel))?;
            if size != item.bytes || hash != item.sha256 {
                return Err("A file changed since the deletion was confirmed; remaining files retained. Consult the maintenance log.".into());
            }
            fs::remove_file(&path)?;
        }
        let mut dir = relative.parent();
        while let Some(d) = dir.filter(|d| d.components().count() > 1) {
            parents.insert(d.to_path_buf());
            dir = d.parent();
        }
        record["deletedFiles"] = serde_json::json!(n + 1);
        record["phase"] = serde_json::json!("deleting");
        record["resumedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        atomic_json(&log, &record)?;
        progress(n as u64 + 1, total);
    }
    // Only now-empty folders below the top-level storage folders; never a recursive delete.
    let mut parents: Vec<_> = parents.into_iter().collect();
    parents.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in parents {
        let _ = fs::remove_dir(root.join(dir));
    }
    record["phase"] = serde_json::json!("complete");
    atomic_json(&log, &record)?;
    Ok(id)
}
