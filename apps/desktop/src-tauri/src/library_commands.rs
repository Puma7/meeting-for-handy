use crate::{blocking, library_service, AppResult, Service};
use std::sync::Arc;
use tauri::State;

/// Save title/VIP changes off the UI thread through revision-checked backend metadata policy.
#[tauri::command]
pub(crate) async fn edit_meeting_metadata(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    expected_revision: u64,
    title: Option<String>,
    pinned: bool,
) -> AppResult<meeting_core::metadata::Metadata> {
    let service = Arc::clone(&service);
    blocking(move || edit_metadata(&service, &meeting_id, expected_revision, title, pinned)).await
}
/// Hold mutation admission while reading the session and saving separate versioned labels;
/// refresh derived index data without editing capture manifests.
pub(crate) fn edit_metadata(
    service: &Service,
    meeting_id: &str,
    expected_revision: u64,
    title: Option<String>,
    pinned: bool,
) -> AppResult<meeting_core::metadata::Metadata> {
    let _lease = service.reserve_operation(crate::operations::Kind::Mutation)?;
    let _source = meeting_core::archive::source_mutation_lease(&service.root)
        .map_err(|_| "Meetingablage wird gerade geändert. Bitte kurz warten. / Meeting storage is being changed; retry shortly.")?;
    let data = service.data(meeting_id)?;
    let mut meta=meeting_core::metadata::edit(&service.root,meeting_id,expected_revision,title,pinned)
            .map_err(|_|"Meeting labels could not be saved. Reload the meeting; originals are preserved. / Meetingname oder VIP-Markierung konnten nicht gespeichert werden. Bitte die Sitzung neu laden.")?;
    if data.read_only && !data.id.starts_with("legacy-") {
        meta.audio_removed = true;
    }
    if library_service::update(&service.root, &data).is_err() {
        if let Ok(mut state) = service.lock() {
            state.error=Some("Meeting labels were saved. Please refresh the local search index. / Meetingbezeichnungen gespeichert. Bitte den lokalen Suchindex aktualisieren.".into());
        }
    }
    Ok(meta)
}

#[tauri::command]
pub(crate) async fn search_meetings(
    service: State<'_, Arc<Service>>,
    query: String,
    offset: u64,
    limit: u32,
) -> AppResult<meeting_core::library::SearchPage> {
    let service = Arc::clone(&service);
    blocking(move || library_service::search(&service.root, &query, offset, limit)).await
}
// Runs off the UI thread: it may wait on the recording transition lock or on disk.
/// Reserve indexing admission before starting an explicitly requested local index rebuild.
#[tauri::command(async)]
pub(crate) fn rebuild_search_index(service: State<'_, Arc<Service>>) -> AppResult<String> {
    let lease = service.reserve_operation(crate::operations::Kind::Index)?;
    service
        .library_jobs
        .start_scan(service.root.clone(), true, Some(lease))
}
#[tauri::command]
pub(crate) fn get_library_progress(
    service: State<'_, Arc<Service>>,
) -> Option<library_service::Progress> {
    service.library_jobs.snapshot()
}
#[tauri::command]
pub(crate) fn cancel_library_job(service: State<'_, Arc<Service>>, job_id: String) -> bool {
    service.library_jobs.cancel(&job_id)
}
