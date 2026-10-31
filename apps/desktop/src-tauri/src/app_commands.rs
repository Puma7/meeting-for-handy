//! Narrow renderer commands; filesystem and service policy stay in the backend.
use super::*;
use tauri::State;

/// Return only scalar telemetry of the active capture; idle polling opens no devices.
#[tauri::command]
pub(crate) fn get_audio_levels(
    service: State<'_, Arc<Service>>,
) -> AppResult<Option<meeting_core::CaptureAudioLevels>> {
    service.audio_levels()
}

/// Whole-computer processor and memory load for the recording status; reads no session data.
#[tauri::command]
pub(crate) fn get_system_load() -> crate::system_load::SystemLoad {
    crate::system_load::sample()
}

/// Collect finished catch-up work when the transition gate is free, then return bounded app state.
/// A collection error is left for a later poll rather than blocking the state snapshot.
#[tauri::command]
pub(crate) async fn get_app_state(service: State<'_, Arc<Service>>) -> AppResult<AppView> {
    let service = Arc::clone(&service);
    blocking(move || {
        // A finished catch-up after stop or resume is collected here, so it never keeps
        // reporting active work. A failed collection is retried on the next poll.
        let _ = service.reap_when_idle();
        service.state()
    })
    .await
}
/// Refresh installation metadata on a worker and return the backend status, including query errors.
/// Explicit paths remain subject to the Service native-choice checks.
#[tauri::command]
pub(crate) async fn refresh_handy(
    service: State<'_, Arc<Service>>,
    executable: Option<String>,
) -> AppResult<HandyStatus> {
    let service = Arc::clone(&service);
    blocking(move || match service.refresh(executable) {
        Ok((_, _, status)) => Ok(status),
        Err(_) => service
            .lock()?
            .handy
            .clone()
            .ok_or("Handy konnte nicht gelesen werden.".into()),
    })
    .await
}
/// Enumerate available audio devices off the UI thread without starting a recording.
#[tauri::command]
pub(crate) async fn list_devices() -> AppResult<Vec<AudioDevice>> {
    blocking(|| {
        meeting_core::list_devices()
            .map_err(|e| format!("Audiogeräte konnten nicht aufgelistet werden: {e} / Audio devices could not be listed."))
    })
    .await
}
/// Delegate a manual recording request to Service admission, model and device validation.
#[tauri::command]
pub(crate) async fn start_meeting(
    service: State<'_, Arc<Service>>,
    request: StartRequest,
) -> AppResult<MeetingView> {
    let service = Arc::clone(&service);
    blocking(move || service.start(request)).await
}
/// Close recording through the manual Service action; unfinished recognition may catch up afterward.
#[tauri::command]
pub(crate) async fn stop_meeting(service: State<'_, Arc<Service>>) -> AppResult<MeetingView> {
    let service = Arc::clone(&service);
    blocking(move || service.stop()).await
}
/// Change pause state only for the named active manual recording and return its updated app view.
#[tauri::command]
pub(crate) async fn set_capture_paused(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    paused: bool,
) -> AppResult<AppView> {
    let service = Arc::clone(&service);
    blocking(move || service.set_capture_control(&meeting_id, Some(paused), None)).await
}
/// Change microphone gating only for the named active manual recording; missing mic tracks fail.
#[tauri::command]
pub(crate) async fn set_microphone_muted(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    muted: bool,
) -> AppResult<AppView> {
    let service = Arc::clone(&service);
    blocking(move || service.set_capture_control(&meeting_id, None, Some(muted))).await
}
/// Apply a visible approval or denial to the matching pending agent request on a worker.
#[tauri::command]
pub(crate) async fn reconnect_microphone(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<AppView> {
    let service = Arc::clone(&service);
    blocking(move || service.reconnect_microphone(&meeting_id)).await
}
/// Apply a visible approval or denial to the matching pending agent request on a worker.
#[tauri::command]
pub(crate) async fn agent_decide_recording(
    service: State<'_, Arc<Service>>,
    request_id: String,
    approve: bool,
    request: Option<StartRequest>,
) -> AppResult<()> {
    let service = Arc::clone(&service);
    blocking(move || service.decide_agent(&request_id, approve, request)).await
}
/// Revoke the agent connection approval and request shutdown through its ownership controller.
#[tauri::command]
pub(crate) fn agent_revoke(service: State<'_, Arc<Service>>) -> AppResult<()> {
    service.agent.revoke();
    Ok(())
}
/// Return derived history summaries without loading full transcripts into the renderer list.
#[tauri::command]
pub(crate) async fn list_meetings(service: State<'_, Arc<Service>>) -> AppResult<Vec<MeetingView>> {
    let service = Arc::clone(&service);
    blocking(move || service.history()).await
}
/// Resolve a known meeting identity and return its full transcript view with separate user labels.
#[tauri::command]
pub(crate) async fn get_meeting(
    service: State<'_, Arc<Service>>,
    id: String,
) -> AppResult<MeetingView> {
    let service = Arc::clone(&service);
    blocking(move || service.data(&id).map(|d| service.meeting_view(d, false))).await
}
/// Resume a saved session through the original-model and operation-admission checks.
#[tauri::command]
pub(crate) async fn resume_meeting(
    service: State<'_, Arc<Service>>,
    id: String,
) -> AppResult<MeetingView> {
    let service = Arc::clone(&service);
    blocking(move || service.resume(id)).await
}
/// Open a natively confirmed legacy manifest read-only and retain a local import reference.
#[tauri::command]
pub(crate) async fn import_legacy(
    service: State<'_, Arc<Service>>,
    manifest: String,
) -> AppResult<MeetingView> {
    let service = Arc::clone(&service);
    blocking(move || service.import(manifest)).await
}
/// Copy at most 32 MiB of supplied text on a worker; report unavailable clipboard access.
#[tauri::command]
pub(crate) async fn copy_text(text: String) -> AppResult<()> {
    blocking(move || {
        if text.len() > 32 * 1024 * 1024 {
            return Err(
                "Der Text ist zum Kopieren zu groß. Bitte den gespeicherten Export verwenden."
                    .into(),
            );
        }
        let mut clipboard =
            arboard::Clipboard::new().map_err(|_| "Die Zwischenablage ist nicht verfügbar.")?;
        clipboard
            .set_text(text)
            .map_err(|_| "Text konnte nicht in die Zwischenablage kopiert werden.".into())
    })
    .await
}
/// Open the folder resolved from a verified meeting identity, never a renderer-supplied path.
#[tauri::command]
pub(crate) async fn open_meeting_folder(
    service: State<'_, Arc<Service>>,
    id: String,
) -> AppResult<()> {
    let service = Arc::clone(&service);
    blocking(move || {
        // Resolve through the known session identity, never a frontend-supplied path.
        service.data(&id)?;
        let folder = if valid_legacy_id(&id) {
            let manifest = service
                .lock()?
                .legacy
                .get(&id)
                .cloned()
                .ok_or("Unknown imported session")?;
            manifest
                .parent()
                .ok_or("Session folder missing")?
                .to_owned()
        } else {
            meeting_files::session_folder(&service.root, &id)?
        };
        meeting_files::open_folder(&folder)
    })
    .await
}
/// Hold an export lease while saving a new full transcript file in the requested format.
/// Legacy originals remain read-only; omitted format keeps the existing TXT behavior.
#[tauri::command]
pub(crate) async fn export_transcript(
    service: State<'_, Arc<Service>>,
    id: String,
    locale: String,
    format: Option<String>,
) -> AppResult<String> {
    let service = Arc::clone(&service);
    blocking(move || {
        let _operation = service.reserve_operation(operations::Kind::Export)?;
        let _source = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Meeting erasure is in progress; wait for it to finish. / Meetingdaten werden gerade gelöscht.")?;
        let data = service.data(&id)?;
        if data.read_only {
            return Err("Diese Originalsitzung ist schreibgeschützt. Das Transkript kann weiterhin vollständig kopiert werden. / This original session is read-only; the full transcript can still be copied.".into());
        }
        let folder = meeting_files::session_folder(&service.root, &id)?;
        if format.as_deref().unwrap_or("txt") == "txt" {
            meeting_files::save_transcript(&folder, &data, &locale)
        } else {
            meeting_files::save_transcript_format(&folder, &data, &locale, format.as_deref().unwrap_or("txt"))
        }
    }).await
}
/// Run analysis through the Service source-snapshot, credential and per-run consent boundary.
#[tauri::command]
pub(crate) async fn generate_notes(
    service: State<'_, Arc<Service>>,
    request: NotesRequest,
) -> AppResult<meeting_notes::Analysis> {
    let service = Arc::clone(&service);
    blocking(move || service.analyze(request)).await
}
/// List individually checked saved analyses and report damaged runs separately.
/// Legacy imports expose no analysis files through this command.
#[tauri::command]
pub(crate) async fn list_analyses(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<meeting_notes::AnalysisListing> {
    let service = Arc::clone(&service);
    // Each result is checked on its own: damaged files are reported as skipped, earlier prompt versions stay listed read-only.
    blocking(move||{if valid_legacy_id(&meeting_id){return Ok(Default::default());}validate_id(&meeting_id)?;let root=service.root.join("sessions").join(&meeting_id).join("analyses");if !root.exists(){return Ok(Default::default());}meeting_notes::list_analyses(&root,&meeting_id).map_err(|_|"Gespeicherte Auswertungen konnten nicht gelesen werden. Originaldateien bleiben erhalten.".into())}).await
}
