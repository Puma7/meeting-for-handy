use super::*;

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
        endpoint: "http://127.0.0.1:1/v1/".into(),
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
