use crate::{blocking, AppResult, Service};
use std::sync::Arc;
use tauri::State;
/// Explicitly archives generated capability proofs without deleting or replacing any file.
#[tauri::command]
pub(crate) async fn archive_speaker_model_checks(
    service: State<'_, Arc<Service>>,
) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || crate::speaker_check_archive::archive(&service)).await
}
#[tauri::command]
/// Checks model capability off the UI thread under native operation admission.
/// The check receives a separate evidence directory, without any meeting audio.
pub(crate) async fn check_speaker_model(
    service: State<'_, Arc<Service>>,
    model_id: String,
) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || crate::speaker_jobs::check_model(&service, &model_id)).await
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
#[tauri::command(async)]
pub(crate) fn start_speaker_experiment(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    model_id: String,
    mode: String,
) -> AppResult<String> {
    service
        .speaker_jobs
        .start(Arc::clone(&service), meeting_id, model_id, mode)
}
#[tauri::command]
pub(crate) fn get_speaker_progress(
    service: State<'_, Arc<Service>>,
) -> Option<crate::speaker_jobs::Progress> {
    service.speaker_jobs.snapshot()
}
#[tauri::command]
pub(crate) fn cancel_speaker_experiment(service: State<'_, Arc<Service>>, job_id: String) -> bool {
    service.speaker_jobs.cancel(&job_id)
}
#[tauri::command]
pub(crate) async fn get_speaker_sections(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<Option<crate::speaker_jobs::SavedSections>> {
    let service = Arc::clone(&service);
    blocking(move || crate::speaker_jobs::saved(&service, &meeting_id)).await
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpeakerView {
    #[serde(flatten)]
    document: meeting_core::speakers::SpeakerDocument,
    sources: std::collections::BTreeMap<String, String>,
}
#[tauri::command]
pub(crate) async fn get_speaker_labels(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<SpeakerView> {
    let service = Arc::clone(&service);
    blocking(move||{let data=service.data(&meeting_id)?;let document=meeting_core::speakers::read(&service.root,&meeting_id).map_err(|_|"Speaker annotations could not be read; prior revisions are preserved. / Sprecherzuordnungen konnten nicht gelesen werden; bisherige Fassungen bleiben erhalten.")?;Ok(SpeakerView{document,sources:data.parts.iter().filter_map(|p|p.text.as_ref().map(|t|(p.file.clone(),meeting_core::speakers::text_hash(t)))).collect()})}).await
}
/// Save revisioned speaker annotations off the UI thread through source-text validation.
#[tauri::command]
pub(crate) async fn save_speaker_labels(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    expected_revision: u64,
    enabled: bool,
    assignments: Vec<meeting_core::speakers::Assignment>,
) -> AppResult<meeting_core::speakers::SpeakerDocument> {
    let service = Arc::clone(&service);
    blocking(move || {
        save_labels(
            &service,
            &meeting_id,
            expected_revision,
            enabled,
            assignments,
        )
    })
    .await
}
/// Hold mutation admission while validating assignments against current source text and saving
/// separate annotations without rewriting capture transcripts.
pub(crate) fn save_labels(
    service: &Service,
    meeting_id: &str,
    expected_revision: u64,
    enabled: bool,
    assignments: Vec<meeting_core::speakers::Assignment>,
) -> AppResult<meeting_core::speakers::SpeakerDocument> {
    let _lease = service.reserve_operation(crate::operations::Kind::Mutation)?;
    let _source = meeting_core::archive::source_mutation_lease(&service.root)
        .map_err(|_| "Meetingablage wird gerade geändert. Bitte kurz warten. / Meeting storage is being changed; retry shortly.")?;
    let data = service.data(meeting_id)?;
    meeting_core::speakers::save(&service.root,&data,expected_revision,enabled,assignments).map_err(|_|"Speaker labels could not be saved. Reload and check the original section and quote. / Sprecherzuordnung nicht gespeichert. Bitte neu laden und Originalabschnitt sowie Zitat prüfen.".into())
}
