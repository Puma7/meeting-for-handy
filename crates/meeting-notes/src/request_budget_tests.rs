//! Synthetic HTTP regressions. No installed model or meeting is used here.
use crate::*;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

struct Server {
    endpoint: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(replies: Vec<(u16, Value)>, cancel: Option<AnalysisControl>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (done, seen) = (Arc::clone(&stop), Arc::clone(&requests));
        let worker = thread::spawn(move || {
            for (index, (status, reply)) in replies.into_iter().enumerate() {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    if done.load(Ordering::Acquire) || Instant::now() >= deadline {
                        return;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                // Accepted sockets can inherit the listener's nonblocking mode on Windows.
                // Reading a split TCP packet must wait for its bounded remainder.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let end = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && bytes.len() < 1_000_000);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let length: usize = String::from_utf8_lossy(&bytes[..end])
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|s| s.parse().unwrap())
                    })
                    .unwrap();
                while bytes.len() < end + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                seen.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&bytes[end..]).unwrap());
                if index == 0 {
                    if let Some(control) = &cancel {
                        control.cancel();
                    }
                }
                let body = reply.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        Self {
            endpoint,
            stop,
            requests,
            worker: Some(worker),
        }
    }
    fn client(&self) -> TextClient {
        TextClient::new(&self.connection()).unwrap()
    }
    fn connection(&self) -> Connection {
        Connection {
            endpoint: self.endpoint.clone(),
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.worker.take().unwrap().join();
        if !thread::panicking() {
            result.unwrap();
        }
    }
}
fn reply(finish: &str, content: &str) -> (u16, Value) {
    (
        200,
        json!({"choices":[{"finish_reason":finish,"message":{"content":content}}],"usage":{"prompt_tokens":1800,"completion_tokens":2048}}),
    )
}
fn step() -> Step {
    Step {
        id: "summary".into(),
        name: "Summary".into(),
        kind: Kind::Summary,
        prompt: "Fasse die Inhalte kurz zusammen.".into(),
    }
}
fn sources() -> Vec<Source> {
    vec![
        Source {
            id: "S1".into(),
            source: "microphone".into(),
            start_ms: 0,
            end_ms: 30000,
            text: "Der Bericht bleibt lokal gespeichert.".into(),
        },
        Source {
            id: "S2".into(),
            source: "system".into(),
            start_ms: 30000,
            end_ms: 60000,
            text: "Vielleicht könnten wir einen Kalender testen.".into(),
        },
    ]
}
fn item(source: &Source) -> Value {
    json!({"text":source.text,"owner":null,"due":null,"evidence":[{"sourceId":source.id,"quote":source.text}]})
}

#[test]
fn length_retry_keeps_messages_schema_and_only_accepts_the_complete_answer() {
    let answer = json!({"items":[item(&sources()[0])]}).to_string();
    let server = Server::new(
        vec![
            reply("length", "PRIVATE_PARTIAL_DO_NOT_STORE"),
            reply("stop", &answer),
        ],
        None,
    );
    let client = server.client();
    let result = client.generate(&sources()[..1], &step()).unwrap();
    assert!(!result.contains("PRIVATE_PARTIAL"));
    assert_eq!(
        validate_answer(&result, &sources()[..1], &Kind::Summary, "")
            .unwrap()
            .items
            .len(),
        1
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["max_tokens"], 2048);
    assert_eq!(requests[1]["max_tokens"], 4096);
    let diagnostic = client.diagnostic();
    assert_eq!(diagnostic.previous_attempts.len(), 1);
    assert!(matches!(
        diagnostic.previous_attempts[0].finish,
        Some(crate::diagnostics::Finish::Length)
    ));
    for key in ["model", "messages", "response_format", "temperature"] {
        assert_eq!(requests[0][key], requests[1][key]);
    }
}

#[test]
fn context_rejection_splits_the_same_batch_without_losing_either_source() {
    let inputs = sources();
    let server = Server::new(
        vec![
            (
                400,
                json!({"error":{"message":"Context size has been exceeded. PRIVATE_SERVER_TEXT"}}),
            ),
            reply("stop", &json!({"items":[item(&inputs[0])]}).to_string()),
            reply("stop", &json!({"items":[item(&inputs[1])]}).to_string()),
        ],
        None,
    );
    let client = server.client();
    let result = client.generate(&inputs, &step()).unwrap();
    let diagnostic = client.diagnostic();
    assert_eq!(diagnostic.previous_attempts.len(), 2);
    assert_eq!(diagnostic.previous_attempts[0].http_status, Some(400));
    assert!(matches!(
        diagnostic.previous_attempts[0].result,
        crate::diagnostics::RequestResult::ContextLimit
    ));
    let checked = validate_answer(&result, &inputs, &Kind::Summary, "").unwrap();
    assert_eq!(checked.items.len(), 2);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let selected = |i: usize| {
        serde_json::from_str::<Value>(requests[i]["messages"][1]["content"].as_str().unwrap())
            .unwrap()
    };
    assert_eq!(selected(0).as_array().unwrap().len(), 2);
    assert_eq!(selected(1)[0]["id"], "S1");
    assert_eq!(selected(2)[0]["id"], "S2");
}

#[test]
fn another_http_400_is_not_retried_and_its_private_body_is_not_exposed() {
    let server = Server::new(
        vec![(
            400,
            json!({"error":{"message":"PRIVATE_TOKEN bad parameter"}}),
        )],
        None,
    );
    let error = server
        .client()
        .generate(&sources(), &step())
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 400"));
    assert!(!error.contains("PRIVATE_TOKEN"));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn a_second_length_stop_is_not_returned_as_an_answer_or_retried_forever() {
    let server = Server::new(
        vec![
            reply("length", "PRIVATE_PARTIAL"),
            reply("length", "PRIVATE_PARTIAL"),
        ],
        None,
    );
    let error = server
        .client()
        .generate(&sources()[..1], &step())
        .unwrap_err()
        .to_string();
    assert!(!error.contains("PRIVATE_PARTIAL"));
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}

#[test]
fn cancellation_prevents_the_output_retry() {
    let control = AnalysisControl::default();
    let server = Server::new(
        vec![reply("length", "PRIVATE_PARTIAL")],
        Some(control.clone()),
    );
    let error = server
        .client()
        .generate_controlled(&sources()[..1], &step(), &control)
        .unwrap_err();
    assert!(is_cancelled(&error));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn a_failed_smaller_batch_keeps_the_confirmed_checkpoint_byte_identical_on_resume() {
    let inputs = sources();
    let first = json!({"items":[item(&inputs[0])]}).to_string();
    let second = json!({"items":[item(&inputs[1])]}).to_string();
    let context = (400, json!({"error":{"code":"context_length_exceeded"}}));
    let server = Server::new(
        vec![
            reply("stop", &first),
            context.clone(),
            reply("stop", &first),
            (503, json!({"error":"temporarily unavailable"})),
            context,
            reply("stop", &first),
            reply("stop", &second),
        ],
        None,
    );
    let folder = tempfile::tempdir().unwrap();
    let mut last = step();
    last.id = "later".into();
    let template = Template {
        id: "synthetic".into(),
        name: "Synthetic".into(),
        steps: vec![step(), last],
    };
    assert!(analyze(
        folder.path(),
        "11111111111111111111111111111111",
        inputs.clone(),
        template.clone(),
        server.connection(),
        None
    )
    .is_err());
    let run = std::fs::read_dir(folder.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let id = run.file_name().unwrap().to_str().unwrap();
    let checkpoint = run.join("step-0000-000000.json");
    let saved = std::fs::read(&checkpoint).unwrap();
    assert!(!run.join("result.json").exists());
    assert!(!run.join("step-0001-000000.json").exists());
    let result = analyze(
        folder.path(),
        "11111111111111111111111111111111",
        inputs,
        template,
        server.connection(),
        Some(id),
    )
    .unwrap();
    assert_eq!(std::fs::read(checkpoint).unwrap(), saved);
    assert_eq!(result.sections[1].items.len(), 2);
    assert_eq!(
        server.requests.lock().unwrap().len(),
        7,
        "confirmed first step must never be requested again"
    );
}

#[test]
fn smaller_live_overview_batches_never_mark_unsent_windows_as_covered() {
    use crate::live::{LiveRunner, LiveText, Trigger};
    let mut a = sources()[0].clone();
    a.id = "M0".into();
    let mut b = sources()[1].clone();
    b.id = "C30".into();
    let window = |source: &Source| {
        json!({"points":[item(source)],"decisions":[],"tasks":[],"questions":[]}).to_string()
    };
    let overview = |source: &Source| json!({"items":[item(source)]}).to_string();
    let server = Server::new(
        vec![
            reply("stop", &window(&a)),
            (500, json!({"error":"busy"})),
            reply("stop", &window(&b)),
            (
                400,
                json!({"error":{"message":"Context size has been exceeded."}}),
            ),
            reply("stop", &overview(&a)),
            reply("stop", &overview(&b)),
        ],
        None,
    );
    let root = tempfile::tempdir().unwrap();
    let mut runner = LiveRunner::start(
        root.path(),
        "11111111111111111111111111111111",
        server.connection(),
        "de",
    )
    .unwrap();
    let mut texts = vec![LiveText {
        key: "first".into(),
        track: "microphone".into(),
        start_ms: 0,
        end_ms: 30000,
        text: a.text.clone(),
    }];
    let outcome = runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    assert!(outcome.overview_error.is_some());
    let run = root.path().join(&runner.run().id);
    let saved = std::fs::read(run.join("window-000001.json")).unwrap();
    texts.push(LiveText {
        key: "second".into(),
        track: "system".into(),
        start_ms: 30000,
        end_ms: 60000,
        text: b.text.clone(),
    });
    runner
        .update(
            &runner.pending(&texts),
            Trigger::Manual,
            &AnalysisControl::default(),
        )
        .unwrap();
    let first_overview: Value =
        serde_json::from_slice(&std::fs::read(run.join("overview-000001.json")).unwrap()).unwrap();
    assert_eq!(
        first_overview["window"], 1,
        "the smaller first request must cover only the window it received"
    );
    assert_eq!(runner.view().overview_window, 2);
    assert!(!runner.view().overview_pending);
    assert_eq!(
        std::fs::read(run.join("window-000001.json")).unwrap(),
        saved
    );
}

#[test]
fn a_smaller_live_window_keeps_unsent_sources_pending_under_their_original_ids() {
    use crate::live::{LiveRunner, LiveText, Trigger};
    let mut a = sources()[0].clone();
    a.id = "M0".into();
    let b = sources()[1].clone();
    let window = json!({"points":[item(&a)],"decisions":[],"tasks":[],"questions":[]}).to_string();
    let overview = json!({"items":[item(&a)]}).to_string();
    let server = Server::new(
        vec![
            (400, json!({"error":{"code":"context_length_exceeded"}})),
            reply("stop", &window),
            reply("stop", &overview),
        ],
        None,
    );
    let root = tempfile::tempdir().unwrap();
    let mut runner = LiveRunner::start(
        root.path(),
        "11111111111111111111111111111111",
        server.connection(),
        "de",
    )
    .unwrap();
    let texts = vec![
        LiveText {
            key: "first".into(),
            track: "microphone".into(),
            start_ms: 0,
            end_ms: 30000,
            text: a.text,
        },
        LiveText {
            key: "second".into(),
            track: "system".into(),
            start_ms: 30000,
            end_ms: 60000,
            text: b.text.clone(),
        },
    ];
    let before = runner.pending(&texts);
    assert_eq!(before.len(), 2);
    runner
        .update(&before, Trigger::Manual, &AnalysisControl::default())
        .unwrap();
    let after = runner.pending(&texts);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before[1].id);
    assert_eq!(after[0].text, b.text);
    assert_eq!(runner.view().summarized_sections, 1);
}

#[test]
fn recovery_history_retains_the_first_error_and_counts_omissions_within_the_existing_log_bound() {
    fn replies(sources: &[Source], out: &mut Vec<(u16, Value)>) {
        if sources.len() == 1 {
            out.push(reply(
                "stop",
                &json!({"items":[item(&sources[0])]}).to_string(),
            ));
        } else {
            out.push((
                400,
                json!({"error":{"code":"context_length_exceeded","message":"PRIVATE_BODY"}}),
            ));
            let mid = sources.len() / 2;
            replies(&sources[..mid], out);
            replies(&sources[mid..], out);
        }
    }
    let sources: Vec<_> = (0..8)
        .map(|i| Source {
            id: format!("S{i}"),
            source: "microphone".into(),
            start_ms: i * 30000,
            end_ms: (i + 1) * 30000,
            text: format!("Der Bericht {i} bleibt lokal gespeichert."),
        })
        .collect();
    let mut responses = Vec::new();
    replies(&sources, &mut responses);
    let server = Server::new(responses, None);
    let client = server.client();
    let result = client.generate(&sources, &step()).unwrap();
    assert_eq!(
        validate_answer(&result, &sources, &Kind::Summary, "")
            .unwrap()
            .items
            .len(),
        8
    );
    let diagnostic = client.diagnostic();
    assert_eq!(diagnostic.previous_attempts.len(), 8);
    assert_eq!(diagnostic.omitted_attempts, 6);
    assert_eq!(diagnostic.previous_attempts[0].http_status, Some(400));
    let bytes = serde_json::to_vec_pretty(&diagnostic).unwrap();
    assert!(bytes.len() < 4096);
    assert!(!String::from_utf8(bytes).unwrap().contains("PRIVATE_BODY"));
}
