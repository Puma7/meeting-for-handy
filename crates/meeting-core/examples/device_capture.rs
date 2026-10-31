//! Explicit opt-in hardware diagnostic. Never called by ordinary verification.
//! Audio stays in the supplied new local session; no model or network is used.
use meeting_core::{CaptureController, CaptureOptions, SessionStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq)]
enum Action {
    Record { root: PathBuf, seconds: u64 },
    Recover { root: PathBuf },
    ListDevices,
}

fn parse(args: &[String]) -> meeting_core::Result<Action> {
    if let [flag] = args {
        if flag == "--list-devices" {
            return Ok(Action::ListDevices);
        }
    }
    if let [flag, root] = args {
        if flag == "--recover" {
            return Ok(Action::Recover { root: root.into() });
        }
    }
    if !(2..=3).contains(&args.len()) {
        return Err(
            "Supply NEW_OUTPUT SECONDS [--allow-long-recording], --recover OUTPUT or --list-devices"
                .into(),
        );
    }
    let seconds: u64 = args[1].parse()?;
    if !(5..=7200).contains(&seconds)
        || (args.len() == 3 && args[2] != "--allow-long-recording")
        || (seconds > 60 && args.len() != 3)
    {
        return Err("5..60 seconds by default; longer device recordings require --allow-long-recording (maximum7200 seconds)".into());
    }
    Ok(Action::Record {
        root: args[0].clone().into(),
        seconds,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pointer {
    session_id: String,
}

fn create_json(path: &Path, value: &impl Serialize) -> meeting_core::Result<()> {
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    output.write_all(&serde_json::to_vec_pretty(value)?)?;
    output.sync_all()?;
    Ok(())
}

fn checkpoint(root: &Path, store: &SessionStore, index: u32) -> meeting_core::Result<()> {
    create_json(
        &root
            .join("checkpoints")
            .join(format!("checkpoint-{index:06}.json")),
        &store.snapshot()?,
    )
}

fn pcm_prefix_hash(path: &Path, count: u64) -> meeting_core::Result<String> {
    if count > 16000 * 300 {
        return Err("Diagnostic prefix exceeds the production chunk limit".into());
    }
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16000
        || spec.bits_per_sample != 16
        || reader.len() < count as u32
    {
        return Err("Diagnostic PCM prefix is incomplete or has changed format".into());
    }
    let mut digest = Sha256::new();
    for sample in reader.samples::<i16>().take(count as usize) {
        digest.update(sample?.to_le_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn recover(root: &Path) -> meeting_core::Result<()> {
    let pointer: Pointer =
        serde_json::from_slice(&std::fs::read(root.join("session-pointer.json"))?)?;
    if pointer.session_id.len() != 32 || !pointer.session_id.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Invalid diagnostic session pointer".into());
    }
    let mut checkpoints: Vec<_> = std::fs::read_dir(root.join("checkpoints"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("checkpoint-") && name.ends_with(".json"))
        })
        .collect();
    checkpoints.sort();
    let latest = checkpoints
        .last()
        .ok_or("No confirmed diagnostic checkpoint exists")?;
    if latest.metadata()?.len() > 8 * 1024 * 1024 {
        return Err("Diagnostic checkpoint is too large".into());
    }
    let before: meeting_core::SessionData = serde_json::from_slice(&std::fs::read(latest)?)?;
    if before.id != pointer.session_id || before.model_id != "audio-only:no-asr" {
        return Err("This output does not identify an owned audio-only diagnostic".into());
    }
    let store = SessionStore::open(&root.join("sessions").join(&pointer.session_id))?;
    let after = store.snapshot()?;
    for part in &before.parts {
        let actual = after
            .parts
            .iter()
            .find(|p| p.file == part.file && p.source == part.source)
            .ok_or("A previously confirmed audio part is missing")?;
        if actual.confirmed_sample_count < part.confirmed_sample_count
            || pcm_prefix_hash(&store.audio_path(actual)?, part.confirmed_sample_count)?
                != part.confirmed_sha256
        {
            return Err("A previously confirmed audio prefix changed".into());
        }
    }
    let result = serde_json::json!({"version":env!("CARGO_PKG_VERSION"), "confirmedPrefixesPreserved":true,
        "checkpointParts":before.parts.len(),"recoveredParts":after.parts.len(),"captureIncomplete":after.capture_incomplete,
        "speechRecognition":false,"powerLossTest":false,
        "limits":"Only prefixes confirmed in the diagnostic checkpoint are compared. An unconfirmed tail may be missing. No transcript/ASR or power-loss validation."});
    create_json(
        &root.join(format!(
            "recovery-{}.json",
            chrono::Utc::now().timestamp_millis()
        )),
        &result,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn main() -> meeting_core::Result<()> {
    match parse(&std::env::args().skip(1).collect::<Vec<_>>())? {
        Action::Record { root, seconds } => record(&root, seconds),
        Action::Recover { root } => recover(&root),
        // Read-only: the platform adapter's actual device query; nothing is recorded.
        Action::ListDevices => {
            println!(
                "{}",
                serde_json::to_string_pretty(&meeting_core::list_devices()?)?
            );
            Ok(())
        }
    }
}

fn record(root: &Path, seconds: u64) -> meeting_core::Result<()> {
    if root.exists() {
        return Err("Use a new directory; existing diagnostic evidence is retained".into());
    }
    // Fixed before capture; not adjusted to the observed measurements.
    let allowed_track_difference_ms = 500_u64;
    std::fs::create_dir_all(root.join("checkpoints"))?;
    create_json(
        &root.join("plan.json"),
        &serde_json::json!({
            "version":env!("CARGO_PKG_VERSION"),"seconds":seconds,"allowedTrackDifferenceMs":allowed_track_difference_ms,
            "realHardware":true,"microphone":true,"speechRecognition":false,"cloud":false,
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"ownedProcessId":std::process::id(),
            "checkpointIntervalSeconds":10,"longRecordingExplicit":seconds>60
        }),
    )?;
    let store = SessionStore::create(
        &root.join("sessions"),
        "Explicit local hardware diagnostic",
        "audio-only:no-asr",
        "default system output",
        Some("default microphone"),
    )?;
    create_json(
        &root.join("session-pointer.json"),
        &Pointer {
            session_id: store.snapshot()?.id,
        },
    )?;
    println!("Owned diagnostic PID: {}. System and microphone audio remain in the supplied local output.", std::process::id());
    let started = Instant::now();
    let mut capture = CaptureController::start(
        store.clone(),
        CaptureOptions {
            output_id: None,
            microphone_id: None,
            include_microphone: true,
        },
    )?;
    let recording_started = Instant::now();
    let mut index = 0;
    while recording_started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_secs(1));
        if recording_started.elapsed().as_secs() / 10 > u64::from(index) {
            checkpoint(root, &store, index)?;
            index += 1;
        }
    }
    capture.stop()?;
    checkpoint(root, &store, index)?;
    let elapsed_ms = started.elapsed().as_millis();
    let data = store.snapshot()?;
    let samples = |source: &str| {
        data.parts
            .iter()
            .filter(|p| p.source == source)
            .map(|p| p.confirmed_sample_count)
            .sum::<u64>()
    };
    let system_ms = samples("system") * 1000 / 16000;
    let microphone_ms = samples("microphone") * 1000 / 16000;
    let portable_startup = capture.portable_startup_timing()?;
    let result = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"realHardware":true,"speechRecognition":false,"sessionId":data.id,"elapsedMs":elapsed_ms,"systemMs":system_ms,"microphoneMs":microphone_ms,"differenceMs":system_ms.abs_diff(microphone_ms),"trackDifferenceWithinLimit":system_ms.abs_diff(microphone_ms)<=allowed_track_difference_ms,"captureIncomplete":data.capture_incomplete,"warnings":data.warnings,"limits":"Audio-only live-device diagnostic of the actual requested duration; no ASR, full-disk or general reliability claim. Recovery is a separate explicit action."});
    let mut result = result;
    result["portableStartup"] = serde_json::to_value(portable_startup)?;
    create_json(&root.join("result.json"), &result)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn long_device_recording_requires_the_explicit_opt_in() {
        assert!(parse(&args(&["new-output", "7200"])).is_err());
        assert!(parse(&args(&["new-output", "7201", "--allow-long-recording"])).is_err());
        assert!(parse(&args(&["new-output", "7200", "--allow-long-recording"])).is_ok());
        assert!(parse(&args(&["new-output", "30", "unknown"])).is_err());
        assert_eq!(
            parse(&args(&["--list-devices"])).unwrap(),
            Action::ListDevices
        );
        assert!(parse(&args(&["--list-devices", "extra"])).is_err());
    }

    #[test]
    fn recovery_does_not_select_a_recording_action() {
        assert_eq!(
            parse(&args(&["--recover", "owned-output"])).unwrap(),
            Action::Recover {
                root: "owned-output".into()
            }
        );
    }

    #[test]
    fn prefix_checker_detects_a_changed_previously_confirmed_sample() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.wav");
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for value in [1_i16, 2, 3] {
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();
        let before = pcm_prefix_hash(&path, 2).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[44] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert_ne!(before, pcm_prefix_hash(&path, 2).unwrap());
        assert!(pcm_prefix_hash(&path, 4).is_err());
    }

    #[test]
    fn diagnostic_recovery_refuses_active_owner_then_checks_both_original_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("checkpoints")).unwrap();
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic diagnostic",
            "audio-only:no-asr",
            "generated",
            Some("generated"),
        )
        .unwrap();
        for (source, samples) in [("system", [1_i16, 2, 3]), ("microphone", [-4, -5, -6])] {
            let mut writer = meeting_core::PcmWriter::new(store.clone(), source).unwrap();
            writer.append(&samples).unwrap();
            writer.finish().unwrap();
        }
        create_json(
            &root.join("session-pointer.json"),
            &Pointer {
                session_id: store.snapshot().unwrap().id,
            },
        )
        .unwrap();
        checkpoint(root, &store, 0).unwrap();
        let original = std::fs::read(store.folder().join("session.json")).unwrap();
        assert!(recover(root).is_err());
        assert_eq!(
            original,
            std::fs::read(store.folder().join("session.json")).unwrap()
        );
        let expected: Vec<_> = store
            .snapshot()
            .unwrap()
            .parts
            .iter()
            .map(|part| {
                (
                    store.audio_path(part).unwrap(),
                    part.confirmed_sha256.clone(),
                )
            })
            .collect();
        drop(store);
        recover(root).unwrap();
        for (path, expected) in expected {
            assert_eq!(pcm_prefix_hash(&path, 3).unwrap(), expected);
        }
    }
}
