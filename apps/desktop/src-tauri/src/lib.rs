mod agent;
mod app_commands;
mod asr;
mod cli;
mod credential_commands;
mod credential_service;
mod library_commands;
mod library_service;
mod live_commands;
mod live_notes;
mod local_models;
mod meeting_files;
mod model_sessions;
mod name_reference;
mod notes_commands;
mod notes_service;
mod operations;
mod path_selection;
mod retention_commands;
mod session_service;
mod speaker_check_archive;
mod speaker_commands;
mod speaker_jobs;
mod system_load;
mod template_commands;
mod update_commands;
mod updater;

use handy_bridge::{HandyBridge, ModelCatalog};
use meeting_core::{AudioDevice, CaptureController, CaptureOptions, SessionData, SessionStore};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
use tauri::{Emitter, Manager};
type AppResult<T> = Result<T, String>;
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelView {
    id: String,
    name: String,
    is_downloaded: bool,
    supports_streaming: bool,
    compatibility_error: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandyStatus {
    executable: Option<String>,
    version: Option<String>,
    models: Vec<ModelView>,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedAudio {
    system: u64,
    microphone: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingView {
    #[serde(flatten)]
    data: SessionData,
    live_preview_limited: bool,
    pending_audio_ms: u64,
    saved_audio_ms: SavedAudio,
    user_metadata: meeting_core::metadata::Metadata,
    metadata_error: Option<String>,
}
fn meeting_view(mut data: SessionData, preview: bool) -> MeetingView {
    let (mut system, mut microphone, mut pending_system, mut pending_microphone) = (0, 0, 0, 0);
    for p in &data.parts {
        let end = (p.start_sample + p.confirmed_sample_count) * 1000
            / u64::from(meeting_core::SAMPLE_RATE);
        let pending = if p.status == "complete" && p.model_id.as_deref() == Some(&data.model_id) {
            0
        } else if p.model_id.as_deref() == Some(&data.model_id) {
            p.confirmed_sample_count.saturating_sub(p.processed_samples)
        } else {
            p.confirmed_sample_count
        };
        if p.source == "system" {
            system = system.max(end);
            pending_system += pending;
        } else if p.source == "microphone" {
            microphone = microphone.max(end);
            pending_microphone += pending;
        }
    }
    data.parts
        .sort_by(|a, b| (a.start_sample, &a.source).cmp(&(b.start_sample, &b.source)));
    let limited = preview && data.parts.len() > 16;
    if limited {
        data.parts.drain(..data.parts.len() - 16);
    }
    MeetingView {
        data,
        live_preview_limited: limited,
        pending_audio_ms: pending_system.max(pending_microphone) * 1000
            / u64::from(meeting_core::SAMPLE_RATE),
        saved_audio_ms: SavedAudio { system, microphone },
        user_metadata: Default::default(),
        metadata_error: None,
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppView {
    version: &'static str,
    handy: Option<HandyStatus>,
    active_meeting: Option<MeetingView>,
    recording: bool,
    capture_control: Option<meeting_core::CaptureControlSnapshot>,
    microphone_reconnect_available: bool,
    error: Option<String>,
    data_directory: String,
    notes_running: bool,
    transitioning: bool,
    agent: Option<agent::View>,
}
struct Active {
    _operation: Option<operations::Lease>,
    owner: Option<String>,
    store: SessionStore,
    capture: Option<CaptureController>,
    stop: Arc<AtomicBool>,
    workers: Vec<asr::RecognitionWorker>,
}
#[derive(Default)]
struct Inner {
    handy: Option<HandyStatus>,
    executable: Option<PathBuf>,
    active: Option<Active>,
    view: Option<SessionStore>,
    legacy: HashMap<String, PathBuf>,
    error: Option<String>,
    transitioning: bool,
    reconnecting_microphone: bool,
    /// Ends a running microphone reconnect when the user stops the meeting.
    reconnect_cancel: Option<Arc<AtomicBool>>,
}
struct Service {
    agent: agent::Controller,
    root: PathBuf,
    inner: Mutex<Inner>,
    transition: Mutex<()>,
    operations: operations::Gate,
    selections: path_selection::Selections,
    refresh_gate: Mutex<()>,
    notes_jobs: notes_service::NotesJobs,
    live_notes: live_notes::LiveNotes,
    model_pulls: Arc<local_models::Pulls>,
    model_sessions: model_sessions::Models,
    library_jobs: Arc<library_service::Jobs>,
    updater: updater::Updater,
    speaker_jobs: Arc<speaker_jobs::Jobs>,
    maintenance_jobs: Arc<library_service::Jobs>,
}
impl Service {
    /// The fast meter poll never waits behind a long application-state operation,
    /// reads transcripts, touches disk, enumerates devices or changes capture.
    fn audio_levels(&self) -> AppResult<Option<meeting_core::CaptureAudioLevels>> {
        let inner = self.inner.try_lock().map_err(|_| {
            "Audiopegel sind vorübergehend nicht verfügbar. / Audio levels are temporarily unavailable.".to_owned()
        })?;
        Ok(inner
            .active
            .as_ref()
            .and_then(|active| active.capture.as_ref())
            .and_then(CaptureController::audio_levels))
    }
    fn meeting_view(&self, data: SessionData, preview: bool) -> MeetingView {
        let mut view = meeting_view(data, preview);
        if view.data.read_only && !view.data.id.starts_with("legacy-") {
            view.saved_audio_ms = SavedAudio {
                system: 0,
                microphone: 0,
            };
            view.pending_audio_ms = 0;
        }
        match meeting_core::metadata::read(&self.root,&view.data.id) {
            Ok(mut meta) => {
                if let Some(title) = &meta.title { view.data.title = title.clone(); }
                if view.data.read_only && !view.data.id.starts_with("legacy-") {
                    meta.audio_removed = true;
                }
                view.user_metadata = meta;
            },
            Err(_)=>view.metadata_error=Some("Saved meeting labels could not be read. Originals are preserved. / Gespeicherte Meetingbezeichnungen konnten nicht gelesen werden.".into()),
        }
        view
    }
    /// Create local session/import-reference directories and idle controllers; load bounded import
    /// reference records without capture or executable approval.
    fn new(root: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(root.join("sessions"))
            .map_err(|_| "Der lokale Sitzungsordner konnte nicht angelegt werden.")?;
        fs::create_dir_all(root.join("legacy-map"))
            .map_err(|_| "Der lokale Importordner konnte nicht angelegt werden.")?;
        let mut inner = Inner::default();
        for entry in fs::read_dir(root.join("legacy-map"))
            .map_err(|_| "Importverweise konnten nicht gelesen werden.")?
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let (Some(id), Ok(meta)) = (
                    path.file_stem().and_then(|s| s.to_str()),
                    fs::metadata(&path),
                ) {
                    if valid_legacy_id(id) && meta.len() <= 32768 {
                        if let Ok(bytes) = fs::read(&path) {
                            if let Ok(manifest) = serde_json::from_slice::<PathBuf>(&bytes) {
                                inner.legacy.insert(id.to_owned(), manifest);
                            }
                        }
                    }
                }
            }
        }
        Ok(Self {
            agent: agent::Controller::default(),
            root,
            inner: Mutex::new(inner),
            transition: Mutex::new(()),
            operations: operations::Gate::default(),
            selections: path_selection::Selections::default(),
            refresh_gate: Mutex::new(()),
            notes_jobs: notes_service::NotesJobs::default(),
            live_notes: live_notes::LiveNotes::default(),
            model_pulls: Arc::new(local_models::Pulls::default()),
            model_sessions: model_sessions::Models::default(),
            library_jobs: Arc::new(library_service::Jobs::default()),
            updater: updater::Updater::default(),
            speaker_jobs: Arc::new(speaker_jobs::Jobs::default()),
            maintenance_jobs: Arc::new(library_service::Jobs::default()),
        })
    }
    fn lock(&self) -> AppResult<MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(|_| {
            "Der Anwendungszustand ist gesperrt. Gesicherte Sitzungen bleiben erhalten.".into()
        })
    }
    /// Try the recording transition mutex without waiting; contention returns a busy error.
    /// Waits a bounded time for a transition that is known to be ending (a cancelled
    /// reconnect returning its capture).
    fn wait_for_transition(&self) -> AppResult<MutexGuard<'_, ()>> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.try_transition() {
                Ok(guard) => return Ok(guard),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
    fn try_transition(&self) -> AppResult<MutexGuard<'_, ()>> {
        self.transition.try_lock().map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => "A recording transition is running. Try again shortly. / Die Aufnahmesteuerung arbeitet noch. Bitte kurz warten.".into(),
            std::sync::TryLockError::Poisoned(_) => "Recording control is unavailable. Saved data is preserved. / Aufnahmesteuerung ist gesperrt; gesicherte Daten bleiben erhalten.".into(),
        })
    }
    /// Collect finished work under the transition gate, then acquire admission. Exclusive kinds
    /// also reject retained Active slots without leases.
    fn reserve_operation(&self, kind: operations::Kind) -> AppResult<operations::Lease> {
        let _transition = self.try_transition()?;
        self.reap_finished()?;
        let inner = self.lock()?;
        // The active check also covers restored older/test-owned sessions without a lease.
        if inner.transitioning || kind.exclusive() && inner.active.is_some() {
            return Err("Finish the active recording or operation first. / Bitte die laufende Aufnahme oder den Vorgang zuerst abschließen.".into());
        }
        self.operations.try_acquire(kind)
    }
    /// Return a bounded meeting preview with global saved/pending audio totals, capture warnings
    /// and ownership derived from the actual Active slot.
    fn state(&self) -> AppResult<AppView> {
        let inner = self.lock()?;
        let store = inner
            .active
            .as_ref()
            .map(|a| &a.store)
            .or(inner.view.as_ref());
        let active_meeting = if let Some(store) = store {
            let data = store
                .preview_snapshot(16)
                .map_err(|_| "Sitzungsstand konnte nicht gelesen werden.")?;
            let progress = store
                .progress()
                .map_err(|_| "Sicherungsstand konnte nicht gelesen werden.")?;
            let mut view = self.meeting_view(data, false);
            view.live_preview_limited = progress.total_parts > 16;
            view.pending_audio_ms = progress.pending_ms;
            view.saved_audio_ms = SavedAudio {
                system: progress.system_confirmed_ms,
                microphone: progress.microphone_confirmed_ms,
            };
            Some(view)
        } else {
            None
        };
        let recording = inner.reconnecting_microphone
            || inner.active.as_ref().is_some_and(|a| a.capture.is_some());
        let capture_control = inner
            .active
            .as_ref()
            .and_then(|a| a.capture.as_ref())
            .map(CaptureController::controls);
        let capture = inner.active.as_ref().and_then(|a| a.capture.as_ref());
        let capture_failed = capture.is_some_and(|c| !c.is_running());
        let stopped_tracks = capture.map(|c| c.stopped_tracks()).unwrap_or_default();
        let error = if capture_failed {
            Some("Die Audioaufnahme wurde unterbrochen. Bereits gesicherte Daten bleiben erhalten; bitte das Meeting beenden und das Audiogerät prüfen.".into())
        } else if !stopped_tracks.is_empty() {
            let names = stopped_tracks
                .iter()
                .map(|s| {
                    if *s == "microphone" {
                        "Mikrofon / microphone"
                    } else {
                        "Systemaudio / system audio"
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("Eine Audiospur nimmt nicht mehr auf: {names}. Die übrige Spur läuft weiter, gesicherte Daten bleiben erhalten; bitte das Gerät prüfen. / One audio track stopped recording; the other track continues and saved audio is retained."))
        } else {
            inner.error.clone()
        };
        Ok(AppView {
            version: VERSION,
            handy: inner.handy.clone(),
            active_meeting,
            recording,
            capture_control,
            microphone_reconnect_available: inner.active.as_ref().is_some_and(|active| {
                active.owner.is_none()
                    && active
                        .capture
                        .as_ref()
                        .is_some_and(CaptureController::can_reconnect_microphone)
            }),
            error,
            data_directory: self.root.to_string_lossy().into_owned(),
            notes_running: self.notes_jobs.running(),
            transitioning: inner.transitioning,
            agent: self.agent.view_for_capture(
                inner
                    .active
                    .as_ref()
                    .and_then(|active| active.owner.as_deref()),
            )?,
        })
    }
    /// Serialize model discovery while distinguishing OS discovery from native grants. Failed
    /// explicit choices retain their origin instead of reverting to an earlier program.
    fn refresh(
        &self,
        explicit: Option<String>,
    ) -> AppResult<(HandyBridge, ModelCatalog, HandyStatus)> {
        let _refresh = self
            .refresh_gate
            .lock()
            .map_err(|_| "Die Handy-Modellabfrage ist gesperrt.")?;
        let explicitly_selected = explicit.is_some();
        let explicit = explicit.map(PathBuf::from);
        let cached = self.lock()?.executable.clone();
        let path = explicit.clone().or(cached.clone());
        let mut discovered = false;
        let attempt = (|| {
            // A chosen path is recorded below either way, but only a Handy program is started.
            if path.as_deref().is_some_and(|p| !plausible_handy_path(p)) {
                return Err("Bitte die Handy-Programmdatei auswählen (handy, Handy.exe, Handy.app oder eine Handy-AppImage). / Choose the Handy program itself.".to_string());
            }
            let discovery_path = self
                .selections
                .handy_discovery_path(explicit.as_deref(), cached.as_deref())?;
            discovered = discovery_path.is_none();
            let bridge =
                HandyBridge::discover(discovery_path.as_deref()).map_err(|e| e.to_string())?;
            let catalog = bridge.refresh().map_err(|e| e.to_string())?;
            let models = catalog
                .models
                .iter()
                .map(|m| ModelView {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    is_downloaded: m.is_downloaded && !m.is_downloading,
                    supports_streaming: m.supports_streaming,
                    compatibility_error: if m.ready_for_streaming() {
                        bridge
                            .resolve_streaming_model(m)
                            .err()
                            .map(|e| e.to_string())
                    } else if m.is_downloading {
                        Some("Das Modell wird in Handy noch heruntergeladen.".into())
                    } else {
                        None
                    },
                })
                .collect();
            let status = HandyStatus {
                executable: Some(bridge.executable().to_string_lossy().into_owned()),
                version: handy_bridge::installed_version(bridge.executable()),
                models,
                error: None,
            };
            Ok::<_, String>((bridge, catalog, status))
        })();
        let mut inner = self.lock()?;
        if explicitly_selected {
            // An invalid newly selected installation must not quietly fall
            // back to a previously selected Handy executable on the next run.
            inner.executable = path.clone();
        }
        match &attempt {
            Ok((bridge, _, status)) => {
                if discovered {
                    self.selections
                        .remember_discovered_handy(bridge.executable())?;
                }
                inner.executable = Some(bridge.executable().to_owned());
                inner.handy = Some(status.clone());
                inner.error = None;
            }
            Err(error) => {
                inner.handy = Some(HandyStatus {
                    executable: path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    version: None,
                    models: vec![],
                    error: Some(error.clone()),
                });
            }
        }
        attempt
    }
    fn start(&self, request: StartRequest) -> AppResult<MeetingView> {
        self.start_for(request, None)
    }
    /// Validate the active meeting and reject agent-owned or capture-less sessions before
    /// applying pause/mute under the transition guard.
    fn set_capture_control(
        &self,
        meeting_id: &str,
        paused: Option<bool>,
        muted: Option<bool>,
    ) -> AppResult<AppView> {
        let _transition = self.try_transition()?;
        {
            let inner = self.lock()?;
            let active = inner
                .active
                .as_ref()
                .ok_or("There is no active recording. / Es läuft keine Aufnahme.")?;
            let current = active
                .store
                .preview_snapshot(0)
                .map_err(|_| "The active session cannot be read")?;
            if current.id != meeting_id {
                return Err("This recording is no longer active; reload its state. / Diese Aufnahme ist nicht mehr aktiv. Bitte neu laden.".into());
            }
            if active.owner.is_some() {
                return Err("Revoke agent recording control before using manual pause or mute. / Bitte die Agenten-Aufnahmesteuerung vor manueller Pause oder Stummschaltung widerrufen.".into());
            }
            let capture = active.capture.as_ref().ok_or("Audio has ended; only recognition remains. / Audioaufnahme ist beendet; die Erkennung wird nachgeholt.")?;
            if let Some(paused) = paused {
                capture
                    .set_paused(paused)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(muted) = muted {
                capture
                    .set_microphone_muted(muted)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.state()
    }

    /// Reopens only the originally selected mic under the recording transition guard.
    /// Device initialization runs outside the state lock; ordinary failure restores the
    /// controller with its system track and operation lease still owned.
    fn reconnect_microphone(&self, meeting_id: &str) -> AppResult<AppView> {
        let transition = self.try_transition()?;
        let mut capture = {
            let mut inner = self.lock()?;
            let active = inner
                .active
                .as_mut()
                .ok_or("Es läuft keine Aufnahme. / No recording is active.")?;
            if active
                .store
                .preview_snapshot(0)
                .map_err(|_| "Sitzung nicht lesbar")?
                .id
                != meeting_id
            {
                return Err(
                    "Diese Aufnahme ist nicht mehr aktiv. / This recording is no longer active."
                        .into(),
                );
            }
            if active.owner.is_some() {
                return Err("Zuerst die Agenten-Aufnahmesteuerung widerrufen. / Revoke agent recording control first.".into());
            }
            let capture = active
                .capture
                .take()
                .ok_or("Audioaufnahme ist beendet. / Audio capture has ended.")?;
            inner.transitioning = true;
            inner.reconnecting_microphone = true;
            inner.reconnect_cancel = Some(capture.reconnect_canceller());
            capture
        };
        let result = capture.reconnect_microphone().map_err(|error| format!("Mikrofon konnte nicht verbunden werden. Das ursprünglich gewählte Gerät prüfen und erneut versuchen. Computerton und gesicherte Daten bleiben erhalten. / Could not reconnect the original microphone. Check the device and retry; system audio and saved data are retained. ({error})"));
        let restored = {
            let mut inner = self.lock()?;
            // The transition guard excludes stop/start/revoke until ownership is restored.
            let restored = match inner.active.as_mut() {
                Some(active) => {
                    active.capture = Some(capture);
                    Ok(())
                }
                None => Err("Recording ownership unavailable".to_string()),
            };
            inner.transitioning = false;
            restored
        };
        // Release the transition before the canceller: a stop that saw the canceller waits
        // for the transition, so it can never find neither a canceller nor a free transition.
        drop(transition);
        {
            let mut inner = self.lock()?;
            inner.reconnecting_microphone = false;
            inner.reconnect_cancel = None;
        }
        restored?;
        result?;
        self.state()
    }
    /// Serialize recording start, check supplied agent approval and transfer admission to Active.
    /// Ordinary errors clear the transition and reach the UI.
    fn start_for(&self, request: StartRequest, owner: Option<String>) -> AppResult<MeetingView> {
        let _gate = self.try_transition()?;
        self.reap_finished()?;
        if let Some(owner) = owner.as_deref() {
            self.agent.start_permitted(owner)?;
        }
        let lease = self.operations.try_acquire(operations::Kind::Recording)?;
        {
            let mut inner = self.lock()?;
            if inner.active.is_some() || inner.transitioning {
                return Err(
                    "Es läuft bereits eine Aufnahme oder Wiederaufnahme. Bitte zuerst abschließen."
                        .into(),
                );
            }
            inner.transitioning = true;
        }
        let result = self.start_inner(request, owner, lease);
        let mut inner = self.lock()?;
        inner.transitioning = false;
        if let Err(error) = &result {
            inner.error = Some(error.clone());
        }
        result
    }
    /// Resolve model/devices and persist provenance before durable capture; start isolated ASR
    /// afterward and retain the recording lease in Active.
    fn start_inner(
        &self,
        request: StartRequest,
        owner: Option<String>,
        lease: operations::Lease,
    ) -> AppResult<MeetingView> {
        if request.title.chars().count() > 200 || request.model_id.len() > 4096 {
            return Err("Titel oder Modellkennung ist zu lang.".into());
        }
        let (bridge, catalog, _) = self.refresh(None)?;
        let model = catalog
            .model(&request.model_id)
            .map_err(|e| e.to_string())?;
        let resolved = bridge
            .resolve_streaming_model(model)
            .map_err(|e| e.to_string())?;
        let devices = meeting_core::list_devices()
            .map_err(|_| "Audiogeräte konnten nicht gelesen werden. Geräteverbindung prüfen.")?;
        let system = devices
            .iter()
            .find(|d| d.source == "system" && d.id == request.system_device_id)
            .ok_or("Bitte eine verfügbare Computerton-Ausgabe auswählen.")?;
        let microphone = request
            .microphone_device_id
            .as_ref()
            .map(|id| {
                devices
                    .iter()
                    .find(|d| d.source == "microphone" && &d.id == id)
                    .ok_or("Das gewählte Mikrofon ist nicht verfügbar.")
            })
            .transpose()?;
        let backend = backend_directory()?;
        let store=SessionStore::create(&self.root.join("sessions"),&request.title,&model.id,&system.name,microphone.map(|m|m.name.as_str())).map_err(|_|"Die Sitzung konnte nicht angelegt werden. Freien Speicherplatz und Ordnerrechte prüfen.")?;
        let provenance = serde_json::json!({"applicationVersion":VERSION,"handyInstallation":catalog.installation,"catalogFingerprint":catalog.fingerprint_sha256,"catalogObservedAtUnixMs":catalog.fetched_at_unix_ms,"model":model,"resolvedModel":resolved,"nativeRuntime":handy_bridge::native::runtime_identity(),"capabilityPolicy":"metadata-before-capture; actual-streaming-capability-in-isolated-worker","trackInference":"one-isolated-model-per-track"});
        handy_bridge::write_model_snapshot(store.folder(), model, &resolved, &provenance)
            .map_err(|error| error.to_string())?;
        // Begin durable audio before any model load. Worker failures never own capture.
        let capture = self.agent.capture_if_permitted(owner.as_deref(), || {
            CaptureController::start(store.clone(), CaptureOptions {
                output_id: Some(system.id.clone()),
                microphone_id: microphone.map(|m|m.id.clone()),
                include_microphone: microphone.is_some(),
            }).map_err(|_| {
                let _=store.warn("Aufnahme konnte nicht vollständig gestartet werden. Gesicherte Daten bleiben erhalten; Audiogerät und Berechtigungen prüfen.");
                "Audioaufnahme konnte nicht gestartet werden. Audiogerät und Berechtigungen prüfen.".to_string()
            })
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = vec![asr::spawn_track(
            store.clone(),
            resolved.path.clone(),
            backend.clone(),
            "system".into(),
            stop.clone(),
        )];
        if microphone.is_some() {
            workers.push(asr::spawn_track(
                store.clone(),
                resolved.path,
                backend,
                "microphone".into(),
                stop.clone(),
            ));
        }
        let data = store
            .preview_snapshot(16)
            .map_err(|_| "Sitzungsstand nicht lesbar.")?;
        let mut inner = self.lock()?;
        inner.error = None;
        inner.active = Some(Active {
            _operation: Some(lease),
            owner,
            store,
            capture: Some(capture),
            stop,
            workers,
        });
        Ok(self.meeting_view(data, true))
    }
    fn stop(&self) -> AppResult<MeetingView> {
        self.stop_for(None, None)
    }
    /// Stopping must not wait for a slow microphone: end a running reconnect and wait until
    /// it has returned the capture, so the regular stop below closes every track. Returns
    /// whether a reconnect was running.
    fn cancel_microphone_reconnect(&self) -> AppResult<bool> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut cancelled = false;
        loop {
            match &self.lock()?.reconnect_cancel {
                None => return Ok(cancelled),
                Some(cancel) => {
                    cancel.store(true, Ordering::Release);
                    cancelled = true;
                }
            }
            if Instant::now() >= deadline {
                return Err("Das Mikrofon wird noch verbunden; bitte erneut „Meeting beenden“ wählen. / The microphone is still reconnecting; choose Stop meeting again.".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// Check optional owner/session constraints before closing audio; retain unfinished
    /// recognition as capture-less catch-up instead of waiting for every worker.
    fn stop_for(&self, owner: Option<&str>, session: Option<&str>) -> AppResult<MeetingView> {
        // Only a manual recording can be reconnecting; an agent never stops it from here.
        // A cancelled reconnect still holds the transition briefly while it returns; wait
        // for it instead of refusing the stop.
        let _gate = if owner.is_none() && self.cancel_microphone_reconnect()? {
            self.wait_for_transition()?
        } else {
            self.try_transition()?
        };
        let mut active = {
            let mut inner = self.lock()?;
            if owner.is_some()
                && inner
                    .active
                    .as_ref()
                    .is_none_or(|active| active.owner.as_deref() != owner)
            {
                return Err("This agent does not own the active recording".into());
            }
            if session.is_some()
                && !inner.active.as_ref().is_some_and(|active| {
                    active
                        .store
                        .preview_snapshot(0)
                        .is_ok_and(|data| Some(data.id.as_str()) == session)
                })
            {
                return Err("The requested recording is no longer active".into());
            }
            inner.transitioning = true;
            match inner.active.take() {
                Some(active) => active,
                None => {
                    inner.transitioning = false;
                    return Err("Es läuft keine Aufnahme oder Wiederaufnahme.".into());
                }
            }
        };
        self.lock()?.view = Some(active.store.clone());
        let capture_result = active
            .capture
            .as_mut()
            .map(CaptureController::stop)
            .transpose();
        active.capture = None;
        if active.owner.is_some() {
            self.agent.capture_ended();
        }
        active.stop.store(true, Ordering::Release);
        // Audio is final once capture has stopped. Recognition of the remaining sections can take
        // minutes on slow hardware, so it continues as a capture-less catch-up (like `resume`)
        // instead of holding the transition; `reap_finished` collects it later.
        let catching_up = active
            .workers
            .iter()
            .any(|worker| !worker.handle.is_finished());
        if catching_up {
            if capture_result.is_ok() {
                let _ = active.store.set_status("catching-up");
            }
        } else {
            for worker in active.workers.drain(..) {
                if worker.handle.join().is_err() {
                    let _=active.store.notice("Ein Erkennungsprozess wurde unterbrochen. Offene Abschnitte können später nachgeholt werden.");
                }
            }
            if let Ok(data) = active.store.snapshot() {
                if library_service::update(&self.root, &data).is_err() {
                    let _ = active.store.notice("Die Aufnahme ist gespeichert; bitte den lokalen Suchindex unter Einstellungen aktualisieren. / Recording saved; refresh the local search index in Settings.");
                }
            }
        }
        let result: AppResult<MeetingView> = if capture_result.is_err() {
            Err("Die Aufnahme wurde mit einer Sicherungswarnung beendet. Bitte den Sitzungsstand prüfen.".into())
        } else {
            active
                .store
                .snapshot()
                .map(|d| self.meeting_view(d, false))
                .map_err(|_| "Der gespeicherte Sitzungsstand konnte nicht gelesen werden.".into())
        };
        let mut inner = self.lock()?;
        if catching_up {
            inner.active = Some(active);
        }
        inner.transitioning = false;
        if let Err(error) = &result {
            inner.error = Some(error.clone());
        }
        result
    }
    /// Polling cleanup. It runs only while no start, stop or resume holds the transition
    /// gate, so a session set aside for settling cannot race a new recording into the
    /// active slot; those transitions collect finished work themselves.
    fn reap_when_idle(&self) -> AppResult<()> {
        match self.transition.try_lock() {
            Ok(_gate) => self.reap_finished(),
            Err(std::sync::TryLockError::WouldBlock) => Ok(()),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err("Aufnahmesteuerung ist gesperrt.".into())
            }
        }
    }
    /// Joins ended recognition owners after capture ends, retaining failed status writes
    /// for retry and keeping unfinished audio available for explicit recognition resumption.
    fn reap_finished(&self) -> AppResult<()> {
        let finished = {
            let mut inner = self.lock()?;
            if inner.active.as_ref().is_some_and(|a| {
                a.capture.is_none() && a.workers.iter().all(|worker| worker.handle.is_finished())
            }) {
                inner.active.take()
            } else {
                None
            }
        };
        if let Some(mut active) = finished {
            let mut interrupted = false;
            for worker in active.workers.drain(..) {
                interrupted |= worker.handle.join().is_err();
            }
            self.lock()?.view = Some(active.store.clone());
            if interrupted {
                let _ = active.store.notice("Ein Erkennungsprozess wurde unterbrochen. Offene Abschnitte können später nachgeholt werden.");
            }
            if let Err(error) = Self::settle_catch_up(&active.store) {
                // The workers are joined; keep the session so the next poll retries the
                // status write instead of leaving it "catching-up". Callers hold the
                // transition gate, so no new session can have taken the slot meanwhile;
                // a session that did is never replaced.
                let mut inner = self.lock()?;
                if inner.active.is_none() {
                    inner.active = Some(active);
                }
                return Err(error);
            }
            if let Ok(data) = active.store.snapshot() {
                if library_service::update(&self.root, &data).is_err() {
                    let _ = active.store.notice("Die Aufnahme ist gespeichert; bitte den lokalen Suchindex unter Einstellungen aktualisieren. / Recording saved; refresh the local search index in Settings.");
                }
            }
        }
        Ok(())
    }
    /// Records the end of a session whose recognition workers are all finished.
    fn settle_catch_up(store: &SessionStore) -> AppResult<()> {
        if !Self::finalize_completed_recognition(store)? {
            // No worker remains: a catch-up that ended with open sections must not stay
            // "catching-up", which blocks new recordings and the resume action.
            let caught_up = store
                .preview_snapshot(0)
                .is_ok_and(|data| data.status == "catching-up");
            if caught_up {
                store
                    .set_status("recorded")
                    .map_err(|_| "Sitzungsstatus konnte nicht gesichert werden.")?;
                let _ = store.notice("Die Erkennung wurde ohne vollständiges Ergebnis beendet. Audio ist gespeichert; offene Abschnitte können über Wiederaufnehmen nachgeholt werden. / Recognition ended with open sections. Audio is saved; resume to process them.");
            }
        }
        Ok(())
    }
    // A host can stop after the last durable final text checkpoint but before
    // its worker updates the global status. No model call is needed to finish
    // that already-confirmed state, and failed/open/foreign-model parts must
    // not be hidden by a vacuously empty worker list.
    fn finalize_completed_recognition(store: &SessionStore) -> AppResult<bool> {
        let status = store
            .preview_snapshot(0)
            .map_err(|_| "Sitzungsstand konnte nicht gelesen werden.")?
            .status;
        if !["recorded", "catching-up", "complete"].contains(&status.as_str())
            || !store
                .recognition_complete()
                .map_err(|_| "Erkennungsabschluss konnte nicht geprüft werden.")?
        {
            return Ok(false);
        }
        if status != "complete" {
            store.set_status("complete").map_err(|_| "Der bestätigte Erkennungsabschluss konnte nicht gesichert werden. Vorhandene Audio- und Textstände bleiben erhalten.")?;
        }
        Ok(true)
    }
    /// Resume writable Rust sessions using their original model identity and weights;
    /// already-confirmed final text can settle without loading a model.
    fn resume(&self, id: String) -> AppResult<MeetingView> {
        let _gate = self.try_transition()?;
        self.reap_finished()?;
        let unavailable = {
            let inner = self.lock()?;
            inner.active.is_some() || inner.transitioning
        };
        if unavailable {
            return Err("Bitte zuerst die laufende Sitzung abschließen.".into());
        }
        let lease = self.operations.try_acquire(operations::Kind::Recording)?;
        if valid_legacy_id(&id) {
            return Err("Originalsitzungen werden in dieser Beta ausschließlich gelesen. Zum Nachholen die bisherige Version verwenden.".into());
        }
        let data = self.data(&id)?;
        if data.read_only {
            return Err("Audio ist lokal archiviert. Bitte zuerst Audio wiederherstellen oder eine neue Transkription aus einer Archivkopie anlegen. / Audio is archived locally. Restore audio or create a new transcription from an archive copy first.".into());
        }
        let store = self
            .lock()?
            .view
            .as_ref()
            .filter(|s| s.preview_snapshot(0).is_ok_and(|current| current.id == id))
            .cloned()
            .ok_or("Sitzung konnte nicht wiederaufgenommen werden.")?;
        if Self::finalize_completed_recognition(&store)? {
            return Ok(self.meeting_view(
                store
                    .snapshot()
                    .map_err(|_| "Sitzungsstand konnte nicht gelesen werden.")?,
                false,
            ));
        }
        let (_bridge, catalog, _) = self.refresh(None)?;
        let model=catalog.model(&data.model_id).map_err(|_|"Das ursprüngliche Modell ist in Handy nicht verfügbar. Es wird kein anderes Modell eingesetzt.")?;
        let snapshot_folder = self.root.join("sessions").join(&id);
        let identity =
            handy_bridge::read_model_snapshot(&snapshot_folder).map_err(|e| e.to_string())?;
        handy_bridge::verify_catalog_model(&identity, model).map_err(|e| e.to_string())?;
        handy_bridge::verify_model_snapshot(&snapshot_folder, &identity.canonical_path)
            .map_err(|e| e.to_string())?;
        // Resume pins the actual weights captured at recording start. It never
        // resolves an updated Handy catalog entry to replacement weights.
        let pinned_model_path = identity.canonical_path;
        let backend = backend_directory()?;
        store
            .set_status("catching-up")
            .map_err(|_| "Sitzungsstatus konnte nicht gesichert werden.")?;
        let stop = Arc::new(AtomicBool::new(true));
        let workers = ["system", "microphone"]
            .into_iter()
            .filter(|source| {
                data.parts
                    .iter()
                    .any(|p| p.source == *source && p.status != "complete")
            })
            .map(|source| {
                asr::spawn_track(
                    store.clone(),
                    pinned_model_path.clone(),
                    backend.clone(),
                    source.into(),
                    stop.clone(),
                )
            })
            .collect();
        self.lock()?.active = Some(Active {
            _operation: Some(lease),
            owner: None,
            store: store.clone(),
            capture: None,
            stop,
            workers,
        });
        Ok(self.meeting_view(
            store
                .snapshot()
                .map_err(|_| "Sitzungsstand nicht lesbar.")?,
            false,
        ))
    }
    fn analyze(&self, request: NotesRequest) -> AppResult<meeting_notes::Analysis> {
        notes_service::analyze(self, request)
    }
}
fn note_sources(data: &SessionData) -> AppResult<Vec<meeting_notes::Source>> {
    let mut parts: Vec<_> = data.parts.iter().collect();
    parts.sort_by(|a, b| (a.start_sample, &a.source).cmp(&(b.start_sample, &b.source)));
    let mut sources = vec![];
    for part in parts {
        if part.status != "complete" {
            return Err("Unvollständige Transkriptabschnitte dürfen nicht als abgeschlossene Quelle ausgewertet werden.".into());
        }
        if let Some(text) = &part.text {
            let chars: Vec<_> = text.chars().collect();
            for chunk in chars.chunks(1600) {
                let value: String = chunk.iter().collect();
                if value.trim().is_empty() {
                    continue;
                }
                sources.push(meeting_notes::Source {
                    id: format!("S{:05}", sources.len() + 1),
                    source: part.source.clone(),
                    start_ms: part.start_sample * 1000 / u64::from(meeting_core::SAMPLE_RATE),
                    end_ms: (part.start_sample + part.sample_count) * 1000
                        / u64::from(meeting_core::SAMPLE_RATE),
                    text: value,
                });
            }
        }
    }
    if sources.is_empty() {
        return Err("Das abgeschlossene Transkript enthält noch keinen auswertbaren Text.".into());
    }
    Ok(sources)
}
fn validate_id(id: &str) -> AppResult<()> {
    if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Ungültige Sitzungskennung.".into());
    }
    Ok(())
}
/// Defence in depth for an executable path that arrives through IPC: only something named like
/// the Handy program (or its macOS bundle) is ever started for model discovery.
fn plausible_handy_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name.starts_with("handy")
        && (name == "handy"
            || name.ends_with(".exe")
            || name.ends_with(".app")
            || name.ends_with(".appimage")
            || !name.contains('.'))
}
fn valid_legacy_id(id: &str) -> bool {
    id.strip_prefix("legacy-")
        .is_some_and(|s| s.len() == 16 && s.bytes().all(|c| c.is_ascii_hexdigit()))
}
fn create_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
    let mut file=OpenOptions::new().create_new(true).write(true).open(path).map_err(|_|"Ein neuer Arbeitsstand konnte nicht angelegt werden. Vorhandene Dateien bleiben erhalten.")?;
    file.write_all(
        &serde_json::to_vec_pretty(value)
            .map_err(|_| "Arbeitsstand konnte nicht serialisiert werden.")?,
    )
    .and_then(|_| file.sync_all())
    .map_err(|_| "Arbeitsstand konnte nicht dauerhaft gesichert werden.".into())
}
fn backend_directory() -> AppResult<PathBuf> {
    if let Some(value) = std::env::var_os("HANDYMEETING_NATIVE_DIR") {
        let path = PathBuf::from(value);
        if path.is_dir() {
            return Ok(path);
        }
        return Err("Der konfigurierte Erkennungsordner wurde nicht gefunden.".into());
    }
    let executable = std::env::current_exe().map_err(|_| "Programmordner ist nicht verfügbar.")?;
    let directory = executable
        .parent()
        .ok_or("Programmordner ist nicht verfügbar.")?;
    if directory.join("native").is_dir() {
        Ok(directory.join("native"))
    } else {
        Ok(directory.to_owned())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartRequest {
    title: String,
    model_id: String,
    system_device_id: String,
    microphone_device_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotesRequest {
    #[serde(default)]
    use_saved_key: bool,
    #[serde(default)]
    job_id: Option<String>,
    meeting_id: String,
    template: meeting_notes::Template,
    endpoint: String,
    model: String,
    mode: String,
    cloud_consent: bool,
    api_key: String,
    #[serde(default)]
    resume_run_id: Option<String>,
}
async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| {
            "Der Arbeitsschritt wurde unterbrochen. Bereits gesicherte Daten bleiben erhalten."
                .to_string()
        })?
}

/// Initialize the desktop service, native-choice plugin and narrow IPC handlers. Startup workers
/// remain separate from capture and window-close guards.
pub fn run() {
    let root = std::env::var_os("HANDYMEETING_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|base| base.join("HandyMeeting/RustBeta")))
        .expect("Lokaler Anwendungsordner ist nicht verfügbar.");
    let service =
        Arc::new(Service::new(root).expect("Lokale Sitzungsablage konnte nicht geöffnet werden."));
    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).manage(service).setup(|app| {
        let service=Arc::clone(app.state::<Arc<Service>>().inner());
        if let Ok(lease)=service.reserve_operation(operations::Kind::Index){let _=service.library_jobs.start_scan(service.root.clone(), false, Some(lease));}
        if std::env::args().skip(1).eq(["--agent", "serve"]) { agent::start_stdio(Arc::clone(&service)).map_err(std::io::Error::other)?; }
        tauri::async_runtime::spawn_blocking(move|| {let _=service.refresh(None);});
        Ok(())
    }).invoke_handler(tauri::generate_handler![path_selection::choose_handy_executable,path_selection::choose_ffmpeg_executable,path_selection::choose_legacy_manifest,app_commands::get_app_state,app_commands::get_audio_levels,app_commands::get_system_load,retention_commands::list_audio_copies,retention_commands::transcribe_audio_copy,retention_commands::compact_archive,retention_commands::list_archives,retention_commands::list_unfinished_deletions,retention_commands::resume_permanent_deletion,retention_commands::restore_archive,retention_commands::apply_retention,retention_commands::get_retention_policy,retention_commands::save_retention_policy,retention_commands::preview_retention,retention_commands::get_retention_plan,retention_commands::get_maintenance_progress,retention_commands::cancel_maintenance,library_commands::edit_meeting_metadata,library_commands::search_meetings,library_commands::rebuild_search_index,library_commands::get_library_progress,library_commands::cancel_library_job,speaker_commands::archive_speaker_model_checks,speaker_commands::check_speaker_model,speaker_commands::get_speaker_labels,speaker_commands::save_speaker_labels,speaker_commands::start_speaker_experiment,speaker_commands::get_speaker_progress,speaker_commands::get_speaker_sections,speaker_commands::cancel_speaker_experiment,update_commands::check_for_update,update_commands::download_update,update_commands::get_update_progress,update_commands::cancel_update,update_commands::install_update,update_commands::open_release_page,app_commands::refresh_handy,app_commands::list_devices,app_commands::start_meeting,app_commands::stop_meeting,app_commands::set_capture_paused,app_commands::set_microphone_muted,app_commands::reconnect_microphone,app_commands::agent_decide_recording,app_commands::agent_revoke,app_commands::list_meetings,app_commands::get_meeting,app_commands::resume_meeting,app_commands::import_legacy,app_commands::copy_text,app_commands::open_meeting_folder,app_commands::export_transcript,app_commands::generate_notes,app_commands::list_analyses,notes_commands::list_unfinished_analyses,notes_commands::list_text_models,notes_commands::get_notes_progress,notes_commands::cancel_notes,notes_commands::notes_markdown,live_commands::start_live_notes,live_commands::stop_live_notes,live_commands::refresh_live_notes,live_commands::set_live_notes_interval,live_commands::get_live_notes,live_commands::local_model_setup,live_commands::pull_ollama_model,live_commands::get_model_pull_progress,live_commands::cancel_model_pull,template_commands::list_templates,template_commands::save_templates,template_commands::import_templates,credential_commands::credential_status,credential_commands::save_credential,credential_commands::remove_credential])
    .on_window_event(|window,event|{if let tauri::WindowEvent::CloseRequested{api,..}=event{let service=window.state::<Arc<Service>>();let blocked=service.lock().is_ok_and(|inner|inner.transitioning||inner.active.as_ref().is_some_and(|a|a.capture.is_some()));if blocked{api.prevent_close();if let Ok(mut inner)=service.lock(){inner.error=Some(if inner.transitioning { "Ein Vorgang läuft noch. Bitte warten, bis Audioabschluss und Verarbeitung beendet sind.".into() } else { "Bitte zuerst „Meeting beenden“ wählen. Audio wird gesichert und offene Abschnitte werden verarbeitet.".into() });}let _=window.emit("recording-close-blocked",());}}})
    .run(tauri::generate_context!()).expect("HandyMeeting konnte nicht gestartet werden.");
}
/// Expose explicit diagnostic modes without creating a GUI when the dispatcher handles the
/// supplied arguments.
pub fn run_cli(args: &[String]) -> Option<AppResult<()>> {
    cli::run_cli(args)
}

#[cfg(test)]
mod service_tests;
