use super::*;
use serde_json::json;

fn setup() -> (WorkspaceState, Request, serde_json::Value) {
    let source = Source {
        id: "s_one".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: "Ich gebe den Entwurf morgen frei.".into(),
    };
    let state = WorkspaceState::default();
    let req = Request::new(
        "r_test".into(),
        &WorkspaceContext::default(),
        &state,
        vec![],
        std::slice::from_ref(&source),
        vec![source.clone()],
    );
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_test","changes":[{"proposalId":"p_one","kind":"task","targetId":null,"targetRevision":null,"text":"Entwurf freigeben","status":"open","attention":"current","owner":null,"due":"morgen","evidence":[{"sourceId":"s_one","quote":"Ich gebe den Entwurf morgen frei."}],"relatedEntityIds":[]}],"overviewRefs":["p_one"],"unresolvedContextIds":[],"moreChangesPossible":false});
    (state, req, answer)
}
#[test]
fn promised_task_is_open_and_repeated_result_is_deduplicated() {
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].status, "open");
    let id = state.entities[0].id.clone();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.entities.len(), 1);
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.entities[0].id, id);
}
#[test]
fn absent_impulses_survive_later_empty_update() {
    let (mut state, mut req, mut answer) = setup();
    req.purpose = "reconcile".into();
    answer["changes"][0]["kind"] = json!("impulse");
    answer["changes"][0]["text"] = json!("Wer gibt den Entwurf frei?");
    answer["changes"][0]["due"] = json!(null);
    answer["overviewRefs"] = json!([]);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let id = state.entities[0].id.clone();
    answer["changes"] = json!([]);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].id, id);
    assert_eq!(state.entities[0].status, "open");
}
#[test]
fn invalid_second_change_rolls_back_the_whole_bundle() {
    let (mut state, req, mut answer) = setup();
    let mut bad = answer["changes"][0].clone();
    bad["proposalId"] = json!("p_two");
    bad["evidence"][0]["quote"] = json!("Ein frei erfundenes Originalzitat.");
    answer["changes"].as_array_mut().unwrap().push(bad);
    assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
    assert!(state.entities.is_empty());
    assert!(state.events.is_empty());
}
#[test]
fn incomplete_and_foreign_answers_do_not_advance_state() {
    for key in ["moreChangesPossible", "requestId"] {
        let (mut state, req, mut answer) = setup();
        answer[key] = if key == "requestId" {
            json!("r_other")
        } else {
            json!(true)
        };
        assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
        assert_eq!(state.revision, 0);
    }
}
#[test]
fn promise_does_not_complete_existing_task() {
    let (mut state, mut req, mut answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    req.state = state.entities.clone();
    answer["changes"][0]["targetId"] = json!(state.entities[0].id);
    answer["changes"][0]["targetRevision"] = json!(1);
    answer["changes"][0]["status"] = json!("completed");
    answer["changes"][0]["text"] = json!(null);
    assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
    assert_eq!(state.entities[0].status, "open");
}
#[test]
fn manual_lock_prevents_model_override() {
    let (mut state, mut req, mut answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    state.entities[0].human_locked = true;
    req.state = state.entities.clone();
    answer["changes"][0]["targetId"] = json!(state.entities[0].id);
    answer["changes"][0]["targetRevision"] = json!(1);
    assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
    assert!(state.entities[0].human_locked);
}
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
fn duplicate_json_keys_are_rejected() {
    let (mut state, req, answer) = setup();
    let raw = answer.to_string().replacen(
        "\"requestId\":\"r_test\"",
        "\"requestId\":\"r_test\",\"requestId\":\"r_test\"",
        1,
    );
    assert!(validate::apply(&mut state, &req, &raw).is_err());
}
#[test]
fn durable_deltas_reopen_with_same_ids_and_no_text_duplication() {
    let dir = tempfile::tempdir().unwrap();
    let (lock, old) = store::load(dir.path()).unwrap();
    let (mut next, req, answer) = setup();
    let events = validate::apply(&mut next, &req, &answer.to_string()).unwrap();
    next.revision = 1;
    let d = store::Delta::between(&old, &next, req.transcript_sources.clone(), events);
    store::append(dir.path(), &d).unwrap();
    drop(lock);
    let (_, loaded) = store::load(dir.path()).unwrap();
    assert_eq!(loaded.entities, next.entities);
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.originals.len(), 1);
    assert!(store::append(dir.path(), &d).is_err());
}
#[test]
fn nonbinding_source_is_not_a_task() {
    let (mut state, mut req, mut answer) = setup();
    let quote = "Vielleicht könnten wir den Bericht am Freitag verschicken.";
    req.transcript_sources[0].text = quote.into();
    answer["changes"][0]["text"] = json!("Bericht verschicken");
    answer["changes"][0]["due"] = json!(null);
    answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
    assert!(validate::apply(&mut state, &req, &answer.to_string()).is_err());
    assert!(state.entities.is_empty());
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
fn a_revoked_and_restored_shared_input_still_rejects_inflight_inference() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner
        .set_context(WorkspaceContext {
            shared_notes: vec![SharedNote {
                id: "h_note".into(),
                revision: 1,
                text: "Released text".into(),
            }],
            ..Default::default()
        })
        .unwrap();
    let expected = runner.context.lock().unwrap().clone();
    let mut revoked = expected.clone();
    revoked.shared_notes.clear();
    let (mut restored, _) = update_context(dir.path(), &revoked).unwrap();
    restored.shared_notes = expected.shared_notes.clone();
    let (current, _) = update_context(dir.path(), &restored).unwrap();
    *runner.context.lock().unwrap() = current.clone();
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
    assert_eq!(read_state(dir.path()).unwrap().context, current);
}

#[test]
fn corrupt_context_does_not_get_overwritten_and_private_fields_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let _runner = WorkspaceRunner::open(dir.path()).unwrap();
    let path = dir.path().join("context-00000001.json");
    std::fs::write(&path, b"broken").unwrap();
    assert!(update_context(dir.path(), &WorkspaceContext::default()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"broken");
    assert!(serde_json::from_str::<WorkspaceContext>(
        r#"{"revision":0,"agenda":[],"sharedNotes":[],"overrides":[],"audioPath":"secret"}"#
    )
    .is_err());
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
fn a_status_update_retains_literal_owner_and_due_evidence() {
    let (mut state, mut req, mut answer) = setup();
    answer["changes"][0]["due"] = json!("Morgen");
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    req.state = state.entities.clone();
    req.transcript_sources.push(Source {
        id: "s_done".into(),
        source: "system".into(),
        start_ms: 3000,
        end_ms: 6000,
        text: "Der Entwurf ist erledigt.".into(),
    });
    let mut changed = answer;
    changed["changes"][0]["targetId"] = json!(state.entities[0].id);
    changed["changes"][0]["targetRevision"] = json!(1);
    changed["changes"][0]["text"] = json!(null);
    changed["changes"][0]["status"] = json!("completed");
    changed["changes"][0]["due"] = json!(null);
    changed["changes"][0]["evidence"] =
        json!([{"sourceId":"s_done","quote":"Der Entwurf ist erledigt."}]);
    validate::apply(&mut state, &req, &changed.to_string()).unwrap();
    assert_eq!(state.entities[0].due.as_deref(), Some("Morgen"));
    assert_eq!(state.entities[0].evidence.len(), 2);
}

#[test]
fn recognition_reversion_and_shortening_are_new_pending_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    let mut text = LiveText {
        key: "same-part".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 30000,
        text: "Ursprüngliche Formulierung.".into(),
    };
    let original = runner.pending(&[text.clone()]);
    runner.state.processed.extend(original.clone());
    text.text = "Korrigierte Formulierung.".into();
    let revised = runner.pending(&[text.clone()]);
    runner.state.processed.extend(revised);
    text.text = "Ursprüngliche Formulierung.".into();
    let reverted = runner.pending(&[text.clone()]);
    assert_eq!(reverted.len(), 1);
    assert_ne!(reverted[0].id, original[0].id);
    runner.state.processed.extend(reverted);
    let mut tail = original[0].clone();
    tail.chunk = 2;
    tail.id = "s_tail".into();
    tail.text = "Später entfernter Absatz.".into();
    runner.state.processed.push(tail);
    assert_eq!(
        runner.pending(&[text]).len(),
        1,
        "A removed trailing chunk requires invalidation"
    );
}

#[test]
fn removed_text_withdraws_active_authority_but_retains_originals_and_manual_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let text = LiveText {
        key: "part".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: req.transcript_sources[0].text.clone(),
    };
    let processed = live::pending(&[], &[text]);
    let id = source_id(&processed[0]);
    state.entities[0].evidence[0].source_id = id.clone();
    state.originals = vec![Source {
        id: id.clone(),
        ..req.transcript_sources[0].clone()
    }];
    state.processed = processed;
    state.context = runner.context.lock().unwrap().clone();
    runner.state = state;
    assert!(runner.pending(&[]).is_empty());
    assert!(runner.context_dirty());
    let client = TextClient::new(&crate::Connection {
        endpoint: "http://127.0.0.1:1235/v1/".into(),
        model: "unused-no-inference".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    runner
        .update(&client, &[], &AnalysisControl::default(), "de")
        .unwrap();
    assert_eq!(runner.state.entities.len(), 1);
    assert_eq!(runner.state.events.len(), 1);
    assert_eq!(runner.state.originals.len(), 1);
    assert!(runner.state.superseded_sources.contains(&id));
    assert!(runner
        .state
        .context_excluded_ids
        .contains(&runner.state.entities[0].id));
    assert!(!runner.context_dirty());
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
fn missing_pledge_is_not_a_task_and_a_wrong_phase_cannot_publish_it() {
    let (mut state, mut req, mut answer) = setup();
    req.transcript_sources[0].text = "Zum Budget fehlt noch eine Zusage.".into();
    answer["changes"][0]["text"] = json!("Budgetzusage einholen");
    answer["changes"][0]["due"] = json!(null);
    answer["changes"][0]["evidence"][0]["quote"] = json!(req.transcript_sources[0].text);
    assert!(
        validate::apply(&mut state, &req, &answer.to_string()).is_err(),
        "A missing pledge was published as an assigned task"
    );
    assert!(state.entities.is_empty());
}
#[test]
fn reconcile_cannot_smuggle_a_task_into_the_impulse_phase() {
    let (mut state, mut req, answer) = setup();
    req.purpose = "reconcile".into();
    assert!(
        validate::apply(&mut state, &req, &answer.to_string()).is_err(),
        "Wrong phase published an otherwise valid task"
    );
    assert!(state.entities.is_empty());
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

#[test]
#[cfg(unix)]
fn linked_workspace_does_not_open_or_write_into_another_directory() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original");
    std::fs::create_dir(&original).unwrap();
    let link = dir.path().join("workspace-v9");
    std::os::unix::fs::symlink(&original, &link).unwrap();
    assert!(read_state(&link).is_err());
    assert!(WorkspaceRunner::open(&link).is_err());
    assert!(!original.join("writer.lock").exists());
}

#[test]
fn explicit_english_promises_pass_the_task_quote_gate() {
    for quote in [
        "I promise to email the report tomorrow.",
        "We commit to sending the report tomorrow.",
    ] {
        assert!(quote_options(quote)
            .iter()
            .any(|quote| validate::task_quote_allowed(quote)));
    }
    for quote in [
        "I do not promise to email the report tomorrow.",
        "I promise to send the report if the budget is approved.",
        "Do I promise to email the report tomorrow?",
    ] {
        assert!(!quote_options(quote)
            .iter()
            .any(|quote| validate::task_quote_allowed(quote)));
    }
}

#[test]
fn latest_display_compares_workspace_activity_across_run_directories() {
    let root = tempfile::tempdir().unwrap();
    let connection = |model: &str| crate::Connection {
        endpoint: "http://127.0.0.1:1235/v1/".into(),
        model: model.into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let first = crate::live::LiveRunner::start(root.path(), "meeting", connection("first"), "de")
        .unwrap()
        .with_workspace(WorkspaceContext::default())
        .unwrap();
    let folder = first.workspace_folder().unwrap();
    drop(first);
    let second =
        crate::live::LiveRunner::start(root.path(), "meeting", connection("second"), "de").unwrap();
    assert_eq!(
        crate::live::read_latest(root.path(), "meeting")
            .unwrap()
            .unwrap()
            .model,
        "second"
    );
    let original = store::read(&folder).unwrap();
    let mut updated = original.clone();
    updated.revision += 1;
    updated.updated_at = Some((chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339());
    store::append(
        &folder,
        &store::Delta::between(&original, &updated, vec![], vec![]),
    )
    .unwrap();
    let selected = crate::live::read_latest(root.path(), "meeting")
        .unwrap()
        .unwrap();
    assert_eq!(selected.model, "first");
    assert_eq!(selected.updated_at, updated.updated_at);
    drop(second);
}

#[test]
fn completion_accepts_explicit_reports_but_rejects_uncertainty() {
    for quote in [
        "Ich habe den Bericht verschickt.",
        "I sent the report yesterday.",
        "We have submitted the report.",
        "Die Freigabe ist erteilt.",
    ] {
        assert!(validate::completion_quote_allowed(quote), "{quote}");
    }
    for quote in [
        "Ich habe den Bericht nicht verschickt.",
        "Ich werde den Bericht verschicken.",
        "If I sent the report, check the inbox.",
        "I haven't sent the report.",
        "I might have sent the report.",
        "Is it done?",
        "Ich habe den Bericht. Danach: Thema verschickt laut Notiz.",
    ] {
        assert!(!validate::completion_quote_allowed(quote), "{quote}");
    }
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
fn later_chunk_reversion_is_a_fresh_source_and_old_ids_survive() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    let first = "a".repeat(1600);
    let mut text = LiveText {
        key: "multi-part".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 30000,
        text: format!("{first}Original second chunk."),
    };
    let old = runner.pending(&[text.clone()]);
    runner.state.processed.extend(old.clone());
    text.text = format!("{first}Corrected second chunk.");
    runner
        .state
        .processed
        .extend(runner.pending(&[text.clone()]));
    text.text = format!("{first}Original second chunk.");
    let reverted = runner.pending(&[text]);
    assert_eq!(reverted.len(), 1);
    assert_eq!(reverted[0].chunk, 2);
    assert_eq!(reverted[0].text, old[1].text);
    assert_ne!(reverted[0].id, old[1].id);
    assert!(runner
        .state
        .processed
        .iter()
        .any(|source| source.id == old[1].id));
}

#[test]
fn an_explicit_empty_fact_overview_clears_old_selection() {
    let (mut state, mut req, mut answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.overview_refs.len(), 1);
    req.purpose = "facts".into();
    answer["changes"] = json!([]);
    answer["overviewRefs"] = json!([]);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert!(state.overview_refs.is_empty());
    assert_eq!(state.entities.len(), 1);
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
#[test]
fn very_short_sources_are_saved_without_an_invalid_provider_schema() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let texts = vec![LiveText {
        key: "short".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 1000,
        text: "Okay.".into(),
    }];
    let pending = runner.pending(&texts);
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
    assert!(runner.pending(&texts).is_empty());
    assert!(runner.state.entities.is_empty());
    assert_eq!(runner.state.originals[0].text, "Okay.");
}

#[test]
fn exactly_twenty_changes_fit_the_published_contract() {
    let (mut state, mut req, mut answer) = setup();
    req.purpose = "facts".into();
    let change = answer["changes"][0].clone();
    answer["overviewRefs"] = json!([]);
    answer["changes"] = json!((0..20)
        .map(|i| {
            let mut item = change.clone();
            item["proposalId"] = json!(format!("p_{i}"));
            item["kind"] = json!("fact");
            item["status"] = json!("recorded");
            item["text"] = json!(format!("Recorded fact {}", char::from(b'a' + i)));
            item["due"] = json!(null);
            item
        })
        .collect::<Vec<_>>());
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.entities.len(), 20);
}

#[test]
fn provider_context_rejection_shrinks_the_existing_frontier_before_sources() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut sizes = Vec::new();
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            let header = loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && bytes.len() < 500000);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(p) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    break p + 4;
                }
            };
            let size = String::from_utf8_lossy(&bytes[..header])
                .lines()
                .filter_map(|v| v.split_once(':'))
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
            let req: serde_json::Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            if req["purpose"] == "facts" {
                let variants = body["response_format"]["json_schema"]["schema"]["properties"]
                    ["changes"]["items"]["anyOf"]
                    .as_array()
                    .unwrap();
                for branch in variants {
                    let kind = branch["properties"]["kind"]["enum"][0].as_str().unwrap();
                    let expected = if kind == "decision" {
                        json!(["recorded"])
                    } else {
                        json!(["recorded", "unclear"])
                    };
                    assert_eq!(
                        branch["properties"]["status"]["enum"], expected,
                        "new {kind} must match validator statuses"
                    );
                }
                assert_eq!(variants.len(), 2); // Only new facts/decisions; the seed tasks are not valid targets.
                assert!(variants
                    .iter()
                    .all(|branch| branch["properties"]["targetId"]["type"] == "null"));
            }
            let count = req["state"].as_array().unwrap().len();
            assert_eq!(
                req["window"]["requiredSourceIds"].as_array().unwrap().len(),
                1
            );
            sizes.push(count);
            if count > 2 {
                let payload = "{\"error\":{\"message\":\"context length exceeded\"}}";
                write!(stream,"HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
            } else {
                let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":[],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                if req["purpose"] == "reconcile" {
                    break;
                }
            }
            assert!(sizes.len() < 20);
        }
        sizes
    });
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let entity = state.entities[0].clone();
    state.entities = (0..6)
        .map(|i| {
            let mut e = entity.clone();
            e.id = format!("e_old_{i}");
            e
        })
        .collect();
    state.originals = req.transcript_sources;
    runner.state = state;
    let client = TextClient::new(&crate::Connection {
        endpoint,
        model: "synthetic".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    let pending = runner.pending(&[LiveText {
        key: "fresh".into(),
        track: "system".into(),
        start_ms: 3000,
        end_ms: 6000,
        text: "Ich schreibe den Bericht morgen.".into(),
    }]);
    assert_eq!(
        runner
            .update(&client, &pending, &AnalysisControl::default(), "en")
            .unwrap(),
        1
    );
    let sizes = server.join().unwrap();
    assert!(sizes.starts_with(&[6, 3, 1, 2, 3, 1, 2]), "{sizes:?}");
    assert_eq!(runner.state.processed.len(), 1);
    assert_eq!(runner.state.entities.len(), 6);
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
fn acknowledged_open_questions_get_a_bounded_request_and_ignored_constraints_do_not_loop() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    for omit in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut calls = Vec::new();
            loop {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 8192];
                let header = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && bytes.len() < 500000);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(p) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break p + 4;
                    }
                };
                let size = String::from_utf8_lossy(&bytes[..header])
                    .lines()
                    .filter_map(|v| v.split_once(':'))
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
                let req: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                let schema = &body["response_format"]["json_schema"]["schema"];
                let forced = schema["properties"]["changes"]["minItems"] == 1;
                let phase = req["purpose"].as_str().unwrap();
                calls.push((phase.to_owned(), forced));
                assert!(calls.len() <= 3);
                let mut changes = Vec::new();
                if phase == "facts" || (forced && !omit) {
                    changes.push(json!({"proposalId":"p_new","kind":if forced {"impulse"} else {"fact"},"targetId":null,"targetRevision":null,"text":if forced {"Who can approve the budget?"} else {"Budget commitment is missing."},"status":if forced {"open"} else {"unclear"},"attention":"clarification","owner":null,"due":null,"evidence":[{"sourceId":req["transcriptSources"][0]["id"],"quote":"There is no commitment on the budget."}],"relatedEntityIds":[]}));
                } else if !forced {
                    changes.push(json!({"proposalId":"p_agenda","kind":"agenda","targetId":"e_budget","targetRevision":1,"text":null,"status":"touched","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":req["transcriptSources"][0]["id"],"quote":"There is no commitment on the budget."}],"relatedEntityIds":[]}));
                }
                if forced {
                    assert_eq!(
                        schema["properties"]["changes"]["items"]["anyOf"]
                            .as_array()
                            .unwrap()
                            .len(),
                        1
                    );
                    assert_eq!(
                        schema["properties"]["changes"]["items"]["anyOf"][0]["properties"]["kind"]
                            ["enum"],
                        json!(["impulse"])
                    );
                }
                let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                if phase == "reconcile" && (omit || !forced) {
                    break;
                }
            }
            calls
        });
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner
            .set_context(WorkspaceContext {
                agenda: vec![Agenda {
                    id: "e_budget".into(),
                    revision: 1,
                    text: "Clarify budget".into(),
                }],
                ..Default::default()
            })
            .unwrap();
        let client = TextClient::new(&crate::Connection {
            endpoint,
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        })
        .unwrap();
        let texts = [LiveText {
            key: "question".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: "Who can approve the budget? There is no commitment on the budget.".into(),
        }];
        let pending = runner.pending(&texts);
        let result = runner.update(&client, &pending, &AnalysisControl::default(), "en");
        let calls = server.join().unwrap();
        if omit {
            assert!(result.is_err());
            assert!(runner.state.processed.is_empty());
            assert_eq!(runner.state.revision, 0);
            assert_eq!(calls.len(), 2);
        } else {
            assert_eq!(result.unwrap(), 1);
            assert_eq!(
                calls,
                vec![
                    ("facts".into(), false),
                    ("reconcile".into(), true),
                    ("reconcile".into(), false)
                ]
            );
            assert!(runner.state.entities.iter().any(|e| e.kind == "impulse"));
            assert_eq!(
                runner
                    .state
                    .entities
                    .iter()
                    .find(|e| e.id == "e_budget")
                    .unwrap()
                    .status,
                "touched"
            );
        }
    }
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
        endpoint: "http://127.0.0.1:1235/v1/".into(),
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
fn legacy_context_times_remain_readable_but_malformed_times_are_preserved_as_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    drop(runner);
    let path = dir.path().join("context-00000001.json");
    let mut entry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    entry.as_object_mut().unwrap().remove("createdAt");
    std::fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();
    let old = read_state(dir.path()).unwrap();
    assert!(old.activity_at().is_none());
    assert!(serde_json::to_value(&old)
        .unwrap()
        .get("contextUpdatedAt")
        .is_none());
    entry["createdAt"] = json!("not a timestamp");
    let bytes = serde_json::to_vec(&entry).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(read_state(dir.path()).is_err());
    assert!(update_context(dir.path(), &old.context).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
