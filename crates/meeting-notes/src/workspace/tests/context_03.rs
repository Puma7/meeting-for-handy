use super::*;

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
