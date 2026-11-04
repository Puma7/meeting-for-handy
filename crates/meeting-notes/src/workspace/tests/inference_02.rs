use super::*;

#[test]
fn facts_frontier_survives_an_empty_tasks_phase_with_more_than_six_selected_facts() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut facts = HashSet::new();
        let mut queued_versions = Vec::new();
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "synthetic request sequence did not finish"
            );
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            // Windows may inherit the listener's nonblocking mode. Accepted
            // synthetic HTTP streams use bounded blocking reads below.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
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
                for entity in req["state"].as_array().unwrap() {
                    if entity["kind"] == "fact" {
                        facts.insert(entity["id"].as_str().unwrap().to_owned());
                    }
                }
                if let Some(entity) = req["state"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"] == "e_fact_7")
                {
                    queued_versions.push((
                        entity["revision"].as_u64().unwrap(),
                        entity["text"].as_str().unwrap().to_owned(),
                    ));
                }
            }
            let changes = if req["purpose"] == "facts" && queued_versions.len() == 1 {
                json!([{"proposalId":"p_seed","kind":"fact","targetId":"e_fact_7","targetRevision":1,"text":"Aktueller Entwurf","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_one","quote":"Ich gebe den Entwurf morgen frei."}],"relatedEntityIds":[]}])
            } else {
                json!([])
            };
            let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
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
        (facts, queued_versions)
    });
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let task = state.entities.remove(0);
    state.originals = req.transcript_sources.clone();
    for i in 0..8 {
        let mut fact = task.clone();
        fact.id = format!("e_fact_{i}");
        fact.kind = "fact".into();
        fact.status = "recorded".into();
        fact.due = None;
        state.overview_refs.push(fact.id.clone());
        state.entities.push(fact);
    }
    state.overview_refs.retain(|id| id.starts_with("e_fact_"));
    state.entities.push(task);
    let context = runner.context.lock().unwrap().clone();
    state.apply_context(&context).unwrap();
    runner
        .commit(
            state,
            req.transcript_sources,
            vec![],
            &AnalysisControl::default(),
            &context,
        )
        .unwrap();
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
        text: "Der Entwurf ist zur Prüfung bereit.".into(),
    }]);
    runner
        .update(&client, &pending, &AnalysisControl::default(), "en")
        .unwrap();
    let (facts, versions) = server.join().unwrap();
    assert_eq!(facts, (0..8).map(|i| format!("e_fact_{i}")).collect());
    assert!(versions.len() >= 2);
    assert_eq!(versions[0].0, 1);
    assert!(
        versions[1..]
            .iter()
            .all(|(revision, text)| *revision == 2 && text == "Aktueller Entwurf"),
        "{versions:?}"
    );
}

#[test]
fn a_request_snapshot_cannot_overwrite_a_newer_current_entity() {
    let (mut state, mut request, mut answer) = setup();
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    request.state = state.entities.clone();
    answer["changes"][0]["targetId"] = json!(state.entities[0].id);
    answer["changes"][0]["targetRevision"] = json!(1);
    answer["changes"][0]["text"] = json!("Aktueller Entwurf");
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].revision, 2);
    let bytes = serde_json::to_vec(&state).unwrap();
    answer["changes"][0]["text"] = json!("Überholter Entwurf");
    assert!(validate::apply(&mut state, &request, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
}
