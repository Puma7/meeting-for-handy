//! Generate a tiny isolated synthetic meeting for native UI checks. No devices or ASR.
use meeting_core::{PcmWriter, SessionStore, TranscriptCheckpoint};
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("New data directory required")?,
    );
    if root.exists() {
        return Err("Preserve existing evidence".into());
    }
    std::fs::create_dir_all(&root)?;
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetisches KI-Prüfmeeting",
        "synthetic-no-asr",
        "generated-system",
        Some("generated-microphone"),
    )?;
    for source in ["system", "microphone"] {
        let mut writer = PcmWriter::new(store.clone(), source)?;
        let pcm = (0..16000)
            .map(|i| ((i as f64 * std::f64::consts::TAU * 440.0 / 16000.0).sin() * 4096.0) as i16)
            .collect::<Vec<_>>();
        writer.append(&pcm)?;
        writer.finish()?;
    }
    let data = store.snapshot()?;
    for part in data.parts {
        let text = if part.source == "system" {
            "Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget fehlt noch eine Zusage."
        } else {
            "Vielleicht könnten wir die Webseite im Dezember erneuern. Das ist keine Zusage. Die Freigabe bleibt offen."
        };
        store.append_transcript(TranscriptCheckpoint {
            sequence: 0,
            at: String::new(),
            file: part.file,
            model_id: "synthetic-no-asr".into(),
            full: text.into(),
            committed: text.into(),
            tentative: String::new(),
            processed_samples: part.confirmed_sample_count,
            final_text: true,
            revision_after_restart: false,
        })?;
    }
    store.set_status("complete")?;
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"id":store.snapshot()?.id,"synthetic":true,"physicalDeviceOrAsr":false})
        )?
    );
    Ok(())
}
