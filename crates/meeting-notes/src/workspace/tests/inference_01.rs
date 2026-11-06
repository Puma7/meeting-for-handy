use super::*;

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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let mut stream = accept_synthetic_before(&listener, deadline).unwrap();
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
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let mut stream = accept_synthetic_before(&listener, deadline).unwrap();
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
fn numeric_task_status_retains_original_text_evidence_without_accepting_new_numbers_or_promised_completion(
) {
    let (mut state, mut request, mut answer) = setup();
    request.transcript_sources[0].text = "I will send 3 reports.".into();
    answer["changes"][0]["text"] = json!("Send 3 reports");
    answer["changes"][0]["due"] = json!(null);
    answer["changes"][0]["evidence"][0]["quote"] = json!("I will send 3 reports.");
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    state.originals = request.transcript_sources.clone();
    let original = state.entities[0].evidence[0].clone();
    let done = Source {
        id: "s_done".into(),
        source: "microphone".into(),
        start_ms: 3000,
        end_ms: 6000,
        text: "I have sent the reports.".into(),
    };
    let mut sources = state.originals.clone();
    sources.push(done.clone());
    let mut completion = Request::new(
        "r_done".into(),
        &WorkspaceContext::default(),
        &state,
        state.entities.clone(),
        std::slice::from_ref(&done),
        sources.clone(),
    );
    completion.purpose = "tasks".into();
    answer["requestId"] = json!("r_done");
    answer["changes"][0]["targetId"] = json!(state.entities[0].id);
    answer["changes"][0]["targetRevision"] = json!(1);
    answer["changes"][0]["text"] = json!(null);
    answer["changes"][0]["status"] = json!("completed");
    answer["changes"][0]["evidence"] =
        json!([{"sourceId":"s_done","quote":"I have sent the reports."}]);
    let before = state.clone();
    validate::apply(&mut state, &completion, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].status, "completed");
    assert_eq!(state.entities[0].text, "Send 3 reports");
    assert!(state.entities[0].evidence.contains(&original));
    assert!(state.entities[0]
        .evidence
        .iter()
        .any(|e| e.source_id == "s_done"));
    state.originals = sources.clone();
    state.revision = 1;
    let dir = tempfile::tempdir().unwrap();
    store::append(
        dir.path(),
        &store::Delta::between(&WorkspaceState::default(), &state, sources, vec![]),
    )
    .unwrap();
    assert_eq!(read_state(dir.path()).unwrap().entities, state.entities);
    let bytes = serde_json::to_vec(&before).unwrap();
    let mut rejected = before.clone();
    let mut changed = answer.clone();
    changed["changes"][0]["text"] = json!("Send 30 reports");
    assert!(validate::apply(&mut rejected, &completion, &changed.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
    completion.transcript_sources[1].text = "I will send the reports.".into();
    answer["changes"][0]["evidence"][0]["quote"] = json!("I will send the reports.");
    assert!(validate::apply(&mut rejected, &completion, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
    completion.transcript_sources[1].text = "We might cancel the reports.".into();
    answer["changes"][0]["status"] = json!("withdrawn");
    answer["changes"][0]["evidence"][0]["quote"] = json!("We might cancel the reports.");
    assert!(validate::apply(&mut rejected, &completion, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
    answer["changes"][0]["status"] = json!("open");
    answer["changes"][0]["evidence"] = json!([]);
    assert!(validate::apply(&mut rejected, &completion, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
}

#[test]
fn model_switch_keeps_retired_historical_sources_retired() {
    let root = tempfile::tempdir().unwrap();
    let old_text = LiveText {
        key: "old".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: "I will send the report.".into(),
    };
    let new_text = LiveText {
        key: "new".into(),
        track: "system".into(),
        start_ms: 4000,
        end_ms: 7000,
        text: "I will check the report.".into(),
    };
    let live = live::pending(&[], &[old_text.clone(), new_text.clone()]);
    let originals = live
        .iter()
        .map(|s| Source {
            id: source_id(s),
            source: s.source.clone(),
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text.clone(),
        })
        .collect::<Vec<_>>();
    let first_folder = root.path().join("model-a");
    let mut a = WorkspaceRunner::open(&first_folder).unwrap();
    a.set_context(Default::default()).unwrap();
    let context = a.context.lock().unwrap().clone();
    let mut state = a.state.clone();
    let mut req = Request::new(
        "r_seed".into(),
        &context,
        &state,
        vec![],
        std::slice::from_ref(&originals[0]),
        originals.clone(),
    );
    req.purpose = "tasks".into();
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_seed","changes":[{"proposalId":"p_task","kind":"task","targetId":null,"targetRevision":null,"text":"Send the report","status":"open","attention":"current","owner":null,"due":null,"evidence":[{"sourceId":originals[0].id,"quote":originals[0].text}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let id = state.entities[0].id.clone();
    let mut req = Request::new(
        "r_edit".into(),
        &context,
        &state,
        state.entities.clone(),
        std::slice::from_ref(&originals[1]),
        originals.clone(),
    );
    req.purpose = "tasks".into();
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_edit","changes":[{"proposalId":"p_edit","kind":"task","targetId":id,"targetRevision":state.entities[0].revision,"text":"Check the report","status":"open","attention":"current","owner":null,"due":null,"evidence":[{"sourceId":originals[1].id,"quote":originals[1].text}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].evidence.len(), 1);
    assert_eq!(state.entities[0].evidence[0].source_id, originals[1].id);
    assert!(state
        .events
        .iter()
        .any(|e| e.evidence.iter().any(|v| v.source_id == originals[0].id)));
    state.originals = originals.clone();
    state.processed = live;
    a.commit(
        state,
        originals.clone(),
        vec![],
        &AnalysisControl::default(),
        &context,
    )
    .unwrap();
    let mut context = a.context.lock().unwrap().clone();
    context.overrides.push(Override {
        id: id.clone(),
        expected_revision: a.state.entities[0].revision,
        status: "completed".into(),
        saved: true,
    });
    drop(a);
    update_context(&first_folder, &context).unwrap();
    let mut a = WorkspaceRunner::open(&first_folder).unwrap();
    assert!(a.pending(std::slice::from_ref(&new_text)).is_empty());
    let client = TextClient::new(&crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: "no-request".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    a.update(&client, &[], &AnalysisControl::default(), "en")
        .unwrap();
    let previous = a.view();
    assert!(previous.superseded_sources.contains(&originals[0].id));
    assert!(!previous.context_excluded_ids.contains(&id));
    assert!(previous.entities[0].human_locked);
    assert_eq!(previous.entities[0].status, "completed");
    let second_folder = root.path().join("model-b");
    let mut b = WorkspaceRunner::open(&second_folder).unwrap();
    b.set_context(Default::default()).unwrap();
    b.inherit_manual(&previous).unwrap();
    drop(b);
    let b = WorkspaceRunner::open(&second_folder).unwrap();
    assert!(b.state.superseded_sources.contains(&originals[0].id));
    assert_eq!(b.state.originals, originals);
    let pending = b.pending(&[old_text, new_text]);
    assert_eq!(pending.len(), 2);
    assert_ne!(
        source_id(&pending[0]),
        originals[0].id,
        "A withdrawn historical identity must never be revived by equal text."
    );
    assert_eq!(
        source_id(&pending[1]),
        originals[1].id,
        "The still-valid current witness retains its identity."
    );
    assert_eq!(b.state.entities[0], previous.entities[0]);
    assert_eq!(
        serde_json::to_vec(&b.view()).unwrap(),
        serde_json::to_vec(&read_state(&second_folder).unwrap()).unwrap()
    );
}

#[test]
fn model_switch_reuses_unchanged_inherited_sources_without_reopening_human_tasks() {
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
    for reopen in [false, true] {
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
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                let mut changes = vec![];
                if req["purpose"] == "tasks" {
                    for source in req["transcriptSources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|s| {
                            req["window"]["requiredSourceIds"]
                                .as_array()
                                .unwrap()
                                .contains(&s["id"])
                        })
                    {
                        if source["text"] == "I will send the report tomorrow." {
                            changes.push(json!({"proposalId":format!("p_{}",changes.len()),"kind":"task","targetId":null,"targetRevision":null,"text":"Send the report","status":"open","attention":"current","owner":null,"due":"tomorrow","evidence":[{"sourceId":source["id"],"quote":source["text"]}],"relatedEntityIds":[]}));
                        }
                    }
                }
                let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
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
        let root = tempfile::tempdir().unwrap();
        let connection = |model: &str| crate::Connection {
            endpoint: endpoint.clone(),
            model: model.into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let first =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-a"), "en")
                .unwrap()
                .with_workspace(Default::default())
                .unwrap();
        let first_folder = first.workspace_folder().unwrap();
        drop(first);
        let text = LiveText {
            key: "part-0".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: "I will send the report tomorrow.".into(),
        };
        let client = TextClient::new(&connection("synthetic-switch")).unwrap();
        let mut a = WorkspaceRunner::open(&first_folder).unwrap();
        let pending = a.pending(std::slice::from_ref(&text));
        let original_id = source_id(&pending[0]);
        a.update(&client, &pending, &AnalysisControl::default(), "en")
            .unwrap();
        let entity_id = a.state.entities[0].id.clone();
        let mut context = a.context.lock().unwrap().clone();
        context.overrides.push(Override {
            id: entity_id.clone(),
            expected_revision: a.state.entities[0].revision,
            status: "completed".into(),
            saved: true,
        });
        drop(a);
        update_context(&first_folder, &context).unwrap();
        let completed = read_state(&first_folder).unwrap().entities[0].clone();
        let old_files = std::fs::read_dir(&first_folder)
            .unwrap()
            .map(Result::unwrap)
            .filter(|e| e.file_name().to_string_lossy().starts_with("revision-"))
            .map(|e| (e.path(), std::fs::read(e.path()).unwrap()))
            .collect::<Vec<_>>();
        let second =
            crate::live::LiveRunner::start(root.path(), "meeting", connection("model-b"), "en")
                .unwrap()
                .with_workspace(Default::default())
                .unwrap();
        let second_folder = second.workspace_folder().unwrap();
        drop(second);
        let mut b = WorkspaceRunner::open(&second_folder).unwrap();
        assert!(b.state.processed.is_empty());
        assert_eq!(b.state.entities[0], completed);
        if reopen {
            drop(b);
            b = WorkspaceRunner::open(&second_folder).unwrap();
        }
        let pending = b.pending(std::slice::from_ref(&text));
        let offered_id = source_id(&pending[0]);
        b.update(&client, &pending, &AnalysisControl::default(), "en")
            .unwrap();
        if offered_id != original_id
            || b.state.entities.iter().filter(|e| e.kind == "task").count() != 1
        {
            failures.push(format!(
                "reopen={reopen}: {} -> {}; tasks={}",
                original_id,
                offered_id,
                b.state.entities.iter().filter(|e| e.kind == "task").count()
            ));
        }
        assert_eq!(
            b.state.entities.iter().find(|e| e.id == entity_id).unwrap(),
            &completed
        );
        assert!(b.pending(std::slice::from_ref(&text)).is_empty());
        drop(b);
        let mut b = WorkspaceRunner::open(&second_folder).unwrap();
        let state = serde_json::to_vec(&b.view()).unwrap();
        assert_eq!(
            state,
            serde_json::to_vec(&read_state(&second_folder).unwrap()).unwrap()
        );
        assert!(b.pending(std::slice::from_ref(&text)).is_empty());
        let mut independent = text.clone();
        independent.key = "part-1".into();
        independent.start_ms = 4000;
        independent.end_ms = 7000;
        let pending = b.pending(&[text, independent]);
        assert_eq!(pending.len(), 1);
        assert_ne!(source_id(&pending[0]), original_id);
        b.update(&client, &pending, &AnalysisControl::default(), "en")
            .unwrap();
        assert!(b
            .state
            .entities
            .iter()
            .any(|e| e.kind == "task" && e.status == "open" && e.id != entity_id));
        for (path, bytes) in old_files {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        stop.store(true, Ordering::SeqCst);
        let requests = server.join().unwrap();
        let second_task = requests
            .iter()
            .filter(|r| r["purpose"] == "tasks")
            .nth(1)
            .unwrap();
        if !second_task["state"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == entity_id && e["status"] == "completed" && e["humanLocked"] == true)
        {
            failures.push(format!(
                "reopen={reopen}: inherited terminal identity missing from actual tasks request"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "Model-switch source regression: {failures:?}"
    );
}

#[test]
fn inherited_confirmations_keep_multi_entity_order_and_exact_reopen_state() {
    for (target, append_new) in [(0, false), (1, false), (2, false), (0, true)] {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner.set_context(Default::default()).unwrap();
        let context = runner.context.lock().unwrap().clone();
        let sources: Vec<_> = [
            "Ich gebe den Entwurf morgen frei.",
            "Ich prüfe die Unterlagen morgen.",
            "Ich schreibe den Bericht morgen.",
        ]
        .iter()
        .enumerate()
        .map(|(i, quote)| Source {
            id: format!("s_order_{i}"),
            source: "system".into(),
            start_ms: i as u64 * 3000,
            end_ms: i as u64 * 3000 + 3000,
            text: (*quote).into(),
        })
        .collect();
        let mut state = WorkspaceState::default();
        state.apply_context(&context).unwrap();
        let request = Request::new(
            "r_order".into(),
            &context,
            &state,
            vec![],
            &sources,
            sources.clone(),
        );
        let changes: Vec<_> = sources.iter().enumerate().map(|(i, source)| json!({
            "proposalId":format!("p_order_{i}"),"kind":"task","targetId":null,"targetRevision":null,
            "text":(["Entwurf freigeben","Unterlagen prüfen","Bericht schreiben"][i]),
            "status":"open","attention":"normal","owner":null,"due":null,
            "evidence":[{"sourceId":source.id,"quote":source.text}],"relatedEntityIds":[]
        })).collect();
        let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_order","changes":changes,
            "overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
        validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        state.originals = sources.clone();
        runner
            .commit(
                state,
                sources,
                vec![],
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
        let original_order: Vec<_> = runner.state.entities.iter().map(|e| e.id.clone()).collect();
        let saved_before: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .map(|p| {
                let bytes = std::fs::read(&p).unwrap();
                (p, bytes)
            })
            .collect();
        let mut incoming = runner.state.clone();
        let selected = &mut incoming.entities[target];
        selected.human_locked = true;
        selected.status = "completed".into();
        selected.revision += 1;
        let id = selected.id.clone();
        incoming.human_activity.insert(
            id.clone(),
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:03Z").unwrap(),
        );
        let appended_id = if append_new {
            let mut added = incoming.entities[1].clone();
            added.id = "e_appended_confirmation".into();
            added.human_locked = true;
            incoming.human_activity.insert(
                added.id.clone(),
                chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:04Z").unwrap(),
            );
            let id = added.id.clone();
            incoming.entities.push(added);
            Some(id)
        } else {
            None
        };
        // Source-run display order is not authority to reorder the destination.
        incoming.entities.reverse();
        runner.inherit_manual(&incoming).unwrap();
        let expected = runner.view();
        let reopened = read_state(dir.path()).unwrap();
        assert_eq!(
            expected.entities, reopened.entities,
            "target={target}, append={append_new}"
        );
        assert_eq!(
            serde_json::to_vec(&expected).unwrap(),
            serde_json::to_vec(&reopened).unwrap()
        );
        assert_eq!(expected.human_activity, reopened.human_activity);
        let mut expected_order = original_order;
        if let Some(id) = appended_id {
            expected_order.push(id);
        }
        assert_eq!(
            expected
                .entities
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>(),
            expected_order
        );
        let confirmed = expected.entities.iter().find(|e| e.id == id).unwrap();
        assert_eq!(confirmed.status, "completed");
        assert!(confirmed.human_locked);
        for (path, bytes) in saved_before {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        let revision = runner.state.revision;
        runner.inherit_manual(&incoming).unwrap();
        assert_eq!(
            runner.state.revision, revision,
            "same confirmation must be idempotent"
        );
        drop(runner);
        let runner = WorkspaceRunner::open(dir.path()).unwrap();
        assert_eq!(
            serde_json::to_vec(&runner.view()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
    }
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
