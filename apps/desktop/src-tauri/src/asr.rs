//! One owned native worker per track. Capture never waits on this module.
use handy_bridge::native::{default_run_options, native_error, NativeEngine};
use handy_bridge::BridgeError;
use meeting_core::{
    continues_confirmed_text, read_pcm_samples, AudioPart, SessionStore, TranscriptCheckpoint,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_REQUEST: usize = 16 * 1024;
const MAX_REPLY: usize = 2 * 1024 * 1024;
const MAX_STDERR: usize = 16 * 1024 * 1024;
const FEED_SAMPLES: usize = 8_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Request {
    SpeakerIdentity {
        identity: handy_bridge::ModelIdentity,
    },
    SpeakerSection {
        part: AudioPart,
    },
    Begin {
        file: String,
    },
    Feed {
        offset_samples: u64,
        sample_count: usize,
        confirmed_samples: u64,
    },
    Finalize,
    Quit,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Reply {
    pub ok: bool,
    #[serde(default)]
    pub speaker_turns: Vec<handy_bridge::native::SpeakerTurn>,
    #[serde(default)]
    pub full: String,
    #[serde(default)]
    pub committed: String,
    #[serde(default)]
    pub tentative: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub error: Option<String>,
}

fn bridge_protocol(_: impl std::fmt::Display) -> BridgeError {
    BridgeError::Native(
        "Recognition worker protocol or confirmed audio was invalid. Saved audio is retained."
            .into(),
    )
}

/// Args are the values after `--asr-worker`: model, backend directory, session
/// directory, track source, and optional `--cpu`. This function opens no device.
pub fn worker_main(args: &[String]) -> Result<(), String> {
    if !(4..=5).contains(&args.len())
        || !["system", "microphone"].contains(&args[3].as_str())
        || (args.len() == 5 && args[4] != "--cpu")
    {
        return Err("Invalid recognition worker arguments".into());
    }
    let model_path = Path::new(&args[0]);
    let backend_dir = Path::new(&args[1]);
    let folder = Path::new(&args[2])
        .canonicalize()
        .map_err(|_| "Session directory is unavailable")?;
    let source = &args[3];
    handy_bridge::verify_model_snapshot(&folder, model_path).map_err(|error| error.to_string())?;
    let engine = NativeEngine::load(model_path, args.len() == 5, Some(backend_dir))
        .map_err(|error| error.to_string())?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    send_reply(
        &mut output,
        Reply {
            ok: true,
            backend: engine.backend(),
            ..Reply::default()
        },
    )?;
    loop {
        let request = read_request(&mut input)?.ok_or("Recognition controller closed its input")?;
        match request {
            Request::Quit => return Ok(()),
            Request::Begin { file } => {
                let path = worker_audio_path(&folder, &file, source)?;
                let result = engine.with_stream(&default_run_options(), |stream| {
                    send_reply(
                        &mut output,
                        Reply {
                            ok: true,
                            ..Reply::default()
                        },
                    )
                    .map_err(bridge_protocol)?;
                    let mut cursor = 0_u64;
                    loop {
                        match read_request(&mut input)
                            .map_err(bridge_protocol)?
                            .ok_or_else(|| bridge_protocol("input closed"))?
                        {
                            Request::Feed {
                                offset_samples,
                                sample_count,
                                confirmed_samples,
                            } => {
                                validate_feed(
                                    cursor,
                                    offset_samples,
                                    sample_count,
                                    confirmed_samples,
                                )
                                .map_err(bridge_protocol)?;
                                let pcm = read_pcm_samples(&path, offset_samples, sample_count)
                                    .map_err(bridge_protocol)?;
                                if pcm.len() != sample_count {
                                    return Err(bridge_protocol("confirmed audio ended early"));
                                }
                                stream.feed(&pcm).map_err(native_error)?;
                                cursor += sample_count as u64;
                                let text = stream.text();
                                send_reply(
                                    &mut output,
                                    Reply {
                                        ok: true,
                                        full: text.full,
                                        committed: text.committed,
                                        tentative: text.tentative,
                                        ..Reply::default()
                                    },
                                )
                                .map_err(bridge_protocol)?;
                            }
                            Request::Finalize => {
                                stream.finalize().map_err(native_error)?;
                                let text = stream.text();
                                send_reply(
                                    &mut output,
                                    Reply {
                                        ok: true,
                                        full: text.full,
                                        committed: text.committed,
                                        tentative: text.tentative,
                                        ..Reply::default()
                                    },
                                )
                                .map_err(bridge_protocol)?;
                                return Ok(());
                            }
                            _ => {
                                return Err(bridge_protocol("unexpected command in active stream"))
                            }
                        }
                    }
                });
                if let Err(error) = result {
                    let _ = send_reply(
                        &mut output,
                        Reply {
                            ok: false,
                            error: Some(error.to_string()),
                            ..Reply::default()
                        },
                    );
                    return Err(error.to_string());
                }
            }
            _ => return Err("Recognition requires a new bounded audio window first".into()),
        }
    }
}

fn validate_feed(cursor: u64, offset: u64, count: usize, confirmed: u64) -> Result<(), String> {
    if count == 0
        || count > FEED_SAMPLES
        || offset != cursor
        || offset
            .checked_add(count as u64)
            .is_none_or(|end| end > confirmed)
    {
        return Err(
            "Recognition may read only sequential confirmed half-second PCM packets".into(),
        );
    }
    Ok(())
}

fn worker_audio_path(folder: &Path, file: &str, source: &str) -> Result<PathBuf, String> {
    let prefix = format!("{source}-");
    let sample = file
        .strip_prefix(&prefix)
        .and_then(|name| name.strip_suffix(".wav"))
        .ok_or("Invalid recognition audio name")?;
    if sample.len() < 12
        || !sample.bytes().all(|byte| byte.is_ascii_digit())
        || sample.parse::<u64>().is_err()
    {
        return Err("Invalid recognition audio index".into());
    }
    let audio = folder.join("audio");
    let path = audio.join(file);
    for selected in [&audio, &path] {
        let meta =
            std::fs::symlink_metadata(selected).map_err(|_| "Recognition audio is unavailable")?;
        if meta.file_type().is_symlink() {
            return Err("Audio links are not accepted by the worker".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Audio reparse points are not accepted by the worker".into());
            }
        }
    }
    Ok(path)
}

pub(super) fn read_request(reader: &mut impl BufRead) -> Result<Option<Request>, String> {
    let bytes = bounded_line(reader, MAX_REQUEST)?;
    bytes
        .map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|_| "Invalid recognition request JSON".into())
        })
        .transpose()
}

fn bounded_line(reader: &mut impl BufRead, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::with_capacity(1024);
    let count = reader
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|_| "Recognition pipe read failed")?;
    if count == 0 {
        return Ok(None);
    }
    if count > limit || !line.ends_with(b"\n") {
        return Err("Recognition pipe line was oversized or incomplete".into());
    }
    Ok(Some(line))
}

pub(super) fn send_reply(writer: &mut impl Write, reply: Reply) -> Result<(), String> {
    let bytes = serde_json::to_vec(&reply).map_err(|_| "Recognition reply could not be encoded")?;
    if bytes.len() > MAX_REPLY {
        return Err("Recognition reply exceeded its safety limit".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|_| "Recognition reply pipe closed".into())
}

pub(super) struct Worker {
    child: Child,
    input: ChildStdin,
    replies: Receiver<Result<Reply, String>>,
    protocol_fault: Arc<AtomicBool>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    absolute_deadline: Option<Instant>,
    cancellation: Option<Arc<AtomicBool>>,
}
enum WorkerKind {
    Asr,
    Speaker(Arc<AtomicBool>),
}

impl Worker {
    fn start(
        model: &Path,
        backend: &Path,
        folder: &Path,
        source: &str,
        cpu_only: bool,
        absolute_deadline: Option<Instant>,
    ) -> Result<Self, String> {
        Self::start_mode(
            model,
            backend,
            folder,
            source,
            cpu_only,
            absolute_deadline,
            WorkerKind::Asr,
        )
    }
    pub(super) fn start_speaker(
        model: &Path,
        backend: &Path,
        folder: &Path,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        Self::start_mode(
            model,
            backend,
            folder,
            "speaker",
            true,
            None,
            WorkerKind::Speaker(cancel),
        )
    }
    fn start_mode(
        model: &Path,
        backend: &Path,
        folder: &Path,
        source: &str,
        cpu_only: bool,
        absolute_deadline: Option<Instant>,
        kind: WorkerKind,
    ) -> Result<Self, String> {
        if absolute_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("Synthetic worker deadline elapsed".into());
        }
        let executable =
            std::env::current_exe().map_err(|_| "Recognition executable was not found")?;
        let mut command = Command::new(executable);
        command
            .arg(if matches!(&kind, WorkerKind::Speaker(_)) {
                "--speaker-worker"
            } else {
                "--asr-worker"
            })
            .arg(model)
            .arg(backend)
            .arg(folder)
            .arg(source);
        if cpu_only {
            command.arg("--cpu");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Recognition child process could not start")?;
        let input = child
            .stdin
            .take()
            .ok_or("Recognition input pipe unavailable")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Recognition output pipe unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Recognition diagnostic pipe unavailable")?;
        let (sender, replies) = mpsc::sync_channel(1);
        let protocol_fault = Arc::new(AtomicBool::new(false));
        let fault = Arc::clone(&protocol_fault);
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let next = match bounded_line(&mut reader, MAX_REPLY) {
                    Ok(Some(bytes)) => serde_json::from_slice::<Reply>(&bytes)
                        .map_err(|_| "Recognition returned invalid protocol JSON".into()),
                    Ok(None) => Err("Recognition process exited before its reply".into()),
                    Err(error) => Err(error),
                };
                let terminal = next.is_err();
                match sender.try_send(next) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        fault.store(true, Ordering::Release);
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
                if terminal {
                    break;
                }
            }
        });
        let fault = Arc::clone(&protocol_fault);
        let stderr_thread = thread::spawn(move || {
            let mut reader = stderr;
            let mut buffer = [0; 4096];
            let mut total = 0_usize;
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                total = total.saturating_add(count);
                if total > MAX_STDERR {
                    fault.store(true, Ordering::Release);
                }
                // Native diagnostics are drained but never persisted or shown as meeting text.
            }
        });
        let worker = Self {
            child,
            input,
            replies,
            protocol_fault,
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            absolute_deadline,
            cancellation: match kind {
                WorkerKind::Asr => None,
                WorkerKind::Speaker(flag) => Some(flag),
            },
        };
        worker.receive(Duration::from_secs(180))?;
        Ok(worker)
    }

    fn receive(&self, timeout: Duration) -> Result<Reply, String> {
        let deadline = self.absolute_deadline.map_or_else(
            || Instant::now() + timeout,
            |end| end.min(Instant::now() + timeout),
        );
        loop {
            if self
                .cancellation
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                return Err("Optional speaker analysis cancelled; recording and original transcripts are unchanged".into());
            }
            if Instant::now() >= deadline {
                return Err("Recognition worker deadline elapsed; saved audio is retained".into());
            }
            if self.protocol_fault.load(Ordering::Acquire) {
                return Err(
                    "Recognition worker exceeded its protocol or diagnostic output limit".into(),
                );
            }
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => {
                    let reply = result?;
                    if !reply.ok {
                        return Err(reply.error.unwrap_or_else(|| {
                            "Recognition worker rejected the operation".into()
                        }));
                    }
                    return Ok(reply);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Recognition reply channel closed".into())
                }
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                    return Err(
                        "Recognition worker exceeded its time limit; saved audio is retained"
                            .into(),
                    )
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    pub(super) fn call(&mut self, request: Request) -> Result<Reply, String> {
        let timeout = if matches!(&request, Request::SpeakerIdentity { .. }) {
            180
        } else {
            90
        };
        let bytes =
            serde_json::to_vec(&request).map_err(|_| "Recognition request could not be encoded")?;
        if bytes.len() >= MAX_REQUEST {
            return Err("Recognition request exceeded its limit".into());
        }
        self.input
            .write_all(&bytes)
            .and_then(|_| self.input.write_all(b"\n"))
            .and_then(|_| self.input.flush())
            .map_err(|_| "Recognition child input closed")?;
        self.receive(Duration::from_secs(timeout))
    }
}

/// One recognition stream as the track controller sees it. Production uses the owned
/// child process; unit tests substitute a synthetic in-process recognizer.
trait Recognizer {
    fn request(&mut self, request: Request) -> Result<Reply, String>;
    /// Ends the owned recognizer; used only by the synthetic interruption hook.
    fn terminate(&mut self) -> Result<(), String>;
}

impl Recognizer for Worker {
    fn request(&mut self, request: Request) -> Result<Reply, String> {
        self.call(request)
    }
    fn terminate(&mut self) -> Result<(), String> {
        self.child
            .kill()
            .map_err(|_| "Owned synthetic recognition child could not be terminated".into())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // This handle was created above, so no foreign Handy or user process can be killed.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.stdout_thread.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_thread.take() {
            let _ = reader.join();
        }
    }
}

/// The exact track owned by a recognizer, retained even after its thread has ended.
/// Callers must still join the handle; completion only removes compute-priority admission.
pub(crate) struct RecognitionWorker {
    pub source: String,
    pub handle: JoinHandle<()>,
}

/// Starts recognition independently of capture and retains its track identity for scheduling.
pub(crate) fn spawn_track(
    store: SessionStore,
    model_path: PathBuf,
    backend_dir: PathBuf,
    source: String,
    stop: Arc<AtomicBool>,
) -> RecognitionWorker {
    let worker_source = source.clone();
    let handle = spawn_track_options(
        store,
        model_path,
        backend_dir,
        source,
        stop,
        TrackRunOptions::default(),
    );
    RecognitionWorker {
        source: worker_source,
        handle,
    }
}

#[derive(Clone, Default)]
struct TrackRunOptions {
    deadline: Option<Instant>,
    force_cpu: bool,
    inject_kill: Option<Arc<AtomicBool>>,
    first_journal: Option<Arc<Mutex<Vec<u8>>>>,
    kill_checkpoint: Option<Arc<Mutex<Option<serde_json::Value>>>>,
    restarts: Option<Arc<AtomicUsize>>,
}

fn spawn_track_options(
    store: SessionStore,
    model_path: PathBuf,
    backend_dir: PathBuf,
    source: String,
    stop: Arc<AtomicBool>,
    options: TrackRunOptions,
) -> JoinHandle<()> {
    let worker_source = source.clone();
    let deadline = options.deadline;
    spawn_track_with(store, source, stop, options, move |folder, cpu_only| {
        Worker::start(
            &model_path,
            &backend_dir,
            folder,
            &worker_source,
            cpu_only,
            deadline,
        )
    })
}

fn spawn_track_with<R, S>(
    store: SessionStore,
    source: String,
    stop: Arc<AtomicBool>,
    options: TrackRunOptions,
    start: S,
) -> JoinHandle<()>
where
    R: Recognizer,
    S: Fn(&Path, bool) -> Result<R, String> + Send + 'static,
{
    thread::spawn(move || {
        if !["system", "microphone"].contains(&source.as_str()) {
            let _ = store.notice("Recognition track was invalid; audio remains available.");
            return;
        }
        for attempt in 0..3 {
            let cpu_only = options.force_cpu || attempt >= 2;
            match run_track(&store, &source, &stop, cpu_only, &options, &start) {
                Ok(()) => {
                    if let Ok(snapshot) = store.snapshot() {
                        if ["recorded", "catching-up", "complete"]
                            .contains(&snapshot.status.as_str())
                            && snapshot
                                .parts
                                .iter()
                                .all(|part| part.audio_closed && part.status == "complete")
                        {
                            let _ = store.set_status("complete");
                        }
                    }
                    return;
                }
                Err(_) if attempt < 2 => {
                    if let Some(count) = &options.restarts {
                        count.fetch_add(1, Ordering::AcqRel);
                    }
                    let route = if attempt == 1 {
                        "CPU replacement"
                    } else {
                        "a fresh recognition process"
                    };
                    let _ = store.notice(&format!("{source}: recognition interrupted; restarting with {route}. Audio capture remains independent and saved checkpoints are retained."));
                    thread::sleep(Duration::from_millis(500));
                }
                Err(error) => {
                    let _ = store.notice(&format!("{source}: recognition could not continue after two restarts including CPU replacement. {error} Audio remains saved; pending parts can be retried from history."));
                    return;
                }
            }
        }
    })
}

fn run_track<R: Recognizer>(
    store: &SessionStore,
    source: &str,
    stop: &AtomicBool,
    cpu_only: bool,
    options: &TrackRunOptions,
    start: &impl Fn(&Path, bool) -> Result<R, String>,
) -> Result<(), String> {
    let model_id = store.model_id().map_err(|_| "Session unavailable")?;
    let mut worker = start(store.folder(), cpu_only)?;
    // Open stream: file, fed cursor, and whether this fresh stream still replays a part
    // that already had durable progress of this model (a recognition restart, #20).
    let mut current: Option<(String, u64, bool)> = None;
    loop {
        if store.model_id().map_err(|_| "Session unavailable")? != model_id {
            return Err(
                "Model changed during recognition; start a separate explicit analysis run".into(),
            );
        }
        let next = store
            .next_pending_part(source, &model_id)
            .map_err(|_| "Session unavailable")?;
        let Some(part) = next else {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        };
        if !part.has_signal && part.audio_closed {
            persist(
                store,
                &part,
                &model_id,
                Reply {
                    ok: true,
                    ..Reply::default()
                },
                part.confirmed_sample_count,
                true,
                false,
            )?;
            current = None;
            continue;
        }
        if !part.has_signal {
            if stop.load(Ordering::Acquire) {
                return Err("Open audio has not been closed; pending work is retained".into());
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        if current
            .as_ref()
            .is_none_or(|(file, _, _)| file != &part.file)
        {
            worker.request(Request::Begin {
                file: part.file.clone(),
            })?;
            let restart_replay =
                part.model_id.as_deref() == Some(model_id.as_str()) && part.processed_samples > 0;
            current = Some((part.file.clone(), 0, restart_replay));
        }
        let (_, cursor, restart_replay) =
            current.as_mut().ok_or("Recognition cursor unavailable")?;
        let available = part
            .confirmed_sample_count
            .checked_sub(*cursor)
            .ok_or("Confirmed audio moved backward")?;
        if available >= FEED_SAMPLES as u64 || (part.audio_closed && available > 0) {
            let count = available.min(FEED_SAMPLES as u64) as usize;
            let reply = worker.request(Request::Feed {
                offset_samples: *cursor,
                sample_count: count,
                confirmed_samples: part.confirmed_sample_count,
            })?;
            let has_hypothesis = !reply.full.is_empty();
            let kill_checkpoint = options.kill_checkpoint.as_ref().map(|_| {
                serde_json::json!({
                    "full": reply.full,
                    "committed": reply.committed,
                    "tentative": reply.tentative,
                    "processedSamples": *cursor + count as u64,
                    "modelConfirmedPrefixWasEmpty": reply.committed.is_empty(),
                    "textWasTentative": reply.full != reply.committed,
                    "audioPartWasFinalized": false,
                    "durableJournalCheckpoint": true
                })
            });
            *cursor += count as u64;
            if persist(
                store,
                &part,
                &model_id,
                reply,
                *cursor,
                false,
                *restart_replay,
            )? {
                // The replay caught up with the durable progress; later checkpoints of
                // this stream are ordinary continuations again.
                *restart_replay = false;
            }
            if has_hypothesis
                && options
                    .inject_kill
                    .as_ref()
                    .is_some_and(|injected| !injected.load(Ordering::Acquire))
            {
                if let Some(checkpoint) = &options.kill_checkpoint {
                    *checkpoint
                        .lock()
                        .map_err(|_| "Synthetic kill checkpoint lock failed")? = kill_checkpoint;
                }
                if let Some(prefix) = &options.first_journal {
                    *prefix
                        .lock()
                        .map_err(|_| "Synthetic checkpoint lock failed")? =
                        std::fs::read(store.folder().join("transcript-journal.jsonl"))
                            .map_err(|_| "Synthetic checkpoint could not be read")?;
                }
                worker.terminate()?;
                if let Some(injected) = &options.inject_kill {
                    injected.store(true, Ordering::Release);
                }
                return Err("Intentional interruption of owned synthetic recognition child".into());
            }
            continue;
        }
        if part.audio_closed && *cursor == part.confirmed_sample_count {
            let reply = worker.request(Request::Finalize)?;
            persist(
                store,
                &part,
                &model_id,
                reply,
                *cursor,
                true,
                *restart_replay,
            )?;
            current = None;
            continue;
        }
        if stop.load(Ordering::Acquire) {
            return Err("Recording must close its audio chunks before final recognition; pending work is retained".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Returns whether a checkpoint was written. `restart_replay` is true only while a
/// stream reconstructed after a recognition restart has not yet written a checkpoint
/// for its part.
fn persist(
    store: &SessionStore,
    part: &AudioPart,
    model_id: &str,
    reply: Reply,
    samples: u64,
    final_text: bool,
    restart_replay: bool,
) -> Result<bool, String> {
    // Reconstruct a fresh native stream without replacing a later durable text
    // checkpoint with its earlier, usually empty replay hypothesis.
    if part.model_id.as_deref() == Some(model_id) && samples < part.processed_samples {
        return Ok(false);
    }
    // A caught-up replay can decode differently, e.g. on the CPU replacement after a
    // GPU fault (#20). It becomes an explicitly marked new text version; the earlier
    // checkpoints remain in the journal. Outside this case a changed prefix is still
    // rejected by the store.
    let revision_after_restart =
        restart_replay && !continues_confirmed_text(part, model_id, &reply.committed, &reply.full);
    store
        .append_transcript(TranscriptCheckpoint {
            sequence: 0,
            at: String::new(),
            file: part.file.clone(),
            model_id: model_id.into(),
            full: reply.full,
            committed: reply.committed,
            tentative: reply.tentative,
            processed_samples: samples,
            final_text,
            revision_after_restart,
        })
        .map(|()| true)
        .map_err(|error| {
            format!(
                "Recognition checkpoint rejected: {error}. Previous confirmed text is retained."
            )
        })
}

/// Accelerated native test using explicitly supplied synthetic speech fixtures.
/// This opens no audio device and is not a real-time or long-session test.
pub fn synthetic_smoke(args: &[String]) -> Result<(), String> {
    if !(5..=6).contains(&args.len()) {
        return Err(
            "Usage: --synthetic-smoke MODEL BACKEND_DIR SYSTEM_WAV MICROPHONE_WAV OUTPUT_ROOT [HANDY_EXECUTABLE]"
                .into(),
        );
    }
    let started = Instant::now();
    let output = Path::new(&args[4]);
    std::fs::create_dir_all(output)
        .map_err(|_| "Synthetic output directory could not be created")?;
    let result_path = output.join("result.json");
    if result_path.exists() {
        return Err("Synthetic result already exists; use a new output directory to preserve previous evidence".into());
    }
    let requested_model = Path::new(&args[0])
        .canonicalize()
        .map_err(|_| "Synthetic model path was not found")?;
    // CI may select its private, pinned real Handy bundle explicitly. Refresh,
    // installed-model matching and native identity checks remain mandatory.
    let bridge = handy_bridge::HandyBridge::discover(args.get(5).map(Path::new))
        .map_err(|error| error.to_string())?;
    let catalog = bridge.refresh().map_err(|error| error.to_string())?;
    let mut selected = None;
    for model in catalog.installed_streaming_models() {
        if let Ok(resolved) = bridge.resolve_streaming_model(model) {
            if resolved.path.canonicalize().ok().as_ref() == Some(&requested_model) {
                selected = Some((model.clone(), resolved));
                break;
            }
        }
    }
    let (model, resolved) = selected.ok_or(
        "Synthetic model is not a currently installed streaming model advertised by Handy",
    )?;
    let store = SessionStore::create(
        output,
        "Synthetic native two-track streaming probe",
        &model.id,
        "generated-system",
        Some("generated-microphone"),
    )
    .map_err(|_| "Synthetic session could not be created")?;
    handy_bridge::write_model_snapshot(store.folder(), &model, &resolved, &serde_json::json!({"synthetic":true,"purpose":"owned two-track native streaming restart test"})).map_err(|error| error.to_string())?;
    for (source, input) in [("system", &args[2]), ("microphone", &args[3])] {
        let mut writer = meeting_core::PcmWriter::new(store.clone(), source)
            .map_err(|_| "Synthetic audio writer could not start")?;
        let mut offset = 0_u64;
        loop {
            if offset >= 16_000 * 300 {
                return Err("Synthetic fixture exceeds five minutes".into());
            }
            let pcm = read_pcm_samples(Path::new(input), offset, 16_000)
                .map_err(|_| "Synthetic fixture must be readable 16-kHz mono 16-bit PCM")?;
            if pcm.is_empty() {
                break;
            }
            let samples: Vec<i16> = pcm.iter().map(|value| (value * 32768.0) as i16).collect();
            writer
                .append(&samples)
                .map_err(|_| "Synthetic audio could not be stored")?;
            offset += samples.len() as u64;
            if samples.len() < 16_000 {
                break;
            }
        }
        writer
            .finish()
            .map_err(|_| "Synthetic audio could not be committed")?;
    }
    store
        .set_status("recorded")
        .map_err(|_| "Synthetic session metadata could not be saved")?;
    let before = store
        .snapshot()
        .map_err(|_| "Synthetic session could not be read")?;
    let expected: Vec<_> = before
        .parts
        .iter()
        .map(|part| {
            (
                part.file.clone(),
                part.confirmed_sample_count,
                part.confirmed_sha256.clone(),
            )
        })
        .collect();
    if expected.len() != 2 || expected[0].2 == expected[1].2 {
        return Err(
            "Synthetic two-track test requires two different short speech recordings".into(),
        );
    }
    let stop = Arc::new(AtomicBool::new(true));
    let injected = Arc::new(AtomicBool::new(false));
    let first_journal = Arc::new(Mutex::new(Vec::new()));
    let kill_checkpoint = Arc::new(Mutex::new(None));
    let restarts = Arc::new(AtomicUsize::new(0));
    // Keep the whole synthetic recognition/restart probe under five minutes.
    // Deadline-aware child RPC cleanup kills and reaps only its own worker.
    let deadline = Instant::now() + Duration::from_secs(240);
    let system = spawn_track_options(
        store.clone(),
        args[0].clone().into(),
        args[1].clone().into(),
        "system".into(),
        Arc::clone(&stop),
        TrackRunOptions {
            deadline: Some(deadline),
            force_cpu: true,
            inject_kill: Some(Arc::clone(&injected)),
            first_journal: Some(Arc::clone(&first_journal)),
            kill_checkpoint: Some(Arc::clone(&kill_checkpoint)),
            restarts: Some(Arc::clone(&restarts)),
        },
    );
    let microphone = spawn_track_options(
        store.clone(),
        args[0].clone().into(),
        args[1].clone().into(),
        "microphone".into(),
        stop,
        TrackRunOptions {
            deadline: Some(deadline),
            force_cpu: true,
            ..TrackRunOptions::default()
        },
    );
    system
        .join()
        .map_err(|_| "Synthetic system recognizer thread failed")?;
    microphone
        .join()
        .map_err(|_| "Synthetic microphone recognizer thread failed")?;
    let completed = store
        .snapshot()
        .map_err(|_| "Synthetic completed session unavailable")?;
    let journal =
        std::fs::read(store.folder().join("transcript-journal.jsonl")).unwrap_or_default();
    let prefix = first_journal
        .lock()
        .map_err(|_| "Synthetic journal prefix unavailable")?;
    let prefix_preserved = !prefix.is_empty() && journal.starts_with(&prefix);
    let killed_checkpoint = kill_checkpoint
        .lock()
        .map_err(|_| "Synthetic kill checkpoint unavailable")?
        .clone();
    let folder = store.folder().to_owned();
    drop(store);
    let recovered = SessionStore::open(&folder)
        .map_err(|_| "Synthetic confirmed audio failed reopen verification")?;
    let after = recovered
        .snapshot()
        .map_err(|_| "Synthetic reopened session unavailable")?;
    let hashes_preserved = expected.iter().all(|(file, count, digest)| {
        after.parts.iter().any(|part| {
            &part.file == file
                && &part.confirmed_sample_count == count
                && &part.confirmed_sha256 == digest
        })
    });
    let all_complete = after.parts.iter().all(|part| {
        part.status == "complete"
            && part
                .text
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
    });
    let text_matches = after.parts.iter().all(|part| {
        completed
            .parts
            .iter()
            .any(|old| old.file == part.file && old.text == part.text)
    });
    let success = text_matches
        && hashes_preserved
        && all_complete
        && prefix_preserved
        && injected.load(Ordering::Acquire)
        && killed_checkpoint.is_some()
        && restarts.load(Ordering::Acquire) >= 1;
    let result = serde_json::json!({
        "success": success,
        "version": env!("CARGO_PKG_VERSION"),
        "synthetic": true,
        "realtime": false,
        "openedAudioDevices": false,
        "backend": "CPU",
        "elapsedSeconds": started.elapsed().as_secs_f64(),
        "intentionalOwnedAsrKill": injected.load(Ordering::Acquire),
        "killTextWasTentative": killed_checkpoint.as_ref().and_then(|checkpoint| checkpoint.get("textWasTentative")).and_then(serde_json::Value::as_bool),
        "killCheckpoint": killed_checkpoint,
        "preservedJournalPrefixBytes": prefix.len(),
        "systemRestarts": restarts.load(Ordering::Acquire),
        "confirmedAudioHashesPreserved": hashes_preserved,
        "confirmedTextJournalPrefixPreserved": prefix_preserved,
        "bothTranscriptsComplete": all_complete,
        "reopenedTextMatches": text_matches,
        "parts": after.parts,
        "warnings": after.warnings,
        "limits": "Short accelerated synthetic speech probe. No live device, real-time latency, long-session memory or power-loss claim. Model retained separately per worker; process RAM not measured here."
    });
    let bytes =
        serde_json::to_vec_pretty(&result).map_err(|_| "Synthetic result could not be encoded")?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(result_path)
        .map_err(|_| "Synthetic result could not be created")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Synthetic result could not be committed")?;
    println!("{}", String::from_utf8_lossy(&bytes));
    if !success {
        return Err("Synthetic native streaming/restart verification failed; result.json retains the evidence".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feed_rejects_unconfirmed_or_nonsequential_audio() {
        assert!(validate_feed(0, 0, 8000, 8000).is_ok());
        assert!(validate_feed(0, 0, 8001, 9000).is_err());
        assert!(validate_feed(0, 0, 8000, 7999).is_err());
        assert!(validate_feed(2, 0, 1, 3).is_err());
        assert!(validate_feed(u64::MAX, u64::MAX, 1, u64::MAX).is_err());
    }
    #[test]
    fn protocol_rejects_oversized_unterminated_and_unknown_requests() {
        assert!(read_request(&mut &b"{\"operation\":\"quit\"}\n"[..])
            .unwrap()
            .is_some());
        assert!(read_request(&mut &b"{\"operation\":\"quit\"}"[..]).is_err());
        assert!(read_request(&mut &b"{\"operation\":\"execute\"}\n"[..]).is_err());
        assert!(read_request(&mut &vec![b'x'; MAX_REQUEST + 1][..]).is_err());
    }
    #[test]
    fn worker_rejects_cross_track_and_path_traversal_names() {
        assert!(worker_audio_path(Path::new("unused"), "../secret.wav", "system").is_err());
        assert!(
            worker_audio_path(Path::new("unused"), "microphone-000000000000.wav", "system")
                .is_err()
        );
    }

    #[test]
    fn restart_replay_retains_nonempty_twenty_second_checkpoint_until_caught_up() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.build/asr-replay-regressions");
        let store = SessionStore::create(
            &root,
            "Synthetic restart replay regression",
            "m",
            "synthetic",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        for _ in 0..20 {
            writer.append(&vec![42_i16; 16000]).unwrap();
        }
        writer.finish().unwrap();
        let part = store.snapshot().unwrap().parts[0].clone();
        assert!(persist(
            &store,
            &part,
            "m",
            Reply {
                ok: true,
                full: "Confirmed twenty seconds.".into(),
                committed: "Confirmed twenty seconds.".into(),
                ..Reply::default()
            },
            320000,
            false,
            false,
        )
        .unwrap());
        let saved = store.snapshot().unwrap().parts[0].clone();
        let journal = std::fs::read(store.folder().join("transcript-journal.jsonl")).unwrap();
        // Actual replay persistence path receives an earlier half-second hypothesis.
        // It is skipped, also as a restart replay, and nothing is written.
        assert!(!persist(
            &store,
            &saved,
            "m",
            Reply {
                ok: true,
                full: "partial replay".into(),
                ..Reply::default()
            },
            8000,
            false,
            true,
        )
        .unwrap());
        assert_eq!(
            store.snapshot().unwrap().parts[0].text.as_deref(),
            Some("Confirmed twenty seconds.")
        );
        assert_eq!(
            std::fs::read(store.folder().join("transcript-journal.jsonl")).unwrap(),
            journal
        );
        let folder = store.folder().to_owned();
        drop(writer);
        drop(store);
        let reopened = SessionStore::open(&folder).unwrap();
        assert_eq!(
            reopened.snapshot().unwrap().parts[0].processed_samples,
            320000
        );
        assert_eq!(
            reopened.snapshot().unwrap().parts[0].text.as_deref(),
            Some("Confirmed twenty seconds.")
        );
        let saved = reopened.snapshot().unwrap().parts[0].clone();
        // This fixture was never stopped, so reopen already reports an interruption.
        let incomplete = reopened.snapshot().unwrap().capture_incomplete;
        let diverged = || Reply {
            ok: true,
            full: "Diverged text.".into(),
            committed: "Diverged text.".into(),
            ..Reply::default()
        };
        // Outside a restart replay a changed confirmed prefix is still rejected.
        assert!(persist(&reopened, &saved, "m", diverged(), 320000, false, false).is_err());
        assert_eq!(
            reopened.snapshot().unwrap().parts[0].text.as_deref(),
            Some("Confirmed twenty seconds.")
        );
        assert_eq!(
            std::fs::read(folder.join("transcript-journal.jsonl")).unwrap(),
            journal
        );
        // A caught-up restart replay with diverging text is kept as a marked new version.
        assert!(persist(&reopened, &saved, "m", diverged(), 320000, false, true).unwrap());
        let revised = reopened.snapshot().unwrap();
        assert_eq!(revised.parts[0].text.as_deref(), Some("Diverged text."));
        assert_eq!(revised.parts[0].processed_samples, 320000);
        // The revision notice is informational and never marks the capture incomplete.
        assert_eq!(revised.capture_incomplete, incomplete);
        assert!(revised
            .warnings
            .iter()
            .any(|warning| warning.contains("earlier text is kept")));
        let after = std::fs::read(folder.join("transcript-journal.jsonl")).unwrap();
        assert!(after.starts_with(&journal));
        let tail = String::from_utf8(after[journal.len()..].to_vec()).unwrap();
        assert!(tail.contains("\"revisionAfterRestart\":true"));
        assert!(tail.contains("Diverged text."));
    }

    /// Synthetic in-process recognizer standing in for the native worker (#20). It
    /// decodes nothing: each half-second packet adds one word naming its backend, so a
    /// CPU replay of a GPU-recognized prefix diverges deterministically.
    struct SyntheticRecognizer {
        word: &'static str,
        fed: u64,
        fail_at: Option<u64>,
    }
    impl SyntheticRecognizer {
        fn text(&self) -> Reply {
            let words = vec![self.word; (self.fed / FEED_SAMPLES as u64) as usize].join(" ");
            Reply {
                ok: true,
                full: words.clone(),
                committed: words,
                ..Reply::default()
            }
        }
    }
    impl Recognizer for SyntheticRecognizer {
        fn request(&mut self, request: Request) -> Result<Reply, String> {
            match request {
                Request::Begin { .. } => {
                    self.fed = 0;
                    Ok(Reply {
                        ok: true,
                        ..Reply::default()
                    })
                }
                Request::Feed { sample_count, .. } => {
                    if self.fail_at.is_some_and(|limit| self.fed >= limit) {
                        return Err("Synthetic GPU device was lost".into());
                    }
                    self.fed += sample_count as u64;
                    Ok(self.text())
                }
                Request::Finalize => Ok(self.text()),
                _ => Err("Synthetic recognizer received an unexpected request".into()),
            }
        }
        fn terminate(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    /// Synthetic, accelerated regression for #20: no native model, no device, no GPU.
    /// The first GPU stream confirms text for one second and fails, the fresh GPU
    /// process cannot start, and the CPU replacement replays the part with different
    /// confirmed text. Before the fix the CPU checkpoint was rejected and the part
    /// stayed pending forever.
    #[test]
    fn synthetic_cpu_replay_with_diverging_text_completes_as_new_version() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.build/asr-replay-regressions");
        let store = SessionStore::create(
            &root,
            "Synthetic GPU to CPU replay regression",
            "m",
            "synthetic",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&vec![42_i16; 32000]).unwrap();
        writer.finish().unwrap();
        drop(writer);
        store.set_status("recorded").unwrap();
        let folder = store.folder().to_owned();
        let starts = Arc::new(AtomicUsize::new(0));
        let journal_before_cpu = Arc::new(Mutex::new(Vec::new()));
        let (count, before_cpu) = (Arc::clone(&starts), Arc::clone(&journal_before_cpu));
        let handle = spawn_track_with(
            store.clone(),
            "system".into(),
            Arc::new(AtomicBool::new(true)),
            TrackRunOptions::default(),
            move |folder: &Path, cpu_only| match (count.fetch_add(1, Ordering::AcqRel), cpu_only) {
                (_, true) => {
                    *before_cpu.lock().unwrap() =
                        std::fs::read(folder.join("transcript-journal.jsonl")).unwrap();
                    Ok(SyntheticRecognizer {
                        word: "cpu",
                        fed: 0,
                        fail_at: None,
                    })
                }
                (0, false) => Ok(SyntheticRecognizer {
                    word: "gpu",
                    fed: 0,
                    fail_at: Some(16000),
                }),
                _ => Err("Synthetic GPU recognition process could not start".into()),
            },
        );
        handle.join().unwrap();
        assert_eq!(starts.load(Ordering::Acquire), 3);
        let done = store.snapshot().unwrap();
        let part = &done.parts[0];
        assert_eq!(part.status, "complete");
        assert_eq!(done.status, "complete");
        assert_eq!(part.text.as_deref(), Some("cpu cpu cpu cpu"));
        assert_eq!(part.processed_samples, 32000);
        assert!(!done.capture_incomplete);
        let notices: Vec<_> = done
            .warnings
            .iter()
            .filter(|warning| warning.contains("earlier text is kept"))
            .collect();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].starts_with("system 00:00:00:"));
        // The GPU version stays byte-identical in the append-only journal.
        let prefix = journal_before_cpu.lock().unwrap().clone();
        assert!(String::from_utf8_lossy(&prefix).contains("\"committed\":\"gpu gpu\""));
        let journal = std::fs::read(folder.join("transcript-journal.jsonl")).unwrap();
        assert!(journal.starts_with(&prefix));
        let added: Vec<serde_json::Value> = String::from_utf8(journal[prefix.len()..].to_vec())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        // Only the first caught-up CPU checkpoint carries the marker.
        let marked: Vec<_> = added
            .iter()
            .map(|line| line.get("revisionAfterRestart").is_some())
            .collect();
        assert_eq!(marked, [true, false, false, false]);
        assert_eq!(added[0]["committed"], "cpu cpu");
        assert_eq!(added[0]["processedSamples"], 16000);
        drop(store);
        let reopened = SessionStore::open(&folder).unwrap().snapshot().unwrap();
        assert_eq!(reopened.parts[0].text.as_deref(), Some("cpu cpu cpu cpu"));
        assert_eq!(reopened.parts[0].status, "complete");
        assert!(!reopened.capture_incomplete);
    }
}
