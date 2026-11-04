use super::*;

#[test]
fn new_model_run_inherits_manual_state_and_preserves_its_evidence() {
    let (mut previous, req, answer) = setup();
    validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
    previous.originals = req.transcript_sources.clone();
    previous.entities[0].human_locked = true;
    previous.entities[0].revision = 2;
    previous.entities[0].status = "discussed".into();
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    runner.inherit_manual(&previous).unwrap();
    assert_eq!(runner.state.entities, previous.entities);
    assert_eq!(runner.state.originals, previous.originals);
    assert!(runner.state.processed.is_empty());
    drop(runner);
    let reopened = read_state(dir.path()).unwrap();
    assert_eq!(reopened.entities, previous.entities);
    assert_eq!(reopened.originals, previous.originals);
}

#[test]
fn inherited_evidence_is_invalidated_before_the_first_new_model_window() {
    for removed in [false, true] {
        let (mut previous, mut req, mut answer) = setup();
        let live = live::pending(
            &[],
            &[LiveText {
                key: "original".into(),
                track: "system".into(),
                start_ms: 0,
                end_ms: 3000,
                text: req.transcript_sources[0].text.clone(),
            }],
        )
        .remove(0);
        let id = source_id(&live);
        req.transcript_sources[0].id = id.clone();
        req = Request::new(
            "r_test".into(),
            &WorkspaceContext::default(),
            &previous,
            vec![],
            &req.transcript_sources,
            req.transcript_sources.clone(),
        );
        answer["changes"][0]["evidence"][0]["sourceId"] = json!(id);
        validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
        previous.originals = req.transcript_sources.clone();
        previous.processed.push(live);
        previous.entities[0].human_locked = true;
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner.set_context(Default::default()).unwrap();
        runner.inherit_manual(&previous).unwrap();
        assert!(runner.state.processed.is_empty());
        drop(runner);
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        let texts = if removed {
            vec![]
        } else {
            vec![LiveText {
                key: "original".into(),
                track: "system".into(),
                start_ms: 0,
                end_ms: 3000,
                text: "Okay.".into(),
            }]
        };
        let pending = runner.pending(&texts);
        assert!(runner.context_dirty());
        let client = TextClient::new(&crate::Connection {
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: "unreachable".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        })
        .unwrap();
        runner
            .update(&client, &pending, &AnalysisControl::default(), "en")
            .unwrap();
        assert!(runner.state.superseded_sources.contains(&id));
        assert!(runner
            .state
            .context_excluded_ids
            .contains(&previous.entities[0].id));
        assert_eq!(runner.state.originals[0], previous.originals[0]);
        assert_eq!(runner.state.processed.len(), usize::from(!removed));
        drop(runner);
        let state = read_state(dir.path()).unwrap();
        assert!(state
            .context_excluded_ids
            .contains(&previous.entities[0].id));
    }
}

#[test]
fn resuming_an_existing_model_imports_newer_human_confirmation_without_losing_its_history() {
    let root = tempfile::tempdir().unwrap();
    let connection = |model: &str| crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: model.into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let first = crate::live::LiveRunner::start(root.path(), "meeting", connection("first"), "en")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    let folder = first.workspace_folder().unwrap();
    drop(first);
    let mut runner = WorkspaceRunner::open(&folder).unwrap();
    let context = runner.context.lock().unwrap().clone();
    let (mut state, mut req, mut answer) = setup();
    req.purpose = "reconcile".into();
    answer["changes"][0]["kind"] = json!("impulse");
    answer["changes"][0]["text"] = json!("Who approves the draft?");
    answer["changes"][0]["due"] = json!(null);
    answer["overviewRefs"] = json!([]);
    state.apply_context(&context).unwrap();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let id = state.entities[0].id.clone();
    runner
        .commit(
            state,
            req.transcript_sources.clone(),
            vec![],
            &AnalysisControl::default(),
            &context,
        )
        .unwrap();
    drop(runner);
    let mut action = read_state(&folder).unwrap().context;
    action.overrides.push(Override {
        id: id.clone(),
        expected_revision: 1,
        status: "open".into(),
        saved: false,
    });
    update_context(&folder, &action).unwrap();
    let second = crate::live::LiveRunner::start(root.path(), "meeting", connection("second"), "en")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    let second_folder = second.workspace_folder().unwrap();
    let second_state = second.view().workspace.unwrap();
    assert_eq!(second_state.entities[0].status, "open");
    drop(second);
    let mut action = second_state.context.clone();
    action.overrides.push(Override {
        id: id.clone(),
        expected_revision: second_state.entities[0].revision,
        status: "discussed".into(),
        saved: false,
    });
    update_context(&second_folder, &action).unwrap();
    let first = crate::live::LiveRunner::start(root.path(), "meeting", connection("first"), "en")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    let view = first.view();
    let state = view.workspace.unwrap();
    assert_eq!(
        state.entities.iter().find(|e| e.id == id).unwrap().status,
        "discussed"
    );
    assert!(view.questions.is_empty());
    assert_eq!(state.events.iter().filter(|e| e.human).count(), 2);
    assert_eq!(state.originals, req.transcript_sources);
    drop(first);
    assert_eq!(read_state(&folder).unwrap().entities[0].status, "discussed");
}

#[test]
fn inheritance_uses_per_item_human_chronology_and_keeps_revisions_monotone() {
    let (mut previous, req, answer) = setup();
    validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
    previous.originals = req.transcript_sources.clone();
    previous.entities[0].human_locked = true;
    previous.entities[0].revision = 3;
    let id = previous.entities[0].id.clone();
    let at = |second| {
        chrono::DateTime::parse_from_rfc3339(&format!("2026-01-01T00:00:{second:02}Z")).unwrap()
    };
    previous.human_activity.insert(id.clone(), at(1));
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    runner.inherit_manual(&previous).unwrap();
    let mut next = runner.state.clone();
    next.entities[0].revision = 7;
    next.entities[0].status = "discussed".into();
    next.human_activity.insert(id.clone(), at(2));
    let context = next.context.clone();
    runner
        .commit(next, vec![], vec![], &AnalysisControl::default(), &context)
        .unwrap();
    // A later unrelated note or model result cannot make the older open status newer.
    previous.context_updated_at = Some(at(30));
    previous.updated_at = Some(at(40).to_rfc3339());
    let revision = runner.state.revision;
    runner.inherit_manual(&previous).unwrap();
    assert_eq!(runner.state.revision, revision);
    assert_eq!(runner.state.entities[0].status, "discussed");
    previous.human_activity.insert(id, at(3));
    runner.inherit_manual(&previous).unwrap();
    assert_eq!(runner.state.entities[0].status, "open");
    assert_eq!(runner.state.entities[0].revision, 8);
    drop(runner);
    let reopened = read_state(dir.path()).unwrap();
    assert_eq!(reopened.entities[0].revision, 8);
    assert_eq!(reopened.human_activity.values().next(), Some(&at(3)));
    assert_eq!(reopened.originals, previous.originals);
}

#[test]
fn malformed_inherited_human_timestamps_preserve_the_unreadable_revision() {
    let (mut previous, req, answer) = setup();
    validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
    previous.originals = req.transcript_sources;
    previous.entities[0].human_locked = true;
    for field in ["humanActivity", "contextUpdatedAt"] {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner.set_context(Default::default()).unwrap();
        runner.inherit_manual(&previous).unwrap();
        drop(runner);
        let path = dir.path().join("revision-00000001.json");
        let mut delta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        delta[field] = if field == "humanActivity" {
            json!({"e_one":"broken"})
        } else {
            json!("broken")
        };
        let bytes = serde_json::to_vec(&delta).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_state(dir.path()).is_err());
        assert!(WorkspaceRunner::open(dir.path()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn restored_inherited_wording_gets_fresh_source_authority_and_stays_processed_after_reopen() {
    let (mut previous, mut req, mut answer) = setup();
    let text = LiveText {
        key: "original".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: req.transcript_sources[0].text.clone(),
    };
    let inherited = live::pending(&[], std::slice::from_ref(&text)).remove(0);
    let old_id = source_id(&inherited);
    req.transcript_sources[0].id = old_id.clone();
    req = Request::new(
        "r_test".into(),
        &WorkspaceContext::default(),
        &previous,
        vec![],
        &req.transcript_sources,
        req.transcript_sources.clone(),
    );
    answer["changes"][0]["evidence"][0]["sourceId"] = json!(old_id);
    validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
    previous.originals = req.transcript_sources.clone();
    previous.processed.push(inherited.clone());
    previous.entities[0].human_locked = true;
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    runner.inherit_manual(&previous).unwrap();
    assert!(runner.state.processed.is_empty());
    assert!(runner.pending(&[]).is_empty());
    let client = TextClient::new(&crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: "no-request".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    runner
        .update(&client, &[], &AnalysisControl::default(), "en")
        .unwrap();
    assert!(runner.state.superseded_sources.contains(&old_id));
    let restored = runner.pending(std::slice::from_ref(&text));
    assert_eq!(restored.len(), 1);
    assert_ne!(restored[0].id, inherited.id);
    let new_id = source_id(&restored[0]);
    assert_ne!(new_id, old_id);
    let original = Source {
        id: new_id.clone(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: text.text.clone(),
    };
    let context = runner.context.lock().unwrap().clone();
    let mut next = runner.state.clone();
    let request = Request::new(
        "r_test".into(),
        &context,
        &next,
        vec![],
        std::slice::from_ref(&original),
        vec![original.clone()],
    );
    answer["changes"][0]["evidence"][0]["sourceId"] = json!(new_id);
    validate::apply(&mut next, &request, &answer.to_string()).unwrap();
    next.originals.push(original.clone());
    next.processed.extend(restored);
    runner
        .commit(
            next,
            vec![original],
            vec![],
            &AnalysisControl::default(),
            &context,
        )
        .unwrap();
    drop(runner);
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    assert!(runner.pending(&[text]).is_empty());
    assert!(!runner.context_dirty());
    let state = runner.view();
    assert!(state.originals.iter().any(|source| source.id == old_id));
    assert!(state.originals.iter().any(|source| source.id == new_id));
    assert!(!state.superseded_sources.contains(&new_id));
    assert_eq!(runner.active_sources().len(), 1);
    let mut classic = crate::live::LiveRunner::start(
        &dir.path().join("display"),
        "meeting",
        crate::Connection {
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: "no-request".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        },
        "en",
    )
    .unwrap()
    .view();
    classic.attach_workspace(state);
    assert_eq!(classic.tasks.len(), 1);
    assert_eq!(classic.tasks[0].evidence[0].source_id, new_id);
}
