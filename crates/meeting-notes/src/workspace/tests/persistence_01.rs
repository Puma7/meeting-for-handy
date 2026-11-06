use super::*;

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
fn legacy_context_times_remain_readable_but_malformed_times_are_preserved_as_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    drop(runner);
    let path = dir.path().join("context-00000001.json");
    let mut entry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    entry.as_object_mut().unwrap().remove("createdAt");
    std::fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();
    let old = read_state(dir.path()).unwrap();
    assert!(old.activity_at().is_none());
    assert!(serde_json::to_value(&old)
        .unwrap()
        .get("contextUpdatedAt")
        .is_none());
    entry["createdAt"] = json!("not a timestamp");
    let bytes = serde_json::to_vec(&entry).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(read_state(dir.path()).is_err());
    assert!(update_context(dir.path(), &old.context).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn corrupt_persisted_entity_and_event_evidence_is_rejected_without_rewriting_files() {
    for target in ["entities", "events"] {
        for corruption in ["unknown-source", "nonliteral-quote", "changed-original"] {
            let dir = tempfile::tempdir().unwrap();
            let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
            runner.set_context(Default::default()).unwrap();
            let (mut state, req, answer) = setup();
            validate::apply(&mut state, &req, &answer.to_string()).unwrap();
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
            drop(runner);
            assert!(read_state(dir.path()).is_ok());
            let path = dir.path().join("revision-00000001.json");
            let mut delta: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            match corruption {
                "unknown-source" => {
                    delta[target][0]["evidence"][0]["sourceId"] = json!("s_missing")
                }
                "nonliteral-quote" => {
                    delta[target][0]["evidence"][0]["quote"] =
                        json!("This quote never appeared in the original.")
                }
                _ => delta["originals"][0]["text"] = json!("Entirely different original text."),
            };
            let bytes = serde_json::to_vec(&delta).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(read_state(dir.path()).is_err(), "{target}: {corruption}");
            assert!(WorkspaceRunner::open(dir.path()).is_err());
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
}

#[test]
fn dropping_writer_releases_its_lease_even_with_a_duplicated_handle() {
    let dir = tempfile::tempdir().unwrap();
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    let duplicate = runner._writer.try_clone().unwrap();
    assert!(WorkspaceRunner::open(dir.path()).is_err());
    drop(runner);
    let reopened = WorkspaceRunner::open(dir.path()).unwrap();
    assert!(WorkspaceRunner::open(dir.path()).is_err());
    drop(duplicate);
    assert!(WorkspaceRunner::open(dir.path()).is_err());
    drop(reopened);
}

#[test]
fn status_only_evidence_stays_bounded_for_impulses_with_durable_history() {
    for initial_quotes in [1, 6] {
        for replace_text in [false, true] {
            assert_status_only_evidence_history("impulse", replace_text, initial_quotes);
        }
    }
}

#[test]
fn status_only_evidence_stays_bounded_for_tasks_with_literal_metadata() {
    for initial_quotes in [1, 6] {
        for replace_text in [false, true] {
            assert_status_only_evidence_history("task", replace_text, initial_quotes);
        }
    }
}

fn assert_status_only_evidence_history(kind: &str, replace_text: bool, initial_quotes: usize) {
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let context = runner.context.lock().unwrap().clone();
    let (_, mut req, mut answer) = setup();
    let original = if kind == "impulse" {
        "Who approves phase 7?"
    } else {
        "Robin: Ich übernehme den Bericht bis Freitag."
    };
    req.purpose = if kind == "impulse" {
        "reconcile"
    } else {
        "tasks"
    }
    .into();
    req.transcript_sources[0].text = original.into();
    let mut initial_evidence = vec![json!({"sourceId":"s_one","quote":original})];
    for label in ["alpha", "bravo", "charlie", "delta", "echo"]
        .into_iter()
        .take(initial_quotes - 1)
    {
        let quote = format!("Zulu review checkpoint {label} was mentioned.");
        req.transcript_sources[0]
            .text
            .push_str(&format!(" {quote}"));
        initial_evidence.push(json!({"sourceId":"s_one","quote":quote}));
    }
    let change = &mut answer["changes"][0];
    change["kind"] = json!(kind);
    change["text"] = json!(if kind == "impulse" {
        original
    } else {
        "Bericht liefern"
    });
    change["owner"] = if kind == "task" {
        json!("Robin")
    } else {
        json!(null)
    };
    change["due"] = if kind == "task" {
        json!("Freitag")
    } else {
        json!(null)
    };
    change["evidence"] = json!(initial_evidence);
    answer["overviewRefs"] = json!([]);
    let mut next = runner.state.clone();
    let events = validate::apply(&mut next, &req, &answer.to_string()).unwrap();
    next.originals = req.transcript_sources.clone();
    runner
        .commit(
            next,
            req.transcript_sources.clone(),
            events,
            &AnalysisControl::default(),
            &context,
        )
        .unwrap();
    let id = runner.state.entities[0].id.clone();
    let original_event = serde_json::to_vec(&runner.state.events[0]).unwrap();
    assert_eq!(runner.state.events[0].evidence.len(), initial_quotes);
    let mut files = std::collections::BTreeMap::new();
    let mut all_sources = req.transcript_sources.clone();
    let mut text_source_id = "s_one";
    let mut current_text_quote = original;
    if replace_text {
        let source = Source {
            id: "s_replacement".into(),
            source: "system".into(),
            start_ms: 1000,
            end_ms: 2000,
            text: if kind == "impulse" {
                "Who approves phase 8?"
            } else {
                "Robin: Ich übernehme den Bericht 8 bis Freitag."
            }
            .into(),
        };
        current_text_quote = if kind == "impulse" {
            "Who approves phase 8?"
        } else {
            "Robin: Ich übernehme den Bericht 8 bis Freitag."
        };
        text_source_id = "s_replacement";
        all_sources.push(source.clone());
        req = Request::new(
            "r_replace".into(),
            &context,
            &runner.state,
            runner.state.entities.clone(),
            std::slice::from_ref(&source),
            all_sources.clone(),
        );
        req.purpose = if kind == "impulse" {
            "reconcile"
        } else {
            "tasks"
        }
        .into();
        answer["requestId"] = json!(req.request_id);
        let change = &mut answer["changes"][0];
        change["targetId"] = json!(id);
        change["targetRevision"] = json!(1);
        change["text"] = json!(if kind == "impulse" {
            "Who approves phase 8?"
        } else {
            "Bericht 8 liefern"
        });
        change["evidence"] = json!([{"sourceId":source.id,"quote":source.text}]);
        let mut next = runner.state.clone();
        let events = validate::apply(&mut next, &req, &answer.to_string()).unwrap();
        next.originals = all_sources.clone();
        runner
            .commit(
                next,
                vec![source],
                events,
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
    }
    let labels = ["alpha", "bravo", "charlie", "delta", "echo", "final"];
    for (i, label) in labels.iter().enumerate() {
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("revision-")
            {
                files
                    .entry(path.clone())
                    .or_insert_with(|| std::fs::read(path).unwrap());
            }
        }
        let quote = if i == 5 {
            if kind == "impulse" {
                "The approval question was discussed and resolved.".to_owned()
            } else {
                "Robin: Der Bericht ist erledigt.".to_owned()
            }
        } else if kind == "impulse" {
            format!("The approval remains unclear in checkpoint {label}.")
        } else {
            format!("Robin: Ich übernehme weiterhin den Bericht, checkpoint {label}.")
        };
        let source = Source {
            id: format!("s_update_{label}"),
            source: "system".into(),
            start_ms: (i as u64 + 1) * 3000,
            end_ms: (i as u64 + 2) * 3000,
            text: quote.clone(),
        };
        all_sources.push(source.clone());
        req = Request::new(
            format!("r_update_{label}"),
            &context,
            &runner.state,
            runner.state.entities.clone(),
            std::slice::from_ref(&source),
            all_sources.clone(),
        );
        req.purpose = if kind == "impulse" {
            "reconcile"
        } else {
            "tasks"
        }
        .into();
        answer["requestId"] = json!(req.request_id);
        let change = &mut answer["changes"][0];
        change["targetId"] = json!(id);
        change["targetRevision"] = json!(runner.state.entities[0].revision);
        change["text"] = json!(null);
        change["owner"] = json!(null);
        change["due"] = json!(null);
        change["status"] = json!(if i == 5 {
            if kind == "impulse" {
                "discussed"
            } else {
                "completed"
            }
        } else if kind == "impulse" {
            "unclear"
        } else {
            "open"
        });
        change["evidence"] = json!([{"sourceId":source.id,"quote":quote}]);
        let mut next = runner.state.clone();
        let result = validate::apply(&mut next, &req, &answer.to_string());
        assert!(result.is_ok(), "{kind}/{label}: {result:?}");
        next.originals = all_sources.clone();
        runner
            .commit(
                next,
                vec![source.clone()],
                result.unwrap(),
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_vec(&runner.state.events[0]).unwrap(),
            original_event
        );
        for (path, bytes) in &files {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
        }
        let expected = serde_json::to_vec(&runner.state).unwrap();
        drop(runner);
        runner = WorkspaceRunner::open(dir.path()).unwrap();
        assert_eq!(serde_json::to_vec(&runner.state).unwrap(), expected);
    }
    let current = &runner.state.entities[0];
    assert_eq!(current.id, id);
    assert_eq!(current.revision, 7 + u64::from(replace_text));
    assert_eq!(current.evidence.len(), 2);
    assert!(current
        .evidence
        .iter()
        .any(|e| e.quote == current_text_quote));
    assert_eq!(
        current.text,
        if replace_text {
            if kind == "impulse" {
                "Who approves phase 8?"
            } else {
                "Bericht 8 liefern"
            }
        } else if kind == "impulse" {
            original
        } else {
            "Bericht liefern"
        }
    );
    assert!(current
        .evidence
        .iter()
        .any(|e| e.source_id == "s_update_final"));
    if kind == "task" {
        assert_eq!(current.owner.as_deref(), Some("Robin"));
        assert_eq!(current.due.as_deref(), Some("Freitag"));
    }
    assert_eq!(runner.state.originals, all_sources);
    assert_eq!(runner.state.events.len(), 7 + usize::from(replace_text));
    for label in labels {
        assert!(runner.state.events.iter().any(|e| e
            .evidence
            .iter()
            .any(|v| v.source_id == format!("s_update_{label}"))));
    }
    req.state = runner.state.entities.clone();
    answer["changes"][0]["targetRevision"] = json!(current.revision);
    let valid = answer.clone();
    let before = serde_json::to_vec(&runner.state).unwrap();
    for invalid in [
        "empty",
        "unknown",
        "nonliteral",
        "too many",
        "missing text source",
    ] {
        let mut bad = valid.clone();
        let mut bad_req = Request::new(
            req.request_id.clone(),
            &context,
            &runner.state,
            req.state.clone(),
            &[],
            req.transcript_sources.clone(),
        );
        bad_req.purpose = req.purpose.clone();
        match invalid {
            "empty" => bad["changes"][0]["evidence"] = json!([]),
            "unknown" => bad["changes"][0]["evidence"][0]["sourceId"] = json!("s_unknown"),
            "nonliteral" => {
                bad["changes"][0]["evidence"][0]["quote"] = json!("This sentence was never spoken.")
            }
            "too many" => {
                bad["changes"][0]["evidence"] =
                    json!(vec![bad["changes"][0]["evidence"][0].clone(); 7])
            }
            _ => bad_req
                .transcript_sources
                .retain(|s| s.id != text_source_id),
        }
        let mut next = runner.state.clone();
        assert!(
            validate::apply(&mut next, &bad_req, &bad.to_string()).is_err(),
            "{kind}/{invalid}"
        );
        assert_eq!(serde_json::to_vec(&next).unwrap(), before);
        for (path, bytes) in &files {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
        }
    }
}

#[test]
fn six_initial_quotes_select_required_number_witnesses_without_raising_the_budget() {
    for required in [2, 6] {
        let (mut state, mut req, mut answer) = setup();
        req.purpose = "reconcile".into();
        let quotes = (1..=6)
            .map(|n| format!("Phase {n} needs a review."))
            .collect::<Vec<_>>();
        req.transcript_sources[0].text = quotes.join(" ");
        let numbers = (1..=required)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let change = &mut answer["changes"][0];
        change["kind"] = json!("impulse");
        change["text"] = json!(format!("Who reviews phases {numbers}?"));
        change["due"] = json!(null);
        change["evidence"] = json!(quotes
            .iter()
            .map(|q| json!({"sourceId":"s_one","quote":q}))
            .collect::<Vec<_>>());
        answer["overviewRefs"] = json!([]);
        validate::apply(&mut state, &req, &answer.to_string()).unwrap();
        let history = serde_json::to_vec(&state.events).unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        req.state = state.entities.clone();
        req.transcript_sources.push(Source {
            id: "s_done".into(),
            source: "system".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: "The phase review was discussed and resolved.".into(),
        });
        let change = &mut answer["changes"][0];
        change["targetId"] = json!(state.entities[0].id);
        change["targetRevision"] = json!(1);
        change["text"] = json!(null);
        change["status"] = json!("discussed");
        change["evidence"] =
            json!([{"sourceId":"s_done","quote":"The phase review was discussed and resolved."}]);
        let result = validate::apply(&mut state, &req, &answer.to_string());
        if required == 6 {
            assert!(result.is_err(), "Six independently required text quotes plus a new status cannot fit the unchanged limit");
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        } else {
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(state.entities[0].evidence.len(), 3);
            for quote in &quotes[..2] {
                assert!(state.entities[0].evidence.iter().any(|e| &e.quote == quote));
            }
            assert!(state.entities[0]
                .evidence
                .iter()
                .any(|e| e.source_id == "s_done"));
            assert_eq!(serde_json::to_vec(&state.events[..1]).unwrap(), history);
        }
    }
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
