//! Commands for live notes during a meeting and for the local text model setup.
use crate::live_notes::{CurrentWorkspaceEvidence, LiveNotesRequest, LiveNotesStatus};
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
        let view = service
            .live_notes
            .status_with_preference(&service, &meeting_id, true)?
            .view
            .filter(|view| view.run_id.as_deref() == Some(run_id.as_str()))
            .and_then(|view| view.workspace)
            .ok_or("Der Auswertungsstand hat sich geändert. / The analysis changed.")?;
        let data = service.data(&meeting_id)?;
        if data.read_only || data.id != meeting_id {
            return Err("Diese Sitzung ist schreibgeschützt. / This session is read-only.".into());
        }
        let results = workspace_export(&meeting_id, &run_id, &view, &data);
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
    data: &meeting_core::SessionData,
) -> serde_json::Value {
    let mut current = CurrentWorkspaceEvidence::new(view, data);
    let confirmed = view.current_confirmations();
    let entries: Vec<_> = view
        .entities
        .iter()
        .filter(|entry| !view.context_excluded_ids.contains(&entry.id))
        .filter(|entry| current.supports(&entry.evidence))
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
fn export_preserves_confirmed_revision_and_rejects_a_new_transcript_version() {
    use meeting_core::{PcmWriter, SessionStore, TranscriptCheckpoint, SAMPLE_RATE};
    use meeting_notes::workspace::{Entity, Event, WorkspaceState};
    use sha2::{Digest, Sha256};

    const ORIGINAL: &str = "We selected the blue room.";
    let profile = crate::test_profile::TestProfile::new().unwrap();
    let store = SessionStore::create(
        profile.path(),
        "Synthetic export meeting",
        "synthetic-model",
        "system",
        Some("mic"),
    )
    .unwrap();
    for track in ["system", "microphone"] {
        let mut writer = PcmWriter::new(store.clone(), track).unwrap();
        writer.append(&vec![1000; SAMPLE_RATE as usize]).unwrap();
        writer.finish().unwrap();
    }
    let recorded = store.snapshot().unwrap();
    let write_text = |part: &meeting_core::AudioPart, text: &str, revision_after_restart| {
        store
            .append_transcript(TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file.clone(),
                model_id: "synthetic-model".into(),
                full: text.into(),
                committed: text.into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart,
            })
            .unwrap();
    };
    for part in &recorded.parts {
        write_text(part, ORIGINAL, false);
    }
    let original = store.snapshot().unwrap();
    let source = meeting_notes::live::pending(&[], &crate::live_notes::live_texts(&original).0)
        .into_iter()
        .find(|source| source.source == "system")
        .unwrap();
    let hash = Sha256::digest(format!("{}\0{}", source.id, source.text));
    let source_id = format!("s_{}", &format!("{hash:x}")[..16]);
    let mut view = WorkspaceState::default();
    view.originals.push(meeting_notes::Source {
        id: source_id.clone(),
        ..source.as_source()
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
            source_id: source_id.clone(),
            quote: ORIGINAL.into(),
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
    // Fixture for a workspace generation that has already recorded and confirmed the old source.
    let mut stored = serde_json::to_value(&view).unwrap();
    stored["processed"] = serde_json::json!([source]);
    let mut view: WorkspaceState = serde_json::from_value(stored).unwrap();
    let export = |view: &WorkspaceState, data: &meeting_core::SessionData| {
        workspace_export(&original.id, "run", view, data)
    };
    let exported = export(&view, &original);
    assert_eq!(exported["entries"][0]["review"], "confirmed");
    assert_eq!(exported["entries"][0]["entry"]["status"], "recorded");
    let system = original
        .parts
        .iter()
        .find(|part| part.source == "system")
        .unwrap();
    write_text(system, "We selected the red room.", true);
    let revised = store.snapshot().unwrap();
    assert!(view.current_confirmations().contains_key("decision1"));
    assert!(revised
        .parts
        .iter()
        .any(|part| part.source == "microphone" && part.text.as_deref() == Some(ORIGINAL)));
    assert!(export(&view, &revised)["entries"]
        .as_array()
        .unwrap()
        .is_empty());

    view.entities[0].revision = 3;
    assert_eq!(
        export(&view, &original)["entries"][0]["review"],
        "status-edited"
    );
    view.superseded_sources.push(source_id);
    assert!(export(&view, &original)["entries"]
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
        if prefer_workspace.unwrap_or(false) {
            service.live_notes.workspace_status(&service, &meeting_id)
        } else {
            service
                .live_notes
                .status_with_preference(&service, &meeting_id, false)
        }
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
