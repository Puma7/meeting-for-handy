use super::*;

#[test]
/// The previous prompt stays readable but cannot acquire new answers from changed instructions.
fn previous_quality_prompt_is_read_only_and_keeps_its_saved_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("workspace-v10");
    std::fs::create_dir(&folder).unwrap();
    let state = WorkspaceState {
        revision: 1,
        ..Default::default()
    };
    let mut delta = serde_json::to_value(store::Delta::between(
        &WorkspaceState::default(),
        &state,
        vec![],
        vec![],
    ))
    .unwrap();
    delta["schema"] = json!("handymeeting-workspace-10");
    let bytes = serde_json::to_vec(&delta).unwrap();
    let path = folder.join("revision-00000001.json");
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(
        read_state(&folder).unwrap().prompt_version(),
        "handymeeting-workspace-10"
    );
    assert_eq!(WorkspaceRunner::open(&folder).err().unwrap().to_string(),
        "Ältere Meetingfassungen werden nur gelesen. Eine neue Auswertung mit den aktuellen Prompts starten.");
    assert_eq!(
        crate::live::workspace_folder(dir.path()).unwrap(),
        Some(folder)
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
/// Speaker labels must not turn a request or an unknown actor into an assignment.
fn speaker_labels_preserve_pledges_but_do_not_bypass_assignment_screening() {
    assert_quality_assignment_cases(&[
        ("Robin: Bitte holt die Unterlagen ab.", false),
        ("Robin: Someone will collect the equipment.", false),
        ("Robin: Someone will prepare the documents.", false),
        ("Robin says: Someone will prepare the documents.", false),
        ("Robin sagt: Bitte holt die Unterlagen ab.", false),
        ("sp_abc1: Someone will prepare the documents.", false),
        ("Robin: Probably Jana will collect the equipment.", false),
        ("Robin: Ich übernehme die Unterlagen.", true),
        ("Robin: I will collect the equipment.", true),
        ("Robin says: I will collect the equipment.", true),
        ("Robin: Jana holt die Unterlagen ab.", true),
        ("Sprecher 1: Jana holt die Unterlagen ab.", true),
        ("Ich übernehme nichts: I will write the report.", false),
    ]);
}

#[test]
/// Predicate uncertainty blocks work; an uncertain deadline alone keeps the task undated.
fn hedged_explicit_pledges_do_not_publish_tasks() {
    assert_quality_assignment_cases(&[
        ("Ich werde wahrscheinlich den Bericht schreiben.", false),
        ("I will probably write the report.", false),
        ("Ich werde hoffentlich den Bericht schreiben.", false),
        ("I will hopefully write the report.", false),
        (
            "Ich werde den Bericht schreiben, wahrscheinlich bis Freitag.",
            true,
        ),
        ("I will write the report, probably by Friday.", true),
        ("Ich werde den Bericht schreiben.", true),
        ("I will write the report.", true),
    ]);
}

/// Check native publication atomically and preserve literal evidence on admitted work.
fn assert_quality_assignment_cases(cases: &[(&str, bool)]) {
    let mut mismatches = Vec::new();
    for &(quote, allowed) in cases {
        let (mut state, mut request, mut answer) = setup();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["text"] = json!("Unterlagen prüfen");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["owner"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let before = serde_json::to_vec(&state).unwrap();
        let result = validate::apply(&mut state, &request, &answer.to_string());
        if result.is_ok() != allowed || validate::task_quote_allowed(quote) != allowed {
            mismatches.push(format!("{quote}: {result:?}"));
        } else if allowed {
            assert_eq!(state.entities[0].evidence[0].quote, quote);
        } else {
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
/// A promised safeguard can concern nothing being missed, without promising inaction.
fn positive_safeguards_keep_tasks_without_accepting_negative_objects() {
    assert_quality_assignment_cases(&[
        ("I will make sure nothing is missed.", true),
        ("I will ensure nothing is missed.", true),
        ("Ich werde darauf achten, nichts zu übersehen.", true),
        ("Ich werde sicherstellen, dass nichts übersehen wird.", true),
        ("I will collect nothing.", false),
        ("Ich übernehme nichts.", false),
        ("I will not make sure nothing is missed.", false),
        (
            "I will make sure nothing is missed, but I will not do the check.",
            false,
        ),
        (
            "Ich werde darauf achten, nichts zu übersehen, aber ich übernehme nichts.",
            false,
        ),
    ]);
}

#[test]
/// Reject speaker wishes and requests without excluding explicit named/pronominal work.
fn hedges_requests_and_indefinite_people_are_not_assignment_subjects() {
    for (quote, allowed) in [
        ("Wahrscheinlich holt Jana die Technik ab.", false),
        ("Hoffentlich bringt Jana die Unterlagen mit.", false),
        ("Bitte holt die Unterlagen ab.", false),
        ("Someone will collect the equipment.", false),
        ("Someone will prepare the documents.", false),
        ("Please bring the documents.", false),
        ("Probably Jana will collect the equipment.", false),
        ("Jana holt die Technik ab.", true),
        ("Der Leon bringt die Unterlagen mit.", true),
        ("Sie bringt die Unterlagen mit.", true),
        ("She brings the documents.", true),
        ("Jana will collect the equipment.", true),
    ] {
        assert_eq!(validate::task_quote_allowed(quote), allowed, "{quote}");
        let (mut state, mut request, mut answer) = setup();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["text"] = json!(quote);
        answer["changes"][0]["owner"] = json!(null);
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        assert_eq!(
            validate::apply(&mut state, &request, &answer.to_string()).is_ok(),
            allowed,
            "{quote}"
        );
        assert_eq!(state.entities.is_empty(), !allowed, "{quote}");
    }
}

#[test]
/// A negative object must leave the complete native task state unpublished.
fn negated_collection_pronouns_cannot_publish_new_tasks() {
    for (quote, allowed) in [
        ("Jana holt nichts ab.", false),
        ("Jana bringt nichts mit.", false),
        ("Jana brings nothing.", false),
        ("Ich übernehme nichts.", false),
        ("I will collect nothing.", false),
        ("Jana holt die Unterlagen ab.", true),
        ("Jana brings the documents.", true),
        ("I will check whether nothing is missing.", true),
    ] {
        assert_eq!(validate::task_quote_allowed(quote), allowed, "{quote}");
        let (mut state, mut request, mut answer) = setup();
        request.transcript_sources[0].text = quote.into();
        answer["changes"][0]["text"] = json!(quote);
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["owner"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let result = validate::apply(&mut state, &request, &answer.to_string());
        assert_eq!(result.is_ok(), allowed, "{quote}: {result:?}");
        assert_eq!(state.entities.is_empty(), !allowed, "{quote}");
    }
}

#[test]
/// An unrelated but literal suggestion cannot satisfy an acknowledged question's evidence.
fn unrelated_impulse_cannot_replace_an_acknowledged_open_question() {
    let (mut state, mut req, mut answer) = setup();
    let question = "Ich weiß nicht, wer das Budget freigeben kann.";
    let suggestion = "Vielleicht könnten wir die Webseite erneuern.";
    req.transcript_sources[0].text = format!("{question} {suggestion}");
    req.purpose = "facts".into();
    answer["changes"][0]["kind"] = json!("fact");
    answer["changes"][0]["status"] = json!("unclear");
    answer["changes"][0]["text"] = json!("Budgetfreigabe unklar");
    answer["changes"][0]["owner"] = json!(null);
    answer["changes"][0]["due"] = json!(null);
    answer["changes"][0]["evidence"][0]["quote"] = json!(question);
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let before = serde_json::to_vec(&state).unwrap();
    let mut reconcile = Request::new(
        "r_question".into(),
        &Default::default(),
        &state,
        state.entities.clone(),
        &req.transcript_sources,
        req.transcript_sources.clone(),
    );
    reconcile.purpose = "reconcile".into();
    answer["requestId"] = json!("r_question");
    answer["overviewRefs"] = json!([]);
    answer["changes"][0]["kind"] = json!("impulse");
    answer["changes"][0]["status"] = json!("open");
    answer["changes"][0]["text"] = json!("Webseite erneuern");
    answer["changes"][0]["evidence"][0]["quote"] = json!(suggestion);
    assert!(validate::apply(&mut state, &reconcile, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    answer["changes"][0]["text"] = json!("Wer kann das Budget freigeben?");
    answer["changes"][0]["evidence"][0]["quote"] = json!(question);
    validate::apply(&mut state, &reconcile, &answer.to_string()).unwrap();
    assert!(state
        .entities
        .iter()
        .any(|e| e.kind == "impulse" && e.evidence.iter().any(|v| v.quote == question)));
}

#[test]
/// Keep finite collection assignments distinct from questions and conditional proposals.
fn named_collection_assignments_reach_the_task_phase_without_accepting_questions() {
    for quote in [
        "Jana holt die Technik ab.",
        "Mara besorgt die Getränke bis Freitag.",
        "Der Leon bringt die Unterlagen zum Termin mit.",
    ] {
        assert!(validate::task_quote_allowed(quote), "{quote}");
    }
    for quote in [
        "Wer holt die Technik ab",
        "Vielleicht holt Jana die Technik ab.",
        "Jana holt die Technik nicht ab.",
        "Jana könnte die Technik abholen.",
        "Ich weiß nicht, wer die Technik abholt.",
        "Wenn die Freigabe kommt, holt Jana die Technik ab.",
    ] {
        assert!(!validate::task_quote_allowed(quote), "{quote}");
    }
}

#[test]
/// Preserve legacy checkpoint bytes while rejecting writers and mixed prompt generations.
fn legacy_prompt_stays_readable_and_cannot_be_mixed_with_new_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let mut original = WorkspaceState {
        revision: 1,
        context: WorkspaceContext {
            revision: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    original.context.agenda.push(Agenda {
        id: "e_topic".into(),
        revision: 1,
        text: "Erhaltenes menschliches Thema".into(),
    });
    let delta = store::Delta::between(&WorkspaceState::default(), &original, vec![], vec![]);
    store::append(dir.path(), &delta).unwrap();
    let path = dir.path().join("revision-00000001.json");
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    legacy["schema"] = json!(LEGACY_PROMPT_VERSION);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let old = read_state(dir.path()).unwrap();
    assert_eq!(old.prompt_version(), LEGACY_PROMPT_VERSION);
    assert_eq!(old.context.agenda, original.context.agenda);
    assert!(WorkspaceRunner::open(dir.path()).is_err());
    assert!(update_context(
        dir.path(),
        &WorkspaceContext {
            revision: 1,
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut mixed = old.clone();
    mixed.revision = 2;
    store::append(
        dir.path(),
        &store::Delta::between(&old, &mixed, vec![], vec![]),
    )
    .unwrap();
    assert!(read_state(dir.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
/// Compact generated wording without truncating evidence or partially publishing invalid output.
fn compact_model_titles_keep_literal_quotes_and_reject_oversized_generated_text() {
    let (mut state, mut request, mut answer) = setup();
    request.compact_titles();
    answer["changes"][0]["text"] = json!("Prüfung   durchführen\nund dokumentieren");
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    assert_eq!(
        state.entities[0].text,
        "Prüfung durchführen und dokumentieren"
    );
    let (mut untouched, mut request, mut answer) = setup();
    request.compact_titles();
    answer["changes"][0]["text"] = json!("x".repeat(181));
    assert!(validate::apply(&mut untouched, &request, &answer.to_string()).is_err());
    assert!(untouched.entities.is_empty());
}

#[test]
/// Resume the new generation by its own identity while retaining the legacy run and deltas.
fn new_prompt_generation_has_its_own_run_and_preserves_legacy_identity() {
    use crate::live::LiveRunner;
    let root = tempfile::tempdir().unwrap();
    let connection = || crate::Connection {
        endpoint: "http://127.0.0.1:1/v1/".into(),
        model: "synthetic".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let legacy = LiveRunner::start(root.path(), "meeting-legacy", connection(), "de").unwrap();
    let old_id = legacy.run().id.clone();
    drop(legacy);
    let old_folder = root.path().join(&old_id).join(LEGACY_WORKSPACE_FOLDER);
    std::fs::create_dir(&old_folder).unwrap();
    let state = WorkspaceState {
        revision: 1,
        ..Default::default()
    };
    let delta = store::Delta::between(&WorkspaceState::default(), &state, vec![], vec![]);
    let mut value = serde_json::to_value(delta).unwrap();
    value["schema"] = json!(LEGACY_PROMPT_VERSION);
    let old_revision = old_folder.join("revision-00000001.json");
    let old_bytes = serde_json::to_vec(&value).unwrap();
    std::fs::write(&old_revision, &old_bytes).unwrap();
    let metadata = root.path().join(&old_id).join("run.json");
    let old_metadata = std::fs::read(&metadata).unwrap();
    let current = LiveRunner::start_workspace(root.path(), "meeting-legacy", connection(), "de")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    let id = current.run().id.clone();
    assert_ne!(id, old_id);
    assert_eq!(current.run().prompt_version, PROMPT_VERSION);
    assert!(root.path().join(&id).join(WORKSPACE_FOLDER).is_dir());
    assert_eq!(std::fs::read(&old_revision).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&metadata).unwrap(), old_metadata);
    assert_eq!(
        read_state(&old_folder).unwrap().prompt_version(),
        LEGACY_PROMPT_VERSION
    );
    drop(current);
    let resumed = LiveRunner::start_workspace(root.path(), "meeting-legacy", connection(), "de")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    assert_eq!(resumed.run().id, id);
    assert_eq!(std::fs::read(&old_revision).unwrap(), old_bytes);
}

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
/// Re-reading must reject corrupt evidence in either current entities or historical events.
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

/// Exercise bounded evidence selection across status changes, text replacement and durable reopen.
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
        let quote = if kind == "impulse" {
            format!("Would someone confirm who approves phase 7 at checkpoint {label}?")
        } else {
            format!("Ich übernehme den Bericht für den Prüfpunkt {label}.")
        };
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
                format!("Dana approved phase {}.", if replace_text { 8 } else { 7 })
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
        let done = Source {
            id: "s_done".into(),
            source: "system".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: "The phase review was discussed and resolved.".into(),
        };
        let mut sources = req.transcript_sources.clone();
        sources.push(done.clone());
        req = Request::new(
            req.request_id,
            &state.context,
            &state,
            state.entities.clone(),
            std::slice::from_ref(&done),
            sources,
        );
        req.purpose = "reconcile".into();
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
    req.transcript_sources[0].text = "Wer gibt den Entwurf frei?".into();
    answer["changes"][0]["evidence"][0]["quote"] = json!("Wer gibt den Entwurf frei?");
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
/// Incomplete responses and foreign request identities must leave the previous state intact.
fn incomplete_and_foreign_answers_do_not_advance_state() {
    for key in ["moreChangesPossible", "requestId"] {
        let (mut state, req, mut answer) = setup();
        answer[key] = if key == "requestId" {
            json!("r_other")
        } else {
            json!(true)
        };
        let error = validate::apply(&mut state, &req, &answer.to_string()).unwrap_err();
        if key == "moreChangesPossible" {
            assert!(matches!(
                error.downcast_ref::<crate::transport::BudgetError>(),
                Some(crate::transport::BudgetError::WorkspaceChanges)
            ));
        } else {
            assert!(!crate::transport::is_budget_error(&error));
        }
        assert_eq!(state.revision, 0);
        assert!(state.entities.is_empty());
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

/// Quote ordering cannot replace the authoritative inherited deadline with a hedged witness.
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
        "Robin sagt: Der Versandbericht ist fertig und wurde verschickt.",
        "Leon sagt: Den Schlüssel für den zweiten Standort habe ich abgeholt.",
        "Nora sagt: Die Anleitung ist fertig.",
        "Der Versandbericht wurde verschickt.",
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
        "Die Anleitung ist noch nicht fertig.",
        "Die Anleitung ist fast fertig.",
        "Die Anleitung ist fertig geplant.",
        "Ich hoffe, dass die Anleitung fertig ist.",
        "Wenn die Anleitung fertig ist, schicke sie mir.",
        "Die Anleitung ist fertig?",
        "Ich habe den Schlüssel noch nicht abgeholt.",
        "Ich werde den Schlüssel abgeholt haben.",
        "Der Versandbericht wurde noch nicht verschickt.",
        "Der Versandbericht muss verschickt werden.",
        "Der Versandbericht wurde vielleicht verschickt.",
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
/// Condition screening uses clause/word boundaries and preserves unconditional suffix wording.
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
/// Explicit refusals reject new work while affirmative negation idioms retain valid promises.
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
/// A decision against an action differs from denying that any decision was made.
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

/// Run each terminal-status case against native publication and retained original evidence.
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
        } else if status_name == "completed" {
            request.transcript_sources[0].text = "Ich werde den Bericht verschicken.".into();
            answer["changes"][0]["text"] = json!("Bericht verschicken");
            answer["changes"][0]["evidence"][0]["quote"] =
                json!("Ich werde den Bericht verschicken.");
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
/// Uncertain or negated decisions must roll back atomically; explicit decisions stay admissible.
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
