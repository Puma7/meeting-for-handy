//! Bounded local audition of an already confirmed recording section.
use crate::{meeting_files, AppResult, Service};
use std::sync::Arc;

const PREVIEW_SECONDS: usize = 30;

#[tauri::command]
pub(crate) async fn get_source_audio(
    service: tauri::State<'_, Arc<Service>>,
    id: String,
    file: String,
) -> AppResult<tauri::ipc::Response> {
    let service = Arc::clone(service.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Sitzung wird bearbeitet. / Session storage is busy.")?;
        let folder = meeting_files::session_folder(&service.root, &id)?;
        let snapshot = service.data(&id)?;
        let part = snapshot
            .parts
            .iter()
            .find(|part| part.file == file)
            .ok_or("Audiostelle nicht gefunden. / Audio section not found.")?;
        let pcm = meeting_core::read_closed_audio(&folder, part).map_err(|_| {
            "Bestätigter Ton ist lokal nicht verfügbar. / Confirmed audio is unavailable locally."
        })?;
        Ok(tauri::ipc::Response::new(encode_preview(&pcm)))
    })
    .await
    .map_err(|_| "Audiovorschau fehlgeschlagen. / Audio preview failed.")?
}

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
}
