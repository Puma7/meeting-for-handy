//! Bounded local audition of an already confirmed recording section.
use crate::{meeting_files, AppResult, Service};
use std::sync::Arc;

const PREVIEW_SECONDS: usize = 30;
const UNAVAILABLE: &str =
    "Bestätigter Ton ist lokal nicht verfügbar. / Confirmed audio is unavailable locally.";

#[tauri::command]
pub(crate) async fn get_transcript_audio(
    service: tauri::State<'_, Arc<Service>>,
    id: String,
    source_id: String,
) -> AppResult<tauri::ipc::Response> {
    let service = Arc::clone(service.inner());
    crate::blocking(move || {
        let _lease = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Sitzung wird bearbeitet. / Session storage is busy.")?;
        let folder = meeting_files::session_folder(&service.root, &id)?;
        let data = service.data(&id)?;
        let mut candidates = data.parts.iter().filter(|part| part.file == source_id);
        let part = candidates
            .next()
            .filter(|part| {
                part.status == "complete"
                    && part.text.as_ref().is_some_and(|t| !t.trim().is_empty())
            })
            .ok_or(UNAVAILABLE)?;
        if data.id != id || candidates.next().is_some() {
            return Err(UNAVAILABLE.into());
        }
        read_preview(&folder, &data.parts, part, 0).map(tauri::ipc::Response::new)
    })
    .await
}

#[tauri::command]
/// Verify source ownership before reading a bounded original audio preview.
pub(crate) async fn get_source_audio(
    service: tauri::State<'_, Arc<Service>>,
    id: String,
    run_id: String,
    source_id: String,
) -> AppResult<tauri::ipc::Response> {
    let service = Arc::clone(service.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Sitzung wird bearbeitet. / Session storage is busy.")?;
        let folder = meeting_files::session_folder(&service.root, &id)?;
        let workspace = service
            .live_notes
            .status_with_preference(&service, &id, true)?
            .view
            .filter(|view| view.run_id.as_deref() == Some(run_id.as_str()))
            .and_then(|view| view.workspace)
            .ok_or(UNAVAILABLE)?;
        let source = workspace.audio_source(&source_id).ok_or(UNAVAILABLE)?;
        let snapshot = service.data(&id)?;
        let (part, offset_ms) = source_audio_part(&snapshot.parts, source).ok_or(UNAVAILABLE)?;
        read_preview(&folder, &snapshot.parts, part, offset_ms).map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "Audiovorschau fehlgeschlagen. / Audio preview failed.")?
}

fn read_preview(
    folder: &std::path::Path,
    parts: &[meeting_core::AudioPart],
    first: &meeting_core::AudioPart,
    offset_ms: u64,
) -> AppResult<Vec<u8>> {
    let mut pcm = meeting_core::read_closed_audio_preview_at(folder, first, offset_ms)
        .map_err(|_| UNAVAILABLE)?;
    let limit = meeting_core::SAMPLE_RATE as usize * PREVIEW_SECONDS;
    let mut previous = first;
    for _ in 0..7 {
        if pcm.len() >= limit {
            break;
        }
        let Some(next) = contiguous_part(parts, previous) else {
            break;
        };
        let Ok(audio) = meeting_core::read_closed_audio_preview(folder, next) else {
            break;
        };
        pcm.extend(audio.into_iter().take(limit - pcm.len()));
        previous = next;
    }
    Ok(encode_preview(&pcm))
}

fn contiguous_part<'a>(
    parts: &'a [meeting_core::AudioPart],
    previous: &meeting_core::AudioPart,
) -> Option<&'a meeting_core::AudioPart> {
    if previous.confirmed_sample_count != previous.sample_count {
        return None;
    }
    let end = previous.start_sample.checked_add(previous.sample_count)?;
    let mut matches = parts.iter().filter(|part| {
        part.source == previous.source
            && part.start_sample <= end
            && part.start_sample > previous.start_sample
            && part.start_sample.saturating_add(part.sample_count) > end
    });
    let next = matches.next()?;
    (next.start_sample == end
        && next.audio_closed
        && next.file != previous.file
        && matches.next().is_none()
        && parts.iter().filter(|part| part.file == next.file).count() == 1)
        .then_some(next)
}

/// Resolve the original quote against exactly one current, completed track section.
fn source_audio_part<'a>(
    parts: &'a [meeting_core::AudioPart],
    source: &meeting_notes::live::LiveSource,
) -> Option<(&'a meeting_core::AudioPart, u64)> {
    let samples_per_ms = u64::from(meeting_core::SAMPLE_RATE) / 1_000;
    if source.text.is_empty() || source.end_ms <= source.start_ms {
        return None;
    }
    let mut matching = parts.iter().filter(|part| {
        part.file == source.key
            && part.source == source.source
            && part.status == "complete"
            && part.start_sample / samples_per_ms <= source.start_ms
            && part.start_sample.saturating_add(part.sample_count) / samples_per_ms >= source.end_ms
            && part
                .text
                .as_ref()
                .is_some_and(|text| source.matches_transcript(text))
    });
    let part = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    Some((part, source.start_ms - part.start_sample / samples_per_ms))
}

/// Encode only the bounded PCM excerpt, retaining source sample rate and channel format.
fn encode_preview(pcm: &[f32]) -> Vec<u8> {
    let sample_rate = meeting_core::SAMPLE_RATE;
    let pcm = &pcm[..pcm.len().min(sample_rate as usize * PREVIEW_SECONDS)];
    let data_bytes = pcm.len() as u32 * 2;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend(b"RIFF");
    wav.extend((36 + data_bytes).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes()); // PCM
    wav.extend(1u16.to_le_bytes()); // mono
    wav.extend(sample_rate.to_le_bytes());
    wav.extend((sample_rate * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_bytes.to_le_bytes());
    for &sample in pcm {
        wav.extend(((sample * 32768.0) as i16).to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_current_exact_chunk_on_its_owned_track_can_resolve() {
        let source = meeting_notes::live::LiveSource {
            id: "C10".into(),
            key: "owned.wav".into(),
            chunk: 1,
            source: "system".into(),
            start_ms: 10_000,
            end_ms: 20_000,
            text: "Original words.".into(),
        };
        let part: meeting_core::AudioPart = serde_json::from_value(serde_json::json!({
            "file": "owned.wav", "source": "system", "startSample": 160000,
            "sampleCount": 160000, "status": "complete", "text": "Original words."
        }))
        .unwrap();
        assert_eq!(
            source_audio_part(std::slice::from_ref(&part), &source)
                .unwrap()
                .1,
            0
        );
        let mut other = part.clone();
        other.file = "different.wav".into();
        assert!(source_audio_part(&[other], &source).is_none());
        let mut revised = part.clone();
        revised.text = Some("Original words. More words.".into());
        assert!(source_audio_part(&[revised], &source).is_none());
        assert!(source_audio_part(&[part.clone(), part], &source).is_none());
    }

    #[test]
    fn preview_preserves_pcm_and_caps_returned_duration() {
        let count = meeting_core::SAMPLE_RATE as usize * PREVIEW_SECONDS;
        let mut pcm = vec![-1.0; count + 1];
        pcm[1] = 32767.0 / 32768.0;
        let wav = encode_preview(&pcm);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + count * 2);
        assert_eq!(&wav[44..48], &[0, 128, 255, 127]);
    }

    #[test]
    fn verified_preview_crosses_only_contiguous_sections_on_the_same_track() {
        let root =
            std::env::temp_dir().join(format!("hm-joined-preview-{}", meeting_notes::new_job_id()));
        std::fs::create_dir(&root).unwrap();
        struct Owned(std::path::PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = Owned(root);
        let store =
            meeting_core::SessionStore::create(&root.0, "Synthetic", "test", "system", Some("mic"))
                .unwrap();
        let rate = meeting_core::SAMPLE_RATE as usize;
        for (track, first, second) in [("system", 1000, -2000), ("microphone", 3000, 4000)] {
            let mut writer = meeting_core::PcmWriter::new(store.clone(), track).unwrap();
            for _ in 0..6 {
                writer.append(&vec![first; rate * 5]).unwrap();
            }
            writer.append(&vec![second; rate * 5]).unwrap();
            writer.finish().unwrap();
        }
        let data = store.snapshot().unwrap();
        let first = data
            .parts
            .iter()
            .find(|part| part.source == "system" && part.start_sample == 0)
            .unwrap();
        let second = contiguous_part(&data.parts, first).unwrap();
        assert_eq!(second.source, "system");
        let preview = read_preview(store.folder(), &data.parts, first, 29_000).unwrap();
        assert_eq!(preview.len(), 44 + rate * 6 * 2);
        assert_eq!(
            &preview[44 + 2 * rate - 2..44 + 2 * rate],
            &1000i16.to_le_bytes()
        );
        assert_eq!(
            &preview[44 + 2 * rate..44 + 2 * rate + 2],
            &(-2000i16).to_le_bytes()
        );
        let mut altered = data.parts.clone();
        altered
            .iter_mut()
            .find(|part| part.file == second.file)
            .unwrap()
            .confirmed_sha256 = first.confirmed_sha256.clone();
        let preview = read_preview(store.folder(), &altered, first, 29_000).unwrap();
        assert_eq!(preview.len(), 44 + rate * 2);
        let mut gapped = data.parts.clone();
        gapped
            .iter_mut()
            .find(|part| part.file == second.file)
            .unwrap()
            .start_sample += 1;
        assert!(contiguous_part(&gapped, first).is_none());
    }
}
