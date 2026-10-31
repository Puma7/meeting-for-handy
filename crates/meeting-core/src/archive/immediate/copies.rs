//! Attribution for manual erasure, including remaining bytes after an interruption.
//! Automatic retention's complete-archive/age/VIP checks remain separate and unchanged.
use super::*;
use std::collections::BTreeSet;

/// Checks both available manifest generations without requiring incomplete staging
/// to be replayable. A readable foreign identity always preserves the entire tree,
/// even when that identity is ineligible to authorize native erasure itself.
pub(super) fn reject_conflicting_identity(folder: &Path, id: &str) -> Result<()> {
    for name in ["session.json", "session.json.bak"] {
        let path = folder.join(name);
        metadata::reject_link_if_present(&path)?;
        if let Ok(value) = read_record(&path) {
            if value["id"].as_str().is_some_and(|found| found != id) {
                return Err("Source manifest belongs to another meeting; erasure blocked".into());
            }
        }
    }
    Ok(())
}

/// Reads only identity, without reopening/repairing journals and without imposing
/// the visible sessions/{id} name on a retained archives/{archive}.source folder.
pub(super) fn manifest_id(folder: &Path) -> Result<String> {
    metadata::checked_directory(folder)?;
    let value = read_record(&folder.join("session.json"))
        .or_else(|_| read_record(&folder.join("session.json.bak")))?;
    let id = value["id"]
        .as_str()
        .filter(|id| native_id(id))
        .map(str::to_owned)
        .ok_or("Missing native source identity")?;
    // The primary generation is not permission to ignore a readable backup.
    // Reconcile before this identity can filter an orphan out of the inventory.
    reject_conflicting_identity(folder, &id)?;
    Ok(id)
}

/// Pending creation or a confirmed retry marker attributes incomplete ZIP bytes.
/// Every readable identity must agree even when the full archive verifies successfully.
fn reject_conflicting_zip_identity(path: &Path, id: &str, cancel: &AtomicBool) -> Result<()> {
    regular(path)?;
    let Ok(mut reader) = zip::ZipArchive::new(File::open(path)?) else {
        return Ok(()); // A process interruption may leave no central directory.
    };
    if reader.len() > MAX_FILES * 2 + 1 {
        return Err("Too many archive entries".into());
    }
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for n in 0..reader.len() {
        if cancel.load(Ordering::Acquire) {
            return Err("Erasure preview cancelled".into());
        }
        let mut file = reader.by_index(n)?;
        let name = file.name()?.into_owned();
        if ![
            "ARCHIVE_MANIFEST.json",
            "session/session.json",
            "session/session.json.bak",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        if !seen.insert(name) {
            return Err("Duplicate archive identity record; erasure blocked".into());
        }
        total = total
            .checked_add(file.size())
            .ok_or("Archive identity size overflow")?;
        if total > MAX_MANIFEST {
            return Err("Oversized archive identity evidence; erasure blocked".into());
        }
        let mut bytes = Vec::new();
        if file
            .by_ref()
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue; // An unfinished entry is not readable identity evidence.
        }
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("Oversized archive identity evidence; erasure blocked".into());
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            for found in [&value["meetingId"], &value["session"]["id"], &value["id"]] {
                if found.as_str().is_some_and(|found| found != id) {
                    return Err(
                        "Archive ZIP conflicts with its attributed meeting; erasure blocked".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Collects archive identities without trusting paths embedded in any record.
pub(super) fn targets(
    root: &Path,
    id: &str,
    known: &[String],
    cancel: &AtomicBool,
) -> Result<Vec<String>> {
    let folder = root.join("archives");
    let mut ids: BTreeSet<String> = known.iter().cloned().collect();
    let mut assigned = BTreeMap::new();
    let mut pending = BTreeSet::new();
    let mut assign = |aid: &str, meeting: String| -> Result<()> {
        if !native_id(aid) || !native_id(&meeting) {
            return Err("Invalid archive identity".into());
        }
        if assigned
            .insert(aid.to_owned(), meeting.clone())
            .is_some_and(|prior| prior != meeting)
        {
            return Err("Conflicting archive identities; erasure blocked".into());
        }
        if meeting == id {
            ids.insert(aid.to_owned());
        } else if ids.contains(aid) {
            return Err("Previously attributed archive now belongs to another meeting".into());
        }
        Ok(())
    };
    // Receipts and pending creation records remain authoritative even when some bytes
    // have already been removed. They are identity-checked, never executed as paths.
    for entry in optional_entries(&folder)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid archive name")?;
        if let Some(aid) = name.strip_suffix(".receipt.json") {
            assign(aid, receipt_at(&folder, aid)?.meeting_id)?;
        } else if let Some(aid) = name.strip_suffix(".pending.json") {
            assign(aid, deletion::pending_meeting(&path)?)?;
            pending.insert(aid.to_owned());
        }
    }
    // An orphan ZIP/source requires its own valid manifest, unless a prior confirmed
    // erasure already recorded that archive's identity for interruption recovery.
    for entry in optional_entries(&folder)? {
        if cancel.load(Ordering::Acquire) {
            return Err("Erasure preview cancelled".into());
        }
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid archive name")?;
        let candidate = name
            .strip_suffix(".zip")
            .or_else(|| name.strip_suffix(".source"));
        if let Some(aid) = candidate {
            if !native_id(aid) {
                return Err("Invalid archive copy identity".into());
            }
            if let Some(expected) = assigned
                .get(aid)
                .map(String::as_str)
                .or_else(|| ids.contains(aid).then_some(id))
            {
                if expected != id {
                    // A foreign receipt or earlier orphan is not permission to
                    // skip a contradictory copy. Reconcile readable identities
                    // before filtering, without requiring unrelated ZIP data to replay.
                    if name.ends_with(".zip") {
                        reject_conflicting_zip_identity(&path, expected, cancel)?;
                    } else {
                        metadata::checked_directory(&path)?;
                        reject_conflicting_identity(&path, expected)?;
                    }
                    continue;
                }
                if name.ends_with(".zip") {
                    match verify(&path, Some(cancel), |_, _| {}) {
                        Ok(manifest) if manifest.meeting_id != expected => {
                            return Err("Archive ZIP conflicts with its attributed meeting; erasure blocked".into());
                        }
                        Ok(_) => reject_conflicting_zip_identity(&path, expected, cancel)?,
                        Err(_) if pending.contains(aid) || known.iter().any(|id| id == aid) => {
                            reject_conflicting_zip_identity(&path, expected, cancel)?;
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    metadata::checked_directory(&path)?;
                    match manifest_id(&path) {
                        Ok(found) if found != expected => {
                            return Err("Retained source identity mismatch; erasure blocked".into())
                        }
                        Ok(_) => (),
                        Err(_) if known.contains(&aid.to_owned()) => (), // Partial confirmed removal.
                        Err(error) => return Err(error),
                    }
                }
                continue;
            }
            let meeting = if name.ends_with(".zip") {
                let meeting = verify(&path, Some(cancel), |_, _| {})?.meeting_id;
                reject_conflicting_zip_identity(&path, &meeting, cancel)?;
                meeting
            } else {
                manifest_id(&path)?
            };
            assigned.insert(aid.into(), meeting.clone());
            if meeting == id {
                ids.insert(aid.into());
            }
        }
    }
    Ok(ids.into_iter().collect())
}
/// Adds fixed namespaces for targets already attributed under the caller's exclusive
/// archive/publication leases. Each file still receives its own inventory hash.
pub(super) fn inventory(
    root: &Path,
    id: &str,
    checked_targets: Vec<String>,
    cancel: &AtomicBool,
) -> Result<Inventory> {
    metadata::checked_directory(root)?;
    let mut data = Inventory {
        files: Vec::new(),
        directories: Vec::new(),
        archive_ids: checked_targets,
        preserved_atomic_remnants: 0,
    };
    for path in [
        root.join("sessions").join(id),
        root.join("user-metadata").join(format!("{id}-history")),
        root.join("speaker-labels").join(format!("{id}-history")),
    ] {
        metadata::reject_link_if_present(&path)?;
        if path.exists() {
            add_tree(root, &path, cancel, &mut data.files, &mut data.directories)?;
        }
    }
    for parent in ["user-metadata", "speaker-labels"] {
        metadata::reject_link_if_present(&root.join(parent))?;
        for suffix in [".json", ".json.bak"] {
            let path = root.join(parent).join(format!("{id}{suffix}"));
            metadata::reject_link_if_present(&path)?;
            if path.exists() {
                push_delete_file(root, &path, cancel, &mut data.files)?;
            }
        }
        // These legacy atomic payloads have no meeting identity field. Guessing
        // ownership from their titles or quotes could delete another meeting's data.
        for entry in optional_entries(&root.join(parent))? {
            let path = entry?.path();
            if records::is_temporary(&path)? {
                regular(&path)?;
                data.preserved_atomic_remnants += 1;
            }
        }
    }
    let archives = root.join("archives");
    for entry in optional_entries(&archives)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid archive name")?;
        if name == ".leases" {
            // Relocation leases are a stable coordination directory, not an
            // interrupted JSON file. Preserve only its documented empty locks;
            // unknown content must never be hidden by this namespace exception.
            metadata::checked_directory(&path)?;
            for entry in fs::read_dir(&path)? {
                let lock = entry?.path();
                let stem = lock
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_suffix(".lock"))
                    .ok_or("Unknown archive lease content; erasure blocked")?;
                if stem.len() != 64
                    || !stem
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || regular(&lock)?.len() != 0
                {
                    return Err("Unknown archive lease content; erasure blocked".into());
                }
            }
            continue;
        }
        if records::is_temporary(&path)? {
            match records::temporary_identities(&path, true)? {
                Some(ids) if ids.len() == 1 && ids.contains(id) => {
                    push_delete_file(root, &path, cancel, &mut data.files)?;
                }
                Some(ids) if ids.len() == 1 => (),
                _ => data.preserved_atomic_remnants += 1,
            }
            continue;
        }
        if let Some(aid) = data
            .archive_ids
            .iter()
            .find(|aid| name.starts_with(&format!("{aid}.")))
        {
            let suffix = name
                .strip_prefix(aid.as_str())
                .ok_or("Archive identity mismatch")?;
            if suffix == ".source" {
                add_tree(root, &path, cancel, &mut data.files, &mut data.directories)?;
            } else if [
                ".zip",
                ".receipt.json",
                ".pending.json",
                ".placement.json",
                ".placement.json.bak",
            ]
            .contains(&suffix)
                || (1..=restore::MAX_PLACEMENT_ATTEMPTS)
                    .any(|n| path == placement_attempt(&archives, aid, n))
            {
                push_delete_file(root, &path, cancel, &mut data.files)?;
            } else if suffix.starts_with(".restoring-") {
                metadata::checked_directory(&path)?;
                let inner = path.join(id);
                metadata::reject_link_if_present(&inner)?;
                if inner.exists() {
                    reject_conflicting_identity(&inner, id)?;
                    add_tree(root, &inner, cancel, &mut data.files, &mut data.directories)?;
                }
                // Removing the parent is safe only when it is empty; late/foreign files survive.
                data.directories.push(path);
            } else {
                return Err("Unrecognized attributed archive artifact; erasure blocked".into());
            }
        } else if name.contains(".restoring-") {
            metadata::checked_directory(&path)?;
            let inner = path.join(id);
            metadata::reject_link_if_present(&inner)?;
            if inner.exists() {
                reject_conflicting_identity(&inner, id)?;
                add_tree(root, &inner, cancel, &mut data.files, &mut data.directories)?;
                data.directories.push(path);
            }
        }
    }
    for entry in optional_entries(&root.join("audio-copies"))? {
        let path = entry?.path();
        if records::is_temporary(&path)? {
            match records::temporary_identities(&path, true)? {
                Some(ids) if ids.len() == 1 && ids.contains(id) => {
                    push_delete_file(root, &path, cancel, &mut data.files)?
                }
                Some(ids) if ids.len() == 1 => (),
                _ => data.preserved_atomic_remnants += 1,
            }
            continue;
        }
        let value = read_record(&path)?;
        if value["meetingId"].as_str() == Some(id) {
            let run = value["runId"]
                .as_str()
                .filter(|s| native_id(s))
                .ok_or("Invalid audio-copy identity")?;
            if path.file_name().and_then(|s| s.to_str()) != Some(format!("{run}.json").as_str()) {
                return Err("Audio-copy identity mismatch".into());
            }
            push_delete_file(root, &path, cancel, &mut data.files)?;
        }
    }
    Ok(data)
}
