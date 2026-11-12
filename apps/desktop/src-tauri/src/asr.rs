//! Owned native recognition workers. Streaming uses one per track, batch shares one.
use handy_bridge::native::{default_run_options, native_error, NativeEngine};
use handy_bridge::BridgeError;
use meeting_core::{
    continues_confirmed_text, read_pcm_samples, AudioPart, SessionStore, TranscriptCheckpoint,
};
use meeting_notes::AnalysisControl;
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
const BATCH_CONTEXT_SAMPLES: usize = meeting_core::SAMPLE_RATE as usize;
const MAX_BATCH_SAMPLES: usize = meeting_core::SAMPLE_RATE as usize * 30;

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
    BatchSection {
        part: AudioPart,
        #[serde(default)]
        previous: Option<Box<AudioPart>>,
    },
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
    pub overlap_words: usize,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// Map protocol failures to a content-free error while retaining saved session audio.
fn bridge_protocol(_: impl std::fmt::Display) -> BridgeError {
    BridgeError::Native(
        "Recognition worker protocol or confirmed audio was invalid. Saved audio is retained."
            .into(),
    )
}

/// Args are the values after `--asr-worker`: model, backend directory, session
/// directory, track source, and optional `--cpu`. This function opens no device.
pub fn worker_main(args: &[String]) -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    let (engine, folder, batch) = initialize_worker(args, &mut output)?;
    let source = &args[3];
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
            Request::BatchSection { part, previous } if batch => {
                let result = batch_section(&folder, &part, previous.as_deref(), |pcm, context| {
                    engine
                        .transcribe_batch(pcm, context)
                        .map_err(|error| error.to_string())
                });
                reply_to_batch(&mut output, result)?;
            }

            Request::Begin { file } if !batch => {
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

/// Send controlled model/identity/argument failures before the ready handshake,
/// so the supervisor retries only an actual unexplained process interruption.
fn initialize_worker(
    args: &[String],
    output: &mut impl Write,
) -> Result<(NativeEngine, PathBuf, bool), String> {
    let result = (|| -> Result<_, String> {
        if !(4..=5).contains(&args.len())
            || !["system", "microphone", "batch"].contains(&args[3].as_str())
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
        let identity = handy_bridge::verify_model_snapshot(&folder, model_path)
            .map_err(|error| error.to_string())?;
        let batch = source == "batch";
        if batch != identity.source_model_metadata.ready_for_batch() {
            return Err("Recognition mode differs from the saved model identity".into());
        }
        let engine = if batch {
            NativeEngine::load_batch(model_path, args.len() == 5, Some(backend_dir))
        } else {
            NativeEngine::load(model_path, args.len() == 5, Some(backend_dir))
        }
        .map_err(|error| error.to_string())?;
        Ok((engine, folder, batch))
    })();
    if let Err(error) = &result {
        let _ = send_reply(
            output,
            Reply {
                ok: false,
                error: Some(error.clone()),
                ..Reply::default()
            },
        );
    }
    result
}

/// Verify closed audio and same-track context before granting PCM to native batch ASR.
fn batch_section(
    folder: &Path,
    part: &AudioPart,
    previous: Option<&AudioPart>,
    transcribe: impl FnOnce(&[f32], usize) -> Result<(String, usize), String>,
) -> Result<Reply, String> {
    batch_sample_count(part.confirmed_sample_count)?;
    let mut pcm = Vec::new();
    if let Some(previous) = previous {
        if previous.source != part.source
            || previous.start_sample >= part.start_sample
            || previous
                .start_sample
                .checked_add(previous.confirmed_sample_count)
                != Some(part.start_sample)
        {
            return Err("Batch context is not contiguous on this track".into());
        }
        batch_sample_count(previous.confirmed_sample_count)?;
        let preceding = meeting_core::read_closed_audio(folder, previous)
            .map_err(|_| "Batch context failed verification")?;
        pcm.extend_from_slice(&preceding[preceding.len().saturating_sub(BATCH_CONTEXT_SAMPLES)..]);
    }
    let context_samples = pcm.len();
    pcm.extend(
        meeting_core::read_closed_audio(folder, part)
            .map_err(|_| "Closed audio did not pass batch verification")?,
    );
    let (text, overlap_words) = transcribe(&pcm, context_samples)?;
    Ok(Reply {
        ok: true,
        committed: text.clone(),
        full: text,
        overlap_words,
        ..Reply::default()
    })
}

/// Send explicit permanent failures instead of misclassifying a controlled exit as process loss.
fn reply_to_batch(output: &mut impl Write, result: Result<Reply, String>) -> Result<(), String> {
    match result {
        Ok(reply) => send_reply(output, reply),
        Err(error) => {
            let _ = send_reply(
                output,
                Reply {
                    ok: false,
                    error: Some(error.clone()),
                    ..Reply::default()
                },
            );
            Err(error)
        }
    }
}

/// Convert a confirmed section length only within the fixed batch-audio safety limit.
fn batch_sample_count(samples: u64) -> Result<usize, String> {
    usize::try_from(samples)
        .ok()
        .filter(|count| (1..=MAX_BATCH_SAMPLES).contains(count))
        .ok_or_else(|| "Closed batch audio section exceeds its safe limit".into())
}

/// Reject nonsequential, oversized or unconfirmed streaming audio reads.
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

/// Resolve only a regular audio filename belonging to the expected track and session root.
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

/// Read one bounded protocol request, distinguishing clean EOF from malformed input.
pub(super) fn read_request(reader: &mut impl BufRead) -> Result<Option<Request>, String> {
    let bytes = bounded_line(reader, MAX_REQUEST)?;
    bytes
        .map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|_| "Invalid recognition request JSON".into())
        })
        .transpose()
}

/// Read a line without allowing an unterminated protocol message to allocate without limit.
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

/// Serialize a bounded worker reply and flush it to the supervising process.
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
    transient_failure: std::cell::Cell<bool>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    absolute_deadline: Option<Instant>,
    kind: WorkerKind,
}
enum WorkerKind {
    Asr,
    Speaker(Arc<AtomicBool>),
    Setup(AnalysisControl),
}

impl WorkerKind {
    fn check_cancellation(&self) -> Result<(), String> {
        match self {
            Self::Speaker(flag) if flag.load(Ordering::Acquire) => {
                Err("Optional speaker analysis cancelled; recording and original transcripts are unchanged".into())
            }
            Self::Setup(control) => crate::setup_probe::check_cancellation(control),
            _ => Ok(()),
        }
    }
}

/// Exercise the same file and worker protocol on generated samples in a private
/// scratch directory, with no capture handle and no user session or recording.
/// The input is a test tone, so an empty transcript is a valid worker response.
pub(crate) fn setup_signal_probe(
    root: &Path,
    model: &handy_bridge::HandyModel,
    resolved: &handy_bridge::ResolvedModel,
    backend: &Path,
    control: &AnalysisControl,
) -> Result<(bool, bool), String> {
    struct OwnedScratch(PathBuf);
    impl Drop for OwnedScratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    crate::setup_probe::check_cancellation(control)?;
    let scratch = root.join(format!(".setup-probe-{}", meeting_notes::new_job_id()));
    std::fs::create_dir(&scratch).map_err(|_| "Die Testablage konnte nicht angelegt werden. / Could not create the setup scratch directory.")?;
    let scratch = OwnedScratch(scratch);
    let store = SessionStore::create(
        &scratch.0,
        "Synthetic setup probe",
        &model.id,
        "generated",
        None,
    )
    .map_err(|error| error.to_string())?;
    let samples: Vec<i16> = (0..meeting_core::SAMPLE_RATE as usize * 2)
        .map(|index| {
            if index < 8_000 {
                if index % 40 < 20 {
                    256
                } else {
                    -256
                }
            } else {
                0
            }
        })
        .collect();
    let mut writer =
        meeting_core::PcmWriter::new(store.clone(), "system").map_err(|error| error.to_string())?;
    writer
        .append(&samples)
        .and_then(|_| writer.finish())
        .map_err(|error| error.to_string())?;
    let snapshot = store.snapshot().map_err(|error| error.to_string())?;
    let part = snapshot
        .parts
        .first()
        .ok_or("Testabschnitt fehlt. / The test section is missing.")?;
    let pcm =
        meeting_core::read_closed_audio(store.folder(), part).map_err(|error| error.to_string())?;
    let audio = pcm.len() == samples.len()
        && pcm
            .iter()
            .zip(&samples)
            .all(|(read, original)| *read == f32::from(*original) / 32768.0);
    if !audio {
        return Ok((false, false));
    }
    handy_bridge::write_model_snapshot_cancellable(
        store.folder(),
        model,
        resolved,
        &serde_json::json!({"synthetic":true,"purpose":"setup check"}),
        || control.is_cancelled(),
    )
    .map_err(|error| error.to_string())?;
    let batch = model.ready_for_batch();
    let worker_check = (|| -> Result<(), String> {
        let mut worker = Worker::start_mode(
            &resolved.path,
            backend,
            store.folder(),
            if batch { "batch" } else { "system" },
            true,
            Some(Instant::now() + Duration::from_secs(90)),
            WorkerKind::Setup(control.clone()),
        )
        .map_err(|error| error.message)?;
        if batch {
            worker.call(Request::BatchSection {
                part: part.clone(),
                previous: None,
            })?;
        } else {
            worker.call(Request::Begin {
                file: part.file.clone(),
            })?;
            for offset in (0..samples.len()).step_by(FEED_SAMPLES) {
                worker.call(Request::Feed {
                    offset_samples: offset as u64,
                    sample_count: (samples.len() - offset).min(FEED_SAMPLES),
                    confirmed_samples: samples.len() as u64,
                })?;
            }
            worker.call(Request::Finalize)?;
        }
        Ok(())
    })();
    Ok((true, worker_check.is_ok()))
}

/// Preserve process-loss classification across the startup handshake without
/// turning model, protocol or integrity errors into automatic retries.
#[derive(Debug)]
struct StartFailure {
    message: String,
    transient: bool,
}
impl From<String> for StartFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            transient: false,
        }
    }
}
impl From<&str> for StartFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl Worker {
    /// Start an owned ASR process and retain the legacy caller-facing error text.
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
        .map_err(|error| error.message)
    }
    /// Start optional speaker analysis with its own cancellation flag and no capture ownership.
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
        .map_err(|error| error.message)
    }
    /// Build the owned worker command and retain typed startup failures through the ready handshake.
    fn start_mode(
        model: &Path,
        backend: &Path,
        folder: &Path,
        source: &str,
        cpu_only: bool,
        absolute_deadline: Option<Instant>,
        kind: WorkerKind,
    ) -> Result<Self, StartFailure> {
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
        Self::start_command(command, kind, absolute_deadline, Duration::from_secs(180))
    }

    /// Supervise one owned process and validate its initial reply before exposing it.
    /// The shorter deadline in tests exercises the actual pipe/handshake path.
    fn start_command(
        mut command: Command,
        kind: WorkerKind,
        absolute_deadline: Option<Instant>,
        ready_timeout: Duration,
    ) -> Result<Self, StartFailure> {
        kind.check_cancellation()?;
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
            transient_failure: std::cell::Cell::new(false),
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            absolute_deadline,
            kind,
        };
        worker
            .receive(ready_timeout)
            .map_err(|message| StartFailure {
                message,
                transient: worker.transient_failure.get(),
            })?;
        Ok(worker)
    }

    /// Wait for a worker reply with bounded diagnostics and explicit transient process-loss classification.
    fn receive(&self, timeout: Duration) -> Result<Reply, String> {
        let deadline = self.absolute_deadline.map_or_else(
            || Instant::now() + timeout,
            |end| end.min(Instant::now() + timeout),
        );
        loop {
            self.kind.check_cancellation()?;
            if Instant::now() >= deadline {
                self.transient_failure.set(true);
                return Err("Recognition worker deadline elapsed; saved audio is retained".into());
            }
            if self.protocol_fault.load(Ordering::Acquire) {
                return Err(
                    "Recognition worker exceeded its protocol or diagnostic output limit".into(),
                );
            }
            match self.replies.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => {
                    if result
                        .as_ref()
                        .is_err_and(|error| error == "Recognition process exited before its reply")
                    {
                        self.transient_failure.set(true);
                    }
                    let reply = result?;
                    if !reply.ok {
                        return Err(reply.error.unwrap_or_else(|| {
                            "Recognition worker rejected the operation".into()
                        }));
                    }
                    return Ok(reply);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.transient_failure.set(true);
                    return Err("Recognition reply channel closed".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                    self.transient_failure.set(true);
                    return Err(
                        "Recognition worker exceeded its time limit; saved audio is retained"
                            .into(),
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Send one request and read its reply; protocol/provider/storage failures are not treated as transport loss.
    pub(super) fn call(&mut self, request: Request) -> Result<Reply, String> {
        self.transient_failure.set(false);
        self.kind.check_cancellation()?;
        let timeout = if matches!(
            &request,
            Request::SpeakerIdentity { .. } | Request::BatchSection { .. }
        ) {
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
            .map_err(|_| {
                self.transient_failure.set(true);
                "Recognition child input closed"
            })?;
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

/// A single model processes closed sections from both tracks in time order.
pub(crate) fn spawn_batch(
    store: SessionStore,
    model: PathBuf,
    backend: PathBuf,
    stop: Arc<AtomicBool>,
) -> RecognitionWorker {
    let handle = thread::spawn(move || {
        let result = retry_batch(
            || {
                let mut worker = None;
                let mut retryable = false;
                let mut progressed = false;
                let attempt_result = run_batch(&store, &stop, &mut progressed, |part, previous| {
                    let worker = match &mut worker {
                        Some(worker) => worker,
                        slot @ None => match Worker::start_mode(
                            &model,
                            &backend,
                            store.folder(),
                            "batch",
                            true,
                            None,
                            WorkerKind::Asr,
                        ) {
                            Ok(worker) => slot.insert(worker),
                            Err(error) => {
                                retryable = error.transient;
                                return Err(error.message);
                            }
                        },
                    };
                    let audio_only = |part: &AudioPart| {
                        let mut audio = part.clone();
                        audio.text = None;
                        audio.committed_text.clear();
                        audio.tentative_text.clear();
                        audio.error = None;
                        audio
                    };
                    let reply = worker.call(Request::BatchSection {
                        part: audio_only(part),
                        previous: previous.map(|part| Box::new(audio_only(part))),
                    });
                    retryable = worker.transient_failure.get();
                    reply
                });
                drop(worker);
                (attempt_result, retryable, progressed)
            },
            || {
                let _ = store.notice("Der Batch-Erkennungsprozess wurde unterbrochen und wird erneut gestartet. Gesicherte Abschnitte bleiben erhalten. / Batch recognition is restarting; saved sections are retained.");
                thread::sleep(Duration::from_millis(500));
            },
        );
        if let Err(error) = result {
            let _ = store.notice(&format!("Batch transcription stopped: {error}. Saved audio and text are retained. Retry from history."));
        }
    });
    RecognitionWorker {
        source: "batch".into(),
        handle,
    }
}

/// Retry at most two consecutive transport/process losses without durable progress.
/// Provider rejections, bad protocol and storage failures never qualify for retry.
fn retry_batch(
    mut attempt: impl FnMut() -> (Result<(), String>, bool, bool),
    mut before_retry: impl FnMut(),
) -> Result<(), String> {
    let mut failures = 0;
    loop {
        let (result, transient, progressed) = attempt();
        if result.is_ok() || !transient {
            return result;
        }
        if progressed {
            failures = 0;
        }
        failures += 1;
        if failures == 3 {
            return result;
        }
        before_retry();
    }
}

/// Process closed track heads in time order, publishing each result before advancing the durable cursor.
fn run_batch(
    store: &SessionStore,
    stop: &AtomicBool,
    progressed: &mut bool,
    mut transcribe: impl FnMut(&AudioPart, Option<&AudioPart>) -> Result<Reply, String>,
) -> Result<(), String> {
    let model = store.model_id().map_err(|_| "Saved session unavailable")?;
    loop {
        // Acquire the flag before reading the track heads. Once stop is visible,
        // capture has closed and journaled its final sections.
        let stopped = stop.load(Ordering::Acquire);
        let mut heads = Vec::with_capacity(2);
        for track in ["system", "microphone"] {
            if let Some(part) = store
                .next_pending_part(track, &model)
                .map_err(|_| "Saved session unavailable")?
            {
                heads.push(part);
            }
        }
        let pending = heads
            .iter()
            .filter(|part| part.audio_closed)
            .min_by_key(|part| part.start_sample);
        let Some(part) = pending else {
            if stopped {
                return if heads.is_empty() {
                    Ok(())
                } else {
                    Err("An audio section is still open; saved work is retained".into())
                };
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        };
        let samples = part.confirmed_sample_count;
        batch_sample_count(samples)?;
        let reply = if part.has_signal {
            let previous = store
                .previous_recognized_part(part, &model)
                .map_err(|_| "Previous audio section unavailable")?
                .filter(|previous| previous.has_signal);
            let mut reply = transcribe(part, previous.as_ref())?;
            if let Some(text) = previous.as_ref().and_then(|part| part.text.as_deref()) {
                reply.full = trim_batch_overlap(text, &reply.full, reply.overlap_words);
                reply.committed.clone_from(&reply.full);
            }
            reply
        } else {
            Reply {
                ok: true,
                ..Reply::default()
            }
        };
        persist(store, part, &model, reply, samples, true, false)?;
        *progressed = true;
    }
}

// Only drop a strong, bounded suffix/prefix match. With weak matches keep
// the recognized text so an uncertain stitch cannot silently delete speech.
/// Remove only a strong bounded overlap supported by context; retain uncertain repeated speech.
fn trim_batch_overlap(previous: &str, current: &str, overlap_words: usize) -> String {
    let word = |value: &str| {
        value
            .trim_matches(|ch: char| !ch.is_alphanumeric())
            .to_lowercase()
    };
    let prior: Vec<_> = previous
        .split_whitespace()
        .rev()
        .take(5)
        .map(word)
        .collect();
    let prefix: Vec<_> = current
        .split_whitespace()
        .take(5.min(overlap_words))
        .map(word)
        .collect();
    let overlap = (2..=prior.len().min(prefix.len()))
        .rev()
        .find(|&count| {
            prefix[..count]
                .iter()
                .zip(prior[..count].iter().rev())
                .all(|(left, right)| !left.is_empty() && left == right)
        })
        .unwrap_or(0);
    if overlap == 0 {
        return current.into();
    }
    let mut remainder = current;
    for _ in 0..overlap {
        remainder = remainder.trim_start();
        remainder = remainder
            .find(char::is_whitespace)
            .map_or("", |at| &remainder[at..]);
    }
    remainder.trim_start().into()
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

/// Launch one recognition track with its configured replay and backend options.
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

/// Supervise a track recognizer, restarting only within the bounded recovery policy.
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

/// Read only confirmed sequential audio and persist recognition checkpoints independently of capture.
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

    /// Own only a newly created synthetic root; preserve failed fixtures and all older roots.
    struct BatchFixture {
        root: PathBuf,
        parent: PathBuf,
    }
    impl BatchFixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "handymeeting-batch-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            Self { root, parent }
        }
    }
    impl Drop for BatchFixture {
        fn drop(&mut self) {
            if std::thread::panicking() {
                return;
            }
            // Never follow a replaced root or leave the verified temporary parent.
            assert_eq!(self.root.parent(), Some(self.parent.as_path()));
            assert_eq!(self.root.canonicalize().unwrap(), self.root);
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn batch_closes_both_tracks_and_feeds_live_text_while_capture_continues() {
        use std::sync::mpsc::sync_channel;
        let fixture = BatchFixture::new();
        let root = fixture.root.clone();
        let store = SessionStore::create(
            &root,
            "Synthetic live batch",
            handy_bridge::PARAKEET_ULTRA_ID,
            "Output",
            Some("Mic"),
        )
        .unwrap()
        .with_quiet_sections();
        let mut system = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        let mut mic = meeting_core::PcmWriter::new(store.clone(), "microphone").unwrap();
        let loud = vec![3000_i16; 16_000];
        let quiet = vec![0_i16; 16_000];
        for _ in 0..20 {
            system.append(&loud).unwrap();
        }
        // Split the quiet period across packets to exercise its sample counter.
        system.append(&quiet[..3500]).unwrap();
        system.append(&quiet[3500..]).unwrap();
        for _ in 0..30 {
            mic.append(&loud).unwrap();
        }
        let (entered, events) = sync_channel(1);
        let (release, gate) = sync_channel(0);
        let stop = AtomicBool::new(false);
        struct StopOnDrop<'a>(&'a AtomicBool);
        impl Drop for StopOnDrop<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        thread::scope(|scope| {
            let _stop_on_failure = StopOnDrop(&stop);
            let worker_store = &store;
            let worker_stop = &stop;
            let worker = scope.spawn(move || {
                run_batch(worker_store, worker_stop, &mut false, |part, previous| {
                    if part.start_sample == 0 {
                        entered
                            .send(part.source.clone())
                            .map_err(|e| e.to_string())?;
                        // This controller-owned latch deliberately holds the
                        // fake recognizer while capture writes proceed. It is
                        // not an inference deadline. StopOnDrop cancels the
                        // wait if the controller fails before releasing it.
                        loop {
                            match gate.recv_timeout(Duration::from_millis(100)) {
                                Ok(()) => break,
                                Err(mpsc::RecvTimeoutError::Timeout)
                                    if !worker_stop.load(Ordering::Acquire) => {}
                                Err(error) => return Err(error.to_string()),
                            }
                        }
                    }
                    let (text, overlap_words) = match (part.source.as_str(), previous.is_some()) {
                        ("system", false) => ("Send the report today.", 0),
                        ("system", true) => ("report today. Next subject.", 2),
                        ("microphone", false) => ("The budget is open.", 0),
                        _ => ("A new microphone section.", 0),
                    };
                    Ok(Reply {
                        full: text.into(),
                        committed: text.into(),
                        overlap_words,
                        ..Reply::default()
                    })
                })
            });
            let limit = Duration::from_secs(5);
            assert_eq!(events.recv_timeout(limit).unwrap(), "system");
            // The controller can be busy longer than the fixture's former
            // five-second gate. This is deliberate synthetic delay, not an
            // inference deadline or a physical capture performance assertion.
            thread::sleep(Duration::from_secs(6));
            // Writes progress even while the recognizer cannot complete its first section.
            system.append(&loud).unwrap();
            mic.append(&loud).unwrap();
            system.checkpoint().unwrap();
            mic.checkpoint().unwrap();
            assert_eq!(system.written_samples(), 22 * 16_000);
            assert_eq!(mic.written_samples(), 31 * 16_000);
            assert!(crate::live_notes::live_texts(&store.snapshot().unwrap())
                .0
                .is_empty());
            release.send(()).unwrap();
            assert_eq!(events.recv_timeout(limit).unwrap(), "microphone");
            let (texts, finished) = crate::live_notes::live_texts(&store.snapshot().unwrap());
            assert!(!finished);
            assert_eq!(texts.len(), 1);
            assert_eq!(texts[0].text, "Send the report today.");
            assert_eq!(texts[0].end_ms, 20_450);
            release.send(()).unwrap();
            let deadline = Instant::now() + limit;
            loop {
                let (texts, finished) = crate::live_notes::live_texts(&store.snapshot().unwrap());
                assert!(!finished);
                if texts.len() == 2 {
                    assert!(texts
                        .iter()
                        .any(|text| text.track == "microphone" && text.end_ms == 30_000));
                    break;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            system.finish().unwrap();
            mic.finish().unwrap();
            store.set_status("recorded").unwrap();
            stop.store(true, Ordering::Release);
            worker.join().unwrap().unwrap();
        });
        let snapshot = store.snapshot().unwrap();
        let (texts, _) = crate::live_notes::live_texts(&snapshot);
        assert_eq!(texts.len(), 4);
        assert!(texts.iter().any(|text| text.text == "Next subject."));
        for (track, expected) in [
            (
                "system",
                [loud.repeat(20), quiet.clone(), loud.clone()].concat(),
            ),
            ("microphone", loud.repeat(31)),
        ] {
            let mut parts: Vec<_> = snapshot
                .parts
                .iter()
                .filter(|part| part.source == track)
                .collect();
            parts.sort_by_key(|part| part.start_sample);
            let mut recorded = Vec::new();
            for part in parts {
                assert_eq!(part.start_sample, recorded.len() as u64);
                recorded.extend(
                    meeting_core::read_closed_audio(store.folder(), part)
                        .unwrap()
                        .into_iter()
                        .map(|sample| (sample * 32768.0) as i16),
                );
            }
            assert_eq!(recorded, expected);
        }
        assert_eq!(
            trim_batch_overlap("report today.", "report today. Again.", 0),
            "report today. Again."
        );
    }

    #[test]
    fn batch_retries_only_unfinished_sections() {
        let fixture = BatchFixture::new();
        let dir = fixture.root.clone();
        let store = SessionStore::create(
            &dir,
            "Synthetic batch",
            handy_bridge::PARAKEET_ULTRA_ID,
            "Output",
            Some("Mic"),
        )
        .unwrap();
        for source in ["system", "microphone"] {
            let mut writer = meeting_core::PcmWriter::new(store.clone(), source).unwrap();
            writer.append(&vec![3000_i16; 16_000]).unwrap();
            writer.finish().unwrap();
        }
        store.set_status("recorded").unwrap();
        let original = store.snapshot().unwrap();
        let mut seen = Vec::new();
        let mut attempts = 0;
        retry_batch(
            || {
                attempts += 1;
                let mut progressed = false;
                let result = run_batch(
                    &store,
                    &AtomicBool::new(true),
                    &mut progressed,
                    |part, _| {
                        seen.push(part.file.clone());
                        if seen.len() == 2 {
                            return Err("synthetic pipe failure".into());
                        }
                        Ok(Reply {
                            full: if seen.len() == 1 {
                                "First track."
                            } else {
                                "Second track."
                            }
                            .into(),
                            committed: if seen.len() == 1 {
                                "First track."
                            } else {
                                "Second track."
                            }
                            .into(),
                            ..Reply::default()
                        })
                    },
                );
                (result, true, progressed)
            },
            || {
                let saved = store.snapshot().unwrap();
                assert_eq!(
                    saved
                        .parts
                        .iter()
                        .filter(|part| part.status == "complete")
                        .count(),
                    1
                );
            },
        )
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[1], seen[2]);
        assert_ne!(seen[0], seen[2]);
        let resumed = store.snapshot().unwrap();
        assert!(resumed.parts.iter().all(|part| part.status == "complete"));
        for part in &resumed.parts {
            let old = original
                .parts
                .iter()
                .find(|old| old.file == part.file)
                .unwrap();
            assert_eq!(old.confirmed_sha256, part.confirmed_sha256);
            if part.file == seen[0] {
                assert_eq!(part.text.as_deref(), Some("First track."));
            }
        }
    }

    #[test]
    fn automatic_batch_retry_is_bounded_and_does_not_retry_permanent_errors() {
        let mut calls = 0;
        let mut restarts = 0;
        retry_batch(
            || {
                calls += 1;
                (
                    if calls == 1 {
                        Err("pipe closed".into())
                    } else {
                        Ok(())
                    },
                    true,
                    false,
                )
            },
            || restarts += 1,
        )
        .unwrap();
        assert_eq!((calls, restarts), (2, 1));
        calls = 0;
        assert!(retry_batch(
            || {
                calls += 1;
                (Err("no response".into()), true, false)
            },
            || {}
        )
        .is_err());
        assert_eq!(calls, 3);
        calls = 0;
        assert!(retry_batch(
            || {
                calls += 1;
                (Err("invalid audio hash".into()), false, false)
            },
            || panic!("must not restart")
        )
        .is_err());
        assert_eq!(calls, 1);
        calls = 0;
        assert!(retry_batch(
            || {
                calls += 1;
                (Err("invalid hash after saved progress".into()), false, true)
            },
            || panic!("permanent failures must not restart after progress")
        )
        .is_err());
        assert_eq!(calls, 1);
        calls = 0;
        assert!(retry_batch(
            || {
                calls += 1;
                (Err("later stalled section".into()), true, calls == 2)
            },
            || {}
        )
        .is_err());
        assert_eq!(calls, 4); // The progressed attempt starts a fresh three-attempt section budget.
    }

    #[test]
    fn batch_recovers_separate_crashes_after_durable_progress_on_both_tracks() {
        let fixture = BatchFixture::new();
        let store = SessionStore::create(
            &fixture.root,
            "Synthetic progressing recovery",
            handy_bridge::PARAKEET_ULTRA_ID,
            "Output",
            Some("Mic"),
        )
        .unwrap()
        .with_quiet_sections();
        for source in ["system", "microphone"] {
            let mut writer = meeting_core::PcmWriter::new(store.clone(), source).unwrap();
            for _ in 0..61 {
                writer.append(&vec![3000_i16; 16_000]).unwrap();
            }
            writer.finish().unwrap();
        }
        store.set_status("recorded").unwrap();
        let original = store.snapshot().unwrap();
        assert_eq!(original.parts.len(), 6);
        let mut attempts = 0;
        let mut completed = 0;
        retry_batch(
            || {
                attempts += 1;
                let mut calls = 0;
                let mut progressed = false;
                let result = run_batch(
                    &store,
                    &AtomicBool::new(true),
                    &mut progressed,
                    |part, _| {
                        calls += 1;
                        if calls == 2 {
                            return Err("synthetic unrelated pipe loss".into());
                        }
                        let text = format!("Saved {} section {}.", part.source, part.start_sample);
                        Ok(Reply {
                            ok: true,
                            full: text.clone(),
                            committed: text,
                            ..Reply::default()
                        })
                    },
                );
                (result, true, progressed)
            },
            || {
                let now = store
                    .snapshot()
                    .unwrap()
                    .parts
                    .iter()
                    .filter(|part| part.status == "complete")
                    .count();
                assert!(now > completed);
                completed = now;
            },
        )
        .unwrap();
        assert_eq!(attempts, 6);
        let folder = store.folder().to_path_buf();
        drop(store);
        let reopened = SessionStore::open(&folder).unwrap().snapshot().unwrap();
        for part in &reopened.parts {
            assert_eq!(part.status, "complete");
            let prior = original
                .parts
                .iter()
                .find(|old| old.file == part.file)
                .unwrap();
            assert_eq!(part.confirmed_sha256, prior.confirmed_sha256);
            assert!(part.text.as_deref().unwrap().starts_with("Saved "));
        }
        assert_eq!(crate::live_notes::live_texts(&reopened).0.len(), 6);
    }

    fn protocol_test_child(script: &str) -> Command {
        #[cfg(windows)]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
            command.creation_flags(0x08000000);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    // These cases test protocol classification, not shell launch latency. Use the
    // same ready budget as production; the dedicated stalled-start case stays at 2s.
    const PROTOCOL_FIXTURE_READY_TIMEOUT: Duration = Duration::from_secs(180);

    #[test]
    fn setup_worker_cancellation_reaps_its_pending_request_before_recording_admission() {
        let gate = crate::operations::Gate::default();
        let lease = gate
            .try_acquire(crate::operations::Kind::SetupProbe)
            .unwrap();
        let control = lease.setup_control().unwrap().clone();
        let scratch = BatchFixture::new();
        let received = scratch.root.join("request-received");
        #[cfg(windows)]
        let script = r#"[Console]::Out.WriteLine('{"ok":true}');$null=[Console]::In.ReadLine();[IO.File]::WriteAllText($env:HANDYMEETING_TEST_SETUP_REQUEST,'');$null=[Console]::In.ReadLine()"#;
        #[cfg(not(windows))]
        let script = r#"printf '%s\n' '{"ok":true}'; IFS= read -r line; : > "$HANDYMEETING_TEST_SETUP_REQUEST"; IFS= read -r line"#;
        let mut command = protocol_test_child(script);
        command.env("HANDYMEETING_TEST_SETUP_REQUEST", &received);
        let mut worker = Worker::start_command(
            command,
            WorkerKind::Setup(control),
            None,
            PROTOCOL_FIXTURE_READY_TIMEOUT,
        )
        .unwrap();
        let work = thread::spawn(move || {
            let result = worker.call(Request::Finalize);
            assert!(worker.child.try_wait().unwrap().is_none());
            drop(worker);
            drop(lease);
            result
        });
        let ready_by = Instant::now() + Duration::from_secs(5);
        while !received.is_file() {
            if Instant::now() >= ready_by {
                // Release the fixture even when its acknowledgement is unexpectedly missing.
                let _ = gate.acquire_recording();
                let _ = work.join();
                panic!("Setup fixture did not receive the request");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let recording = gate.acquire_recording().unwrap();
        assert_eq!(
            work.join().unwrap().unwrap_err(),
            crate::operations::SETUP_CANCELLED
        );
        drop(recording);
    }

    #[test]
    fn batch_retries_transient_startup_but_not_protocol_or_model_failures() {
        #[cfg(windows)]
        let cases = [
            ("exit 0", true),
            ("Start-Sleep -Seconds 5", true),
            ("[Console]::Out.WriteLine('not JSON')", false),
            (
                "[Console]::Out.WriteLine('{\"ok\":false,\"error\":\"model unavailable\"}')",
                false,
            ),
        ];
        #[cfg(not(windows))]
        let cases = [
            ("exit 0", true),
            ("exec sleep 5", true),
            ("printf 'not JSON\n'", false),
            (
                "printf '{\"ok\":false,\"error\":\"model unavailable\"}\n'",
                false,
            ),
        ];
        for (script, transient) in cases {
            let mut attempts = 0;
            let result = retry_batch(
                || {
                    attempts += 1;
                    match Worker::start_command(
                        protocol_test_child(script),
                        WorkerKind::Asr,
                        None,
                        if script.to_ascii_lowercase().contains("sleep") {
                            Duration::from_secs(2)
                        } else {
                            PROTOCOL_FIXTURE_READY_TIMEOUT
                        },
                    ) {
                        Ok(_) => (Ok(()), false, false),
                        Err(error) => (Err(error.message), error.transient, false),
                    }
                },
                || {},
            );
            assert!(result.is_err());
            assert_eq!(attempts, if transient { 3 } else { 1 }, "{script}");
        }
    }

    #[test]
    fn controlled_initialization_errors_are_reported_before_ready() {
        let mut bytes = Vec::new();
        assert!(initialize_worker(&[], &mut bytes).is_err());
        let reply: Reply = serde_json::from_slice(&bytes).unwrap();
        assert!(!reply.ok);
        assert_eq!(
            reply.error.as_deref(),
            Some("Invalid recognition worker arguments")
        );
        let fixture = BatchFixture::new();
        let root = fixture.root.clone();
        std::fs::create_dir_all(&root).unwrap();
        bytes.clear();
        let args = vec![
            "missing-model.gguf".into(),
            "missing-runtime".into(),
            root.to_string_lossy().into_owned(),
            "batch".into(),
        ];
        assert!(initialize_worker(&args, &mut bytes).is_err());
        assert!(!serde_json::from_slice::<Reply>(&bytes).unwrap().ok);
    }
    #[test]
    fn corrupt_batch_audio_sends_a_permanent_error_reply_before_exit() {
        let fixture = BatchFixture::new();
        let dir = fixture.root.clone();
        let store = SessionStore::create(
            &dir,
            "Synthetic corrupt batch",
            handy_bridge::PARAKEET_ULTRA_ID,
            "Output",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[3000; 1600]).unwrap();
        writer.finish().unwrap();
        let mut part = store.snapshot().unwrap().parts[0].clone();
        part.confirmed_sha256 = "0".repeat(64);
        let result = batch_section(store.folder(), &part, None, |_, _| {
            panic!("corrupt audio must not reach native ASR")
        });
        let mut output = Vec::new();
        assert!(reply_to_batch(&mut output, result).is_err());
        let reply: Reply =
            serde_json::from_slice(&output).expect("explicit error reply before exit");
        assert!(!reply.ok);
        assert_eq!(
            reply.error.as_deref(),
            Some("Closed audio did not pass batch verification")
        );
        let wire = serde_json::to_string(&reply).unwrap();
        assert!(!wire.contains('\''));
        #[cfg(windows)]
        let script = format!(
            r#"[Console]::Out.WriteLine('{{"ok":true}}');$null=[Console]::In.ReadLine();[Console]::Out.WriteLine('{wire}');$null=[Console]::In.ReadLine()"#
        );
        #[cfg(not(windows))]
        let script=format!("printf '%s\n' '{{\"ok\":true}}'; IFS= read -r line; printf '%s\n' '{wire}'; IFS= read -r line");
        let mut worker = Worker::start_command(
            protocol_test_child(&script),
            WorkerKind::Asr,
            None,
            PROTOCOL_FIXTURE_READY_TIMEOUT,
        )
        .unwrap();
        let error = worker
            .call(Request::BatchSection {
                part,
                previous: None,
            })
            .unwrap_err();
        assert_eq!(error, "Closed audio did not pass batch verification");
        assert!(!worker.transient_failure.get());
    }
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
