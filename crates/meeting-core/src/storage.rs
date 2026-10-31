use crate::{AudioPart, Result, SessionData, SessionProgress, TranscriptCheckpoint, SAMPLE_RATE};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Digest;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};
use uuid::Uuid;

const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_JOURNAL_LINE: usize = 2 * 1024 * 1024;
/// Optional audio analysis may read only a closed, digest-verified bounded section.
pub fn read_closed_audio(folder: &Path, part: &AudioPart) -> Result<Vec<f32>> {
    reject_link(folder)?;
    validate_part(part)?;
    if !part.audio_closed
        || part.confirmed_sample_count == 0
        || part.confirmed_sample_count > u64::from(SAMPLE_RATE) * 120
    {
        return Err("Speaker analysis needs a closed audio section of at most two minutes".into());
    }
    let path = safe_audio_path(folder, &part.file)?;
    crate::pcm::verify_confirmed(&path, part)?;
    let mut pcm = Vec::with_capacity(part.confirmed_sample_count as usize);
    while pcm.len() < part.confirmed_sample_count as usize {
        let count = (part.confirmed_sample_count as usize - pcm.len()).min(SAMPLE_RATE as usize);
        let next = crate::read_pcm_samples(&path, pcm.len() as u64, count)?;
        if next.len() != count {
            return Err("Confirmed audio ended early".into());
        }
        pcm.extend(next);
    }
    Ok(pcm)
}

struct Inner {
    data: SessionData,
    _lease: SessionLease,
}
pub(crate) struct SessionLease {
    internal: Option<File>,
    relocation: Option<File>,
}
pub(crate) struct RelocationGuard {
    _internal: Option<File>,
    _relocation: File,
}

/// Clones share one writer and one exclusive lease. New Rust sessions only.
#[derive(Clone)]
pub struct SessionStore {
    folder: PathBuf,
    // Join the derived writer before releasing the last session lease.
    mirror: Arc<TextMirror>,
    writers: Arc<Writers>,
    inner: Arc<Mutex<Inner>>,
}

/// One writer lock per durable file (#18). File writes and fsync run under these and
/// never under the shared data lock, so a slow transcript or manifest write cannot
/// delay an audio checkpoint. Lock order: writer lock, then the data lock briefly.
/// A record becomes visible in memory only after it is durable.
#[derive(Default)]
struct Writers {
    audio: Mutex<()>,
    transcript: Mutex<()>,
    manifest: Mutex<()>,
    #[cfg(test)]
    test: TestGate,
}

/// Test hook only: parks a transcript append between validation and its file write,
/// standing in for a slow fsync on HDD, network or scanned storage.
#[cfg(test)]
#[derive(Default)]
struct TestGate {
    transcript_io: Mutex<()>,
    transcript_waiting: std::sync::atomic::AtomicBool,
    /// Readable-TXT refresh attempts that fail with an I/O error before touching files.
    refresh_failures: std::sync::atomic::AtomicUsize,
}
#[cfg(test)]
impl TestGate {
    fn refresh_fails(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.refresh_failures
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }
    fn pause_transcript_io(&self) {
        use std::sync::atomic::Ordering;
        self.transcript_waiting.store(true, Ordering::Release);
        drop(self.transcript_io.lock());
        self.transcript_waiting.store(false, Ordering::Release);
    }
}

struct TextMirror {
    sender: Option<mpsc::SyncSender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
const TXT_WARNING: &str = "Die lesbare TXT-Kopie konnte nicht aktualisiert werden. Originaljournale und bestätigte Texte bleiben erhalten. / The readable TXT copy could not be updated; confirmed journals are preserved.";
impl Drop for TextMirror {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl SessionStore {
    fn from_parts(folder: PathBuf, data: SessionData, lease: SessionLease) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            data,
            _lease: lease,
        }));
        let weak = Arc::downgrade(&inner);
        let path = folder.clone();
        let writers: Arc<Writers> = Arc::default();
        #[cfg(test)]
        let gate = Arc::clone(&writers);
        // At most one pending notification. Each write uses the latest confirmed state.
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("meeting-readable-text".into())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    let mut released = false;
                    let outcome = crate::live_text::retry_io(
                        || {
                            // Hold a strong reference only while copying, so owner checks
                            // stay exact. Each attempt writes the latest confirmed state.
                            let data = {
                                let Some(inner) = weak.upgrade() else {
                                    released = true;
                                    return Ok(());
                                };
                                let Ok(guard) = inner.lock() else {
                                    released = true;
                                    return Ok(());
                                };
                                guard.data.clone()
                            };
                            #[cfg(test)]
                            if gate.test.refresh_fails() {
                                return Err(std::io::Error::other("synthetic busy file").into());
                            }
                            // All filesystem work is outside the audio checkpoint lock.
                            crate::live_text::refresh(&path, &data)
                        },
                        std::thread::sleep,
                    );
                    if released {
                        break;
                    }
                    if outcome.is_err() {
                        if let Some(inner) = weak.upgrade() {
                            if let Ok(mut guard) = inner.lock() {
                                note_data(&mut guard.data, TXT_WARNING);
                            }
                        }
                    }
                }
            })
            .ok();
        Self {
            folder,
            mirror: Arc::new(TextMirror {
                sender: Some(sender),
                worker,
            }),
            writers,
            inner,
        }
    }
    fn queue_text_mirror(&self) {
        if let Some(sender) = self.mirror.sender.as_ref() {
            if matches!(
                sender.try_send(()),
                Err(mpsc::TrySendError::Disconnected(()))
            ) {
                if let Ok(mut guard) = self.inner.lock() {
                    note_data(&mut guard.data, TXT_WARNING);
                }
            }
        }
    }
    /// Bounded latest text revisions; no full-meeting clone or audio access.
    pub fn transcript_page(&self, after: u64, limit: usize) -> Result<crate::TranscriptPage> {
        if !(1..=8).contains(&limit) {
            return Err("Use a text page limit from 1 to 8".into());
        }
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        if after > guard.data.snapshot_sequence {
            return Err("Transcript cursor is ahead of this session".into());
        }
        let mut selected: Vec<&AudioPart> = Vec::with_capacity(limit + 1);
        let mut count = 0;
        for part in &guard.data.parts {
            if part.transcript_sequence <= after {
                continue;
            }
            count += 1;
            let index =
                selected.partition_point(|old| old.transcript_sequence < part.transcript_sequence);
            selected.insert(index, part);
            if selected.len() > limit {
                selected.pop();
            }
        }
        let next_sequence = selected
            .last()
            .map_or(after, |part| part.transcript_sequence);
        Ok(crate::TranscriptPage {
            next_sequence,
            more: count > limit,
            updates: selected
                .into_iter()
                .map(|part| text_slice(part, 0, 1024))
                .collect(),
            untrusted_data: true,
        })
    }
    pub fn transcript_slice(
        &self,
        source: &str,
        start: u64,
        sequence: u64,
        offset: usize,
        limit: usize,
    ) -> Result<crate::TranscriptSlice> {
        if !["system", "microphone"].contains(&source) || !(4..=4096).contains(&limit) {
            return Err("Invalid text slice range".into());
        }
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        let part = guard
            .data
            .parts
            .iter()
            .find(|part| part.source == source && part.start_sample == start)
            .ok_or("Unknown transcript section")?;
        if part.transcript_sequence != sequence {
            return Err("Transcript changed; request its latest revision before continuing".into());
        }
        let text = part.text.as_deref().unwrap_or("");
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err("Text offset must be a UTF-8 byte boundary".into());
        }
        Ok(text_slice(part, offset, limit))
    }
    pub fn create(
        root: &Path,
        title: &str,
        model_id: &str,
        output_name: &str,
        microphone_name: Option<&str>,
    ) -> Result<Self> {
        fs::create_dir_all(root)?;
        let id = Uuid::new_v4().simple().to_string();
        let folder = root.join(&id);
        fs::create_dir(&folder)?;
        fs::create_dir(folder.join("audio"))?;
        let lease = lease(&folder)?;
        let data = SessionData {
            schema_version: 2,
            id,
            title: if title.trim().is_empty() {
                "Meeting".into()
            } else {
                title.trim().into()
            },
            started_at: now(),
            ended_at: None,
            status: "recording".into(),
            model_id: model_id.into(),
            output_name: output_name.into(),
            microphone_name: microphone_name.map(str::to_owned),
            chunk_seconds: 30,
            duration_ms: 0,
            capture_incomplete: false,
            warnings: vec![],
            parts: vec![],
            snapshot_sequence: 0,
            read_only: false,
        };
        atomic_json(&folder.join("session.json"), &data)?;
        Ok(Self::from_parts(folder, data, lease))
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }
    pub(crate) fn into_relocation_guard(mut self) -> Result<RelocationGuard> {
        // As sole owner, finish the derived TXT writer first; it can briefly hold the session.
        if let Some(mirror) = Arc::get_mut(&mut self.mirror) {
            mirror.sender.take();
            if let Some(worker) = mirror.worker.take() {
                let _ = worker.join();
            }
        }
        if Arc::strong_count(&self.inner) != 1 {
            return Err(
                "Other session readers or writers are still active; archive move stopped".into(),
            );
        }
        let mut inner = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        let internal = inner._lease.internal.take();
        #[cfg(windows)]
        let internal = {
            drop(internal);
            None
        };
        let guard = inner
            ._lease
            .relocation
            .take()
            .ok_or("Relocation lease unavailable")?;
        drop(inner);
        drop(self);
        Ok(RelocationGuard {
            _internal: internal,
            _relocation: guard,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_owner_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
    pub fn snapshot(&self) -> Result<SessionData> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Session lock was poisoned")?
            .data
            .clone())
    }

    pub fn model_id(&self) -> Result<String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Session lock was poisoned")?
            .data
            .model_id
            .clone())
    }

    pub fn next_pending_part(&self, source: &str, model_id: &str) -> Result<Option<AudioPart>> {
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        Ok(guard
            .data
            .parts
            .iter()
            .filter(|part| {
                part.source == source
                    && !(part.status == "complete" && part.model_id.as_deref() == Some(model_id))
            })
            .min_by_key(|part| part.start_sample)
            .cloned())
    }

    pub fn recognition_complete(&self) -> Result<bool> {
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        Ok(guard.data.parts.iter().all(|part| {
            part.audio_closed
                && part.status == "complete"
                && part.model_id.as_deref() == Some(guard.data.model_id.as_str())
        }))
    }

    /// Clone only bounded recent parts; full meeting text never enters this path.
    pub fn preview_snapshot(&self, limit: usize) -> Result<SessionData> {
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        let data = &guard.data;
        let limit = limit.min(64);
        let mut recent: Vec<&AudioPart> = Vec::with_capacity(limit + 1);
        for part in &data.parts {
            let index = recent.partition_point(|old| {
                (old.start_sample, old.source.as_str()) <= (part.start_sample, part.source.as_str())
            });
            if recent.len() == limit && index == 0 {
                continue;
            }
            recent.insert(index, part);
            if recent.len() > limit {
                recent.remove(0);
            }
        }
        Ok(SessionData {
            schema_version: data.schema_version,
            id: data.id.clone(),
            title: data.title.clone(),
            started_at: data.started_at.clone(),
            ended_at: data.ended_at.clone(),
            status: data.status.clone(),
            model_id: data.model_id.clone(),
            output_name: data.output_name.clone(),
            microphone_name: data.microphone_name.clone(),
            chunk_seconds: data.chunk_seconds,
            duration_ms: data.duration_ms,
            capture_incomplete: data.capture_incomplete,
            warnings: data.warnings.clone(),
            parts: recent.into_iter().cloned().collect(),
            snapshot_sequence: data.snapshot_sequence,
            read_only: data.read_only,
        })
    }

    pub fn progress(&self) -> Result<SessionProgress> {
        let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        let data = &guard.data;
        let mut result = SessionProgress {
            total_parts: data.parts.len(),
            complete_parts: 0,
            system_confirmed_ms: 0,
            microphone_confirmed_ms: 0,
            pending_ms: 0,
        };
        let mut pending_system = 0;
        let mut pending_microphone = 0;
        for part in &data.parts {
            let same_model = part.model_id.as_deref() == Some(data.model_id.as_str());
            if same_model && part.status == "complete" {
                result.complete_parts += 1;
            }
            let end_ms =
                (part.start_sample + part.confirmed_sample_count) * 1000 / u64::from(SAMPLE_RATE);
            let pending = if same_model {
                part.confirmed_sample_count
                    .saturating_sub(part.processed_samples)
            } else {
                part.confirmed_sample_count
            };
            if part.source == "system" {
                result.system_confirmed_ms = result.system_confirmed_ms.max(end_ms);
                pending_system += pending;
            } else {
                result.microphone_confirmed_ms = result.microphone_confirmed_ms.max(end_ms);
                pending_microphone += pending;
            }
        }
        result.pending_ms = pending_system.max(pending_microphone) * 1000 / u64::from(SAMPLE_RATE);
        Ok(result)
    }

    /// Recover a Rust v2 session. Legacy sessions are available through read_legacy only.
    pub fn open(folder: &Path) -> Result<Self> {
        reject_link(folder)?;
        let lease = lease(folder)?;
        let manifest = folder.join("session.json");
        let (mut data, backup): (SessionData, bool) = match read_json(&manifest) {
            Ok(data) => (data, false),
            Err(_) => (read_json(&folder.join("session.json.bak"))?, true),
        };
        if data.schema_version != 2 {
            return Err(
                "Use read_legacy for original C# sessions; they are never rewritten".into(),
            );
        }
        validate_data(&data)?;
        reject_link(&folder.join("audio"))?;
        let interrupted = data.ended_at.is_none();
        // The audio journal is authoritative for acknowledged sample counts. Bytes beyond
        // that point are preserved on disk but never silently advertised as confirmed.
        let mut malformed = 0;
        replay_audio_journal(folder, &mut data, &mut malformed)?;
        let mut unclosed = 0;
        for part in &mut data.parts {
            validate_part(part)?;
            let audio = safe_audio_path(folder, &part.file)?;
            crate::pcm::verify_confirmed(&audio, part)?;
            part.sample_count = part.confirmed_sample_count;
            // The exclusive lease is held, so no writer in any process can still append
            // to this part, and a writer never reattaches to a track that has audio.
            // Close every part at its confirmed prefix, also after a stopped recording
            // whose chunk finalization failed (#15). Unconfirmed bytes stay on disk.
            if !part.audio_closed {
                part.audio_closed = true;
                // "complete" is only derived for closed audio; the transcript replay
                // below re-derives it from the journal for the closed prefix.
                if part.status == "complete" {
                    part.status = "pending".into();
                }
                unclosed += 1;
            }
        }
        replay_transcript_journal(folder, &mut data, &mut malformed)?;
        if malformed > 0 {
            warn_data(&mut data, &format!("{malformed} unfinished journal line(s) retained; last confirmed data recovered."));
        }
        if backup {
            warn_data(
                &mut data,
                "Recovered session metadata from its backup; original damaged file retained.",
            );
        }
        if interrupted {
            warn_data(&mut data, "Recording process was interrupted. Confirmed audio and text recovered; an unconfirmed tail may be missing.");
            data.status = "recorded".into();
        } else if unclosed > 0 {
            warn_data(&mut data, "An audio section was not closed when the recording stopped, for example because the disk was full. It was closed at its last confirmed point; an unconfirmed tail may be missing.");
        }
        data.duration_ms = duration(&data.parts);
        let store = Self::from_parts(folder.to_owned(), data, lease);
        // Recovery is journal-derived and read-only until another explicit update.
        Ok(store)
    }

    pub fn audio_path(&self, part: &AudioPart) -> Result<PathBuf> {
        validate_part(part)?;
        safe_audio_path(&self.folder, &part.file)
    }

    pub fn audio_checkpoint(&self, mut part: AudioPart) -> Result<()> {
        validate_part(&part)?;
        // Serializes audio records, so journal order equals publish order and the
        // monotonic check below stays valid until this record is published.
        let _journal = self
            .writers
            .audio
            .lock()
            .map_err(|_| "Audio journal lock was poisoned")?;
        {
            let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
            if let Some(existing) = guard.data.parts.iter().find(|old| old.file == part.file) {
                if part.confirmed_sample_count < existing.confirmed_sample_count {
                    return Err("Confirmed audio cannot move backward".into());
                }
                keep_text(&mut part, existing);
            }
        }
        append_json(&self.folder.join("audio-journal.jsonl"), &part)?;
        // Publish only after the record is durable.
        let mut guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        guard.data.duration_ms = guard
            .data
            .duration_ms
            .max((part.start_sample + part.confirmed_sample_count) * 1000 / u64::from(SAMPLE_RATE));
        if let Some(existing) = guard
            .data
            .parts
            .iter_mut()
            .find(|old| old.file == part.file)
        {
            // Recognition may have advanced while the audio record was synced.
            keep_text(&mut part, existing);
            *existing = part;
        } else {
            guard.data.parts.push(part);
        }
        Ok(())
    }

    pub fn append_transcript(&self, mut checkpoint: TranscriptCheckpoint) -> Result<()> {
        // Serializes transcript records: sequence assignment, journal order and publish
        // order stay identical. Only this path changes text fields and the sequence.
        let journal = self
            .writers
            .transcript
            .lock()
            .map_err(|_| "Transcript journal lock was poisoned")?;
        let (next, notice) = {
            let guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
            let part = guard
                .data
                .parts
                .iter()
                .find(|part| part.file == checkpoint.file)
                .ok_or("Unknown audio part")?;
            // Confirmed audio only grows, so this validation still holds at publish time.
            validate_checkpoint(part, &checkpoint)?;
            validate_continuity(part, &checkpoint)?;
            let next = guard
                .data
                .snapshot_sequence
                .checked_add(1)
                .ok_or("Sequence overflow")?;
            if checkpoint.sequence != 0 && checkpoint.sequence != next {
                return Err("Unexpected transcript sequence".into());
            }
            let notice = checkpoint
                .revision_after_restart
                .then(|| revision_notice(part));
            (next, notice)
        };
        checkpoint.sequence = next;
        checkpoint.at = now();
        #[cfg(test)]
        self.writers.test.pause_transcript_io();
        append_json(&self.folder.join("transcript-journal.jsonl"), &checkpoint)?;
        // Publish only after the record is durable.
        let mut guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        let part = guard
            .data
            .parts
            .iter_mut()
            .find(|part| part.file == checkpoint.file)
            .ok_or("Unknown audio part")?;
        apply_checkpoint(part, &checkpoint);
        guard.data.snapshot_sequence = next;
        drop(guard);
        self.queue_text_mirror();
        drop(journal);
        if let Some(notice) = notice {
            // The marked checkpoint is already durable and authoritative; journal replay
            // re-derives this notice on reopen if the manifest write fails here.
            let _ = self.notice(&notice);
        }
        Ok(())
    }

    /// Persist a manifest change without holding the data lock during the write, then
    /// apply the same idempotent change to the current state. Audio and text recorded
    /// meanwhile stay in memory; their journals remain authoritative.
    fn update_manifest(&self, change: impl Fn(&mut SessionData)) -> Result<()> {
        let _manifest = self
            .writers
            .manifest
            .lock()
            .map_err(|_| "Session manifest lock was poisoned")?;
        let mut updated = self
            .inner
            .lock()
            .map_err(|_| "Session lock was poisoned")?
            .data
            .clone();
        change(&mut updated);
        atomic_json(&self.folder.join("session.json"), &updated)?;
        let mut guard = self.inner.lock().map_err(|_| "Session lock was poisoned")?;
        change(&mut guard.data);
        Ok(())
    }

    pub fn set_status(&self, status: &str) -> Result<()> {
        if ![
            "recording",
            "recorded",
            "streaming",
            "catching-up",
            "complete",
            "error",
        ]
        .contains(&status)
        {
            return Err("Unknown session status".into());
        }
        // Every status after capture records the end once. Without it, a session that
        // was recovered after a crash and then finished would read as interrupted and
        // fall back to "recorded" on every later open (#16).
        let ended_at = (!["recording", "streaming"].contains(&status)).then(now);
        self.update_manifest(|data| {
            data.status = status.into();
            if data.ended_at.is_none() {
                data.ended_at.clone_from(&ended_at);
            }
        })?;
        // An explicit update reconciles text recovered from the journals after a crash.
        self.queue_text_mirror();
        Ok(())
    }

    pub fn warn(&self, warning: &str) -> Result<()> {
        self.diagnostic(warning, true)
    }

    /// An inference/UI warning does not imply that the audio capture was incomplete.
    pub fn notice(&self, warning: &str) -> Result<()> {
        self.diagnostic(warning, false)
    }

    /// A diagnostic's failed manifest write remains an error, but its warning stays
    /// visible to the current session. Audio writers use their independent journal.
    fn diagnostic(&self, warning: &str, capture_incomplete: bool) -> Result<()> {
        let apply = |data: &mut SessionData| {
            if capture_incomplete {
                warn_data(data, warning);
            } else {
                note_data(data, warning);
            }
        };
        let result = self.update_manifest(apply);
        if let Err(error) = &result {
            if let Ok(mut guard) = self.inner.lock() {
                apply(&mut guard.data);
                note_data(&mut guard.data, &format!("A session diagnostic could not be saved: {error}. It remains visible while this session is open; check storage before closing. Saved audio and text journals are retained."));
            }
        }
        result
    }

    pub fn transcript_text(&self) -> Result<String> {
        Ok(render_transcript(&self.snapshot()?))
    }
}

fn text_slice(part: &AudioPart, offset: usize, limit: usize) -> crate::TranscriptSlice {
    let text = part.text.as_deref().unwrap_or("");
    let mut end = text.len().min(offset.saturating_add(limit));
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    crate::TranscriptSlice {
        source: part.source.clone(),
        start_sample: part.start_sample,
        end_sample: part.start_sample + part.confirmed_sample_count,
        sequence: part.transcript_sequence,
        status: part.status.clone(),
        text: text[offset..end].into(),
        offset,
        next_offset: end,
        total_bytes: text.len(),
        committed_bytes: part.committed_text.len(),
    }
}

pub fn list_sessions(root: &Path) -> Result<Vec<SessionData>> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() && reject_link(&path).is_ok() {
            let manifest = path.join("session.json");
            if let Ok(data) = read_json::<SessionData>(&manifest)
                .or_else(|_| read_json(&path.join("session.json.bak")))
            {
                if validate_data(&data).is_ok() {
                    result.push(data);
                }
            }
        }
    }
    result.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(result)
}

/// Read only a validated durable manifest for rebuilding the derived search index.
/// Full recovery still requires opening the selected session and checking its journals/audio.
pub fn inspect_manifest(folder: &Path) -> Result<SessionData> {
    reject_link(folder)?;
    let data: SessionData = read_json(&folder.join("session.json"))
        .or_else(|_| read_json(&folder.join("session.json.bak")))?;
    validate_data(&data)?;
    if folder.file_name().and_then(|s| s.to_str()) != Some(data.id.as_str()) {
        return Err("Manifest identity differs from its session directory".into());
    }
    Ok(data)
}

fn replay_audio_journal(
    folder: &Path,
    data: &mut SessionData,
    malformed: &mut usize,
) -> Result<()> {
    let mut seen_audio = std::collections::HashMap::<String, u64>::new();
    read_lines::<AudioPart, _>(&folder.join("audio-journal.jsonl"), malformed, |part| {
        validate_part(&part)?;
        if seen_audio
            .insert(part.file.clone(), part.confirmed_sample_count)
            .is_some_and(|previous| part.confirmed_sample_count < previous)
        {
            return Err("Audio journal moved backward".into());
        }
        if let Some(existing) = data.parts.iter_mut().find(|old| old.file == part.file) {
            if part.confirmed_sample_count < existing.confirmed_sample_count {
                return Ok(());
            }
            // A manifest saved after recovery closed exactly this confirmed prefix keeps
            // it closed; the journal itself is never rewritten by recovery.
            let closed = part.audio_closed
                || (existing.audio_closed
                    && existing.confirmed_sample_count == part.confirmed_sample_count
                    && existing.confirmed_sha256 == part.confirmed_sha256);
            let mut part = part;
            keep_text(&mut part, existing);
            part.audio_closed = closed;
            *existing = part;
        } else {
            data.parts.push(part);
        }
        Ok(())
    })?;
    Ok(())
}
/// Audio records carry the recognition fields of the part only as a copy; the
/// transcript journal owns them.
fn keep_text(part: &mut AudioPart, from: &AudioPart) {
    part.text = from.text.clone();
    part.committed_text = from.committed_text.clone();
    part.tentative_text = from.tentative_text.clone();
    part.processed_samples = from.processed_samples;
    part.model_id = from.model_id.clone();
    part.status = from.status.clone();
    part.transcript_sequence = from.transcript_sequence;
}
fn replay_transcript_journal(
    folder: &Path,
    data: &mut SessionData,
    malformed: &mut usize,
) -> Result<()> {
    read_lines::<TranscriptCheckpoint, _>(
        &folder.join("transcript-journal.jsonl"),
        malformed,
        |checkpoint| {
            let Some(part) = data
                .parts
                .iter_mut()
                .find(|part| part.file == checkpoint.file)
            else {
                return Err("Text journal references unknown audio".into());
            };
            validate_checkpoint(part, &checkpoint)?;
            let mut notice = None;
            if checkpoint.sequence >= part.transcript_sequence {
                validate_continuity(part, &checkpoint)?;
                // Re-derived from the journal, so the notice survives a crash between
                // the durable checkpoint and its manifest update.
                notice = checkpoint
                    .revision_after_restart
                    .then(|| revision_notice(part));
                apply_checkpoint(part, &checkpoint);
            }
            data.snapshot_sequence = data.snapshot_sequence.max(checkpoint.sequence);
            if let Some(notice) = notice {
                note_data(data, &notice);
            }
            Ok(())
        },
    )?;
    Ok(())
}
/// Text-only, read-only journal replay for the derived search index: no lease, no audio
/// verification and no writes. Opening the session remains the authoritative recovery.
pub fn inspect_journals(folder: &Path) -> Result<SessionData> {
    let mut data = inspect_manifest(folder)?;
    if data.schema_version != 2 {
        return Ok(data);
    }
    let mut malformed = 0;
    replay_audio_journal(folder, &mut data, &mut malformed)?;
    replay_transcript_journal(folder, &mut data, &mut malformed)?;
    data.duration_ms = duration(&data.parts);
    Ok(data)
}

/// Inspect existing v1 data without creating a lease, repairing a header or writing exports.
pub fn read_legacy(manifest: &Path) -> Result<SessionData> {
    reject_link(manifest)?;
    let folder = manifest.parent().ok_or("Session directory missing")?;
    reject_link(folder)?;
    let mut data: SessionData = read_json(manifest)?;
    if data.schema_version != 1 {
        return Err("Unsupported legacy session version".into());
    }
    validate_data(&data)?;
    for part in &mut data.parts {
        if !safe_audio_path(folder, &part.file)?.is_file() {
            return Err("Legacy audio file is missing".into());
        }
        part.confirmed_sample_count = part.sample_count;
    }
    let mut malformed = 0;
    let manifest_sequence = data.snapshot_sequence;
    let selected_model = data.model_id.clone();
    read_lines::<TranscriptCheckpoint, _>(
        &folder.join("transcript-journal.jsonl"),
        &mut malformed,
        |checkpoint| {
            if let Some(part) = data
                .parts
                .iter_mut()
                .find(|part| part.file == checkpoint.file)
            {
                validate_checkpoint(part, &checkpoint)?;
                // Batch transcription in v1 can commit a newer full manifest without
                // appending a streaming journal entry. Do not replace that result with
                // an older or another model's partial checkpoint during read-only import.
                if checkpoint.model_id != selected_model
                    || (part.status == "complete" && checkpoint.sequence <= manifest_sequence)
                    || (part.status == "complete"
                        && checkpoint.processed_samples < part.sample_count)
                    || (part.model_id.as_deref() == Some(checkpoint.model_id.as_str())
                        && checkpoint.processed_samples < part.processed_samples)
                {
                    return Ok(());
                }
                apply_checkpoint(part, &checkpoint);
            }
            Ok(())
        },
    )?;
    if malformed > 0 {
        data.warnings
            .push("Legacy journal has an unfinished line; original bytes were retained.".into());
    }
    data.read_only = true;
    Ok(data)
}

pub fn render_transcript(data: &SessionData) -> String {
    let mut text = format!(
        "{}\n{} · {}\nSection times are recording ranges, not exact word times.\n",
        data.title, data.started_at, data.model_id
    );
    let mut parts: Vec<_> = data.parts.iter().collect();
    parts.sort_by_key(|part| (part.start_sample, part.source.clone()));
    for part in parts {
        if let Some(value) = &part.text {
            if !value.trim().is_empty() {
                text.push_str(&format!(
                    "\n[{}–{}] {}\n{}\n",
                    time(part.start_sample),
                    time(part.start_sample + part.sample_count),
                    part.source,
                    value
                ));
            }
        }
    }
    text
}

fn time(sample: u64) -> String {
    let seconds = sample / u64::from(SAMPLE_RATE);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}
fn duration(parts: &[AudioPart]) -> u64 {
    parts
        .iter()
        .map(|p| (p.start_sample + p.confirmed_sample_count) * 1000 / u64::from(SAMPLE_RATE))
        .max()
        .unwrap_or(0)
}
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn warn_data(data: &mut SessionData, text: &str) {
    data.capture_incomplete = true;
    note_data(data, text);
}
/// The readable TXT is derived from the journals; its failures never imply lost audio.
fn note_data(data: &mut SessionData, text: &str) {
    if !data.warnings.iter().any(|old| old == text) {
        data.warnings.push(text.into());
    }
}

pub(crate) fn lease(folder: &Path) -> Result<SessionLease> {
    reject_link(folder)?;
    let parent = folder.parent().ok_or("Session has no storage parent")?;
    reject_link(parent)?;
    let locks = parent.join(".leases");
    match fs::create_dir(&locks) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    reject_link(&locks)?;
    if !locks.is_dir() {
        return Err("Session lease storage is not an ordinary directory".into());
    }
    let key = format!(
        "{:x}",
        sha2::Sha256::digest(folder.canonicalize()?.to_string_lossy().as_bytes())
    );
    let relocation_path = locks.join(format!("{key}.lock"));
    // A dangling link reads as missing to exists(), and create(true) would follow it.
    exists_no_link(&relocation_path)?;
    let relocation = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(relocation_path)?;
    // A child process started elsewhere can briefly inherit an open lease handle
    // between fork and exec; a lease that stays held is still a hard error.
    crate::metadata::lock_soon(&relocation)
        .map_err(|_| "This session is already active in another process")?;
    let path = folder.join("session.lock");
    exists_no_link(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    crate::metadata::lock_soon(&file)
        .map_err(|_| "This session is already active in another process")?;
    Ok(SessionLease {
        internal: Some(file),
        relocation: Some(relocation),
    })
}
/// Holds cross-process relocation exclusion without opening or repairing a transcript.
/// The internal lock handle is released on Windows so the confirmed tree can be removed;
/// the external stable lease remains held until removal finishes.
pub(crate) fn erasure_lease(folder: &Path) -> Result<RelocationGuard> {
    let mut owner = lease(folder)?;
    let internal = owner.internal.take();
    #[cfg(windows)]
    let internal = {
        drop(internal);
        None
    };
    Ok(RelocationGuard {
        _internal: internal,
        _relocation: owner.relocation.take().ok_or("Missing erasure lease")?,
    })
}
fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    reject_link(path)?;
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_MANIFEST_BYTES {
        return Err("Metadata exceeds safety limit".into());
    }
    Ok(serde_json::from_reader(file)?)
}

pub(crate) fn validate_part(part: &AudioPart) -> Result<()> {
    let (source, suffix) = part.file.split_once('-').ok_or("Invalid audio file name")?;
    let number = suffix
        .strip_suffix(".wav")
        .ok_or("Invalid audio file suffix")?;
    if !["system", "microphone"].contains(&source)
        || source != part.source
        || number.len() < 12
        || !number.bytes().all(|b| b.is_ascii_digit())
        || number.parse::<u64>()? != part.start_sample
        || part.sample_count > u64::from(SAMPLE_RATE) * 300
        || part.confirmed_sample_count > part.sample_count
    {
        return Err("Unsafe or inconsistent audio part".into());
    }
    part.start_sample
        .checked_add(part.sample_count)
        .ok_or("Audio timeline overflow")?;
    Ok(())
}
fn validate_data(data: &SessionData) -> Result<()> {
    if ![1, 2].contains(&data.schema_version)
        || !(1..=300).contains(&data.chunk_seconds)
        || data.parts.len() > 100_000
    {
        return Err("Unsupported or oversized session".into());
    }
    for part in &data.parts {
        validate_part(part)?;
    }
    Ok(())
}
fn validate_checkpoint(part: &AudioPart, checkpoint: &TranscriptCheckpoint) -> Result<()> {
    if checkpoint.model_id.is_empty()
        || checkpoint.processed_samples > part.confirmed_sample_count
        || checkpoint.full.len() > 1024 * 1024
        || checkpoint.committed.len() > 1024 * 1024
        || checkpoint.tentative.len() > 1024 * 1024
    {
        return Err("Transcript does not match confirmed audio".into());
    }
    Ok(())
}

/// Whether `committed`/`full` continue the stored confirmed prefix of `part`. Text of
/// another model or of a part without confirmed text never conflicts.
pub fn continues_confirmed_text(
    part: &AudioPart,
    model_id: &str,
    committed: &str,
    full: &str,
) -> bool {
    part.model_id.as_deref() != Some(model_id)
        || part.committed_text.is_empty()
        || (committed.starts_with(&part.committed_text) && full.starts_with(&part.committed_text))
}

/// Progress never moves backward, also not for a marked revision. A change of the
/// confirmed prefix is accepted only as an explicitly marked new text version after a
/// recognition restart (#20); unmarked, it is rejected, so text is never replaced
/// silently.
fn validate_continuity(part: &AudioPart, checkpoint: &TranscriptCheckpoint) -> Result<()> {
    if part.model_id.as_deref() == Some(checkpoint.model_id.as_str())
        && checkpoint.processed_samples < part.processed_samples
    {
        return Err(
            "Confirmed transcript progress cannot move backward during recognition restart".into(),
        );
    }
    if continues_confirmed_text(
        part,
        &checkpoint.model_id,
        &checkpoint.committed,
        &checkpoint.full,
    ) || checkpoint.revision_after_restart
    {
        return Ok(());
    }
    Err("Confirmed transcript prefix changed during recognition restart; previous confirmed text remains visible".into())
}

/// Shown once per part; the audio is intact, so this never marks capture incomplete.
fn revision_notice(part: &AudioPart) -> String {
    format!(
        "{} {}: Die Erkennung nach einem Neustart ergab für diesen Abschnitt einen anderen bestätigten Text; der frühere Text bleibt im Transkriptjournal erhalten. / Recognition after a restart produced different confirmed text for this section; the earlier text is kept in the transcript journal.",
        part.source,
        time(part.start_sample)
    )
}
fn apply_checkpoint(part: &mut AudioPart, checkpoint: &TranscriptCheckpoint) {
    part.text = Some(checkpoint.full.clone());
    part.committed_text = checkpoint.committed.clone();
    part.tentative_text = checkpoint.tentative.clone();
    part.processed_samples = checkpoint.processed_samples;
    part.model_id = Some(checkpoint.model_id.clone());
    part.transcript_sequence = checkpoint.sequence;
    part.status = if checkpoint.final_text
        && part.audio_closed
        && checkpoint.processed_samples == part.confirmed_sample_count
    {
        "complete"
    } else {
        "pending"
    }
    .into();
}

pub(crate) fn safe_audio_path(folder: &Path, name: &str) -> Result<PathBuf> {
    if Path::new(name).components().count() != 1 || name.contains(['/', '\\', ':']) {
        return Err("Unsafe audio path".into());
    }
    let audio = folder.join("audio");
    reject_link(&audio)?;
    let path = audio.join(name);
    exists_no_link(&path)?;
    Ok(path)
}
/// Like `Path::exists`, but any link, including a dangling one that `exists()` reports
/// as missing, is an error. Use before opening or creating a file in session storage.
pub(crate) fn exists_no_link(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            check_not_link(&meta)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn reject_link(path: &Path) -> Result<()> {
    check_not_link(&fs::symlink_metadata(path)?)
}
fn check_not_link(meta: &fs::Metadata) -> Result<()> {
    if meta.file_type().is_symlink() {
        return Err("Links are not allowed in session storage".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Reparse points are not allowed in session storage".into());
        }
    }
    Ok(())
}

fn append_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    exists_no_link(path)?;
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_JOURNAL_LINE {
        return Err("Journal entry exceeds safety limit".into());
    }
    let mut output = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    // Retain an unfinished crash tail, but separate it from the next complete record.
    if output.metadata()?.len() > 0 {
        use std::io::{Seek, SeekFrom};
        output.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        output.read_exact(&mut last)?;
        if last[0] != b'\n' {
            output.write_all(b"\n")?;
        }
    }
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    Ok(())
}

fn read_lines<T: DeserializeOwned, F: FnMut(T) -> Result<()>>(
    path: &Path,
    malformed: &mut usize,
    mut consume: F,
) -> Result<()> {
    // A dangling link is rejected, not silently replayed as an empty journal.
    if !exists_no_link(path)? {
        return Ok(());
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = vec![];
    // Replay one bounded line at a time, regardless of meeting duration.
    loop {
        line.clear();
        let bytes = reader
            .by_ref()
            .take((MAX_JOURNAL_LINE + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if bytes == 0 {
            break;
        }
        if bytes > MAX_JOURNAL_LINE {
            return Err("Oversized journal entry".into());
        }
        if !line.ends_with(b"\n") {
            *malformed += 1;
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice(&line) {
            Ok(value) => consume(value)?,
            Err(_) => *malformed += 1,
        }
    }
    Ok(())
}

pub(crate) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().ok_or("Metadata directory missing")?;
    let bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Metadata exceeds safety limit".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    if exists_no_link(path)? {
        let backup = path.with_extension("json.bak");
        // fs::copy would write through a dangling backup link.
        exists_no_link(&backup)?;
        fs::copy(path, &backup)?;
        OpenOptions::new().write(true).open(backup)?.sync_all()?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_hour_metadata_preview_is_bounded_but_old_pending_audio_remains_selectable() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Two-hour metadata fixture",
            "m",
            "synthetic",
            Some("synthetic"),
        )
        .unwrap();
        {
            let mut guard = store.inner.lock().unwrap();
            // Accelerated metadata-only fixture, not a real-time recording test.
            for window in (0..240_u64).rev() {
                for source in ["system", "microphone"] {
                    let start = window * 480_000;
                    guard.data.parts.push(AudioPart {
                        file: format!("{source}-{start:012}.wav"),
                        source: source.into(),
                        start_sample: start,
                        sample_count: 480_000,
                        confirmed_sample_count: 480_000,
                        has_signal: true,
                        audio_closed: true,
                        status: if window == 0 && source == "system" {
                            "pending"
                        } else {
                            "complete"
                        }
                        .into(),
                        text: Some("x".repeat(8192)),
                        committed_text: String::new(),
                        tentative_text: String::new(),
                        processed_samples: 480_000,
                        model_id: Some("m".into()),
                        error: None,
                        confirmed_sha256: String::new(),
                        transcript_sequence: 0,
                    });
                }
            }
            guard.data.duration_ms = 7_200_000;
        }
        let preview = store.preview_snapshot(16).unwrap();
        assert_eq!(preview.parts.len(), 16);
        assert_eq!(preview.duration_ms, 7_200_000);
        assert!(preview
            .parts
            .iter()
            .all(|part| part.start_sample >= 232 * 480_000));
        assert!(serde_json::to_vec(&preview).unwrap().len() < 150_000);
        assert!(store.preview_snapshot(0).unwrap().parts.is_empty());
        assert_eq!(
            store
                .next_pending_part("system", "m")
                .unwrap()
                .unwrap()
                .start_sample,
            0
        );
        assert_eq!(store.progress().unwrap().total_parts, 480);
        assert_eq!(store.progress().unwrap().complete_parts, 479);
        assert!(!store.recognition_complete().unwrap());
    }

    #[test]
    fn briefly_inherited_lease_handle_does_not_fail_open_but_a_held_one_does() {
        // Synthetic: a second handle stands in for a lease inherited by a child process
        // between fork and exec.
        use fs2::FileExt;
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Lease", "m", "s", None).unwrap();
        let folder = store.folder().to_owned();
        drop(store);
        let inherited = File::open(folder.join("session.lock")).unwrap();
        inherited.try_lock_exclusive().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            drop(inherited);
        });
        let reopened = SessionStore::open(&folder).unwrap();
        release.join().unwrap();
        // An owner that keeps the lease still blocks a second writer.
        assert!(SessionStore::open(&folder).is_err());
        drop(reopened);
        assert!(SessionStore::open(&folder).is_ok());
    }

    #[test]
    fn readable_text_worker_repeats_a_refresh_after_brief_io_failures() {
        // Synthetic: a test gate fails refresh attempts with an I/O error, standing in
        // for a scanner or reader briefly holding the TXT open on Windows.
        use std::{sync::atomic::Ordering, thread, time::Duration};
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Busy text", "m", "s", None).unwrap();
        let folder = store.folder().to_owned();
        let mut writer = crate::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[1; 1600]).unwrap();
        writer.finish().unwrap();
        drop(writer);
        let failures = &store.writers.test.refresh_failures;
        let confirm = |text: &str| {
            let part = store.snapshot().unwrap().parts[0].clone();
            store
                .append_transcript(TranscriptCheckpoint {
                    sequence: 0,
                    at: String::new(),
                    file: part.file,
                    model_id: "m".into(),
                    full: text.into(),
                    committed: text.into(),
                    tentative: String::new(),
                    processed_samples: part.confirmed_sample_count,
                    final_text: true,
                    revision_after_restart: false,
                })
                .unwrap();
        };
        let has_warning = || {
            store
                .snapshot()
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning == TXT_WARNING)
        };
        let file = folder.join("transcript.txt");
        let wait = |done: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !done() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "text writer did not finish"
                );
                thread::sleep(Duration::from_millis(5));
            }
        };

        // Three failed attempts, then the last confirmed text is written without a warning.
        failures.store(3, Ordering::Release);
        confirm("Written after three busy attempts.");
        wait(&|| {
            fs::read_to_string(&file)
                .is_ok_and(|text| text.contains("Written after three busy attempts."))
        });
        assert_eq!(failures.load(Ordering::Acquire), 0);
        assert!(!has_warning());

        // A file that stays busy is reported after a bounded number of attempts.
        failures.store(1000, Ordering::Release);
        confirm("Written after three busy attempts. Still busy.");
        wait(&has_warning);
        let attempts = 1000 - failures.load(Ordering::Acquire);
        assert!((8..=16).contains(&attempts), "{attempts} attempts");
        // The earlier readable text and the confirmed journal are untouched.
        let text = fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("Written after three busy attempts.") && !text.contains("Still busy.")
        );
        assert!(fs::read_to_string(folder.join("transcript-journal.jsonl"))
            .unwrap()
            .contains("Still busy."));
        failures.store(0, Ordering::Release);
    }

    #[test]
    fn slow_transcript_write_does_not_delay_audio_checkpoints() {
        // Synthetic (#18): a test gate parks the transcript append exactly where its
        // journal write and fsync run, standing in for slow storage. No real disk delay.
        use std::{sync::atomic::Ordering, thread, time::Duration};
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Slow text", "m", "s", None).unwrap();
        let folder = store.folder().to_owned();
        let mut writer = crate::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[1; 1600]).unwrap();
        writer.checkpoint().unwrap();
        let part = store.snapshot().unwrap().parts[0].clone();
        let gate = store.writers.test.transcript_io.lock().unwrap();
        let recognizer = {
            let store = store.clone();
            thread::spawn(move || {
                store.append_transcript(TranscriptCheckpoint {
                    sequence: 0,
                    at: String::new(),
                    file: part.file.clone(),
                    model_id: "m".into(),
                    full: "Parked words".into(),
                    committed: "Parked words".into(),
                    tentative: String::new(),
                    processed_samples: part.confirmed_sample_count,
                    final_text: false,
                    revision_after_restart: false,
                })
            })
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !store
            .writers
            .test
            .transcript_waiting
            .load(Ordering::Acquire)
        {
            assert!(std::time::Instant::now() < deadline, "append never started");
            thread::sleep(Duration::from_millis(1));
        }
        let (done, finished) = mpsc::channel();
        let capture = thread::spawn(move || {
            writer.append(&[2; 1600]).unwrap();
            writer.checkpoint().unwrap();
            done.send(()).unwrap();
            writer
        });
        finished
            .recv_timeout(Duration::from_secs(5))
            .expect("audio checkpoint waited for the parked transcript write");
        let data = store.snapshot().unwrap();
        assert_eq!(data.parts[0].confirmed_sample_count, 3200);
        // A checkpoint becomes visible only after it is durable.
        assert_eq!(data.snapshot_sequence, 0);
        assert!(data.parts[0].text.is_none());
        assert!(!folder.join("transcript-journal.jsonl").exists());
        drop(gate);
        recognizer.join().unwrap().unwrap();
        let data = store.snapshot().unwrap();
        assert_eq!(data.snapshot_sequence, 1);
        assert_eq!(data.parts[0].text.as_deref(), Some("Parked words"));
        assert_eq!(data.parts[0].confirmed_sample_count, 3200);
        let mut writer = capture.join().unwrap();
        writer.finish().unwrap();
        drop(writer);
        drop(store);
        let reopened = SessionStore::open(&folder).unwrap().snapshot().unwrap();
        assert_eq!(reopened.parts[0].confirmed_sample_count, 3200);
        assert_eq!(reopened.parts[0].text.as_deref(), Some("Parked words"));
    }
}
