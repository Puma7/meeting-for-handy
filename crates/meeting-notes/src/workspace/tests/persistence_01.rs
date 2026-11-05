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
    for replace_text in [false, true] {
        assert_status_only_evidence_history("impulse", replace_text);
    }
}

#[test]
fn status_only_evidence_stays_bounded_for_tasks_with_literal_metadata() {
    for replace_text in [false, true] {
        assert_status_only_evidence_history("task", replace_text);
    }
}

fn assert_status_only_evidence_history(kind: &str, replace_text: bool) {
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
    change["evidence"][0]["quote"] = json!(original);
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
