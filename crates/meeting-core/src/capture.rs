use crate::{AudioDevice, CaptureAudioLevels, CaptureSnapshot, Result, SessionStore};
use serde::{Deserialize, Serialize};
#[path = "capture_controls.rs"]
mod controls;
use controls::CaptureControls;
pub use controls::{
    read_capture_control_events, CaptureControlEvent, CaptureControlLog, CaptureControlSnapshot,
    PortableStartupTiming,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

// Release builds resolve the same three readiness clock reads directly to std.
// Only a scoped unit-test clock can separate fixture scheduling from idle time.
#[cfg(all(not(test), any(windows, target_os = "linux", target_os = "macos")))]
type ReadinessClock = Instant;
#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
struct TestReadinessClock {
    read: Box<dyn FnMut(u64) -> Instant>,
    observed_progress: u64,
}
#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
thread_local! {
    static READINESS_TEST_CLOCK: std::cell::RefCell<Option<TestReadinessClock>> = const { std::cell::RefCell::new(None) };
}
#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
struct ReadinessClock;
#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
impl ReadinessClock {
    /// Use real time unless this test thread explicitly owns a scoped clock.
    fn now() -> Instant {
        READINESS_TEST_CLOCK.with(|clock| match clock.borrow_mut().as_mut() {
            Some(clock) => (clock.read)(clock.observed_progress),
            None => Instant::now(),
        })
    }
    /// Bind virtual time to the exact progress snapshot the policy inspected,
    /// never to a second racing read of the recorder's counter.
    fn observe(progress: u64) {
        READINESS_TEST_CLOCK.with(|clock| {
            if let Some(clock) = clock.borrow_mut().as_mut() {
                clock.observed_progress = progress;
            }
        });
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
type TrackRecorder = fn(
    SessionStore,
    &str,
    Option<String>,
    Arc<AtomicBool>,
    mpsc::SyncSender<std::result::Result<(), String>>,
    Arc<CaptureControls>,
) -> Result<()>;

#[cfg(windows)]
#[path = "windows_capture.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux_capture.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos_capture.rs"]
mod platform;
#[cfg(any(target_os = "linux", target_os = "macos", test))]
#[path = "portable_capture.rs"]
mod portable;
// Type-checks and unit-tests the macOS adapter's pure parsing/argument code on
// other build hosts; it never starts the Swift helper there.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos_capture.rs"]
#[allow(dead_code)]
mod macos_adapter_checks;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOptions {
    pub output_id: Option<String>,
    pub microphone_id: Option<String>,
    pub include_microphone: bool,
}

/// One owned capture thread and its own stop request. An explicit stop sets every
/// track's flag; a track that missed its readiness deadline is stopped alone.
type Track = (&'static str, JoinHandle<()>, Arc<AtomicBool>);

/// A track owns its capture client and disk writer on one dedicated thread.
/// There is no unbounded queue between the device and durable storage.
pub struct CaptureController {
    meeting_id: String,
    stop: Arc<AtomicBool>,
    tracks: Vec<Track>,
    store: SessionStore,
    controls: Arc<CaptureControls>,
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    microphone_id: Option<String>,
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    recorder: TrackRecorder,
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    reconnect_attempts: u8,
    /// Longest wait for a track's readiness without any reported startup progress.
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    ready_timeout: Duration,
    /// Set from outside (an explicit stop) to end a running readiness wait at once.
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    start_cancel: Arc<AtomicBool>,
}

impl CaptureController {
    /// Reads bounded passive portable alignment counters without reading audio or
    /// changing capture state. Available after stop while this controller is retained.
    pub fn portable_startup_timing(&self) -> Result<Vec<PortableStartupTiming>> {
        self.controls.portable_startup_timing()
    }
    /// A handle that ends a running microphone reconnect at once, so stopping the meeting
    /// never waits for a slow device. The started track is stopped like a missed deadline.
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    pub fn reconnect_canceller(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.start_cancel)
    }
    /// True only for an ended, explicitly selected microphone in this still-running capture.
    pub fn can_reconnect_microphone(&self) -> bool {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            self.is_running()
                && self.microphone_id.is_some()
                && self.reconnect_attempts < 32
                && !self
                    .tracks
                    .iter()
                    .any(|(source, track, _)| *source == "microphone" && !track.is_finished())
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            false
        }
    }
    /// Explicitly reconnects the originally selected microphone, leaving system capture owned.
    pub fn reconnect_microphone(&mut self) -> Result<()> {
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err("Microphone reconnect is unavailable on this platform".into())
        }
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            if self.stop.load(Ordering::Acquire) || !self.is_running() {
                return Err("No capture is running; start a new recording".into());
            }
            let id = self.microphone_id.clone().ok_or("No explicitly selected microphone belongs to this recording; start a new recording with the desired device")?;
            if self
                .tracks
                .iter()
                .any(|(source, track, _)| *source == "microphone" && !track.is_finished())
            {
                return Err(
                    "The microphone is still running or starting; a second track was not opened"
                        .into(),
                );
            }
            if self.reconnect_attempts >= 32 {
                return Err("Microphone reconnect limit reached; stop and check the device before a new recording".into());
            }
            self.reconnect_attempts += 1;
            // A stop request keeps setting the canceller while this reconnect runs.
            self.start_cancel.store(false, Ordering::Release);
            if let Some(index) = self
                .tracks
                .iter()
                .position(|(source, _, _)| *source == "microphone")
            {
                let (_, track, _) = self.tracks.remove(index);
                let _ = track.join();
            }
            self.controls.set_has_microphone(false);
            let result = self.start_track("microphone", Some(id), self.recorder);
            // A failed or late start was stopped by `start_track`; it can never record
            // audio that the app reports as missing or cannot mute.
            if result.is_ok() {
                self.controls.set_has_microphone(true);
            }
            // Never erase the interruption flag or stop/reopen the healthy system track.
            let _ = self.store.notice(if result.is_ok() {
                "Microphone reconnected to its original device. Missing audio is not recovered; pause and app-mute remain in effect."
            } else {
                "Microphone reconnect failed. Saved audio and the system track remain available; check the original device and retry."
            });
            result
        }
    }
    /// Starts independent system/microphone writers; a microphone startup failure leaves system capture running.
    pub fn start(store: SessionStore, options: CaptureOptions) -> Result<Self> {
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            let _ = (store, options);
            Err("This operating system has no supported audio capture adapter.".into())
        }
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            Self::start_with_recorder(store, options, platform::record)
        }
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Keeps thread ownership/readiness identical for native and synthetic recorders.
    fn start_with_recorder(
        store: SessionStore,
        options: CaptureOptions,
        recorder: TrackRecorder,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let controls = Arc::new(CaptureControls::new(&store)?);
        let mut controller = Self {
            meeting_id: store.preview_snapshot(0)?.id,
            stop,
            tracks: vec![],
            store,
            controls,
            microphone_id: reconnect_target(
                options
                    .include_microphone
                    .then(|| options.microphone_id.clone())
                    .flatten(),
                cfg!(target_os = "macos"),
            ),
            recorder,
            reconnect_attempts: 0,
            ready_timeout: Duration::from_secs(if cfg!(target_os = "macos") { 60 } else { 10 }),
            start_cancel: Arc::new(AtomicBool::new(false)),
        };
        if let Err(error) = controller.start_track("system", options.output_id, recorder) {
            if let Err(cleanup) = controller.stop() {
                return Err(format!("{error}. Capture cleanup also failed: {cleanup}").into());
            }
            return Err(error);
        }
        if options.include_microphone {
            // A microphone startup failure must not terminate a working system track.
            if let Err(error) =
                controller.start_track("microphone", options.microphone_id, recorder)
            {
                // Diagnostic persistence must not release this controller and stop
                // the already healthy system writer. The store retains a UI fallback.
                let _ = controller.store.warn(&format!(
                    "Microphone could not start: {error}. System audio continues."
                ));
            } else {
                controller.controls.set_has_microphone(true);
            }
        }
        Ok(controller)
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Starts one owned capture thread and waits for its readiness acknowledgment. The
    /// deadline restarts whenever the track reports startup progress (verifying saved
    /// audio, writing gap silence), so a long meeting cannot exhaust it; a device that
    /// makes no progress is stopped instead of recording unseen after an error.
    fn start_track(
        &mut self,
        source: &'static str,
        device_id: Option<String>,
        recorder: TrackRecorder,
    ) -> Result<()> {
        let track_stop = Arc::new(AtomicBool::new(self.stop.load(Ordering::Acquire)));
        let stop = Arc::clone(&track_stop);
        let store = self.store.clone();
        let controls = Arc::clone(&self.controls);
        let (ready, result) = mpsc::sync_channel(1);
        let track = thread::Builder::new().name(format!("meeting-{source}-capture")).spawn(move || {
            if let Err(error) = recorder(store.clone(), source, device_id, stop, ready, controls) {
                let _ = store.warn(&format!("{source}: capture stopped: {error}. Saved audio is retained; check the device before recording again."));
            }
        })?;
        self.tracks.push((source, track, Arc::clone(&track_stop)));
        let mut progress = self.controls.startup_progress();
        #[cfg(test)]
        ReadinessClock::observe(progress);
        let mut deadline = ReadinessClock::now() + self.ready_timeout;
        loop {
            match result.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(error)) => return Err(error.into()),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(
                    "Audio capture ended before readiness; check the retained capture diagnostic"
                        .into(),
                ),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.start_cancel.load(Ordering::Acquire) {
                        track_stop.store(true, Ordering::Release);
                        return Err(
                            "The recording is stopping; the starting track was stopped".into()
                        );
                    }
                    let current = self.controls.startup_progress();
                    #[cfg(test)]
                    ReadinessClock::observe(current);
                    if current != progress {
                        progress = current;
                        deadline = ReadinessClock::now() + self.ready_timeout;
                    } else if ReadinessClock::now() >= deadline {
                        // Stop before the receiver is dropped: a late acknowledgment then
                        // finds its stop request and finalizes instead of recording.
                        track_stop.store(true, Ordering::Release);
                        return Err(
                            "Audio device did not become ready in time; the late start was stopped"
                                .into(),
                        );
                    }
                }
            }
        }
    }

    /// Returns capture liveness and a bounded session preview without cloning the full transcript.
    pub fn snapshot(&self) -> Result<CaptureSnapshot> {
        Ok(CaptureSnapshot {
            running: self.is_running(),
            session: self.store.preview_snapshot(16)?,
        })
    }

    /// Reports whether an owned track is still alive before an explicit stop request.
    pub fn is_running(&self) -> bool {
        !self.stop.load(Ordering::Acquire)
            && self.tracks.iter().any(|(_, track, _)| !track.is_finished())
    }

    /// Returns pause/mute state and derives microphone availability from its live capture thread.
    pub fn controls(&self) -> CaptureControlSnapshot {
        let mut snapshot = self.controls.snapshot();
        snapshot.has_microphone = snapshot.has_microphone
            && !self.stop.load(Ordering::Acquire)
            && self
                .tracks
                .iter()
                .any(|(source, track, _)| *source == "microphone" && !track.is_finished());
        snapshot
    }

    /// Read-only real-PCM telemetry. Dead/missing tracks are unavailable even if
    /// their old samples are recent; an explicit stop removes the whole snapshot.
    pub fn audio_levels(&self) -> Option<CaptureAudioLevels> {
        if self.stop.load(Ordering::Acquire) {
            return None;
        }
        let alive = |name| {
            self.tracks
                .iter()
                .any(|(source, track, _)| *source == name && !track.is_finished())
        };
        let [system, microphone] = self.controls.audio_levels(
            alive("system"),
            self.controls.snapshot().has_microphone && alive("microphone"),
        );
        Some(CaptureAudioLevels {
            meeting_id: self.meeting_id.clone(),
            system,
            microphone,
        })
    }

    /// Changes both track gates only while capture runs; successful return excludes old-epoch speech writes.
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        if !self.is_running() {
            return Err("No capture is running".into());
        }
        self.controls.set_paused(paused)
    }

    /// Changes only the live microphone gate; system capture continues independently.
    pub fn set_microphone_muted(&self, muted: bool) -> Result<()> {
        if !self.is_running() || self.stopped_tracks().contains(&"microphone") {
            return Err("No microphone capture is running".into());
        }
        self.controls.set_microphone_muted(muted)
    }

    /// Tracks ("system", "microphone") whose capture ended before an explicit stop, while
    /// another track may still record. A partly failed capture must stay visible.
    pub fn stopped_tracks(&self) -> Vec<&'static str> {
        if self.stop.load(Ordering::Acquire) {
            return Vec::new();
        }
        self.tracks
            .iter()
            .filter(|(_, track, _)| track.is_finished())
            .map(|(source, _, _)| *source)
            .collect()
    }

    /// Requests owned tracks to stop, joins their writers and marks the retained session recorded.
    pub fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        for (_, _, track_stop) in &self.tracks {
            track_stop.store(true, Ordering::Release);
        }
        let mut failures = vec![];
        for (_, track, _) in self.tracks.drain(..) {
            if track.join().is_err() {
                if let Err(error) = self
                    .store
                    .warn("A recording thread stopped unexpectedly. Confirmed audio is retained.")
                {
                    failures.push(format!("Recording warning could not be saved: {error}"));
                }
            }
        }
        // No diagnostic error may skip joining another owned disk writer or the
        // final status attempt. Report every failed persistence step afterwards.
        if let Err(error) = self.store.set_status("recorded") {
            failures.push(format!(
                "Recorded session status could not be saved: {error}"
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; ").into())
        }
    }
}

impl Drop for CaptureController {
    /// Stops only this controller's owned capture threads when the controller is released.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Only an explicitly chosen device can be reconnected. The macOS `default` entry follows
/// whatever input is current, so reopening it could record a different microphone.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn reconnect_target(id: Option<String>, macos: bool) -> Option<String> {
    id.filter(|id| !id.is_empty() && (!macos || id != "default"))
}

/// Pads a resumed track with silence up to `now()`, reading the clock again after every
/// bounded five-second step. Writing a long gap takes time while system audio advances;
/// without the re-read, the first resumed packet would be placed early by that time and the
/// recorded gap would end too soon. Returns the reached position.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn pad_to_clock(
    writer: &mut crate::PcmWriter,
    now: &dyn Fn() -> u64,
    stop: &AtomicBool,
    progress: &dyn Fn(),
) -> Result<u64> {
    loop {
        let target = now();
        if writer.written_samples() >= target {
            return Ok(writer.written_samples());
        }
        if stop.load(Ordering::Acquire) {
            return Err("Capture stopped while aligning the reconnected microphone".into());
        }
        let step = target.min(writer.written_samples() + u64::from(crate::SAMPLE_RATE) * 5);
        writer.pad_to(step)?;
        progress();
    }
}

/// The first driver-reported discontinuity of a track, with its position in the saved
/// track, so the affected range can be found next to the transcript section times.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn discontinuity_warning(source: &str, saved_samples: u64) -> String {
    let s = saved_samples / u64::from(crate::SAMPLE_RATE);
    let at = if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    };
    format!("{source}: audio driver reported a discontinuity at {at} of the saved track. Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.")
}

/// Enumerates supported platform devices on a separate thread without starting a recording.
pub fn list_devices() -> Result<Vec<AudioDevice>> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    {
        thread::spawn(platform::list_devices)
            .join()
            .map_err(|_| "Device enumeration stopped unexpectedly")?
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err("This operating system has no supported audio device adapter.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discontinuity_warning_names_its_position_in_the_saved_track() {
        assert!(discontinuity_warning("system", 0).starts_with(
            "system: audio driver reported a discontinuity at 00:00 of the saved track."
        ));
        assert!(discontinuity_warning("microphone", 16_000 * 192 + 15_999)
            .starts_with("microphone: audio driver reported a discontinuity at 03:12 of"));
        assert!(discontinuity_warning("system", 16_000 * 3_725).contains(" at 1:02:05 of"));
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    fn reconnect_fixture(
        store: SessionStore,
        source: &str,
        id: Option<String>,
        stop: Arc<AtomicBool>,
        ready: mpsc::SyncSender<std::result::Result<(), String>>,
        controls: Arc<CaptureControls>,
    ) -> Result<()> {
        let mut writer = crate::PcmWriter::for_capture(store.clone(), source, &|| {
            controls.note_startup_progress()
        })?;
        let before = writer.written_samples();
        let target = controls.elapsed_samples();
        writer.pad_to(target)?;
        if source == "microphone" {
            assert_eq!(id.as_deref(), Some("original-mic"));
            if before > 0 && target > before {
                controls.record_microphone_gap(before, target - before)?;
            }
            let marker = store.folder().join("synthetic-first-mic");
            if std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(marker)
                .is_ok()
            {
                writer.append(&[71, -72, 73])?;
                writer.finish()?;
                ready.send(Ok(()))?;
                return Err("Synthetic device interruption".into());
            }
        }
        ready.send(Ok(()))?;
        while !stop.load(Ordering::Acquire) {
            controls.write(source, controls.epoch(source), &mut writer, &[81, -82, 83])?;
            writer.checkpoint()?;
            thread::sleep(Duration::from_millis(5));
        }
        writer.finish()
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn reconnect_preserves_original_audio_system_progress_and_privacy_gates() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic reconnect", "m", "s", Some("mic"))
            .unwrap();
        let mut controller = CaptureController::start_with_recorder(
            store.clone(),
            CaptureOptions {
                include_microphone: true,
                microphone_id: Some("original-mic".into()),
                ..Default::default()
            },
            reconnect_fixture,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !controller.stopped_tracks().contains(&"microphone") {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let original = store
            .snapshot()
            .unwrap()
            .parts
            .into_iter()
            .find(|p| p.source == "microphone")
            .unwrap();
        let path = store.audio_path(&original).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        controller.set_paused(true).unwrap();
        controller.controls.set_microphone_muted(true).unwrap();
        thread::sleep(Duration::from_millis(30));
        controller.reconnect_microphone().unwrap();
        assert!(controller.controls().has_microphone);
        assert!(controller.controls().paused);
        assert!(controller.controls().microphone_muted);
        assert!(
            controller.reconnect_microphone().is_err(),
            "No second live microphone may start"
        );
        thread::sleep(Duration::from_millis(30));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        controller.set_paused(false).unwrap();
        thread::sleep(Duration::from_millis(30));
        controller.stop().unwrap();
        let snapshot = store.snapshot().unwrap();
        assert!(
            snapshot.capture_incomplete,
            "Recovery must not erase the real interruption"
        );
        let log = read_capture_control_events(store.folder())
            .unwrap()
            .unwrap();
        assert!(log.events.iter().any(|e| e.kind == "device-gap"));
        assert!(
            snapshot
                .parts
                .iter()
                .filter(|p| p.source == "system")
                .map(|p| p.confirmed_sample_count)
                .sum::<u64>()
                > 3
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Synthetic slow device: the first microphone start ends at once. Reconnect
    /// reports eight 50ms steps only when requested. Clock-controlled cases wait
    /// for acknowledgment of each step and for the caller's final readiness gate.
    fn slow_reconnect_fixture(
        store: SessionStore,
        source: &str,
        _: Option<String>,
        stop: Arc<AtomicBool>,
        ready: mpsc::SyncSender<std::result::Result<(), String>>,
        controls: Arc<CaptureControls>,
    ) -> Result<()> {
        if source == "microphone" {
            let first = store.folder().join("synthetic-first-mic");
            if std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(first)
                .is_ok()
            {
                ready.send(Ok(()))?;
                return Err("Synthetic device interruption".into());
            }
            let reports = store.folder().join("synthetic-progress").exists();
            let controlled = store.folder().join("synthetic-controlled-clock").exists();
            for step in 0..8 {
                if reports && controlled {
                    let ack = store.folder().join(format!("synthetic-startup-ack-{step}"));
                    while !ack.exists() && !stop.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(5));
                    }
                    if stop.load(Ordering::Acquire) {
                        return Err("Synthetic startup stopped before readiness".into());
                    }
                }
                thread::sleep(Duration::from_millis(50));
                if reports {
                    controls.note_startup_progress();
                }
            }
            if reports && controlled {
                let allow = store.folder().join("synthetic-allow-ready");
                while !allow.exists() && !stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(5));
                }
                if stop.load(Ordering::Acquire) {
                    return Err("Synthetic startup stopped before readiness".into());
                }
            }
            if ready.send(Ok(())).is_err() {
                return Err("Capture controller was closed".into());
            }
            while !stop.load(Ordering::Acquire) {
                std::fs::write(store.folder().join("synthetic-late-mic-recorded"), b"")?;
                thread::sleep(Duration::from_millis(5));
            }
            return Ok(());
        }
        ready.send(Ok(()))?;
        while !stop.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    fn interrupted_slow_microphone(
        progress: bool,
    ) -> (tempfile::TempDir, SessionStore, CaptureController) {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic slow reconnect",
            "m",
            "s",
            Some("mic"),
        )
        .unwrap();
        if progress {
            std::fs::write(store.folder().join("synthetic-progress"), b"").unwrap();
            std::fs::write(store.folder().join("synthetic-controlled-clock"), b"").unwrap();
        }
        let mut controller = CaptureController::start_with_recorder(
            store.clone(),
            CaptureOptions {
                include_microphone: true,
                microphone_id: Some("original-mic".into()),
                ..Default::default()
            },
            slow_reconnect_fixture,
        )
        .unwrap();
        controller.ready_timeout = Duration::from_millis(150);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !controller.can_reconnect_microphone() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        (root, store, controller)
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Restore any outer clock even if a fixture assertion unwinds. Other test
    /// threads, recorders and release builds always keep their real clock.
    fn with_readiness_clock<T>(
        clock: impl FnMut(u64) -> Instant + 'static,
        work: impl FnOnce() -> T,
    ) -> T {
        struct Restore(Option<TestReadinessClock>);
        impl Drop for Restore {
            fn drop(&mut self) {
                READINESS_TEST_CLOCK.with(|clock| *clock.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(READINESS_TEST_CLOCK.with(|slot| {
            slot.borrow_mut().replace(TestReadinessClock {
                read: Box::new(clock),
                observed_progress: 0,
            })
        }));
        work()
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Exercise the actual readiness loop while eight acknowledged reports move
    /// only the synthetic clock. A final idle turn tests renewal itself; the
    /// unchanged five-second real harness guard still catches a stuck fixture.
    fn reconnect_with_progress_clock(
        controller: &mut CaptureController,
        store: &SessionStore,
        final_idle: Duration,
        release_ready: bool,
    ) -> Result<()> {
        let base = Instant::now();
        let real_started = Instant::now();
        let folder = store.folder().to_owned();
        let mut previous = None;
        with_readiness_clock(
            move |count| {
                assert!(
                    real_started.elapsed() < Duration::from_secs(5),
                    "Synthetic readiness fixture did not finish"
                );
                let idle = count >= 8 && previous == Some(count);
                if idle && release_ready {
                    std::fs::write(folder.join("synthetic-allow-ready"), b"").unwrap();
                }
                previous = Some(count);
                std::fs::write(folder.join(format!("synthetic-startup-ack-{count}")), b"").unwrap();
                base + Duration::from_millis(count * 50)
                    + if idle { final_idle } else { Duration::ZERO }
            },
            || controller.reconnect_microphone(),
        )
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_reconnect_that_misses_its_deadline_is_stopped_and_can_be_retried() {
        let (_root, store, mut controller) = interrupted_slow_microphone(false);
        let error = controller.reconnect_microphone().unwrap_err().to_string();
        assert!(error.contains("did not become ready in time"), "{error}");
        assert!(!controller.controls().has_microphone);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !controller.can_reconnect_microphone() {
            assert!(
                std::time::Instant::now() < deadline,
                "the late start must end"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !store.folder().join("synthetic-late-mic-recorded").exists(),
            "a late microphone must never record while the app reports it missing"
        );
        assert!(controller.is_running(), "system audio keeps recording");
        controller.stop().unwrap();
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    /// Acknowledged synthetic progress must renew the idle deadline before readiness.
    fn reported_startup_progress_extends_the_readiness_wait() {
        let (_root, store, mut controller) = interrupted_slow_microphone(true);
        reconnect_with_progress_clock(&mut controller, &store, Duration::from_millis(100), true)
            .unwrap();
        assert!(controller.controls().has_microphone);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !store.folder().join("synthetic-late-mic-recorded").exists() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        controller.stop().unwrap();
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    /// A final idle interval must still stop the late track and retain healthy capture.
    fn reported_startup_progress_does_not_waive_the_idle_deadline() {
        let (_root, store, mut controller) = interrupted_slow_microphone(true);
        let error = reconnect_with_progress_clock(
            &mut controller,
            &store,
            Duration::from_millis(200),
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("did not become ready in time"), "{error}");
        assert!(!controller.controls().has_microphone);
        assert!(
            controller.is_running(),
            "healthy system capture is still owned"
        );
        controller.stop().unwrap();
        assert!(!store.folder().join("synthetic-late-mic-recorded").exists());
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_stop_request_ends_a_running_reconnect_without_waiting_for_the_device() {
        let (_root, store, mut controller) = interrupted_slow_microphone(false);
        controller.ready_timeout = Duration::from_secs(30);
        let cancel = controller.reconnect_canceller();
        let requester = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            cancel.store(true, Ordering::Release);
        });
        let started = std::time::Instant::now();
        let error = controller.reconnect_microphone().unwrap_err().to_string();
        requester.join().unwrap();
        assert!(error.contains("recording is stopping"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!controller.controls().has_microphone);
        controller.stop().unwrap();
        assert!(!store.folder().join("synthetic-late-mic-recorded").exists());
    }

    #[test]
    fn gap_padding_follows_the_clock_while_the_gap_is_written() {
        let root = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create(root.path(), "Synthetic clock", "m", "s", Some("mic")).unwrap();
        let mut writer = crate::PcmWriter::new(store, "microphone").unwrap();
        // Synthetic shared clock: writing the gap costs three seconds of wall time at first.
        let rate = u64::from(crate::SAMPLE_RATE);
        let reads = std::cell::Cell::new(0_u64);
        let now = || {
            reads.set(reads.get() + 1);
            rate * 10 + rate * 3 * reads.get().min(4)
        };
        let steps = std::cell::Cell::new(0);
        let reached = pad_to_clock(&mut writer, &now, &AtomicBool::new(false), &|| {
            steps.set(steps.get() + 1)
        })
        .unwrap();
        assert_eq!(
            reached,
            rate * 22,
            "the gap covers the time spent writing it"
        );
        assert_eq!(writer.written_samples(), reached);
        assert!(steps.get() >= 5);
        let stopped = pad_to_clock(&mut writer, &|| rate * 60, &AtomicBool::new(true), &|| {});
        assert!(stopped.is_err());
        writer.finish().unwrap();
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn only_an_explicit_device_is_a_reconnect_target() {
        assert_eq!(
            reconnect_target(Some("mic-1".into()), false).as_deref(),
            Some("mic-1")
        );
        assert_eq!(
            reconnect_target(Some("default".into()), false).as_deref(),
            Some("default")
        );
        assert_eq!(reconnect_target(Some("default".into()), true), None);
        assert_eq!(reconnect_target(Some(String::new()), false), None);
        assert_eq!(reconnect_target(None, false), None);
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    /// Replaces only the device backend; production readiness, ownership and PCM
    /// journaling still run. A directory deterministically blocks manifest backup.
    fn synthetic_recorder_with_failed_microphone(
        store: SessionStore,
        source: &str,
        _: Option<String>,
        stop: Arc<AtomicBool>,
        ready: mpsc::SyncSender<std::result::Result<(), String>>,
        _: Arc<CaptureControls>,
    ) -> Result<()> {
        if source == "microphone" {
            ready.send(Err("Synthetic microphone startup failure".into()))?;
            return Err("Synthetic microphone startup failure".into());
        }
        let mut writer = crate::PcmWriter::new(store, source)?;
        writer.append(&[11, -12, 13, -14])?;
        writer.checkpoint()?;
        ready.send(Ok(()))?;
        while !stop.load(Ordering::Acquire) {
            writer.append(&[21, -22, 23, -24])?;
            writer.checkpoint()?;
            thread::sleep(Duration::from_millis(5));
        }
        writer.finish()
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn failed_microphone_warning_does_not_stop_the_healthy_system_writer() {
        let root = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create(root.path(), "Synthetic diagnostics", "m", "s", None).unwrap();
        let manifest = store.folder().join("session.json");
        let original = std::fs::read(&manifest).unwrap();
        let backup = store.folder().join("session.json.bak");
        std::fs::create_dir(&backup).unwrap();
        let mut controller = CaptureController::start_with_recorder(
            store.clone(),
            CaptureOptions {
                include_microphone: true,
                ..CaptureOptions::default()
            },
            synthetic_recorder_with_failed_microphone,
        )
        .expect("Only the optional microphone failed; system ownership must survive");
        assert!(controller.is_running());
        assert_eq!(
            controller.audio_levels().unwrap().microphone.status,
            crate::AudioLevelStatus::Unavailable
        );
        let initial = controller.snapshot().unwrap().session.parts[0].confirmed_sample_count;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while controller.controls().has_microphone {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        loop {
            let snapshot = controller.snapshot().unwrap();
            assert!(snapshot.running);
            assert!(snapshot.session.capture_incomplete);
            assert!(snapshot.session.warnings.iter().any(|warning| {
                warning.contains("Microphone could not start")
                    && warning.contains("System audio continues")
            }));
            assert!(snapshot
                .session
                .warnings
                .iter()
                .any(|warning| warning.contains("diagnostic could not be saved")));
            if snapshot.session.parts[0].confirmed_sample_count > initial {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(std::fs::read(&manifest).unwrap(), original);
        let error = controller.stop().unwrap_err().to_string();
        assert!(error.contains("Recorded session status could not be saved"));
        let data = store.snapshot().unwrap();
        assert!(data.parts[0].audio_closed);
        let samples: Vec<i16> = hound::WavReader::open(store.audio_path(&data.parts[0]).unwrap())
            .unwrap()
            .samples::<i16>()
            .map(std::result::Result::unwrap)
            .collect();
        assert_eq!(&samples[..4], &[11, -12, 13, -14]);
        assert!(samples.len() > initial as usize);
        assert_eq!(samples.len() as u64, data.parts[0].confirmed_sample_count);
        // Remove only this test's empty fault directory. A later successful save
        // persists the retained warning without replacing any original journal.
        std::fs::remove_dir(backup).unwrap();
        controller.stop().unwrap();
        let saved: crate::SessionData =
            serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        assert_eq!(saved.status, "recorded");
        assert!(saved.capture_incomplete);
        assert!(saved
            .warnings
            .iter()
            .any(|warning| warning.contains("Microphone could not start")));
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn failed_panic_warning_still_joins_other_writers_and_attempts_final_status() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic stop diagnostics", "m", "s", None)
            .unwrap();
        let backup = store.folder().join("session.json.bak");
        std::fs::create_dir(&backup).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let tail_stop = Arc::clone(&stop);
        let tail_store = store.clone();
        let tail_joined = Arc::new(AtomicBool::new(false));
        let tail_finished = Arc::clone(&tail_joined);
        let (waiting, tail_waiting) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let tail = thread::spawn(move || {
            while !tail_stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            waiting.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(10)).unwrap();
            let mut writer = crate::PcmWriter::new(tail_store, "system").unwrap();
            writer.append(&[31, -32, 33, -34]).unwrap();
            writer.finish().unwrap();
            tail_finished.store(true, Ordering::Release);
        });
        let mut controller = CaptureController {
            meeting_id: store.preview_snapshot(0).unwrap().id,
            stop,
            tracks: vec![
                (
                    "microphone",
                    thread::spawn(|| panic!("Synthetic track panic")),
                    Arc::new(AtomicBool::new(false)),
                ),
                ("system", tail, Arc::new(AtomicBool::new(false))),
            ],
            controls: Arc::new(CaptureControls::new(&store).unwrap()),
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            microphone_id: None,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            recorder: platform::record,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            reconnect_attempts: 0,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            ready_timeout: Duration::from_secs(10),
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            start_cancel: Arc::new(AtomicBool::new(false)),
            store: store.clone(),
        };
        let (finished, stopped) = mpsc::sync_channel(1);
        let stopping = thread::spawn(move || {
            let result = controller.stop().map_err(|error| error.to_string());
            finished
                .send((
                    result,
                    tail_joined.load(Ordering::Acquire),
                    controller.tracks.len(),
                ))
                .unwrap();
        });
        tail_waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !store
            .snapshot()
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.contains("diagnostic could not be saved"))
        {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let early = stopped.recv_timeout(Duration::from_millis(200)).ok();
        // Repair the manifest fault while the second writer is deliberately
        // blocked, so its join and the final status attempt are both observable.
        std::fs::remove_dir(backup).unwrap();
        release.send(()).unwrap();
        let result = early
            .clone()
            .unwrap_or_else(|| stopped.recv_timeout(Duration::from_secs(5)).unwrap());
        stopping.join().unwrap();
        assert!(
            early.is_none(),
            "stop returned before its owned writer finished"
        );
        assert!(
            result.1,
            "the final PCM writer must finish before stop returns"
        );
        assert_eq!(result.2, 0);
        let error = result.0.unwrap_err();
        assert!(error.contains("Recording warning could not be saved"));
        assert!(!error.contains("Recorded session status could not be saved"));
        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.status, "recorded");
        assert!(snapshot.capture_incomplete);
        assert!(snapshot.parts[0].audio_closed);
        assert_eq!(snapshot.parts[0].confirmed_sample_count, 4);
        let saved: crate::SessionData =
            serde_json::from_slice(&std::fs::read(store.folder().join("session.json")).unwrap())
                .unwrap();
        assert_eq!(saved.status, "recorded");
        assert!(saved
            .warnings
            .iter()
            .any(|warning| warning.contains("recording thread stopped unexpectedly")));
    }

    #[test]
    fn unsaved_notice_remains_visible_without_claiming_incomplete_audio() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic notice", "m", "s", None).unwrap();
        let manifest = store.folder().join("session.json");
        let original = std::fs::read(&manifest).unwrap();
        std::fs::create_dir(store.folder().join("session.json.bak")).unwrap();
        assert!(store.notice("Synthetic approximate timing notice").is_err());
        let preview = store.preview_snapshot(16).unwrap();
        assert!(!preview.capture_incomplete);
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning == "Synthetic approximate timing notice"));
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("diagnostic could not be saved")));
        assert_eq!(std::fs::read(manifest).unwrap(), original);
    }

    #[test]
    /// Checks synthetic thread liveness reports one failed track while the other remains active.
    fn a_track_that_ends_early_is_reported_while_the_other_keeps_recording() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic", "m", "s", None).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let keep = Arc::clone(&stop);
        let system = std::thread::spawn(move || {
            while !keep.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        let microphone = std::thread::spawn(|| {});
        let mut controller = CaptureController {
            meeting_id: store.preview_snapshot(0).unwrap().id,
            stop,
            tracks: vec![
                ("system", system, Arc::new(AtomicBool::new(false))),
                ("microphone", microphone, Arc::new(AtomicBool::new(false))),
            ],
            controls: Arc::new(CaptureControls::new(&store).unwrap()),
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            microphone_id: None,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            recorder: platform::record,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            reconnect_attempts: 0,
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            ready_timeout: Duration::from_secs(10),
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            start_cancel: Arc::new(AtomicBool::new(false)),
            store,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while controller.stopped_tracks().is_empty() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(controller.is_running(), "system audio still records");
        assert_eq!(controller.stopped_tracks(), vec!["microphone"]);
        let levels = controller.audio_levels().unwrap();
        assert_eq!(levels.system.status, crate::AudioLevelStatus::Waiting);
        assert_eq!(
            levels.microphone.status,
            crate::AudioLevelStatus::Unavailable
        );
        controller.stop().unwrap();
        assert!(controller.stopped_tracks().is_empty());
        assert!(controller.audio_levels().is_none());
    }
}
