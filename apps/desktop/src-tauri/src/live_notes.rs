//! Live notes worker: reads only the confirmed transcript that the UI also reads and sends text
//! to the chosen model. It never holds the recording transition, a capture handle or audio.
use crate::{valid_legacy_id, validate_id, AppResult, Service};
use meeting_core::SessionData;
use meeting_notes::live::{self, LiveRunner, LiveText, LiveView, Trigger};
use meeting_notes::AnalysisControl;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// How often the worker looks for newly confirmed text.
const POLL: Duration = Duration::from_millis(2000);
/// A failed request is retried after this pause, doubling up to `MAX_BACKOFF`.
const FIRST_BACKOFF: Duration = Duration::from_secs(20);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// Without new text, a missing overview is retried at most this often.
const OVERVIEW_RETRY: Duration = Duration::from_secs(60);
/// After the meeting ended, a failing overview is attempted this often before the worker ends.
const MAX_OVERVIEW_FAILURES: u32 = 3;
/// Shown while live notes yield to recognition for more than two minutes.
const RECOGNITION_WAIT: &str = "Die Live-Auswertung wartet seit mehr als zwei Minuten auf die Erkennung. Aufnahme und gespeicherte Daten bleiben erhalten. Erkennungsfortschritt prüfen; die Live-Auswertung kann beendet und später erneuert werden. / Live notes have waited over two minutes for recognition. Recording and saved data are retained; check recognition progress or stop and retry live notes later.";
pub(crate) const DEFAULT_INTERVAL_MS: u64 = 3 * 60_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveNotesRequest {
    meeting_id: String,
    endpoint: String,
    model: String,
    mode: String,
    #[serde(default)]
    cloud_consent: bool,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    use_saved_key: bool,
    #[serde(default)]
    interval_ms: Option<u64>,
    language: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveNotesStatus {
    meeting_id: String,
    /// off | starting | waiting | updating | finished | partial | stopped | failed
    status: String,
    running: bool,
    waiting_for_transcript: bool,
    error: Option<String>,
    pending_chars: usize,
    interval_ms: u64,
    next_update_in_ms: Option<u64>,
    last_trigger: Option<Trigger>,
    view: Option<LiveView>,
}

struct Shared {
    status: String,
    waiting_for_transcript: bool,
    error: Option<String>,
    pending_chars: usize,
    last_update: Instant,
    last_trigger: Option<Trigger>,
    retry_at: Option<Instant>,
    view: Option<LiveView>,
}

struct Job {
    meeting_id: String,
    control: AnalysisControl,
    manual: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
    shared: Arc<Mutex<Shared>>,
    worker: Option<thread::JoinHandle<()>>,
    models: crate::model_sessions::Models,
}
impl Job {
    fn running(&self) -> bool {
        self.shared
            .lock()
            .is_ok_and(|s| ["starting", "waiting", "updating"].contains(&s.status.as_str()))
    }
    /// Reads one job's scalar progress and retained view. Terminal jobs never advertise
    /// a pending transcript wait, and retry backoff can only postpone the next update.
    fn status(&self) -> LiveNotesStatus {
        let interval_ms = self.interval_ms.load(Ordering::Acquire);
        let shared = self.shared.lock();
        let Ok(shared) = shared else {
            return off(
                &self.meeting_id,
                None,
                Some("Live-Zusammenfassung ist gesperrt.".into()),
            );
        };
        let running = ["starting", "waiting", "updating"].contains(&shared.status.as_str());
        let next_update_in_ms = (running && shared.status == "waiting").then(|| {
            let elapsed = shared.last_update.elapsed().as_millis() as u64;
            let due = interval_ms.saturating_sub(elapsed);
            shared.retry_at.map_or(due, |at| {
                due.max(at.saturating_duration_since(Instant::now()).as_millis() as u64)
            })
        });
        LiveNotesStatus {
            meeting_id: self.meeting_id.clone(),
            status: shared.status.clone(),
            running,
            waiting_for_transcript: running && shared.waiting_for_transcript,
            error: shared.error.clone(),
            pending_chars: shared.pending_chars,
            interval_ms,
            next_update_in_ms,
            last_trigger: shared.last_trigger,
            view: shared.view.clone(),
        }
    }
}

fn off(meeting_id: &str, view: Option<LiveView>, error: Option<String>) -> LiveNotesStatus {
    LiveNotesStatus {
        meeting_id: meeting_id.into(),
        status: "off".into(),
        running: false,
        waiting_for_transcript: false,
        error,
        pending_chars: 0,
        interval_ms: DEFAULT_INTERVAL_MS,
        next_update_in_ms: None,
        last_trigger: None,
        view,
    }
}

#[derive(Default)]
pub(crate) struct LiveNotes(Mutex<Option<Job>>);

fn clamp_interval(ms: Option<u64>) -> u64 {
    ms.unwrap_or(DEFAULT_INTERVAL_MS)
        .clamp(live::MIN_INTERVAL_MS, live::MAX_INTERVAL_MS)
}

fn live_root(service: &Service, meeting_id: &str) -> std::path::PathBuf {
    service
        .root
        .join("sessions")
        .join(meeting_id)
        .join("live-notes")
}

/// Confirmed text of complete parts and whether the saved status alone says that no more text
/// follows. Recognition can still run in the active slot after the status left "recording";
/// [`Snapshot::finished`] also checks that.
pub(crate) fn live_texts(data: &SessionData) -> (Vec<LiveText>, bool) {
    let texts = data
        .parts
        .iter()
        .filter(|p| p.status == "complete")
        .filter_map(|p| {
            let text = p.text.as_ref()?;
            let rate = u64::from(meeting_core::SAMPLE_RATE);
            Some(LiveText {
                key: p.file.clone(),
                track: p.source.clone(),
                start_ms: p.start_sample * 1000 / rate,
                end_ms: (p.start_sample + p.sample_count) * 1000 / rate,
                text: text.clone(),
            })
        })
        .collect();
    let active = ["recording", "streaming", "catching-up"].contains(&data.status.as_str());
    (texts, !active)
}

/// One meeting state as seen by the live worker.
pub(crate) struct Snapshot {
    data: SessionData,
    /// The meeting still owns the active slot, so capture or recognition may add text.
    active: bool,
    /// Only living recognizers can consume compute for their own pending track.
    recognizing_tracks: [bool; 2],
}
impl Snapshot {
    /// A living recognizer owns the compute budget while its track has over five seconds
    /// unprocessed. Completed same-model parts, ended workers and archived meetings never
    /// defer confirmed-text analysis, including after a resume with older progress counters.
    fn recognition_needs_priority(&self) -> bool {
        if !self.active {
            return false;
        }
        let mut tracks = [0_u64; 2];
        for part in &self.data.parts {
            let same_model = part.model_id.as_deref() == Some(self.data.model_id.as_str());
            let pending = if same_model && part.status == "complete" {
                0
            } else if same_model {
                part.confirmed_sample_count
                    .saturating_sub(part.processed_samples)
            } else {
                part.confirmed_sample_count
            };
            let index = usize::from(part.source == "microphone");
            if self.recognizing_tracks[index] {
                tracks[index] = tracks[index].saturating_add(pending);
            }
        }
        tracks.into_iter().max().unwrap_or(0) > u64::from(meeting_core::SAMPLE_RATE) * 5
    }
    /// No more confirmed text can follow: the status says so and no recognition still runs.
    fn finished(&self) -> bool {
        !self.active && live_texts(&self.data).1
    }
}

impl Service {
    /// A snapshot of one meeting for the live worker. It prefers the stores the application
    /// already holds. A meeting that left both slots has finished; its last snapshot is reused
    /// instead of reopening (and re-verifying) the session on every poll. Only without such a
    /// snapshot is the session opened once, without replacing the store the user is viewing.
    fn live_snapshot(&self, id: &str, finished: Option<&SessionData>) -> AppResult<Snapshot> {
        // Only the store handles are taken under the application lock; the snapshot itself
        // is read afterwards, so a large meeting never blocks the audio-level poll.
        let (active_store, view_store) = {
            let inner = self.lock()?;
            (
                inner.active.as_ref().map(|a| {
                    let mut recognizing_tracks = [false; 2];
                    for worker in &a.workers {
                        if !worker.handle.is_finished() {
                            match worker.source.as_str() {
                                "system" => recognizing_tracks[0] = true,
                                "microphone" => recognizing_tracks[1] = true,
                                _ => {}
                            }
                        }
                    }
                    (a.store.clone(), recognizing_tracks)
                }),
                inner.view.clone(),
            )
        };
        if let Some((store, recognizing_tracks)) = active_store {
            if let Ok(data) = store.snapshot() {
                if data.id == id {
                    return Ok(Snapshot {
                        data,
                        active: true,
                        recognizing_tracks,
                    });
                }
            }
        }
        if let Some(store) = view_store {
            if let Ok(data) = store.snapshot() {
                if data.id == id {
                    return Ok(Snapshot {
                        data,
                        active: false,
                        recognizing_tracks: [false; 2],
                    });
                }
            }
        }
        if let Some(data) = finished {
            return Ok(Snapshot {
                data: data.clone(),
                active: false,
                recognizing_tracks: [false; 2],
            });
        }
        let folder = self.root.join("sessions").join(id);
        let store = meeting_core::SessionStore::open(&folder)
            .map_err(|_| "Die Sitzung ist gerade nicht lesbar.")?;
        let data = store
            .snapshot()
            .map_err(|_| "Sitzungsstand konnte nicht gelesen werden.")?;
        if data.id != id {
            return Err("Sitzungskennung und Ordner stimmen nicht überein.".into());
        }
        Ok(Snapshot {
            data,
            active: false,
            recognizing_tracks: [false; 2],
        })
    }
}

impl LiveNotes {
    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Option<Job>>> {
        self.0
            .lock()
            .map_err(|_| "Live-Zusammenfassung ist gesperrt.".into())
    }

    /// Checks inactivity and releases the matching meeting atomically with respect to start.
    /// The caller has already verified complete coverage of the current transcript.
    pub(crate) fn finish_inactive_model(
        &self,
        meeting: &str,
        models: &crate::model_sessions::Models,
    ) {
        let Ok(guard) = self.lock() else {
            return;
        };
        if guard
            .as_ref()
            .is_some_and(|job| job.meeting_id == meeting && job.running())
        {
            return;
        }
        let cleanup = models.detach_owner(&crate::model_sessions::Owner::Live(meeting.to_owned()));
        drop(guard);
        cleanup();
    }

    pub(crate) fn status(&self, service: &Service, meeting_id: &str) -> AppResult<LiveNotesStatus> {
        validate_id(meeting_id)?;
        if let Some(job) = self.lock()?.as_ref().filter(|j| j.meeting_id == meeting_id) {
            return Ok(job.status());
        }
        match live::read_latest(&live_root(service, meeting_id), meeting_id) {
            Ok(view) => Ok(off(meeting_id, view, None)),
            Err(_) => Ok(off(
                meeting_id,
                None,
                Some("Gespeicherte Live-Zusammenfassungen konnten nicht gelesen werden. Sie bleiben unverändert erhalten.".into()),
            )),
        }
    }

    pub(crate) fn refresh(&self) -> AppResult<bool> {
        Ok(self
            .lock()?
            .as_ref()
            .filter(|j| j.running())
            .map(|j| j.manual.store(true, Ordering::Release))
            .is_some())
    }

    pub(crate) fn set_interval(&self, interval_ms: u64) -> AppResult<bool> {
        Ok(self
            .lock()?
            .as_ref()
            .map(|j| {
                j.interval_ms
                    .store(clamp_interval(Some(interval_ms)), Ordering::Release)
            })
            .is_some())
    }

    pub(crate) fn stop(&self) -> AppResult<bool> {
        let guard = self.lock()?;
        let Some(job) = guard.as_ref() else {
            return Ok(false);
        };
        let running = job.running();
        if running {
            job.control.cancel();
            if let Ok(mut shared) = job.shared.lock() {
                shared.status = "stopped".into();
            }
        }
        let cleanup = job
            .models
            .detach_owner(&crate::model_sessions::Owner::Live(job.meeting_id.clone()));
        drop(guard);
        cleanup();
        Ok(running)
    }

    /// Starts live notes for one meeting. Saved keys still need cloud text release for this
    /// meeting; the release is never stored and does not carry over to a restart.
    pub(crate) fn start(
        &self,
        service: &Arc<Service>,
        mut request: LiveNotesRequest,
    ) -> AppResult<LiveNotesStatus> {
        let mut key = Zeroizing::new(std::mem::take(&mut request.api_key));
        if valid_legacy_id(&request.meeting_id) {
            return Err(
                "Importierte Sitzungen der bisherigen Version werden nicht verändert.".into(),
            );
        }
        validate_id(&request.meeting_id)?;
        if !["de", "en"].contains(&request.language.as_str()) {
            return Err("Unbekannte Sprache.".into());
        }
        let mut guard = self.lock()?;
        if guard.as_ref().is_some_and(Job::running) {
            return Err(
                "Eine Live-Zusammenfassung läuft bereits. Bitte sie zuerst beenden.".into(),
            );
        }
        // A stopped worker may still be finishing a cancelled request; it must be gone before
        // a new runner reads the stored windows, so two writers never share a run.
        if let Some(handle) = guard.as_mut().and_then(|job| job.worker.take()) {
            let _ = handle.join();
        }
        // The meeting must exist and be readable before any model client is created.
        let data = service.live_snapshot(&request.meeting_id, None)?.data;
        // Held by the worker until it ends, so archive moves or installation never run while
        // live notes write into the session folder.
        let lease = service.reserve_operation(crate::operations::Kind::LiveNotes)?;
        if request.use_saved_key {
            if !key.is_empty() {
                return Err("Bitte entweder den gespeicherten Schlüssel oder einen Schlüssel nur für diese Sitzung wählen.".into());
            }
            key = crate::credential_service::load_for_run(
                &request.endpoint,
                &request.mode,
                request.cloud_consent,
            )?;
        }
        let connection = meeting_notes::Connection {
            endpoint: request.endpoint,
            model: request.model,
            mode: request.mode,
            cloud_consent: request.cloud_consent,
            api_key: std::mem::take(&mut *key),
        };
        let model_work = service.model_sessions.reserve(
            crate::model_sessions::Owner::Live(request.meeting_id.clone()),
            &connection,
            crate::local_models::provider_for_endpoint(&connection.endpoint),
        )?;
        let model_lease = model_work.as_ref().map(|work| work.lease()).transpose()?;
        let runner = LiveRunner::start(
            &live_root(service, &request.meeting_id),
            &request.meeting_id,
            connection,
            &request.language,
        )
        .map_err(|e| e.to_string())?;
        let runner = match &model_lease {
            Some(lease) => runner
                .with_residency(Arc::clone(lease))
                .map_err(|e| e.to_string())?,
            None => runner,
        };
        let (texts, _) = live_texts(&data);
        let shared = Arc::new(Mutex::new(Shared {
            status: "starting".into(),
            waiting_for_transcript: false,
            error: None,
            pending_chars: live::pending_chars(&runner.pending(&texts)),
            last_update: Instant::now(),
            last_trigger: None,
            retry_at: None,
            view: Some(runner.view()),
        }));
        let pending_at_start = shared.lock().is_ok_and(|s| s.pending_chars > 0);
        let mut job = Job {
            meeting_id: request.meeting_id.clone(),
            control: AnalysisControl::default(),
            // Text that already exists is summarized right away instead of after the interval.
            manual: Arc::new(AtomicBool::new(pending_at_start)),
            interval_ms: Arc::new(AtomicU64::new(clamp_interval(request.interval_ms))),
            shared,
            worker: None,
            models: service.model_sessions.clone(),
        };
        let worker = Worker {
            model_work,
            model_lease,
            _lease: lease,
            service: Arc::clone(service),
            meeting_id: request.meeting_id,
            control: job.control.clone(),
            manual: Arc::clone(&job.manual),
            interval_ms: Arc::clone(&job.interval_ms),
            shared: Arc::clone(&job.shared),
        };
        job.worker = Some(
            thread::Builder::new()
                .name("live-notes".into())
                .spawn(move || worker.run(runner))
                .map_err(|_| "Die Live-Zusammenfassung konnte nicht gestartet werden.")?,
        );
        let status = job.status();
        *guard = Some(job);
        Ok(status)
    }
}

/// A failed update keeps its trigger: once the backoff has passed it runs again, even when the
/// pending text alone would not trigger an update. During backoff nothing is taken.
/// The same carry holds the rest of a backlog after a stored window (see `after_window`).
fn with_retry(
    trigger: Option<Trigger>,
    retry: &mut Option<Trigger>,
    backing_off: bool,
    pending_chars: usize,
) -> Option<Trigger> {
    if trigger.is_some() || backing_off || pending_chars == 0 {
        return trigger;
    }
    retry.take()
}
/// After a stored window, text that was already captured before it is summarized next without
/// waiting for a threshold or the interval, whatever triggered the window; text that arrives
/// later follows the normal rules. `captured_left` counts that rest in the same snapshot.
fn after_window(trigger: Trigger, captured_left: usize) -> Option<Trigger> {
    (captured_left > 0).then_some(trigger)
}
/// Ends only the recognition waiting notice; an update or overview error stays until a
/// later result replaces it.
fn clear_recognition_wait(error: &mut Option<String>) {
    if error.as_deref() == Some(RECOGNITION_WAIT) {
        *error = None;
    }
}
/// Whether only the overview should be rebuilt now: a bounded number of attempts, never in a
/// tight loop, also after the meeting has ended.
fn overview_retry(due: bool, failures: u32, manual: bool, since_attempt: Option<Duration>) -> bool {
    due && failures < MAX_OVERVIEW_FAILURES
        && (manual || since_attempt.is_none_or(|elapsed| elapsed >= OVERVIEW_RETRY))
}

struct Worker {
    model_work: Option<crate::model_sessions::ModelWork>,
    model_lease: Option<Arc<meeting_notes::residency::ModelLease>>,
    _lease: crate::operations::Lease,
    service: Arc<Service>,
    meeting_id: String,
    control: AnalysisControl,
    manual: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
    shared: Arc<Mutex<Shared>>,
}

impl Worker {
    fn set(&self, change: impl FnOnce(&mut Shared)) {
        if let Ok(mut shared) = self.shared.lock() {
            // A stop wins over everything the worker reports afterwards.
            if shared.status != "stopped" {
                change(&mut shared);
            }
        }
    }

    /// Publishes the stored state even after a stop, so a window saved just before the stop
    /// is shown and not only after the next start.
    fn publish(&self, runner: &LiveRunner) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.view = Some(runner.view());
        }
    }

    fn wait(&self) {
        self.wait_with_manual(true);
    }

    /// Waits cooperatively for cancellation, optionally allowing a queued manual update
    /// to interrupt. ASR-priority waits retain manual work without spinning on that flag.
    fn wait_with_manual(&self, interrupt_for_manual: bool) {
        let until = Instant::now() + POLL;
        while Instant::now() < until && !self.control.is_cancelled() {
            if interrupt_for_manual && self.manual.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Schedules text-only windows from confirmed sections, deferring only for living ASR
    /// workers with backlog. Retries retain their trigger and backoff; cancellation wins
    /// over later progress, and completed windows remain available after a stop.
    fn run(self, mut runner: LiveRunner) {
        if let Some(lease) = &self.model_lease {
            if let Err(error) = lease.prepare(&self.control) {
                if self.control.is_cancelled() {
                    if let Some(work) = &self.model_work {
                        work.finish();
                    }
                    self.service
                        .model_sessions
                        .finish_owner(&crate::model_sessions::Owner::Live(self.meeting_id.clone()));
                    self.set(|shared| {
                        shared.status = "failed".into();
                        shared.error = Some(error.to_string());
                    });
                    return;
                }
                // Each inference prepares again. Keep the existing update/backoff
                // loop and retained owner available after a transient load failure.
                self.set(|shared| shared.error = Some(error.to_string()));
            }
        }
        let mut failures: u32 = 0;
        let mut overview_failures: u32 = 0;
        let mut overview_attempt: Option<Instant> = None;
        // A failed update or the rest of a captured backlog, taken by the next round.
        let mut carry: Option<Trigger> = None;
        let mut finished_state: Option<SessionData> = None;
        let mut deferred_since: Option<Instant> = None;
        self.set(|s| s.status = "waiting".into());
        while !self.control.is_cancelled() {
            let snapshot = match self
                .service
                .live_snapshot(&self.meeting_id, finished_state.as_ref())
            {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    self.set(|s| s.error = Some(error));
                    self.wait();
                    continue;
                }
            };
            let finished = snapshot.finished();
            if finished {
                finished_state = Some(snapshot.data.clone());
            }
            let (texts, _) = live_texts(&snapshot.data);
            let pending = runner.pending(&texts);
            let pending_chars = live::pending_chars(&pending);
            let (since, retry_at) = self
                .shared
                .lock()
                .map(|s| (s.last_update.elapsed(), s.retry_at))
                .unwrap_or((Duration::ZERO, None));
            self.set(|s| s.pending_chars = pending_chars);
            let defer = snapshot.recognition_needs_priority();
            self.set(|s| s.waiting_for_transcript = defer);
            if defer {
                let since = deferred_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(120) {
                    self.set(|s| s.error = Some(RECOGNITION_WAIT.into()));
                }
                // Keep manual requests queued, but never busy-spin on them while ASR
                // catches up. No new model request competes with the recording worker.
                self.wait_with_manual(false);
                continue;
            }
            if deferred_since.take().is_some() {
                self.set(|s| clear_recognition_wait(&mut s.error));
            }
            let backing_off = retry_at.is_some_and(|at| Instant::now() < at);
            let manual = self.manual.swap(false, Ordering::AcqRel);
            let interval = self.interval_ms.load(Ordering::Acquire);
            let mut trigger = live::decide(
                &pending,
                since.as_millis() as u64,
                interval,
                manual,
                finished,
            );
            trigger = with_retry(trigger, &mut carry, backing_off, pending_chars);
            // A missing overview is retried at a bounded pace, also after the meeting ended.
            let overview_open = runner.overview_due() && overview_failures < MAX_OVERVIEW_FAILURES;
            if trigger.is_none()
                && overview_retry(
                    runner.overview_due(),
                    overview_failures,
                    manual,
                    overview_attempt.map(|at| at.elapsed()),
                )
            {
                trigger = Some(Trigger::Manual);
            }
            if finished && trigger.is_none() && !overview_open {
                if !runner.overview_due() {
                    if let Some(work) = &self.model_work {
                        work.finish();
                    }
                    self.service
                        .model_sessions
                        .finish_owner(&crate::model_sessions::Owner::Live(self.meeting_id.clone()));
                }
                self.publish(&runner);
                self.set(|s| {
                    s.status = if runner.overview_due() {
                        "partial"
                    } else {
                        "finished"
                    }
                    .into();
                    s.pending_chars = 0;
                });
                return;
            }
            match trigger {
                Some(trigger) if !backing_off || manual => {
                    self.set(|s| s.status = "updating".into());
                    overview_attempt = Some(Instant::now());
                    let result = runner.update(&pending, trigger, &self.control);
                    if self.control.is_cancelled() {
                        break;
                    }
                    match result {
                        Ok(outcome) => {
                            // The rest of the already captured text follows at once, after any
                            // window and not only after a manual one.
                            carry = outcome.window.and_then(|_| {
                                after_window(trigger, live::pending_chars(&runner.pending(&texts)))
                            });
                            failures = 0;
                            if outcome.overview_error.is_some() {
                                overview_failures = overview_failures.saturating_add(1);
                            } else if outcome.overview.is_some() {
                                overview_failures = 0;
                            }
                            let summarized = outcome.window.is_some();
                            let view = runner.view();
                            self.set(|s| {
                                s.status = "waiting".into();
                                s.error = outcome
                                    .overview_error
                                    .clone()
                                    .or(outcome.diagnostic_warning.clone());
                                // An overview-only retry does not postpone the next text update.
                                if summarized {
                                    s.last_update = Instant::now();
                                    s.last_trigger = Some(trigger);
                                }
                                s.retry_at = None;
                                s.view = Some(view);
                            });
                            // A remaining backlog is handled by the next decision without waiting;
                            // an overview-only retry always waits.
                            if summarized {
                                continue;
                            }
                        }
                        Err(error) => {
                            carry = Some(trigger);
                            failures = failures.saturating_add(1);
                            let pause = FIRST_BACKOFF
                                .saturating_mul(1 << failures.min(5).saturating_sub(1))
                                .min(MAX_BACKOFF);
                            self.set(|s| {
                                s.status = "waiting".into();
                                s.error = Some(error.to_string());
                                s.retry_at = Some(Instant::now() + pause);
                            });
                            if finished && failures >= 3 {
                                // The meeting is over; stop retrying but keep everything saved.
                                self.publish(&runner);
                                self.set(|s| s.status = "failed".into());
                                return;
                            }
                        }
                    }
                }
                _ => {}
            }
            self.wait();
        }
        self.publish(&runner);
        self.set(|s| s.status = "stopped".into());
        if let Some(work) = &self.model_work {
            work.finish();
        }
        self.service
            .model_sessions
            .finish_owner(&crate::model_sessions::Owner::Live(self.meeting_id.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meeting_core::SessionStore;

    /// A transient native preload refusal must leave the worker alive until explicit cancellation.
    #[test]
    fn transient_preload_failure_keeps_the_live_loop_and_anchor_available() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 1024];
            while !bytes.windows(4).any(|p| p == b"\r\n\r\n") {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && bytes.len() < 8192);
                bytes.extend_from_slice(&buffer[..n]);
            }
            stream
                .write_all(
                    b"HTTP/1.1 503 Synthetic\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .unwrap();
        });
        let root = fixture_root("preload-retry");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic preload", "m", "s", None)
                .unwrap();
        let id = store.snapshot().unwrap().id;
        service.lock().unwrap().view = Some(store);
        let connection = meeting_notes::Connection {
            endpoint,
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let work = service
            .model_sessions
            .reserve(
                crate::model_sessions::Owner::Live(id.clone()),
                &connection,
                Some(meeting_notes::residency::LocalProvider::Ollama),
            )
            .unwrap()
            .unwrap();
        let lease = work.lease().unwrap();
        let runner = LiveRunner::start(&live_root(&service, &id), &id, connection, "de")
            .unwrap()
            .with_residency(Arc::clone(&lease))
            .unwrap();
        let shared = Arc::new(Mutex::new(Shared {
            status: "starting".into(),
            waiting_for_transcript: false,
            error: None,
            pending_chars: 0,
            last_update: Instant::now(),
            last_trigger: None,
            retry_at: None,
            view: None,
        }));
        let control = AnalysisControl::default();
        let worker = Worker {
            model_work: Some(work.clone()),
            model_lease: Some(lease),
            _lease: service
                .reserve_operation(crate::operations::Kind::LiveNotes)
                .unwrap(),
            service: Arc::clone(&service),
            meeting_id: id,
            control: control.clone(),
            manual: Arc::new(AtomicBool::new(false)),
            interval_ms: Arc::new(AtomicU64::new(DEFAULT_INTERVAL_MS)),
            shared: Arc::clone(&shared),
        };
        let handle = thread::spawn(move || worker.run(runner));
        server.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while shared.lock().unwrap().status == "starting" && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let status = shared.lock().unwrap().status.clone();
        let running = !handle.is_finished();
        let retained = work.lease().is_ok();
        control.cancel();
        handle.join().unwrap();
        assert_eq!(
            status, "waiting",
            "A transient preload refusal ended the live summary"
        );
        assert!(running && retained);
        assert!(
            work.lease().is_err(),
            "Explicit stop must release this owner"
        );
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catching_up_with_recognition_keeps_unrelated_errors() {
        let mut error = Some(RECOGNITION_WAIT.to_string());
        clear_recognition_wait(&mut error);
        assert_eq!(error, None);
        let mut error = Some("Kurzfassung fehlgeschlagen.".to_string());
        clear_recognition_wait(&mut error);
        assert_eq!(error.as_deref(), Some("Kurzfassung fehlgeschlagen."));
    }

    #[test]
    fn terminal_status_never_reports_waiting_for_transcript() {
        let shared = Arc::new(Mutex::new(Shared {
            status: "waiting".into(),
            waiting_for_transcript: true,
            error: None,
            pending_chars: 250,
            last_update: Instant::now(),
            last_trigger: None,
            retry_at: None,
            view: None,
        }));
        let job = Job {
            meeting_id: "a".repeat(32),
            control: AnalysisControl::default(),
            manual: Arc::new(AtomicBool::new(false)),
            interval_ms: Arc::new(AtomicU64::new(DEFAULT_INTERVAL_MS)),
            shared: Arc::clone(&shared),
            worker: None,
            models: crate::model_sessions::Models::default(),
        };
        assert!(job.status().waiting_for_transcript);
        for status in ["stopped", "finished", "partial", "failed"] {
            shared.lock().unwrap().status = status.into();
            let view = job.status();
            assert!(!view.running);
            assert!(!view.waiting_for_transcript, "{status}");
            assert_eq!(view.pending_chars, 250);
        }
    }

    #[test]
    fn live_generation_defers_for_real_unprocessed_audio_only() {
        let mut data: SessionData = serde_json::from_value(serde_json::json!({
            "schemaVersion":2,"id":"a".repeat(32),"title":"Synthetic backlog","startedAt":"2026-10-28T00:00:00Z",
            "status":"streaming","modelId":"synthetic",
            "parts":[{"file":"microphone-000000000000.wav","source":"microphone","startSample":0,"sampleCount":160000,"confirmedSampleCount":160000,"processedSamples":0,"modelId":"synthetic","status":"pending"}]
        })).unwrap();
        assert!(Snapshot {
            data: data.clone(),
            active: true,
            recognizing_tracks: [true; 2],
        }
        .recognition_needs_priority());
        data.parts[0].processed_samples = 80_000;
        assert!(!Snapshot {
            data: data.clone(),
            active: true,
            recognizing_tracks: [true; 2],
        }
        .recognition_needs_priority());
        data.parts[0].processed_samples = 160_000;
        assert!(!Snapshot {
            data: data.clone(),
            active: true,
            recognizing_tracks: [true; 2],
        }
        .recognition_needs_priority());
        data.parts[0].model_id = Some("another-model".into());
        assert!(Snapshot {
            data: data.clone(),
            active: true,
            recognizing_tracks: [true; 2],
        }
        .recognition_needs_priority());
        assert!(!Snapshot {
            data,
            active: false,
            recognizing_tracks: [false; 2],
        }
        .recognition_needs_priority());
    }

    #[test]
    /// Resumed sessions may retain completed text with an older progress counter. Only the
    /// unfinished same-track audio should defer summaries; another model still needs work.
    fn completed_resumed_parts_do_not_defer_live_notes_for_their_pending_track() {
        for source in ["system", "microphone"] {
            for pending_seconds in [1, 6] {
                let data: SessionData = serde_json::from_value(serde_json::json!({
                    "schemaVersion":2,"id":"a".repeat(32),"title":"Synthetic resumed backlog",
                    "startedAt":"2026-10-29T00:00:00Z","status":"catching-up","modelId":"synthetic",
                    "parts":[
                        {"file":format!("{source}-000000000000.wav"),"source":source,"startSample":0,
                         "sampleCount":160000,"confirmedSampleCount":160000,"audioClosed":true,
                         "processedSamples":0,"modelId":"synthetic","status":"complete","text":"Confirmed original text."},
                        {"file":format!("{source}-000000160000.wav"),"source":source,"startSample":160000,
                         "sampleCount":pending_seconds*16000,"confirmedSampleCount":pending_seconds*16000,
                         "audioClosed":true,"processedSamples":0,"modelId":"synthetic","status":"pending"}
                    ]
                })).unwrap();
                let before = serde_json::to_vec(&data).unwrap();
                let mut snapshot = Snapshot {
                    data,
                    active: true,
                    recognizing_tracks: [true; 2],
                };
                assert_eq!(
                    live_texts(&snapshot.data).0[0].text,
                    "Confirmed original text."
                );
                assert_eq!(
                    snapshot.recognition_needs_priority(),
                    pending_seconds > 5,
                    "{source}: only the unfinished {pending_seconds}s should count"
                );
                assert_eq!(serde_json::to_vec(&snapshot.data).unwrap(), before);
                snapshot.data.parts[0].model_id = Some("another-model".into());
                assert!(
                    snapshot.recognition_needs_priority(),
                    "other-model audio still needs recognition"
                );
            }
        }
    }

    #[test]
    /// Exercises both ended-track directions against one- and six-second healthy backlogs.
    /// Only the living recognizer's work may defer notes; retained session bytes must match.
    fn ended_track_backlogs_do_not_starve_a_healthy_tracks_confirmed_text() {
        for failed_track in ["system", "microphone"] {
            for healthy_seconds in [1, 6] {
                let root = fixture_root("ended-track-backlog");
                let service = Service::new(root.clone()).unwrap();
                let store = meeting_core::SessionStore::create(
                    &root.join("sessions"),
                    "Synthetic track failure",
                    "synthetic",
                    "s",
                    None,
                )
                .unwrap();
                let mut workers = Vec::new();
                let (release, busy) = std::sync::mpsc::channel::<()>();
                let mut busy = Some(busy);
                for track in ["system", "microphone"] {
                    let seconds = if track == failed_track {
                        6
                    } else {
                        healthy_seconds
                    };
                    let mut writer = meeting_core::PcmWriter::new(store.clone(), track).unwrap();
                    for _ in 0..seconds {
                        writer
                            .append(&vec![1_i16; meeting_core::SAMPLE_RATE as usize])
                            .unwrap();
                    }
                    writer.finish().unwrap();
                    let worker = if track == failed_track {
                        let worker = thread::spawn(|| {});
                        while !worker.is_finished() {
                            thread::yield_now();
                        }
                        worker
                    } else {
                        let wait = busy.take().unwrap();
                        thread::spawn(move || {
                            let _ = wait.recv();
                        })
                    };
                    workers.push(crate::asr::RecognitionWorker {
                        source: track.into(),
                        handle: worker,
                    });
                }
                store.set_status("catching-up").unwrap();
                let before = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
                let id = store.snapshot().unwrap().id;
                service.lock().unwrap().active = Some(crate::Active {
                    _operation: None,
                    owner: None,
                    store: store.clone(),
                    capture: None,
                    stop: Arc::new(AtomicBool::new(true)),
                    workers,
                });
                let snapshot = service.live_snapshot(&id, None).unwrap();
                let defer = snapshot.recognition_needs_priority();
                release.send(()).unwrap();
                for worker in service.lock().unwrap().active.take().unwrap().workers {
                    worker.handle.join().unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&store.snapshot().unwrap()).unwrap(),
                    before
                );
                assert_eq!(
                    defer,
                    healthy_seconds > 5,
                    "ended {failed_track}, healthy backlog {healthy_seconds}s"
                );
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }

    fn fixture_root(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "handymeeting-live-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn request(meeting_id: &str, mode: &str, endpoint: &str) -> LiveNotesRequest {
        LiveNotesRequest {
            meeting_id: meeting_id.into(),
            endpoint: endpoint.into(),
            model: "synthetic".into(),
            mode: mode.into(),
            cloud_consent: false,
            api_key: String::new(),
            use_saved_key: false,
            interval_ms: None,
            language: "de".into(),
        }
    }

    fn wait_for(service: &Service, id: &str, wanted: &str) -> LiveNotesStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = service.live_notes.status(service, id).unwrap();
            if status.status == wanted {
                return status;
            }
            assert!(Instant::now() < deadline, "still {}", status.status);
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn only_complete_parts_count_and_an_active_meeting_is_not_finished() {
        let data: SessionData = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1, "id": "a".repeat(32), "title": "t", "startedAt": "2026-10-28T00:00:00Z",
            "status": "recording",
            "parts": [
                {"file":"system-000000000000.wav","source":"system","startSample":0,"sampleCount":480000,"status":"complete","text":"Erster Abschnitt."},
                {"file":"microphone-000000480000.wav","source":"microphone","startSample":480000,"sampleCount":16000,"status":"pending","committedText":"noch offen"},
                {"file":"system-000000480000.wav","source":"system","startSample":480000,"sampleCount":16000,"status":"complete"}
            ]
        }))
        .unwrap();
        let (texts, finished) = live_texts(&data);
        assert!(!finished);
        assert_eq!(texts.len(), 1);
        let rate = u64::from(meeting_core::SAMPLE_RATE);
        assert_eq!(texts[0].end_ms, 480_000 * 1000 / rate);
        let mut done = data.clone();
        done.status = "complete".into();
        assert!(live_texts(&done).1);
        done.status = "catching-up".into();
        assert!(!live_texts(&done).1, "catch-up still produces text");
    }

    #[test]
    fn a_meeting_still_in_the_active_slot_is_not_finished_even_when_marked_recorded() {
        let root = fixture_root("active-slot");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic", "m", "s", None).unwrap();
        let id = store.snapshot().unwrap().id;
        // Synthetic: capture has stopped ("recorded") but a recognition worker still runs.
        store.set_status("recorded").unwrap();
        let (release, busy) = std::sync::mpsc::channel::<()>();
        let worker = thread::spawn(move || {
            let _ = busy.recv();
        });
        service.lock().unwrap().active = Some(crate::Active {
            _operation: None,
            owner: None,
            store: store.clone(),
            capture: None,
            stop: Arc::new(AtomicBool::new(false)),
            workers: vec![crate::asr::RecognitionWorker {
                source: "system".into(),
                handle: worker,
            }],
        });
        service
            .live_notes
            .start(&service, request(&id, "local", "http://127.0.0.1:9/v1/"))
            .unwrap();
        wait_for(&service, &id, "waiting");
        thread::sleep(POLL + Duration::from_millis(300));
        assert_eq!(
            service.live_notes.status(&service, &id).unwrap().status,
            "waiting",
            "catch-up can still add text"
        );
        // Recognition finished: the application releases the slot on its next poll.
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while service.lock().unwrap().active.is_some() {
            assert!(Instant::now() < deadline);
            service.reap_finished().unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        wait_for(&service, &id, "finished");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_failed_update_is_retried_after_its_backoff_even_below_the_thresholds() {
        // CodeRabbit review 5682803580: a failed manual update with little pending text was
        // dropped, because the normal rules do not trigger for so few characters.
        let mut retry = Some(Trigger::Manual);
        assert_eq!(
            with_retry(None, &mut retry, true, 50),
            None,
            "waits during backoff"
        );
        assert_eq!(retry, Some(Trigger::Manual));
        assert_eq!(
            with_retry(None, &mut retry, false, 50),
            Some(Trigger::Manual)
        );
        assert_eq!(retry, None, "taken once; a new failure stores it again");
        let mut retry = Some(Trigger::Manual);
        assert_eq!(
            with_retry(None, &mut retry, false, 0),
            None,
            "nothing left to summarize"
        );
        assert_eq!(
            with_retry(Some(Trigger::Interval), &mut retry, false, 50),
            Some(Trigger::Interval)
        );
        assert_eq!(retry, Some(Trigger::Manual));
    }

    #[test]
    fn the_captured_rest_of_a_backlog_follows_any_window_at_once() {
        // Review 6474935681 (P2): only a manual window queued the next one. With 6,550
        // confirmed characters a volume window took 6,400 and the remaining 150 stayed
        // unsummarized until new speech or the interval, which may never come.
        let texts: Vec<LiveText> = (0..5u64)
            .map(|i| LiveText {
                key: format!("system-{i}.wav"),
                track: "system".into(),
                start_ms: i * 60_000,
                end_ms: (i + 1) * 60_000,
                text: if i < 4 {
                    "a".repeat(1600)
                } else {
                    "b".repeat(150)
                },
            })
            .collect();
        let pending = live::pending(&[], &texts);
        assert_eq!(live::pending_chars(&pending), 6_550);
        let trigger = live::decide(&pending, 0, live::MIN_INTERVAL_MS, false, false);
        assert_eq!(trigger, Some(Trigger::Volume));
        let window = live::next_window(&pending);
        assert_eq!(live::pending_chars(&window), 6_400);
        let rest = live::pending(&window, &texts);
        assert_eq!(live::pending_chars(&rest), 150);
        assert_eq!(
            live::decide(&rest, 0, live::MIN_INTERVAL_MS, false, false),
            None,
            "the thresholds alone leave the rest waiting"
        );
        let mut carry = after_window(Trigger::Volume, live::pending_chars(&rest));
        assert_eq!(carry, Some(Trigger::Volume));
        assert_eq!(
            with_retry(None, &mut carry, true, 150),
            None,
            "an error backoff still applies"
        );
        assert_eq!(
            with_retry(None, &mut carry, false, 150),
            Some(Trigger::Volume),
            "taken by the next round"
        );
        for trigger in [Trigger::Interval, Trigger::LongContribution, Trigger::Final] {
            assert_eq!(after_window(trigger, 150), Some(trigger));
        }
        assert_eq!(
            after_window(Trigger::Volume, 0),
            None,
            "nothing captured is left"
        );
    }

    #[test]
    fn a_failing_overview_is_retried_at_a_bounded_pace_and_then_given_up() {
        let now = Some(Duration::from_secs(1));
        let later = Some(OVERVIEW_RETRY);
        assert!(overview_retry(true, 0, false, None));
        assert!(!overview_retry(true, 0, false, now), "no tight retry loop");
        assert!(overview_retry(true, 1, false, later));
        assert!(
            overview_retry(true, 1, true, now),
            "a manual update may retry at once"
        );
        assert!(!overview_retry(true, MAX_OVERVIEW_FAILURES, true, later));
        assert!(!overview_retry(false, 0, true, None));
    }

    #[test]
    fn running_live_notes_keep_archive_and_installation_waiting_until_they_end() {
        let root = fixture_root("lease");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic", "m", "s", None).unwrap();
        let id = store.snapshot().unwrap().id;
        service.lock().unwrap().view = Some(store);
        service
            .live_notes
            .start(&service, request(&id, "local", "http://127.0.0.1:9/v1/"))
            .unwrap();
        wait_for(&service, &id, "waiting");
        for kind in [
            crate::operations::Kind::Maintenance,
            crate::operations::Kind::Installation,
        ] {
            assert!(service.reserve_operation(kind).is_err(), "{kind:?}");
        }
        // Post-meeting analysis is not blocked by live notes.
        drop(
            service
                .reserve_operation(crate::operations::Kind::Analysis)
                .unwrap(),
        );
        service.live_notes.stop().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while service
            .reserve_operation(crate::operations::Kind::Maintenance)
            .is_err()
        {
            assert!(Instant::now() < deadline, "the lease ends with the worker");
            thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cloud_without_release_and_legacy_meetings_are_refused_before_anything_is_written() {
        let root = fixture_root("refused");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic", "m", "s", None).unwrap();
        let id = store.snapshot().unwrap().id;
        service.lock().unwrap().view = Some(store);
        let cloud = request(&id, "cloud", "https://api.example.com/v1/");
        assert!(service.live_notes.start(&service, cloud).is_err());
        assert!(!live_root(&service, &id).exists());
        let legacy = request("legacy-0123456789abcdef", "local", "http://127.0.0.1:9/v1/");
        assert!(service.live_notes.start(&service, legacy).is_err());
        let unknown = request(&"b".repeat(32), "local", "http://127.0.0.1:9/v1/");
        assert!(service.live_notes.start(&service, unknown).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn live_notes_run_beside_a_recording_and_stopping_them_leaves_the_recording_untouched() {
        let root = fixture_root("beside-recording");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic", "m", "s", None).unwrap();
        let id = store.snapshot().unwrap().id;
        service.lock().unwrap().view = Some(store.clone());
        // Synthetic: nothing listens here, and no text exists yet, so no request is made.
        let started = service
            .live_notes
            .start(&service, request(&id, "local", "http://127.0.0.1:9/v1/"))
            .unwrap();
        assert!(started.running);
        wait_for(&service, &id, "waiting");
        assert!(service
            .live_notes
            .start(&service, request(&id, "local", "http://127.0.0.1:9/v1/"))
            .is_err());
        assert!(service.live_notes.refresh().unwrap());
        assert!(service.live_notes.set_interval(1).unwrap());
        assert_eq!(
            service
                .live_notes
                .status(&service, &id)
                .unwrap()
                .interval_ms,
            live::MIN_INTERVAL_MS
        );
        assert!(service.live_notes.stop().unwrap());
        let stopped = service.live_notes.status(&service, &id).unwrap();
        assert_eq!(stopped.status, "stopped");
        assert!(!stopped.running);
        assert_eq!(store.snapshot().unwrap().status, "recording");
        // A finished meeting without new text ends the next run by itself.
        store.set_status("complete").unwrap();
        service
            .live_notes
            .start(&service, request(&id, "local", "http://127.0.0.1:9/v1/"))
            .unwrap();
        let finished = wait_for(&service, &id, "finished");
        assert!(finished.view.is_some_and(|v| v.windows == 0));
        assert_eq!(
            live::list_runs(&live_root(&service, &id)).unwrap().0.len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
