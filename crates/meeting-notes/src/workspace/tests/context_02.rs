use super::*;

#[test]
fn damaged_classic_runs_without_workspace_do_not_block_new_shared_context() {
    for marker in ["absent", "directory", "file", "non-directory-run"] {
        let root = tempfile::tempdir().unwrap();
        let connection = |model: &str| crate::Connection {
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: model.into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let classic = crate::live::LiveRunner::start(
            root.path(),
            "unrelated-classic-meeting",
            connection("old-model"),
            "en",
        )
        .unwrap();
        let folder = root.path().join(&classic.run().id);
        drop(classic);
        let damaged = b"{damaged synthetic classic identity";
        if marker == "non-directory-run" {
            std::fs::write(
                root.path().join("abcdef0123456789abcdef0123456789"),
                damaged,
            )
            .unwrap();
        } else {
            std::fs::write(folder.join("run.json"), damaged).unwrap();
        }
        match marker {
            "directory" => std::fs::create_dir(folder.join("workspace-v9")).unwrap(),
            "file" => std::fs::write(folder.join("workspace-v9"), b"synthetic marker").unwrap(),
            _ => (),
        }
        let result =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("new-model"), "en")
                .unwrap()
                .with_workspace(Default::default());
        assert_eq!(
            result.is_ok(),
            marker == "absent",
            "Damaged run classification: {marker}"
        );
        if marker != "non-directory-run" {
            assert_eq!(std::fs::read(folder.join("run.json")).unwrap(), damaged);
        }
        if marker == "file" {
            assert_eq!(
                std::fs::read(folder.join("workspace-v9")).unwrap(),
                b"synthetic marker"
            );
        }
    }
}

#[test]
fn resumed_model_uses_latest_durable_shared_inputs_instead_of_old_cache() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    let mut failures = Vec::new();
    for withdraw in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let connection = |model: &str| crate::Connection {
            endpoint: endpoint.clone(),
            model: model.into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let initial = WorkspaceContext {
            agenda: vec![Agenda {
                id: "e_shared".into(),
                revision: 1,
                text: "Old synthetic agenda".into(),
            }],
            shared_notes: vec![SharedNote {
                id: "h_shared".into(),
                revision: 1,
                text: "Old synthetic protocol".into(),
            }],
            ..Default::default()
        };
        let first =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
                .unwrap()
                .with_workspace(initial.clone())
                .unwrap();
        let first_id = first.run().id.clone();
        let first_folder = first.workspace_folder().unwrap();
        let first_bytes = std::fs::read(first_folder.join("context-00000001.json")).unwrap();
        drop(first);
        // Synthetic checkpoint time must not outrank a later human-context edit.
        let mut checkpoint = WorkspaceState::default();
        checkpoint
            .apply_context(&context_store::read(&first_folder, &Default::default()).unwrap())
            .unwrap();
        checkpoint.revision = 1;
        checkpoint.updated_at = Some("2099-01-01T00:00:00Z".into());
        store::append(
            &first_folder,
            &store::Delta::between(&WorkspaceState::default(), &checkpoint, vec![], vec![]),
        )
        .unwrap();
        let second =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-b"), "en")
                .unwrap()
                .with_workspace(initial.clone())
                .unwrap();
        let second_folder = second.workspace_folder().unwrap();
        let mut edited = second.workspace_context().unwrap().lock().unwrap().clone();
        if withdraw {
            edited.agenda.clear();
            edited.shared_notes.clear();
        } else {
            edited.agenda[0].text = "New synthetic agenda".into();
            edited.agenda[0].revision += 1;
            edited.shared_notes[0].text = "New synthetic protocol".into();
            edited.shared_notes[0].revision += 1;
        }
        let (durable, _) = update_context(&second_folder, &edited).unwrap();
        *second.workspace_context().unwrap().lock().unwrap() = durable.clone();
        let second_bytes = std::fs::read(second_folder.join("context-00000002.json")).unwrap();
        drop(second);
        // A later confirmation in A does not release A's older shared text again.
        let mut old_context = read_state(&first_folder).unwrap().context;
        old_context.overrides.push(Override {
            id: "e_shared".into(),
            expected_revision: 1,
            status: "discussed".into(),
            saved: false,
        });
        update_context(&first_folder, &old_context).unwrap();
        let server = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(8);
            let mut requests = Vec::new();
            while requests.len() < 2 {
                assert!(Instant::now() < end, "synthetic inference did not finish");
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 8192];
                let header = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && bytes.len() < 500_000);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(at) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let size = String::from_utf8_lossy(&bytes[..header])
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .unwrap()
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                while bytes.len() < header + size {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes[header..header + size]).unwrap();
                let req: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":[],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                requests.push(req);
            }
            requests
        });
        let mut resumed =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
                .unwrap()
                .with_workspace(initial.clone())
                .unwrap();
        assert_eq!(resumed.run().id, first_id);
        let current = resumed.workspace_context().unwrap().lock().unwrap().clone();
        let pending = resumed.pending(&[LiveText {
            key: "new-part".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: "We discussed the draft without choosing an owner.".into(),
        }]);
        let result = resumed.update(
            &pending,
            crate::live::Trigger::Manual,
            &AnalysisControl::default(),
        );
        let requests = server.join().unwrap();
        result.unwrap();
        if current.agenda != durable.agenda || current.shared_notes != durable.shared_notes {
            failures.push(format!("withdraw={withdraw}: resumed older shared inputs"));
        }
        for req in requests {
            let sent = req.to_string();
            if sent.contains("Old synthetic protocol") || sent.contains("Old synthetic agenda") {
                failures.push(format!(
                    "withdraw={withdraw}: stale input reached {} request",
                    req["purpose"]
                ));
            }
            if !withdraw
                && (!sent.contains("New synthetic protocol")
                    || (req["purpose"] == "reconcile" && !sent.contains("New synthetic agenda")))
            {
                failures.push(format!(
                    "withdraw={withdraw}: current native input missing from {} request",
                    req["purpose"]
                ));
            }
        }
        let expected = resumed.view().workspace.unwrap();
        drop(resumed);
        assert_eq!(
            serde_json::to_vec(&read_state(&first_folder).unwrap()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        let again =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
                .unwrap()
                .with_workspace(initial.clone())
                .unwrap();
        let restored = again.workspace_context().unwrap().lock().unwrap().clone();
        if restored.agenda != durable.agenda || restored.shared_notes != durable.shared_notes {
            failures.push(format!(
                "withdraw={withdraw}: same-model restart revived stale inputs"
            ));
        }
        drop(again);
        let fresh =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-c"), "en")
                .unwrap()
                .with_workspace(initial.clone())
                .unwrap();
        let restored = fresh.workspace_context().unwrap().lock().unwrap().clone();
        if restored.agenda != durable.agenda || restored.shared_notes != durable.shared_notes {
            failures.push(format!(
                "withdraw={withdraw}: fresh model trusted stale browser input"
            ));
        }
        drop(fresh);
        assert_eq!(
            std::fs::read(first_folder.join("context-00000001.json")).unwrap(),
            first_bytes
        );
        assert_eq!(
            std::fs::read(second_folder.join("context-00000002.json")).unwrap(),
            second_bytes
        );
    }
    assert!(
        failures.is_empty(),
        "Shared-input resume failures: {failures:?}"
    );
}

#[test]
fn unreadable_shared_context_blocks_resume_without_falling_back_to_withdrawn_text() {
    let root = tempfile::tempdir().unwrap();
    let connection = |model: &str| crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: model.into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    for model in ["model-a", "model-b"] {
        let runner =
            crate::live::LiveRunner::start(root.path(), "meeting", connection(model), "en")
                .unwrap()
                .with_workspace(Default::default())
                .unwrap();
        let folder = runner.workspace_folder().unwrap();
        drop(runner);
        if model == "model-b" {
            std::fs::write(
                folder.join("context-00000001.json"),
                b"{damaged synthetic context",
            )
            .unwrap();
        }
    }
    let resumed =
        crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
            .unwrap()
            .with_workspace(Default::default());
    assert!(
        resumed.is_err(),
        "Damaged native shared context was silently ignored"
    );
}

#[test]
fn conflicting_legacy_contexts_need_confirmation_instead_of_model_creation_order() {
    let root = tempfile::tempdir().unwrap();
    let connection = |model: &str| crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: model.into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let mut original_files = Vec::new();
    for (model, text) in [
        ("model-a", "Synthetic original sharing"),
        ("model-b", "Synthetic different sharing"),
    ] {
        let live = crate::live::LiveRunner::start(root.path(), "meeting", connection(model), "en")
            .unwrap()
            .with_workspace(Default::default())
            .unwrap();
        let folder = live.workspace_folder().unwrap();
        let mut context = live.workspace_context().unwrap().lock().unwrap().clone();
        context.shared_notes = vec![SharedNote {
            id: "h_note".into(),
            revision: 1,
            text: text.into(),
        }];
        update_context(&folder, &context).unwrap();
        drop(live);
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("context-")
                && path.extension().is_some_and(|v| v == "json")
            {
                let mut json: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                json.as_object_mut().unwrap().remove("createdAt");
                let bytes = serde_json::to_vec(&json).unwrap();
                std::fs::write(&path, &bytes).unwrap();
                original_files.push((path, bytes));
            }
        }
    }
    let resumed =
        crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
            .unwrap()
            .with_workspace(Default::default());
    assert!(
        resumed.is_err(),
        "Model creation order guessed authority between conflicting untimestamped contexts"
    );
    for (path, bytes) in original_files {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn edited_protocol_retains_human_confirmed_state_before_and_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    let initial = WorkspaceContext {
        shared_notes: vec![SharedNote {
            id: "h_protocol".into(),
            revision: 1,
            text: "Shared context".into(),
        }],
        ..Default::default()
    };
    runner.set_context(initial).unwrap();
    let current = runner.context.lock().unwrap().clone();
    let (_, mut req, mut answer) = setup();
    req.purpose = "reconcile".into();
    answer["changes"][0]["kind"] = json!("impulse");
    answer["changes"][0]["text"] = json!("Wer gibt den Entwurf frei?");
    answer["changes"][0]["due"] = json!(null);
    answer["overviewRefs"] = json!([]);
    let mut next = runner.state.clone();
    next.apply_context(&current).unwrap();
    validate::apply(&mut next, &req, &answer.to_string()).unwrap();
    next.originals = req.transcript_sources.clone();
    let mut draft = next.entities[0].clone();
    draft.id = "e_draft".into();
    next.entities.push(draft);
    let id = next.entities[0].id.clone();
    runner
        .commit(
            next,
            req.transcript_sources.clone(),
            vec![],
            &AnalysisControl::default(),
            &current,
        )
        .unwrap();
    let mut manual = current;
    manual.overrides.push(Override {
        id: id.clone(),
        expected_revision: 1,
        status: "discussed".into(),
        saved: false,
    });
    let (mut edited, _) = update_context(dir.path(), &manual).unwrap();
    // No inference checkpoint has incorporated the human action yet.
    edited.shared_notes[0].text.push('!');
    edited.shared_notes[0].revision += 1;
    let (_, active) = update_context(dir.path(), &edited).unwrap();
    assert!(!active.context_excluded_ids.contains(&id));
    assert!(!active.context_excluded_ids.contains(&"e_draft".into()));
    assert_eq!(active.entities[0].status, "discussed");
    assert!(active.entities[0].human_locked);
    drop(runner);
    let reopened = read_state(dir.path()).unwrap();
    assert!(!reopened.context_excluded_ids.contains(&id));
    assert!(reopened.entities[0].human_locked);
    assert_eq!(reopened.events.iter().filter(|e| e.human).count(), 1);
    assert_eq!(reopened.originals, req.transcript_sources);
    let mut revoked = reopened.clone();
    let mut without_note = revoked.context.clone();
    without_note.shared_notes.clear();
    revoked.apply_context(&without_note).unwrap();
    assert!(!revoked.context_excluded_ids.contains(&id));
    assert!(revoked.context_excluded_ids.contains(&"e_draft".into()));
    let mut withdrawn = reopened;
    withdrawn
        .superseded_sources
        .push(req.transcript_sources[0].id.clone());
    let mut edited = withdrawn.context.clone();
    edited.shared_notes.clear();
    withdrawn.apply_context(&edited).unwrap();
    assert!(withdrawn.context_excluded_ids.contains(&id));
}

#[test]
fn an_acknowledged_missing_commitment_cannot_be_only_a_touched_agenda() {
    let (mut state, mut req, mut answer) = setup();
    req.purpose = "reconcile".into();
    req.transcript_sources[0].text = "There is no commitment on the budget.".into();
    let agenda = Entity {
        id: "e_agenda".into(),
        revision: 1,
        kind: "agenda".into(),
        text: "Clarify budget".into(),
        status: "open".into(),
        attention: "normal".into(),
        owner: None,
        due: None,
        evidence: vec![],
        human_locked: false,
    };
    state.entities.push(agenda.clone());
    req.state.push(agenda);
    let change = &mut answer["changes"][0];
    change["kind"] = json!("agenda");
    change["targetId"] = json!("e_agenda");
    change["targetRevision"] = json!(1);
    change["text"] = json!(null);
    change["status"] = json!("touched");
    change["due"] = json!(null);
    change["evidence"][0]["quote"] = json!("There is no commitment on the budget.");
    answer["overviewRefs"] = json!([]);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    let mut impulse = answer["changes"][0].clone();
    impulse["proposalId"] = json!("p_question");
    impulse["kind"] = json!("impulse");
    impulse["targetId"] = json!(null);
    impulse["targetRevision"] = json!(null);
    impulse["text"] = json!("Who can approve the budget?");
    impulse["status"] = json!("open");
    impulse["attention"] = json!("clarification");
    answer["changes"].as_array_mut().unwrap().push(impulse);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert!(state
        .entities
        .iter()
        .any(|e| e.kind == "impulse" && e.status == "open"));
}

#[test]
fn numbered_human_agenda_can_change_status_without_generating_a_title_claim() {
    for invalid in ["none", "many quotes", "invented quote", "rewrite title"] {
        let (mut state, mut req, mut answer) = setup();
        state
            .apply_context(&WorkspaceContext {
                agenda: vec![Agenda {
                    id: "e_topic".into(),
                    revision: 1,
                    text: "1. Budget 2027".into(),
                }],
                ..Default::default()
            })
            .unwrap();
        req.state = state.entities.clone();
        req.purpose = "reconcile".into();
        req.transcript_sources[0].text = "We discussed the budget.".into();
        let change = &mut answer["changes"][0];
        change["kind"] = json!("agenda");
        change["targetId"] = json!("e_topic");
        change["targetRevision"] = json!(1);
        change["text"] = json!(null);
        change["status"] = json!("discussed");
        change["due"] = json!(null);
        change["evidence"][0]["quote"] = json!("We discussed the budget.");
        answer["overviewRefs"] = json!([]);
        if invalid == "invented quote" {
            answer["changes"][0]["evidence"][0]["quote"] = json!("We discussed an invented 42.");
        }
        if invalid == "rewrite title" {
            answer["changes"][0]["text"] = json!("Changed human title");
        }
        if invalid == "many quotes" {
            req.transcript_sources = (0..6)
                .map(|i| Source {
                    id: format!("s_{i}"),
                    source: "system".into(),
                    start_ms: i * 3000,
                    end_ms: (i + 1) * 3000,
                    text: format!("We discussed the budget topic {i}. {}", "a".repeat(400)),
                })
                .collect();
            answer["changes"][0]["evidence"] = json!(req
                .transcript_sources
                .iter()
                .map(|s| json!({"sourceId":s.id,"quote":s.text}))
                .collect::<Vec<_>>());
        }
        let before = serde_json::to_vec(&state).unwrap();
        let result = validate::apply(&mut state, &req, &answer.to_string());
        assert_eq!(
            result.is_ok(),
            ["none", "many quotes"].contains(&invalid),
            "{invalid}: {result:?}"
        );
        if ["none", "many quotes"].contains(&invalid) {
            assert_eq!(state.entities[0].text, "1. Budget 2027");
            assert_eq!(state.entities[0].status, "discussed");
            assert_eq!(
                state.entities[0].evidence.len(),
                if invalid == "many quotes" { 6 } else { 1 }
            );
            assert!(state.entities[0].evidence.iter().all(|e| req
                .transcript_sources
                .iter()
                .any(|s| s.id == e.source_id && s.text.contains(&e.quote))));
        } else {
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn durable_human_activity_selects_the_last_edited_model_without_new_inference() {
    let root = tempfile::tempdir().unwrap();
    let connection = |model: &str| crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: model.into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let mut folders = vec![];
    for (model, at) in [
        ("first", "2026-01-01T00:00:00Z"),
        ("second", "2026-01-02T00:00:00Z"),
    ] {
        let live = crate::live::LiveRunner::start(root.path(), "meeting", connection(model), "en")
            .unwrap()
            .with_workspace(Default::default())
            .unwrap();
        let folder = live.workspace_folder().unwrap();
        drop(live);
        let initial = context_store::read(&folder, &Default::default()).unwrap();
        let (mut state, mut req, mut answer) = setup();
        req.purpose = "reconcile".into();
        answer["changes"][0]["kind"] = json!("impulse");
        answer["changes"][0]["text"] = json!("Who approves the draft?");
        answer["changes"][0]["due"] = json!(null);
        answer["overviewRefs"] = json!([]);
        state.apply_context(&initial).unwrap();
        validate::apply(&mut state, &req, &answer.to_string()).unwrap();
        state.revision = 1;
        state.updated_at = Some(at.into());
        store::append(
            &folder,
            &store::Delta::between(
                &WorkspaceState::default(),
                &state,
                req.transcript_sources.clone(),
                vec![],
            ),
        )
        .unwrap();
        // Deterministic synthetic history, not the host clock or a user record.
        let file = folder.join("context-00000001.json");
        let mut entry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        entry["createdAt"] = json!(at);
        std::fs::write(&file, serde_json::to_vec(&entry).unwrap()).unwrap();
        folders.push(folder);
    }
    assert_eq!(
        crate::live::read_latest_workspace(root.path(), "meeting")
            .unwrap()
            .unwrap()
            .model,
        "second"
    );
    let first = read_state(&folders[0]).unwrap();
    let mut context = first.context.clone();
    context.overrides.push(Override {
        id: first.entities[0].id.clone(),
        expected_revision: 1,
        status: "discussed".into(),
        saved: false,
    });
    update_context(&folders[0], &context).unwrap();
    for selected in [
        crate::live::read_latest(root.path(), "meeting")
            .unwrap()
            .unwrap(),
        crate::live::read_latest_workspace(root.path(), "meeting")
            .unwrap()
            .unwrap(),
    ] {
        assert_eq!(selected.model, "first");
        assert_eq!(selected.updated_at.as_deref(), Some("2026-01-01T00:00:00Z")); // A human edit is not a new model inference.
        assert_eq!(selected.workspace.unwrap().entities[0].status, "discussed");
    }
    let third = crate::live::LiveRunner::start(root.path(), "meeting", connection("third"), "en")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    assert_eq!(
        third.view().workspace.unwrap().entities[0].status,
        "discussed"
    );
}

#[test]
fn ambiguous_legacy_human_conflicts_require_confirmation_without_replacing_saved_bytes() {
    let (mut previous, req, answer) = setup();
    validate::apply(&mut previous, &req, &answer.to_string()).unwrap();
    previous.originals = req.transcript_sources;
    previous.entities[0].human_locked = true;
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    runner.inherit_manual(&previous).unwrap();
    let path = dir.path().join("revision-00000001.json");
    let bytes = std::fs::read(&path).unwrap();
    let mut identical_confirmation = previous.clone();
    identical_confirmation.entities[0].revision += 10;
    runner.inherit_manual(&identical_confirmation).unwrap();
    assert_eq!(runner.state.revision, 1);
    assert_eq!(
        runner.state.entities[0].revision,
        previous.entities[0].revision
    );
    previous.entities[0].status = "discussed".into();
    assert!(runner
        .inherit_manual(&previous)
        .unwrap_err()
        .to_string()
        .contains("neu bestätigen"));
    assert_eq!(runner.state.entities[0].status, "open");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(!dir.path().join("revision-00000002.json").exists());
}

#[test]
fn oversized_shared_notes_are_rejected_before_publication_and_keep_the_prior_context() {
    for escaped in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let runner = WorkspaceRunner::open(dir.path()).unwrap();
        let (mut context, _) = update_context(dir.path(), &WorkspaceContext::default()).unwrap();
        let path = dir.path().join("context-00000001.json");
        let bytes = std::fs::read(&path).unwrap();
        let text = if escaped {
            "\"".repeat(1600)
        } else {
            "a".repeat(1600)
        };
        context.shared_notes = (0..if escaped { 12 } else { 20 })
            .map(|i| SharedNote {
                id: format!("h_note_{i}"),
                revision: 1,
                text: text.clone(),
            })
            .collect();
        let draft = context.shared_notes.clone();
        let error = update_context(dir.path(), &context).unwrap_err();
        assert!(error.to_string().contains("kürzen"));
        assert_eq!(context.shared_notes, draft);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(!dir.path().join("context-00000002.json").exists());
        assert!(read_state(dir.path())
            .unwrap()
            .context
            .shared_notes
            .is_empty());
        drop(runner);
    }
}

#[test]
fn oversized_legacy_shared_context_remains_readable_and_can_be_shortened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context-00000001.json");
    let notes = (0..20)
        .map(|i| SharedNote {
            id: format!("h_old_{i}"),
            revision: 1,
            text: "x".repeat(1600),
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&json!({"createdAt":"2026-01-01T00:00:00Z","revision":1,"reset":true,"agenda":[],"sharedNotes":notes,"overrides":[]})).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    let mut context = runner.context.lock().unwrap().clone();
    assert_eq!(context.shared_notes.len(), 20);
    assert_eq!(read_state(dir.path()).unwrap().context, context);
    context.shared_notes[0].text = "Shorter released context".into();
    context.shared_notes.truncate(1);
    let (_, saved) = update_context(dir.path(), &context).unwrap();
    assert_eq!(saved.context.shared_notes, context.shared_notes);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(read_state(dir.path()).unwrap().context, saved.context);
}
#[test]
fn withdrawal_requires_explicit_current_intent_and_rejection_preserves_both_kinds() {
    let invalid = [
        "Ich hoffe, wir brauchen den Bericht nicht mehr.",
        "Wir bestreiten, wir brauchen den Bericht nicht mehr.",
        "Wir müssen prüfen, wir brauchen den Bericht nicht mehr.",
        "Bitte prüfen, wir brauchen den Bericht nicht mehr.",
        "We check that the decision has been withdrawn.",
        "We denied that the decision has been withdrawn.",
        "We deny that the decision has been withdrawn.",
        "We disputed that the decision has been withdrawn.",
        "We imagined that the decision has been withdrawn.",
        "We confirmed that we denied that the decision has been withdrawn.",
        "We denied the decision has been withdrawn.",
        "Wir bestreiten, dass wir den Beschluss zurückgenommen haben.",
        "Wir haben bestritten, dass der Beschluss zurückgenommen wurde.",
        "The members of the team must check that the decision has been withdrawn.",
        "A team must check that the decision has been withdrawn.",
        "All engineers must verify that the decision has been withdrawn.",
        "Our platform security team must check that the decision has been withdrawn.",
        "We must check that the decision has been withdrawn.",
        "We must check that we have cancelled the report.",
        "The team must confirm that the decision has been revoked.",
        "We had to check that the report has been cancelled.",
        "We have to verify that the report has been cancelled.",
        "We need to confirm that the decision has been withdrawn.",
        "Please check that the decision has been withdrawn.",
        "Check that the decision has been withdrawn.",
        "Verify that we have cancelled the report.",
        "Wir mussten prüfen, dass wir den Beschluss zurückgenommen haben.",
        "Das Team muss bestätigen, dass der Beschluss zurückgenommen wurde.",
        "Bitte bestätigen, dass wir den Beschluss zurückgenommen haben.",
        "Das Team musste widerrufen.",
        "Wir hatten den Beschluss widerrufen wollen.",
        "Wir haben den Beschluss widerrufen müssen.",
        "Das Team konnte widerrufen.",
        "Das Team wollte stornieren.",
        "Das Team durfte widerrufen.",
        "Das Team mochte stornieren.",
        "Die Teams mussten widerrufen.",
        "Die Teams konnten widerrufen.",
        "Die Teams wollten stornieren.",
        "We hope the team has withdrawn the decision.",
        "Wir haben gehofft, dass wir den Beschluss zurückgenommen haben.",
        "We expect that the report has been cancelled.",
        "We wished we had withdrawn the decision.",
        "The team can cancel the report.",
        "Our team must cancel the report.",
        "The team may cancel the report.",
        "ANNA CAN CANCEL THE REPORT.",
        "Das Team kann widerrufen.",
        "Das Team muss widerrufen.",
        "Das Team darf widerrufen.",
        "Das Team möchte widerrufen.",
        "The team may have cancelled the report.",
        "We can have cancelled the report.",
        "Anna must have withdrawn the decision.",
        "Anna plans to cancel the report.",
        "ANNA PLANS TO CANCEL THE REPORT.",
        "WE WANT IT CANCELLED.",
        "Das Team plant, den Beschluss zu widerrufen.",
        "We plan to cancel the report.",
        "We have planned to cancel the report.",
        "Ich habe vorgeschlagen, dass der Beschluss zurückgenommen wird.",
        "Wir müssen den Beschluss widerrufen.",
        "We plan to have cancelled the report.",
        "Wir haben vor den Beschluss widerrufen zu haben.",
        "Wir nehmen den Beschluss nicht zurück.",
        "Wir widerrufen das nicht.",
        "Darauf kommen wir morgen zurück.",
        "Wir kommen auf den Bericht zurück.",
        "Der Beschluss ist nicht zurückgenommen.",
        "Wir nehmen den Beschluss nicht mehr zurück.",
        "Wir wissen nicht mehr, was der Beschluss war.",
        "Wir brauchen den Beschluss nicht mehr zurückzunehmen.",
        "Vielleicht nehmen wir den Beschluss zurück.",
        "Wenn nötig nehmen wir den Beschluss zurück.",
        "Wir würden den Beschluss zurücknehmen.",
        "Wir ziehen den Beschluss zurück?",
        "Das Widerrufsformular ist angekommen.",
        "We do not withdraw the decision.",
        "We didn't cancel the report.",
        "We don’t cancel the report.",
        "We never cancelled the report.",
        "We might cancel the report.",
        "If needed, we withdraw the decision.",
        "The report is uncancelled.",
        "We discuss the cancellation policy.",
        "Nobody cancelled the report.",
        "Noone withdrew the decision.",
        "Wir haben die Zusage keineswegs widerrufen.",
        "Wir haben nichts zurückgenommen.",
        "We withdrew nothing.",
        "We have withdrawn none of the decisions.",
        "We withdrew neither decision.",
        "Wir widerrufen nichts.",
        "We revoked none of the decisions.",
    ];
    let valid = [
        "Wir haben bestätigt, wir brauchen den Bericht nicht mehr.",
        "Wir hoffen. Wir brauchen den Bericht nicht mehr.",
        "Ich habe bestätigt, wir benötigen den Bericht nicht mehr.",
        "We did check that the decision has been withdrawn.",
        "We verified that the decision has been withdrawn.",
        "The members of the team confirmed that the decision has been withdrawn.",
        "We denied the claim, but the decision has been withdrawn.",
        "Our platform security team has confirmed that the decision has been withdrawn.",
        "We have checked that the decision has been withdrawn.",
        "We must check, but the decision has been withdrawn.",
        "We must check. The decision has been withdrawn.",
        "Wir haben bestätigt, dass wir den Beschluss zurückgenommen haben.",
        "We hoped, but the decision has been withdrawn.",
        "Wir hofften, aber der Beschluss wurde zurückgenommen.",
        "May cancelled the report.",
        "Anna cancelled the report.",
        "Anna withdrew the decision.",
        "Robin widerruft die Zusage.",
        "The team cancelled the report.",
        "Das zuständige Team widerruft die Zusage.",
        "Anna just cancelled the report.",
        "Anna Marie cancelled the report.",
        "We’ve withdrawn the decision.",
        "I've cancelled the report.",
        "Wir haben den Beschluss, wie besprochen, zurückgenommen.",
        "We have, as agreed, cancelled the report.",
        "Wir nehmen den Beschluss zurück.",
        "Wir ziehen die Zusage zurück.",
        "Der Beschluss wurde zurückgenommen.",
        "Ich widerrufe die Zusage.",
        "Wir widerrufen den Beschluss.",
        "Wir brauchen den Bericht nicht mehr.",
        "Wir nehmen den Beschluss jedenfalls zurück.",
        "We withdraw the decision.",
        "The decision has been withdrawn.",
        "We cancelled the report.",
        "We withdrew the decision nonetheless.",
        "Ich habe den Beschluss zurückgenommen.",
        "We have withdrawn the decision.",
    ];
    let mut accepted_invalid = Vec::new();
    let mut rejected_valid = Vec::new();
    for kind in ["task", "decision"] {
        let (mut state, mut request, mut answer) = setup();
        if kind == "decision" {
            request.purpose = "facts".into();
            request.transcript_sources[0].text = "Wir beschließen den Pilotstart.".into();
            answer["changes"][0]["kind"] = json!("decision");
            answer["changes"][0]["text"] = json!("Pilotstart beschlossen");
            answer["changes"][0]["status"] = json!("recorded");
            answer["changes"][0]["evidence"][0]["quote"] = json!("Wir beschließen den Pilotstart.");
        }
        answer["changes"][0]["due"] = json!(null);
        let seed_events = validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        state.revision = 1;
        state.originals = request.transcript_sources.clone();
        let bytes = serde_json::to_vec(&state).unwrap();
        for (quote, allowed) in invalid
            .iter()
            .map(|q| (*q, false))
            .chain(valid.iter().map(|q| (*q, true)))
        {
            let directory = tempfile::tempdir().unwrap();
            let (_writer, empty) = store::load(directory.path()).unwrap();
            store::append(
                directory.path(),
                &store::Delta::between(
                    &empty,
                    &state,
                    request.transcript_sources.clone(),
                    seed_events.clone(),
                ),
            )
            .unwrap();
            let checkpoint =
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap();
            let source = Source {
                id: "s_status".into(),
                source: "system".into(),
                start_ms: 3000,
                end_ms: 6000,
                text: quote.into(),
            };
            let mut sources = request.transcript_sources.clone();
            sources.push(source.clone());
            let mut status = Request::new(
                "r_status".into(),
                &Default::default(),
                &state,
                state.entities.clone(),
                std::slice::from_ref(&source),
                sources,
            );
            status.purpose = if kind == "task" { "tasks" } else { "facts" }.into();
            let mut change = answer.clone();
            change["requestId"] = json!("r_status");
            change["changes"][0]["targetId"] = json!(state.entities[0].id);
            change["changes"][0]["targetRevision"] = json!(state.entities[0].revision);
            change["changes"][0]["text"] = json!(null);
            change["changes"][0]["status"] = json!("withdrawn");
            change["changes"][0]["evidence"] = json!([{"sourceId":"s_status","quote":quote}]);
            let mut next = state.clone();
            let result = validate::apply(&mut next, &status, &change.to_string());
            if allowed && result.is_err() {
                rejected_valid.push(format!("{kind}: {quote}: {}", result.unwrap_err()));
                assert_eq!(serde_json::to_vec(&next).unwrap(), bytes, "{quote}");
                assert!(!directory.path().join("revision-00000002.json").exists());
                assert_eq!(
                    read_state(directory.path()).unwrap().entities,
                    state.entities
                );
            } else if allowed {
                let events = result.unwrap_or_else(|e| panic!("{kind}: {quote}: {e}"));
                assert_eq!(next.entities[0].status, "withdrawn");
                assert_eq!(next.entities[0].text, state.entities[0].text);
                assert!(state.entities[0]
                    .evidence
                    .iter()
                    .all(|e| next.entities[0].evidence.contains(e)));
                next.revision = 2;
                store::append(
                    directory.path(),
                    &store::Delta::between(&state, &next, vec![source], events),
                )
                .unwrap();
                let reopened = read_state(directory.path()).unwrap();
                assert_eq!(reopened.entities, next.entities);
                assert!(reopened.originals.iter().any(|s| s.text == quote));
                assert_eq!(
                    serde_json::to_vec(&reopened).unwrap(),
                    serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap()
                );
            } else if result.is_ok() {
                accepted_invalid.push(format!("{kind}: {quote}"));
            } else {
                assert_eq!(serde_json::to_vec(&next).unwrap(), bytes, "{quote}");
                assert!(!directory.path().join("revision-00000002.json").exists());
                assert_eq!(
                    read_state(directory.path()).unwrap().entities,
                    state.entities
                );
            }
            assert_eq!(
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap(),
                checkpoint
            );
        }
    }
    assert!(
        rejected_valid.is_empty(),
        "Valid withdrawal rejected: {rejected_valid:?}"
    );
    assert!(
        accepted_invalid.is_empty(),
        "Invalid withdrawal accepted: {accepted_invalid:?}"
    );
}

#[test]
fn invalid_model_answer_gets_one_fully_validated_retry_and_never_a_partial_save() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    for always_bad in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_owned();
        let server = thread::spawn(move || {
            let expected = if always_bad { 2 } else { 4 };
            let end = Instant::now() + Duration::from_secs(5);
            let mut phases = Vec::new();
            while phases.len() < expected && Instant::now() < end {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 8192];
                let header;
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0 && bytes.len() < 1_000_000);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(p) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        header = p + 4;
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&bytes[..header]);
                let size = text
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .unwrap()
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                while bytes.len() < header + size {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes[header..header + size]).unwrap();
                let input: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                let index = phases.len();
                phases.push(input["purpose"].as_str().unwrap().to_owned());
                assert!(
                    !folder.join("revision-00000001.json").exists(),
                    "No intermediate or invalid answer may be published"
                );
                let change = json!({"proposalId":"p_one","kind":"task","targetId":null,"targetRevision":null,"text":"Entwurf freigeben","status":"open","attention":"current","owner":null,"due":"morgen","evidence":[{"sourceId":input["transcriptSources"][0]["id"],"quote":"Ich gebe den Entwurf morgen frei."}],"relatedEntityIds":[]});
                let changes = if always_bad || index == 0 {
                    json!([change.clone(), change])
                } else if index == 1 {
                    json!([change])
                } else {
                    json!([])
                };
                let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":input["requestId"],"changes":changes,"overviewRefs":if index<=1{json!(["p_one"])}else{json!([])},"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
            }
            phases
        });
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner.set_context(Default::default()).unwrap();
        let client = TextClient::new(&crate::Connection {
            endpoint,
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        })
        .unwrap();
        let texts = vec![LiveText {
            key: "generated".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: "Ich gebe den Entwurf morgen frei.".into(),
        }];
        let pending = runner.pending(&texts);
        let control = AnalysisControl::default();
        let result = runner.update(&client, &pending, &control, "de");
        assert_eq!(control.validation_retries(), 1);
        let phases = server.join().unwrap();
        if always_bad {
            assert!(result.is_err());
            assert_eq!(runner.state.revision, 0);
            assert!(!dir.path().join("revision-00000001.json").exists());
            assert_eq!(phases, ["tasks", "tasks"]);
        } else {
            result.unwrap();
            assert_eq!(runner.state.entities.len(), 1);
            assert_eq!(runner.state.revision, 1);
            assert_eq!(phases, ["tasks", "tasks", "facts", "reconcile"]);
        }
    }
}
