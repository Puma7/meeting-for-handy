use super::*;

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

#[test]
fn perfect_decisions_validate_literal_sources_and_durable_reopen() {
    let mut mismatches = Vec::new();
    for (quote, allowed) in [
        ("We have decided to release on Friday.", true),
        ("We've decided to release on Friday.", true),
        ("We’ve decided to release on Friday.", true),
        ("WE'VE DECIDED to release on Friday.", true),
        ("We have already decided to release on Friday.", true),
        ("We've just unanimously decided to release on Friday.", true),
        ("We had decided to release on Friday.", true),
        ("We have decided not to release on Friday.", true),
        ("We decided to release on Friday.", true),
        ("We have agreed to release on Friday.", true),
        ("We have not decided to release on Friday.", false),
        ("We haven't decided to release on Friday.", false),
        ("We haven’t decided to release on Friday.", false),
        ("We've never decided to release on Friday.", false),
        ("We have perhaps decided to release on Friday.", false),
        ("We will decide to release on Friday.", false),
        ("We've decided to release on Friday?", false),
        (
            "We have decided to release on Friday if approval arrives.",
            false,
        ),
        (
            "If approval arrives, we've decided to release on Friday.",
            false,
        ),
        (
            "We've decided to release on Friday unless the owner objects.",
            false,
        ),
        ("Weve decided to release on Friday.", false),
        ("archiveWe've decided to release on Friday.", false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut next, mut req, mut answer) = setup();
        req.purpose = "facts".into();
        req.transcript_sources[0].text = quote.into();
        answer["changes"][0]["kind"] = json!("decision");
        answer["changes"][0]["status"] = json!("recorded");
        answer["changes"][0]["text"] = json!("Release decision");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let offered = quote_options(quote)
            .iter()
            .any(|q| validate::recorded_decision_quote_allowed(q));
        let result = validate::apply(&mut next, &req, &answer.to_string());
        if result.is_ok() != allowed || offered != allowed {
            mismatches.push(format!(
                "{quote}: allowed={allowed}, offered={offered}, result={result:?}"
            ));
            continue;
        }
        if allowed {
            next.revision = 1;
            store::append(
                directory.path(),
                &store::Delta::between(
                    &empty,
                    &next,
                    req.transcript_sources.clone(),
                    result.unwrap(),
                ),
            )
            .unwrap();
            let checkpoint =
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap();
            let reopened = read_state(directory.path()).unwrap();
            assert_eq!(reopened.entities.len(), 1);
            assert_eq!(reopened.entities[0].status, "recorded");
            assert_eq!(reopened.entities[0].evidence[0].quote, quote);
            assert_eq!(reopened.originals[0].text, quote);
            assert_eq!(
                serde_json::to_vec(&reopened).unwrap(),
                serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap()
            );
            assert_eq!(
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap(),
                checkpoint
            );
        } else {
            assert_eq!(
                serde_json::to_vec(&next).unwrap(),
                serde_json::to_vec(&empty).unwrap()
            );
            assert!(!directory.path().join("revision-00000001.json").exists());
        }
    }
    assert!(mismatches.is_empty(), "Decision mismatches: {mismatches:?}");
}

#[test]
fn contracted_perfect_completion_retains_literal_quotes_after_reopen() {
    let cases = [
        ("I've sent the report.", true),
        ("I’ve sent the report.", true),
        ("We've completed the report.", true),
        ("We’ve completed the report.", true),
        ("I have sent the report.", true),
        ("We have completed the report.", true),
        ("I have sent the report nonetheless.", true),
        ("Ich habe den Bericht abgeschlossen.", true),
        ("Ich habe den Bericht vollständig abgeschlossen.", true),
        ("I have completed the report fast.", true),
        ("I have completed the report fully.", true),
        ("Ich habe den Bericht zu 100 Prozent abgeschlossen.", true),
        ("I have completed 100% of the report.", true),
        ("I have completed 100.0 percent of the report.", true),
        ("Ich habe den Bericht zu 100,0 Prozent abgeschlossen.", true),
        ("I have completed 100 per cent of the report.", true),
        ("I have completed the percentage report.", true),
        ("I have completed the 50% discount report.", true),
        ("I have completed the 50-percent-discount report.", true),
        (
            "Ich habe den Bericht zur 50-Prozent-Rabattaktion abgeschlossen.",
            true,
        ),
        (
            "Ich habe den Bericht zur 50 Prozent Rabattaktion abgeschlossen.",
            true,
        ),
        ("I have completed the quarterly report.", true),
        ("I have completed the third report.", true),
        ("I have completed the half-year report.", true),
        ("I have completed the report on 11/5.", true),
        ("Ich habe den Bericht am 11/5 abgeschlossen.", true),
        ("I have completed the report on 2026/11/5.", true),
        ("I have completed the 2026/2027 budget report.", true),
        (
            "Ich habe den Haushaltsbericht 2026/2027 abgeschlossen.",
            true,
        ),
        ("I have completed the Q1/2026 budget report.", true),
        ("Ich habe den Haushaltsbericht Q1/2026 abgeschlossen.", true),
        ("I have completed report 1 of 2.", true),
        ("Ich habe Bericht 1 von 2 abgeschlossen.", true),
        ("I have completed 1/2 today.", false),
        ("Ich habe 1/2 abgeschlossen.", false),
        ("I have completed 1 of 2 today.", false),
        ("I have completed 2/2.", true),
        ("I have completed 2 of 2.", true),
        ("I have completed 1/2parts of the report.", false),
        ("Ich habe 1/2Teilen des Berichts abgeschlossen.", false),
        ("I have completed 2 of 2 parts of the report.", true),
        ("Ich habe 2 von 2 Teilen des Berichts abgeschlossen.", true),
        ("I have completed 2/2 parts of the report.", true),
        ("I have completed three thirds of the report.", true),
        ("Ich habe drei Drittel des Berichts abgeschlossen.", true),
        ("I have completed 100% today.", true),
        (
            "Ich habe den Bericht zu 100 Prozent heute abgeschlossen.",
            true,
        ),
        ("I have completed 50% today.", false),
        ("I have completed 50% really quickly.", false),
        ("I have completed the report 50% yesterday.", false),
        (
            "Ich habe den Bericht zu 50 Prozent heute abgeschlossen.",
            false,
        ),
        (
            "Ich habe den Bericht zu 50 Prozent schon abgeschlossen.",
            false,
        ),
        (
            "Ich habe den Bericht zu 50 Prozent mit kurzem Review abgeschlossen.",
            false,
        ),
        ("I have completed half of the report.", false),
        ("Ich habe den Bericht zur Hälfte abgeschlossen.", false),
        ("Ich habe den Bericht zur Haelfte abgeschlossen.", false),
        ("I have completed one third of the report.", false),
        ("Ich habe ein Drittel des Berichts abgeschlossen.", false),
        ("I have completed 1 of 2 parts of the report.", false),
        ("Ich habe 1 von 2 Teilen des Berichts abgeschlossen.", false),
        ("I have completed 1/2 of the report.", false),
        ("I have completed 50％ of the report.", false),
        ("I have completed 50٪ of the report.", false),
        ("Ich habe den Bericht zu 50 Prozent abgeschlossen.", false),
        ("I have completed 50% of the report.", false),
        ("Ich habe den Bericht zu 99,9 Prozent abgeschlossen.", false),
        ("I have completed 99.9 percent of the report.", false),
        ("I have completed 50 per cent of the report.", false),
        ("I have completed 0% of the report.", false),
        ("I have completed 101% of the report.", false),
        (
            "I have completed 99.9999999999999999999% of the report.",
            false,
        ),
        ("I have completed 90–100% of the report.", false),
        (
            "Ich habe den Bericht zu 90 bis 100 Prozent abgeschlossen.",
            false,
        ),
        (
            "I have completed between 50 and 100 percent of the report.",
            false,
        ),
        ("I have completed 50 of 100 percent of the report.", false),
        (
            "Ich habe den Bericht zu 50 von 100 Prozent abgeschlossen.",
            false,
        ),
        ("I have completed fifty percent of the report.", false),
        (
            "Ich habe den Bericht zu fünfzig Prozent abgeschlossen.",
            false,
        ),
        ("Ich habe den Bericht fast abgeschlossen.", false),
        ("Ich habe den Bericht beinahe abgeschlossen.", false),
        ("Ich habe den Bericht nahezu abgeschlossen.", false),
        ("Ich habe den Bericht teilweise abgeschlossen.", false),
        ("Ich habe den Bericht weitgehend abgeschlossen.", false),
        ("I have completed almost all of the report.", false),
        ("I have completed nearly all of the report.", false),
        ("I have completed the report partly.", false),
        ("I have completed the report partially.", false),
        ("I have completed the report mostly.", false),
        ("Ich habe nichts verschickt.", false),
        ("Wir haben nichts verschickt.", false),
        ("I've sent nothing.", false),
        ("I’ve sent nothing.", false),
        ("We have completed none of the reports.", false),
        ("We've completed none of the reports.", false),
        ("We’ve completed none of the reports.", false),
        ("I have sent neither report.", false),
        ("We've completed neither report.", false),
        ("We’ve completed neither report.", false),
        ("I've not sent the report.", false),
        ("I’ve never sent the report.", false),
        ("We've not completed the report.", false),
        ("We’ve never completed the report.", false),
        ("I haven't sent the report.", false),
        ("We haven’t completed the report.", false),
        ("I've sent the report if approval arrives.", false),
        ("We’ve completed the report?", false),
        ("I ve sent the report.", false),
        ("Ive sent the report.", false),
        ("archiveI've sent the report.", false),
    ];
    let mut mismatches = Vec::new();
    for (quote, allowed) in cases {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut seed, mut request, mut answer) = setup();
        request.transcript_sources[0].text = "I will send the report.".into();
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!("I will send the report.");
        let events = validate::apply(&mut seed, &request, &answer.to_string()).unwrap();
        seed.revision = 1;
        store::append(
            directory.path(),
            &store::Delta::between(&empty, &seed, request.transcript_sources.clone(), events),
        )
        .unwrap();
        let seed = read_state(directory.path()).unwrap();
        let old_checkpoint =
            std::fs::read(directory.path().join("revision-00000001.json")).unwrap();
        let source = Source {
            id: "s_completion".into(),
            source: "microphone".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: quote.into(),
        };
        let mut sources = seed.originals.clone();
        sources.push(source.clone());
        let mut completion = Request::new(
            "r_completion".into(),
            &seed.context,
            &seed,
            seed.entities.clone(),
            std::slice::from_ref(&source),
            sources,
        );
        completion.purpose = "tasks".into();
        answer["requestId"] = json!("r_completion");
        answer["changes"][0]["targetId"] = json!(seed.entities[0].id);
        answer["changes"][0]["targetRevision"] = json!(seed.entities[0].revision);
        answer["changes"][0]["text"] = json!(null);
        answer["changes"][0]["status"] = json!("completed");
        answer["changes"][0]["evidence"] = json!([{"sourceId":"s_completion","quote":quote}]);
        let mut next = seed.clone();
        let result = validate::apply(&mut next, &completion, &answer.to_string());
        if result.is_ok() != allowed {
            mismatches.push(format!("{quote}: allowed={allowed}, result={result:?}"));
            continue;
        }
        if allowed {
            next.revision += 1;
            store::append(
                directory.path(),
                &store::Delta::between(&seed, &next, vec![source], result.unwrap()),
            )
            .unwrap();
            let reopened = read_state(directory.path()).unwrap();
            assert_eq!(reopened.entities[0].status, "completed");
            assert_eq!(reopened.entities[0].text, seed.entities[0].text);
            assert!(seed.entities[0]
                .evidence
                .iter()
                .all(|e| reopened.entities[0].evidence.contains(e)));
            assert!(reopened.entities[0]
                .evidence
                .iter()
                .any(|e| e.source_id == "s_completion" && e.quote == quote));
            assert!(reopened
                .originals
                .iter()
                .any(|s| s.id == "s_completion" && s.text == quote));
            assert_eq!(
                serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap(),
                serde_json::to_vec(&reopened).unwrap()
            );
        } else {
            assert_eq!(
                serde_json::to_vec(&next).unwrap(),
                serde_json::to_vec(&seed).unwrap(),
                "{quote}"
            );
            assert!(!directory.path().join("revision-00000002.json").exists());
        }
        assert_eq!(
            std::fs::read(directory.path().join("revision-00000001.json")).unwrap(),
            old_checkpoint
        );
    }
    assert!(
        mismatches.is_empty(),
        "Completion admission mismatches: {mismatches:?}"
    );
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
fn status_updates_select_valid_inherited_due_witnesses_independently_of_quote_order() {
    assert_valid_inherited_due_witness_order(false);
}

#[test]
fn status_updates_select_binding_inherited_due_witnesses_independently_of_quote_order() {
    assert_valid_inherited_due_witness_order(true);
}

fn assert_valid_inherited_due_witness_order(hedged: bool) {
    for misleading_first in [false, true] {
        let (mut state, mut req, mut answer) = setup();
        req.transcript_sources[0].text = "Robin: Ich übernehme den Bericht.".into();
        answer["changes"][0]["text"] = json!("Bericht liefern");
        answer["changes"][0]["owner"] = json!("Robin");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(req.transcript_sources[0].text);
        validate::apply(&mut state, &req, &answer.to_string()).unwrap();
        for (id, misleading) in [("s_a", misleading_first), ("s_b", !misleading_first)] {
            let text = if misleading && hedged {
                "Ich werde den Bericht vielleicht bis zum 5. Mai liefern.".to_owned()
            } else {
                format!(
                    "Ich werde am {}. Mai den Bericht liefern.",
                    if misleading { 15 } else { 5 }
                )
            };
            req.transcript_sources.push(Source {
                id: id.into(),
                source: "system".into(),
                start_ms: 3000,
                end_ms: 6000,
                text,
            });
        }
        req.state = state.entities.clone();
        let change = &mut answer["changes"][0];
        change["targetId"] = json!(state.entities[0].id);
        change["targetRevision"] = json!(1);
        change["text"] = json!(null);
        change["owner"] = json!(null);
        change["due"] = json!("5. Mai");
        change["evidence"] = json!(req.transcript_sources[1..]
            .iter()
            .map(|s| json!({"sourceId":s.id,"quote":s.text}))
            .collect::<Vec<_>>());
        validate::apply(&mut state, &req, &answer.to_string()).unwrap();
        assert_eq!(state.entities[0].due.as_deref(), Some("5. Mai"));
        let history = serde_json::to_vec(&state.events).unwrap();
        req.state = state.entities.clone();
        req.transcript_sources.push(Source {
            id: "s_done".into(),
            source: "system".into(),
            start_ms: 6000,
            end_ms: 9000,
            text: "Der Bericht ist erledigt.".into(),
        });
        let change = &mut answer["changes"][0];
        change["targetRevision"] = json!(2);
        change["status"] = json!("completed");
        change["due"] = json!(null);
        change["evidence"] = json!([{"sourceId":"s_done","quote":"Der Bericht ist erledigt."}]);
        let result = validate::apply(&mut state, &req, &answer.to_string());
        assert!(
            result.is_ok(),
            "misleading-first={misleading_first}: {result:?}"
        );
        assert_eq!(state.entities[0].status, "completed");
        assert_eq!(state.entities[0].due.as_deref(), Some("5. Mai"));
        assert_eq!(state.entities[0].owner.as_deref(), Some("Robin"));
        assert!(state.entities[0]
            .evidence
            .iter()
            .any(|e| e.quote == "Ich werde am 5. Mai den Bericht liefern."));
        assert_eq!(serde_json::to_vec(&state.events[..2]).unwrap(), history);
    }
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
fn explicit_english_promises_pass_the_task_quote_gate() {
    for quote in [
        "I promise to email the report tomorrow.",
        "We commit to sending the report tomorrow.",
        "I'll email the report tomorrow.",
        "We'll send the report tomorrow.",
        "I’ll email the report tomorrow.",
        "We’ll send the report tomorrow.",
    ] {
        assert!(quote_options(quote)
            .iter()
            .any(|quote| validate::task_quote_allowed(quote)));
    }
    for quote in [
        "I do not promise to email the report tomorrow.",
        "I promise to send the report if the budget is approved.",
        "Do I promise to email the report tomorrow?",
        "I'll not email the report tomorrow.",
        "We'll not send the report tomorrow.",
        "I’ll not email the report tomorrow.",
        "We’ll not send the report tomorrow.",
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
fn task_exclusions_match_words_without_rejecting_unconditional_suffixes() {
    for quote in [
        "Ich schreibe jedenfalls den Bericht bis Freitag.",
        "Ich schreibe ebenfalls den Bericht bis Freitag.",
        "Ich prüfe den Tarif bis Freitag.",
    ] {
        let (mut state, mut request, mut answer) = setup();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        answer["changes"][0]["due"] = serde_json::Value::Null;
        validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        assert_eq!(state.entities[0].kind, "task", "{quote}");
    }
    for quote in [
        "Ich schreibe den Bericht, falls ich Zeit habe.",
        "Ich schreibe den Bericht falls\nich Zeit habe.",
        "Ich schreibe den Bericht, wenn ich Zeit habe.",
        "Ich schreibe den Bericht, sofern ich Zeit habe.",
        "I will write the report if I have time.",
        "I will write the report unless approval is missing.",
        "Niemand übernimmt den Bericht.",
        "Keiner übernimmt den Bericht.",
        "Nobody will handle the report.",
        "Ich schreibe den Bericht?",
        "Die Zusage fehlt, ich schreibe den Bericht noch nicht verbindlich.",
    ] {
        assert!(!validate::task_quote_allowed(quote), "{quote}");
    }
}

#[test]
fn explicit_task_refusals_cannot_publish_new_work_but_affirmations_remain_valid() {
    for (quote, allowed) in [
        ("Ich übernehme den Bericht nicht.", false),
        ("Wir übernehmen den Bericht keineswegs.", false),
        ("Ich schreibe den Bericht niemals.", false),
        ("Ich verspreche nicht, den Bericht zu schreiben.", false),
        ("I do not promise to email the report.", false),
        ("We never take the report.", false),
        ("Ich übernehme den Bericht.", true),
        ("Wir übernehmen den Bericht.", true),
        ("Ich schreibe den Bericht.", true),
        ("Ich werde den Bericht nicht vergessen.", true),
        ("I will not forget to email the report.", true),
        ("Kein Problem, ich übernehme den Bericht.", true),
        ("KEIN PROBLEM – ich übernehme den Bericht.", true),
        ("Keine Sorge, ich schreibe den Bericht.", true),
        ("Not a problem, I will email the report.", true),
        ("Kein Problem: Ich werde den Bericht nicht vergessen.", true),
        ("Kein Problem, ich übernehme den Bericht nicht.", false),
        (
            "Keine Sorge, ich verspreche nicht, den Bericht zu schreiben.",
            false,
        ),
        ("Not a problem, I will not email the report.", false),
        ("Ich übernehme keine Sorgepflicht für den Bericht.", false),
        ("I won’t email the report.", false),
        ("Not a problem, I won’t email the report.", false),
        ("I don't promise to email the report.", false),
        ("I don’t promise to email the report.", false),
        ("I can't take the report.", false),
        ("I can’t take the report.", false),
        ("I cannot write the report.", false),
        ("Don't worry, I can't take the report.", false),
        ("We can't promise that we will send the report.", false),
        ("We can’t promise that we will send the report.", false),
        ("Don't worry, I will email the report.", true),
        ("Do not worry, I will email the report.", true),
        ("Don't forget, I will email the report.", true),
        ("I can't wait, I will email the report.", true),
        ("I cannot wait, I will email the report.", true),
        ("I don't mind, I will email the report.", true),
        ("It doesn't matter, I will email the report.", true),
        ("I can't wait, but I won't email the report.", false),
        ("I cannot wait, but I cannot write the report.", false),
        ("I don't mind, but I won't email the report.", false),
        ("It doesn't matter, I cannot write the report.", false),
    ] {
        let (mut state, mut request, mut answer) = setup();
        let before = serde_json::to_vec(&state).unwrap();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["text"] = json!("Bericht übernehmen");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let result = validate::apply(&mut state, &request, &answer.to_string());
        if allowed {
            result.unwrap_or_else(|e| panic!("{quote}: {e}"));
            assert_eq!(state.entities[0].status, "open");
        } else {
            assert!(result.is_err(), "{quote}");
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn review22_negative_decision_content_is_distinct_from_a_negated_assertion() {
    for (quote, allowed) in [
        ("We decided not to release on Friday.", true),
        ("We agreed not to start the pilot.", true),
        ("Wir haben beschlossen, den Piloten nicht zu starten.", true),
        ("Wir haben beschlossen den Piloten nicht zu starten.", true),
        ("Wir beschließen, dass wir den Piloten nicht starten.", true),
        ("We did not decide to release on Friday.", false),
        ("We haven't agreed not to start the pilot.", false),
        (
            "Wir haben nicht beschlossen, den Piloten zu starten.",
            false,
        ),
        ("Wir beschließen nicht, den Piloten zu starten.", false),
        ("Wir beschließen den Pilotstart nicht.", false),
        ("If approved, we decided not to start the pilot.", false),
    ] {
        let (mut state, mut request, mut answer) = setup();
        let before = serde_json::to_vec(&state).unwrap();
        request.purpose = "facts".into();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["kind"] = json!("decision");
        answer["changes"][0]["text"] = json!("Pilotstart");
        answer["changes"][0]["status"] = json!("recorded");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let result = validate::apply(&mut state, &request, &answer.to_string());
        if allowed {
            result.unwrap_or_else(|e| panic!("{quote}: {e}"));
            assert_eq!(state.entities[0].status, "recorded");
        } else {
            assert!(result.is_err(), "{quote}");
            assert_eq!(serde_json::to_vec(&state).unwrap(), before, "{quote}");
        }
    }
}

#[test]
fn review22_contracted_future_withdrawal_cannot_close_tasks_or_decisions() {
    review22_status_cases(
        "withdrawn",
        &["task", "decision"],
        &[
            ("We'll cancel the report tomorrow.", false),
            ("We’ll withdraw the decision tomorrow.", false),
            ("I'll cancel the report tomorrow.", false),
            ("They’ll withdraw the decision tomorrow.", false),
            ("We will cancel the report tomorrow.", false),
            ("We cancelled the report.", true),
            ("We withdraw the decision.", true),
        ],
    );
}

#[test]
fn review22_denied_completion_preserves_the_open_task_and_its_original_evidence() {
    review22_status_cases(
        "completed",
        &["task"],
        &[
            ("Ich habe den Bericht niemals verschickt.", false),
            ("Ich habe den Bericht keineswegs verschickt.", false),
            ("Ich habe den Bericht nie verschickt.", false),
            ("Nobody has been approved.", false),
            ("Noone has been approved.", false),
            ("Ich habe den Bericht verschickt.", true),
            ("I have sent the report.", true),
        ],
    );
}

fn review22_status_cases(status_name: &str, kinds: &[&str], cases: &[(&str, bool)]) {
    for kind in kinds {
        let (mut state, mut request, mut answer) = setup();
        if *kind == "decision" {
            request.purpose = "facts".into();
            request.transcript_sources[0].text = "Wir beschließen den Pilotstart.".into();
            answer["changes"][0]["kind"] = json!("decision");
            answer["changes"][0]["text"] = json!("Pilotstart beschlossen");
            answer["changes"][0]["status"] = json!("recorded");
            answer["changes"][0]["evidence"][0]["quote"] = json!("Wir beschließen den Pilotstart.");
        }
        answer["changes"][0]["due"] = json!(null);
        validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        for (quote, allowed) in cases {
            let source = Source {
                id: "s_status".into(),
                source: "system".into(),
                start_ms: 3000,
                end_ms: 6000,
                text: (*quote).into(),
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
            status.purpose = if *kind == "task" { "tasks" } else { "facts" }.into();
            let mut change = answer.clone();
            change["requestId"] = json!("r_status");
            change["changes"][0]["targetId"] = json!(state.entities[0].id);
            change["changes"][0]["targetRevision"] = json!(state.entities[0].revision);
            change["changes"][0]["text"] = json!(null);
            change["changes"][0]["status"] = json!(status_name);
            change["changes"][0]["evidence"] = json!([{"sourceId":"s_status","quote":quote}]);
            let mut next = state.clone();
            let result = validate::apply(&mut next, &status, &change.to_string());
            if *allowed {
                result.unwrap_or_else(|e| panic!("{kind}: {quote}: {e}"));
                assert_eq!(next.entities[0].status, status_name);
                assert_eq!(next.entities[0].text, state.entities[0].text);
                assert!(state.entities[0]
                    .evidence
                    .iter()
                    .all(|e| next.entities[0].evidence.contains(e)));
            } else {
                assert!(result.is_err(), "{kind}: {quote}");
                assert_eq!(serde_json::to_vec(&next).unwrap(), before, "{quote}");
            }
        }
    }
}

#[test]
fn recorded_decisions_reject_negation_and_uncertainty_without_publishing_state() {
    let invalid = [
        "Wir haben den Pilotstart nicht beschlossen.",
        "Wir haben keinen Pilotstart beschlossen.",
        "Wir werden morgen entscheiden.",
        "Vielleicht beschließen wir den Pilotstart.",
        "Wenn alles passt, beschließen wir den Pilotstart.",
        "Haben wir den Pilotstart beschlossen?",
        "We have not agreed to start the pilot.",
        "If approved, we decided to start the pilot.",
        "We decided to start the pilot if approved.",
        "We decided to check the report if the tests pass.",
        "We decided to check if the report is ready if the tests pass.",
        "If approved, we decided to check if the report is ready.",
        "We decided to ask if the report is ready unless the owner objects.",
        "We decided to start the pilot unless approval fails.",
        "Wir haben den Pilotstart beschlossen, wenn das Budget kommt.",
        "Wir haben den Pilotstart beschlossen, sofern das Budget kommt.",
        "Wir haben den Pilotstart beschlossen, falls das Budget kommt.",
        "We might have agreed to start the pilot.",
        "We haven't agreed to start the pilot.",
        "Wir müssen noch über den Pilotstart entscheiden.",
        "Das ist ein entscheidender Hinweis auf den Pilotstart.",
        "Nobody agreed to start the pilot.",
        "Noone agreed to start the pilot.",
        "Wir haben keineswegs den Pilotstart beschlossen.",
    ];
    let valid = [
        "Wir beschließen den Pilotstart.",
        "Wir entscheiden uns für den Pilotstart.",
        "Wir haben den Pilotstart beschlossen.",
        "Der Pilotstart ist beschlossen.",
        "We decide to start the pilot.",
        "We decided to start the pilot.",
        "We agreed to start the pilot.",
        "We decided not to start the pilot.",
        "Wir haben beschlossen, den Piloten nicht zu starten.",
        "We decided to ask whether the pilot should start.",
        "We decided to check if the report is ready.",
        "We decided to ask if the report is ready.",
        "We decided to ask if the report is ready and check if the pilot can start.",
        "We agreed not to check if the report is ready.",
        "Wir haben beschlossen, zu prüfen, ob der Pilot starten soll.",
    ];
    let mut accepted_invalid = Vec::new();
    let mut rejected_valid = Vec::new();
    for (quote, allowed) in invalid
        .iter()
        .map(|q| (*q, false))
        .chain(valid.iter().map(|q| (*q, true)))
    {
        let (mut state, mut request, mut answer) = setup();
        let before = serde_json::to_vec(&state).unwrap();
        request.purpose = "facts".into();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["kind"] = json!("decision");
        answer["changes"][0]["text"] = json!("Pilotstart");
        answer["changes"][0]["status"] = json!("recorded");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let result = validate::apply(&mut state, &request, &answer.to_string());
        if allowed {
            if let Err(error) = result {
                rejected_valid.push(format!("{quote}: {error}"));
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            } else {
                assert_eq!(state.entities[0].status, "recorded");
            }
        } else if result.is_ok() {
            accepted_invalid.push(quote);
        } else {
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    assert!(
        rejected_valid.is_empty(),
        "Unconditional decision rejected: {rejected_valid:?}"
    );
    assert!(
        accepted_invalid.is_empty(),
        "Invalid recorded decision accepted: {accepted_invalid:?}"
    );
}

#[test]
fn withdrawal_requires_explicit_current_intent_and_rejection_preserves_both_kinds() {
    let invalid = [
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
    ];
    let valid = [
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
    ];
    let mut accepted_invalid = Vec::new();
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
        validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        for (quote, allowed) in invalid
            .iter()
            .map(|q| (*q, false))
            .chain(valid.iter().map(|q| (*q, true)))
        {
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
            if allowed {
                result.unwrap_or_else(|e| panic!("{kind}: {quote}: {e}"));
                assert_eq!(next.entities[0].status, "withdrawn");
                assert_eq!(next.entities[0].text, state.entities[0].text);
                assert!(state.entities[0]
                    .evidence
                    .iter()
                    .all(|e| next.entities[0].evidence.contains(e)));
            } else if result.is_ok() {
                accepted_invalid.push(format!("{kind}: {quote}"));
            } else {
                assert_eq!(serde_json::to_vec(&next).unwrap(), bytes, "{quote}");
            }
        }
    }
    assert!(
        accepted_invalid.is_empty(),
        "Invalid withdrawal accepted: {accepted_invalid:?}"
    );
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
