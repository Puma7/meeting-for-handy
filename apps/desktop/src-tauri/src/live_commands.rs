//! Commands for live notes during a meeting and for the local text model setup.
use crate::live_notes::{LiveNotesRequest, LiveNotesStatus};
use crate::local_models::{LocalModelSetup, PullProgress};
use crate::{blocking, AppResult, Service};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub(crate) async fn start_live_notes(
    service: State<'_, Arc<Service>>,
    request: LiveNotesRequest,
) -> AppResult<LiveNotesStatus> {
    let service = Arc::clone(&service);
    blocking(move || service.live_notes.start(&service, request)).await
}

#[tauri::command]
pub(crate) async fn stop_live_notes(service: State<'_, Arc<Service>>) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || service.live_notes.stop()).await
}

#[tauri::command]
pub(crate) async fn refresh_live_notes(service: State<'_, Arc<Service>>) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || service.live_notes.refresh()).await
}

#[tauri::command]
pub(crate) async fn set_live_notes_interval(
    service: State<'_, Arc<Service>>,
    interval_ms: u64,
) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || service.live_notes.set_interval(interval_ms)).await
}

#[tauri::command]
pub(crate) async fn get_live_notes(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    prefer_workspace: Option<bool>,
) -> AppResult<LiveNotesStatus> {
    let service = Arc::clone(&service);
    blocking(move || {
        service.live_notes.status_with_preference(
            &service,
            &meeting_id,
            prefer_workspace.unwrap_or(false),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn local_model_setup() -> AppResult<LocalModelSetup> {
    blocking(|| Ok(crate::local_models::setup())).await
}

#[tauri::command]
pub(crate) fn pull_ollama_model(
    service: State<'_, Arc<Service>>,
    model: String,
) -> AppResult<PullProgress> {
    service.model_pulls.start(model)
}

#[tauri::command]
pub(crate) fn get_model_pull_progress(
    service: State<'_, Arc<Service>>,
) -> AppResult<Option<PullProgress>> {
    Ok(service.model_pulls.snapshot())
}

#[tauri::command]
pub(crate) fn cancel_model_pull(service: State<'_, Arc<Service>>) -> AppResult<bool> {
    Ok(service.model_pulls.cancel())
}

#[tauri::command]
pub(crate) async fn set_workspace_context(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    context: meeting_notes::workspace::WorkspaceContext,
) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || {
        service
            .live_notes
            .set_workspace_context(&service, &meeting_id, context)
    })
    .await
}
