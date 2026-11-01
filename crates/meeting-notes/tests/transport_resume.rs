//! Independent integration regressions with a synthetic loopback HTTP service.
//! No real model, cloud credentials, TLS provider or recording is used here.
use meeting_notes::{
    analyze, analyze_controlled, is_cancelled, list_analyses, read_analysis, read_pending_analysis,
    read_snapshot, AnalysisControl, Connection, Kind, RejectionReason, Rejections, SkipReason,
    Source, Step, Template, TextClient,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const ORIGINAL: &str = "Anna schreibt den Bericht bis Freitag.";
const SYNTHETIC_KEY: &str = "synthetic-test-key-not-a-real-credential";

#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
    location: Option<String>,
    pause: bool,
}

impl Reply {
    fn successful(task: bool) -> Self {
        let answer = json!({"items":[{"text":"Bericht schreiben", "owner":if task {Some("Anna")} else {None}, "due":if task {Some("Freitag")} else {None}, "evidence":[{"sourceId":"S00001","quote":ORIGINAL}]}]});
        Self {
            status: 200,
            body: json!({"choices":[{"finish_reason":"stop","message":{"content":answer.to_string()}}]}).to_string(),
            location: None,
            pause: false,
        }
    }

    fn failed() -> Self {
        Self {
            status: 500,
            body: json!({"error":{"message":"Synthetic unavailable provider"}}).to_string(),
            location: None,
            pause: false,
        }
    }
}

struct SyntheticHttp {
    endpoint: String,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    count: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    cancelled_connection_closed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    // Accepted sockets can inherit nonblocking mode from the listener on Windows.
    // The fixture polls accept, but complete request reads must wait for TCP data.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let header_end;
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 256_000 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            header_end = end + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if length > 256_000 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    while bytes.len() < header_end + length {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(String::from_utf8_lossy(&bytes[..header_end + length]).into_owned())
}

impl SyntheticHttp {
    fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let replies = Arc::new(Mutex::new(VecDeque::from(replies)));
        let count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let cancelled_connection_closed = Arc::new(AtomicBool::new(false));
        let (pause_signal, closed_signal) = (
            Arc::clone(&paused),
            Arc::clone(&cancelled_connection_closed),
        );
        let (queue, seen, incoming, finish) = (
            Arc::clone(&replies),
            Arc::clone(&count),
            Arc::clone(&requests),
            Arc::clone(&stopped),
        );
        let thread = thread::spawn(move || {
            while !finish.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if let Ok(request) = read_request(&mut stream) {
                            incoming.lock().unwrap().push(request);
                            seen.fetch_add(1, Ordering::SeqCst);
                            let reply = queue
                                .lock()
                                .unwrap()
                                .pop_front()
                                .unwrap_or_else(Reply::failed);
                            let location = reply
                                .location
                                .map(|url| format!("Location: {url}\r\n"))
                                .unwrap_or_default();
                            let response = format!("HTTP/1.1 {} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}", reply.status, reply.body.len(), location, reply.body);
                            if reply.pause {
                                let header_end = response.find("\r\n\r\n").unwrap() + 4;
                                stream
                                    .write_all(&response.as_bytes()[..header_end + 1])
                                    .unwrap();
                                pause_signal.store(true, Ordering::SeqCst);
                                stream
                                    .set_read_timeout(Some(Duration::from_secs(3)))
                                    .unwrap();
                                let mut byte = [0_u8; 1];
                                let closed = match stream.read(&mut byte) {
                                    Ok(0) => true,
                                    Err(e)
                                        if [
                                            std::io::ErrorKind::ConnectionAborted,
                                            std::io::ErrorKind::ConnectionReset,
                                        ]
                                        .contains(&e.kind()) =>
                                    {
                                        true
                                    }
                                    _ => false,
                                };
                                closed_signal.store(closed, Ordering::SeqCst);
                            } else {
                                let _ = stream.write_all(response.as_bytes());
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("Synthetic listener failed: {error}"),
                }
            }
        });
        Self {
            endpoint,
            replies,
            count,
            requests,
            stopped,
            paused,
            cancelled_connection_closed,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
    fn queue(&self, reply: Reply) {
        self.replies.lock().unwrap().push_back(reply);
    }
}

impl Drop for SyntheticHttp {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn connection(endpoint: &str) -> Connection {
    Connection {
        endpoint: endpoint.into(),
        model: "synthetic-model".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: SYNTHETIC_KEY.into(),
    }
}
fn sources() -> Vec<Source> {
    vec![Source {
        id: "S00001".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 30_000,
        text: ORIGINAL.into(),
    }]
}
fn template() -> Template {
    Template {
        id: "synthetic-template".into(),
        name: "Synthetic steps".into(),
        steps: vec![
            Step {
                id: "summary".into(),
                name: "Summary".into(),
                kind: Kind::Summary,
                prompt: "Fasse den belegten Text zusammen.".into(),
            },
            Step {
                id: "tasks".into(),
                name: "Tasks".into(),
                kind: Kind::Tasks,
                prompt: "Extrahiere ausdrücklich zugewiesene Aufgaben mit Zitaten.".into(),
            },
        ],
    }
}
fn run_folder(root: &Path) -> PathBuf {
    let entries: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "Exactly one immutable analysis run should exist"
    );
    entries.into_iter().next().unwrap()
}
fn only_run_id(folder: &Path) -> &str {
    folder.file_name().unwrap().to_str().unwrap()
}

#[test]
fn failed_second_step_preserves_first_checkpoint_and_resume_only_requests_missing_work() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    assert_eq!(server.requests(), 2);
    let folder = run_folder(root.path());
    let snapshot = fs::read(folder.join("snapshot.json")).unwrap();
    let confirmed = fs::read(folder.join("step-0000-000000.json")).unwrap();
    assert!(!folder.join("step-0001-000000.json").exists());
    assert!(!folder.join("result.json").exists());
    let pending = read_pending_analysis(&folder).unwrap().unwrap();
    assert_eq!(pending.confirmed_tasks, 1);
    assert_eq!(pending.total_tasks, 2);
    assert_eq!(pending.model, "synthetic-model");
    assert_eq!(
        pending.source_hash,
        meeting_notes::source_hash(&sources()).unwrap()
    );
    assert_eq!(
        server.requests(),
        2,
        "discovery must not issue model requests"
    );
    server.queue(Reply::successful(true));
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&folder)),
    )
    .unwrap();
    assert_eq!(
        server.requests(),
        3,
        "The confirmed first step must not be regenerated"
    );
    assert_eq!(result.sections.len(), 2);
    assert!(read_pending_analysis(&folder).unwrap().is_none());
    assert_eq!(result.sections[1].items[0].owner.as_deref(), Some("Anna"));
    assert_eq!(result.sections[1].items[0].due.as_deref(), Some("Freitag"));
    assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), snapshot);
    assert_eq!(
        fs::read(folder.join("step-0000-000000.json")).unwrap(),
        confirmed
    );
    let finished = fs::read(folder.join("result.json")).unwrap();
    let again = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(&result.id),
    )
    .unwrap();
    assert_eq!(again.id, result.id);
    assert_eq!(
        server.requests(),
        3,
        "A completed run must be read without a model request"
    );
    assert_eq!(fs::read(folder.join("result.json")).unwrap(), finished);
    for entry in fs::read_dir(&folder).unwrap() {
        let data = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(
            !data.contains(SYNTHETIC_KEY),
            "API key must not enter any snapshot, step or export"
        );
    }
    let requests = server.requests.lock().unwrap();
    for request in requests
        .iter()
        .filter(|request| request.starts_with("POST"))
    {
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        let text_sources: Vec<Source> =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            text_sources,
            sources(),
            "Transport must receive only the supplied transcript sources"
        );
        assert!(body.get("session").is_none());
        assert!(body.get("audio").is_none());
    }
}

#[test]
fn pending_discovery_rejects_corrupted_checkpoints_and_snapshot_sources_without_repair() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    let folder = run_folder(root.path());
    let snapshot = fs::read(folder.join("snapshot.json")).unwrap();
    let checkpoint = folder.join("step-0000-000000.json");
    let original_checkpoint = fs::read(&checkpoint).unwrap();
    fs::write(&checkpoint, b"{unfinished synthetic checkpoint").unwrap();
    let damaged = fs::read(&checkpoint).unwrap();
    assert!(read_pending_analysis(&folder).is_err());
    assert_eq!(fs::read(&checkpoint).unwrap(), damaged);
    assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), snapshot);
    fs::write(&checkpoint, original_checkpoint).unwrap();
    let mut changed: Value = serde_json::from_slice(&snapshot).unwrap();
    changed["sources"][0]["text"] = json!("A different original transcript.");
    fs::write(
        folder.join("snapshot.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    let changed = fs::read(folder.join("snapshot.json")).unwrap();
    assert!(read_snapshot(&folder.join("snapshot.json")).is_err());
    assert!(read_pending_analysis(&folder).is_err());
    assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), changed);
    assert_eq!(server.requests(), 2);
}

#[test]
fn changing_model_provider_prompt_or_original_sources_refuses_resume_before_network() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    let folder = run_folder(root.path());
    let id = only_run_id(&folder);
    let snapshot = fs::read(folder.join("snapshot.json")).unwrap();
    let checkpoint = fs::read(folder.join("step-0000-000000.json")).unwrap();
    let mut changed = connection(&server.endpoint);
    changed.model = "different-model".into();
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        changed,
        Some(id)
    )
    .is_err());
    let other_provider = SyntheticHttp::new(vec![Reply::successful(false)]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&other_provider.endpoint),
        Some(id)
    )
    .is_err());
    let mut changed_template = template();
    changed_template.steps[0]
        .prompt
        .push_str(" Neue Anweisung.");
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        changed_template,
        connection(&server.endpoint),
        Some(id)
    )
    .is_err());
    let mut changed_sources = sources();
    changed_sources[0].text = "Anna schreibt einen anderen Bericht bis Freitag.".into();
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        changed_sources,
        template(),
        connection(&server.endpoint),
        Some(id)
    )
    .is_err());
    assert_eq!(server.requests(), 2);
    assert_eq!(other_provider.requests(), 0);
    assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), snapshot);
    assert_eq!(
        fs::read(folder.join("step-0000-000000.json")).unwrap(),
        checkpoint
    );
}

#[test]
fn tampered_final_quotes_or_source_hash_are_rejected_without_requesting_a_replacement() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::successful(true)]);
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None,
    )
    .unwrap();
    let path = root.path().join(&result.id).join("result.json");
    let original = fs::read(&path).unwrap();
    let mut hash_tamper: Value = serde_json::from_slice(&original).unwrap();
    hash_tamper["sourceHash"] = "changed-hash".into();
    fs::write(&path, serde_json::to_vec(&hash_tamper).unwrap()).unwrap();
    assert!(read_analysis(&path).is_err());
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(&result.id)
    )
    .is_err());
    let mut quote_tamper: Value = serde_json::from_slice(&original).unwrap();
    quote_tamper["sections"][0]["items"][0]["evidence"][0]["quote"] =
        "Ben verfasst die Abrechnung bis Dienstag.".into();
    fs::write(&path, serde_json::to_vec(&quote_tamper).unwrap()).unwrap();
    assert!(read_analysis(&path).is_err());
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(&result.id)
    )
    .is_err());
    assert_eq!(
        server.requests(),
        2,
        "Corruption must not silently trigger paid or changed replacement analysis"
    );
}

#[test]
fn corrupt_confirmed_step_is_retained_and_blocks_resume_before_the_missing_request() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    let folder = run_folder(root.path());
    let step = folder.join("step-0000-000000.json");
    let corrupt=br#"{"items":[{"text":"Invented","owner":null,"due":null,"evidence":[{"sourceId":"S99999","quote":"Anna schreibt den Bericht bis Freitag."}]}]}"#;
    fs::write(&step, corrupt).unwrap();
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&folder))
    )
    .is_err());
    assert_eq!(server.requests(), 2);
    assert_eq!(fs::read(step).unwrap(), corrupt);
}

#[test]
fn local_redirect_never_reaches_the_second_listener_and_remote_hosts_are_rejected() {
    let target = SyntheticHttp::new(vec![Reply::successful(false)]);
    let initial = SyntheticHttp::new(vec![Reply {
        status: 302,
        body: String::new(),
        location: Some(format!("{}models", target.endpoint)),
        pause: false,
    }]);
    let client = TextClient::new(&connection(&initial.endpoint)).unwrap();
    assert!(client.models().is_err());
    assert_eq!(initial.requests(), 1);
    assert_eq!(target.requests(), 0);
    for endpoint in [
        "https://example.invalid/v1/",
        "http://127.0.0.1@example.invalid/v1/",
        "http://127.0.0.1/v1/?credential=synthetic",
    ] {
        assert!(TextClient::new(&connection(endpoint)).is_err());
    }
    assert_eq!(target.requests(), 0);
}

#[test]
fn simulated_cloud_configuration_requires_each_run_consent_and_https_without_real_provider_calls() {
    // Configuration simulation only: no real cloud key, TLS server or provider is exercised.
    let untouched = SyntheticHttp::new(Vec::new());
    let mut config = connection(&untouched.endpoint.replace("http:", "https:"));
    config.mode = "cloud".into();
    assert!(TextClient::new(&config).is_err());
    config.cloud_consent = true;
    assert!(TextClient::new(&config).is_ok());
    config.cloud_consent = false;
    assert!(TextClient::new(&config).is_err());
    config.cloud_consent = true;
    config.endpoint = untouched.endpoint.clone();
    assert!(TextClient::new(&config).is_err());
    assert_eq!(untouched.requests(), 0);
}

#[test]
fn local_discovery_is_get_only_without_keys_sources_or_cloud_detection() {
    let server=SyntheticHttp::new(vec![Reply{status:200,body:json!({"data":[{"id":"zeta"},{"id":"alpha"},{"id":"alpha"},{"id":""},{"id":"bad\nmodel"},{"id":42}]}).to_string(),location:None,pause:false}]);
    let client = TextClient::new(&connection(&server.endpoint)).unwrap();
    assert_eq!(client.models().unwrap(), ["alpha", "zeta"]);
    let incoming = server.requests.lock().unwrap();
    assert_eq!(incoming.len(), 1);
    let request = &incoming[0];
    assert!(request.starts_with("GET /v1/models HTTP/1.1"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    assert!(request.split_once("\r\n\r\n").unwrap().1.is_empty());
    assert!(!request.contains(ORIGINAL));
    assert!(!request.contains(SYNTHETIC_KEY));
    drop(incoming);
    assert!(meeting_notes::local_model_ids("https://provider.example/v1/").is_err());
    let mut cloud = connection("https://provider.example/v1/");
    cloud.mode = "cloud".into();
    cloud.cloud_consent = true;
    assert!(TextClient::new(&cloud).unwrap().models().is_err());
    assert_eq!(server.requests(), 1);
}

#[test]
fn credential_identity_is_bound_to_validated_route_port_and_base_path() {
    let scope = meeting_notes::credential_account("http://localhost:1234/v1", "local").unwrap();
    assert_eq!(
        scope,
        meeting_notes::credential_account("http://127.0.0.1:1234/v1/", "local").unwrap()
    );
    for (url, mode) in [
        ("http://127.0.0.1:1235/v1/", "local"),
        ("http://127.0.0.1:1234/other/", "local"),
        ("https://127.0.0.1:1234/v1/", "cloud"),
    ] {
        assert_ne!(scope, meeting_notes::credential_account(url, mode).unwrap());
    }
    for (url, mode) in [
        ("https://provider.example/v1/", "local"),
        ("http://provider.example/v1/", "cloud"),
        ("https://key@provider.example/v1/", "cloud"),
        ("https://provider.example/v1/?key=secret", "cloud"),
    ] {
        assert!(meeting_notes::credential_account(url, mode).is_err());
    }
    assert!(!scope.contains("1234"));
    assert_eq!(scope.len(), 64);
}

#[test]
fn cancellation_interrupts_partial_http_body_and_resume_preserves_confirmed_work() {
    let root = tempfile::tempdir().unwrap();
    let mut slow = Reply::successful(true);
    slow.pause = true;
    let server = SyntheticHttp::new(vec![Reply::successful(false), slow]);
    let control = AnalysisControl::default();
    let worker_control = control.clone();
    let endpoint = server.endpoint.clone();
    let path = root.path().to_path_buf();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let worker_progress = Arc::clone(&progress);
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let result = analyze_controlled(
            &path,
            "synthetic-meeting",
            sources(),
            template(),
            connection(&endpoint),
            None,
            &worker_control,
            |p| worker_progress.lock().unwrap().push(p),
        );
        tx.send(result).unwrap();
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !server.paused.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        server.paused.load(Ordering::SeqCst),
        "Fixture must pause inside a real partial response body"
    );
    control.cancel();
    let result = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Cancellation must interrupt waiting HTTP without waiting for provider timeout");
    assert!(is_cancelled(&result.unwrap_err()));
    worker.join().unwrap();
    let folder = run_folder(root.path());
    let snapshot = fs::read(folder.join("snapshot.json")).unwrap();
    let committed = fs::read(folder.join("step-0000-000000.json")).unwrap();
    assert!(!folder.join("step-0001-000000.json").exists());
    assert!(!folder.join("result.json").exists());
    let last = progress.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.stage, "requesting");
    assert_eq!(last.step_index, 2);
    assert_eq!(last.batch_index, 1);
    assert_eq!(last.confirmed_tasks, 1);
    assert_eq!(last.total_tasks, 2);
    let close_deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !server.cancelled_connection_closed.load(Ordering::SeqCst)
        && std::time::Instant::now() < close_deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        server.cancelled_connection_closed.load(Ordering::SeqCst),
        "Cancelling must drop the incomplete HTTP connection"
    );
    server.queue(Reply::successful(true));
    let mut resumed_progress = vec![];
    let result = analyze_controlled(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&folder)),
        &AnalysisControl::default(),
        |p| resumed_progress.push(p),
    )
    .unwrap();
    assert_eq!(
        server.requests(),
        3,
        "Only the unconfirmed task may be requested again"
    );
    assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), snapshot);
    assert_eq!(
        fs::read(folder.join("step-0000-000000.json")).unwrap(),
        committed
    );
    let finished = resumed_progress.last().unwrap();
    assert_eq!(finished.stage, "finished");
    assert_eq!(finished.confirmed_tasks, 2);
    assert_eq!(finished.total_tasks, 2);
    assert_eq!(
        read_analysis(&folder.join("result.json")).unwrap().id,
        result.id
    );
    assert!(folder.join("result.md").exists());
}

#[test]
fn cancellation_after_completion_commit_does_not_hide_the_finished_result() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::successful(true)]);
    let control = AnalysisControl::default();
    let result = analyze_controlled(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None,
        &control,
        |p| {
            if p.stage == "finished" {
                control.cancel();
            }
        },
    )
    .unwrap();
    assert!(control.is_cancelled());
    assert_eq!(
        read_analysis(&root.path().join(&result.id).join("result.json"))
            .unwrap()
            .id,
        result.id
    );
    let mut finished = None;
    let reopened = analyze_controlled(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(&result.id),
        &AnalysisControl::default(),
        |p| finished = Some(p),
    )
    .unwrap();
    assert_eq!(reopened.id, result.id);
    let finished = finished.unwrap();
    assert_eq!(finished.stage, "finished");
    assert_eq!(finished.confirmed_tasks, 2);
    assert_eq!(finished.total_tasks, 2);
    assert_eq!(
        server.requests(),
        2,
        "Reading the committed result must not repeat inference"
    );
}

fn name_reply(text: &str, owner: Option<&str>) -> Reply {
    let answer = json!({"items":[{"text":text, "owner":owner, "due":null, "evidence":[{"sourceId":"S00001","quote":ORIGINAL}]}]});
    Reply {
        status: 200,
        body:
            json!({"choices":[{"finish_reason":"stop","message":{"content":answer.to_string()}}]})
                .to_string(),
        location: None,
        pause: false,
    }
}

#[test]
fn invalid_name_candidates_are_discarded_per_item_and_legacy_batches_never_block_resume() {
    let root = tempfile::tempdir().unwrap();
    let template = meeting_notes::name_candidates::template().unwrap();
    // The literal name is missing from its quote: valid JSON evidence, invalid candidate.
    let server = SyntheticHttp::new(vec![name_reply("Bernd", None)]);
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template.clone(),
        connection(&server.endpoint),
        None,
    )
    .unwrap();
    assert!(result.sections[0].items.is_empty());
    assert_eq!(
        result.sections[0].rejected,
        Rejections::from([(RejectionReason::InvalidNameCandidate, 1)])
    );
    let folder = run_folder(root.path());
    for entry in fs::read_dir(&folder).unwrap() {
        let data = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(
            !data.contains("Bernd"),
            "A discarded candidate must never be stored as a result"
        );
    }

    // A batch saved by an earlier build before per-batch checks existed.
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(vec![Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template.clone(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    let folder = run_folder(root.path());
    let step = folder.join("step-0000-000000.json");
    assert!(
        !step.exists(),
        "A failed request must not leave a checkpoint"
    );
    fs::write(
        &step,
        json!({"items":[{"text":"Bernd","owner":null,"due":null,"evidence":[{"sourceId":"S00001","quote":ORIGINAL}]}]}).to_string(),
    )
    .unwrap();
    server.queue(name_reply("Anna", None));
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template,
        connection(&server.endpoint),
        Some(only_run_id(&folder)),
    )
    .unwrap();
    assert_eq!(result.sections[0].items[0].text, "Anna");
    assert!(result.sections[0].items[0].owner.is_none());
    assert!(
        folder.join("step-0000-000000.rejected-0.json").is_file(),
        "The rejected answer is kept as evidence"
    );
}

fn reply_items(items: Value) -> Reply {
    let answer = json!({ "items": items });
    Reply {
        status: 200,
        body:
            json!({"choices":[{"finish_reason":"stop","message":{"content":answer.to_string()}}]})
                .to_string(),
        location: None,
        pause: false,
    }
}
const INVENTED: &str = "Ben verfasst die Abrechnung bis Dienstag.";

#[test]
fn invalid_statements_are_discarded_counted_and_only_verified_ones_are_stored() {
    let root = tempfile::tempdir().unwrap();
    let valid = json!({"text":"Bericht wird geschrieben","owner":null,"due":null,"evidence":[{"sourceId":"S00001","quote":ORIGINAL}]});
    let invented = json!({"text":"Erfundene Abrechnung","owner":null,"due":null,"evidence":[{"sourceId":"S00001","quote":INVENTED}]});
    let unknown_source = json!({"text":"Unbekannte Quelle","owner":null,"due":null,"evidence":[{"sourceId":"S99999","quote":ORIGINAL}]});
    let invented_owner = json!({"text":"Bericht","owner":"Ben","due":null,"evidence":[{"sourceId":"S00001","quote":ORIGINAL}]});
    let task = json!({"text":"Bericht schreiben","owner":"Anna","due":"Freitag","evidence":[{"sourceId":"S00001","quote":ORIGINAL}]});
    let summary_reply = reply_items(json!([invented, valid, unknown_source]));
    let task_reply = reply_items(json!([invented_owner, task]));
    let server = SyntheticHttp::new(vec![summary_reply, task_reply]);
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None,
    )
    .unwrap();
    assert_eq!(server.requests(), 2, "Partial answers must not be retried");
    assert_eq!(result.result_schema, meeting_notes::RESULT_SCHEMA);
    let summary = &result.sections[0];
    assert_eq!(summary.items.len(), 1);
    assert_eq!(summary.items[0].text, "Bericht wird geschrieben");
    assert_eq!(
        summary.rejected,
        Rejections::from([(RejectionReason::QuoteNotLiteral, 2)])
    );
    let tasks = &result.sections[1];
    assert_eq!(tasks.items.len(), 1);
    assert_eq!(tasks.items[0].owner.as_deref(), Some("Anna"));
    assert_eq!(
        tasks.rejected,
        Rejections::from([(RejectionReason::UnverifiedOwnerOrDue, 1)])
    );
    let folder = root.path().join(&result.id);
    let stored = read_analysis(&folder.join("result.json")).unwrap();
    assert_eq!(stored.sections[0].rejected, summary.rejected);
    for section in &stored.sections {
        for fact in &section.items {
            for evidence in &fact.evidence {
                let source = stored
                    .sources
                    .iter()
                    .find(|s| s.id == evidence.source_id)
                    .expect("kept evidence must cite an existing source");
                assert!(source.text.contains(&evidence.quote));
            }
        }
    }
    let checkpoint: Value =
        serde_json::from_slice(&fs::read(folder.join("step-0000-000000.json")).unwrap()).unwrap();
    assert_eq!(checkpoint["rejected"], json!({"quoteNotLiteral": 2}));
    assert_eq!(checkpoint["items"].as_array().unwrap().len(), 1);
    for entry in fs::read_dir(&folder).unwrap() {
        let data = fs::read_to_string(entry.unwrap().path()).unwrap();
        for discarded in [
            INVENTED,
            "Erfundene Abrechnung",
            "Unbekannte Quelle",
            "\"Ben\"",
        ] {
            assert!(
                !data.contains(discarded),
                "Discarded model text must not be stored: {discarded}"
            );
        }
    }
    let markdown = fs::read_to_string(folder.join("result.md")).unwrap();
    assert!(markdown.contains("Hinweis: 2 KI-Aussage(n) wurden verworfen"));
    let text = fs::read_to_string(folder.join("result.txt")).unwrap();
    assert!(text.contains("Hinweis: 1 KI-Aussage(n) wurden verworfen"));

    // Reopening the finished run neither re-requests nor changes the stored counts.
    let finished = fs::read(folder.join("result.json")).unwrap();
    let again = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(&result.id),
    )
    .unwrap();
    assert_eq!(again.sections[0].rejected, summary.rejected);
    assert_eq!(server.requests(), 2);
    assert_eq!(fs::read(folder.join("result.json")).unwrap(), finished);
}

#[test]
fn structurally_invalid_answers_still_fail_the_step_without_a_checkpoint() {
    for content in [
        "not json".to_string(),
        json!({"items":[{"text":"Bericht","evidence":[{"sourceId":"S00001","quote":ORIGINAL}]}]})
            .to_string(),
        json!({"items":[], "notes":"extra field"}).to_string(),
    ] {
        let root = tempfile::tempdir().unwrap();
        let server = SyntheticHttp::new(vec![Reply {
            status: 200,
            body: json!({"choices":[{"finish_reason":"stop","message":{"content":content}}]})
                .to_string(),
            location: None,
            pause: false,
        }]);
        assert!(analyze(
            root.path(),
            "synthetic-meeting",
            sources(),
            template(),
            connection(&server.endpoint),
            None
        )
        .is_err());
        let folder = run_folder(root.path());
        assert!(!folder.join("step-0000-000000.json").exists());
        assert!(!folder.join("result.json").exists());
    }
}

#[test]
fn resumed_partial_checkpoint_keeps_counts_and_tampered_checkpoint_items_block() {
    let root = tempfile::tempdir().unwrap();
    let valid = json!({"text":"Bericht wird geschrieben","owner":null,"due":null,"evidence":[{"sourceId":"S00001","quote":ORIGINAL}]});
    let invented = json!({"text":"Erfunden","owner":null,"due":null,"evidence":[{"sourceId":"S00001","quote":INVENTED}]});
    let server = SyntheticHttp::new(vec![reply_items(json!([valid, invented])), Reply::failed()]);
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None
    )
    .is_err());
    let folder = run_folder(root.path());
    let step = folder.join("step-0000-000000.json");
    let checkpoint = fs::read(&step).unwrap();
    assert_eq!(
        read_pending_analysis(&folder)
            .unwrap()
            .unwrap()
            .confirmed_tasks,
        1
    );
    // A saved checkpoint is read strictly: an item edited to an invented quote blocks resume.
    let mut tampered: Value = serde_json::from_slice(&checkpoint).unwrap();
    tampered["items"][0]["evidence"][0]["quote"] = INVENTED.into();
    fs::write(&step, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(read_pending_analysis(&folder).is_err());
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&folder))
    )
    .is_err());
    assert_eq!(
        server.requests(),
        2,
        "A damaged checkpoint is never repaired by a new request"
    );
    fs::write(&step, &checkpoint).unwrap();
    server.queue(Reply::successful(true));
    let result = analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&folder)),
    )
    .unwrap();
    assert_eq!(server.requests(), 3);
    assert_eq!(
        result.sections[0].rejected,
        Rejections::from([(RejectionReason::QuoteNotLiteral, 1)])
    );
    assert!(result.sections[1].rejected.is_empty());
    assert_eq!(fs::read(&step).unwrap(), checkpoint);
}

fn finished_run(root: &Path, server: &SyntheticHttp) -> PathBuf {
    server.queue(Reply::successful(false));
    server.queue(Reply::successful(true));
    let result = analyze(
        root,
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        None,
    )
    .unwrap();
    root.join(result.id)
}
fn rewrite(path: &Path, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    change(&mut value);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}
/// Shape of a result written by 1.0.2: no result schema and no rejection counts.
fn as_legacy_result(value: &mut Value) {
    value.as_object_mut().unwrap().remove("resultSchema");
    for section in value["sections"].as_array_mut().unwrap() {
        section.as_object_mut().unwrap().remove("rejected");
    }
}

#[test]
fn earlier_prompt_results_stay_listed_read_only_and_damaged_files_are_skipped_untouched() {
    let root = tempfile::tempdir().unwrap();
    let server = SyntheticHttp::new(Vec::new());
    let current = finished_run(root.path(), &server);
    let earlier = finished_run(root.path(), &server);
    let legacy = finished_run(root.path(), &server);
    let damaged = finished_run(root.path(), &server);
    let newer = finished_run(root.path(), &server);
    let forged = finished_run(root.path(), &server);
    let foreign = finished_run(root.path(), &server);
    let earlier_bytes = rewrite(&earlier.join("result.json"), |v| {
        v["promptVersion"] = "handymeeting-rust-notes-1".into();
        as_legacy_result(v);
    });
    // A 1.0.2 result of the current prompt version stays readable as schema 1.
    rewrite(&legacy.join("result.json"), as_legacy_result);
    fs::write(damaged.join("result.json"), b"{\"id\":").unwrap();
    rewrite(&newer.join("result.json"), |v| {
        v["resultSchema"] = 99.into()
    });
    // Schema 1 never discarded statements, so a recorded count there is forged.
    rewrite(&forged.join("result.json"), |v| {
        as_legacy_result(v);
        v["sections"][0]["rejected"] = json!({"quoteNotLiteral": 1});
    });
    rewrite(&foreign.join("result.json"), |v| {
        v["meetingId"] = "other-meeting".into()
    });
    fs::create_dir(root.path().join("not-a-run")).unwrap();
    let before: Vec<_> = [
        &current, &earlier, &legacy, &damaged, &newer, &forged, &foreign,
    ]
    .iter()
    .map(|folder| fs::read(folder.join("result.json")).unwrap())
    .collect();

    let listing = list_analyses(root.path(), "synthetic-meeting").unwrap();
    let id = |folder: &PathBuf| only_run_id(folder).to_string();
    let mut listed: Vec<_> = listing
        .analyses
        .iter()
        .map(|item| (item.analysis.id.clone(), item.current_prompt))
        .collect();
    listed.sort();
    let mut expected = vec![
        (id(&current), true),
        (id(&earlier), false),
        (id(&legacy), true),
    ];
    expected.sort();
    assert_eq!(listed, expected);
    let old = listing
        .analyses
        .iter()
        .find(|item| item.analysis.id == id(&earlier))
        .unwrap();
    assert_eq!(old.analysis.prompt_version, "handymeeting-rust-notes-1");
    assert_eq!(old.analysis.result_schema, 1);
    let public = serde_json::to_value(old).unwrap();
    assert_eq!(public["currentPrompt"], json!(false));
    assert_eq!(public["promptVersion"], json!("handymeeting-rust-notes-1"));
    let mut skipped: Vec<_> = listing
        .skipped
        .iter()
        .map(|item| (item.id.clone(), item.reason))
        .collect();
    skipped.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = vec![
        (id(&damaged), SkipReason::Damaged),
        (id(&newer), SkipReason::NewerFormat),
        (id(&forged), SkipReason::Damaged),
        (id(&foreign), SkipReason::OtherMeeting),
    ];
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(skipped, expected);
    let after: Vec<_> = [
        &current, &earlier, &legacy, &damaged, &newer, &forged, &foreign,
    ]
    .iter()
    .map(|folder| fs::read(folder.join("result.json")).unwrap())
    .collect();
    assert_eq!(
        before, after,
        "Listing must never modify stored work states"
    );

    // The earlier version renders read-only, but is never reopened as current work.
    assert!(
        meeting_notes::render_markdown(&read_analysis(&earlier.join("result.json")).unwrap())
            .is_ok()
    );
    rewrite(&earlier.join("snapshot.json"), |v| {
        v["promptVersion"] = "handymeeting-rust-notes-1".into()
    });
    let requests = server.requests();
    assert!(analyze(
        root.path(),
        "synthetic-meeting",
        sources(),
        template(),
        connection(&server.endpoint),
        Some(only_run_id(&earlier))
    )
    .is_err());
    assert_eq!(server.requests(), requests);
    assert_eq!(
        fs::read(earlier.join("result.json")).unwrap(),
        earlier_bytes
    );
}

#[test]
fn unfinished_run_of_an_earlier_prompt_is_listed_read_only_and_never_resumed() {
    for old_prompt in ["handymeeting-rust-notes-1", "handymeeting-rust-notes-3"] {
        let root = tempfile::tempdir().unwrap();
        let server = SyntheticHttp::new(vec![Reply::successful(false), Reply::failed()]);
        assert!(analyze(
            root.path(),
            "synthetic-meeting",
            sources(),
            template(),
            connection(&server.endpoint),
            None
        )
        .is_err());
        let folder = run_folder(root.path());
        let current = read_pending_analysis(&folder).unwrap().unwrap();
        assert!(current.resumable);
        assert_eq!(current.prompt_version, meeting_notes::PROMPT_VERSION);
        let snapshot = rewrite(&folder.join("snapshot.json"), |v| {
            v["promptVersion"] = old_prompt.into();
            if old_prompt == "handymeeting-rust-notes-1" {
                v.as_object_mut().unwrap().remove("resultSchema");
            }
        });
        let checkpoint = fs::read(folder.join("step-0000-000000.json")).unwrap();
        let pending = read_pending_analysis(&folder).unwrap().unwrap();
        assert!(!pending.resumable);
        assert_eq!(pending.prompt_version, old_prompt);
        assert_eq!(pending.confirmed_tasks, 1);
        let error = read_snapshot(&folder.join("snapshot.json")).unwrap_err();
        assert!(error.to_string().contains("anderen Promptfassung"));
        server.queue(Reply::successful(true));
        assert!(analyze(
            root.path(),
            "synthetic-meeting",
            sources(),
            template(),
            connection(&server.endpoint),
            Some(only_run_id(&folder))
        )
        .is_err());
        assert_eq!(
            server.requests(),
            2,
            "An earlier prompt version must not be continued"
        );
        assert!(!folder.join("step-0001-000000.json").exists());
        assert!(!folder.join("result.json").exists());
        assert_eq!(fs::read(folder.join("snapshot.json")).unwrap(), snapshot);
        assert_eq!(
            fs::read(folder.join("step-0000-000000.json")).unwrap(),
            checkpoint
        );
        // A snapshot from an unknown newer storage format is reported as such, not guessed at.
        rewrite(&folder.join("snapshot.json"), |v| {
            v["resultSchema"] = 99.into()
        });
        let error = read_pending_analysis(&folder).unwrap_err();
        assert_eq!(meeting_notes::skip_reason(&error), SkipReason::NewerFormat);
    }
}
