use super::*;

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
