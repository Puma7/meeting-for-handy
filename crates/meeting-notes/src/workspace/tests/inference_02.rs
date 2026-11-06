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

/// Replay an actual bounded source window after withdrawal, beyond optional model seeds.
#[test]
fn withdrawn_decision_replay_keeps_identity_when_optional_seeds_hide_it() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    /// Persist validated synthetic windows through the production workspace writer.
    fn seed_window(
        runner: &mut WorkspaceRunner,
        texts: &[LiveText],
        mut changes: serde_json::Value,
    ) -> Vec<Source> {
        let context = runner.context.lock().unwrap().clone();
        let processed = live::pending(&[], texts);
        let fresh = processed
            .iter()
            .map(|s| Source {
                id: source_id(s),
                source: s.source.clone(),
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: s.text.clone(),
            })
            .collect::<Vec<_>>();
        let mut originals = runner.state.originals.clone();
        originals.extend(fresh.clone());
        for change in changes.as_array_mut().unwrap() {
            for evidence in change["evidence"].as_array_mut().unwrap() {
                let quote = evidence["quote"].as_str().unwrap();
                evidence["sourceId"] =
                    json!(fresh.iter().find(|s| s.text.contains(quote)).unwrap().id);
            }
        }
        let mut next = runner.state.clone();
        next.apply_context(&context).unwrap();
        let mut request = Request::new(
            "r_seed".into(),
            &context,
            &next,
            next.entities.clone(),
            &fresh,
            originals.clone(),
        );
        request.purpose = "facts".into();
        let response = json!({"contractVersion":CONTRACT_VERSION,"requestId":request.request_id,"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
        let events = validate::apply(&mut next, &request, &response.to_string()).unwrap();
        next.originals = originals;
        next.processed.extend(processed);
        runner
            .commit(
                next,
                fresh.clone(),
                events,
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
        fresh
    }
    let directory = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(directory.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let first_quote = "We decided to release the report.";
    let other_quote = "We have decided to release the report.";
    let make_text = |index: u64, text: String| LiveText {
        key: format!("part-{index}"),
        track: "system".into(),
        start_ms: index * 3000,
        end_ms: (index + 1) * 3000,
        text,
    };
    let padding = "Background context. ".repeat(80);
    let mut first = format!("{first_quote} {other_quote} {padding}");
    first.truncate(1500);
    let mut texts = vec![make_text(0, first)];
    for index in 1..4 {
        let mut value = padding.clone();
        value.truncate(1500);
        texts.push(make_text(index, value));
    }
    let original = seed_window(&mut runner,&texts,json!([{"proposalId":"p_decision","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":first_quote},{"sourceId":"s_pending","quote":other_quote}],"relatedEntityIds":[]}]))[0].clone();
    let decision_id = runner.state.entities[0].id.clone();
    let mut filler = padding.clone();
    filler.truncate(1500);
    let withdrawal = "We withdraw the release decision.";
    let later = vec![make_text(4, filler), make_text(5, withdrawal.into())];
    seed_window(
        &mut runner,
        &later,
        json!([{"proposalId":"p_withdraw","kind":"decision","targetId":decision_id,"targetRevision":1,"text":null,"status":"withdrawn","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":withdrawal}],"relatedEntityIds":[]}]),
    );
    texts.extend(later);
    let facts = [
        "Budget discussed.",
        "Scope reviewed.",
        "Schedule reviewed.",
        "Testing reviewed.",
        "Costs reviewed.",
        "Risks reviewed.",
    ];
    let fact_text = make_text(6, facts.join(" "));
    let changes=facts.iter().enumerate().map(|(index,quote)|json!({"proposalId":format!("p_fact_{index}"),"kind":"fact","targetId":null,"targetRevision":null,"text":quote,"status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":quote}],"relatedEntityIds":[]})).collect::<Vec<_>>();
    seed_window(
        &mut runner,
        std::slice::from_ref(&fact_text),
        json!(changes),
    );
    texts.push(fact_text);
    let before = runner.view();
    let withdrawn = before
        .entities
        .iter()
        .find(|e| e.id == decision_id)
        .unwrap()
        .clone();
    assert_eq!(withdrawn.status, "withdrawn");
    assert_eq!(withdrawn.evidence.len(), 2);
    let omitted = [first_quote, other_quote]
        .into_iter()
        .find(|quote| !withdrawn.evidence.iter().any(|e| e.quote == *quote))
        .unwrap();
    assert!(before
        .events
        .iter()
        .any(|event| event.entity_id == decision_id
            && event.status == "recorded"
            && event.evidence.iter().any(|e| e.quote == omitted)));
    let old_files = (1..=before.revision)
        .map(|revision| {
            let path = directory
                .path()
                .join(format!("revision-{revision:08}.json"));
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    let context = runner.context.lock().unwrap().clone();
    let mut forced_request = Request::new(
        "r_forced".into(),
        &context,
        &before,
        before.entities.clone(),
        std::slice::from_ref(&original),
        before.originals.clone(),
    );
    forced_request.purpose = "facts".into();
    let forced_answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_forced","changes":[{"proposalId":"p_forced","kind":"decision","targetId":decision_id,"targetRevision":withdrawn.revision,"text":null,"status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":original.id,"quote":omitted}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
    let mut forced = before.clone();
    let forced_result = validate::apply(&mut forced, &forced_request, &forced_answer.to_string());
    let mut mismatches = Vec::new();
    if forced_result.is_ok() || forced.entities != before.entities {
        mismatches.push("an explicit model target reopened a withdrawn decision".to_owned());
    }
    let mut changed = context.clone();
    changed.agenda.push(Agenda {
        id: "e_release_review".into(),
        revision: 1,
        text: "Release review".into(),
    });
    let changed = update_context(directory.path(), &changed).unwrap().0;
    *runner.context.lock().unwrap() = changed;
    assert!(runner.context_dirty());
    assert_eq!(live::next_window(&runner.active_sources()).len(), 4);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server_original = original.clone();
    let server_decision = decision_id.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut facts_seen = 0;
        loop {
            let mut stream = accept_synthetic_before(&listener, deadline).unwrap();
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
                .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
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
            let changes = if req["purpose"] == "facts" {
                facts_seen += 1;
                let selected = req["state"].as_array().unwrap();
                assert_eq!(selected.len(), 6);
                assert!(selected
                    .iter()
                    .all(|e| e["kind"] == "fact" && e["id"] != server_decision));
                assert!(!req["transcriptSources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["text"].as_str().unwrap().contains("withdraw")));
                json!([{"proposalId":"p_replay","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":server_original.id,"quote":omitted}],"relatedEntityIds":[]}])
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
        facts_seen
    });
    let client = TextClient::new(&crate::Connection {
        endpoint,
        model: "synthetic".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    runner
        .update(&client, &[], &AnalysisControl::default(), "en")
        .unwrap();
    assert_eq!(server.join().unwrap(), 1);
    let replayed = runner.view();
    if replayed
        .entities
        .iter()
        .filter(|e| e.kind == "decision")
        .count()
        != 1
        || replayed
            .entities
            .iter()
            .any(|e| e.kind == "decision" && e.status != "withdrawn")
    {
        mismatches.push("context replay republished a withdrawn decision as recorded".to_owned());
    }
    for (path, bytes) in old_files {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    drop(runner);
    let reopened = WorkspaceRunner::open(directory.path()).unwrap().view();
    assert_eq!(reopened.entities, replayed.entities);
    assert_eq!(
        serde_json::to_vec(&reopened).unwrap(),
        serde_json::to_vec(&WorkspaceRunner::open(directory.path()).unwrap().view()).unwrap()
    );
    let mut fresh = original.clone();
    fresh.id = "s_independent_decision".into();
    fresh.start_ms = 24000;
    fresh.end_ms = 27000;
    let mut all = before.originals.clone();
    all.push(fresh.clone());
    let mut request = Request::new(
        "r_fresh".into(),
        &context,
        &before,
        before.entities.clone(),
        std::slice::from_ref(&fresh),
        all,
    );
    request.purpose = "facts".into();
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_fresh","changes":[{"proposalId":"p_fresh","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":fresh.id,"quote":omitted}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
    let mut independent = before.clone();
    validate::apply(&mut independent, &request, &answer.to_string()).unwrap();
    assert_eq!(
        independent
            .entities
            .iter()
            .filter(|e| e.kind == "decision" && e.status == "recorded")
            .count(),
        1
    );
    assert_eq!(
        independent
            .entities
            .iter()
            .find(|e| e.id == decision_id)
            .unwrap(),
        &withdrawn
    );
    assert!(
        mismatches.is_empty(),
        "Withdrawn decision regressions: {mismatches:?}"
    );
}

#[test]
fn terminal_replay_uses_immutable_promise_history_after_evidence_minimization() {
    let mut duplicates = Vec::new();
    for terminal in ["completed", "withdrawn"] {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut state, mut request, mut answer) = setup();
        request.purpose = "tasks".into();
        request.transcript_sources[0].text = "I will send the report tomorrow.".into();
        let alternative = Source {
            id: "s_other_promise".into(),
            source: "microphone".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: "I promise to send the report tomorrow.".into(),
        };
        request.transcript_sources.push(alternative.clone());
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["due"] = json!("tomorrow");
        answer["changes"][0]["evidence"] = json!([{"sourceId":"s_one","quote":"I will send the report tomorrow."},{"sourceId":alternative.id,"quote":alternative.text}]);
        let events = validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        assert_eq!(state.entities[0].evidence.len(), 2);
        state.revision = 1;
        store::append(
            directory.path(),
            &store::Delta::between(&empty, &state, request.transcript_sources.clone(), events),
        )
        .unwrap();
        let seed = read_state(directory.path()).unwrap();
        let done = Source {
            id: "s_terminal".into(),
            source: "system".into(),
            start_ms: 6000,
            end_ms: 9000,
            text: if terminal == "completed" {
                "I have sent the report."
            } else {
                "I withdraw the report task."
            }
            .into(),
        };
        let mut sources = seed.originals.clone();
        sources.push(done.clone());
        let mut completion = Request::new(
            "r_terminal".into(),
            &seed.context,
            &seed,
            seed.entities.clone(),
            std::slice::from_ref(&done),
            sources,
        );
        completion.purpose = "tasks".into();
        answer["requestId"] = json!("r_terminal");
        answer["changes"][0]["targetId"] = json!(seed.entities[0].id);
        answer["changes"][0]["targetRevision"] = json!(seed.entities[0].revision);
        answer["changes"][0]["text"] = json!(null);
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["status"] = json!(terminal);
        answer["changes"][0]["evidence"] = json!([{"sourceId":done.id,"quote":done.text}]);
        let mut next = seed.clone();
        let events = validate::apply(&mut next, &completion, &answer.to_string()).unwrap();
        next.revision = 2;
        store::append(
            directory.path(),
            &store::Delta::between(&seed, &next, vec![done], events),
        )
        .unwrap();
        let terminal_state = read_state(directory.path()).unwrap();
        assert_eq!(terminal_state.entities[0].evidence.len(), 2);
        let omitted = seed.entities[0]
            .evidence
            .iter()
            .find(|e| !terminal_state.entities[0].evidence.contains(e))
            .unwrap()
            .clone();
        assert!(terminal_state
            .events
            .iter()
            .any(|e| e.entity_id == seed.entities[0].id
                && e.revision == 1
                && e.evidence.contains(&omitted)));
        let old_files = (1..=2)
            .map(|i| {
                let path = directory.path().join(format!("revision-{i:08}.json"));
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<Vec<_>>();
        let source = terminal_state
            .originals
            .iter()
            .find(|s| s.id == omitted.source_id)
            .unwrap()
            .clone();
        let mut replay = Request::new(
            "r_replay".into(),
            &terminal_state.context,
            &terminal_state,
            terminal_state.entities.clone(),
            std::slice::from_ref(&source),
            terminal_state.originals.clone(),
        );
        replay.purpose = "tasks".into();
        answer["requestId"] = json!("r_replay");
        answer["changes"][0]["targetId"] = json!(null);
        answer["changes"][0]["targetRevision"] = json!(null);
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["status"] = json!("open");
        answer["changes"][0]["evidence"] =
            json!([{"sourceId":omitted.source_id,"quote":omitted.quote}]);
        let mut replayed = terminal_state.clone();
        let events = validate::apply(&mut replayed, &replay, &answer.to_string()).unwrap();
        if replayed.entities != terminal_state.entities || !events.is_empty() {
            duplicates.push(format!("{terminal}: minimized current evidence lost the original promise identity; entities={}, events={}",replayed.entities.len(),events.len()));
        }
        replayed.revision += 1;
        store::append(
            directory.path(),
            &store::Delta::between(&terminal_state, &replayed, vec![], events),
        )
        .unwrap();
        let reopened = read_state(directory.path()).unwrap();
        assert_eq!(reopened.entities, replayed.entities);
        assert_eq!(
            serde_json::to_vec(&reopened).unwrap(),
            serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap()
        );
        let fresh = Source {
            id: "s_fresh_promise".into(),
            source: "microphone".into(),
            start_ms: 9000,
            end_ms: 12000,
            text: source.text.clone(),
        };
        let mut sources = reopened.originals.clone();
        sources.push(fresh.clone());
        let mut req = Request::new(
            "r_fresh".into(),
            &reopened.context,
            &reopened,
            reopened.entities.clone(),
            std::slice::from_ref(&fresh),
            sources,
        );
        req.purpose = "tasks".into();
        answer["requestId"] = json!("r_fresh");
        answer["changes"][0]["evidence"] = json!([{"sourceId":fresh.id,"quote":fresh.text}]);
        let mut renewed = reopened.clone();
        validate::apply(&mut renewed, &req, &answer.to_string()).unwrap();
        assert_eq!(
            renewed
                .entities
                .iter()
                .filter(|e| e.kind == "task" && e.status == "open")
                .count(),
            1
        );
        assert_eq!(
            renewed
                .entities
                .iter()
                .find(|e| e.id == terminal_state.entities[0].id)
                .unwrap(),
            &terminal_state.entities[0]
        );
        for (path, bytes) in old_files {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
    assert!(
        duplicates.is_empty(),
        "Historical replay duplicates: {duplicates:?}"
    );
}
