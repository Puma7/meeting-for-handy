use super::*;

fn source(id: &str, at: u64, text: &str) -> Source {
    Source {
        id: id.into(),
        source: "microphone".into(),
        start_ms: at,
        end_ms: at + 3000,
        text: text.into(),
    }
}

fn request(state: &WorkspaceState, phase: &str, current: &Source) -> Request {
    let mut originals = state.originals.clone();
    originals.push(current.clone());
    let mut request = Request::new(
        "r_check".into(),
        &state.context,
        state,
        state.entities.clone(),
        std::slice::from_ref(current),
        originals,
    );
    request.purpose = phase.into();
    request
}

fn answer(changes: serde_json::Value) -> String {
    json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_check","changes":changes,
        "overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false})
    .to_string()
}

fn seeded_task() -> WorkspaceState {
    let mut state = WorkspaceState::default();
    let promise = source(
        "s_pledge",
        0,
        "Avery says: I will send the access checklist by Thursday.",
    );
    let request = request(&state, "tasks", &promise);
    validate::apply(&mut state, &request, &answer(json!([{
        "proposalId":"p_new","kind":"task","targetId":null,"targetRevision":null,
        "text":"Send access checklist","status":"open","attention":"normal","owner":"Avery","due":"Thursday",
        "evidence":[{"sourceId":promise.id,"quote":promise.text}],"relatedEntityIds":[]
    }]))).unwrap();
    state.originals.push(promise);
    state
}

#[test]
fn a_fresh_unassigned_topic_cannot_overwrite_an_existing_commitment() {
    let mut state = seeded_task();
    let old = state.entities[0].clone();
    let unassigned = source(
        "s_unassigned",
        3000,
        "Nobody has agreed to do the microphone test.",
    );
    let request = request(&state, "tasks", &unassigned);
    let response = answer(json!([{
        "proposalId":"p_replace","kind":"task","targetId":old.id,"targetRevision":old.revision,
        "text":"Run the microphone test","status":"open","attention":"normal","owner":"Avery","due":null,
        "evidence":[{"sourceId":unassigned.id,"quote":unassigned.text},old.evidence[0]],"relatedEntityIds":[]
    }]));
    assert!(validate::apply(&mut state, &request, &response).is_err());
    assert_eq!(state.entities, vec![old]);
}

#[test]
fn a_missed_completion_can_be_recovered_only_for_a_matching_named_task() {
    let mut state = seeded_task();
    for (index, text) in [
        "Morgan says: I have sent the access checklist.",
        "Avery says: I have not sent the access checklist.",
        "Avery says: I have sent the seating plan.",
        "Avery says: I have sent the safety checklist.",
        "Avery says: Jordan has sent the access checklist.",
    ]
    .into_iter()
    .enumerate()
    {
        let current = source(&format!("s_other_{index}"), 3000, text);
        let request = request(&state, "tasks", &current);
        assert!(
            validate::apply(&mut state, &request, &answer(json!([])))
                .unwrap()
                .is_empty(),
            "{text}"
        );
        assert_eq!(state.entities[0].status, "open", "{text}");
    }

    let original = state.entities[0].clone();
    let unrelated = source(
        "s_wrong_done",
        6000,
        "Avery says: I have sent the safety checklist.",
    );
    let wrong_request = request(&state, "tasks", &unrelated);
    let wrong_response = answer(json!([{
        "proposalId":"p_wrong_done","kind":"task","targetId":original.id,"targetRevision":original.revision,
        "text":null,"status":"completed","attention":"normal","owner":null,"due":null,
        "evidence":[{"sourceId":unrelated.id,"quote":unrelated.text}],"relatedEntityIds":[]
    }]));
    assert!(validate::apply(&mut state, &wrong_request, &wrong_response).is_err());
    assert_eq!(state.entities, vec![original.clone()]);
    let current = source(
        "s_done",
        6000,
        "Avery says: I have sent the access checklist.",
    );
    let req = request(&state, "tasks", &current);
    let mut ambiguous = state.clone();
    let mut alternate = original.clone();
    alternate.id = "e_other".into();
    ambiguous.entities.push(alternate);
    let ambiguous_req = request(&ambiguous, "tasks", &current);
    assert!(
        validate::apply(&mut ambiguous, &ambiguous_req, &answer(json!([])))
            .unwrap()
            .is_empty()
    );

    let events = validate::apply(&mut state, &req, &answer(json!([]))).unwrap();
    assert_eq!(events.len(), 1);
    let completed = &state.entities[0];
    assert_eq!(completed.id, original.id);
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.due, original.due);
    assert_eq!(completed.evidence.len(), 2);
    assert!(completed
        .evidence
        .iter()
        .any(|item| item.quote == current.text));
}

#[test]
fn repeated_questions_keep_the_human_status_and_unrelated_answers_do_not_close_them() {
    let mut state = WorkspaceState::default();
    let budget = source("s_budget", 0, "Wer kann das Versandbudget freigeben?");
    let req = request(&state, "reconcile", &budget);
    let new_impulse = |title: &str, source: &Source| {
        json!({
            "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
            "text":title,"status":"open","attention":"clarification","owner":null,"due":null,
            "evidence":[{"sourceId":source.id,"quote":source.text}],"relatedEntityIds":[]
        })
    };
    validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(
            "Wie wird das Versandbudget freigegeben?",
            &budget
        )])),
    )
    .unwrap();
    state.originals.push(budget);
    state.entities[0].human_locked = true;
    state.entities[0].status = "discussed".into();
    let original = state.entities[0].clone();

    let repeat = source(
        "s_repeat",
        3000,
        "Die Freigabe des Versandbudgets steht weiter aus.",
    );
    let req = request(&state, "reconcile", &repeat);
    assert!(validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(
            "Wie wird das Budget freigegeben?",
            &repeat
        )]))
    )
    .unwrap()
    .is_empty());
    assert_eq!(state.entities, vec![original]);

    let safety = source(
        "s_safety",
        6000,
        "Wer erklärt der Testgruppe die Sicherheitsregeln?",
    );
    let req = request(&state, "reconcile", &safety);
    validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(&safety.text, &safety)])),
    )
    .unwrap();
    state.originals.push(safety);
    let safety = state.entities[1].clone();
    let guide = source(
        "s_guide",
        9000,
        "Nora sagt: Die Anleitung ist fertig. Der Eingang Nord steht in der Anleitung.",
    );
    let req = request(&state, "reconcile", &guide);
    let response = answer(json!([{
        "proposalId":"p_close","kind":"impulse","targetId":safety.id,"targetRevision":safety.revision,
        "text":null,"status":"discussed","attention":"normal","owner":null,"due":null,
        "evidence":[{"sourceId":guide.id,"quote":guide.text}],"relatedEntityIds":[]
    }]));
    assert!(validate::apply(&mut state, &req, &response).is_err());
    assert_eq!(state.entities[1], safety);
}

#[test]
fn translated_repeat_does_not_duplicate_a_confirmed_question() {
    let mut state = WorkspaceState::default();
    let budget = source(
        "s_budget",
        0,
        "Who can approve the printing budget? It still has no approval.",
    );
    let new_impulse = |title: &str, source: &Source| {
        json!({
            "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
            "text":title,"status":"open","attention":"clarification","owner":null,"due":null,
            "evidence":[{"sourceId":source.id,"quote":source.text}],"relatedEntityIds":[]
        })
    };
    let req = request(&state, "reconcile", &budget);
    validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(
            "Who can approve the printing budget?",
            &budget
        )])),
    )
    .unwrap();
    state.originals.push(budget);
    state.entities[0].human_locked = true;
    state.entities[0].status = "discussed".into();
    let original = state.entities[0].clone();
    let repeated = source("s_repeat", 3000, "The printing budget has not changed.");
    let req = request(&state, "reconcile", &repeated);
    assert!(validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(
            "Status des Druckbudgets klären",
            &repeated
        )]))
    )
    .unwrap()
    .is_empty());
    assert_eq!(state.entities, vec![original.clone()]);

    let separate = source("s_separate", 6000, "How much can we spend on printing?");
    let req = request(&state, "reconcile", &separate);
    validate::apply(
        &mut state,
        &req,
        &answer(json!([new_impulse(
            "How much can we spend on printing?",
            &separate
        )])),
    )
    .unwrap();
    assert_eq!(state.entities.len(), 2);
    assert_eq!(state.entities[0], original);
}

#[test]
fn a_second_proposal_with_less_metadata_reuses_the_same_literal_task() {
    let mut state = WorkspaceState::default();
    let promise = source(
        "s_pledge",
        0,
        "Avery says: I will send the access checklist by Thursday.",
    );
    let req = request(&state, "tasks", &promise);
    let rich = json!({
        "proposalId":"p_rich","kind":"task","targetId":null,"targetRevision":null,
        "text":"Send access checklist","status":"open","attention":"normal","owner":"Avery","due":"Thursday",
        "evidence":[{"sourceId":promise.id,"quote":promise.text}],"relatedEntityIds":[]
    });
    let mut redundant = rich.clone();
    redundant["proposalId"] = json!("p_redundant");
    redundant["owner"] = json!(null);
    redundant["due"] = json!(null);
    let events = validate::apply(&mut state, &req, &answer(json!([rich, redundant]))).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(state.entities.len(), 1);
    assert_eq!(state.entities[0].owner.as_deref(), Some("Avery"));
    assert_eq!(state.entities[0].due.as_deref(), Some("Thursday"));
}

#[test]
fn identical_status_echoes_commit_once_but_conflicting_changes_are_atomic() {
    let mut state = seeded_task();
    let old = state.entities[0].clone();
    let current = source(
        "s_done",
        3000,
        "Avery says: I have sent the access checklist.",
    );
    let req = request(&state, "tasks", &current);
    let status = json!({
        "proposalId":"p_first","kind":"task","targetId":old.id,"targetRevision":old.revision,
        "text":null,"status":"completed","attention":"normal","owner":null,"due":null,
        "evidence":[{"sourceId":current.id,"quote":current.text}],"relatedEntityIds":[]
    });
    let mut echo = status.clone();
    echo["proposalId"] = json!("p_echo");
    let mut conflict = echo.clone();
    conflict["status"] = json!("open");
    assert!(validate::apply(&mut state, &req, &answer(json!([status, conflict]))).is_err());
    assert_eq!(state.entities, vec![old.clone()]);
    let events = validate::apply(&mut state, &req, &answer(json!([status, echo]))).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(state.entities[0].status, "completed");
    assert_eq!(state.entities[0].revision, old.revision + 1);
}

#[test]
fn a_promise_alone_cannot_create_an_open_impulse() {
    let mut state = WorkspaceState::default();
    let proposal = |witness: &Source| {
        answer(json!([{
            "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
            "text":"Who will send the access checklist?","status":"open","attention":"clarification",
            "owner":null,"due":null,
            "evidence":[{"sourceId":witness.id,"quote":witness.text}],"relatedEntityIds":[]
        }]))
    };
    let pledge = source(
        "s_pledge",
        0,
        "Avery says: I will send the access checklist by Thursday.",
    );
    let req = request(&state, "reconcile", &pledge);
    assert!(validate::apply(&mut state, &req, &proposal(&pledge)).is_err());
    assert!(state.entities.is_empty());

    let open = source(
        "s_open",
        3000,
        "Nobody has agreed to send the access checklist.",
    );
    let req = request(&state, "reconcile", &open);
    assert_eq!(
        validate::apply(&mut state, &req, &proposal(&open))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(state.entities[0].status, "open");
}

#[test]
fn an_unrelated_decision_cannot_supply_the_quote_for_an_open_question() {
    let mut state = WorkspaceState::default();
    let decision = source(
        "s_room",
        0,
        "We decided to hold the workshop in Room Blue. The old room was only an option.",
    );
    let question = source(
        "s_budget",
        3000,
        "Who can approve the printing budget? The printing budget still has no approval.",
    );
    let unrelated_open_topic = source(
        "s_website",
        6000,
        "I do not know who will rebuild the website.",
    );
    assert!(validate::explicit_question(&unrelated_open_topic.text));
    let fresh = [
        decision.clone(),
        question.clone(),
        unrelated_open_topic.clone(),
    ];
    let mut req = Request::new(
        "r_check".into(),
        &WorkspaceContext::default(),
        &state,
        vec![],
        &fresh,
        fresh.to_vec(),
    );
    req.purpose = "reconcile".into();
    let proposal = |witness: &Source| {
        answer(json!([{
            "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
            "text":"Who can approve the printing budget?","status":"open","attention":"clarification",
            "owner":null,"due":null,
            "evidence":[{"sourceId":witness.id,"quote":witness.text}],"relatedEntityIds":[]
        }]))
    };
    assert!(validate::apply(&mut state, &req, &proposal(&decision)).is_err());
    assert!(validate::apply(&mut state, &req, &proposal(&unrelated_open_topic)).is_err());
    assert!(state.entities.is_empty());
    assert_eq!(
        validate::apply(&mut state, &req, &proposal(&question))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn an_open_question_cannot_borrow_a_binding_quote_to_become_a_task() {
    let mut state = WorkspaceState::default();
    let pledge = source(
        "s_pledge",
        0,
        "Avery says: I will send the access checklist by Thursday.",
    );
    let req = request(&state, "tasks", &pledge);
    let good = json!({
        "proposalId":"p_task","kind":"task","targetId":null,"targetRevision":null,
        "text":"Send the access checklist","status":"open","attention":"normal","owner":"Avery","due":"Thursday",
        "evidence":[{"sourceId":pledge.id,"quote":pledge.text}],"relatedEntityIds":[]
    });
    let mut false_task = good.clone();
    false_task["proposalId"] = json!("p_budget");
    false_task["text"] = json!("Who can approve the printing budget?");
    false_task["owner"] = json!(null);
    false_task["due"] = json!(null);
    assert!(validate::apply(&mut state, &req, &answer(json!([good, false_task]))).is_err());
    assert!(state.entities.is_empty());
    assert_eq!(
        validate::apply(&mut state, &req, &answer(json!([good])))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn task_keeps_its_own_sentence_and_recovers_only_an_explicit_weekday() {
    let src = source("s_work", 0,
        "Für den Nordstern-Pilot prüfen wir heute den Versand und die Abnahme. Robin sagt: Ich schreibe den Versandbericht bis Freitag.");
    let mut state = WorkspaceState::default();
    let mut req = request(&state, "tasks", &src);
    req.language = "auto".into();
    let proposal = |title: &str, witness: &Source| {
        answer(json!([{
            "proposalId":"p_report", "kind":"task", "targetId":null, "targetRevision":null,
            "text":title, "status":"open", "attention":"normal", "owner":null, "due":null,
            "evidence":[{"sourceId":witness.id,"quote":witness.text}], "relatedEntityIds":[]
        }]))
    };
    assert!(validate::apply(
        &mut state,
        &req,
        &proposal("Versand und Abnahme prüfen", &src)
    )
    .is_err());
    assert!(state.entities.is_empty());
    validate::apply(
        &mut state,
        &req,
        &proposal("Versandbericht schreiben", &src),
    )
    .unwrap();
    assert_eq!(state.entities[0].due.as_deref(), Some("Freitag"));
    assert_eq!(
        state.entities[0].evidence[0].quote,
        "Robin sagt: Ich schreibe den Versandbericht bis Freitag."
    );
    assert_eq!(
        crate::validation::explicit_task_weekday(
            "Casey says: I will email the seating plan by Wednesday."
        ),
        Some("Wednesday")
    );
    for quote in [
        "I will send the report by Thursday or Friday.",
        "I will send the report, probably by Friday.",
        "I will send the report by Friday and email the plan by Monday.",
        "I will send the report by 12 Friday.",
        "Wir haben bis Freitag noch keine Zusage.",
    ] {
        assert_eq!(
            crate::validation::explicit_task_weekday(quote),
            None,
            "{quote}"
        );
    }
}

#[test]
fn decisions_do_not_borrow_another_decision_and_keep_the_complete_calendar_quote() {
    let src = source(
        "s_mix",
        0,
        "We did not decide to hire a photographer. We decided to hold the workshop in Room Blue.",
    );
    let mut state = WorkspaceState::default();
    let mut req = request(&state, "facts", &src);
    req.language = "auto".into();
    let decision = |title: &str, id: &str, quote: &str| {
        answer(json!([{
            "proposalId":"p_decision", "kind":"decision", "targetId":null, "targetRevision":null,
            "text":title, "status":"recorded", "attention":"normal", "owner":null, "due":null,
            "evidence":[{"sourceId":id,"quote":quote}], "relatedEntityIds":[]
        }]))
    };
    assert!(validate::apply(
        &mut state,
        &req,
        &decision("Hire a photographer", &src.id, &src.text)
    )
    .is_err());
    validate::apply(
        &mut state,
        &req,
        &decision("Hold the workshop in Room Blue", &src.id, &src.text),
    )
    .unwrap();
    assert_eq!(
        state.entities[0].evidence[0].quote,
        "We decided to hold the workshop in Room Blue."
    );

    let src = source("s_date", 3000,
        "Vielleicht erneuern wir die Webseite. Wir entscheiden uns für den Pilotstart am 12. November.");
    let mut req = request(&state, "facts", &src);
    req.language = "auto".into();
    assert!(quote_options(&src.text)
        .contains(&"Wir entscheiden uns für den Pilotstart am 12. November.".to_owned()));
    assert!(!quote_options(&src.text)
        .iter()
        .any(|q| q.ends_with("am 12.")));
    validate::apply(
        &mut state,
        &req,
        &decision(
            "Pilotstart am 12. November",
            &src.id,
            "Wir entscheiden uns für den Pilotstart am 12.",
        ),
    )
    .unwrap();
    assert_eq!(
        state.entities[1].evidence[0].quote,
        "Wir entscheiden uns für den Pilotstart am 12. November."
    );
}

#[test]
fn impulse_keeps_a_matching_original_quote_without_an_unrelated_citation() {
    let room = source("s_room", 0, "研讨会改在蓝色会议室举行。");
    let question = source("s_projector", 3000, "备用投影仪由谁负责还没有确定。");
    let fresh = [room.clone(), question.clone()];
    let mut state = WorkspaceState::default();
    let mut req = Request::new(
        "r_check".into(),
        &state.context,
        &state,
        vec![],
        &fresh,
        fresh.to_vec(),
    );
    req.purpose = "reconcile".into();
    let mut proposal = json!({
        "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
        "text":"谁负责备用投影仪？","status":"open","attention":"clarification","owner":null,"due":null,
        "evidence":[{"sourceId":room.id,"quote":room.text},{"sourceId":question.id,"quote":question.text}],
        "relatedEntityIds":[]
    });
    let valid = proposal.clone();
    proposal["evidence"][0]["quote"] = json!("a fabricated room citation");
    assert!(validate::apply(&mut state, &req, &answer(json!([proposal]))).is_err());
    assert!(state.entities.is_empty());
    validate::apply(&mut state, &req, &answer(json!([valid]))).unwrap();
    assert_eq!(state.entities[0].evidence.len(), 1);
    assert_eq!(state.entities[0].evidence[0].quote, question.text);
}

#[test]
fn forced_first_impulse_prefers_a_literal_question_with_a_matching_open_answer() {
    let budget = source(
        "s_budget",
        0,
        "Wer kann das Versandbudget freigeben? Das Versandbudget ist noch nicht freigegeben.",
    );
    let website = source(
        "s_website",
        3000,
        "Vielleicht könnten wir die Webseite erneuern. Das ist keine Zusage.",
    );
    let fresh = [budget.clone(), website.clone()];
    let mut state = WorkspaceState::default();
    let mut req = Request::new(
        "r_check".into(),
        &state.context,
        &state,
        vec![],
        &fresh,
        fresh.to_vec(),
    );
    req.purpose = "facts".into();
    let acknowledged = answer(json!([{
        "proposalId":"p_website","kind":"fact","targetId":null,"targetRevision":null,
        "text":"Webseiten-Erneuerung bleibt eine Idee","status":"unclear","attention":"normal","owner":null,"due":null,
        "evidence":[{"sourceId":website.id,"quote":website.text}],"relatedEntityIds":[]
    }]));
    validate::apply(&mut state, &req, &acknowledged).unwrap();
    req.purpose = "reconcile".into();
    assert_eq!(
        validate::initial_question_evidence(&state, &req),
        vec![crate::Evidence {
            source_id: budget.id,
            quote: "Wer kann das Versandbudget freigeben?".into()
        }]
    );
}

#[test]
fn an_unrelated_status_quote_cannot_accumulate_on_an_existing_question() {
    let original = source("s_website", 0, "Wird die Webseite im Dezember erneuert?");
    let north = source("s_entrance", 3000, "Wir entscheiden uns für Eingang Nord.");
    let mut state = WorkspaceState::default();
    let mut req = request(&state, "reconcile", &original);
    req.language = "auto".into();
    let mut proposal = json!({
        "proposalId":"p_question","kind":"impulse","targetId":null,"targetRevision":null,
        "text":original.text,"status":"open","attention":"clarification","owner":null,"due":null,
        "evidence":[{"sourceId":original.id,"quote":original.text}],"relatedEntityIds":[]
    });
    validate::apply(&mut state, &req, &answer(json!([proposal.clone()]))).unwrap();
    state.originals.push(original);
    let first = state.entities[0].clone();
    req = request(&state, "reconcile", &north);
    req.language = "auto".into();
    proposal["targetId"] = json!(first.id);
    proposal["targetRevision"] = json!(first.revision);
    proposal["text"] = json!(null);
    proposal["status"] = json!("unclear");
    proposal["evidence"] = json!([{"sourceId":north.id,"quote":north.text}]);
    assert!(
        validate::apply(&mut state, &req, &answer(json!([proposal])))
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.entities, vec![first]);
}
