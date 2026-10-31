//! Bounded stdout PCM transport. It owns only the exact helper it starts.
use super::controls::{CaptureControls, PortableStartupTiming};
use crate::{PcmWriter, Result, SessionStore, SAMPLE_RATE};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const PACKET: usize = 3200;
const READY: &[u8] = b"HANDYMEETING_AUDIO_READY";
pub(super) struct HelperSpec {
    pub command: Command,
    pub readiness_marker: bool,
    pub graceful_stdin: bool,
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    /// Kills and reaps only the child process represented by this owned helper handle.
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
enum Packet {
    Data(Vec<u8>),
    End,
    Failed,
}

/// Small read-only device queries: bounded output and an owned ten-second deadline.
#[cfg(any(target_os = "linux", test))]
pub(super) fn text_command(command: &mut Command) -> Result<String> {
    query_command(command, "PulseAudio tools are missing. Install pulseaudio-utils and use a running PulseAudio or PipeWire-Pulse service.")
}

/// Same bounded query with the platform's own message for a missing tool.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(super) fn query_command(command: &mut Command, missing: &'static str) -> Result<String> {
    let mut child = OwnedChild(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| missing)?,
    );
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or("Device query output pipe is unavailable")?;
    let reading = thread::spawn(move || {
        let mut data = Vec::new();
        let read = stdout.take(1024 * 1024 + 1).read_to_end(&mut data);
        (read, data)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            return Err("Audio device query timed out; check the local audio service".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let (read, data) = reading.join().map_err(|_| "Device query reader failed")?;
    read?;
    if !status.success() {
        return Err("Audio device query failed; check the local audio service".into());
    }
    if data.len() > 1024 * 1024 {
        return Err("Device query exceeded its output limit".into());
    }
    Ok(String::from_utf8(data)?)
}

#[cfg(test)]
/// Runs the portable recorder with journal-free gates for synthetic helper fixtures.
pub(super) fn record(
    store: SessionStore,
    source: &str,
    stop: Arc<AtomicBool>,
    ready: SyncSender<std::result::Result<(), String>>,
    spec: HelperSpec,
) -> Result<()> {
    record_controlled(
        store,
        source,
        stop,
        ready,
        spec,
        Arc::new(CaptureControls::uncontrolled()),
    )
}

/// Persists bounded helper PCM by privacy epoch, restarting owned helpers after pause/mute transitions.
pub(super) fn record_controlled(
    store: SessionStore,
    source: &str,
    stop: Arc<AtomicBool>,
    ready: SyncSender<std::result::Result<(), String>>,
    mut spec: HelperSpec,
    controls: Arc<CaptureControls>,
) -> Result<()> {
    let mut announced = false;
    let result = (|| -> Result<()> {
        if !["system", "microphone"].contains(&source) {
            return Err("Invalid audio track".into());
        }
        let mut writer =
            PcmWriter::for_capture(store.clone(), source, &|| controls.note_startup_progress())?;
        let previous_end = writer.written_samples();
        let resumed_at = controls.elapsed_samples();
        // Close any gap silence already written when this startup step fails, so a later
        // reconnect can verify and continue the track.
        let startup = (|| -> Result<()> {
            if source == "microphone" && previous_end > 0 && resumed_at > previous_end {
                if resumed_at - previous_end > u64::from(SAMPLE_RATE) * 86400 {
                    return Err(
                        "Audio receipt clock gap exceeds one day; check the recording timeline"
                            .into(),
                    );
                }
                while writer.written_samples() < resumed_at {
                    if stop.load(Ordering::Acquire) {
                        return Err(
                            "Capture stopped while aligning the reconnected microphone".into()
                        );
                    }
                    let step =
                        resumed_at.min(writer.written_samples() + u64::from(SAMPLE_RATE) * 300);
                    pad_estimated(&mut writer, step)?;
                    controls.note_startup_progress();
                }
                controls.record_microphone_gap(previous_end, resumed_at - previous_end)?;
            }
            Ok(())
        })();
        if let Err(error) = startup {
            if let Err(finish) = writer.finish() {
                return Err(format!("{error}; gap silence could not be closed: {finish}").into());
            }
            return Err(error);
        }
        // Diagnostic failures remain visible in the live store and never gate PCM.
        let _ = store.notice("Portable audio track ranges use approximate receipt timing, not exact device or word timestamps.");
        let epochs_result = (|| -> Result<()> {
            while !stop.load(Ordering::Acquire) {
                while controls.suppressed(source) && !stop.load(Ordering::Acquire) {
                    // A reconnected track can be ready in a privacy-gated state. Do not
                    // open a helper or wait for speech before acknowledging this gate.
                    if !announced {
                        ready
                            .send(Ok(()))
                            .map_err(|_| "Capture controller closed")?;
                        announced = true;
                    }
                    pad_estimated(&mut writer, controls.elapsed_samples())?;
                    writer.checkpoint_if_due()?;
                    thread::sleep(Duration::from_millis(50));
                }
                if stop.load(Ordering::Acquire) {
                    break;
                }
                // Give intentionally ignored silence its own closed section; ASR can skip
                // all-zero parts without treating it as missing speech from a real part.
                writer.finish()?;
                let epoch = controls.epoch(source);
                let mut child=OwnedChild(spec.command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|_|"Audio helper could not start. Check installed utilities, the complete app bundle and recording permissions.")?);
                let stdout = child
                    .0
                    .stdout
                    .take()
                    .ok_or("Audio helper output pipe unavailable")?;
                let stderr = child
                    .0
                    .stderr
                    .take()
                    .ok_or("Audio helper diagnostic pipe unavailable")?;
                let (sender, receiver) = mpsc::sync_channel(2);
                let reading = thread::spawn(move || read_packets(stdout, sender));
                let marked = Arc::new(AtomicBool::new(false));
                let overflow = Arc::new(AtomicBool::new(false));
                let marked_reader = Arc::clone(&marked);
                let overflow_reader = Arc::clone(&overflow);
                let diagnostics = thread::spawn(move || {
                    let mut input = stderr;
                    let mut buffer = [0_u8; 4096];
                    let mut tail = Vec::with_capacity(8192);
                    let mut count = 0_usize;
                    while let Ok(read) = input.read(&mut buffer) {
                        if read == 0 {
                            break;
                        }
                        count = count.saturating_add(read);
                        if count > 65536 {
                            overflow_reader.store(true, Ordering::Release);
                        }
                        tail.extend_from_slice(&buffer[..read]);
                        if tail.windows(READY.len()).any(|value| value == READY) {
                            marked_reader.store(true, Ordering::Release);
                        }
                        if tail.len() > READY.len() {
                            tail.drain(..tail.len() - READY.len());
                        }
                    }
                });
                let start = Instant::now();
                // Every restart stays on the same monotonic meeting clock. A five-minute
                // startup clamp would otherwise compress a later resume of a long meeting.
                let offset = controls.elapsed_samples();
                let mut changed_epoch = false;
                let pump = (|| -> Result<()> {
                    let mut carry = None;
                    let mut pcm = Vec::with_capacity(PACKET / 2 + 1);
                    let mut received = false;
                    let mut epoch_ready = false;
                    let mut waiting_reported = false;
                    let mut late_reported = false;
                    let mut stopping = None;
                    loop {
                        if controls.changed(source, epoch) || controls.suppressed(source) {
                            // Never unmute a previous helper's buffered pipe: retire it and its
                            // bounded reader before any new epoch becomes eligible for speech.
                            changed_epoch = true;
                            break;
                        }
                        if stop.load(Ordering::Acquire) && stopping.is_none() {
                            stopping = Some(Instant::now());
                            if spec.graceful_stdin {
                                drop(child.0.stdin.take());
                            } else {
                                let _ = child.0.kill();
                            }
                        }
                        if stopping.is_some_and(|at| at.elapsed() > Duration::from_secs(2)) {
                            let _ = child.0.kill();
                        }
                        if overflow.load(Ordering::Acquire) {
                            return Err("Audio helper exceeded its diagnostic output limit".into());
                        }
                        match receiver.recv_timeout(Duration::from_millis(100)) {
                            Ok(Packet::Data(bytes)) => {
                                pcm.clear();
                                let mut index = 0;
                                if let Some(low) = carry.take() {
                                    if let Some(high) = bytes.first() {
                                        pcm.push(i16::from_le_bytes([low, *high]));
                                        index = 1;
                                    } else {
                                        carry = Some(low);
                                    }
                                }
                                while index + 1 < bytes.len() {
                                    pcm.push(i16::from_le_bytes([bytes[index], bytes[index + 1]]));
                                    index += 2;
                                }
                                if index < bytes.len() {
                                    carry = Some(bytes[index]);
                                }
                                if !pcm.is_empty() {
                                    if !received {
                                        let lead = offset
                                            + (start.elapsed().as_secs_f64()
                                                * f64::from(SAMPLE_RATE))
                                                as u64;
                                        let target = lead.saturating_sub(pcm.len() as u64);
                                        align_first_packet(
                                            &mut writer,
                                            target,
                                            source,
                                            &pcm,
                                            &controls,
                                            &store,
                                            epoch,
                                        )?;
                                        received = true;
                                    }
                                    controls.write(source, epoch, &mut writer, &pcm)?;
                                    epoch_ready = true;
                                    if !announced {
                                        ready
                                            .send(Ok(()))
                                            .map_err(|_| "Capture controller closed")?;
                                        announced = true;
                                    }
                                }
                            }
                            Ok(Packet::End) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                                if stopping.is_none() {
                                    return Err("Audio helper stopped unexpectedly; check the selected device and recording permissions".into());
                                }
                                break;
                            }
                            Ok(Packet::Failed) => {
                                return Err(
                                    "Audio helper PCM pipe failed; saved audio is retained".into()
                                )
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if !epoch_ready
                                    && ((spec.readiness_marker && marked.load(Ordering::Acquire))
                                        || (!spec.readiness_marker
                                            && start.elapsed() > Duration::from_millis(250)))
                                    && child.0.try_wait()?.is_none()
                                {
                                    if !announced {
                                        ready
                                            .send(Ok(()))
                                            .map_err(|_| "Capture controller closed")?;
                                        announced = true;
                                    }
                                    epoch_ready = true;
                                }
                                if !epoch_ready && start.elapsed() > Duration::from_secs(55) {
                                    return Err("Recording permission or the first audio packet did not arrive; check system permissions and start again".into());
                                }
                                if !received
                                    && start.elapsed() > Duration::from_secs(2)
                                    && !waiting_reported
                                {
                                    let _ = store.notice(&format!("{source}: waiting for the first actual audio packet. Check permissions or play a test sound; no received audio has been confirmed yet."));
                                    waiting_reported = true;
                                }
                                if received {
                                    let target = offset
                                        + ((start.elapsed().as_secs_f64() - 0.5).max(0.0)
                                            * f64::from(SAMPLE_RATE))
                                            as u64;
                                    if target > writer.written_samples() {
                                        if !late_reported {
                                            let _ = store.notice(&format!("{source}: gaps without packets are represented as estimated silence; late real packets are retained and timestamps can drift."));
                                            late_reported = true;
                                        }
                                        pad_estimated(&mut writer, target)?;
                                    }
                                }
                            }
                        }
                        writer.checkpoint_if_due()?;
                        if !received && child.0.try_wait()?.is_some() {
                            return Err("Audio helper exited before producing audio; check recording permissions and the selected device".into());
                        }
                    }
                    if carry.is_some() && !changed_epoch {
                        let _ = store.warn(&format!("{source}: an incomplete final PCM byte was retained only in the helper pipe and could not form an audio sample."));
                    }
                    Ok(())
                })();
                // Stop only our child; continue draining its bounded pipe before finalization.
                let _ = child.0.kill();
                let _ = child.0.wait();
                drop(receiver);
                let _ = reading.join();
                let _ = diagnostics.join();
                pump?;
                if !changed_epoch {
                    break;
                }
                writer.finish()?;
            }
            Ok(())
        })();
        if controls.suppressed(source) && epochs_result.is_ok() {
            pad_estimated(&mut writer, controls.elapsed_samples())?;
        }
        writer.finish()?;
        epochs_result
    })();
    if !announced {
        if let Err(error) = &result {
            let _ = ready.try_send(Err(error.to_string()));
        }
    }
    result
}

/// Forwards bounded stdout chunks and EOF/error markers through the bounded PCM channel.
fn read_packets(mut input: impl Read, sender: SyncSender<Packet>) {
    let mut buffer = [0_u8; PACKET];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(Packet::End);
                break;
            }
            Ok(count) => {
                if sender.send(Packet::Data(buffer[..count].to_vec())).is_err() {
                    break;
                }
            }
            Err(_) => {
                let _ = sender.send(Packet::Failed);
                break;
            }
        }
    }
}
/// Pads approximate receipt-time gaps in bounded sections and refuses gaps exceeding one day.
fn pad_estimated(writer: &mut PcmWriter, target: u64) -> Result<()> {
    if target.saturating_sub(writer.written_samples()) > u64::from(SAMPLE_RATE) * 86400 {
        return Err("Audio receipt clock gap exceeds one day; check the recording timeline".into());
    }
    while writer.written_samples() < target {
        writer.pad_to(target.min(writer.written_samples() + u64::from(SAMPLE_RATE) * 300))?;
    }
    Ok(())
}

/// Keeps existing alignment bytes unchanged while separating inserted padding
/// from zeros already present in the first helper packet. Only fixed source labels
/// and sample counts are exposed; the packet itself is never retained here.
fn align_first_packet(
    writer: &mut PcmWriter,
    target: u64,
    source: &str,
    pcm: &[i16],
    controls: &CaptureControls,
    store: &SessionStore,
    epoch: u64,
) -> Result<()> {
    let source = match source {
        "system" => "system",
        "microphone" => "microphone",
        _ => return Err("Invalid audio track".into()),
    };
    let before = writer.written_samples();
    let padding = target.saturating_sub(before);
    // Even a short estimated initial prefix is not confirmed received audio.
    // Intentional privacy-control resumes retain their existing notice policy.
    if epoch == 0 && padding > 0 {
        let _ = store.warn(&format!("{source}: the source started late; leading silence is estimated and can contain missing audio."));
    } else if padding > 8000 {
        let _ = store.notice(&format!("{source}: capture resumed with estimated startup silence. This silence is intentional control timing, not recovered speech."));
    }
    pad_estimated(writer, target)?;
    controls.note_portable_startup(PortableStartupTiming {
        source,
        written_before_padding: before,
        estimated_padding_samples: writer.written_samples().saturating_sub(before),
        first_packet_samples: pcm.len() as u64,
        first_packet_leading_zero_samples: pcm.iter().take_while(|sample| **sample == 0).count()
            as u64,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Separates a small native estimated pad from incoming zero samples while
    /// preserving the exact PCM, including an existing confirmed prefix.
    fn startup_observation_distinguishes_padding_from_helper_zeros_without_changing_pcm() {
        let root = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create(root.path(), "Synthetic alignment", "m", "s", None).unwrap();
        let controls = CaptureControls::uncontrolled();
        let mut writer = PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[19, -19]).unwrap();
        let packet = [0, 0, 0, 11, -12];
        align_first_packet(&mut writer, 457, "system", &packet, &controls, &store, 0).unwrap();
        writer.append(&packet).unwrap();
        writer.finish().unwrap();
        let observations = controls.portable_startup_timing().unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].written_before_padding, 2);
        assert_eq!(observations[0].estimated_padding_samples, 455);
        assert_eq!(observations[0].first_packet_samples, 5);
        assert_eq!(observations[0].first_packet_leading_zero_samples, 3);
        let data = store.snapshot().unwrap();
        let mut actual = vec![];
        for part in &data.parts {
            actual.extend(
                hound::WavReader::open(store.audio_path(part).unwrap())
                    .unwrap()
                    .samples::<i16>()
                    .map(std::result::Result::unwrap),
            );
        }
        let mut expected = vec![19, -19];
        expected.extend(vec![0; 455]);
        expected.extend(packet);
        assert_eq!(actual, expected);
        // A later restart must not replace the first observation with unrelated data.
        align_first_packet(&mut writer, 0, "system", &[33], &controls, &store, 0).unwrap();
        assert_eq!(controls.portable_startup_timing().unwrap(), observations);
        assert!(
            data.capture_incomplete,
            "Estimated startup silence must be declared even below half a second"
        );
        assert!(data
            .warnings
            .iter()
            .any(|warning| warning.starts_with("system: the source started late;")));
    }

    #[test]
    /// Incoming silence is not native padding and cannot obtain an estimated
    /// startup-gap declaration that would excuse a failing synthetic signal.
    fn incoming_zero_samples_without_native_padding_get_no_startup_gap_declaration() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic incoming silence", "m", "s", None)
            .unwrap();
        let controls = CaptureControls::uncontrolled();
        let mut writer = PcmWriter::new(store.clone(), "system").unwrap();
        let mut packet = vec![0; 523];
        packet.extend([23, -24]);
        align_first_packet(&mut writer, 0, "system", &packet, &controls, &store, 0).unwrap();
        writer.append(&packet).unwrap();
        writer.finish().unwrap();
        let data = store.snapshot().unwrap();
        assert!(!data.capture_incomplete);
        assert!(data.warnings.is_empty());
        let observed = controls.portable_startup_timing().unwrap();
        assert_eq!(observed[0].estimated_padding_samples, 0);
        assert_eq!(observed[0].first_packet_leading_zero_samples, 523);
        let actual: Vec<i16> = hound::WavReader::open(store.audio_path(&data.parts[0]).unwrap())
            .unwrap()
            .samples::<i16>()
            .map(std::result::Result::unwrap)
            .collect();
        assert_eq!(actual, packet);
    }

    #[test]
    /// Exercises the real portable pump with an owned synthetic helper; its
    /// observed padding plus helper zeros must match the durable track prefix.
    fn synthetic_helper_reports_the_origin_of_its_first_zero_samples() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(root.path(), "Synthetic startup", "m", "s", None).unwrap();
        let controls = Arc::new(CaptureControls::uncontrolled());
        let stop = Arc::new(AtomicBool::new(false));
        let _stop = StopOnExit(stop.clone());
        let mut command = python();
        command.args(["-c", "import sys,struct\nsys.stdout.buffer.write(bytes(18)+struct.pack('<hh',71,-72));sys.stdout.buffer.flush()\nsys.stdin.buffer.read()"]);
        let (ready, started) = mpsc::sync_channel(1);
        let thread_store = store.clone();
        let thread_controls = controls.clone();
        let thread_stop = stop.clone();
        let worker = thread::spawn(move || {
            record_controlled(
                thread_store,
                "system",
                thread_stop,
                ready,
                HelperSpec {
                    command,
                    readiness_marker: false,
                    graceful_stdin: false,
                },
                thread_controls,
            )
        });
        let ready = started.recv_timeout(Duration::from_secs(30));
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
        ready.unwrap().unwrap();
        let observations = controls.portable_startup_timing().unwrap();
        assert_eq!(observations.len(), 1);
        let observed = &observations[0];
        assert_eq!(observed.written_before_padding, 0);
        assert_eq!(
            observed.first_packet_leading_zero_samples,
            observed.first_packet_samples.min(9)
        );
        let data = store.snapshot().unwrap();
        let actual: Vec<i16> = data
            .parts
            .iter()
            .flat_map(|part| {
                hound::WavReader::open(store.audio_path(part).unwrap())
                    .unwrap()
                    .samples::<i16>()
                    .map(std::result::Result::unwrap)
                    .collect::<Vec<_>>()
            })
            .collect();
        let prefix = observed.estimated_padding_samples as usize + 9;
        assert!(actual[..prefix].iter().all(|sample| *sample == 0));
        assert_eq!(&actual[prefix..], &[71, -72]);
        let serialized = serde_json::to_value(observations).unwrap();
        let object = serialized[0].as_object().unwrap();
        assert_eq!(object.len(), 5);
        assert_eq!(object["source"], "system");
        assert!(object
            .iter()
            .all(|(key, value)| key == "source" || value.is_u64()));
    }

    #[test]
    fn a_paused_reconnect_acknowledges_without_starting_a_helper() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic paused reconnect",
            "m",
            "s",
            Some("mic"),
        )
        .unwrap();
        let mut original = PcmWriter::new(store.clone(), "microphone").unwrap();
        original.append(&[13, -14]).unwrap();
        original.finish().unwrap();
        let controls = Arc::new(CaptureControls::new(&store).unwrap());
        controls.set_paused(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (ready, acknowledged) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            record_controlled(
                store,
                "microphone",
                worker_stop,
                ready,
                HelperSpec {
                    command: Command::new("synthetic-helper-must-never-run"),
                    readiness_marker: true,
                    graceful_stdin: false,
                },
                controls,
            )
        });
        let ready = acknowledged.recv_timeout(Duration::from_secs(2));
        stop.store(true, Ordering::Release);
        assert!(worker.join().unwrap().is_ok());
        assert!(ready.unwrap().is_ok());
    }

    #[test]
    /// Checks synthetic helper restarts discard muted speech while keeping explicit silent timeline sections.
    fn synthetic_helper_mute_restarts_the_pipe_and_preserves_silent_timeline_sections() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic helper control",
            "m",
            "s",
            Some("mic"),
        )
        .unwrap();
        let controls = Arc::new(CaptureControls::new(&store).unwrap());
        controls.set_has_microphone(true);
        let value = root.path().join("synthetic-helper-value.txt");
        std::fs::write(&value, b"11").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let _stop = StopOnExit(stop.clone());
        let (ready, started) = mpsc::sync_channel(1);
        let thread_store = store.clone();
        let thread_controls = controls.clone();
        let thread_stop = stop.clone();
        let mut command = python();
        command.args(["-c", "import pathlib,sys,time,struct\nn=int(pathlib.Path(sys.argv[1]).read_text())\nsys.stderr.write('HANDYMEETING_AUDIO_READY\\n');sys.stderr.flush()\nwhile True:\n sys.stdout.buffer.write(struct.pack('<h',n)*320);sys.stdout.buffer.flush();time.sleep(0.02)"]).arg(value.clone());
        let worker = thread::spawn(move || {
            record_controlled(
                thread_store,
                "microphone",
                thread_stop,
                ready,
                HelperSpec {
                    command,
                    readiness_marker: true,
                    graceful_stdin: false,
                },
                thread_controls,
            )
        });
        started
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let has_sample = |expected: i16| {
            store
                .snapshot()
                .unwrap()
                .parts
                .iter()
                .filter(|p| p.source == "microphone" && p.confirmed_sample_count > 0)
                .any(|part| {
                    hound::WavReader::open(store.audio_path(part).unwrap())
                        .ok()
                        .is_some_and(|mut reader| {
                            reader
                                .samples::<i16>()
                                .take(part.confirmed_sample_count as usize)
                                .any(|s| s.is_ok_and(|s| s == expected))
                        })
                })
        };
        let initial_deadline = Instant::now() + Duration::from_secs(10);
        while !has_sample(11) {
            assert!(Instant::now() < initial_deadline);
            thread::sleep(Duration::from_millis(10));
        }
        controls.set_microphone_muted(true).unwrap();
        // The old helper read 11 once at process startup. A fresh helper would read 99;
        // while muted neither is allowed to become confirmed speech.
        std::fs::write(&value, b"99").unwrap();
        thread::sleep(Duration::from_millis(700));
        std::fs::write(&value, b"21").unwrap();
        controls.set_microphone_muted(false).unwrap();
        let resumed_deadline = Instant::now() + Duration::from_secs(10);
        while !has_sample(21) {
            assert!(Instant::now() < resumed_deadline);
            thread::sleep(Duration::from_millis(10));
        }
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
        let data = store.snapshot().unwrap();
        assert!(
            data.parts.iter().any(|p| p.source == "microphone"
                && !p.has_signal
                && p.confirmed_sample_count >= 3200),
            "Ignored interval must be an explicitly silent section, independently skippable by ASR"
        );
        let mut samples = vec![];
        for part in data.parts.iter().filter(|p| p.source == "microphone") {
            samples.extend(
                hound::WavReader::open(store.audio_path(part).unwrap())
                    .unwrap()
                    .samples::<i16>()
                    .map(std::result::Result::unwrap),
            );
        }
        assert!(!samples.contains(&99));
        let first_new = samples.iter().position(|s| *s == 21).unwrap();
        assert!(
            samples[first_new..].iter().all(|s| [0, 21].contains(s)),
            "No buffered old-epoch speech may arrive after the new helper epoch"
        );
        let log = super::super::controls::read_capture_control_events(store.folder())
            .unwrap()
            .unwrap();
        assert_eq!(
            log.events
                .iter()
                .filter(|e| e.kind == "microphone-mute")
                .count(),
            2
        );
        assert!(!log.incomplete_tail);
    }
    struct StopOnExit(Arc<AtomicBool>);
    impl Drop for StopOnExit {
        /// Signals the synthetic fixture's stop flag even if its test exits early.
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    /// Selects the host Python command used only by synthetic helper-process tests.
    fn python() -> Command {
        Command::new(if cfg!(windows) { "python" } else { "python3" })
    }
    // Test harness only, after the fixture's owned writer/process/reader handles
    // have closed. Parallel Unix child launches can briefly retain a CLOEXEC
    // flock descriptor until exec. Always acquire the real exclusive lease;
    // never unlock, delete its file, or ignore any other recovery error.
    /// Retries only transient fixture lease contention; real leases and all other recovery errors remain enforced.
    fn reopen_released_fixture(
        folder: &std::path::Path,
        timeout: Duration,
    ) -> Result<SessionStore> {
        let deadline = Instant::now() + timeout;
        loop {
            match SessionStore::open(folder) {
                Ok(store) => return Ok(store),
                Err(error)
                    if error.to_string() == "This session is already active in another process"
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
    }
    #[test]
    /// Checks synthetic metadata helpers reject excessive output and nonzero process exits.
    fn metadata_query_rejects_unbounded_output_and_preserves_nonzero_exit() {
        assert!(
            text_command(python().args(["-c", "import sys;sys.stdout.write('x'*1048577)"]))
                .is_err()
        );
        assert!(text_command(python().args(["-c", "raise SystemExit(9)"])).is_err());
    }
    #[test]
    /// Checks synthetic PCM pipes retain separate tracks and finalize their confirmed bytes on stop.
    fn synthetic_pipe_tracks_are_separate_and_stop_without_hardware() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic portable pipes",
            "synthetic",
            "fake system",
            Some("fake microphone"),
        )
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let _stop_on_exit = StopOnExit(Arc::clone(&stop));
        let gate = root.path().join("synthetic-pcm-start");
        let mut tracks = vec![];
        for (source, hex) in [("system", "0100"), ("microphone", "feff")] {
            let store = store.clone();
            let flag = Arc::clone(&stop);
            let gate = gate.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            tracks.push(thread::spawn(move || {
                let mut cmd = python();
                // An alive process is not proof that Python executed the fixture.
                // Mark readiness after interpreter startup, before the PCM barrier.
                cmd.args(["-c", &format!("import pathlib,sys,time\ngate=pathlib.Path(sys.argv[1])\nsys.stderr.write('HANDYMEETING_AUDIO_READY\\n')\nsys.stderr.flush()\nwhile not gate.exists(): time.sleep(0.01)\nsys.stdout.buffer.write(bytes.fromhex('{hex}')*16000)\nsys.stdout.buffer.flush()\ntime.sleep(30)")]).arg(gate);
                record(store, source, flag, tx, HelperSpec { command: cmd, readiness_marker: true, graceful_stdin: false })
            }));
            // Startup budget is separate from the unchanged ten-second PCM proof.
            rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        }
        assert!(store.snapshot().unwrap().parts.is_empty());
        std::fs::write(&gate, b"release synthetic PCM").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = store.snapshot().unwrap();
            let confirmed_fixtures = ["system", "microphone"].iter().all(|source| {
                snapshot
                    .parts
                    .iter()
                    .filter(|part| part.source == *source)
                    .any(|part| {
                        if part.confirmed_sample_count < 16000 {
                            return false;
                        }
                        let expected = if *source == "system" { 1_i16 } else { -2_i16 };
                        // Count only samples acknowledged after sync, excluding estimated
                        // silence. An alive process or a padded duration cannot pass.
                        hound::WavReader::open(store.audio_path(part).unwrap())
                            .unwrap()
                            .samples::<i16>()
                            .take(part.confirmed_sample_count as usize)
                            .filter(|sample| {
                                sample.as_ref().is_ok_and(|sample| *sample == expected)
                            })
                            .count()
                            == 16000
                    })
            });
            if confirmed_fixtures {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Both complete synthetic PCM fixtures must be durably confirmed before stop: {:?}",
                snapshot.parts
            );
            thread::sleep(Duration::from_millis(10));
        }
        stop.store(true, Ordering::Release);
        for track in tracks {
            track.join().unwrap().unwrap();
        }
        store.set_status("recorded").unwrap();
        let before = store.snapshot().unwrap();
        assert_eq!(before.parts.len(), 2);
        assert_ne!(
            before.parts[0].confirmed_sha256,
            before.parts[1].confirmed_sha256
        );
        for part in &before.parts {
            let expected = if part.source == "system" {
                1_i16
            } else {
                -2_i16
            };
            let actual_samples = hound::WavReader::open(store.audio_path(part).unwrap())
                .unwrap()
                .samples::<i16>()
                .map(|sample| sample.unwrap())
                .filter(|sample| *sample == expected)
                .count();
            assert_eq!(
                actual_samples, 16000,
                "The complete distinct fixture must survive stop"
            );
        }
        let before_hashes: Vec<_> = before
            .parts
            .iter()
            .map(|part| part.confirmed_sha256.clone())
            .collect();
        let folder = store.folder().to_owned();
        // The derived TXT writer may still be copying the final status for a moment.
        let deadline = Instant::now() + Duration::from_secs(2);
        while store.test_owner_count() != 1 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            store.test_owner_count(),
            1,
            "Joined capture tracks must release every store clone"
        );
        drop(store);
        let reopened = reopen_released_fixture(&folder, Duration::from_secs(2)).unwrap();
        let recovered = reopened.snapshot().unwrap();
        assert_eq!(recovered.parts.len(), 2);
        assert_eq!(
            recovered
                .parts
                .iter()
                .map(|part| part.confirmed_sha256.clone())
                .collect::<Vec<_>>(),
            before_hashes
        );
        assert!(recovered
            .parts
            .iter()
            .all(|p| p.audio_closed && p.confirmed_sample_count >= 16000));
    }
    #[test]
    /// Checks a live synthetic helper cannot become ready without emitting its actual readiness marker.
    fn alive_helper_cannot_acknowledge_a_marker_before_executing_it() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic readiness",
            "synthetic",
            "fake",
            None,
        )
        .unwrap();
        let started = root.path().join("fixture-started");
        let release = root.path().join("allow-ready-marker");
        let stop = Arc::new(AtomicBool::new(false));
        let _stop_on_exit = StopOnExit(Arc::clone(&stop));
        let (tx, rx) = mpsc::sync_channel(1);
        let flag = Arc::clone(&stop);
        let mut command = python();
        command.args(["-c", "import pathlib,sys,time\nstarted,release=map(pathlib.Path,sys.argv[1:])\nstarted.write_text('fixture executing')\nwhile not release.exists(): time.sleep(0.01)\nsys.stderr.write('HANDYMEETING_AUDIO_READY\\n')\nsys.stderr.flush()\ntime.sleep(30)"]).arg(&started).arg(&release);
        let worker = thread::spawn(move || {
            record(
                store,
                "system",
                flag,
                tx,
                HelperSpec {
                    command,
                    readiness_marker: true,
                    graceful_stdin: false,
                },
            )
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while !started.exists() {
            assert!(
                Instant::now() < deadline,
                "Fixture interpreter did not start"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(
                rx.recv_timeout(Duration::from_millis(400)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "An alive helper must not authorize PCM timing before its explicit ready marker"
        );
        std::fs::write(release, b"emit ready marker").unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
    }
    #[test]
    /// Checks a synthetic helper crash retains confirmed PCM that can be reopened through the normal lease.
    fn helper_crash_finalizes_received_pcm_and_survives_reopen() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic helper crash",
            "synthetic",
            "fake",
            None,
        )
        .unwrap();
        let (ready, _received) = mpsc::sync_channel(1);
        let mut command = python();
        command.args(["-c", "import sys;sys.stdout.buffer.write(bytes.fromhex('2301')*16000);sys.stdout.buffer.flush();raise SystemExit(23)"]);
        let error = record(
            store.clone(),
            "system",
            Arc::new(AtomicBool::new(false)),
            ready,
            HelperSpec {
                command,
                readiness_marker: false,
                graceful_stdin: false,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("stopped unexpectedly"));
        let before = store.snapshot().unwrap();
        assert_eq!(before.parts.len(), 1);
        assert!(before.parts[0].audio_closed);
        assert!(before.parts[0].confirmed_sample_count >= 16000);
        let hash = before.parts[0].confirmed_sha256.clone();
        let folder = store.folder().to_owned();
        // The derived TXT writer may still be copying the final status for a moment.
        let deadline = Instant::now() + Duration::from_secs(2);
        while store.test_owner_count() != 1 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            store.test_owner_count(),
            1,
            "A stopped helper and finalized writer must release every store clone"
        );
        drop(store);
        let reopened = reopen_released_fixture(&folder, Duration::from_secs(2)).unwrap();
        assert_eq!(reopened.snapshot().unwrap().parts[0].confirmed_sha256, hash);
    }
    #[test]
    /// Checks fixture recovery times out rather than bypassing an active session writer lease.
    fn bounded_fixture_reopen_never_bypasses_an_active_writer_lease() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic active lease",
            "synthetic",
            "fake",
            None,
        )
        .unwrap();
        let folder = store.folder().to_owned();
        let original = std::fs::read(folder.join("session.json")).unwrap();
        let result = reopen_released_fixture(&folder, Duration::from_millis(25));
        let error = result
            .err()
            .expect("An active writer must remain exclusive after the deadline");
        assert_eq!(
            error.to_string(),
            "This session is already active in another process"
        );
        assert_eq!(store.test_owner_count(), 1);
        assert_eq!(
            std::fs::read(folder.join("session.json")).unwrap(),
            original
        );
        drop(store);
        assert!(reopen_released_fixture(&folder, Duration::from_secs(2)).is_ok());
    }
    #[test]
    /// Checks fixture lease retries never hide a damaged original session manifest.
    fn bounded_fixture_reopen_propagates_original_manifest_errors() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::create(
            root.path(),
            "Synthetic damaged manifest",
            "synthetic",
            "fake",
            None,
        )
        .unwrap();
        let folder = store.folder().to_owned();
        drop(store);
        std::fs::write(folder.join("session.json"), b"not JSON").unwrap();
        std::fs::write(folder.join("session.json.bak"), b"not JSON either").unwrap();
        let error = reopen_released_fixture(&folder, Duration::from_secs(2))
            .err()
            .expect("Damaged metadata must not become a successful recovery");
        assert!(
            error.downcast_ref::<serde_json::Error>().is_some(),
            "Keep the original typed JSON failure: {error}"
        );
        assert_eq!(
            std::fs::read(folder.join("session.json")).unwrap(),
            b"not JSON"
        );
        assert_eq!(
            std::fs::read(folder.join("session.json.bak")).unwrap(),
            b"not JSON either"
        );
    }
}
