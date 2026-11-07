use super::*;

/// Create only synthetic immutable generations, including unlocked model results and their events.
fn saved_generation(
    root: &Path,
    id: &str,
    meeting: &str,
    version: &str,
    folder_name: &str,
    text: &str,
) -> std::path::PathBuf {
    let run_folder = root.join(id);
    std::fs::create_dir_all(&run_folder).unwrap();
    let metadata = run_folder.join("run.json");
    if !metadata.exists() {
        let run = crate::live::LiveRun {
            id: id.into(),
            meeting_id: meeting.into(),
            created_at: "2026-11-01T10:00:00Z".into(),
            model: "synthetic-history".into(),
            mode: "local".into(),
            endpoint: "http://127.0.0.1:1/v1/".into(),
            prompt_version: version.into(),
            prompt_profile_hash: None,
            language: "de".into(),
        };
        std::fs::write(metadata, serde_json::to_vec(&run).unwrap()).unwrap();
    }
    let (mut state, req, mut answer) = setup();
    answer["changes"][0]["text"] = json!(text);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    state.revision = 1;
    state.updated_at = Some("2026-11-01T10:10:00Z".into());
    let folder = run_folder.join(folder_name);
    std::fs::create_dir(&folder).unwrap();
    let delta = store::Delta::between(
        &WorkspaceState::default(),
        &state,
        req.transcript_sources.clone(),
        vec![],
    );
    let mut value = serde_json::to_value(delta).unwrap();
    value["schema"] = json!(version);
    std::fs::write(
        folder.join("revision-00000001.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    read_state(&folder).unwrap();
    folder
}

/// Snapshot fixture bytes so listing/loading must neither create locks nor rewrite any generation.
fn history_bytes(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(history_bytes(&path));
        } else {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}

#[test]
/// Explicit history access retains old unlocked results and events after the latest view changes.
fn earlier_unlocked_generations_remain_accessible_without_merging_into_the_latest() {
    let dir = tempfile::tempdir().unwrap();
    let old_id = "11111111111111111111111111111111";
    let new_id = "22222222222222222222222222222222";
    let old = saved_generation(
        dir.path(),
        old_id,
        "meeting",
        PREVIOUS_PROMPT_VERSION,
        PREVIOUS_WORKSPACE_FOLDER,
        "Früherer offener Bericht",
    );
    let current = saved_generation(
        dir.path(),
        new_id,
        "meeting",
        PROMPT_VERSION,
        WORKSPACE_FOLDER,
        "Aktueller Bericht",
    );
    let legacy_id = "33333333333333333333333333333333";
    saved_generation(
        dir.path(),
        legacy_id,
        "meeting",
        LEGACY_PROMPT_VERSION,
        LEGACY_WORKSPACE_FOLDER,
        "Ältester offener Bericht",
    );
    let v11_id = "44444444444444444444444444444444";
    let v11 = saved_generation(
        dir.path(),
        v11_id,
        "meeting",
        V11_PROMPT_VERSION,
        "workspace-v11",
        "Entwurf aus der vorigen Fassung",
    );
    let before = history_bytes(dir.path());
    let listing = crate::live::list_workspace_history(dir.path(), "meeting").unwrap();
    assert_eq!(listing.entries.len(), 4);
    assert_eq!(listing.unreadable_entries, 0);
    let saved =
        crate::live::read_workspace_history(dir.path(), "meeting", old_id, PREVIOUS_PROMPT_VERSION)
            .unwrap();
    assert_eq!(saved.workspace.entities[0].text, "Früherer offener Bericht");
    assert!(!saved.workspace.entities[0].human_locked);
    assert_eq!(saved.workspace.events[0].text, "Früherer offener Bericht");
    assert!(!saved.workspace.originals.is_empty());
    assert_eq!(saved.workspace.prompt_version(), PREVIOUS_PROMPT_VERSION);
    assert_eq!(
        crate::live::read_workspace_history(
            dir.path(),
            "meeting",
            legacy_id,
            LEGACY_PROMPT_VERSION
        )
        .unwrap()
        .workspace
        .entities[0]
            .text,
        "Ältester offener Bericht"
    );
    assert_eq!(
        crate::live::read_workspace_history(dir.path(), "meeting", v11_id, V11_PROMPT_VERSION)
            .unwrap()
            .workspace
            .entities[0]
            .text,
        "Entwurf aus der vorigen Fassung",
    );
    assert_eq!(history_bytes(dir.path()), before);
    assert!(WorkspaceRunner::open(&v11).is_err());
    assert!(WorkspaceRunner::open(&old).is_err());
    assert_eq!(
        read_state(&current).unwrap().entities[0].text,
        "Aktueller Bericht"
    );
    // The writer attempt may create its own coordination file; the history reads above may not.
    let after = history_bytes(dir.path());
    for (path, bytes) in before {
        assert_eq!(after.get(&path), Some(&bytes));
    }
}

#[test]
/// Meeting, run and generation are separate authority checks; strings never become arbitrary paths.
fn historical_selection_rejects_foreign_runs_traversal_and_unknown_generations() {
    let dir = tempfile::tempdir().unwrap();
    let id = "11111111111111111111111111111111";
    saved_generation(
        dir.path(),
        id,
        "meeting",
        PREVIOUS_PROMPT_VERSION,
        PREVIOUS_WORKSPACE_FOLDER,
        "Früherer Bericht",
    );
    let before = history_bytes(dir.path());
    for (meeting, run, version) in [
        ("other", id, PREVIOUS_PROMPT_VERSION),
        ("meeting", "../outside", PREVIOUS_PROMPT_VERSION),
        ("meeting", id, "../workspace-v10"),
        ("meeting", id, PROMPT_VERSION),
    ] {
        assert!(crate::live::read_workspace_history(dir.path(), meeting, run, version).is_err());
    }
    assert_eq!(history_bytes(dir.path()), before);
}

#[test]
/// An explicitly chosen readable old generation stays available despite a damaged newer one.
fn damaged_newer_history_is_reported_without_hiding_or_repairing_the_older_generation() {
    let dir = tempfile::tempdir().unwrap();
    let id = "11111111111111111111111111111111";
    saved_generation(
        dir.path(),
        id,
        "meeting",
        PREVIOUS_PROMPT_VERSION,
        PREVIOUS_WORKSPACE_FOLDER,
        "Früherer Bericht",
    );
    let damaged = dir.path().join(id).join(WORKSPACE_FOLDER);
    std::fs::create_dir(&damaged).unwrap();
    std::fs::write(
        damaged.join("revision-00000001.json"),
        b"broken synthetic revision",
    )
    .unwrap();
    let before = history_bytes(dir.path());
    let listing = crate::live::list_workspace_history(dir.path(), "meeting").unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.unreadable_entries, 1);
    assert!(
        crate::live::read_workspace_history(dir.path(), "meeting", id, PROMPT_VERSION).is_err()
    );
    assert_eq!(
        crate::live::read_workspace_history(dir.path(), "meeting", id, PREVIOUS_PROMPT_VERSION)
            .unwrap()
            .workspace
            .entities[0]
            .text,
        "Früherer Bericht"
    );
    assert_eq!(history_bytes(dir.path()), before);
}

#[test]
fn context_only_workspace_is_still_readable_under_its_original_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let id = "11111111111111111111111111111111";
    let folder = saved_generation(
        dir.path(),
        id,
        "meeting",
        V11_PROMPT_VERSION,
        "workspace-v11",
        "Earlier draft",
    );
    std::fs::remove_file(folder.join("revision-00000001.json")).unwrap();
    let context = WorkspaceContext {
        shared_notes: vec![SharedNote {
            id: "h_note".into(),
            revision: 1,
            text: "A shared note from the older version".into(),
        }],
        ..Default::default()
    };
    let context = context_store::update(&folder, &context).unwrap().0;
    let before = history_bytes(dir.path());
    let saved =
        crate::live::read_workspace_history(dir.path(), "meeting", id, V11_PROMPT_VERSION).unwrap();
    assert_eq!(saved.workspace.revision, 0);
    assert_eq!(saved.workspace.context, context);
    assert_eq!(saved.workspace.prompt_version(), V11_PROMPT_VERSION);
    assert!(update_context(&folder, &context).is_err());
    assert_eq!(history_bytes(dir.path()), before);
}
