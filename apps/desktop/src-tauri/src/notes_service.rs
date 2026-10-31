//! Application jobs own cancellation and progress; transport receives text only.
use crate::{note_sources, valid_legacy_id, AppResult, NotesRequest, Service};
use meeting_notes::{Analysis, AnalysisControl, AnalysisProgress};
use serde::Serialize;
use std::{collections::HashSet, sync::Mutex, time::Instant};
use zeroize::Zeroizing;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotesProgress {
    job_id: String,
    run_id: Option<String>,
    meeting_id: String,
    status: String,
    stage: String,
    step_name: Option<String>,
    step_index: usize,
    total_steps: usize,
    batch_index: usize,
    total_batches: usize,
    confirmed_tasks: usize,
    total_tasks: usize,
    elapsed_ms: u64,
    error: Option<String>,
    cancel_requested: bool,
}
struct Job {
    progress: NotesProgress,
    started: Instant,
    control: AnalysisControl,
}
#[derive(Default)]
struct JobsState {
    last: Option<Job>,
    used: HashSet<String>,
}
#[derive(Default)]
pub(crate) struct NotesJobs(Mutex<JobsState>);
impl NotesJobs {
    fn begin(&self, request: &NotesRequest) -> AppResult<(String, AnalysisControl)> {
        let id = request
            .job_id
            .clone()
            .unwrap_or_else(meeting_notes::new_job_id);
        let canonical = canonical_job_id(&id)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Auswertungssteuerung ist gesperrt.")?;
        if state
            .last
            .as_ref()
            .is_some_and(|job| job.progress.status == "running")
        {
            return Err(
                "Eine Auswertung läuft bereits. Bitte deren Abschluss abwarten oder sie abbrechen."
                    .into(),
            );
        }
        if state.used.len() >= 10_000 {
            return Err("Bitte die Anwendung vor weiteren Auswertungen neu starten. Gespeicherte Arbeitsstände bleiben erhalten.".into());
        }
        if !state.used.insert(canonical) {
            return Err(
                "Diese Auftragskennung wurde bereits verwendet. Bitte einen neuen Auftrag starten."
                    .into(),
            );
        }
        let control = AnalysisControl::default();
        state.last = Some(Job {
            progress: NotesProgress {
                job_id: id.clone(),
                run_id: request.resume_run_id.clone(),
                meeting_id: request.meeting_id.clone(),
                status: "running".into(),
                stage: "preparing".into(),
                step_name: None,
                step_index: 0,
                total_steps: request.template.steps.len(),
                batch_index: 0,
                total_batches: 0,
                confirmed_tasks: 0,
                total_tasks: 0,
                elapsed_ms: 0,
                error: None,
                cancel_requested: false,
            },
            started: Instant::now(),
            control: control.clone(),
        });
        Ok((id, control))
    }
    pub(crate) fn snapshot(&self) -> AppResult<Option<NotesProgress>> {
        let state = self
            .0
            .lock()
            .map_err(|_| "Auswertungssteuerung ist gesperrt.")?;
        Ok(state.last.as_ref().map(|job| {
            let mut progress = job.progress.clone();
            if progress.status == "running" {
                progress.elapsed_ms = elapsed(job.started);
            }
            progress
        }))
    }
    pub(crate) fn running(&self) -> bool {
        self.0
            .lock()
            .map(|s| {
                s.last
                    .as_ref()
                    .is_some_and(|j| j.progress.status == "running")
            })
            .unwrap_or(false)
    }
    pub(crate) fn cancel(&self, id: &str) -> AppResult<bool> {
        canonical_job_id(id)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Auswertungssteuerung ist gesperrt.")?;
        if let Some(job) = &mut state.last {
            if job.progress.job_id == id && job.progress.status == "running" {
                job.progress.cancel_requested = true;
                job.control.cancel();
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn update(&self, id: &str, update: AnalysisProgress) {
        if let Ok(mut state) = self.0.lock() {
            if let Some(job) = state
                .last
                .as_mut()
                .filter(|j| j.progress.job_id == id && j.progress.status == "running")
            {
                job.progress.run_id = Some(update.run_id);
                job.progress.stage = update.stage;
                job.progress.step_name = update.step_name;
                job.progress.step_index = update.step_index;
                job.progress.total_steps = update.total_steps;
                job.progress.batch_index = update.batch_index;
                job.progress.total_batches = update.total_batches;
                job.progress.confirmed_tasks = update.confirmed_tasks;
                job.progress.total_tasks = update.total_tasks;
            }
        }
    }
    fn finish(&self, id: &str, result: &anyhow::Result<Analysis>) {
        if let Ok(mut state) = self.0.lock() {
            if let Some(job) = state
                .last
                .as_mut()
                .filter(|j| j.progress.job_id == id && j.progress.status == "running")
            {
                job.progress.elapsed_ms = elapsed(job.started);
                match result {
                    Ok(analysis) => {
                        job.progress.run_id = Some(analysis.id.clone());
                        job.progress.status = "complete".into();
                        job.progress.stage = "finished".into();
                        job.progress.confirmed_tasks = job.progress.total_tasks;
                        job.progress.cancel_requested = false;
                    }
                    Err(error) => {
                        job.progress.status = if meeting_notes::is_cancelled(error) {
                            "cancelled"
                        } else {
                            "failed"
                        }
                        .into();
                        job.progress.error = Some(error.to_string());
                    }
                }
            }
        }
    }
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}
fn canonical_job_id(id: &str) -> AppResult<String> {
    let valid = match id.len() {
        32 => id.bytes().all(|c| c.is_ascii_hexdigit()),
        36 => id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        }),
        _ => false,
    };
    if !valid {
        return Err("Ungültige Auswertungs-Auftragskennung.".into());
    }
    Ok(id.replace('-', "").to_ascii_lowercase())
}

/// Hold analysis admission while selecting immutable source text for one controlled run. Saved
/// keys still require per-run cloud consent; progress retains confirmed work.
pub(crate) fn analyze(service: &Service, mut request: NotesRequest) -> AppResult<Analysis> {
    let mut key = Zeroizing::new(std::mem::take(&mut request.api_key));
    let _operation = service.reserve_operation(crate::operations::Kind::Analysis)?;
    let (id, control) = service.notes_jobs.begin(&request)?;
    struct Guard<'a> {
        jobs: &'a NotesJobs,
        id: &'a str,
        finished: bool,
    }
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            if !self.finished {
                self.jobs.finish(self.id, &Err(anyhow::anyhow!("Der Arbeitsschritt wurde unterbrochen. Bestätigte Auswertungsstände bleiben erhalten.")));
            }
        }
    }
    let mut guard = Guard {
        jobs: &service.notes_jobs,
        id: &id,
        finished: false,
    };
    let mut model_work = None;
    let mut model_lease = None;
    let result = (|| {
        control.check()?;
        if valid_legacy_id(&request.meeting_id) {
            anyhow::bail!("Diese Beta verändert Originalimporte nicht. Ihre Auswertung bleibt in der bisherigen Version verfügbar.");
        }
        let sources = if let Some(run_id) = &request.resume_run_id {
            let folder = analysis_folder(service, &request.meeting_id, run_id)
                .map_err(anyhow::Error::msg)?;
            let snapshot = meeting_notes::read_snapshot(&folder.join("snapshot.json"))?;
            anyhow::ensure!(
                snapshot.id == *run_id && snapshot.meeting_id == request.meeting_id,
                "Der Arbeitsstand gehört nicht zur ausgewählten Sitzung."
            );
            // Continue the original immutable transcript, never a newer ASR revision.
            snapshot.sources
        } else {
            let data = service
                .data(&request.meeting_id)
                .map_err(anyhow::Error::msg)?;
            anyhow::ensure!(data.status=="complete"&&data.parts.iter().all(|p|p.status=="complete"),"Bitte zuerst alle offenen Transkriptabschnitte nachholen. Ausgewertet wird nur ein abgeschlossener Originalstand.");
            note_sources(&data).map_err(anyhow::Error::msg)?
        };
        if request.use_saved_key {
            anyhow::ensure!(key.is_empty(),"Bitte entweder den gespeicherten Schlüssel oder einen Schlüssel nur für diese Sitzung wählen.");
            key = crate::credential_service::load_for_run(
                &request.endpoint,
                &request.mode,
                request.cloud_consent,
            )
            .map_err(anyhow::Error::msg)?;
        }
        let connection = meeting_notes::Connection {
            endpoint: request.endpoint,
            model: request.model,
            mode: request.mode,
            cloud_consent: request.cloud_consent,
            api_key: std::mem::take(&mut *key),
        };
        model_work = service
            .model_sessions
            .reserve(
                crate::model_sessions::Owner::Analysis(request.meeting_id.clone()),
                &connection,
                crate::local_models::provider_for_endpoint(&connection.endpoint),
            )
            .map_err(anyhow::Error::msg)?;
        model_lease = model_work
            .as_ref()
            .map(|work| work.lease())
            .transpose()
            .map_err(anyhow::Error::msg)?;
        meeting_notes::analyze_controlled_resident(
            &service
                .root
                .join("sessions")
                .join(&request.meeting_id)
                .join("analyses"),
            &request.meeting_id,
            sources,
            request.template,
            connection,
            request.resume_run_id.as_deref(),
            &control,
            model_lease.clone(),
            |p| service.notes_jobs.update(&id, p),
        )
    })();
    if result.is_ok() || result.as_ref().is_err_and(meeting_notes::is_cancelled) {
        service
            .model_sessions
            .finish_owner(&crate::model_sessions::Owner::Analysis(
                request.meeting_id.clone(),
            ));
    }
    if let Ok(analysis) = &result {
        // A resumed old transcript must not release a newer incomplete live finalization.
        if let Ok(data) = service.data(&request.meeting_id) {
            if data.status == "complete"
                && data.parts.iter().all(|part| part.status == "complete")
                && note_sources(&data)
                    .ok()
                    .and_then(|sources| meeting_notes::source_hash(&sources).ok())
                    .as_ref()
                    == Some(&analysis.source_hash)
            {
                service
                    .live_notes
                    .finish_inactive_model(&request.meeting_id, &service.model_sessions);
            }
        }
    }
    service.notes_jobs.finish(&id, &result);
    guard.finished = true;
    result.map_err(|error| error.to_string())
}

fn analysis_folder(
    service: &Service,
    meeting_id: &str,
    run_id: &str,
) -> AppResult<std::path::PathBuf> {
    crate::validate_id(run_id)?;
    let session = crate::meeting_files::session_folder(&service.root, meeting_id)?;
    let analyses = crate::meeting_files::directory(&session.join("analyses"))?;
    crate::meeting_files::directory(&analyses.join(run_id))
}

/// Each unfinished run is checked on its own: a damaged or foreign run is reported as skipped
/// (and left untouched) instead of hiding every other run. Runs of an earlier prompt version
/// are listed with `resumable: false`.
pub(crate) fn pending_analyses(
    service: &Service,
    meeting_id: &str,
) -> AppResult<meeting_notes::PendingListing> {
    let mut listing = meeting_notes::PendingListing::default();
    if valid_legacy_id(meeting_id) {
        return Ok(listing);
    }
    let session = crate::meeting_files::session_folder(&service.root, meeting_id)?;
    let root = session.join("analyses");
    if !root.exists() {
        return Ok(listing);
    }
    let root = crate::meeting_files::directory(&root)?;
    for entry in std::fs::read_dir(root)
        .map_err(|_| "Gespeicherte Arbeitsstände konnten nicht gelesen werden.")?
    {
        let entry =
            entry.map_err(|_| "Gespeicherte Arbeitsstände konnten nicht gelesen werden.")?;
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if crate::validate_id(&id).is_err() {
            continue;
        }
        let reason = match analysis_folder(service, meeting_id, &id)
            .map_err(anyhow::Error::msg)
            .and_then(|folder| meeting_notes::read_pending_analysis(&folder))
        {
            Ok(None) => continue,
            Ok(Some(item)) if item.meeting_id == meeting_id => {
                listing.items.push(item);
                continue;
            }
            Ok(Some(_)) => meeting_notes::SkipReason::OtherMeeting,
            Err(error) => meeting_notes::skip_reason(&error),
        };
        listing
            .skipped
            .push(meeting_notes::SkippedAnalysis { id, reason });
    }
    listing
        .items
        .sort_by(|a, b| b.created_at.cmp(&a.created_at));
    listing.skipped.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(listing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_snapshot_survives_service_restart_and_never_restores_cloud_consent() {
        let root = std::env::temp_dir().join(format!(
            "handymeeting-notes-recovery-{}",
            meeting_notes::new_job_id()
        ));
        let session = meeting_core::SessionStore::create(
            &root.join("sessions"),
            "Synthetic recovery",
            "synthetic",
            "system",
            None,
        )
        .unwrap();
        let meeting_id = session.snapshot().unwrap().id;
        let folder = session
            .folder()
            .join("analyses")
            .join("0123456789abcdef0123456789abcdef");
        drop(session);
        std::fs::create_dir_all(&folder).unwrap();
        let sources = vec![meeting_notes::Source {
            id: "S00001".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1000,
            text: "Original synthetic transcript.".into(),
        }];
        let template = meeting_notes::Template {
            id: "original".into(),
            name: "Original instructions".into(),
            steps: vec![
                meeting_notes::Step {
                    id: "summary".into(),
                    name: "Summary".into(),
                    kind: meeting_notes::Kind::Summary,
                    prompt: "Summarise original statements with evidence.".into(),
                },
                meeting_notes::Step {
                    id: "tasks".into(),
                    name: "Tasks".into(),
                    kind: meeting_notes::Kind::Tasks,
                    prompt: "Find explicitly assigned tasks with evidence.".into(),
                },
            ],
        };
        let snapshot = Analysis {
            id: "0123456789abcdef0123456789abcdef".into(),
            meeting_id: meeting_id.clone(),
            source_hash: meeting_notes::source_hash(&sources).unwrap(),
            template,
            sections: vec![],
            sources,
            created_at: "2026-10-26T08:00:00Z".into(),
            model: "original-model".into(),
            mode: "cloud".into(),
            endpoint: "https://example.test/v1/".into(),
            prompt_version: meeting_notes::PROMPT_VERSION.into(),
            result_schema: meeting_notes::RESULT_SCHEMA,
        };
        let original = serde_json::to_vec(&snapshot).unwrap();
        std::fs::write(folder.join("snapshot.json"), &original).unwrap();
        let checkpoint = br#"{"items":[]}"#;
        std::fs::write(folder.join("step-0000-000000.json"), checkpoint).unwrap();
        let first = Service::new(root.clone()).unwrap();
        assert_eq!(
            pending_analyses(&first, &meeting_id).unwrap().items.len(),
            1
        );
        drop(first);
        let restarted = Service::new(root).unwrap();
        assert!(restarted.notes_jobs.snapshot().unwrap().is_none());
        let pending = pending_analyses(&restarted, &meeting_id)
            .unwrap()
            .items
            .pop()
            .unwrap();
        assert_eq!(pending.confirmed_tasks, 1);
        assert_eq!(pending.total_tasks, 2);
        assert_eq!(pending.model, "original-model");
        assert_eq!(pending.template.name, "Original instructions");
        let public = serde_json::to_value(&pending).unwrap();
        for name in ["sources", "apiKey", "cloudConsent", "useSavedKey"] {
            assert!(public.get(name).is_none());
        }
        let mut attempt = request(&meeting_notes::new_job_id());
        attempt.meeting_id = meeting_id.clone();
        attempt.resume_run_id = Some(pending.id);
        attempt.template = pending.template;
        attempt.endpoint = pending.endpoint;
        attempt.mode = pending.mode;
        attempt.model = pending.model;
        assert!(analyze(&restarted, attempt)
            .unwrap_err()
            .contains("Textfreigabe"));
        assert_eq!(
            std::fs::read(folder.join("snapshot.json")).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read(folder.join("step-0000-000000.json")).unwrap(),
            checkpoint
        );
        assert_eq!(
            pending_analyses(&restarted, &meeting_id)
                .unwrap()
                .items
                .len(),
            1
        );

        // A damaged run and a run of an earlier prompt version must not hide the others.
        let damaged =
            session_analyses(&restarted, &meeting_id).join("1123456789abcdef0123456789abcdef");
        std::fs::create_dir(&damaged).unwrap();
        std::fs::write(damaged.join("snapshot.json"), b"{\"id\":").unwrap();
        let earlier =
            session_analyses(&restarted, &meeting_id).join("2123456789abcdef0123456789abcdef");
        std::fs::create_dir(&earlier).unwrap();
        let mut old: serde_json::Value = serde_json::from_slice(&original).unwrap();
        old["id"] = "2123456789abcdef0123456789abcdef".into();
        old["promptVersion"] = "handymeeting-rust-notes-1".into();
        old.as_object_mut().unwrap().remove("resultSchema");
        let old = serde_json::to_vec(&old).unwrap();
        std::fs::write(earlier.join("snapshot.json"), &old).unwrap();
        let listing = pending_analyses(&restarted, &meeting_id).unwrap();
        assert_eq!(listing.items.len(), 2);
        let readonly = listing
            .items
            .iter()
            .find(|item| item.id == "2123456789abcdef0123456789abcdef")
            .unwrap();
        assert!(!readonly.resumable);
        assert_eq!(readonly.prompt_version, "handymeeting-rust-notes-1");
        assert!(listing
            .items
            .iter()
            .any(|item| item.id == "0123456789abcdef0123456789abcdef" && item.resumable));
        assert_eq!(
            listing.skipped,
            [meeting_notes::SkippedAnalysis {
                id: "1123456789abcdef0123456789abcdef".into(),
                reason: meeting_notes::SkipReason::Damaged,
            }]
        );
        assert_eq!(
            std::fs::read(damaged.join("snapshot.json")).unwrap(),
            b"{\"id\":"
        );
        assert_eq!(std::fs::read(earlier.join("snapshot.json")).unwrap(), old);
        let mut resume = request(&meeting_notes::new_job_id());
        resume.meeting_id = meeting_id.clone();
        resume.resume_run_id = Some(readonly.id.clone());
        resume.template = readonly.template.clone();
        resume.model = readonly.model.clone();
        assert!(analyze(&restarted, resume)
            .unwrap_err()
            .contains("anderen Promptfassung"));
        assert_eq!(std::fs::read(earlier.join("snapshot.json")).unwrap(), old);
    }

    fn session_analyses(service: &Service, meeting_id: &str) -> std::path::PathBuf {
        crate::meeting_files::session_folder(&service.root, meeting_id)
            .unwrap()
            .join("analyses")
    }

    fn request(id: &str) -> NotesRequest {
        NotesRequest {
            job_id: Some(id.into()),
            meeting_id: "0123456789abcdef0123456789abcdef".into(),
            template: meeting_notes::Template {
                id: "fixture".into(),
                name: "Fixture".into(),
                steps: vec![],
            },
            endpoint: "http://127.0.0.1:1234/v1/".into(),
            model: "fixture".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
            use_saved_key: false,
            resume_run_id: None,
        }
    }
    #[test]
    fn stale_cancel_and_reused_ids_cannot_affect_a_new_job() {
        let jobs = NotesJobs::default();
        let first = "0123456789abcdef0123456789abcdef";
        let second = "1123456789abcdef0123456789abcdef";
        let (id, control) = jobs.begin(&request(first)).unwrap();
        assert!(jobs.begin(&request(second)).is_err());
        jobs.finish(&id, &Err(anyhow::anyhow!("synthetic failure")));
        assert!(jobs
            .begin(&request("01234567-89ab-cdef-0123-456789abcdef"))
            .is_err());
        let (_, new_control) = jobs.begin(&request(second)).unwrap();
        assert!(!jobs.cancel(first).unwrap());
        assert!(!new_control.is_cancelled());
        assert!(jobs.cancel(second).unwrap());
        assert!(new_control.is_cancelled());
        assert!(!control.is_cancelled());
    }
    #[test]
    fn progress_counts_only_confirmed_tasks_and_rejects_non_uuid_ids() {
        let jobs = NotesJobs::default();
        assert!(jobs.begin(&request("../not-a-job")).is_err());
        let (id, _) = jobs
            .begin(&request("0123456789abcdef0123456789abcdef"))
            .unwrap();
        jobs.update(
            &id,
            AnalysisProgress {
                run_id: "run".into(),
                stage: "requesting".into(),
                step_name: Some("Summary".into()),
                step_index: 2,
                total_steps: 2,
                batch_index: 1,
                total_batches: 3,
                confirmed_tasks: 3,
                total_tasks: 6,
            },
        );
        let p = jobs.snapshot().unwrap().unwrap();
        assert_eq!(p.confirmed_tasks, 3);
        assert_eq!(p.total_tasks, 6);
        assert_eq!(p.status, "running");
        assert!(p.error.is_none());
    }
}
