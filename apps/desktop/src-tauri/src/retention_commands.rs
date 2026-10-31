use crate::{blocking, AppResult, Service};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::State;
fn plans(root: &Path) -> meeting_core::Result<PathBuf> {
    meeting_core::metadata::checked_directory(root)?;
    let folder = root.join("retention-plans");
    match std::fs::create_dir(&folder) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    meeting_core::metadata::checked_directory(&folder)
}
#[tauri::command]
pub(crate) async fn get_retention_policy(
    service: State<'_, Arc<Service>>,
) -> AppResult<meeting_core::retention::Policy> {
    let service = Arc::clone(&service);
    blocking(move||meeting_core::retention::read_policy(&service.root).map_err(|_|"Aufräumregeln konnten nicht gelesen werden; vorhandene Einstellungen werden nicht überschrieben. / Retention settings could not be read; existing settings are preserved.".into())).await
}
/// Save retention settings off the UI thread through revision and native FFmpeg-choice
/// validation.
#[tauri::command]
pub(crate) async fn save_retention_policy(
    service: State<'_, Arc<Service>>,
    expected_revision: u64,
    policy: meeting_core::retention::Policy,
) -> AppResult<meeting_core::retention::Policy> {
    let service = Arc::clone(&service);
    blocking(move || save_policy(&service, expected_revision, policy)).await
}
/// Hold mutation admission across optional FFmpeg grant verification and revisioned policy
/// saving.
pub(crate) fn save_policy(
    service: &Service,
    expected_revision: u64,
    policy: meeting_core::retention::Policy,
) -> AppResult<meeting_core::retention::Policy> {
    let _lease = service.reserve_operation(crate::operations::Kind::Mutation)?;
    if let Some(path) = policy.ffmpeg_executable.as_deref() {
        service
            .selections
            .verify(crate::path_selection::Purpose::Ffmpeg, path)?;
    }
    meeting_core::retention::save_policy(&service.root,expected_revision,policy).map_err(|_|"Aufräumregeln nicht gespeichert. Unangetasteten Zeitraum, Altersgrenzen und geänderte Einstellungen prüfen. / Retention settings were not saved; check untouched period, age limits and concurrent changes.".into())
}

/// Create a cancellable inventory plan under indexing admission without applying archive,
/// conversion or deletion actions.
#[tauri::command]
pub(crate) fn preview_retention(service: State<'_, Arc<Service>>) -> AppResult<String> {
    let owned = Arc::clone(&service);
    let lease = service.reserve_operation(crate::operations::Kind::Index)?;
    service
        .maintenance_jobs
        .start_work("cleanup-preview", move |id, jobs, cancel| {
            let _lease = lease;
            publish_preview(&owned.root, id, cancel, |done, total| {
                jobs.update_work(id, done, total)
            })
        })
}
/// Keeps construction and publication in one native operation; plans contain original titles.
pub(crate) fn publish_preview(
    root: &Path,
    id: &str,
    cancel: &std::sync::atomic::AtomicBool,
    progress: impl FnMut(u64, u64),
) -> meeting_core::Result<()> {
    crate::validate_id(id)?;
    // Erasure takes exclusive publication admission. Share this lease through
    // the final write without excluding unrelated metadata or speaker edits.
    let _source = meeting_core::archive::source_publication_lease(root)?;
    let mut plan = meeting_core::retention::preview(root, chrono_now(), cancel, progress)?;
    plan.id = id.to_owned();
    crate::create_json(&plans(root)?.join(format!("{id}.json")), &plan).map_err(|e| e.into())
}
fn chrono_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}
#[tauri::command]
pub(crate) async fn get_retention_plan(
    service: State<'_, Arc<Service>>,
    job_id: String,
) -> AppResult<meeting_core::retention::Plan> {
    let service = Arc::clone(&service);
    blocking(move || {
        crate::validate_id(&job_id)?;
        let folder = plans(&service.root).map_err(|_| "Preview storage unavailable")?;
        let path = folder.join(format!("{job_id}.json"));
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "Preview is not complete yet")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Linked preview rejected".into());
            }
        }
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 32 * 1024 * 1024 {
            return Err("Invalid preview file".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "Preview unavailable")?
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Preview unavailable")?;
        let plan: meeting_core::retention::Plan = serde_json::from_slice(&bytes)
            .map_err(|_| "Preview is invalid; original recordings are preserved")?;
        if plan.id != job_id {
            return Err("Preview identity mismatch; create a new preview".into());
        }
        Ok(plan)
    })
    .await
}
#[tauri::command]
pub(crate) fn get_maintenance_progress(
    service: State<'_, Arc<Service>>,
) -> Option<crate::library_service::Progress> {
    service.maintenance_jobs.snapshot()
}
#[tauri::command]
pub(crate) fn cancel_maintenance(service: State<'_, Arc<Service>>, job_id: String) -> bool {
    service.maintenance_jobs.cancel(&job_id)
}
/// Reserve all operations that could move sources before starting the background job.
pub(super) struct MaintenanceGuard {
    service: Arc<Service>,
    _lease: crate::operations::Lease,
}
impl Drop for MaintenanceGuard {
    /// Clear the maintenance transition when its guard leaves scope; the guard also retains
    /// operation admission.
    fn drop(&mut self) {
        if let Ok(mut inner) = self.service.lock() {
            inner.transitioning = false;
            inner.maintenance_reserved = false;
        }
    }
}
/// Acquire exclusive maintenance admission, release the cached session view and mark its
/// transition.
pub(super) fn reserve(service: &Arc<Service>) -> AppResult<MaintenanceGuard> {
    let lease = service.reserve_operation(crate::operations::Kind::Maintenance)?;
    let mut inner = service.lock()?;
    inner.view = None;
    inner.transitioning = true;
    inner.maintenance_reserved = true;
    Ok(MaintenanceGuard {
        service: service.clone(),
        _lease: lease,
    })
}
#[tauri::command]
pub(crate) async fn list_archives(
    service: State<'_, Arc<Service>>,
    offset: usize,
    limit: usize,
) -> AppResult<meeting_core::archive::ArchivePage> {
    let service = Arc::clone(&service);
    blocking(move||meeting_core::archive::list(&service.root,offset,limit).map_err(|_|"Archive list could not be read. Existing files are preserved; inspect the local archive folder. / Archivliste konnte nicht gelesen werden. Vorhandene Dateien bleiben erhalten.".into())).await
}
#[tauri::command]
pub(crate) async fn list_unfinished_deletions(
    service: State<'_, Arc<Service>>,
) -> AppResult<Vec<meeting_core::archive::UnfinishedDeletion>> {
    let service = Arc::clone(&service);
    blocking(move || {
        meeting_core::archive::unfinished_deletions(&service.root).map_err(|_| {
            "Unfinished deletions could not be read; files are preserved. / Unterbrochene Löschungen konnten nicht gelesen werden; Dateien bleiben erhalten.".into()
        })
    })
    .await
}
/// Continue a stopped, already confirmed permanent deletion; the typed confirmation is required again.
#[tauri::command(async)]
pub(crate) fn resume_permanent_deletion(
    service: State<'_, Arc<Service>>,
    log_id: String,
    confirmation: String,
) -> AppResult<String> {
    let owned = Arc::clone(&service);
    let guard = reserve(&owned)?;
    service
        .maintenance_jobs
        .start_work("permanent-deletion-resume", move |id, jobs, cancel| {
            let _guard = guard;
            let meeting = meeting_core::archive::resume_deletion(
                &owned.root,
                &log_id,
                &confirmation,
                cancel,
                |n, total| jobs.update_work(id, n, total),
            )?;
            meeting_core::library::LibraryIndex::open(&owned.root)?.remove(&meeting)?;
            Ok(())
        })
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
#[tauri::command(async)]
pub(crate) fn restore_archive(
    service: State<'_, Arc<Service>>,
    archive_id: String,
    confirmed: bool,
) -> AppResult<String> {
    if !confirmed {
        return Err("Restoration needs confirmation".into());
    }
    crate::validate_id(&archive_id)?;
    let owned = Arc::clone(&service);
    let guard = reserve(&owned)?;
    service
        .maintenance_jobs
        .start_work("archive-restore", move |id, jobs, cancel| {
            let _guard = guard;
            jobs.update_work(id, 0, 1);
            let folder = meeting_core::archive::restore(&owned.root, &archive_id, cancel)?;
            let data = meeting_core::SessionStore::open(&folder)?.snapshot()?;
            crate::library_service::update(&owned.root, &data)?;
            jobs.update_work(id, 1, 1);
            Ok(())
        })
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
#[tauri::command(async)]
pub(crate) fn compact_archive(
    service: State<'_, Arc<Service>>,
    archive_id: String,
    confirmed: bool,
) -> AppResult<String> {
    if !confirmed {
        return Err("Compaction needs explicit confirmation".into());
    }
    crate::validate_id(&archive_id)?;
    let owned = Arc::clone(&service);
    let guard = reserve(&owned)?;
    service
        .maintenance_jobs
        .start_work("archive-compact", move |id, jobs, cancel| {
            let _guard = guard;
            meeting_core::archive::compact_source_copy(
                &owned.root,
                &archive_id,
                cancel,
                |n, total| jobs.update_work(id, n, total),
            )
        })
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
/// Apply one confirmed row of an unexpired unchanged plan under maintenance admission; recheck
/// policy, VIP labels and source evidence before the action.
#[tauri::command(async)]
pub(crate) fn apply_retention(
    service: State<'_, Arc<Service>>,
    plan_id: String,
    candidate_index: usize,
    confirmation: String,
) -> AppResult<String> {
    crate::validate_id(&plan_id)?;
    let owned = Arc::clone(&service);
    let guard = reserve(&owned)?;
    service.maintenance_jobs.start_work("retention-apply",move|id,jobs,cancel|{
        let _guard=guard;
        let folder=plans(&owned.root)?;
        let path=folder.join(format!("{plan_id}.json"));
        let meta=std::fs::symlink_metadata(&path)?;
        #[cfg(windows)]{use std::os::windows::fs::MetadataExt;if meta.file_attributes()&0x400!=0{return Err("Linked preview rejected".into());}}
        if !meta.is_file()||meta.file_type().is_symlink()||meta.len()>32*1024*1024{return Err("Invalid preview".into());}
        let mut bytes=Vec::new();std::fs::File::open(path)?.take(32*1024*1024+1).read_to_end(&mut bytes)?;
        let plan:meeting_core::retention::Plan=serde_json::from_slice(&bytes)?;
        let age=chrono::Utc::now()-chrono::DateTime::parse_from_rfc3339(&plan.created_at)?.with_timezone(&chrono::Utc);
        if plan.id!=plan_id || age<chrono::Duration::zero() || age>chrono::Duration::hours(24){return Err("Preview expired or mismatched; create a new preview".into());}
        let policy=meeting_core::retention::read_policy(&owned.root)?;
        if policy.revision!=plan.policy_revision{return Err("Rules changed; create a new preview".into());}
        let candidate=plan.candidates.get(candidate_index).ok_or("Unknown preview row")?;
        use meeting_core::retention::Action;
        let deleting=candidate.action==Action::DeleteArchivedMeeting;
        let expected=if deleting{format!("DELETE {}",candidate.meeting_id)}else{format!("APPLY {plan_id} {candidate_index}")};
        if confirmation!=expected{return Err("Explicit confirmation did not match the preview action".into());}
        let labels=meeting_core::metadata::read(&owned.root,&candidate.meeting_id)?;
        if labels.pinned||labels.revision!=candidate.metadata_revision{return Err("VIP protection or labels changed; create a new preview".into());}
        let source=owned.root.join("sessions").join(&candidate.meeting_id);
        if !deleting && meeting_core::retention::manifest_fingerprint(&source)?!=candidate.original_manifest_sha256{return Err("Meeting files changed; create a new preview".into());}
        // One immutable preview can be applied once. A partial failure requires a fresh inventory.
        const APPLIED:&str="Diese Vorschau wurde bereits angewendet. Bitte eine neue Vorschau erstellen. / This preview was already applied; create a new preview.";
        let applied=folder.join(format!("{plan_id}.applied.json"));
        if applied.exists(){return Err(APPLIED.into());}
        crate::create_json(&applied,&serde_json::json!({"jobId":id,"candidateIndex":candidate_index,"confirmation":confirmation,"at":chrono::Utc::now().to_rfc3339()})).map_err(|e|if applied.exists(){APPLIED.to_string()}else{e})?;
        jobs.update_work(id,0,0);
        match candidate.action {
            Action::ArchiveMeeting=>{
                if meeting_core::archive::read_archived_transcript(&source)?.is_some(){meeting_core::archive::restore_audio(&owned.root,&candidate.meeting_id,cancel)?;}
                let receipt=meeting_core::archive::create(&owned.root,&candidate.meeting_id,labels.revision,"whole-meeting",cancel,|n,total|jobs.update_work(id,n,total))?;
                meeting_core::archive::move_source_into_archive(&owned.root,&receipt,labels.revision,cancel)?;
                meeting_core::library::LibraryIndex::open(&owned.root)?.remove(&candidate.meeting_id)?;
            },
            Action::ArchiveAudio=>{meeting_core::archive::archive_audio(&owned.root,&candidate.meeting_id,labels.revision,cancel,|n,total|jobs.update_work(id,n,total))?;
                let data=meeting_core::archive::read_archived_transcript(&source)?.ok_or("Archived transcript unavailable")?;crate::library_service::update(&owned.root,&data)?;},
            Action::ConvertAudio=>{
                let path=policy.ffmpeg_executable.as_ref().ok_or("Choose a separately installed FFmpeg executable in retention settings first")?;
                owned.selections.verify(crate::path_selection::Purpose::Ffmpeg,path)?;
                let tool=meeting_core::audio_codec::AudioTool::inspect(path,cancel)?;
                let store=meeting_core::SessionStore::open(&source)?;let data=store.snapshot()?;
                if data.status!="complete"||data.read_only||data.capture_incomplete{return Err("Restore archived audio and finish the meeting before converting".into());}
                let base=source.join("converted-audio");match std::fs::create_dir(&base){Ok(())=>(),Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>(),Err(e)=>return Err(e.into())}
                meeting_core::metadata::checked_directory(&base)?;
                let output=base.join(id);std::fs::create_dir(&output)?;
                let mut proofs=Vec::new();jobs.update_work(id,0,data.parts.len()as u64);
                for part in &data.parts{proofs.push(tool.convert(&source,part,&output,policy.codec,cancel)?);jobs.update_work(id,proofs.len()as u64,data.parts.len()as u64);}
                crate::create_json(&output.join("conversion.json"),&serde_json::json!({"meetingId":candidate.meeting_id,"jobId":id,"originalsRetained":true,"codec":policy.codec,"parts":proofs}))?;
                let catalog=owned.root.join("audio-copies");match std::fs::create_dir(&catalog){Ok(())=>(),Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>(),Err(e)=>return Err(e.into())}
                meeting_core::metadata::checked_directory(&catalog)?;
                crate::create_json(&catalog.join(format!("{id}.json")),&AudioCopy{run_id:id.into(),meeting_id:candidate.meeting_id.clone(),title:candidate.title.clone(),codec:policy.codec,created_at:chrono::Utc::now().to_rfc3339(),sections:proofs.len()})?;
            },
            Action::DeleteArchivedMeeting=>{
                meeting_core::archive::delete_archived_meeting(&owned.root,&candidate.meeting_id,labels.revision,&candidate.original_manifest_sha256,&confirmation,cancel,|n,total|jobs.update_work(id,n,total))?;
                meeting_core::library::LibraryIndex::open(&owned.root)?.remove(&candidate.meeting_id)?;
            }
        }
        Ok(())
    })
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioCopy {
    run_id: String,
    meeting_id: String,
    title: String,
    codec: meeting_core::audio_codec::Codec,
    created_at: String,
    sections: usize,
}
fn read_record<T: serde::de::DeserializeOwned>(path: &Path) -> meeting_core::Result<T> {
    let meta = std::fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Linked record rejected".into());
        }
    }
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 32 * 1024 * 1024 {
        return Err("Invalid retained record".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Oversized retained record".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
#[tauri::command]
/// Returns 25 verified page records and separate catalogue pagination metadata.
/// A damaged next-page record cannot hide this page; retained files are never altered.
pub(crate) async fn list_audio_copies(
    service: State<'_, Arc<Service>>,
    offset: usize,
) -> AppResult<AudioCopyPage> {
    let owned = Arc::clone(&service);
    blocking(move || audio_copy_page(&owned.root, offset)).await
}

#[derive(Default, serde::Serialize)]
pub(crate) struct AudioCopyPage {
    items: Vec<AudioCopy>,
    more: bool,
}

/// Validate only the requested page. `more` counts remaining catalogue names without
/// opening or trusting their contents; every record is validated when its page is requested.
fn audio_copy_page(root: &Path, offset: usize) -> AppResult<AudioCopyPage> {
    if offset > 1000000 {
        return Err("Invalid audio-copy page".into());
    }
    let folder = root.join("audio-copies");
    if !folder.exists() {
        return Ok(AudioCopyPage::default());
    }
    meeting_core::metadata::checked_directory(&folder)
        .map_err(|_| "Audio-copy catalog unavailable")?;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&folder).map_err(|_| "Audio-copy catalog unavailable")? {
        let path = entry.map_err(|_| "Audio-copy catalog unavailable")?.path();
        if path.extension().is_some_and(|v| v == "json") {
            names.push(path);
        }
        if names.len() > 1000000 {
            return Err("Audio-copy catalog exceeds bound".into());
        }
    }
    names.sort();
    let mut output = Vec::new();
    let more = names.len().saturating_sub(offset) > 25;
    for path in names.iter().skip(offset).take(25) {
        let value: AudioCopy = read_record(path)
            .map_err(|_| "Audio-copy catalog is unreadable; retained files preserved")?;
        crate::validate_id(&value.run_id)?;
        crate::validate_id(&value.meeting_id)?;
        if path.file_stem().and_then(|v| v.to_str()) != Some(value.run_id.as_str()) {
            return Err("Audio-copy identity mismatch".into());
        }
        output.push(value);
    }
    Ok(AudioCopyPage {
        items: output,
        more,
    })
}
#[cfg(test)]
mod audio_copy_page_tests {
    use super::*;

    #[test]
    /// A damaged next-page entry must not hide valid retained records, yet remains an
    /// error when explicitly requested. No catalogue file or audio copy is repaired or removed.
    fn invalid_lookahead_keeps_the_current_audio_copy_page_readable() {
        for invalid in ["json", "identity", "directory"] {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "handymeeting-copy-page-{}-{nonce}",
                std::process::id()
            ));
            let folder = root.join("audio-copies");
            std::fs::create_dir_all(&folder).unwrap();
            for number in 0..25 {
                let run_id = format!("{number:032x}");
                crate::create_json(
                    &folder.join(format!("{run_id}.json")),
                    &AudioCopy {
                        run_id,
                        meeting_id: "b".repeat(32),
                        title: "Synthetic retained copy".into(),
                        codec: meeting_core::audio_codec::Codec::Flac,
                        created_at: "2026-10-29T00:00:00Z".into(),
                        sections: 1,
                    },
                )
                .unwrap();
            }
            let bad = folder.join(format!("{:032x}.json", 25));
            match invalid {
                "json" => std::fs::write(&bad, b"{incomplete").unwrap(),
                "identity" => crate::create_json(
                    &bad,
                    &AudioCopy {
                        run_id: "c".repeat(32),
                        meeting_id: "b".repeat(32),
                        title: "Synthetic mismatch".into(),
                        codec: meeting_core::audio_codec::Codec::Flac,
                        created_at: "2026-10-29T00:00:00Z".into(),
                        sections: 1,
                    },
                )
                .unwrap(),
                _ => std::fs::create_dir(&bad).unwrap(),
            }
            let before: Vec<_> = (0..26)
                .map(|number| {
                    let path = folder.join(format!("{number:032x}.json"));
                    (path.is_dir(), std::fs::read(path).ok())
                })
                .collect();
            let first =
                audio_copy_page(&root, 0).expect("valid current page must remain accessible");
            assert_eq!(first.items.len(), 25);
            assert!(first.more, "the unreadable next page must remain reachable");
            assert!(
                audio_copy_page(&root, 25).is_err(),
                "{invalid}: requested invalid entries still fail closed"
            );
            for (number, previous) in before.into_iter().enumerate() {
                let path = folder.join(format!("{number:032x}.json"));
                assert_eq!((path.is_dir(), std::fs::read(path).ok()), previous);
            }
            assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 26);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    /// Exact full and short final pages expose no empty next page. Pagination is JSON
    /// metadata, and counts are bounded independently from each returned record's validity.
    fn audio_copy_page_metadata_handles_full_short_and_missing_catalogues() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "handymeeting-copy-page-count-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let empty = audio_copy_page(&root, 0).unwrap();
        assert!(empty.items.is_empty() && !empty.more);
        let folder = root.join("audio-copies");
        std::fs::create_dir(&folder).unwrap();
        for number in 0..26 {
            let run_id = format!("{number:032x}");
            crate::create_json(
                &folder.join(format!("{run_id}.json")),
                &AudioCopy {
                    run_id,
                    meeting_id: "b".repeat(32),
                    title: "Synthetic retained copy".into(),
                    codec: meeting_core::audio_codec::Codec::Flac,
                    created_at: "2026-10-29T00:00:00Z".into(),
                    sections: 1,
                },
            )
            .unwrap();
            if number >= 23 {
                let page = audio_copy_page(&root, 0).unwrap();
                assert_eq!(page.items.len(), (number + 1).min(25));
                assert_eq!(page.more, number >= 25);
                let json = serde_json::to_value(&page).unwrap();
                assert_eq!(json["items"].as_array().unwrap().len(), page.items.len());
                assert_eq!(json["more"], page.more);
            }
        }
        let last = audio_copy_page(&root, 25).unwrap();
        assert_eq!(last.items.len(), 1);
        assert!(!last.more);
        let past = audio_copy_page(&root, 26).unwrap();
        assert!(past.items.is_empty() && !past.more);
        assert!(audio_copy_page(&root, 1_000_001).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Conversion {
    meeting_id: String,
    job_id: String,
    originals_retained: bool,
    codec: meeting_core::audio_codec::Codec,
    parts: Vec<meeting_core::audio_codec::ConversionProof>,
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
/// Require confirmation and a current FFmpeg grant, decode verified copies into a separate
/// session with provenance, then resume recognition; retain originals and partial work.
#[tauri::command(async)]
pub(crate) fn transcribe_audio_copy(
    service: State<'_, Arc<Service>>,
    run_id: String,
    model_id: String,
    confirmed: bool,
) -> AppResult<String> {
    if !confirmed {
        return Err("New transcription needs confirmation".into());
    }
    crate::validate_id(&run_id)?;
    let owned = Arc::clone(&service);
    let guard = reserve(&owned)?;
    service.maintenance_jobs.start_work("prepare-transcription",move|id,jobs,cancel|{
        let catalog=meeting_core::metadata::checked_directory(&owned.root.join("audio-copies"))?;
        let copy:AudioCopy=read_record(&catalog.join(format!("{run_id}.json")))?;
        if copy.run_id!=run_id{return Err("Audio-copy identity mismatch".into());}crate::validate_id(&copy.meeting_id)?;
        let policy=meeting_core::retention::read_policy(&owned.root)?;
        let selected=policy.ffmpeg_executable.as_ref().ok_or("Choose the separately installed FFmpeg tool first")?;
        owned.selections.verify(crate::path_selection::Purpose::Ffmpeg, selected)?;
        let tool=meeting_core::audio_codec::AudioTool::inspect(selected,cancel)?;
        let source=meeting_core::metadata::checked_directory(&owned.root.join("sessions").join(&copy.meeting_id)).map_err(|_|"Restore the archived whole meeting before using its compact audio copies")?;
        let data=match meeting_core::archive::read_archived_transcript(&source)?{Some(v)=>v,None=>meeting_core::inspect_manifest(&source)?};
        let copies=meeting_core::metadata::checked_directory(&source.join("converted-audio"))?;
        let folder=meeting_core::metadata::checked_directory(&copies.join(&run_id))?;
        let conversion:Conversion=read_record(&folder.join("conversion.json"))?;
        if conversion.meeting_id!=copy.meeting_id||conversion.job_id!=run_id||!conversion.originals_retained||conversion.codec!=copy.codec||conversion.parts.len()!=data.parts.len()||conversion.parts.len()!=copy.sections||data.parts.is_empty(){return Err("Converted audio inventory does not match the original session".into());}
        let (bridge,catalog,_)=owned.refresh(None)?;let model=catalog.model(&model_id)?;
        let resolved=bridge.resolve_streaming_model(model)?;
        let store=meeting_core::SessionStore::create(&owned.root.join("sessions"),&format!("{} · new transcription",copy.title),&model.id,&data.output_name,data.microphone_name.as_deref())?;
        let new_id=store.snapshot()?.id;
        handy_bridge::write_model_snapshot(store.folder(),model,&resolved,&serde_json::json!({"applicationVersion":crate::VERSION,"operation":"new transcription from verified compact audio","sourceMeetingId":copy.meeting_id,"copyRunId":run_id,"codec":copy.codec,"lossy":copy.codec==meeting_core::audio_codec::Codec::Opus,"nativeRuntime":handy_bridge::native::runtime_identity()}))?;
        crate::create_json(&store.folder().join("retranscription-source.json"),&serde_json::json!({"sourceMeetingId":copy.meeting_id,"copyRunId":run_id,"newMeetingId":new_id,"preparedBy":id,"originalStartedAt":data.started_at,"originalsRetained":true,"wordTimes":false,"parts":conversion.parts}))?;
        jobs.update_work(id,0,data.parts.len()as u64);let mut done=0;
        for track in ["system","microphone"]{
            let mut parts:Vec<_>=data.parts.iter().filter(|p|p.source==track).collect();parts.sort_by_key(|p|p.start_sample);
            if parts.is_empty(){continue;}
            let mut writer=meeting_core::PcmWriter::new(store.clone(),track)?;
            for part in parts{
                if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Preparation cancelled; originals and the partial new session are retained".into());}
                if writer.written_samples()>part.start_sample{return Err("Overlapping source sections; new copy retained for inspection".into());}
                let mut proofs=conversion.parts.iter().filter(|p|p.source_id==part.file);
                let proof=proofs.next().ok_or("Missing converted section")?;if proofs.next().is_some(){return Err("Duplicate converted section".into());}
                let pcm=tool.decode_copy(&folder,proof,part,cancel)?;writer.pad_to(part.start_sample)?;
                for packet in pcm.chunks(16000){if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Preparation cancelled; originals and partial new copy retained".into());}writer.append(packet)?;}
                done+=1;jobs.update_work(id,done,data.parts.len()as u64);
            }
            writer.finish()?;
        }
        store.set_status("recorded")?;crate::library_service::update(&owned.root,&store.snapshot()?)?;drop(store);
        drop(guard);
        owned.resume(new_id)?;
        Ok(())
    })
}
