//! Commands for live notes during a meeting and for the local text model setup.
use crate::live_notes::{LiveNotesRequest, LiveNotesStatus};
use crate::local_models::{LocalModelSetup, PullProgress};
use crate::{blocking, AppResult, Service};
use std::sync::Arc;
use tauri::State;

/// Resolve only a validated native session; callers keep its publication lease throughout the read.
fn history_root(service: &Service, meeting_id: &str) -> AppResult<std::path::PathBuf> {
    crate::meeting_files::session_folder(&service.root, meeting_id)
        .map(|path| path.join("live-notes"))
}

/// Enumerate saved workspace generations under the selected native session without starting a model.
#[tauri::command]
pub(crate) async fn list_workspace_history(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
) -> AppResult<meeting_notes::live::WorkspaceHistoryListing> {
    let service = Arc::clone(&service);
    blocking(move || {
        crate::validate_id(&meeting_id)?;
        let _publication = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|e| e.to_string())?;
        let root = history_root(&service, &meeting_id)?;
        meeting_notes::live::list_workspace_history(&root, &meeting_id).map_err(|e| e.to_string())
    })
    .await
}

/// Load one explicitly selected read-only generation while its publication lease is held.
#[tauri::command]
pub(crate) async fn read_workspace_history(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    run_id: String,
    prompt_version: String,
) -> AppResult<meeting_notes::live::SavedWorkspace> {
    let service = Arc::clone(&service);
    blocking(move || {
        crate::validate_id(&meeting_id)?;
        let _publication = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|e| e.to_string())?;
        let root = history_root(&service, &meeting_id)?;
        meeting_notes::live::read_workspace_history(&root, &meeting_id, &run_id, &prompt_version)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_workspace_results(
    service: State<'_, Arc<Service>>,
    meeting_id: String,
    run_id: String,
) -> AppResult<String> {
    let service = Arc::clone(&service);
    blocking(move || {
        let _operation = service.reserve_operation(crate::operations::Kind::Export)?;
        let _publication = meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|e| e.to_string())?;
        let folder = crate::meeting_files::session_folder(&service.root, &meeting_id)?;
        let data = service.data(&meeting_id)?;
        if data.read_only || data.id != meeting_id {
            return Err("Diese Sitzung ist schreibgeschützt. / This session is read-only.".into());
        }
        let view = service
            .live_notes
            .status_with_preference(&service, &meeting_id, true)?
            .view
            .filter(|view| view.run_id.as_deref() == Some(run_id.as_str()))
            .and_then(|view| view.workspace)
            .ok_or("Der Auswertungsstand hat sich geändert. / The analysis changed.")?;
        let results = workspace_export(&meeting_id, &run_id, &view);
        let bytes = serde_json::to_vec_pretty(&results).map_err(|_| {
            "Auswertung konnte nicht exportiert werden. / Could not export the analysis."
        })?;
        crate::meeting_files::save_export_bytes(&folder, "results", "json", &bytes)
    })
    .await
}

fn workspace_export(
    meeting_id: &str,
    run_id: &str,
    view: &meeting_notes::workspace::WorkspaceState,
) -> serde_json::Value {
    let current: std::collections::HashMap<_, _> = view
        .originals
        .iter()
        .filter(|source| !view.superseded_sources.contains(&source.id))
        .map(|source| (&source.id, &source.text))
        .collect();
    let confirmed = view.current_confirmations();
    let entries: Vec<_> = view
        .entities
        .iter()
        .filter(|entry| !view.context_excluded_ids.contains(&entry.id))
        .filter(|entry| {
            !entry.evidence.is_empty()
                && entry.evidence.iter().all(|evidence| {
                    !evidence.quote.trim().is_empty()
                        && current
                            .get(&evidence.source_id)
                            .is_some_and(|text| text.contains(&evidence.quote))
                })
        })
        .map(|entry| {
            let review = if confirmed.contains_key(entry.id.as_str()) {
                "confirmed"
            } else if entry.human_locked {
                "status-edited"
            } else {
                "ai-draft"
            };
            serde_json::json!({"review":review, "entry":entry})
        })
        .collect();
    serde_json::json!({"schemaVersion":1, "meetingId":meeting_id, "runId":run_id,
        "revision":view.revision, "entries":entries})
}

#[test]
fn export_preserves_confirmed_revision_and_omits_superseded_evidence() {
    use meeting_notes::workspace::{Entity, Event, WorkspaceState};
    let mut view = WorkspaceState::default();
    view.originals.push(meeting_notes::Source {
        id: "C1".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 1_000,
        text: "We selected the blue room.".into(),
    });
    view.entities.push(Entity {
        id: "decision1".into(),
        revision: 2,
        kind: "decision".into(),
        text: "Use the blue room.".into(),
        status: "recorded".into(),
        attention: "normal".into(),
        owner: None,
        due: None,
        human_locked: true,
        evidence: vec![meeting_notes::Evidence {
            source_id: "C1".into(),
            quote: "We selected the blue room.".into(),
        }],
    });
    view.events.push(Event {
        human: true,
        id: "review1".into(),
        entity_id: "decision1".into(),
        revision: 2,
        at_ms: 1_000,
        text: "Use the blue room.".into(),
        status: "confirmed".into(),
        evidence: view.entities[0].evidence.clone(),
    });
    let exported = workspace_export("synthetic", "run", &view);
    assert_eq!(exported["entries"][0]["review"], "confirmed");
    assert_eq!(exported["entries"][0]["entry"]["status"], "recorded");
    view.entities[0].revision = 3;
    assert_eq!(
        workspace_export("synthetic", "run", &view)["entries"][0]["review"],
        "status-edited"
    );
    view.superseded_sources.push("C1".into());
    assert!(workspace_export("synthetic", "run", &view)["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

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
    run_id: String,
    context: meeting_notes::workspace::WorkspaceContext,
) -> AppResult<bool> {
    let service = Arc::clone(&service);
    blocking(move || {
        service
            .live_notes
            .set_workspace_context(&service, &meeting_id, &run_id, context)
    })
    .await
}
