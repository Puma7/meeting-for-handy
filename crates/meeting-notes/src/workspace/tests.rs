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
    let (mut state, mut req, answer) = setup();
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
    assert_eq!(state.entities[0].due.as_deref(), Some("morgen"));
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
