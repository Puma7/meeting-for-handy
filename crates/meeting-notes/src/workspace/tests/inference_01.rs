use super::*;

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
                        "targetRevision":null,"text":if original.contains("check") { "Check the report" } else if original.contains("ask") { "Ask about the budget" } else if original.contains("send") { "Send the report" } else { "Email the report" },"status":"open",
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
        let result = runner.update(&client, &pending, &AnalysisControl::default(), "en");
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
