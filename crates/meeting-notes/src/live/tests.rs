//! Synthetic tests: a scripted local OpenAI-compatible server stands in for the text model.
use super::*;
use crate::Step;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Serves one scripted chat answer per request and records every request body.
struct Server {
    endpoint: String,
    bodies: Arc<Mutex<Vec<serde_json::Value>>>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn start(answers: Vec<String>) -> Self {
        Self::with_finish(answers, "stop")
    }
    fn with_finish(answers: Vec<String>, finish: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&bodies);
        let handle = thread::spawn(move || {
            for answer in answers {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 8192];
                let header_end = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|v| v.trim().parse().unwrap())
                    .unwrap_or(0);
                while request.len() < header_end + length {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                recorded
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&request[header_end..]).unwrap());
                let body = serde_json::json!({"choices":[{"finish_reason":finish,"message":{"content":answer}}],"usage":{"prompt_tokens":31,"completion_tokens":17}}).to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        Self {
            endpoint,
            bodies,
            handle: Some(handle),
        }
    }
    fn connection(&self, model: &str) -> Connection {
        Connection {
            endpoint: self.endpoint.clone(),
            model: model.into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        }
    }
    fn finish(mut self) -> Vec<serde_json::Value> {
        self.handle.take().unwrap().join().unwrap();
        self.bodies.lock().unwrap().clone()
    }
}

fn part(key: &str, track: &str, start_s: u64, text: &str) -> LiveText {
    LiveText {
        key: key.into(),
        track: track.into(),
        start_ms: start_s * 1000,
        end_ms: start_s * 1000 + 30_000,
        text: text.into(),
    }
}

fn fact(text: &str, owner: Option<&str>, source: &str, quote: &str) -> serde_json::Value {
    serde_json::json!({"text":text,"owner":owner,"due":null,"evidence":[{"sourceId":source,"quote":quote}]})
}

fn window_answer(
    points: Vec<serde_json::Value>,
    decisions: Vec<serde_json::Value>,
    tasks: Vec<serde_json::Value>,
    questions: Vec<serde_json::Value>,
) -> String {
    serde_json::json!({"points":points,"decisions":decisions,"tasks":tasks,"questions":questions})
        .to_string()
}

fn overview_answer(items: Vec<serde_json::Value>) -> String {
    serde_json::json!({ "items": items }).to_string()
}

const BUDGET: &str = "Wir besprechen heute das Budget für das dritte Quartal.";
const DECISION: &str = "Wir entscheiden uns für den günstigeren Anbieter.";
const TASK: &str = "Anna schickt das Angebot bis Freitag an alle.";
const QUESTION: &str = "Wer klärt eigentlich die Lizenzfrage mit der Rechtsabteilung?";

#[test]
fn identical_task_wording_keeps_distinct_assignees_and_deadlines() {
    let dir = tempfile::tempdir().unwrap();
    let quote_a = "Anna schickt das Angebot bis Freitag an alle.";
    let quote_b = "Peter schickt das Angebot bis Montag an alle.";
    let mut a = fact("Angebot verschicken", Some("Anna"), "C0", quote_a);
    a["due"] = "Freitag".into();
    let mut b = fact("Angebot verschicken", Some("Peter"), "C30", quote_b);
    b["due"] = "Montag".into();
    let server = Server::start(vec![
        window_answer(vec![], vec![], vec![a.clone(), b, a], vec![]),
        overview_answer(vec![]),
    ]);
    let mut runner = LiveRunner::start(
        dir.path(),
        "meeting-1",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let texts = [
        part("a", "system", 0, quote_a),
        part("b", "system", 30, quote_b),
    ];
    let outcome = runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    server.finish();
    assert_eq!(outcome.accepted, 2);
    let tasks = runner.view().tasks;
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].owner.as_deref(), Some("Anna"));
    assert_eq!(tasks[0].due.as_deref(), Some("Freitag"));
    assert_eq!(tasks[1].owner.as_deref(), Some("Peter"));
    assert_eq!(tasks[1].due.as_deref(), Some("Montag"));
}

#[test]
fn a_shared_run_keeps_newer_classic_results_when_workspace_is_present() {
    let dir = tempfile::tempdir().unwrap();
    let quote = "Anna übernimmt den Bericht bis Freitag.";
    let server = Server::start(vec![
        window_answer(
            vec![],
            vec![],
            vec![fact("Bericht schreiben", Some("Anna"), "C0", quote)],
            vec![],
        ),
        overview_answer(vec![]),
    ]);
    let mut runner = LiveRunner::start(
        dir.path(),
        "meeting-1",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let workspace = runner.folder.join("workspace-v9");
    drop(crate::workspace::WorkspaceRunner::open(&workspace).unwrap());
    let pending = runner.pending(&[part("a", "system", 0, quote)]);
    runner
        .update(&pending, Trigger::Manual, &AnalysisControl::default())
        .unwrap();
    server.finish();
    let expected = runner.view();
    assert_eq!(expected.tasks.len(), 1);
    drop(runner);
    let actual = read_latest(dir.path(), "meeting-1").unwrap().unwrap();
    assert_eq!(actual.updated_at, expected.updated_at);
    assert_eq!(actual.tasks[0].text, expected.tasks[0].text);
    assert!(actual.workspace.is_none());
    assert!(read_latest_workspace(dir.path(), "meeting-1")
        .unwrap()
        .unwrap()
        .workspace
        .is_some());
}

#[test]
fn displaying_live_runs_rejects_another_meetings_manifest_without_changing_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(vec![]);
    let foreign = LiveRunner::start(
        dir.path(),
        "meeting-b",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    server.finish();
    let manifest = dir.path().join(&foreign.run().id).join("run.json");
    let before = std::fs::read(&manifest).unwrap();
    let latest = read_latest(dir.path(), "meeting-a").unwrap().unwrap();
    assert_eq!(
        latest.run_id, None,
        "A foreign run must not be displayed as meeting A"
    );
    assert_eq!(latest.damaged_runs, 1);
    assert_eq!(std::fs::read(manifest).unwrap(), before);
}

#[test]
fn windows_and_overview_are_source_checked_stored_and_shown_with_their_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![
        window_answer(
            vec![
                fact(
                    "Budget des dritten Quartals wird besprochen",
                    None,
                    "C0",
                    BUDGET,
                ),
                fact(
                    "Budget des dritten Quartals wird besprochen",
                    None,
                    "C0",
                    BUDGET,
                ),
                fact("Erfunden", None, "C0", "Das Budget wurde verdoppelt."),
            ],
            vec![fact("Günstigerer Anbieter gewählt", None, "M30", DECISION)],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget und Anbieterwahl", None, "M30", DECISION)]),
        window_answer(
            vec![],
            vec![],
            vec![fact("Angebot an alle schicken", Some("Anna"), "C60", TASK)],
            vec![
                fact("Lizenzfrage offen", None, "C90", QUESTION),
                // A source from an earlier window is not allowed as evidence for this window.
                fact("Wiederholung", None, "C0", BUDGET),
            ],
        ),
        overview_answer(vec![
            fact("Budget und Anbieterwahl", None, "M30", DECISION),
            fact("Anna schickt das Angebot", None, "C60", TASK),
        ]),
    ]);
    let mut runner =
        LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
    let control = AnalysisControl::default();
    let first = vec![
        part("system-0", "system", 0, BUDGET),
        part("microphone-30", "microphone", 30, DECISION),
    ];
    let pending = runner.pending(&first);
    assert_eq!(pending.len(), 2);
    let outcome = runner
        .update(&pending, Trigger::Interval, &control)
        .unwrap();
    assert_eq!(outcome.window, Some(1));
    assert_eq!(
        outcome.accepted, 2,
        "the duplicate is dropped, the invention rejected"
    );
    assert_eq!(outcome.rejected, 1);
    assert_eq!(outcome.overview, Some(1));
    assert!(runner.pending(&first).is_empty());

    let mut second = first.clone();
    second.push(part("system-60", "system", 60, TASK));
    second.push(part("system-90", "system", 90, QUESTION));
    let pending = runner.pending(&second);
    assert_eq!(pending.len(), 2);
    let outcome = runner.update(&pending, Trigger::Manual, &control).unwrap();
    assert_eq!(
        (outcome.window, outcome.accepted, outcome.rejected),
        (Some(2), 2, 1)
    );
    assert_eq!(outcome.overview, Some(2));

    let view = runner.view();
    assert_eq!(view.windows, 2);
    assert_eq!(view.summarized_sections, 4);
    assert_eq!(view.points.len(), 1);
    assert_eq!(view.decisions[0].tracks, ["microphone"]);
    assert_eq!(view.tasks[0].owner.as_deref(), Some("Anna"));
    assert_eq!(view.tasks[0].evidence[0].start_ms, 60_000);
    assert_eq!(view.questions.len(), 1);
    assert_eq!(view.overview.len(), 2);
    assert_eq!(view.overview_window, 2);
    assert_eq!(view.rejected, 2);

    // The second overview request carried the first overview and only the new statements.
    let bodies = server.finish();
    assert_eq!(bodies.len(), 4);
    let input: serde_json::Value =
        serde_json::from_str(bodies[3]["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(input["previousSummary"].as_array().unwrap().len(), 1);
    assert_eq!(input["newStatements"].as_array().unwrap().len(), 2);
    assert_eq!(input["newStatements"][0]["list"], "tasks");
    // The second window request listed earlier statements as known, and only new sections.
    let window_input: serde_json::Value =
        serde_json::from_str(bodies[2]["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(window_input["knownStatements"].as_array().unwrap().len(), 2);
    let ids: Vec<_> = window_input["newSections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, ["C60", "C90"]);
    for body in &bodies {
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["stream"], false);
    }

    // Everything reads back verified from disk.
    let stored = read_latest(&root, "meeting-1").unwrap().unwrap();
    assert_eq!(stored.run_id, view.run_id);
    assert_eq!(stored.decisions.len(), 1);
    assert_eq!(stored.overview.len(), 2);
}

#[test]
fn a_restart_continues_the_matching_run_and_another_model_starts_a_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![window_answer(vec![], vec![], vec![], vec![])]);
    let texts = vec![part("system-0", "system", 0, BUDGET)];
    let first_id = {
        let mut runner =
            LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
        let pending = runner.pending(&texts);
        runner
            .update(&pending, Trigger::Interval, &AnalysisControl::default())
            .unwrap();
        runner.run().id.clone()
    };
    // The same local route after a restart; no request is needed to continue.
    let same_route = server.connection("synthetic");
    let other_model = server.connection("other");
    server.finish();
    let again = LiveRunner::start(&root, "meeting-1", same_route, "de").unwrap();
    assert_eq!(again.run().id, first_id);
    assert!(
        again.pending(&texts).is_empty(),
        "nothing is summarized twice"
    );
    assert!(!again.overview_due(), "an empty window needs no overview");
    let other = LiveRunner::start(&root, "meeting-1", other_model, "de").unwrap();
    assert_ne!(other.run().id, first_id);
    assert_eq!(other.pending(&texts).len(), 1);
    // The earlier run is kept unchanged next to the new one.
    assert!(load_run(&root.join(&first_id)).is_ok());
    assert_eq!(list_runs(&root).unwrap().0.len(), 2);
}

#[test]
fn a_damaged_window_blocks_its_run_and_is_never_shrunk_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![
        window_answer(
            vec![fact("Budget", None, "C0", BUDGET)],
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget", None, "C0", BUDGET)]),
    ]);
    let id = {
        let mut runner =
            LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
        let pending = runner.pending(&[part("system-0", "system", 0, BUDGET)]);
        runner
            .update(&pending, Trigger::Interval, &AnalysisControl::default())
            .unwrap();
        runner.run().id.clone()
    };
    server.finish();
    let path = root.join(&id).join("window-000001.json");
    let tampered = fs_read(&path).replace("Wir besprechen heute", "Wir verschieben heute");
    std::fs::write(&path, &tampered).unwrap();
    assert!(load_run(&root.join(&id)).is_err());
    let latest = read_latest(&root, "meeting-1").unwrap().unwrap();
    assert_eq!(latest.run_id, None);
    assert_eq!(latest.damaged_runs, 1);
    assert_eq!(
        fs_read(&path),
        tampered,
        "the damaged file is left as it was"
    );
    // A gap in the numbering is reported as well.
    std::fs::rename(&path, root.join(&id).join("window-000002.json")).unwrap();
    assert!(load_run(&root.join(&id)).is_err());
}

fn fs_read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn an_empty_or_unsupported_overview_keeps_the_previous_one_and_is_retried() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![
        window_answer(
            vec![fact("Budget", None, "C0", BUDGET)],
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget", None, "C0", BUDGET)]),
        window_answer(
            vec![],
            vec![fact("Anbieter", None, "C30", DECISION)],
            vec![],
            vec![],
        ),
        // Cites a section that does not exist: discarded, so the overview stays as it was.
        overview_answer(vec![fact("Erfunden", None, "C999", DECISION)]),
        // Retried on the next update without new text.
        overview_answer(vec![
            fact("Budget", None, "C0", BUDGET),
            fact("Anbieter", None, "C30", DECISION),
        ]),
    ]);
    let mut runner =
        LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "en").unwrap();
    let control = AnalysisControl::default();
    let texts = vec![
        part("system-0", "system", 0, BUDGET),
        part("system-30", "system", 30, DECISION),
    ];
    let first = runner.pending(&texts[..1]);
    runner.update(&first, Trigger::Interval, &control).unwrap();
    let second = runner.pending(&texts);
    let outcome = runner.update(&second, Trigger::Interval, &control).unwrap();
    assert_eq!(outcome.window, Some(2));
    assert!(outcome.overview_error.is_some());
    assert_eq!(runner.view().overview[0].text, "Budget");
    assert!(runner.overview_due());
    let retry = runner.update(&[], Trigger::Manual, &control).unwrap();
    assert_eq!((retry.window, retry.overview), (None, Some(2)));
    assert_eq!(runner.view().overview.len(), 2);
    assert!(!runner.overview_due());
    let bodies = server.finish();
    assert!(bodies[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("Write every text in English"));
}

#[test]
fn a_structurally_invalid_answer_stores_nothing_and_the_text_stays_pending() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![r#"{"points":[],"decisions":[],"tasks":[]}"#.into()]);
    let mut runner =
        LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
    let texts = vec![part("system-0", "system", 0, BUDGET)];
    let pending = runner.pending(&texts);
    assert!(runner
        .update(&pending, Trigger::Interval, &AnalysisControl::default())
        .is_err());
    assert_eq!(runner.pending(&texts).len(), 1);
    assert!(!root
        .join(&runner.run().id)
        .join("window-000001.json")
        .exists());
    server.finish();
}

#[test]
fn owners_outside_tasks_and_invented_owners_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![window_answer(
        vec![fact("Angebot", Some("Anna"), "C0", TASK)],
        vec![],
        vec![fact("Angebot", Some("Ben"), "C0", TASK)],
        vec![],
    )]);
    let mut runner =
        LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
    let pending = runner.pending(&[part("system-0", "system", 0, TASK)]);
    let outcome = runner
        .update(&pending, Trigger::Interval, &AnalysisControl::default())
        .unwrap();
    assert_eq!(
        (outcome.accepted, outcome.rejected, outcome.overview),
        (0, 2, None)
    );
    server.finish();
}

#[test]
fn cloud_live_notes_need_explicit_text_release() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection {
        endpoint: "https://api.example.com/v1/".into(),
        model: "cloud-model".into(),
        mode: "cloud".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    assert!(LiveRunner::start(dir.path(), "meeting-1", connection, "de").is_err());
    assert!(!dir.path().join("live-notes").exists());
}

/// Regression guard: moving the request code into a shared helper must not change the
/// post-meeting analysis request in any byte that reaches the model.
#[test]
fn the_post_meeting_analysis_request_is_unchanged() {
    let server = Server::start(vec![r#"{"items":[]}"#.into()]);
    let client = TextClient::new(&server.connection("synthetic")).unwrap();
    let step = Step {
        id: "s".into(),
        name: "Summary".into(),
        kind: Kind::Summary,
        prompt: "Fasse zusammen.".into(),
    };
    let sources = vec![crate::Source {
        id: "S00001".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 1000,
        text: BUDGET.into(),
    }];
    client.generate(&sources, &step).unwrap();
    let body = server.finish().remove(0);
    let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["text","owner","due","evidence"],"properties":{"text":{"type":"string"},"owner":{"type":["string","null"]},"due":{"type":["string","null"]},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["sourceId","quote"],"properties":{"sourceId":{"type":"string"},"quote":{"type":"string"}}}}}}}}});
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(system.starts_with("Du wertest Meetingtext aus."));
    assert!(system.ends_with(
        "\nowner und due bleiben null.\nAnalyseauftrag als JSON-Zeichenfolge: \"Fasse zusammen.\""
    ));
    let expected = serde_json::json!({"model":"synthetic","stream":false,"messages":[{"role":"system","content":system},{"role":"user","content":serde_json::to_string(&sources).unwrap()}],"response_format":{"type":"json_schema","json_schema":{"name":"meeting_notes","strict":true,"schema":schema}},"max_tokens":2048,"temperature":0});
    assert_eq!(body, expected);
}

#[test]
fn the_run_worked_on_last_is_shown_even_when_another_was_created_later() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let server = Server::start(vec![
        window_answer(vec![], vec![], vec![], vec![]),
        window_answer(vec![], vec![], vec![], vec![]),
    ]);
    let control = AnalysisControl::default();
    let texts = vec![
        part("system-0", "system", 0, BUDGET),
        part("system-30", "system", 30, DECISION),
    ];
    let first = {
        let mut runner =
            LiveRunner::start(&root, "meeting-1", server.connection("x"), "de").unwrap();
        let pending = runner.pending(&texts[..1]);
        runner
            .update(&pending, Trigger::Interval, &control)
            .unwrap();
        runner.run().id.clone()
    };
    std::thread::sleep(Duration::from_millis(20));
    // A different model was tried briefly and created a newer, empty run.
    let other = LiveRunner::start(&root, "meeting-1", server.connection("y"), "de").unwrap();
    assert_ne!(other.run().id, first);
    assert_eq!(
        read_latest(&root, "meeting-1")
            .unwrap()
            .unwrap()
            .run_id
            .as_deref(),
        Some(other.run().id.as_str())
    );
    std::thread::sleep(Duration::from_millis(20));
    // Back to the first model: its run continues and is shown again.
    let mut again = LiveRunner::start(&root, "meeting-1", server.connection("x"), "de").unwrap();
    assert_eq!(again.run().id, first);
    let pending = again.pending(&texts);
    assert_eq!(pending.len(), 1);
    again.update(&pending, Trigger::Interval, &control).unwrap();
    assert_eq!(
        read_latest(&root, "meeting-1")
            .unwrap()
            .unwrap()
            .run_id
            .as_deref(),
        Some(first.as_str())
    );
    server.finish();
}

#[test]
fn a_large_statement_backlog_is_covered_batch_by_batch_and_never_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("live-notes");
    let full = |prefix: &str| -> Vec<serde_json::Value> {
        (0..20)
            .map(|i| {
                fact(
                    &format!("{prefix} {}", char::from(b'a' + i)),
                    None,
                    "C0",
                    BUDGET,
                )
            })
            .collect()
    };
    let tasks: Vec<_> = (0..20)
        .map(|i| {
            fact(
                &format!("Aufgabe {}", char::from(b'a' + i)),
                Some("Anna"),
                "C30",
                TASK,
            )
        })
        .collect();
    let server = Server::start(vec![
        // Window 1 holds the maximum of 80 statements; its overview fails at first.
        window_answer(full("Punkt"), full("Beschluss"), tasks, full("Frage")),
        overview_answer(vec![]),
        // Window 2 adds ten more statements while the first overview is still missing.
        window_answer(
            (0..10)
                .map(|i| {
                    fact(
                        &format!("Später {}", char::from(b'a' + i)),
                        None,
                        "C60",
                        DECISION,
                    )
                })
                .collect(),
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget", None, "C0", BUDGET)]),
        overview_answer(vec![
            fact("Budget", None, "C0", BUDGET),
            fact("Anbieter", None, "C60", DECISION),
        ]),
    ]);
    let mut runner =
        LiveRunner::start(&root, "meeting-1", server.connection("synthetic"), "de").unwrap();
    let control = AnalysisControl::default();
    let mut texts = vec![
        part("system-0", "system", 0, BUDGET),
        part("system-30", "system", 30, TASK),
    ];
    let first = runner.pending(&texts);
    let outcome = runner.update(&first, Trigger::Interval, &control).unwrap();
    assert_eq!(outcome.accepted, 80);
    assert!(outcome.overview_error.is_some());
    texts.push(part("system-60", "system", 60, DECISION));
    let second = runner.pending(&texts);
    let outcome = runner.update(&second, Trigger::Interval, &control).unwrap();
    assert_eq!(outcome.overview, Some(2), "two batches in one update");
    assert!(!runner.overview_due());
    let view = runner.view();
    assert_eq!(view.overview_window, 2);
    let bodies = server.finish();
    let statements = |index: usize| {
        let input: serde_json::Value =
            serde_json::from_str(bodies[index]["messages"][1]["content"].as_str().unwrap())
                .unwrap();
        input["newStatements"].as_array().unwrap().len()
    };
    // Every statement reached an overview request: 80 from window 1, then 10 from window 2.
    assert_eq!((statements(1), statements(3), statements(4)), (80, 80, 10));
    // Stored overviews read back with their exact coverage.
    let stored = load_run(&root.join(&runner.run().id)).unwrap();
    let coverage: Vec<u32> = stored.overviews.iter().map(|o| o.window).collect();
    assert_eq!(coverage, [1, 2]);
}

#[test]
fn failed_overview_is_durable_and_retry_keeps_windows_and_the_previous_overview() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(vec![
        window_answer(
            vec![fact("Budget", None, "C0", BUDGET)],
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget", None, "C0", BUDGET)]),
        window_answer(
            vec![fact("Anbieter", None, "C30", DECISION)],
            vec![],
            vec![],
            vec![],
        ),
        "not valid JSON: PRIVATE_SYNTHETIC_RESPONSE".into(),
        overview_answer(vec![fact("Anbieter", None, "C30", DECISION)]),
    ]);
    let connection = server.connection("synthetic");
    let mut runner = LiveRunner::start(
        dir.path(),
        "synthetic",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let mut texts = vec![part("system-0", "system", 0, BUDGET)];
    runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    let folder = dir.path().join(&runner.run().id);
    let overview = std::fs::read(folder.join("overview-000001.json")).unwrap();
    texts.push(part("system-30", "system", 30, DECISION));
    runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    let window = std::fs::read(folder.join("window-000002.json")).unwrap();
    let saved = read_latest(dir.path(), "synthetic").unwrap().unwrap();
    assert!(saved.overview_pending);
    assert_eq!(saved.overview_window, 1);
    assert_eq!(
        saved.last_request.unwrap().request.result,
        RequestResult::Validation
    );
    let diagnostic = std::fs::read_to_string(folder.join("diagnostic-000004.json")).unwrap();
    assert!(!diagnostic.contains("PRIVATE_SYNTHETIC_RESPONSE"));
    assert!(!diagnostic.contains(BUDGET));
    let mut resumed = LiveRunner::start(dir.path(), "synthetic", connection, "de").unwrap();
    assert!(resumed.pending(&texts).is_empty());
    resumed
        .update(&[], Trigger::Manual, &AnalysisControl::default())
        .unwrap();
    assert!(!resumed.view().overview_pending);
    assert_eq!(resumed.view().overview_window, 2);
    assert_eq!(
        std::fs::read(folder.join("overview-000001.json")).unwrap(),
        overview
    );
    assert_eq!(
        std::fs::read(folder.join("window-000002.json")).unwrap(),
        window
    );
    server.finish();
}

#[test]
fn transport_diagnostics_do_not_copy_free_form_finish_reasons_or_model_content() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::with_finish(
        vec!["PRIVATE_SYNTHETIC_REPLY".into()],
        "PRIVATE_SYNTHETIC_FINISH",
    );
    let mut runner = LiveRunner::start(
        dir.path(),
        "synthetic",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let texts = [part("system-0", "system", 0, BUDGET)];
    assert!(runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default()
        )
        .is_err());
    let folder = dir.path().join(&runner.run().id);
    let raw = std::fs::read_to_string(folder.join("diagnostic-000001.json")).unwrap();
    assert!(!raw.contains("PRIVATE_SYNTHETIC"));
    assert!(!raw.contains(BUDGET));
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["request"]["finish"], "other");
    assert_eq!(value["request"]["result"], "incomplete");
    assert_eq!(value["request"]["httpStatus"], 200);
    assert_eq!(value["request"]["promptTokens"], 31);
    assert_eq!(value["request"]["completionTokens"], 17);
    assert_eq!(value["request"]["outputLimit"], 4096);
    server.finish();
}

#[test]
fn an_interrupted_answer_after_http_200_is_reported_as_a_connection_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 8192];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        // Promise a long body, send only its start and close the connection.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4000\r\nConnection: close\r\n\r\n{\"choices\":[")
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection {
        endpoint,
        model: "synthetic".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let mut runner = LiveRunner::start(dir.path(), "synthetic", connection, "de").unwrap();
    let texts = [part("system-0", "system", 0, BUDGET)];
    assert!(runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default()
        )
        .is_err());
    let folder = dir.path().join(&runner.run().id);
    let request = diagnostics::latest(&folder).unwrap().request;
    assert_eq!(request.result, RequestResult::Transport);
    assert_eq!(request.http_status, Some(200));
    server.join().unwrap();
}

#[test]
fn a_diagnostic_storage_warning_describes_only_the_update_that_failed_to_save() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(vec![
        window_answer(
            vec![fact("Budget", None, "C0", BUDGET)],
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Budget", None, "C0", BUDGET)]),
        window_answer(
            vec![fact("Anbieter", None, "C30", DECISION)],
            vec![],
            vec![],
            vec![],
        ),
        overview_answer(vec![fact("Anbieter", None, "C30", DECISION)]),
    ]);
    let mut runner = LiveRunner::start(
        dir.path(),
        "synthetic",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let folder = dir.path().join(&runner.run().id);
    // Synthetic exhaustion: every bounded diagnostic slot is already taken.
    let taken: Vec<_> = (1..=1024)
        .map(|index| folder.join(format!("diagnostic-{index:06}.json")))
        .collect();
    for path in &taken {
        std::fs::write(path, b"").unwrap();
    }
    let mut texts = vec![part("system-0", "system", 0, BUDGET)];
    let first = runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    assert!(first.window.is_some());
    assert!(first.diagnostic_warning.is_some());
    for path in &taken {
        std::fs::remove_file(path).unwrap();
    }
    texts.push(part("system-30", "system", 30, DECISION));
    let second = runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    assert!(second.window.is_some());
    assert_eq!(second.diagnostic_warning, None);
    server.finish();
}

#[test]
fn a_failed_window_commit_is_distinguished_from_a_model_validation_error() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(vec![window_answer(
        vec![fact("Budget", None, "C0", BUDGET)],
        vec![],
        vec![],
        vec![],
    )]);
    let mut runner = LiveRunner::start(
        dir.path(),
        "synthetic",
        server.connection("synthetic"),
        "de",
    )
    .unwrap();
    let folder = dir.path().join(&runner.run().id);
    let occupied = folder.join("window-000001.json");
    std::fs::create_dir(&occupied).unwrap();
    let texts = [part("system-0", "system", 0, BUDGET)];
    assert!(runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default()
        )
        .is_err());
    assert_eq!(runner.pending(&texts).len(), 1);
    assert_eq!(
        diagnostics::latest(&folder).unwrap().request.result,
        RequestResult::Persistence
    );
    assert!(occupied.is_dir());
    server.finish();
}

#[test]
fn workspace_view_survives_a_newer_classic_run_and_reports_damaged_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(vec![]);
    let workspace = LiveRunner::start(
        dir.path(),
        "meeting-a",
        server.connection("workspace-model"),
        "de",
    )
    .unwrap()
    .with_workspace(crate::workspace::WorkspaceContext::default())
    .unwrap();
    let workspace_id = workspace.run().id.clone();
    let workspace_folder = workspace.workspace_folder().unwrap();
    drop(workspace);
    let classic = LiveRunner::start(
        dir.path(),
        "meeting-a",
        server.connection("classic-model"),
        "de",
    )
    .unwrap();
    server.finish();
    assert_eq!(
        read_latest(dir.path(), "meeting-a").unwrap().unwrap().model,
        "classic-model"
    );
    let saved = read_latest_workspace(dir.path(), "meeting-a")
        .unwrap()
        .unwrap();
    assert_eq!(saved.run_id.as_deref(), Some(workspace_id.as_str()));
    assert!(saved.workspace.is_some());

    let damaged = workspace_folder.join("revision-00000001.json");
    std::fs::write(&damaged, b"broken").unwrap();
    let saved = read_latest_workspace(dir.path(), "meeting-a")
        .unwrap()
        .unwrap();
    assert!(saved.workspace.is_none());
    assert_eq!(saved.damaged_runs, 1);
    assert_eq!(std::fs::read(damaged).unwrap(), b"broken");
    assert_eq!(
        read_latest(dir.path(), "meeting-a").unwrap().unwrap().model,
        classic.run().model
    );
}
