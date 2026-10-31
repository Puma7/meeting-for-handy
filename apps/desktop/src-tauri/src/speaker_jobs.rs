//! Optional speaker work owns only its child process; capture and original text are independent.
use crate::{
    asr::{self, Reply, Request, Worker},
    AppResult, Service,
};
use handy_bridge::native::{NativeSpeakerEngine, SpeakerTurn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{BufReader, BufWriter},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const MAX_MODEL_CHECK_FOLDERS: usize = crate::speaker_check_archive::MAX_RECORDS;

/// Reads a completed capability result only for the freshly rehashed full model/runtime
/// identity. Partial or mismatched evidence never grants model capability.
fn cached_model_check(
    root: &Path,
    identity: &handy_bridge::ModelIdentity,
) -> AppResult<Option<bool>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ResultRecord {
        model_id: String,
        supported: bool,
        checked_at: String,
        audio_access: bool,
    }
    let checks = root.join("speaker-model-checks");
    match std::fs::symlink_metadata(&checks) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Model check evidence unavailable".into()),
        Ok(_) => {}
    }
    meeting_core::metadata::checked_directory(&checks).map_err(|_| "Unsafe model check folder")?;
    let entries = std::fs::read_dir(&checks)
        .map_err(|_| "Model check evidence unavailable")?
        .take(MAX_MODEL_CHECK_FOLDERS + 1)
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| "Model check evidence unavailable")?;
    if entries.len() > MAX_MODEL_CHECK_FOLDERS {
        return Err(format!("{} Das Prüfarchiv ist voll. Vorhandene Nachweise bleiben erhalten. / The check archive is full; all records are retained.", crate::speaker_check_archive::ARCHIVE_REQUIRED));
    }
    let mut found = None;
    for entry in entries {
        let folder = entry.path();
        if meeting_core::metadata::checked_directory(&folder).is_err() {
            continue;
        }
        let stored = json_file(&folder.join("model.json"))
            .ok()
            .and_then(|v| serde_json::from_value::<handy_bridge::ModelIdentity>(v).ok());
        if stored.as_ref() != Some(identity) {
            continue;
        }
        let result = json_file(&folder.join("result.json"))
            .ok()
            .and_then(|v| serde_json::from_value::<ResultRecord>(v).ok());
        let Some(result) = result else {
            continue;
        };
        if result.model_id != identity.source_model_metadata.id
            || result.audio_access
            || chrono::DateTime::parse_from_rfc3339(&result.checked_at).is_err()
        {
            continue;
        }
        if found.is_some_and(|previous| previous != result.supported) {
            return Err(format!("{} Widersprüchliche Prüfnachweise bleiben erhalten. Bitte archivieren und erneut prüfen. / Conflicting check records are retained. Archive them, then check again.", crate::speaker_check_archive::ARCHIVE_REQUIRED));
        }
        found = Some(result.supported);
    }
    Ok(found)
}

/// Preserve all prior checks, including partial failures. Stop reserving new evidence
/// folders at a fixed ceiling instead of silently deleting or overwriting their contents.
fn reserve_model_check_folder(root: &Path) -> AppResult<std::path::PathBuf> {
    let checks = root.join("speaker-model-checks");
    std::fs::create_dir_all(&checks).map_err(|_| "Model check folder unavailable")?;
    meeting_core::metadata::checked_directory(&checks).map_err(|_| "Unsafe model check folder")?;
    let used = std::fs::read_dir(&checks)
        .map_err(|_| "Model check evidence unavailable")?
        .take(MAX_MODEL_CHECK_FOLDERS)
        .try_fold(0_usize, |count, entry| entry.map(|_| count + 1))
        .map_err(|_| "Model check evidence could not be counted")?;
    if used >= MAX_MODEL_CHECK_FOLDERS {
        return Err(format!("{} 128 Sprecher-Prüfnachweise bleiben erhalten. Bitte die Nachweise archivieren und danach erneut prüfen. / 128 speaker-check records are retained. Archive the check records, then run the model check again.", crate::speaker_check_archive::ARCHIVE_REQUIRED));
    }
    let folder = checks.join(meeting_notes::new_job_id());
    std::fs::create_dir(&folder).map_err(|_| "Could not reserve model check")?;
    Ok(folder)
}

/// Explicit capability check in an owned CPU child, without access to any recording folder.
/// Exclusive admission avoids loading another model while capture or analysis is running.
pub(crate) fn check_model(service: &Service, model_id: &str) -> AppResult<bool> {
    let _lease = service.reserve_operation(crate::operations::Kind::Selection)?;
    let _proof_lock = crate::speaker_check_archive::lock_checks(&service.root)?;
    let (bridge, catalog, _) = service.refresh(None)?;
    let model = catalog.model(model_id).map_err(|e| e.to_string())?;
    let resolved = bridge
        .resolve_downloaded_model(model)
        .map_err(|e| e.to_string())?;
    let identity = handy_bridge::capture_optional_model_identity(
        model,
        &resolved,
        handy_bridge::native::runtime_identity(),
    )
    .map_err(|e| e.to_string())?;
    if let Some(supported) = cached_model_check(&service.root, &identity)? {
        return Ok(supported);
    }
    let folder = reserve_model_check_folder(&service.root)?;
    crate::create_json(&folder.join("model.json"), &identity)?;
    let mut worker = Worker::start_speaker(
        &resolved.path,
        &crate::backend_directory()?,
        &folder,
        Arc::new(AtomicBool::new(false)),
    )?;
    let supported = match worker.call(Request::SpeakerIdentity { identity }) {
        Ok(_) => true,
        Err(error) if error.contains("does not support local speaker separation") => false,
        Err(error) => return Err(error),
    };
    crate::create_json(
        &folder.join("result.json"),
        &serde_json::json!({"modelId":model_id,"supported":supported,"checkedAt":chrono::Utc::now().to_rfc3339(),"audioAccess":false}),
    )?;
    Ok(supported)
}

#[cfg(test)]
mod model_check_tests {
    use super::*;

    #[test]
    /// Fills all 128 evidence slots and verifies exact-identity reuse after restart, rejecting
    /// changed weights/runtime/catalogue and malformed or contradictory retained results.
    fn a_full_check_archive_reuses_only_complete_exact_model_and_runtime_identity() {
        let root =
            std::env::temp_dir().join(format!("hm-speaker-cache-{}", meeting_notes::new_job_id()));
        std::fs::create_dir(&root).unwrap();
        let model = handy_bridge::HandyModel {
            id: "synthetic".into(),
            name: "Synthetic".into(),
            filename: "weights.bin".into(),
            source: serde_json::json!({"revision":"one"}),
            is_downloaded: true,
            is_downloading: false,
            supports_streaming: false,
            supported_languages: vec![],
        };
        let weights = root.join("weights.bin");
        std::fs::write(&weights, b"original weights").unwrap();
        let resolved = handy_bridge::ResolvedModel {
            model_id: model.id.clone(),
            path: weights.clone(),
            resolution: "synthetic".into(),
            revision: Some("one".into()),
        };
        let runtime = handy_bridge::RuntimeIdentity {
            compiled_version: "synthetic".into(),
            loaded_version: "synthetic".into(),
            loaded_commit: "one".into(),
            public_header_hash: "one".into(),
        };
        let identity =
            handy_bridge::capture_optional_model_identity(&model, &resolved, runtime.clone())
                .unwrap();
        let folder = reserve_model_check_folder(&root).unwrap();
        crate::create_json(&folder.join("model.json"), &identity).unwrap();
        let result = serde_json::json!({"modelId":model.id,"supported":true,"checkedAt":"2026-10-28T00:00:00Z","audioAccess":false});
        crate::create_json(&folder.join("result.json"), &result).unwrap();
        for _ in 1..MAX_MODEL_CHECK_FOLDERS {
            reserve_model_check_folder(&root).unwrap();
        }
        let before = std::fs::read(folder.join("result.json")).unwrap();
        assert_eq!(cached_model_check(&root, &identity).unwrap(), Some(true));
        assert_eq!(std::fs::read(folder.join("result.json")).unwrap(), before);
        assert!(
            reserve_model_check_folder(&root).is_err(),
            "the fixed evidence limit remains"
        );
        for change in ["runtime", "revision", "metadata", "path", "hash"] {
            let mut different = identity.clone();
            match change {
                "runtime" => different.native_runtime_identity.loaded_commit = "two".into(),
                "revision" => different.resolved_revision = Some("two".into()),
                "metadata" => {
                    different.source_model_metadata.source = serde_json::json!({"revision":"two"})
                }
                "path" => different.canonical_path = root.join("different.bin"),
                _ => different.sha256 = "different".into(),
            }
            assert_eq!(
                cached_model_check(&root, &different).unwrap(),
                None,
                "{change}"
            );
        }
        std::fs::write(&weights, b"changed! weights").unwrap();
        let changed =
            handy_bridge::capture_optional_model_identity(&model, &resolved, runtime).unwrap();
        assert_eq!(changed.bytes, identity.bytes);
        assert_eq!(cached_model_check(&root, &changed).unwrap(), None);
        let mut unsupported = result.clone();
        unsupported["supported"] = false.into();
        std::fs::write(
            folder.join("result.json"),
            serde_json::to_vec(&unsupported).unwrap(),
        )
        .unwrap();
        assert_eq!(cached_model_check(&root, &identity).unwrap(), Some(false));
        for invalid in [b"{".as_slice(), b"{}".as_slice()] {
            std::fs::write(folder.join("result.json"), invalid).unwrap();
            assert_eq!(cached_model_check(&root, &identity).unwrap(), None);
            assert_eq!(std::fs::read(folder.join("result.json")).unwrap(), invalid);
        }
        for (field, value) in [
            ("modelId", serde_json::json!("different")),
            ("checkedAt", serde_json::json!("invalid date")),
            ("audioAccess", serde_json::json!(true)),
        ] {
            let mut invalid = result.clone();
            invalid[field] = value;
            std::fs::write(
                folder.join("result.json"),
                serde_json::to_vec(&invalid).unwrap(),
            )
            .unwrap();
            assert_eq!(
                cached_model_check(&root, &identity).unwrap(),
                None,
                "{field}"
            );
        }
        std::fs::write(folder.join("result.json"), &before).unwrap();
        let conflicting = std::fs::read_dir(root.join("speaker-model-checks"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| *path != folder)
            .unwrap();
        crate::create_json(&conflicting.join("model.json"), &identity).unwrap();
        crate::create_json(&conflicting.join("result.json"), &unsupported).unwrap();
        assert!(cached_model_check(&root, &identity).is_err());
        assert_eq!(std::fs::read(folder.join("result.json")).unwrap(), before);
    }

    #[test]
    /// Retains complete and partial proof folders at the fixed ceiling and verifies that
    /// refusing a 129th reservation neither removes nor overwrites earlier evidence.
    fn model_check_evidence_is_bounded_without_removing_partial_or_complete_records() {
        let root = std::env::temp_dir().join(format!(
            "hm-speaker-check-cap-{}",
            meeting_notes::new_job_id()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut retained = Vec::new();
        for index in 0..MAX_MODEL_CHECK_FOLDERS {
            let folder = reserve_model_check_folder(&root).unwrap();
            if index % 2 == 0 {
                let proof = folder.join("synthetic-proof.json");
                std::fs::write(&proof, b"preserve this confirmed check").unwrap();
                retained.push(proof);
            }
        }
        let error = reserve_model_check_folder(&root).unwrap_err();
        assert!(error.contains("128"));
        assert_eq!(
            std::fs::read_dir(root.join("speaker-model-checks"))
                .unwrap()
                .count(),
            MAX_MODEL_CHECK_FOLDERS
        );
        for path in retained {
            assert_eq!(
                std::fs::read(path).unwrap(),
                b"preserve this confirmed check"
            );
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Section {
    pub source_id: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub audio_sha256: String,
    pub turns: Vec<SpeakerTurn>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Progress {
    pub job_id: String,
    pub meeting_id: String,
    pub mode: String,
    pub status: String,
    pub stage: String,
    pub model_id: String,
    pub confirmed_tasks: u64,
    pub total_tasks: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub cancel_requested: bool,
    pub last_section: Option<Section>,
}
struct Job {
    view: Progress,
    started: Instant,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub(super) struct Jobs {
    inner: Mutex<Option<Job>>,
}
impl Jobs {
    /// Discards finished section text for exactly the explicitly erased meeting.
    pub(crate) fn forget(&self, meeting: &str) -> AppResult<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Sprechersteuerung ist gesperrt.")?;
        if state
            .as_ref()
            .is_some_and(|job| job.view.meeting_id == meeting && job.view.status == "running")
        {
            return Err("Sprecherprüfung läuft noch. / Speaker analysis is still running.".into());
        }
        if state
            .as_ref()
            .is_some_and(|job| job.view.meeting_id == meeting)
        {
            *state = None;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Option<Progress> {
        let state = self.inner.lock().ok()?;
        let job = state.as_ref()?;
        let mut view = job.view.clone();
        if view.status == "running" {
            view.elapsed_ms = job.started.elapsed().as_millis() as u64;
        }
        Some(view)
    }
    pub fn cancel(&self, id: &str) -> bool {
        if let Ok(mut state) = self.inner.lock() {
            if let Some(job) = state
                .as_mut()
                .filter(|j| j.view.job_id == id && j.view.status == "running")
            {
                job.cancel.store(true, Ordering::Release);
                job.view.cancel_requested = true;
                return true;
            }
        }
        false
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut Progress)) {
        if let Ok(mut state) = self.inner.lock() {
            if let Some(job) = state.as_mut().filter(|j| j.view.job_id == id) {
                change(&mut job.view);
                job.view.elapsed_ms = job.started.elapsed().as_millis() as u64;
            }
        }
    }
    /// Validate live/post-meeting experiment state and retain speaker admission in its worker;
    /// optional-worker failure does not own normal capture shutdown.
    pub fn start(
        self: &Arc<Self>,
        service: Arc<Service>,
        meeting_id: String,
        model_id: String,
        mode: String,
    ) -> AppResult<String> {
        if !["post-meeting", "live"].contains(&mode.as_str())
            || !meeting_core::metadata::valid_id(&meeting_id)
            || meeting_id.starts_with("legacy-")
        {
            return Err("Invalid experimental speaker request".into());
        }
        let lease = service.reserve_operation(crate::operations::Kind::Speakers)?;
        let source_lease = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Meeting erasure is in progress; wait for it to finish. / Meetingdaten werden gerade gelöscht.")?;
        let data = service.data(&meeting_id)?;
        let recording = service.lock()?.active.as_ref().is_some_and(|a| {
            a.capture.is_some()
                && a.store
                    .preview_snapshot(0)
                    .is_ok_and(|d| d.id == meeting_id)
        });
        if data.read_only
            || mode == "live" && !recording
            || mode == "post-meeting" && (recording || data.status != "complete")
        {
            return Err("Choose a completed meeting for post-meeting separation, or the active recording for the live experiment. / Abgeschlossenes Meeting oder für das Live-Experiment die laufende Aufnahme wählen.".into());
        }
        let id = meeting_notes::new_job_id();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut state = self
                .inner
                .lock()
                .map_err(|_| "Speaker job state unavailable")?;
            if state.as_ref().is_some_and(|j| j.view.status == "running") {
                return Err("Another optional speaker job is active. Stop it or wait; normal recording remains available.".into());
            }
            *state = Some(Job {
                view: Progress {
                    job_id: id.clone(),
                    meeting_id: meeting_id.clone(),
                    mode: mode.clone(),
                    status: "running".into(),
                    stage: "preparing".into(),
                    model_id: model_id.clone(),
                    confirmed_tasks: 0,
                    total_tasks: 0,
                    elapsed_ms: 0,
                    error: None,
                    cancel_requested: false,
                    last_section: None,
                },
                started: Instant::now(),
                cancel: cancel.clone(),
            });
        }
        let jobs = self.clone();
        let owned = id.clone();
        let spawn=std::thread::Builder::new().name("meeting-optional-speakers".into()).spawn(move||{
   let _lease = lease;
   let _source_lease = source_lease;
   let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||run(&service,&jobs,&owned,&meeting_id,&model_id,&mode,&cancel))).unwrap_or_else(|_|Err("Optional speaker worker stopped unexpectedly; capture and original transcripts are preserved".into()));
   jobs.update(&owned,|p|{p.status=if result.is_ok(){"complete"}else if cancel.load(Ordering::Acquire){"cancelled"}else{"failed"}.into();p.stage="finished".into();p.error=result.err();if p.total_tasks==0{p.total_tasks=p.confirmed_tasks;}});
  });
        if spawn.is_err() {
            self.update(&id, |p| {
                p.status = "failed".into();
                p.stage = "finished".into();
                p.error =
                    Some("Optional speaker worker could not start. Recording is unchanged.".into());
            });
            return Err("Optional speaker job could not start".into());
        }
        Ok(id)
    }
}
fn run(
    service: &Service,
    jobs: &Jobs,
    id: &str,
    meeting_id: &str,
    model_id: &str,
    mode: &str,
    cancel: &Arc<AtomicBool>,
) -> AppResult<()> {
    let (bridge, catalog, _) = service.refresh(None)?;
    let model = catalog.model(model_id).map_err(|e| e.to_string())?;
    let resolved = bridge
        .resolve_downloaded_model(model)
        .map_err(|e| e.to_string())?;
    let identity = handy_bridge::capture_optional_model_identity(
        model,
        &resolved,
        handy_bridge::native::runtime_identity(),
    )
    .map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Acquire) {
        return Err("Optional speaker analysis cancelled".into());
    }
    let folder = crate::meeting_files::session_folder(&service.root, meeting_id)?;
    let runs = folder.join("experimental-speakers");
    match std::fs::create_dir(&runs) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err("Could not create speaker evidence folder".into()),
    }
    meeting_core::metadata::checked_directory(&runs)
        .map_err(|_| "Unsafe speaker evidence folder")?;
    let output = runs.join(id);
    std::fs::create_dir(&output).map_err(|_| "Could not reserve a new speaker revision")?;
    crate::create_json(
        &output.join("snapshot.json"),
        &serde_json::json!({"version":crate::VERSION,"runId":id,"meetingId":meeting_id,"mode":mode,"model":identity,"speakerIdentity":"neutral IDs scoped to each audio section and track; no personal identity or cross-section match","wordAlignment":"none; original ASR section text remains unchanged","compute":"independent CPU worker"}),
    )?;
    jobs.update(id, |p| p.stage = "loading".into());
    let mut worker = Worker::start_speaker(
        &resolved.path,
        &crate::backend_directory()?,
        &folder,
        cancel.clone(),
    )?;
    worker.call(Request::SpeakerIdentity { identity })?;
    let mut seen = HashSet::new();
    let mut count = 0u64;
    let mut open_sections;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Optional speaker analysis cancelled. Confirmed speaker revisions, recordings and original text are retained. / Sprecherexperiment abgebrochen; bestätigte Fassungen und Originale bleiben erhalten.".into());
        }
        // Read the recording state before the data: a section closed in between is then still
        // in this snapshot. Stopping removes the active capture before its threads have joined
        // and closed the final sections, so a running transition also counts as recording.
        let recording = {
            let inner = service.lock()?;
            inner.transitioning
                || inner.active.as_ref().is_some_and(|a| {
                    a.capture.is_some()
                        && a.store
                            .preview_snapshot(0)
                            .is_ok_and(|d| d.id == meeting_id)
                })
        };
        let data = service.data(meeting_id)?;
        open_sections = data.parts.iter().filter(|p| !p.audio_closed).count();
        let parts: Vec<_> = data
            .parts
            .into_iter()
            .filter(|p| p.audio_closed && p.confirmed_sample_count > 0 && !seen.contains(&p.file))
            .collect();
        jobs.update(id, |p| {
            p.stage = if parts.is_empty() && recording {
                "waiting"
            } else {
                "running"
            }
            .into();
            p.total_tasks = if mode == "live" && recording {
                0
            } else {
                count + parts.len() as u64
            };
        });
        for mut part in parts {
            let original = part.clone();
            part.text = None;
            part.committed_text.clear();
            part.tentative_text.clear();
            part.error = None;
            let reply = worker.call(Request::SpeakerSection { part })?;
            let section = Section {
                source_id: original.file.clone(),
                start_ms: original.start_sample * 1000 / u64::from(meeting_core::SAMPLE_RATE),
                duration_ms: original.confirmed_sample_count * 1000
                    / u64::from(meeting_core::SAMPLE_RATE),
                audio_sha256: original.confirmed_sha256,
                turns: reply.speaker_turns,
            };
            crate::create_json(&output.join(format!("section-{count:08}.json")), &section)?;
            seen.insert(original.file);
            count += 1;
            jobs.update(id, |p| {
                p.confirmed_tasks = count;
                p.last_section = Some(section);
            });
        }
        if mode != "live" || !recording {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if open_sections > 0 {
        // Never report a live run as complete while recorded audio is still unclosed.
        return Err("Some recorded sections were not closed, so speaker sections are incomplete. Confirmed sections are kept. / Einige Aufnahmeabschnitte wurden nicht abgeschlossen; bestätigte Sprecherabschnitte bleiben erhalten.".into());
    }
    crate::create_json(
        &output.join("complete.json"),
        &serde_json::json!({"runId":id,"confirmedSections":count,"success":true}),
    )?;
    Ok(())
}
pub fn worker_main(args: &[String]) -> AppResult<()> {
    if args.len() != 5 || args[3] != "speaker" || args[4] != "--cpu" {
        return Err("Invalid independent speaker worker arguments".into());
    }
    let folder = meeting_core::metadata::checked_directory(Path::new(&args[2]))
        .map_err(|_| "Speaker session folder unavailable")?;
    let mut input = BufReader::new(std::io::stdin());
    let mut output = BufWriter::new(std::io::stdout());
    asr::send_reply(
        &mut output,
        Reply {
            ok: true,
            backend: "independent-cpu-speaker-role".into(),
            ..Default::default()
        },
    )?;
    let mut engine = None;
    loop {
        let Some(request) = asr::read_request(&mut input)? else {
            return Ok(());
        };
        let result = (|| -> AppResult<Reply> {
            match request {
                Request::SpeakerIdentity { identity } if engine.is_none() => {
                    handy_bridge::verify_model_identity(
                        &identity,
                        Path::new(&args[0]),
                        &handy_bridge::native::runtime_identity(),
                    )
                    .map_err(|e| e.to_string())?;
                    engine = Some(
                        NativeSpeakerEngine::load(Path::new(&args[0]), Path::new(&args[1]))
                            .map_err(|e| e.to_string())?,
                    );
                    Ok(Reply {
                        ok: true,
                        ..Default::default()
                    })
                }
                Request::SpeakerSection { part } => {
                    let engine = engine
                        .as_ref()
                        .ok_or("Speaker model identity must be confirmed first")?;
                    let pcm=meeting_core::read_closed_audio(&folder,&part).map_err(|_|"Closed speaker audio did not pass its original digest verification; recordings are retained")?;
                    Ok(Reply {
                        ok: true,
                        speaker_turns: engine.run(&pcm).map_err(|e| e.to_string())?,
                        ..Default::default()
                    })
                }
                Request::Quit => Ok(Reply {
                    ok: true,
                    ..Default::default()
                }),
                _ => Err("Unsupported optional speaker operation".into()),
            }
        })();
        match result {
            Ok(reply) => asr::send_reply(&mut output, reply)?,
            Err(error) => {
                asr::send_reply(
                    &mut output,
                    Reply {
                        ok: false,
                        error: Some(error.clone()),
                        ..Default::default()
                    },
                )?;
                return Err(error);
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SavedSections {
    run_id: String,
    mode: String,
    model_id: String,
    sections: Vec<Section>,
}
fn json_file(path: &Path) -> AppResult<serde_json::Value> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Speaker evidence is unavailable")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Linked speaker evidence rejected".into());
        }
    }
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 2 * 1024 * 1024 {
        return Err("Invalid speaker evidence file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Speaker evidence could not be read")?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Speaker evidence could not be read")?;
    serde_json::from_slice(&bytes).map_err(|_| {
        "Speaker evidence is incomplete or invalid; original files are preserved".into()
    })
}
pub(super) fn saved(service: &Service, meeting_id: &str) -> AppResult<Option<SavedSections>> {
    let data = service.data(meeting_id)?;
    let folder = crate::meeting_files::session_folder(&service.root, meeting_id)?
        .join("experimental-speakers");
    if !folder.exists() {
        return Ok(None);
    }
    meeting_core::metadata::checked_directory(&folder)
        .map_err(|_| "Unsafe speaker evidence directory")?;
    let mut runs = Vec::new();
    for (seen_entries, entry) in std::fs::read_dir(folder)
        .map_err(|_| "Speaker revisions unavailable")?
        .enumerate()
    {
        if seen_entries >= 1001 {
            return Err("Speaker folder exceeds browsing bound; all revisions retained".into());
        }
        let entry = entry.map_err(|_| "Speaker revision unavailable")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if crate::validate_id(&name).is_err() {
            continue;
        }
        let path = meeting_core::metadata::checked_directory(&entry.path())
            .map_err(|_| "Unsafe speaker revision")?;
        let time = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|_| "Speaker revision unavailable")?;
        runs.push((time, name, path));
    }
    if runs.len() > 1000 {
        return Err("Too many speaker revisions to select automatically; preserved on disk".into());
    }
    runs.sort_by(|a, b| b.0.cmp(&a.0));
    let Some((_, run_id, path)) = runs.into_iter().next() else {
        return Ok(None);
    };
    if !path.join("snapshot.json").exists()
        && service
            .speaker_jobs
            .snapshot()
            .is_some_and(|p| p.job_id == run_id && p.status == "running")
    {
        return Ok(None);
    }
    let snapshot = json_file(&path.join("snapshot.json"))?;
    if snapshot["runId"] != run_id || snapshot["meetingId"] != meeting_id {
        return Err("Speaker evidence identity mismatch".into());
    }
    let mode = snapshot["mode"]
        .as_str()
        .filter(|s| ["live", "post-meeting"].contains(s))
        .ok_or("Invalid speaker mode")?
        .to_owned();
    let model_id = snapshot["model"]["sourceModelMetadata"]["id"]
        .as_str()
        .ok_or("Speaker model provenance missing")?
        .to_owned();
    let mut files = Vec::new();
    for (seen_entries, entry) in std::fs::read_dir(&path)
        .map_err(|_| "Speaker sections unavailable")?
        .enumerate()
    {
        if seen_entries >= 100002 {
            return Err("Speaker revision exceeds browsing bound; original files retained".into());
        }
        let entry = entry.map_err(|_| "Speaker section unavailable")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 21
            && name.starts_with("section-")
            && name.ends_with(".json")
            && name[8..16].bytes().all(|b| b.is_ascii_digit())
        {
            files.push((name, entry.path()));
        }
    }
    if files.len() > 100000 {
        return Err("Speaker evidence exceeds the section limit; originals preserved".into());
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let mut sections = Vec::new();
    for (_, file) in files.into_iter().take(25) {
        let section: Section = serde_json::from_value(json_file(&file)?)
            .map_err(|_| "Invalid saved speaker section")?;
        let part = data
            .parts
            .iter()
            .find(|p| p.file == section.source_id)
            .ok_or("Speaker evidence references an unknown original section")?;
        if part.confirmed_sha256 != section.audio_sha256
            || section.start_ms != part.start_sample * 1000 / 16000
            || section.duration_ms != part.confirmed_sample_count * 1000 / 16000
            || section.turns.len() > 10000
            || section.turns.iter().any(|t| {
                t.end_ms <= t.start_ms
                    || t.end_ms > section.duration_ms
                    || !(1..=1000).contains(&t.speaker_id)
                    || t.confidence
                        .is_some_and(|p| !p.is_finite() || !(0.0..=1.0).contains(&p))
            })
        {
            return Err("Saved speaker evidence failed its original-section checks; preserved for inspection".into());
        }
        sections.push(section);
    }
    Ok(Some(SavedSections {
        run_id,
        mode,
        model_id,
        sections,
    }))
}
