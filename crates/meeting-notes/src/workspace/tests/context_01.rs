use super::*;

#[test]
fn source_dto_has_track_and_no_path_or_private_context() {
    let (_, req, _) = setup();
    let v = serde_json::to_value(req).unwrap();
    assert_eq!(v["transcriptSources"][0]["track"], "system");
    assert!(v["transcriptSources"][0].get("source").is_none());
    assert_eq!(v["humanContext"], json!([]));
    let private =
        json!({"revision":0,"agenda":[],"sharedNotes":[],"overrides":[],"privateNotes":"secret"});
    assert!(serde_json::from_value::<WorkspaceContext>(private).is_err());
}
#[test]
fn human_changes_are_durable_while_inference_writer_is_held_and_reopen_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (writer, old) = store::load(dir.path()).unwrap();
    let (mut next, req, answer) = setup();
    let events = validate::apply(&mut next, &req, &answer.to_string()).unwrap();
    next.revision = 1;
    store::append(
        dir.path(),
        &store::Delta::between(&old, &next, req.transcript_sources.clone(), events),
    )
    .unwrap();
    let id = next.entities[0].id.clone();
    let mut context = WorkspaceContext::default();
    context.overrides.push(Override {
        id: id.clone(),
        expected_revision: 1,
        status: "discussed".into(),
        saved: false,
    });
    let (mut context, view) = context_store::update(dir.path(), &context).unwrap();
    assert_eq!(view.entities[0].revision, 2);
    context.overrides.push(Override {
        id,
        expected_revision: 2,
        status: "open".into(),
        saved: false,
    });
    let (context, view) = context_store::update(dir.path(), &context).unwrap();
    assert_eq!(view.entities[0].revision, 3);
    assert_eq!(view.entities[0].status, "open");
    assert!(view.entities[0].human_locked);
    assert!(context_store::update(dir.path(), &WorkspaceContext::default()).is_err());
    let mut removed = context.clone();
    removed.overrides.clear();
    assert!(context_store::update(dir.path(), &removed).is_err());
    drop(writer);
    let reopened = read_state(dir.path()).unwrap();
    assert_eq!(reopened.entities, view.entities);
    assert_eq!(reopened.context, context);
    assert!(dir.path().join("context-00000001.json").exists());
    assert!(dir.path().join("context-00000002.json").exists());
}
#[test]
fn a_context_saved_during_inference_rejects_stale_model_publication() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let expected = runner.context.lock().unwrap().clone();
    let mut changed = expected.clone();
    changed.shared_notes.push(SharedNote {
        id: "h_note".into(),
        revision: 1,
        text: "Freigegebene neue Notiz".into(),
    });
    let (next, _) = context_store::update(dir.path(), &changed).unwrap();
    *runner.context.lock().unwrap() = next.clone();
    assert!(runner
        .commit(
            runner.state.clone(),
            vec![],
            vec![],
            &AnalysisControl::default(),
            &expected
        )
        .is_err());
    assert!(!dir.path().join("revision-00000001.json").exists());
    assert_eq!(read_state(dir.path()).unwrap().context, next);
}
#[test]
fn revoked_notes_exclude_derived_items_but_preserve_history() {
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    state.context.shared_notes.push(SharedNote {
        id: "h_note".into(),
        revision: 1,
        text: "Nur freigegebener Kontext".into(),
    });
    let id = state.entities[0].id.clone();
    state.apply_context(&WorkspaceContext::default()).unwrap();
    assert!(state.context_excluded_ids.contains(&id));
    assert_eq!(state.entities.len(), 1);
    assert_eq!(state.events.len(), 1);
    assert!(state.overview_refs.is_empty());
}
#[test]
fn newly_shared_agenda_replays_original_text_without_duplication_or_a_new_transcript() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut inputs = vec![];
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            let header;
            loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && bytes.len() < 500000);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(p) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    header = p + 4;
                    break;
                }
            }
            let text = String::from_utf8_lossy(&bytes[..header]);
            let size = text
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse::<usize>()
                .unwrap();
            while bytes.len() < header + size {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[header..header + size]).unwrap();
            let input: serde_json::Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":input["requestId"],"changes":[],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
            inputs.push(input);
            let payload = format!(
                "data: {}\n\ndata: {}\n\n",
                json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
            );
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
        }
        inputs
    });
    let dir = tempfile::tempdir().unwrap();
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
        key: "generated-part".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: "Zum Budget wurde noch keine Zusage getroffen.".into(),
    }];
    let pending = runner.pending(&texts);
    runner
        .update(&client, &pending, &AnalysisControl::default(), "en")
        .unwrap();
    let mut context = runner.context.lock().unwrap().clone();
    context.agenda.push(Agenda {
        id: "e_budget".into(),
        revision: 1,
        text: "Budget klären".into(),
    });
    let (context, _) = update_context(dir.path(), &context).unwrap();
    *runner.context.lock().unwrap() = context;
    assert!(runner.pending(&texts).is_empty());
    assert!(runner.context_dirty());
    runner
        .update(&client, &[], &AnalysisControl::default(), "en")
        .unwrap();
    assert!(!runner.context_dirty());
    assert_eq!(runner.state.processed.len(), 1);
    assert_eq!(runner.state.originals.len(), 1);
    assert_eq!(runner.state.entities[0].status, "open");
    let requests = server.join().unwrap();
    assert!(requests.iter().all(|input| input["language"] == "en"));
    assert_eq!(
        requests[0]["transcriptSources"],
        requests[2]["transcriptSources"]
    );
    assert_eq!(
        requests[2]["window"]["requiredSourceIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(runner);
    assert_eq!(
        read_state(dir.path()).unwrap().entities[0].text,
        "Budget klären"
    );
}

#[test]
fn a_reshared_agenda_topic_starts_open_without_reusing_old_evidence() {
    let mut state = WorkspaceState::default();
    let context = WorkspaceContext {
        agenda: vec![Agenda {
            id: "e_topic".into(),
            revision: 1,
            text: "Freigabe".into(),
        }],
        ..Default::default()
    };
    state.apply_context(&context).unwrap();
    state.entities[0].status = "discussed".into();
    state.entities[0].human_locked = true;
    state.overview_refs.push("e_topic".into());
    state.entities[0].evidence.push(crate::Evidence {
        source_id: "s_previous".into(),
        quote: "Wir haben die Freigabe besprochen.".into(),
    });
    state.apply_context(&WorkspaceContext::default()).unwrap();
    assert_eq!(state.entities[0].status, "discussed");
    assert!(state.context_excluded_ids.contains(&"e_topic".to_owned()));
    assert!(state.overview_refs.is_empty());
    state.apply_context(&context).unwrap();
    assert!(!state.context_excluded_ids.contains(&"e_topic".to_owned()));
    assert_eq!(state.entities[0].status, "open");
    assert!(!state.entities[0].human_locked);
    assert!(state.entities[0].evidence.is_empty());
}

#[test]
fn human_status_change_is_a_durable_history_event_and_is_not_repeated() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let (writer, old) = store::load(dir.path()).unwrap();
    state.revision = 1;
    store::append(
        dir.path(),
        &store::Delta::between(
            &old,
            &state,
            req.transcript_sources.clone(),
            state.events.clone(),
        ),
    )
    .unwrap();
    let old = state.clone();
    let context = WorkspaceContext {
        revision: 1,
        overrides: vec![Override {
            id: state.entities[0].id.clone(),
            expected_revision: 1,
            status: "completed".into(),
            saved: false,
        }],
        ..Default::default()
    };
    state.apply_context(&context).unwrap();
    assert_eq!(state.events.len(), old.events.len() + 1);
    assert_eq!(state.events.last().unwrap().status, "completed");
    state.apply_context(&context).unwrap();
    assert_eq!(state.events.len(), old.events.len() + 1);
    state.revision = 2;
    store::append(
        dir.path(),
        &store::Delta::between(&old, &state, vec![], vec![]),
    )
    .unwrap();
    drop(writer);
    let (_, reopened) = store::load(dir.path()).unwrap();
    assert_eq!(reopened.events.len(), state.events.len());
}

#[test]
fn context_only_topic_can_be_withdrawn_and_reshared_without_inference() {
    let dir = tempfile::tempdir().unwrap();
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    let input = WorkspaceContext {
        agenda: vec![Agenda {
            id: "e_topic".into(),
            revision: 1,
            text: "Budget".into(),
        }],
        ..Default::default()
    };
    let (mut context, _) = update_context(dir.path(), &input).unwrap();
    context.overrides.push(Override {
        id: "e_topic".into(),
        expected_revision: 1,
        status: "discussed".into(),
        saved: false,
    });
    let (mut context, confirmed) = update_context(dir.path(), &context).unwrap();
    let agenda = context.agenda.clone();
    context.agenda.clear();
    let (mut context, removed) = update_context(dir.path(), &context).unwrap();
    assert_eq!(removed.entities[0].status, "discussed");
    assert!(removed.context_excluded_ids.contains(&"e_topic".into()));
    assert_eq!(removed.events.len(), confirmed.events.len());
    drop(runner);
    assert_eq!(read_state(dir.path()).unwrap().entities, removed.entities);
    context.agenda = agenda;
    let (_, reshared) = update_context(dir.path(), &context).unwrap();
    assert_eq!(reshared.entities[0].status, "open");
    assert!(!reshared.entities[0].human_locked);
    assert_eq!(reshared.events.len(), 1);
    assert!(reshared.context_excluded_ids.is_empty());
    let mut reopened = WorkspaceRunner::open(dir.path()).unwrap();
    let client = TextClient::new(&crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: "unreachable".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    reopened
        .update(&client, &[], &AnalysisControl::default(), "en")
        .unwrap();
    assert_eq!(reopened.view().entities, reshared.entities);
    assert_eq!(reopened.view().events.len(), 1);
}

#[test]
fn a_human_only_append_during_inference_rebases_without_losing_model_publication() {
    for stale_target in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner.set_context(Default::default()).unwrap();
        let expected = runner.context.lock().unwrap().clone();
        let (mut initial, req, answer) = setup();
        validate::apply(&mut initial, &req, &answer.to_string()).unwrap();
        initial.context = expected.clone();
        runner
            .commit(
                initial,
                req.transcript_sources.clone(),
                vec![],
                &AnalysisControl::default(),
                &expected,
            )
            .unwrap();
        let mut model = runner.state.clone();
        // A concurrently changed target remains a conflict, never a guessed merge.
        if stale_target {
            model.entities[0].revision += 1;
        }
        let mut human = expected.clone();
        human.overrides.push(Override {
            id: model.entities[0].id.clone(),
            expected_revision: 1,
            status: "discussed".into(),
            saved: false,
        });
        let (durable, _) = update_context(dir.path(), &human).unwrap();
        *runner.context.lock().unwrap() = durable;
        let result = runner.commit(
            model,
            vec![],
            vec![],
            &AnalysisControl::default(),
            &expected,
        );
        assert_eq!(result.is_err(), stale_target, "{result:?}");
        assert_eq!(runner.state.revision, if stale_target { 1 } else { 2 });
        drop(runner);
        let saved = read_state(dir.path()).unwrap();
        assert_eq!(saved.entities[0].status, "discussed");
        assert!(saved.entities[0].human_locked);
        assert_eq!(saved.events.iter().filter(|event| event.human).count(), 1);
    }
}
