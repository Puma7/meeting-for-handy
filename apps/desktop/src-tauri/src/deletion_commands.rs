//! Explicit per-meeting erasure. Previews are bounded in-memory metadata, never content copies.
use crate::{blocking, AppResult, Service};
use meeting_core::archive::{ErasurePreview, PendingErasure};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::State;

struct Cached {
    preview: ErasurePreview,
    at: Instant,
}
#[derive(Default)]
pub(crate) struct Previews(Mutex<HashMap<String, Cached>>);
impl Previews {
    /// Stores a bounded, short-lived preview capability containing no meeting text.
    fn put(&self, id: &str, preview: ErasurePreview) -> AppResult<()> {
        let mut rows = self.0.lock().map_err(|_| "Löschvorschau ist gesperrt.")?;
        rows.retain(|_, row| row.at.elapsed() < Duration::from_secs(900));
        if rows.len() >= 16 {
            rows.clear();
        }
        rows.insert(
            id.into(),
            Cached {
                preview,
                at: Instant::now(),
            },
        );
        Ok(())
    }
    /// Reads only the exact unexpired preview capability selected by its native job identity.
    fn get(&self, id: &str) -> AppResult<ErasurePreview> {
        crate::validate_id(id)?;
        let rows = self.0.lock().map_err(|_| "Löschvorschau ist gesperrt.")?;
        rows.get(id).filter(|row|row.at.elapsed()<Duration::from_secs(900)).map(|row|row.preview.clone()).ok_or_else(||"Löschvorschau ist abgelaufen. Bitte erneut prüfen. / Deletion preview expired; create another preview.".into())
    }
    /// Consumes one matching confirmed capability once; a different meeting or word leaves it intact.
    fn take(&self, id: &str, meeting: &str, confirmation: &str) -> AppResult<ErasurePreview> {
        let preview = self.get(id)?;
        if preview.meeting_id != meeting || !["LÖSCHEN", "DELETE"].contains(&confirmation) {
            return Err(
                "Bestätige genau das ausgewählte Meeting. / Confirm exactly the selected meeting."
                    .into(),
            );
        }
        self.0
            .lock()
            .map_err(|_| "Löschvorschau ist gesperrt.")?
            .remove(id)
            .ok_or("Löschvorschau wurde bereits verwendet.")?;
        Ok(preview)
    }
}

/// Starts a cancellable read-only inventory under exclusive operation admission.
#[tauri::command(async)]
pub(crate) fn preview_meeting_erasure(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<String> {
    crate::validate_id(&meeting_id)?;
    if crate::valid_legacy_id(&meeting_id) {
        return Err("Importierte Originale liegen außerhalb dieser App-Ablage. / Imported originals are outside app-owned storage.".into());
    }
    let owned = Arc::clone(&service);
    let guard = crate::retention_commands::reserve(&owned)?;
    service
        .maintenance_jobs
        .start_work("meeting-erasure-preview", move |job, jobs, cancel| {
            let _guard = guard;
            jobs.update_work(job, 0, 1);
            let preview = meeting_core::archive::preview_erasure(&owned.root, &meeting_id, cancel)?;
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Löschvorschau abgebrochen. / Deletion preview cancelled.".into());
            }
            owned.deletion_previews.put(job, preview)?;
            jobs.update_work(job, 1, 1);
            Ok(())
        })
}
/// Returns only the exact original preview's count/identity/hash, without local file paths.
#[tauri::command]
pub(crate) fn get_meeting_erasure_preview(
    service: State<'_, Arc<Service>>,
    job_id: String,
) -> AppResult<ErasurePreview> {
    service.deletion_previews.get(&job_id)
}

/// Consumes one confirmed preview; any changed content requires another explicit preview.
#[tauri::command(async)]
pub(crate) fn erase_meeting_now(
    service: State<'_, Arc<Service>>,
    preview_id: String,
    meeting_id: String,
    confirmation: String,
) -> AppResult<String> {
    let owned = Arc::clone(&service);
    // Validate confirmation before changing cached view or reserving destructive work.
    let preview = service.deletion_previews.get(&preview_id)?;
    if preview.meeting_id != meeting_id || !["LÖSCHEN", "DELETE"].contains(&confirmation.as_str())
    {
        return Err("Bestätigung passt nicht zum ausgewählten Meeting. / Confirmation does not match the selected meeting.".into());
    }
    let guard = crate::retention_commands::reserve(&owned)?;
    let preview = service
        .deletion_previews
        .take(&preview_id, &meeting_id, &confirmation)?;
    start_confirmed_erasure(owned, preview, guard)
}

/// The worker owns maintenance admission only through the destructive work; its
/// guard leaves scope before Jobs publishes terminal status and the UI rebuilds search.
fn start_confirmed_erasure(
    owned: Arc<Service>,
    preview: ErasurePreview,
    guard: crate::retention_commands::MaintenanceGuard,
) -> AppResult<String> {
    let meeting_id = preview.meeting_id.clone();
    owned
        .maintenance_jobs
        .clone()
        .start_work("meeting-erasure", move |job, jobs, cancel| {
            let _guard = guard;
            // All writers have ended before maintenance admission. Forget their finished
            // in-memory copies after explicit confirmation and before any file removal.
            owned.lock()?.error = None;
            owned.live_notes.forget(&meeting_id)?;
            owned.notes_jobs.forget(&meeting_id)?;
            owned.speaker_jobs.forget(&meeting_id)?;
            owned.library_jobs.invalidate_finished();
            // Admission has excluded cooperating writers. Detach only this meeting's
            // anchors; drop them after all app/map locks end. Other owners and any
            // in-flight clients keep their leases and delay provider cleanup normally.
            let release_live = owned
                .model_sessions
                .detach_owner(&crate::model_sessions::Owner::Live(meeting_id.clone()));
            let release_analysis = owned
                .model_sessions
                .detach_owner(&crate::model_sessions::Owner::Analysis(meeting_id.clone()));
            release_live();
            release_analysis();
            meeting_core::archive::erase_meeting(
                &owned.root,
                &meeting_id,
                &preview.fingerprint,
                &format!("DELETE {meeting_id}"),
                cancel,
                |n, total| jobs.update_work(job, n, total),
            )?;
            Ok(())
        })
}
/// Minimal interruption markers allow a new confirmation after restarting the application.
#[tauri::command]
pub(crate) async fn list_pending_erasures(
    service: State<'_, Arc<Service>>,
) -> AppResult<Vec<PendingErasure>> {
    let owned = Arc::clone(&service);
    blocking(move||meeting_core::archive::pending_erasures(&owned.root).map_err(|_|"Unterbrochene Löschungen konnten nicht geprüft werden. / Interrupted deletions could not be checked.".into())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise retained coordinator anchors through the real confirmed worker.
    /// The route is non-listening: no model is loaded and no native unload is claimed.
    fn assert_erasure_releases_model_owners(change_after_preview: bool) {
        use crate::model_sessions::Owner;
        use meeting_notes::{residency::LocalProvider, Connection};
        let root = std::env::temp_dir().join(format!(
            "handymeeting-erasure-model-anchors-{}",
            meeting_notes::new_job_id()
        ));
        let store = meeting_core::SessionStore::create(
            &root.join("sessions"),
            "Synthetic retained model owner",
            "m",
            "s",
            None,
        )
        .unwrap();
        let meeting = store.snapshot().unwrap().id;
        store.set_status("complete").unwrap();
        drop(store);
        let other = meeting_core::SessionStore::create(
            &root.join("sessions"),
            "Synthetic other meeting",
            "m",
            "s",
            None,
        )
        .unwrap();
        let other_id = other.snapshot().unwrap().id;
        other.set_status("complete").unwrap();
        drop(other);
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let connection = Connection {
            endpoint: "http://127.0.0.1:9/v1/".into(),
            model: "synthetic-shared-model".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let reserve = |owner| {
            service
                .model_sessions
                .reserve(owner, &connection, Some(LocalProvider::Ollama))
                .unwrap()
                .unwrap()
        };
        let live = reserve(Owner::Live(meeting.clone()));
        let analysis = reserve(Owner::Analysis(meeting.clone()));
        let other_live = reserve(Owner::Live(other_id.clone()));
        let other_analysis = reserve(Owner::Analysis(other_id.clone()));
        let in_flight = analysis.lease().unwrap();
        assert!(in_flight.same_model(&other_live.lease().unwrap()));
        // Refused admission is not a confirmed destructive request and releases nothing.
        service.lock().unwrap().transitioning = true;
        assert!(crate::retention_commands::reserve(&service).is_err());
        assert!(live.lease().is_ok() && analysis.lease().is_ok());
        service.lock().unwrap().transitioning = false;
        let preview = meeting_core::archive::preview_erasure(
            &root,
            &meeting,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        if change_after_preview {
            let reopened =
                meeting_core::SessionStore::open(&root.join("sessions").join(&meeting)).unwrap();
            reopened
                .warn("Synthetic changed content must require a fresh preview")
                .unwrap();
        }
        let guard = crate::retention_commands::reserve(&service).unwrap();
        let job = start_confirmed_erasure(Arc::clone(&service), preview, guard).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = service.maintenance_jobs.snapshot().unwrap();
            assert_eq!(status.job_id, job);
            if status.status != "running" {
                assert_eq!(
                    status.status,
                    if change_after_preview {
                        "failed"
                    } else {
                        "complete"
                    },
                    "{:?}",
                    status.error
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            root.join("sessions").join(&meeting).exists(),
            change_after_preview
        );
        assert!(root.join("sessions").join(&other_id).exists());
        assert!(other_live.lease().is_ok() && other_analysis.lease().is_ok());
        assert!(in_flight
            .matches_connection(&connection, LocalProvider::Ollama)
            .unwrap());
        assert!(
            live.lease().is_err(),
            "Deleted meeting retained its live model anchor"
        );
        assert!(
            analysis.lease().is_err(),
            "Deleted meeting retained its analysis model anchor"
        );
        assert_eq!(
            Arc::strong_count(&in_flight),
            1,
            "An in-flight client survives without a retained meeting anchor"
        );
    }

    /// Completed erasure releases only the two selected meeting anchors.
    #[test]
    fn completed_erasure_releases_only_its_retained_models() {
        assert_erasure_releases_model_owners(false);
    }

    /// A confirmed worker failure releases anchors while preserving changed source bytes.
    #[test]
    fn failed_erasure_releases_only_its_retained_models() {
        assert_erasure_releases_model_owners(true);
    }

    /// Exercise the real deletion worker and then the actual index-start admission.
    /// This guards the ordering of guard destruction and terminal status publication.
    #[test]
    fn completed_erasure_releases_maintenance_before_search_rebuild() {
        let root = std::env::temp_dir().join(format!(
            "handymeeting-erasure-complete-{}",
            meeting_notes::new_job_id()
        ));
        let store = meeting_core::SessionStore::create(
            &root.join("sessions"),
            "Synthetic completion scope",
            "m",
            "s",
            None,
        )
        .unwrap();
        let meeting = store.snapshot().unwrap().id;
        store.set_status("complete").unwrap();
        drop(store);
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let preview = meeting_core::archive::preview_erasure(
            &root,
            &meeting,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let guard = crate::retention_commands::reserve(&service).unwrap();
        let job = start_confirmed_erasure(Arc::clone(&service), preview, guard).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = service.maintenance_jobs.snapshot().unwrap();
            assert_eq!(status.job_id, job);
            if status.status != "running" {
                assert_eq!(status.status, "complete", "{:?}", status.error);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!service.lock().unwrap().maintenance_reserved);
        assert!(!service.lock().unwrap().transitioning);
        assert!(!root.join("sessions").join(meeting).exists());
        let lease = service
            .reserve_operation(crate::operations::Kind::Index)
            .expect("Completed deletion retained maintenance admission");
        service
            .library_jobs
            .start_scan(root, true, Some(lease))
            .unwrap();
        loop {
            let status = service.library_jobs.snapshot().unwrap();
            if status.status != "running" {
                assert_eq!(status.status, "complete", "{:?}", status.error);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    /// Both Services completed a real scan before erasure. Recreating only one
    /// retained row must not hide the other original when the UI rebuild is absent.
    fn erasure_never_reuses_a_completed_scan_for_a_recreated_partial_index() {
        let root = std::env::temp_dir().join(format!(
            "handymeeting-erasure-history-{}",
            meeting_notes::new_job_id()
        ));
        let mut snapshots = vec![];
        for title in [
            "Synthetic erased",
            "Synthetic retained A",
            "Synthetic retained B",
        ] {
            let store =
                meeting_core::SessionStore::create(&root.join("sessions"), title, "m", "s", None)
                    .unwrap();
            store.set_status("complete").unwrap();
            snapshots.push(store.snapshot().unwrap());
        }
        let first = Arc::new(Service::new(root.clone()).unwrap());
        let second = Arc::new(Service::new(root.clone()).unwrap());
        let wait = |jobs: &crate::library_service::Jobs, id: &str| {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let state = jobs.snapshot().unwrap();
                assert_eq!(state.job_id, id);
                if state.status != "running" {
                    assert_eq!(state.status, "complete", "{:?}", state.error);
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        for service in [&first, &second] {
            let job = service
                .library_jobs
                .start_scan(root.clone(), true, None)
                .unwrap();
            wait(&service.library_jobs, &job);
            assert_eq!(service.history().unwrap().len(), 3);
            assert!(
                service
                    .library_jobs
                    .completed_history_index(&root)
                    .is_some(),
                "An unchanged fully scanned cache must still serve indexed history"
            );
        }
        let retained_bytes: Vec<_> = snapshots[1..]
            .iter()
            .map(|data| {
                let path = root.join("sessions").join(&data.id).join("session.json");
                (path.clone(), std::fs::read(path).unwrap())
            })
            .collect();
        let preview = meeting_core::archive::preview_erasure(
            &root,
            &snapshots[0].id,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let guard = crate::retention_commands::reserve(&first).unwrap();
        let job = start_confirmed_erasure(first.clone(), preview, guard).unwrap();
        wait(&first.maintenance_jobs, &job);
        assert!(!root.join("library-index/library.sqlite3").exists());
        // A real metadata/update path can recreate one row before any full rebuild.
        crate::library_service::update(&root, &snapshots[1]).unwrap();
        let mut expected = snapshots[1..]
            .iter()
            .map(|data| data.id.clone())
            .collect::<Vec<_>>();
        expected.sort();
        let actual: Vec<_> = [&first, &second]
            .iter()
            .map(|service| {
                let mut ids = service
                    .history()
                    .unwrap()
                    .into_iter()
                    .map(|meeting| meeting.data.id)
                    .collect::<Vec<_>>();
                ids.sort();
                ids
            })
            .collect();
        assert_eq!(
            actual,
            vec![expected.clone(), expected],
            "A recreated partial cache must not become a complete history in either Service"
        );
        assert!(first.library_jobs.snapshot().is_none());
        assert!(second.library_jobs.snapshot().is_none());
        for (path, bytes) in retained_bytes {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn previews_bind_confirmation_to_one_meeting_and_are_single_use() {
        let rows = Previews::default();
        let job = "a".repeat(32);
        let meeting = "b".repeat(32);
        rows.put(
            &job,
            ErasurePreview {
                meeting_id: meeting.clone(),
                fingerprint: "synthetic".into(),
                files: 1,
                bytes: 2,
                rebuild_search: true,
                preserved_atomic_remnants: 0,
            },
        )
        .unwrap();
        assert!(rows.take(&job, &"c".repeat(32), "LÖSCHEN").is_err());
        assert!(rows.take(&job, &meeting, "yes").is_err());
        assert!(rows.get(&job).is_ok());
        assert!(rows.take(&job, &meeting, "LÖSCHEN").is_ok());
        assert!(rows.take(&job, &meeting, "LÖSCHEN").is_err());
    }
}
