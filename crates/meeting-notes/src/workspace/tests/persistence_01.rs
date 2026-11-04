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
