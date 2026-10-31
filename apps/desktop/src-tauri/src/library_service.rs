//! Derived search jobs do not own capture, ASR workers or original transcript writers.
use meeting_core::{
    library::{LibraryIndex, SearchPage},
    metadata, SessionData,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Progress {
    pub job_id: String,
    pub kind: String,
    pub status: String,
    pub confirmed_tasks: u64,
    pub total_tasks: u64,
    pub elapsed_ms: u64,
    pub skipped: u64,
    pub error: Option<String>,
    pub cancel_requested: bool,
}
#[derive(Default)]
pub(super) struct Jobs {
    pub state: Mutex<Option<Progress>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    started: Mutex<Option<(String, Instant)>>,
    indexed_cache: Mutex<Option<String>>,
}
#[derive(Clone, Copy)]
enum JobPolicy {
    Maintenance,
    Search,
}
impl JobPolicy {
    fn message(self, maintenance: &'static str, search: &'static str) -> &'static str {
        match self {
            Self::Maintenance => maintenance,
            Self::Search => search,
        }
    }
}
impl Jobs {
    /// Forget only terminal search progress. Callers holding maintenance admission
    /// invalidate it before destructive cache removal; active jobs retain control.
    pub fn invalidate_finished(&self) {
        if let Ok(mut state) = self.state.lock() {
            if state
                .as_ref()
                .is_some_and(|progress| progress.status == "running")
            {
                return;
            }
            *state = None;
        }
        if let Ok(mut identity) = self.indexed_cache.lock() {
            *identity = None;
        }
    }
    /// A remembered completion is usable only for the same actual SQLite cache.
    /// Return its still-leased handle through all history pages. Missing, invalid
    /// or recreated caches fall back to original session folders in every Service.
    pub fn completed_history_index(&self, root: &Path) -> Option<LibraryIndex> {
        if self
            .snapshot()
            .is_none_or(|progress| progress.status != "complete")
        {
            return None;
        }
        let expected = self
            .indexed_cache
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let index = expected.and_then(|expected| {
            if !root.join("library-index/library.sqlite3").exists() {
                return None;
            }
            LibraryIndex::open(root).ok().filter(|index| {
                index
                    .cache_identity()
                    .is_ok_and(|actual| actual == expected)
            })
        });
        if index.is_none() {
            self.invalidate_finished();
        }
        index
    }
    pub fn update_work(&self, id: &str, done: u64, total: u64) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(p) = state
                .as_mut()
                .filter(|p| p.job_id == id && p.status == "running")
            {
                p.confirmed_tasks = done;
                p.total_tasks = total;
            }
        }
    }
    pub fn start_work<F>(self: &Arc<Self>, kind: &str, work: F) -> Result<String, String>
    where
        F: FnOnce(&str, &Arc<Self>, &Arc<AtomicBool>) -> meeting_core::Result<()> + Send + 'static,
    {
        self.start_job(kind, JobPolicy::Maintenance, None, work)
    }
    /// Keep admission, cancellation, clock and terminal progress identical for derived jobs.
    /// Search errors remain redacted; a captured operation lease spans terminal publication.
    fn start_job<F>(
        self: &Arc<Self>,
        kind: &str,
        policy: JobPolicy,
        lease: Option<crate::operations::Lease>,
        work: F,
    ) -> Result<String, String>
    where
        F: FnOnce(&str, &Arc<Self>, &Arc<AtomicBool>) -> meeting_core::Result<()> + Send + 'static,
    {
        let id = meeting_notes::new_job_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        {
            let mut state = self.state.lock().map_err(|_| {
                policy.message(
                    "Maintenance state unavailable",
                    "Search job state is unavailable",
                )
            })?;
            if state.as_ref().is_some_and(|p| p.status == "running") {
                return Err(policy
                    .message(
                        "Another maintenance job is already running",
                        "The search index is already being updated",
                    )
                    .into());
            }
            let mut cancellation = self.cancel.lock().map_err(|_| {
                policy.message(
                    "Maintenance cancellation unavailable",
                    "Search cancellation unavailable",
                )
            })?;
            let mut clock = self.started.lock().map_err(|_| {
                policy.message("Maintenance clock unavailable", "Search clock unavailable")
            })?;
            *state = Some(Progress {
                job_id: id.clone(),
                kind: kind.into(),
                status: "running".into(),
                confirmed_tasks: 0,
                total_tasks: 0,
                elapsed_ms: 0,
                skipped: 0,
                error: None,
                cancel_requested: false,
            });
            *cancellation = Some(cancel.clone());
            *clock = Some((id.clone(), started));
        }
        let jobs = self.clone();
        let owned = id.clone();
        let spawn=std::thread::Builder::new().name(policy.message("meeting-maintenance", "meeting-search-index").into()).spawn(move||{
            let _lease = lease;
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||work(&owned,&jobs,&cancel))).unwrap_or_else(|_|Err(policy.message("Maintenance stopped unexpectedly. Review the retained operation log before continuing.", "Search worker stopped unexpectedly; original sessions are preserved").into()));
            if let Ok(mut state)=jobs.state.lock(){if let Some(p)=state.as_mut().filter(|p|p.job_id==owned){p.elapsed_ms=started.elapsed().as_millis()as u64;p.status=if result.is_ok(){"complete"}else if cancel.load(Ordering::Acquire){"cancelled"}else{"failed"}.into();p.error=result.err().map(|e|match policy{JobPolicy::Maintenance=>e.to_string(),JobPolicy::Search=>"Search index could not be fully updated. Original sessions are unchanged. / Der Suchindex konnte nicht vollständig aktualisiert werden. Originale bleiben erhalten.".into()});}}
        });
        if spawn.is_err() {
            if let Ok(mut state) = self.state.lock() {
                if let Some(p) = state.as_mut().filter(|p| p.job_id == id) {
                    p.status = "failed".into();
                    p.elapsed_ms = started.elapsed().as_millis() as u64;
                    p.error = Some(policy.message("Maintenance could not start", "Search indexing could not start. Original sessions are preserved. / Indexierung konnte nicht gestartet werden; Originale bleiben erhalten.").into());
                }
            }
            return Err(policy
                .message(
                    "Maintenance worker could not start",
                    "The local search worker could not start",
                )
                .into());
        }
        Ok(id)
    }
    pub fn snapshot(&self) -> Option<Progress> {
        let mut view = self.state.lock().ok()?.clone()?;
        if view.status == "running" {
            if let Some((id, start)) = self
                .started
                .lock()
                .ok()?
                .as_ref()
                .filter(|(id, _)| id == &view.job_id)
            {
                let _ = id;
                view.elapsed_ms = start.elapsed().as_millis() as u64;
            }
        }
        Some(view)
    }
    pub fn cancel(&self, id: &str) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if let Some(p) = state
            .as_mut()
            .filter(|p| p.job_id == id && p.status == "running")
        {
            if let Ok(flag) = self.cancel.lock() {
                if let Some(flag) = flag.as_ref() {
                    flag.store(true, Ordering::SeqCst);
                    p.cancel_requested = true;
                    return true;
                }
            }
        }
        false
    }
    /// `rebuild` is a user request: an index SQLite cannot read is moved aside and recreated.
    pub fn start_scan(
        self: &Arc<Self>,
        root: PathBuf,
        rebuild: bool,
        lease: Option<crate::operations::Lease>,
    ) -> Result<String, String> {
        self.invalidate_finished();
        self.start_job("search-index", JobPolicy::Search, lease, move |id, jobs, cancel| {
                let sessions = root.join("sessions");
                let sessions = metadata::checked_directory(&sessions)?;
                let total = fs::read_dir(&sessions)?.count() as u64;
                let mut index = if rebuild { LibraryIndex::open_for_rebuild(&root)?.0 } else { LibraryIndex::open(&root)? };
                let mut entries=fs::read_dir(&sessions)?;
                let mut number=0u64;
                loop {
                    let batch:Vec<_>=entries.by_ref().take(500).collect();
                    if batch.is_empty(){break;}
                    let count=batch.len() as u64;
                    let mut skipped=0;
                    index.transaction_batch(|index| {
                      for entry in batch {
                        if cancel.load(Ordering::SeqCst) {
                          return Err("Search indexing cancelled; existing recordings and the last indexed results are preserved".into());
                        }
                        let one = (|| -> meeting_core::Result<()> {
                        let entry = entry?;
                        let id = entry.file_name().to_string_lossy().into_owned();
                        if !metadata::valid_id(&id) {
                            return Ok(());
                        }
                        let path = entry.path();
                        if !path.is_dir() {
                            return Ok(());
                        }
                        let fingerprint = fingerprint(&path, &root, &id)?;
                        if index.current_fingerprint(&id)?.as_deref() == Some(fingerprint.as_str())
                        {
                            return Ok(());
                        }
                        // Confirmed text lives in the journals; the manifest alone can lag behind it.
                        let data = match meeting_core::archive::read_archived_transcript(&path)?{Some(data)=>data,None=>meeting_core::inspect_journals(&path)?};
                        let meta = metadata::read(&root, &id)?;
                        index.upsert(&data, &meta, &fingerprint)
                        })();
                        if one.is_err(){skipped+=1;}
                      }
                      Ok(())
                    })?;
                    number+=count;
                    if let Ok(mut state) = jobs.state.lock() {
                        if let Some(p) = state.as_mut().filter(|p| p.job_id == id) {
                            p.total_tasks = total.max(number);
                            p.confirmed_tasks = number;
                            p.skipped += skipped;
                        }
                    }
                }
                index.prune_missing_sessions(&sessions,cancel)?;
                *jobs.indexed_cache.lock().map_err(|_|"Search completeness state is unavailable")?=Some(index.cache_identity()?);
                if let Ok(mut state)=jobs.state.lock(){if let Some(p)=state.as_mut().filter(|p|p.job_id==id){p.total_tasks=p.confirmed_tasks;}}
                Ok(())
        })
    }
}
fn fingerprint(folder: &Path, root: &Path, id: &str) -> meeting_core::Result<String> {
    let mut result = String::new();
    for path in [
        folder.join("session.json"),
        folder.join("session.json.bak"),
        folder.join("audio-journal.jsonl"),
        folder.join("transcript-journal.jsonl"),
        folder.join("audio-archive.json"),
        root.join("user-metadata").join(format!("{id}.json")),
    ] {
        match fs::metadata(&path) {
            Ok(meta) => result.push_str(&format!("{}:{:?};", meta.len(), meta.modified()?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => result.push_str("missing;"),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(result)
}
pub(super) fn search(
    root: &Path,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchPage, String> {
    LibraryIndex::open(root).and_then(|index|index.search(query,offset,limit)).map_err(|_|"Search is unavailable. Update the local index; original sessions are preserved. / Suche nicht verfügbar. Bitte den lokalen Suchindex aktualisieren.".into())
}
/// Retain index admission from source revalidation through upsert, so a maintenance
/// snapshot delayed past erasure cannot restore content to a newly created cache.
pub(super) fn update(root: &Path, data: &SessionData) -> Result<(), String> {
    let mut index = LibraryIndex::open(root).map_err(|_| "The search index is unavailable")?;
    if !crate::valid_legacy_id(&data.id) {
        crate::validate_id(&data.id)?;
        let current = meeting_core::inspect_manifest(&root.join("sessions").join(&data.id))
            .map_err(|_| "Meeting source is no longer available; index update refused")?;
        if current.id != data.id {
            return Err("Meeting source identity changed; index update refused".into());
        }
    }
    let meta = metadata::read(root, &data.id)
        .map_err(|_| "Saved meeting labels could not be read; originals are preserved")?;
    index
        .upsert(data, &meta, "explicit-current-session")
        .map_err(|_| "The search index could not be updated".into())
}
#[cfg(test)]
mod clock_tests {
    use super::*;
    #[test]
    fn both_job_policies_keep_admission_until_cancelled_work_finishes_and_search_errors_are_redacted(
    ) {
        for policy in [JobPolicy::Maintenance, JobPolicy::Search] {
            let gate = crate::operations::Gate::default();
            let lease = gate.try_acquire(crate::operations::Kind::Index).unwrap();
            let jobs = Arc::new(Jobs::default());
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (finish_tx, finish_rx) = std::sync::mpsc::channel();
            let id = jobs
                .start_job("fixture", policy, Some(lease), move |id, jobs, cancel| {
                    jobs.update_work(id, 1, 3);
                    ready_tx.send(()).unwrap();
                    finish_rx.recv().unwrap();
                    assert!(cancel.load(Ordering::Acquire));
                    Err("private synthetic transcript /outside/meeting.json".into())
                })
                .unwrap();
            ready_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            assert!(jobs.start_work("second", |_, _, _| Ok(())).is_err());
            assert!(!jobs.cancel("a different job"));
            assert!(jobs.cancel(&id));
            assert_eq!(jobs.snapshot().unwrap().status, "running");
            assert!(jobs.snapshot().unwrap().cancel_requested);
            assert!(gate
                .try_acquire(crate::operations::Kind::Maintenance)
                .is_err());
            finish_tx.send(()).unwrap();
            let done = finished(&jobs);
            assert_eq!(done.status, "cancelled");
            assert_eq!((done.confirmed_tasks, done.total_tasks), (1, 3));
            assert!(!jobs.cancel(&id));
            let error = done.error.unwrap();
            match policy {
                JobPolicy::Maintenance => assert!(error.contains("private synthetic transcript")),
                JobPolicy::Search => {
                    assert!(!error.contains("private synthetic transcript"));
                    assert!(!error.contains("/outside/meeting.json"));
                    assert!(error.contains("Original sessions are unchanged"));
                }
            }
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if let Ok(released) = gate.try_acquire(crate::operations::Kind::Maintenance) {
                    drop(released);
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            jobs.start_work("next", |_, _, _| Ok(())).unwrap();
            assert_eq!(finished(&jobs).status, "complete");
        }
    }
    #[test]
    fn scan_counts_unreadable_sessions_without_changing_or_publishing_their_contents() {
        let root =
            std::env::temp_dir().join(format!("handymeeting-skip-{}", meeting_notes::new_job_id()));
        let session = root.join("sessions").join("a".repeat(32));
        fs::create_dir_all(&session).unwrap();
        let original = b"private synthetic incomplete manifest, never a public error";
        fs::write(session.join("session.json"), original).unwrap();
        let jobs = Arc::new(Jobs::default());
        jobs.start_scan(root.clone(), false, None).unwrap();
        let done = finished(&jobs);
        assert_eq!(done.status, "complete");
        assert_eq!(
            (done.confirmed_tasks, done.total_tasks, done.skipped),
            (1, 1, 1)
        );
        assert!(done.error.is_none());
        assert_eq!(fs::read(session.join("session.json")).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn maintenance_clock_advances_then_freezes_and_terminal_failure_keeps_confirmed_work() {
        let jobs = Arc::new(Jobs::default());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        jobs.start_work("fixture", move |id, jobs, _| {
            jobs.update_work(id, 1, 3);
            ready_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Err("Synthetic failure after one confirmed step".into())
        })
        .unwrap();
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let before = jobs.snapshot().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let running = jobs.snapshot().unwrap();
        assert!(running.elapsed_ms > before.elapsed_ms);
        finish_tx.send(()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while jobs.snapshot().unwrap().status == "running" {
            assert!(Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let done = jobs.snapshot().unwrap();
        assert_eq!(done.status, "failed");
        assert_eq!(done.confirmed_tasks, 1);
        assert_eq!(done.total_tasks, 3);
        assert!(done.elapsed_ms >= running.elapsed_ms);
        assert!(done.error.unwrap().contains("Synthetic failure"));
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(jobs.snapshot().unwrap().elapsed_ms, done.elapsed_ms);
    }
    fn finished(jobs: &Arc<Jobs>) -> Progress {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let progress = jobs.snapshot().unwrap();
            if progress.status != "running" {
                return progress;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    /// Verify an unreadable synthetic index is preserved until an explicit rebuild moves it
    /// aside.
    #[test]
    fn only_a_user_rebuild_replaces_an_unreadable_index_and_keeps_its_bytes() {
        // Synthetic damage to the derived index; no session folder is touched.
        let root = std::env::temp_dir().join(format!(
            "handymeeting-index-{}",
            meeting_notes::new_job_id()
        ));
        fs::create_dir_all(root.join("sessions")).unwrap();
        let folder = root.join("library-index");
        fs::create_dir(&folder).unwrap();
        let damaged = b"synthetic unreadable index ".repeat(300);
        fs::write(folder.join("library.sqlite3"), &damaged).unwrap();
        let jobs = Arc::new(Jobs::default());
        jobs.start_scan(root.clone(), false, None).unwrap();
        assert_eq!(finished(&jobs).status, "failed");
        assert_eq!(fs::read(folder.join("library.sqlite3")).unwrap(), damaged);
        jobs.start_scan(root.clone(), true, None).unwrap();
        assert_eq!(finished(&jobs).status, "complete");
        let aside: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(fs::read(aside[0].join("library.sqlite3")).unwrap(), damaged);
        assert!(LibraryIndex::open(&root).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }
}
