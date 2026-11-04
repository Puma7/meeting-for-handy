//! Real-time synthetic PCM through the actual durable writer, without opening devices.
use meeting_core::{PcmWriter, SessionStore, TranscriptCheckpoint, SAMPLE_RATE};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> meeting_core::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let root = Path::new(args.get(1).ok_or("New isolated output root required")?);
    if root.exists() {
        return Err("Preserve prior evidence; choose a new root".into());
    }
    let seconds = args
        .get(2)
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(300);
    if !(210..=600).contains(&seconds) {
        return Err("Real-time fixture must last 210..600 seconds".into());
    }
    let store = SessionStore::create(
        root,
        "Synthetic workspace load test",
        "synthetic-no-asr",
        "generated-system",
        Some("generated-microphone"),
    )?;
    let mut writers = [
        PcmWriter::new(store.clone(), "system")?,
        PcmWriter::new(store.clone(), "microphone")?,
    ];
    let start = Instant::now();
    let frames = seconds * 50;
    let samples_per_frame = SAMPLE_RATE / 50;
    let mut maximum_delay = 0_u64;
    for frame in 0..frames {
        let expected = Duration::from_millis(frame * 20);
        let now = start.elapsed();
        if expected > now {
            std::thread::sleep(expected - now);
        } else {
            maximum_delay = maximum_delay.max((now - expected).as_millis() as u64);
        }
        for (channel, writer) in writers.iter_mut().enumerate() {
            let frequency = if channel == 0 { 440.0 } else { 660.0 };
            let samples = (0..samples_per_frame)
                .map(|i| {
                    ((std::f64::consts::TAU
                        * frequency
                        * (frame as f64 * f64::from(samples_per_frame) + f64::from(i))
                        / f64::from(SAMPLE_RATE))
                    .sin()
                        * 4096.0) as i16
                })
                .collect::<Vec<_>>();
            writer.append(&samples)?;
        }
        if frame % 1500 == 0 {
            println!(
                "synthetic capture {}s; both native writers active",
                frame / 50
            );
        }
    }
    for mut writer in writers {
        writer.finish()?;
    }
    let data = store.snapshot()?;
    for part in &data.parts {
        store.append_transcript(TranscriptCheckpoint{sequence:0,at:String::new(),file:part.file.clone(),model_id:"synthetic-no-asr".into(),full:"Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget fehlt noch eine Zusage.".into(),committed:"Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget fehlt noch eine Zusage.".into(),tentative:String::new(),processed_samples:part.confirmed_sample_count,final_text:true,revision_after_restart:false})?;
    }
    let folder = store.folder().to_path_buf();
    drop(store);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("session.json"))?)?;
    if manifest["parts"]
        .as_array()
        .is_some_and(|parts| !parts.is_empty())
    {
        return Err(
            "Fixture requires a stale empty manifest so journal recovery is actually exercised"
                .into(),
        );
    }
    let restored = SessionStore::open(&folder)?;
    let reopened = restored.snapshot()?;
    if reopened.parts.iter().any(|part| part.status != "complete" || part.text.as_deref() != Some("Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget fehlt noch eine Zusage.")) {
        return Err("Transcript journal recovery lost confirmed text".into());
    }
    restored.set_status("recorded")?;
    let counts = ["system", "microphone"].map(|source| {
        reopened
            .parts
            .iter()
            .filter(|p| p.source == source)
            .map(|p| p.confirmed_sample_count)
            .sum::<u64>()
    });
    if counts != [seconds * u64::from(SAMPLE_RATE); 2] {
        return Err("Synthetic PCM sample count changed".into());
    }
    let result = serde_json::json!({"synthetic":true,"nativePcmWriter":true,"realtimeSeconds":start.elapsed().as_secs_f64(),"expectedSamplesPerTrack":seconds*u64::from(SAMPLE_RATE),"actualSamplesPerTrack":counts,"maximumProducerDelayMs":maximum_delay,"journalAndReopen":true,"physicalDeviceOrAsrTest":false});
    std::fs::write(
        root.join("capture-result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}
