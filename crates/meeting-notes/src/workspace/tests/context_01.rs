use super::*;

#[test]
fn terminal_tasks_survive_context_replay_without_duplicate_open_obligations() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    let mut failures = Vec::new();
    for terminal in ["completed", "withdrawn"] {
        for (human, revoke_note) in [(false, false), (true, false), (false, true), (true, true)]
            .into_iter()
            .filter(|(human, _)| !(*human && terminal == "withdrawn"))
        {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let server_stop = stop.clone();
            let server = thread::spawn(move || {
                let mut requests = Vec::new();
                let deadline = Instant::now() + Duration::from_secs(10);
                while !server_stop.load(Ordering::SeqCst) {
                    assert!(Instant::now() < deadline);
                    let mut stream = match accept_synthetic_before(
                        &listener,
                        Instant::now() + Duration::from_millis(100),
                    ) {
                        Ok(s) => s,
                        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                        Err(e) => panic!("{e}"),
                    };
                    let mut bytes = Vec::new();
                    let mut buffer = [0; 8192];
                    let header = loop {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0 && bytes.len() < 500000);
                        bytes.extend_from_slice(&buffer[..n]);
                        if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
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
                    assert!(size < 500000);
                    while bytes.len() < header + size {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                    let body: serde_json::Value =
                        serde_json::from_slice(&bytes[header..header + size]).unwrap();
                    let req: serde_json::Value =
                        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap())
                            .unwrap();
                    let mut changes = vec![];
                    if req["purpose"] == "tasks" {
                        let required = req["window"]["requiredSourceIds"].as_array().unwrap();
                        let sources = req["transcriptSources"].as_array().unwrap();
                        if let Some(promise) = sources.iter().find(|s| {
                            required.contains(&s["id"])
                                && s["text"]
                                    .as_str()
                                    .unwrap()
                                    .starts_with("I will send the report tomorrow.")
                        }) {
                            // Deliberately repeat the old proposal even when terminal context is supplied.
                            changes.push(json!({"proposalId":"p_promise","kind":"task","targetId":null,"targetRevision":null,"text":"Send the report","status":"open","attention":"current","owner":null,"due":if revoke_note || req["humanContext"].as_array().unwrap().is_empty() { json!("tomorrow") } else { json!(null) },"evidence":[{"sourceId":promise["id"],"quote":"I will send the report tomorrow."}],"relatedEntityIds":[]}));
                        }
                        if let Some(done) = sources.iter().find(|s| {
                            required.contains(&s["id"])
                                && s["text"].as_str().unwrap().starts_with(
                                    if terminal == "completed" {
                                        "I have sent the report."
                                    } else {
                                        "I withdraw the report task."
                                    },
                                )
                        }) {
                            for entity in req["state"].as_array().unwrap().iter().filter(|e| {
                                e["kind"] == "task"
                                    && e["status"] == "open"
                                    && e["humanLocked"] == false
                            }) {
                                changes.push(json!({"proposalId":format!("p_done_{}",changes.len()),"kind":"task","targetId":entity["id"],"targetRevision":entity["revision"],"text":null,"status":terminal,"attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":done["id"],"quote":if terminal=="completed" {"I have sent the report."} else {"I withdraw the report task."}}],"relatedEntityIds":[]}));
                            }
                        }
                    }
                    let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                    requests.push((
                        req,
                        body["response_format"]["json_schema"]["schema"].clone(),
                    ));
                    let payload = format!(
                        "data: {}\n\ndata: {}\n\n",
                        json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                        json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                    );
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                }
                requests
            });
            let dir = tempfile::tempdir().unwrap();
            let client = TextClient::new(&crate::Connection {
                endpoint,
                model: "synthetic-terminal-replay".into(),
                mode: "local".into(),
                cloud_consent: false,
                api_key: String::new(),
            })
            .unwrap();
            let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
            let mut initial = WorkspaceContext::default();
            if revoke_note {
                initial.shared_notes.push(SharedNote {
                    id: "h_terminal".into(),
                    revision: 1,
                    text: "Review the retained conversation.".into(),
                });
            }
            runner.set_context(initial).unwrap();
            let padded = |start: &str| format!("{start} {}", "z".repeat(1599 - start.len()));
            let mut texts = (0..8)
                .map(|i| LiveText {
                    key: format!("part-{i}"),
                    track: "system".into(),
                    start_ms: i * 30000,
                    end_ms: (i + 1) * 30000,
                    text: padded(if i == 0 {
                        "I will send the report tomorrow."
                    } else if i == 7 {
                        if terminal == "completed" {
                            "I have sent the report."
                        } else {
                            "I withdraw the report task."
                        }
                    } else {
                        "Synthetic neutral context."
                    }),
                })
                .collect::<Vec<_>>();
            for _ in 0..2 {
                let pending = runner.pending(&texts);
                assert_eq!(
                    runner
                        .update(&client, &pending, &AnalysisControl::default(), "en")
                        .unwrap(),
                    4
                );
            }
            assert_eq!(runner.state.entities.len(), 1);
            assert_eq!(runner.state.entities[0].status, terminal);
            assert_eq!(runner.state.entities[0].evidence.len(), 2);
            let original_id = runner.state.entities[0].id.clone();
            let promise_id = source_id(&live::pending(&[], &texts)[0]);
            let old_files = std::fs::read_dir(dir.path())
                .unwrap()
                .map(Result::unwrap)
                .filter(|e| e.file_name().to_string_lossy().starts_with("revision-"))
                .map(|e| (e.path(), std::fs::read(e.path()).unwrap()))
                .collect::<Vec<_>>();
            let mut context = runner.context.lock().unwrap().clone();
            if human {
                context.overrides.push(Override {
                    id: original_id.clone(),
                    expected_revision: runner.state.entities[0].revision,
                    status: terminal.into(),
                    saved: true,
                });
            }
            if revoke_note {
                context.shared_notes.clear();
            } else {
                context.shared_notes.push(SharedNote {
                    id: "h_terminal".into(),
                    revision: 1,
                    text: "Review the retained conversation.".into(),
                });
            }
            let (context, _) = update_context(dir.path(), &context).unwrap();
            *runner.context.lock().unwrap() = context;
            drop(runner);
            runner = WorkspaceRunner::open(dir.path()).unwrap();
            let confirmed_terminal = read_state(dir.path())
                .unwrap()
                .entities
                .iter()
                .find(|e| e.id == original_id)
                .unwrap()
                .clone();
            for _ in 0..2 {
                assert!(runner.pending(&texts).is_empty());
                runner
                    .update(&client, &[], &AnalysisControl::default(), "en")
                    .unwrap();
                if runner
                    .state
                    .entities
                    .iter()
                    .any(|e| e.kind == "task" && e.status == "open")
                {
                    failures.push(format!(
                        "{terminal} human={human}: replay published an open duplicate"
                    ));
                }
                drop(runner);
                runner = WorkspaceRunner::open(dir.path()).unwrap();
            }
            assert!(!runner.context_dirty());
            if runner
                .state
                .entities
                .iter()
                .filter(|e| e.kind == "task")
                .count()
                != 1
            {
                failures.push(format!(
                    "{terminal} human={human}: replay retained duplicate identities"
                ));
            }
            assert_eq!(
                runner
                    .state
                    .entities
                    .iter()
                    .find(|e| e.id == original_id)
                    .unwrap()
                    .status,
                terminal
            );
            assert_eq!(
                runner
                    .state
                    .entities
                    .iter()
                    .find(|e| e.id == original_id)
                    .unwrap(),
                &confirmed_terminal
            );
            assert_eq!(confirmed_terminal.due.as_deref(), Some("tomorrow"));
            let source = runner
                .state
                .originals
                .iter()
                .find(|s| s.id == promise_id)
                .unwrap()
                .clone();
            let mut req = Request::new(
                "r_terminal".into(),
                &runner.state.context,
                &runner.state,
                vec![confirmed_terminal.clone()],
                std::slice::from_ref(&source),
                runner.state.originals.clone(),
            );
            req.purpose = "tasks".into();
            let bad = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_terminal","changes":[{"proposalId":"p_reopen","kind":"task","targetId":original_id,"targetRevision":confirmed_terminal.revision,"text":null,"status":"open","attention":"current","owner":null,"due":null,"evidence":[{"sourceId":promise_id,"quote":"I will send the report tomorrow."}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
            let mut unchanged = runner.state.clone();
            assert!(validate::apply(&mut unchanged, &req, &bad.to_string()).is_err());
            assert_eq!(
                serde_json::to_vec(&unchanged).unwrap(),
                serde_json::to_vec(&runner.state).unwrap()
            );
            if revoke_note && !human {
                let terminal_source = confirmed_terminal
                    .evidence
                    .iter()
                    .find(|e| {
                        e.quote == "I have sent the report."
                            || e.quote == "I withdraw the report task."
                    })
                    .unwrap()
                    .source_id
                    .clone();
                // A stale terminal result must not suppress work after its
                // transcript proof was replaced, removed or superseded.
                for invalidation in ["superseded", "missing", "changed"] {
                    let mut invalid = runner.state.clone();
                    match invalidation {
                        "superseded" => invalid.superseded_sources.push(terminal_source.clone()),
                        "missing" => invalid.originals.retain(|s| s.id != terminal_source),
                        _ => {
                            invalid
                                .originals
                                .iter_mut()
                                .find(|s| s.id == terminal_source)
                                .unwrap()
                                .text = "Replaced synthetic terminal source.".into()
                        }
                    }
                    let mut request = Request::new(
                        "r_terminal".into(),
                        &invalid.context,
                        &invalid,
                        vec![],
                        std::slice::from_ref(&source),
                        invalid.originals.clone(),
                    );
                    request.purpose = "tasks".into();
                    let mut proposal = bad.clone();
                    proposal["changes"][0]["targetId"] = json!(null);
                    proposal["changes"][0]["targetRevision"] = json!(null);
                    proposal["changes"][0]["text"] = json!("Send the report");
                    proposal["changes"][0]["due"] = json!("tomorrow");
                    let count = invalid.entities.len();
                    let events =
                        validate::apply(&mut invalid, &request, &proposal.to_string()).unwrap();
                    assert_eq!(events.len(), 1, "{terminal}: {invalidation}");
                    assert_eq!(invalid.entities.len(), count + 1);
                    assert_eq!(
                        invalid
                            .entities
                            .iter()
                            .find(|e| e.id == original_id)
                            .unwrap(),
                        &confirmed_terminal
                    );
                }
            }
            // An independently recorded later promise remains a legitimate new task.
            texts.push(LiveText {
                key: "new-promise".into(),
                track: "microphone".into(),
                start_ms: 240000,
                end_ms: 270000,
                text: "I will send the report tomorrow.".into(),
            });
            let pending = runner.pending(&texts);
            runner
                .update(&client, &pending, &AnalysisControl::default(), "en")
                .unwrap();
            assert_eq!(
                runner
                    .state
                    .entities
                    .iter()
                    .filter(|e| e.kind == "task" && e.status == "open")
                    .count(),
                1
            );
            let view = serde_json::to_vec(&runner.view()).unwrap();
            drop(runner);
            assert_eq!(
                serde_json::to_vec(&read_state(dir.path()).unwrap()).unwrap(),
                view
            );
            for (path, bytes) in old_files {
                assert_eq!(std::fs::read(path).unwrap(), bytes);
            }
            stop.store(true, Ordering::SeqCst);
            let requests = server.join().unwrap();
            let relevant = requests
                .iter()
                .find(|(r, _)| {
                    r["purpose"] == "tasks"
                        && (r["humanContext"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|h| h["id"] == "h_terminal")
                            != revoke_note)
                        && r["window"]["requiredSourceIds"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|id| id == &promise_id)
                })
                .unwrap();
            let terminal_in_request = relevant.0["state"].as_array().unwrap().iter().any(|e| {
                e["id"] == original_id
                    && e["status"] == terminal
                    && e["evidence"].as_array().unwrap().len() == 2
            });
            if revoke_note && !human {
                // Context withdrawal still hides the old AI draft. Deduplication
                // must use its private transcript witnesses without re-exposure.
                assert!(!terminal_in_request);
                assert!(!relevant
                    .0
                    .to_string()
                    .contains("Review the retained conversation."));
            } else if !terminal_in_request {
                failures.push(format!("{terminal} human={human}: terminal identity/evidence absent from actual tasks request"));
            }
            {
                let branches = relevant.1["properties"]["changes"]["items"]["anyOf"]
                    .as_array()
                    .unwrap();
                assert!(!branches.iter().any(|b| b["properties"]["targetId"]["enum"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(original_id)))));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Terminal replay failures: {failures:?}"
    );
}

#[test]
fn replay_rechecks_every_retained_source_after_removal_or_shortening_and_restart() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    let mut omissions = Vec::new();
    for shorten in [false, true] {
        for restart in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let reject_next = Arc::new(AtomicBool::new(false));
            let server_reject = reject_next.clone();
            let server_stop = stop.clone();
            let server = thread::spawn(move || {
                let mut requests = Vec::new();
                let end = Instant::now() + Duration::from_secs(10);
                while !server_stop.load(Ordering::SeqCst) {
                    assert!(
                        Instant::now() < end,
                        "synthetic replay server exceeded its bound"
                    );
                    let mut stream = match accept_synthetic_before(
                        &listener,
                        Instant::now() + Duration::from_millis(100),
                    ) {
                        Ok(stream) => stream,
                        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                        Err(e) => panic!("{e}"),
                    };
                    let mut bytes = Vec::new();
                    let mut buffer = [0; 8192];
                    let header = loop {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0 && bytes.len() < 500000);
                        bytes.extend_from_slice(&buffer[..n]);
                        if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                            break at + 4;
                        }
                    };
                    let size = String::from_utf8_lossy(&bytes[..header])
                        .lines()
                        .filter_map(|s| s.split_once(':'))
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .unwrap()
                        .1
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    assert!(size < 500000);
                    while bytes.len() < header + size {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                    let body: serde_json::Value =
                        serde_json::from_slice(&bytes[header..header + size]).unwrap();
                    let req: serde_json::Value =
                        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap())
                            .unwrap();
                    if server_reject.swap(false, Ordering::SeqCst) {
                        let error = "{\"error\":{\"message\":\"synthetic unavailable\"}}";
                        write!(stream,"HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{error}",error.len()).unwrap();
                        continue;
                    }
                    let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":[],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                    requests.push(req);
                    let payload = format!(
                        "data: {}\n\ndata: {}\n\n",
                        json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                        json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                    );
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                }
                requests
            });
            let directory = tempfile::tempdir().unwrap();
            let client = TextClient::new(&crate::Connection {
                endpoint,
                model: "synthetic-replay".into(),
                mode: "local".into(),
                cloud_consent: false,
                api_key: String::new(),
            })
            .unwrap();
            let mut runner = WorkspaceRunner::open(directory.path()).unwrap();
            runner.set_context(Default::default()).unwrap();
            let part = |i: usize| {
                let prefix = format!("Synthetic neutral source {i}. ");
                format!("{prefix}{}", "z".repeat(1600 - prefix.len()))
            };
            let mut texts = (0..if shorten { 7 } else { 8 })
                .map(|i| LiveText {
                    key: format!("part-{i}"),
                    track: "system".into(),
                    start_ms: i as u64 * 30000,
                    end_ms: (i as u64 + 1) * 30000,
                    text: if shorten && i == 0 {
                        format!("{}{}", part(0), part(8))
                    } else {
                        part(i)
                    },
                })
                .collect::<Vec<_>>();
            let originals = live::pending(&[], &texts);
            assert_eq!(originals.len(), 8);
            for _ in 0..2 {
                let pending = runner.pending(&texts);
                assert_eq!(
                    runner
                        .update(&client, &pending, &AnalysisControl::default(), "en")
                        .unwrap(),
                    4
                );
            }
            assert!(runner.pending(&texts).is_empty());
            assert!(!runner.context_dirty());
            let mut context = runner.context.lock().unwrap().clone();
            context.shared_notes.push(SharedNote {
                id: "h_replay".into(),
                revision: 1,
                text: "New synthetic shared context".into(),
            });
            context.agenda.push(Agenda {
                id: "e_replay".into(),
                revision: 1,
                text: "Review every retained source".into(),
            });
            let (context, _) = update_context(directory.path(), &context).unwrap();
            *runner.context.lock().unwrap() = context;
            runner
                .update(&client, &[], &AnalysisControl::default(), "en")
                .unwrap();
            assert_eq!(runner.state.replay_cursor, Some(4));
            assert!(runner.context_dirty());
            let retained_files = std::fs::read_dir(directory.path())
                .unwrap()
                .map(Result::unwrap)
                .filter(|e| e.file_name().to_string_lossy().starts_with("revision-"))
                .map(|e| (e.path(), std::fs::read(e.path()).unwrap()))
                .collect::<Vec<_>>();
            if restart {
                drop(runner);
                runner = WorkspaceRunner::open(directory.path()).unwrap();
                assert_eq!(runner.state.replay_cursor, Some(4));
            }
            if shorten {
                texts[0].text = part(0);
            } else {
                texts.remove(0);
            }
            if restart {
                let pending = runner.pending(&texts);
                reject_next.store(true, Ordering::SeqCst);
                assert!(runner
                    .update(&client, &pending, &AnalysisControl::default(), "en")
                    .unwrap_err()
                    .to_string()
                    .contains("500"));
                assert_eq!(runner.state.replay_cursor, Some(4));
                assert!(runner.context_dirty());
                for (path, bytes) in &retained_files {
                    assert_eq!(&std::fs::read(path).unwrap(), bytes);
                }
                drop(runner);
                runner = WorkspaceRunner::open(directory.path()).unwrap();
                assert_eq!(runner.state.replay_cursor, Some(4));
            }
            let latest = live::pending(&[], &texts);
            let retained = originals
                .iter()
                .filter(|old| {
                    latest.iter().any(|new| {
                        new.key == old.key && new.chunk == old.chunk && new.text == old.text
                    })
                })
                .map(source_id)
                .collect::<HashSet<_>>();
            for _ in 0..8 {
                let pending = runner.pending(&texts);
                if pending.is_empty() && !runner.context_dirty() {
                    break;
                }
                runner
                    .update(&client, &pending, &AnalysisControl::default(), "en")
                    .unwrap();
                if restart {
                    drop(runner);
                    runner = WorkspaceRunner::open(directory.path()).unwrap();
                }
            }
            assert!(runner.pending(&texts).is_empty());
            assert!(!runner.context_dirty());
            assert!(originals.iter().all(|s| runner
                .state
                .originals
                .iter()
                .any(|o| o.id == source_id(s))));
            for (path, bytes) in retained_files {
                assert_eq!(std::fs::read(path).unwrap(), bytes);
            }
            let final_bytes = serde_json::to_vec(&runner.view()).unwrap();
            drop(runner);
            assert_eq!(
                serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap(),
                final_bytes
            );
            stop.store(true, Ordering::SeqCst);
            let requests = server.join().unwrap();
            let seen = requests
                .iter()
                .filter(|r| {
                    r["purpose"] == "reconcile"
                        && r["humanContext"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|h| h["id"] == "h_replay")
                })
                .flat_map(|r| {
                    r["window"]["requiredSourceIds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_owned())
                })
                .collect::<HashSet<_>>();
            let missing = retained.difference(&seen).cloned().collect::<Vec<_>>();
            if !missing.is_empty() {
                omissions.push(format!("shorten={shorten} restart={restart}: context_dirty=false but retained sources never reconciled: {missing:?}"));
            }
        }
    }
    assert!(omissions.is_empty(), "Replay omissions: {omissions:?}");
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for _ in 0..4 {
            let mut stream = accept_synthetic_before(&listener, deadline).unwrap();
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

#[test]
fn contracted_promises_reach_the_tasks_phase_with_literal_reopen_state() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    let mut phase_mismatches = Vec::new();
    for (quote, allowed) in [
        ("I'll email the report tomorrow.", true),
        ("We'll send the report tomorrow.", true),
        ("I’ll email the report tomorrow.", true),
        ("We’ll send the report tomorrow.", true),
        ("I'll not email the report tomorrow.", false),
        ("We'll not send the report tomorrow.", false),
        ("I’ll not email the report tomorrow.", false),
        ("We’ll not send the report tomorrow.", false),
        ("I will check if the report is ready.", true),
        ("I’ll ask if the budget is approved.", true),
        ("I will check whether the report is ready.", true),
        ("I’ll ask whether the budget is approved.", true),
        (
            "I will ask if the report is ready and check if the pilot can start.",
            true,
        ),
        ("I will email the report if the budget is approved.", false),
        (
            "I will check if the report is ready if the budget is approved.",
            false,
        ),
        (
            "If the budget is approved, I'll ask if the report is ready.",
            false,
        ),
        (
            "I'll check if the report is ready unless the owner objects.",
            false,
        ),
        ("I will not check if the report is ready.", false),
        ("I’ll not ask if the budget is approved.", false),
        ("I will check that the report is not public.", true),
        (
            "Ich übernehme die Prüfung, ob der Bericht nicht öffentlich ist.",
            true,
        ),
        ("I will check if the report is not public.", true),
        ("I’ll ask whether there is no commitment yet.", true),
        ("Ich prüfe, ob der Bericht nicht öffentlich ist.", true),
        ("I will ensure that the report is not public.", true),
        ("Robin prüft, ob der Bericht nicht öffentlich ist.", true),
        ("Robin prüft, ob der Bericht öffentlich ist.", true),
        ("I will check that nobody has published the report.", true),
        (
            "Ich prüfe, ob niemand den Bericht veröffentlicht hat.",
            true,
        ),
        ("I will not check that the report is public.", false),
        (
            "Ich übernehme die Prüfung nicht, ob der Bericht öffentlich ist.",
            false,
        ),
        ("Ich prüfe nicht, ob der Bericht öffentlich ist.", false),
        (
            "I will check that the report is not public, but I will not do the check.",
            false,
        ),
        (
            "Ich prüfe, ob der Bericht nicht öffentlich ist, aber ich prüfe nicht.",
            false,
        ),
        (
            "I will check that the report is not public; no commitment.",
            false,
        ),
        (
            "I will check that the report is not public and I will not do the check.",
            false,
        ),
        (
            "I will check if the report is not public if approval arrives.",
            false,
        ),
        (
            "I won’t forget to check that the report is not public.",
            true,
        ),
        ("I won’t forget to check if the report is ready.", true),
        ("I won’t forget to not check the report.", false),
        (
            "I won’t forget to check that the report is not public, but I won’t do the check.",
            false,
        ),
        ("Robin will check if the report is ready.", true),
        ("Robin will check that the report is not public.", true),
        ("Robin will ask if the report is ready.", true),
        ("Robin will not check if the report is ready.", false),
        ("Robin will check the report if approval arrives.", false),
        ("No one will check the report.", false),
        ("None of us will send the report.", false),
        ("Neither Robin nor Casey will send the report.", false),
        (
            "I will check that none of us has published the report.",
            true,
        ),
        ("I will check whether neither copy is public.", true),
        ("No employee will send the report.", false),
        ("Today no participant will prepare the report.", false),
        ("Not a problem, no employee will send the report.", false),
        ("I will check that no one has published the report.", true),
        (
            "I will ask whether no employee has published the report.",
            true,
        ),
        (
            "No problem, I will check that no one has published the report.",
            true,
        ),
        ("No later than Friday, I will send the report.", true),
        ("I will send the report no later than Friday.", true),
        ("Das Protokoll wurde erstellt.", false),
        ("Der Bericht ist erstellt.", false),
        ("Das Protokoll wurde bereits erstellt.", false),
        ("Der Bericht war heute schon vollständig erstellt.", false),
        ("Das Protokoll ist bereits erstellt, wie besprochen.", false),
        ("Robin ist bereit und erstellt den Bericht.", true),
        ("Robin war krank, erstellt jetzt den Bericht.", true),
        (
            "Ich übernehme die Prüfung, ob das Protokoll bereits erstellt ist.",
            true,
        ),
        ("Der Bericht wurde erstellt .", false),
        ("Das Protokoll ist erstellt, wie besprochen.", false),
        ("Robin erstellt, wie vereinbart, den Bericht.", true),
        ("Robin erstellt den Bericht.", true),
        (
            "Ich übernehme die Prüfung, ob das Protokoll erstellt ist.",
            true,
        ),
        (
            "Das Protokoll wurde erstellt, ich übernehme die Prüfung.",
            true,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let original = quote.to_owned();
        let server = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(5);
            let mut phases = Vec::new();
            while Instant::now() < end {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 8192];
                let header = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && bytes.len() < 1_000_000);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(at) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let size = String::from_utf8_lossy(&bytes[..header])
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .unwrap()
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                assert!(size < 1_000_000);
                while bytes.len() < header + size {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes[header..header + size]).unwrap();
                let req: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                assert_eq!(req["transcriptSources"][0]["text"], original);
                let phase = req["purpose"].as_str().unwrap().to_owned();
                phases.push(phase.clone());
                let changes = if phase == "tasks" {
                    json!([{"proposalId":"p_promise","kind":"task","targetId":null,
                        "targetRevision":null,"text":if original.to_lowercase().contains("prüf") { "Bericht prüfen" } else if original.contains("check") || original.contains("ensure") { "Check the report" } else if original.contains("ask") { "Ask about the budget" } else if original.contains("send") { "Send the report" } else { "Email the report" },"status":"open",
                        "attention":"current","owner":null,"due":null,
                        "evidence":[{"sourceId":req["transcriptSources"][0]["id"],
                            "quote":original}],"relatedEntityIds":[]}])
                } else {
                    json!([])
                };
                let answer = json!({"contractVersion":CONTRACT_VERSION,
                    "requestId":req["requestId"],"changes":changes,"overviewRefs":[],
                    "unresolvedContextIds":[],"moreChangesPossible":false})
                .to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                if phase == "reconcile" {
                    break;
                }
            }
            phases
        });
        let folder = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(folder.path()).unwrap();
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
            key: "promise".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: quote.into(),
        }];
        let pending = runner.pending(&texts);
        let language = if quote.to_lowercase().contains("prüf") {
            "de"
        } else {
            "en"
        };
        let result = runner.update(&client, &pending, &AnalysisControl::default(), language);
        let phases = server.join().unwrap();
        result.unwrap_or_else(|error| panic!("{quote}: {error}"));
        let expected_phases = if allowed {
            vec!["tasks", "facts", "reconcile"]
        } else {
            vec!["facts", "reconcile"]
        };
        if phases != expected_phases || runner.state.entities.len() != usize::from(allowed) {
            phase_mismatches.push(format!(
                "{quote}: phases={phases:?}, tasks={}, source_processed={}",
                runner.state.entities.len(),
                runner.pending(&texts).is_empty()
            ));
        }
        if allowed && runner.state.entities.len() == 1 {
            assert_eq!(runner.state.entities[0].evidence[0].quote, quote);
        }
        assert_eq!(runner.state.originals[0].text, quote);
        assert!(runner.pending(&texts).is_empty());
        let before = serde_json::to_vec(&runner.state).unwrap();
        drop(runner);
        let reopened = WorkspaceRunner::open(folder.path()).unwrap();
        assert_eq!(
            serde_json::to_vec(&reopened.state).unwrap(),
            before,
            "{quote}"
        );
    }
    assert!(
        phase_mismatches.is_empty(),
        "Task admission lost an unconditional pledge or admitted a condition: {phase_mismatches:?}"
    );
}
