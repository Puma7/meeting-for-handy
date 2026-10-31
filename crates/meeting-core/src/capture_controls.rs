//! Per-track privacy gates. A control change invalidates the whole old device/pipe epoch;
//! reopening never makes already-buffered private audio eligible for persistence.
use crate::{audio_levels::AudioMeter, AudioLevel, PcmWriter, Result, SessionStore, SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Instant,
};

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureControlSnapshot {
    pub paused: bool,
    pub microphone_muted: bool,
    pub has_microphone: bool,
}

/// Passive first-packet counters for portable receipt-time alignment. No audio,
/// device names, paths or session identities are retained in this diagnostic.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableStartupTiming {
    pub source: &'static str,
    pub written_before_padding: u64,
    pub estimated_padding_samples: u64,
    pub first_packet_samples: u64,
    pub first_packet_leading_zero_samples: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureControlEvent {
    pub schema_version: u32,
    pub sequence: u64,
    pub at: String,
    pub requested_sample: u64,
    pub kind: String,
    pub paused: bool,
    pub microphone_muted: bool,
    pub timeline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_samples: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_start_sample: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureControlLog {
    pub events: Vec<CaptureControlEvent>,
    pub incomplete_tail: bool,
}

/// Reads only the bounded complete event prefix; no repair or file rewrite occurs.
/// Older sessions without this optional sidecar remain compatible. Unknown complete
/// events or versions are rejected instead of being silently presented as verified.
pub fn read_capture_control_events(folder: &Path) -> Result<Option<CaptureControlLog>> {
    const MAX_BYTES: u64 = 8 * 1024 * 1024;
    let path = folder.join("capture-controls.jsonl");
    if !crate::storage::exists_no_link(&path)? {
        return Ok(None);
    }
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("Capture control log exceeds its safety limit".into());
    }
    let mut bytes = vec![];
    File::open(&path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Capture control log exceeds its safety limit".into());
    }
    let mut events = vec![];
    let mut incomplete_tail = false;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !line.ends_with(b"\n") {
            incomplete_tail = true;
            break;
        }
        if line.len() > 8192 || events.len() >= 32768 {
            return Err("Capture control event exceeds its safety limit".into());
        }
        let event: CaptureControlEvent = serde_json::from_slice(line)?;
        let previous_sample = events.last().map_or(0, |previous: &CaptureControlEvent| {
            previous.requested_sample
        });
        if event.schema_version != 1
            || event.sequence != events.len() as u64 + 1
            || event.requested_sample < previous_sample
            || chrono::DateTime::parse_from_rfc3339(&event.at).is_err()
            || !["started", "pause", "microphone-mute", "device-gap"].contains(&event.kind.as_str())
            || (events.is_empty() && event.kind != "started")
            || (!events.is_empty() && event.kind == "started")
            || (event.kind == "device-gap"
                && (event.source.as_deref() != Some("microphone")
                    || event.missing_samples.unwrap_or(0) == 0))
            || (event.kind == "device-gap" && event.source_start_sample.is_none())
            || (event.kind != "device-gap"
                && (event.source.is_some()
                    || event.missing_samples.is_some()
                    || event.source_start_sample.is_some()))
        {
            return Err("Capture control event is unsupported or inconsistent; original evidence is preserved".into());
        }
        events.push(event);
    }
    if bytes.is_empty() {
        return Err("Capture control log has no confirmed event".into());
    }
    Ok(Some(CaptureControlLog {
        events,
        incomplete_tail,
    }))
}

pub(super) struct CaptureControls {
    paused: AtomicBool,
    microphone_muted: AtomicBool,
    has_microphone: AtomicBool,
    epochs: [AtomicU64; 2],
    // Normal writes of different tracks never acquire the same mutex.
    writes: [Mutex<()>; 2],
    transitions: Mutex<()>,
    journal: Option<Mutex<File>>,
    sequence: AtomicU64,
    start: Instant,
    journal_failed: AtomicBool,
    meters: [AudioMeter; 2],
    startup_progress: AtomicU64,
    portable_startup: Mutex<[Option<PortableStartupTiming>; 2]>,
}

impl CaptureControls {
    /// Creates a new control journal and syncs its initial event before returning the gates.
    pub fn new(store: &SessionStore) -> Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(store.folder().join("capture-controls.jsonl"))?;
        let controls = Self::with_journal(Some(file));
        controls.record("started", false, false)?;
        Ok(controls)
    }

    #[cfg(test)]
    /// Constructs journal-free gates for synthetic capture fixtures only.
    pub fn uncontrolled() -> Self {
        Self::with_journal(None)
    }

    /// Initializes independent track gates, epochs and a shared monotonic timing origin.
    fn with_journal(journal: Option<File>) -> Self {
        Self {
            paused: AtomicBool::new(false),
            microphone_muted: AtomicBool::new(false),
            has_microphone: AtomicBool::new(false),
            epochs: [AtomicU64::new(0), AtomicU64::new(0)],
            writes: [Mutex::new(()), Mutex::new(())],
            transitions: Mutex::new(()),
            journal: journal.map(Mutex::new),
            sequence: AtomicU64::new(0),
            start: Instant::now(),
            journal_failed: AtomicBool::new(false),
            meters: [AudioMeter::default(), AudioMeter::default()],
            startup_progress: AtomicU64::new(0),
            portable_startup: Mutex::new([None, None]),
        }
    }

    /// Reports that a starting track finished one bounded preparation step, such as
    /// verifying a saved section or writing gap silence. It extends only its readiness wait.
    pub fn note_startup_progress(&self) {
        self.startup_progress.fetch_add(1, Ordering::AcqRel);
    }
    /// Counter of preparation steps reported by starting tracks.
    pub fn startup_progress(&self) -> u64 {
        self.startup_progress.load(Ordering::Acquire)
    }
    /// Returns at most one first-packet observation per portable source. Windows
    /// and tracks with no received packet have no observation; absence is not zero.
    pub fn portable_startup_timing(&self) -> Result<Vec<PortableStartupTiming>> {
        Ok(self
            .portable_startup
            .lock()
            .map_err(|_| "Portable startup diagnostics are unavailable")?
            .iter()
            .flatten()
            .cloned()
            .collect())
    }
    /// Captures only the first portable packet's alignment counters. Diagnostics
    /// never gate PCM persistence, and restarts cannot overwrite the first observation.
    #[cfg(any(target_os = "linux", target_os = "macos", test))]
    pub fn note_portable_startup(&self, timing: PortableStartupTiming) {
        if let Ok(mut observations) = self.portable_startup.lock() {
            let slot = &mut observations[Self::index(timing.source)];
            if slot.is_none() {
                *slot = Some(timing);
            }
        }
    }
    /// Updates whether a microphone track was successfully made available.
    pub fn set_has_microphone(&self, value: bool) {
        self.has_microphone.store(value, Ordering::Release);
    }
    /// Reads the current pause, microphone-mute and availability flags without modifying them.
    pub fn snapshot(&self) -> CaptureControlSnapshot {
        CaptureControlSnapshot {
            paused: self.paused.load(Ordering::Acquire),
            microphone_muted: self.microphone_muted.load(Ordering::Acquire),
            has_microphone: self.has_microphone.load(Ordering::Acquire),
        }
    }
    /// Reads only bounded in-memory telemetry, never the session or devices.
    pub fn audio_levels(&self, system_alive: bool, microphone_alive: bool) -> [AudioLevel; 2] {
        let gates = self.snapshot();
        let now_ms = self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        [
            self.meters[0].snapshot(
                self.epoch("system"),
                now_ms,
                system_alive,
                gates.paused,
                false,
            ),
            self.meters[1].snapshot(
                self.epoch("microphone"),
                now_ms,
                microphone_alive,
                gates.paused,
                gates.microphone_muted,
            ),
        ]
    }
    /// Maps the internal microphone/system source labels to their separate gate slots.
    fn index(source: &str) -> usize {
        usize::from(source == "microphone")
    }
    /// Returns the current privacy generation for the selected track.
    pub fn epoch(&self, source: &str) -> u64 {
        self.epochs[Self::index(source)].load(Ordering::Acquire)
    }
    /// Reports whether pause or microphone-only mute currently blocks this track's speech.
    pub fn suppressed(&self, source: &str) -> bool {
        self.paused.load(Ordering::Acquire)
            || (source == "microphone" && self.microphone_muted.load(Ordering::Acquire))
    }
    /// Detects packets belonging to a track generation invalidated by a control transition.
    pub fn changed(&self, source: &str, epoch: u64) -> bool {
        self.epoch(source) != epoch
    }
    /// Converts monotonic elapsed capture time to a saturating sample position.
    pub fn elapsed_samples(&self) -> u64 {
        (self
            .start
            .elapsed()
            .as_nanos()
            .saturating_mul(u128::from(SAMPLE_RATE))
            / 1_000_000_000)
            .min(u128::from(u64::MAX)) as u64
    }
    /// Serializes a pause/resume request across both tracks and journals the requested state.
    pub fn set_paused(&self, value: bool) -> Result<()> {
        self.change(false, value)
    }
    /// Rejects an unavailable microphone before requesting its independent mute transition.
    pub fn set_microphone_muted(&self, value: bool) -> Result<()> {
        if !self.has_microphone.load(Ordering::Acquire) {
            return Err("No microphone track is active".into());
        }
        self.change(true, value)
    }
    /// Invalidates affected packet epochs and waits for in-flight writes; journal failure keeps reopening blocked.
    fn change(&self, microphone_only: bool, value: bool) -> Result<()> {
        let _transition = self
            .transitions
            .lock()
            .map_err(|_| "Capture control is unavailable")?;
        let flag = if microphone_only {
            &self.microphone_muted
        } else {
            &self.paused
        };
        if self.journal_failed.load(Ordering::Acquire) {
            if value {
                flag.store(true, Ordering::Release);
            }
            return Err("Capture control evidence could not be saved. Ignored audio remains blocked; stop and check storage before starting again".into());
        }
        if flag.load(Ordering::Acquire) == value {
            return Ok(());
        }
        let indices: &[usize] = if microphone_only { &[1] } else { &[0, 1] };
        // Closing takes effect before disk I/O. If its receipt fails, remain safely closed.
        // Opening is only made effective after the requested change is durable.
        if value {
            flag.store(true, Ordering::Release);
        }
        for &index in indices {
            self.epochs[index].fetch_add(1, Ordering::AcqRel);
        }
        // Acknowledgment waits for writes already in flight, independently per track.
        for &index in indices {
            drop(
                self.writes[index]
                    .lock()
                    .map_err(|_| "Capture write gate is unavailable")?,
            );
        }
        let paused = if microphone_only {
            self.paused.load(Ordering::Acquire)
        } else {
            value
        };
        let muted = if microphone_only {
            value
        } else {
            self.microphone_muted.load(Ordering::Acquire)
        };
        self.record(
            if microphone_only {
                "microphone-mute"
            } else {
                "pause"
            },
            paused,
            muted,
        )?;
        if !value {
            flag.store(false, Ordering::Release);
        }
        Ok(())
    }
    /// Appends a state-transition event without optional device-gap metadata.
    fn record(&self, kind: &str, paused: bool, microphone_muted: bool) -> Result<()> {
        self.record_event(kind, paused, microphone_muted, None, None, None)
    }
    #[cfg(any(windows, target_os = "linux", target_os = "macos", test))]
    /// Records explicit missing microphone samples separately from intentional pause/mute silence.
    pub fn record_microphone_gap(&self, start_sample: u64, missing_samples: u64) -> Result<()> {
        let snapshot = self.snapshot();
        self.record_event(
            "device-gap",
            snapshot.paused,
            snapshot.microphone_muted,
            Some("microphone"),
            Some(missing_samples),
            Some(start_sample),
        )
    }
    /// Writes and syncs a sequenced control event, retaining a sticky failure state after journal errors.
    fn record_event(
        &self,
        kind: &str,
        paused: bool,
        microphone_muted: bool,
        source: Option<&str>,
        missing_samples: Option<u64>,
        source_start_sample: Option<u64>,
    ) -> Result<()> {
        let Some(journal) = &self.journal else {
            return Ok(());
        };
        let mut file = journal
            .lock()
            .map_err(|_| "Capture control journal is unavailable")?;
        if self.journal_failed.load(Ordering::Acquire) {
            return Err(
                "Capture control journal failed; preserved partial evidence requires review".into(),
            );
        }
        let event = CaptureControlEvent {
            schema_version: 1,
            sequence: self.sequence.fetch_add(1, Ordering::AcqRel) + 1,
            at: chrono::Utc::now().to_rfc3339(),
            requested_sample: self.elapsed_samples(),
            kind: kind.into(),
            paused,
            microphone_muted,
            source: source.map(str::to_owned),
            missing_samples,
            source_start_sample,
            timeline: "ignored audio is represented by silence; device restart time is approximate"
                .into(),
        };
        let result = (|| -> Result<()> {
            serde_json::to_writer(&mut *file, &event)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            self.journal_failed.store(true, Ordering::Release);
        }
        result
    }

    /// Persists samples under the track gate, substituting silence for suppressed or stale-epoch packets.
    pub fn write(
        &self,
        source: &str,
        epoch: u64,
        writer: &mut PcmWriter,
        samples: &[i16],
    ) -> Result<()> {
        // Use packet receipt at this gate, before any filesystem/checkpoint stall.
        // A slow append must not make old captured PCM appear freshly received.
        let received_ms = self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let _write = self.writes[Self::index(source)]
            .lock()
            .map_err(|_| "Capture write gate is unavailable")?;
        if self.suppressed(source) || self.changed(source, epoch) {
            let zeros = [0_i16; 1600];
            for part in samples.chunks(zeros.len()) {
                writer.append(&zeros[..part.len()])?;
            }
            Ok(())
        } else {
            writer.append(samples)?;
            // Adapters already produce effective 16 kHz mono i16 PCM here. Do
            // not meter synthetic pad_to silence, private/stale-epoch packets,
            // transcripts or recognition output. Telemetry cannot fail this write.
            self.meters[Self::index(source)].observe(epoch, samples, received_ms);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioLevelStatus;
    /// Collects synthetic persisted PCM for one track's privacy assertions.
    fn read(store: &SessionStore, source: &str) -> Vec<i16> {
        let mut result = vec![];
        for part in store
            .snapshot()
            .unwrap()
            .parts
            .iter()
            .filter(|p| p.source == source)
        {
            result.extend(
                hound::WavReader::open(store.audio_path(part).unwrap())
                    .unwrap()
                    .samples::<i16>()
                    .map(std::result::Result::unwrap),
            );
        }
        result
    }

    #[test]
    /// Synthetic PCM exercises the real common write gate, including controls,
    /// with independent sources and exact retained samples; no devices are opened.
    fn synthetic_meters_follow_separate_effective_pcm_without_changing_audio_or_gates() {
        let root = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create(root.path(), "Synthetic audio meters", "m", "s", Some("mic"))
                .unwrap();
        let controls = CaptureControls::new(&store).unwrap();
        controls.set_has_microphone(true);
        let mut system = PcmWriter::new(store.clone(), "system").unwrap();
        let mut microphone = PcmWriter::new(store.clone(), "microphone").unwrap();
        let initial = controls.audio_levels(true, false);
        assert_eq!(initial[0].status, AudioLevelStatus::Waiting);
        assert_eq!(initial[1].status, AudioLevelStatus::Unavailable);
        // Estimated gap padding is not an observation of actual captured PCM.
        system.pad_to(2).unwrap();
        assert_eq!(
            controls.audio_levels(true, true)[0].status,
            AudioLevelStatus::Waiting
        );
        controls
            .write("system", 0, &mut system, &[16_384, -16_384])
            .unwrap();
        controls
            .write("microphone", 0, &mut microphone, &[0, 0])
            .unwrap();
        let levels = controls.audio_levels(true, true);
        assert!((levels[0].peak_dbfs.unwrap() - -6.020_599_913).abs() < 0.000_001);
        assert_eq!(levels[1].peak_dbfs, Some(-96.0));
        assert_eq!(levels[1].status, AudioLevelStatus::Active);
        assert!(!levels[0].clipped && !levels[1].clipped);
        // A microphone-only signal must not contaminate the system level.
        controls
            .write("microphone", 0, &mut microphone, &[i16::MIN, i16::MAX])
            .unwrap();
        let levels = controls.audio_levels(true, true);
        assert_eq!(levels[1].peak_dbfs, Some(0.0));
        assert!(levels[1].clipped);
        assert!(!levels[0].clipped);
        controls.set_microphone_muted(true).unwrap();
        let levels = controls.audio_levels(true, true);
        assert_eq!(levels[0].status, AudioLevelStatus::Active);
        assert_eq!(levels[1].status, AudioLevelStatus::Muted);
        assert_eq!(levels[1].peak_dbfs, None);
        controls
            .write("microphone", 0, &mut microphone, &[111, 112])
            .unwrap();
        controls.set_microphone_muted(false).unwrap();
        controls
            .write("microphone", 0, &mut microphone, &[211, 212])
            .unwrap();
        assert_eq!(
            controls.audio_levels(true, true)[1].status,
            AudioLevelStatus::Waiting
        );
        controls
            .write(
                "microphone",
                controls.epoch("microphone"),
                &mut microphone,
                &[8192, -8192],
            )
            .unwrap();
        assert!(!controls.audio_levels(true, true)[1].clipped);
        let system_epoch = controls.epoch("system");
        controls.set_paused(true).unwrap();
        for level in controls.audio_levels(true, true) {
            assert_eq!(level.status, AudioLevelStatus::Paused);
            assert_eq!(
                (level.peak_dbfs, level.rms_dbfs, level.age_ms),
                (None, None, None)
            );
            assert!(!level.clipped);
        }
        controls
            .write("system", system_epoch, &mut system, &[311, 312])
            .unwrap();
        controls.set_paused(false).unwrap();
        controls
            .write("system", system_epoch, &mut system, &[411, 412])
            .unwrap();
        for level in controls.audio_levels(true, true) {
            assert_eq!(level.status, AudioLevelStatus::Waiting);
        }
        controls
            .write("system", controls.epoch("system"), &mut system, &[0, 0])
            .unwrap();
        assert_eq!(controls.audio_levels(true, true)[0].peak_dbfs, Some(-96.0));
        assert_eq!(
            controls.audio_levels(true, false)[1].status,
            AudioLevelStatus::Unavailable
        );
        system.finish().unwrap();
        microphone.finish().unwrap();
        assert_eq!(
            read(&store, "system"),
            [0, 0, 16_384, -16_384, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            read(&store, "microphone"),
            [0, 0, i16::MIN, i16::MAX, 0, 0, 0, 0, 8192, -8192]
        );
        assert_eq!(
            read_capture_control_events(store.folder())
                .unwrap()
                .unwrap()
                .events
                .len(),
            5
        );
        assert_eq!(
            CaptureControls::uncontrolled().audio_levels(true, true)[0].status,
            AudioLevelStatus::Waiting
        );
    }
    #[test]
    /// Checks synthetic PCM from old privacy epochs stays silent after mute/pause and reopening.
    fn synthetic_pause_and_mute_never_release_buffered_old_epoch_speech() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic privacy transitions",
            "m",
            "s",
            Some("mic"),
        )
        .unwrap();
        let controls = CaptureControls::new(&store).unwrap();
        controls.set_has_microphone(true);
        let mut system = PcmWriter::new(store.clone(), "system").unwrap();
        let mut microphone = PcmWriter::new(store.clone(), "microphone").unwrap();
        controls
            .write("microphone", 0, &mut microphone, &[11, 12])
            .unwrap();
        controls.set_microphone_muted(true).unwrap();
        controls
            .write("microphone", 0, &mut microphone, &[99, 99])
            .unwrap();
        controls.write("system", 0, &mut system, &[31, 32]).unwrap();
        controls.set_microphone_muted(false).unwrap();
        // An old OS/helper packet arriving after Unmute remains suppressed.
        controls
            .write("microphone", 0, &mut microphone, &[88, 88])
            .unwrap();
        controls
            .write(
                "microphone",
                controls.epoch("microphone"),
                &mut microphone,
                &[21, 22],
            )
            .unwrap();
        let system_epoch = controls.epoch("system");
        controls.set_paused(true).unwrap();
        controls
            .write("system", system_epoch, &mut system, &[77, 77])
            .unwrap();
        controls.set_paused(false).unwrap();
        controls
            .write("system", system_epoch, &mut system, &[66, 66])
            .unwrap();
        controls
            .write("system", controls.epoch("system"), &mut system, &[41, 42])
            .unwrap();
        microphone.finish().unwrap();
        system.finish().unwrap();
        assert_eq!(read(&store, "microphone"), [11, 12, 0, 0, 0, 0, 21, 22]);
        assert_eq!(read(&store, "system"), [31, 32, 0, 0, 0, 0, 41, 42]);
        let lines = std::fs::read_to_string(store.folder().join("capture-controls.jsonl")).unwrap();
        assert_eq!(lines.lines().count(), 5);
        for (index, line) in lines.lines().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["sequence"], index + 1);
        }
        assert!(!store.snapshot().unwrap().capture_incomplete);
    }

    #[test]
    /// Checks truncated synthetic logs retain the confirmed prefix and reject unsupported complete events.
    fn interrupted_control_log_preserves_prefix_and_refuses_unknown_complete_events() {
        let root = tempfile::tempdir().unwrap();
        assert!(read_capture_control_events(root.path()).unwrap().is_none());
        let store = SessionStore::create(root.path(), "Synthetic control recovery", "m", "s", None)
            .unwrap();
        let controls = CaptureControls::new(&store).unwrap();
        controls.set_paused(true).unwrap();
        let path = store.folder().join("capture-controls.jsonl");
        drop(controls);
        let original = std::fs::read(&path).unwrap();
        let mut interrupted = original.clone();
        interrupted.extend_from_slice(b"{\"schemaVersion\":1");
        std::fs::write(&path, &interrupted).unwrap();
        let log = read_capture_control_events(store.folder())
            .unwrap()
            .unwrap();
        assert_eq!(log.events.len(), 2);
        assert!(log.incomplete_tail);
        assert_eq!(std::fs::read(&path).unwrap(), interrupted);
        let mut changed: serde_json::Value =
            serde_json::from_slice(original.split(|b| *b == b'\n').next().unwrap()).unwrap();
        changed["schemaVersion"] = serde_json::json!(99);
        let mut unknown = serde_json::to_vec(&changed).unwrap();
        unknown.push(b'\n');
        std::fs::write(&path, &unknown).unwrap();
        assert!(read_capture_control_events(store.folder()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), unknown);
    }

    #[test]
    /// Checks synthetic journal write failure leaves speech blocked and preserves the failed evidence file.
    fn a_control_receipt_write_failure_keeps_private_audio_blocked_and_refuses_resume() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic failed control receipt",
            "m",
            "s",
            None,
        )
        .unwrap();
        let path = store.folder().join("synthetic-read-only-control-journal");
        std::fs::write(&path, b"retained evidence").unwrap();
        let controls = CaptureControls::with_journal(Some(File::open(&path).unwrap()));
        assert!(controls.set_paused(true).is_err());
        assert!(controls.snapshot().paused);
        assert!(controls.set_paused(false).is_err());
        assert!(controls.snapshot().paused);
        let mut writer = PcmWriter::new(store.clone(), "system").unwrap();
        controls
            .write("system", controls.epoch("system"), &mut writer, &[987, 654])
            .unwrap();
        writer.finish().unwrap();
        assert_eq!(read(&store, "system"), [0, 0]);
        assert_eq!(std::fs::read(&path).unwrap(), b"retained evidence");
        assert!(!store.snapshot().unwrap().parts[0].has_signal);
    }
}
