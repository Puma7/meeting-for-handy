use super::*;
use serde_json::json;

/// Accept a request for an owned synthetic server before its finite deadline.
fn accept_synthetic_before(
    listener: &std::net::TcpListener,
    deadline: std::time::Instant,
) -> std::io::Result<std::net::TcpStream> {
    use std::{
        io::{Error, ErrorKind},
        thread,
        time::{Duration, Instant},
    };
    listener.set_nonblocking(true)?;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::new(
                ErrorKind::TimedOut,
                "synthetic request sequence did not finish",
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                // Accepted Windows handles may inherit nonblocking mode.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                return Ok(stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn synthetic_accept_times_out_without_requests_and_accepts_queued_clients() {
    use std::{
        io::ErrorKind,
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (send, receive) = mpsc::channel();
    let server = thread::spawn(move || {
        let result =
            accept_synthetic_before(&listener, Instant::now() + Duration::from_millis(100));
        send.send(result.map(|_| ())).unwrap();
    });
    let result = receive.recv_timeout(Duration::from_millis(750));
    if matches!(result, Err(mpsc::RecvTimeoutError::Timeout)) {
        // Wake only this owned listener so the before-regression cannot hang the suite.
        TcpStream::connect_timeout(&address, Duration::from_millis(100)).unwrap();
    }
    server.join().unwrap();
    assert_eq!(
        result
            .expect("synthetic accept did not honor its deadline")
            .unwrap_err()
            .kind(),
        ErrorKind::TimedOut
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let accepted =
        accept_synthetic_before(&listener, Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(accepted.peer_addr().unwrap(), client.local_addr().unwrap());
    assert_eq!(accepted.local_addr().unwrap(), client.peer_addr().unwrap());
    assert_eq!(
        accepted.read_timeout().unwrap(),
        Some(Duration::from_secs(3))
    );
    assert_eq!(
        accepted.write_timeout().unwrap(),
        Some(Duration::from_secs(3))
    );
}

fn setup() -> (WorkspaceState, Request, serde_json::Value) {
    let source = Source {
        id: "s_one".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: "Ich gebe den Entwurf morgen frei.".into(),
    };
    let state = WorkspaceState::default();
    let req = Request::new(
        "r_test".into(),
        &WorkspaceContext::default(),
        &state,
        vec![],
        std::slice::from_ref(&source),
        vec![source.clone()],
    );
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_test","changes":[{"proposalId":"p_one","kind":"task","targetId":null,"targetRevision":null,"text":"Entwurf freigeben","status":"open","attention":"current","owner":null,"due":"morgen","evidence":[{"sourceId":"s_one","quote":"Ich gebe den Entwurf morgen frei."}],"relatedEntityIds":[]}],"overviewRefs":["p_one"],"unresolvedContextIds":[],"moreChangesPossible":false});
    (state, req, answer)
}

mod context_01;
mod context_02;
#[path = "tests/history_01.rs"]
mod history_01;
mod inference_01;
mod persistence_01;
mod validation_01;

#[test]
fn removed_agenda_evidence_resets_active_assessment_but_preserves_history_after_reopen() {
    for human in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
        runner
            .set_context(WorkspaceContext {
                agenda: vec![Agenda {
                    id: "e_topic".into(),
                    revision: 1,
                    text: "Freigabe".into(),
                }],
                ..Default::default()
            })
            .unwrap();
        let context = runner.context.lock().unwrap().clone();
        let mut state = runner.state.clone();
        state.apply_context(&context).unwrap();
        let source = LiveText {
            key: "part".into(),
            track: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: "Wir haben die Freigabe besprochen.".into(),
        };
        let processed = live::pending(&[], &[source]);
        let id = source_id(&processed[0]);
        let original = Source {
            id: id.clone(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 3000,
            text: processed[0].text.clone(),
        };
        state.originals.push(original.clone());
        state.processed = processed;
        state.entities[0].status = "discussed".into();
        state.entities[0].revision = 2;
        state.entities[0].human_locked = human;
        state.entities[0].evidence = vec![crate::Evidence {
            source_id: id.clone(),
            quote: original.text.clone(),
        }];
        state.events.push(Event {
            human: false,
            id: "v_prior".into(),
            entity_id: "e_topic".into(),
            revision: 2,
            at_ms: 3000,
            text: "Freigabe".into(),
            status: "discussed".into(),
            evidence: state.entities[0].evidence.clone(),
        });
        runner
            .commit(
                state,
                vec![original],
                vec![],
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
        let prior = std::fs::read(dir.path().join("revision-00000001.json")).unwrap();
        assert!(runner.pending(&[]).is_empty());
        let client = TextClient::new(&crate::Connection {
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: "no-inference".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        })
        .unwrap();
        runner
            .update(&client, &[], &AnalysisControl::default(), "en")
            .unwrap();
        drop(runner);
        let state = read_state(dir.path()).unwrap();
        assert_eq!(state.entities[0].status, "open");
        assert!(!state.entities[0].human_locked);
        assert!(state.entities[0].evidence.is_empty());
        assert!(!state.context_excluded_ids.contains(&"e_topic".into()));
        assert_eq!(state.events[0].status, "discussed");
        assert_eq!(state.events[0].evidence[0].source_id, id);
        assert_eq!(
            std::fs::read(dir.path().join("revision-00000001.json")).unwrap(),
            prior
        );
    }
}

#[test]
fn context_journal_must_reach_and_match_its_checkpoint_before_any_recovery_write() {
    for damage in [
        "missing tail",
        "different checkpoint",
        "different checkpoint before suffix",
    ] {
        for operation in ["open", "read", "update"] {
            let dir = tempfile::tempdir().unwrap();
            let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
            let (first, _) = update_context(
                dir.path(),
                &WorkspaceContext {
                    agenda: vec![Agenda {
                        id: "e_topic".into(),
                        revision: 1,
                        text: "Budget".into(),
                    }],
                    shared_notes: vec![SharedNote {
                        id: "h_note".into(),
                        revision: 1,
                        text: "First released note".into(),
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
            let mut edited = first.clone();
            edited.shared_notes[0].text = "Latest released note".into();
            edited.shared_notes[0].revision = 2;
            edited.overrides.push(Override {
                id: "e_topic".into(),
                expected_revision: 1,
                status: "discussed".into(),
                saved: false,
            });
            let (checkpoint, state) = update_context(dir.path(), &edited).unwrap();
            *runner.context.lock().unwrap() = checkpoint.clone();
            runner
                .commit(
                    state,
                    vec![],
                    vec![],
                    &AnalysisControl::default(),
                    &checkpoint,
                )
                .unwrap();
            drop(runner);
            assert_eq!(read_state(dir.path()).unwrap().context, checkpoint);
            if damage == "different checkpoint before suffix" {
                let mut later = checkpoint.clone();
                later.shared_notes[0].text.push('!');
                later.shared_notes[0].revision += 1;
                update_context(dir.path(), &later).unwrap();
            }
            let path = dir.path().join("context-00000002.json");
            if damage == "missing tail" {
                std::fs::remove_file(path).unwrap();
            } else {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                value["sharedNotes"][0]["text"] = json!("Changed checkpoint note");
                std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            let snapshot = || {
                let mut files = std::fs::read_dir(dir.path())
                    .unwrap()
                    .map(|e| {
                        let p = e.unwrap().path();
                        (p.file_name().unwrap().to_owned(), std::fs::read(p).unwrap())
                    })
                    .collect::<Vec<_>>();
                files.sort_by(|a, b| a.0.cmp(&b.0));
                files
            };
            let before = snapshot();
            match operation {
                "open" => assert!(
                    WorkspaceRunner::open(dir.path()).is_err(),
                    "{damage}: reopened damaged context"
                ),
                "read" => assert!(
                    read_state(dir.path()).is_err(),
                    "{damage}: projected damaged context"
                ),
                _ => {
                    assert!(
                        update_context(dir.path(), &first).is_err(),
                        "{damage}: reused lost context revision"
                    );
                    assert!(
                        update_context(dir.path(), &checkpoint).is_err(),
                        "{damage}: wrote beyond damaged base"
                    );
                }
            }
            assert_eq!(
                snapshot(),
                before,
                "{damage}/{operation} changed retained bytes"
            );
        }
    }
}

#[test]
fn checkpoint_only_context_can_start_a_new_journal_and_replay_a_valid_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let old = WorkspaceState::default();
    let context = WorkspaceContext {
        revision: 5,
        shared_notes: vec![SharedNote {
            id: "h_legacy".into(),
            revision: 1,
            text: "Retained checkpoint-only note".into(),
        }],
        ..Default::default()
    };
    let mut state = old.clone();
    state.context = context.clone();
    state.revision = 1;
    store::append(
        dir.path(),
        &store::Delta::between(&old, &state, vec![], vec![]),
    )
    .unwrap();
    let checkpoint = std::fs::read(dir.path().join("revision-00000001.json")).unwrap();
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    assert_eq!(*runner.context.lock().unwrap(), context);
    drop(runner);
    let mut edit = context.clone();
    edit.shared_notes[0].text.push('!');
    edit.shared_notes[0].revision += 1;
    let (latest, _) = update_context(dir.path(), &edit).unwrap();
    assert_eq!(latest.revision, 6);
    assert_eq!(read_state(dir.path()).unwrap().context, latest);
    let runner = WorkspaceRunner::open(dir.path()).unwrap();
    assert_eq!(*runner.context.lock().unwrap(), latest);
    drop(runner);
    assert_eq!(
        std::fs::read(dir.path().join("revision-00000001.json")).unwrap(),
        checkpoint
    );
}
