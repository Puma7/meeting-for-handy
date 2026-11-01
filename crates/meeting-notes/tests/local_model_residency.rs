//! Synthetic native-provider protocol tests. No model, audio or external service is used.
use meeting_notes::{
    residency::{LocalModelPool, LocalProvider},
    AnalysisControl, Connection, Kind, Source, Step, TextClient,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Server {
    /// Starts only this fixture-owned bounded loopback HTTP server; no actual model service is contacted.
    fn new(handler: impl FnMut(&str, &Value) -> (u16, Value) + Send + 'static) -> Self {
        Self::on(TcpListener::bind("127.0.0.1:0").unwrap(), handler)
    }
    /// Serves only the caller-owned concrete IPv4/IPv6 fixture listener for origin-binding checks.
    fn on(
        listener: TcpListener,
        mut handler: impl FnMut(&str, &Value) -> (u16, Value) + Send + 'static,
    ) -> Self {
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, stopped) = (Arc::clone(&requests), Arc::clone(&stop));
        let handle = thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (path, body) = read(&mut stream);
                        seen.lock().unwrap().push((path.clone(), body.clone()));
                        let (status, reply) = handler(&path, &body);
                        let bytes = reply.to_string();
                        let response = format!("HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}", bytes.len());
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            handle: Some(handle),
        }
    }
    /// Snapshots protocol requests so preservation and cleanup assertions inspect the actual wire contract.
    fn seen(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }
    /// Builds a local-only synthetic connection without credentials or cloud permission.
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
    /// Stops and joins only this fixture server thread, leaving application and user processes untouched.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.handle.take().unwrap().join().unwrap();
    }
}
/// Parses bounded fixture HTTP bodies to distinguish management requests from inference text.
fn read(stream: &mut TcpStream) -> (String, Value) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 256_000);
        if let Some(i) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let size: usize = head
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.trim().parse().unwrap())
        .unwrap_or(0);
    assert!(size < 256_000);
    while bytes.len() < end + size {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    let path = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    (
        path,
        if size == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + size]).unwrap()
        },
    )
}
/// Provides deterministic native Ollama acknowledgements without model or audio access.
fn ollama(path: &str, _: &Value) -> (u16, Value) {
    match path {
        "/api/ps" => (200, json!({"models":[]})),
        "/api/generate" => (200, json!({"done":true,"done_reason":"load"})),
        "/api/chat" => (
            200,
            json!({"done":true,"done_reason":"stop","message":{"content":"{\"items\":[]}"},"eval_count":17,"prompt_eval_count":29}),
        ),
        _ => (500, json!({})),
    }
}
/// Separates foreign and owned synthetic LM Studio instance IDs for cleanup assertions.
fn lm(path: &str, body: &Value) -> (u16, Value) {
    match path {
        "/api/v1/models" => (
            200,
            json!({"models":[{"type":"llm","key":"synthetic","loaded_instances":[]},{"type":"llm","key":"foreign-key","loaded_instances":[{"id":"foreign"}]}]}),
        ),
        "/api/v1/models/load" => (
            200,
            json!({"type":"llm","status":"loaded","instance_id":"owned"}),
        ),
        "/api/v1/models/unload" => (200, json!({"instance_id":body["instance_id"]})),
        "/v1/chat/completions" => (
            200,
            json!({"choices":[{"finish_reason":"stop","message":{"content":"{\"items\":[]}"}}]}),
        ),
        _ => (500, json!({})),
    }
}
/// Holds an actual fixture cleanup response until the test explicitly releases it.
fn gated_cleanup(
    provider: LocalProvider,
) -> (
    Server,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::SyncSender<()>,
) {
    let (entered, observed) = std::sync::mpsc::sync_channel(1);
    let (release, allowed) = std::sync::mpsc::sync_channel(1);
    let server = Server::new(move |path, body| {
        let cleanup = match provider {
            LocalProvider::Ollama => path == "/api/generate" && body["keep_alive"] == 0,
            LocalProvider::LmStudio => path == "/api/v1/models/unload",
        };
        if cleanup {
            entered.send(()).unwrap();
            allowed.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        match provider {
            LocalProvider::Ollama => ollama(path, body),
            LocalProvider::LmStudio => lm(path, body),
        }
    });
    (server, observed, release)
}
/// Proves a pending native cleanup cannot hold the registry needed by another model.
fn check_unrelated_progress_during_cleanup(provider: LocalProvider) {
    let (slow, entered, release) = gated_cleanup(provider);
    let fast = Server::new(move |path, body| match provider {
        LocalProvider::Ollama => ollama(path, body),
        LocalProvider::LmStudio => {
            let (status, mut reply) = lm(path, body);
            if path == "/api/v1/models" {
                reply["models"][0]["key"] = json!("unrelated");
            }
            (status, reply)
        }
    });
    let pool = Arc::new(LocalModelPool::default());
    let lease = pool.reserve(&slow.connection(), provider).unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    let cleanup = thread::spawn(move || drop(lease));
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut connection = fast.connection();
    connection.model = "unrelated".into();
    let other_pool = Arc::clone(&pool);
    let (done, observed) = std::sync::mpsc::sync_channel(1);
    let other = thread::spawn(move || {
        let lease = other_pool.reserve(&connection, provider).unwrap();
        let client = TextClient::new(&connection)
            .unwrap()
            .with_residency(lease)
            .unwrap();
        generate(&client).unwrap();
        drop(client);
        done.send(()).unwrap();
    });
    let progressed = observed.recv_timeout(Duration::from_secs(2)).is_ok();
    // Always release/join the test's own threads before reporting a regression.
    release.send(()).unwrap();
    cleanup.join().unwrap();
    other.join().unwrap();
    assert!(
        progressed,
        "Unrelated reservation/inference waited for native cleanup"
    );
    assert!(fast
        .seen()
        .iter()
        .any(|(path, _)| path.ends_with("/chat") || path.ends_with("/completions")));
}
/// Ollama logical release must finish without invoking even a deliberately gated
/// destructive endpoint, then leave the shared registry usable by another model.
#[test]
fn ollama_final_release_avoids_global_cleanup_and_keeps_unrelated_work_usable() {
    let (server, entered, release) = gated_cleanup(LocalProvider::Ollama);
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    let (done, finished) = std::sync::mpsc::sync_channel(1);
    let cleanup = thread::spawn(move || {
        drop(lease);
        done.send(()).unwrap();
    });
    let immediate = finished.recv_timeout(Duration::from_secs(2)).is_ok();
    let destructive_request = entered.try_recv().is_ok();
    release.send(()).unwrap();
    cleanup.join().unwrap();
    assert!(
        immediate && !destructive_request,
        "Shared Ollama cleanup was attempted"
    );
    assert_eq!(server.seen().len(), 2);
    let mut connection = server.connection();
    connection.model = "unrelated".into();
    let lease = pool.reserve(&connection, LocalProvider::Ollama).unwrap();
    let client = TextClient::new(&connection)
        .unwrap()
        .with_residency(lease)
        .unwrap();
    generate(&client).unwrap();
}
/// Exercises the native LM Studio unload path while another model completes inference.
#[test]
fn slow_lm_studio_final_cleanup_does_not_block_unrelated_model_work() {
    check_unrelated_progress_during_cleanup(LocalProvider::LmStudio);
}
/// A newly inserted loopback alias must not load until its family's old cleanup acknowledges.
#[test]
fn a_new_lm_alias_waits_for_pending_cleanup_then_keeps_its_own_lease() {
    let provider = LocalProvider::LmStudio;
    let (server, entered, release) = gated_cleanup(provider);
    let pool = Arc::new(LocalModelPool::default());
    let first = pool.reserve(&server.connection(), provider).unwrap();
    first.prepare(&AnalysisControl::default()).unwrap();
    let cleanup = thread::spawn(move || drop(first));
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut alias = server.connection();
    alias.endpoint = alias.endpoint.replace("127.0.0.1", "localhost");
    let other_pool = Arc::clone(&pool);
    let (started, observed_start) = std::sync::mpsc::sync_channel(1);
    let (reserved, observed) = std::sync::mpsc::sync_channel(1);
    let reserve = thread::spawn(move || {
        started.send(()).unwrap();
        let lease = other_pool.reserve(&alias, provider).unwrap();
        lease.prepare(&AnalysisControl::default()).unwrap();
        reserved.send(lease).unwrap();
    });
    observed_start.recv_timeout(Duration::from_secs(2)).unwrap();
    let early = observed.recv_timeout(Duration::from_millis(150));
    let waited = early.is_err();
    release.send(()).unwrap();
    cleanup.join().unwrap();
    let lease = early.unwrap_or_else(|_| observed.recv_timeout(Duration::from_secs(2)).unwrap());
    reserve.join().unwrap();
    let unloads = |seen: &[(String, Value)]| {
        seen.iter()
            .filter(|(path, body)| {
                path == "/api/v1/models/unload"
                    || (path == "/api/generate" && body["keep_alive"] == 0)
            })
            .count()
    };
    assert_eq!(unloads(&server.seen()), 1, "New active alias was unloaded");
    release.send(()).unwrap();
    drop(lease);
    assert_eq!(unloads(&server.seen()), 2);
    assert!(waited, "Same-model reservation overtook pending cleanup");
}
/// Ollama aliases read fresh provider state after logical release without any global eviction.
#[test]
fn a_new_ollama_alias_after_release_keeps_its_own_finite_reservation() {
    let (server, lifetime) = stateful_ollama(0);
    let pool = LocalModelPool::default();
    let first = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    first.prepare(&AnalysisControl::default()).unwrap();
    drop(first);
    let mut connection = server.connection();
    connection.endpoint = connection.endpoint.replace("127.0.0.1", "localhost");
    connection.model.push_str(":latest");
    let alias = pool.reserve(&connection, LocalProvider::Ollama).unwrap();
    alias.prepare(&AnalysisControl::default()).unwrap();
    assert!((1..=300).contains(&lifetime.load(Ordering::Acquire)));
    let before = server.seen();
    drop(alias);
    assert_eq!(server.seen(), before);
    assert!((1..=300).contains(&lifetime.load(Ordering::Acquire)));
}

/// Native authority for localhost's configured IPv4 identity must never reach an
/// IPv6-only same-port listener; explicit IPv6 remains a separately valid route.
#[test]
fn review_native_localhost_management_rejects_an_ipv6_only_listener() {
    let mut outcomes = Vec::new();
    for provider in [LocalProvider::Ollama, LocalProvider::LmStudio] {
        let (ipv4, listener) = native_loopback_pair(TcpListener::bind("127.0.0.1:0").unwrap());
        drop(ipv4); // The IPv4 side is deliberately absent before testing the IPv6-only peer.
        let server = Server::on(listener, move |p, b| match provider {
            LocalProvider::Ollama => ollama(p, b),
            LocalProvider::LmStudio => lm(p, b),
        });
        let mut alias = server.connection();
        alias.endpoint = alias.endpoint.replace("[::1]", "localhost");
        let pool = LocalModelPool::default();
        let lease = pool.reserve(&alias, provider).unwrap();
        let refused = lease.prepare(&AnalysisControl::default()).is_err();
        drop(lease);
        outcomes.push((provider, refused, server.seen().len()));
        // Positive control: choosing this exact IPv6 service retains native support.
        let explicit = server.connection();
        let lease = pool.reserve(&explicit, provider).unwrap();
        lease.prepare(&AnalysisControl::default()).unwrap();
        let client = TextClient::new(&explicit)
            .unwrap()
            .with_residency(lease)
            .unwrap();
        generate(&client).unwrap();
        drop(client);
    }
    assert!(
        outcomes
            .iter()
            .all(|&(_, refused, requests)| refused && requests == 0),
        "Ambiguous localhost acquired native authority on IPv6: {outcomes:?}"
    );
}

/// Every response closes its connection; after the configured IPv4 service exits,
/// leased inference and exact-instance cleanup must fail without contacting the IPv6 peer.
#[test]
fn review_native_localhost_inference_and_cleanup_keep_the_configured_origin() {
    let mut outcomes = Vec::new();
    for provider in [LocalProvider::Ollama, LocalProvider::LmStudio] {
        let (primary, alternate_listener) =
            native_loopback_pair(TcpListener::bind("127.0.0.1:0").unwrap());
        let alternate = Server::on(alternate_listener, move |p, b| match provider {
            LocalProvider::Ollama => ollama(p, b),
            LocalProvider::LmStudio => lm(p, b),
        });
        let primary = Server::on(primary, move |p, b| match provider {
            LocalProvider::Ollama => ollama(p, b),
            LocalProvider::LmStudio => lm(p, b),
        });
        let mut connection = primary.connection();
        connection.endpoint = connection.endpoint.replace("127.0.0.1", "localhost");
        let pool = LocalModelPool::default();
        let lease = pool.reserve(&connection, provider).unwrap();
        lease.prepare(&AnalysisControl::default()).unwrap();
        drop(primary); // Only this fixture listener stops, before reconnecting for text.
        let client = TextClient::new(&connection)
            .unwrap()
            .with_residency(Arc::clone(&lease))
            .unwrap();
        let refused = generate(&client).is_err();
        drop(client);
        drop(lease);
        let cleanup_blocked =
            provider != LocalProvider::LmStudio || pool.reserve(&connection, provider).is_err();
        outcomes.push((provider, refused, alternate.seen().len(), cleanup_blocked));
    }
    assert!(
        outcomes
            .iter()
            .all(|&(_, refused, requests, blocked)| refused && requests == 0 && blocked),
        "A lifecycle reconnect reached the different IPv6 service: {outcomes:?}"
    );
}

/// Drops the listening socket before writing the first inventory response so a
/// second native request necessarily opens a new connection after IPv4 is gone.
#[test]
fn review_native_localhost_load_does_not_switch_after_inventory() {
    let mut outcomes = Vec::new();
    for provider in [LocalProvider::Ollama, LocalProvider::LmStudio] {
        let (listener, alternate_listener) =
            native_loopback_pair(TcpListener::bind("127.0.0.1:0").unwrap());
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let alternate = Server::on(alternate_listener, move |p, b| match provider {
            LocalProvider::Ollama => ollama(p, b),
            LocalProvider::LmStudio => lm(p, b),
        });
        let inventory = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        drop(listener);
                        let (path, body) = read(&mut stream);
                        let (status, reply) = match provider {
                            LocalProvider::Ollama => ollama(&path, &body),
                            LocalProvider::LmStudio => lm(&path, &body),
                        };
                        let bytes = reply.to_string();
                        write!(stream,"HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).unwrap();
                        return Some(path);
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return None,
                    Err(e) => panic!("{e}"),
                }
            }
        });
        let connection = Connection {
            endpoint: format!("http://localhost:{port}/v1/"),
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let pool = LocalModelPool::default();
        let lease = pool.reserve(&connection, provider).unwrap();
        let refused = lease.prepare(&AnalysisControl::default()).is_err();
        drop(lease);
        let path = inventory.join().unwrap();
        let expected = if provider == LocalProvider::Ollama {
            "/api/ps"
        } else {
            "/api/v1/models"
        };
        outcomes.push((
            provider,
            refused,
            alternate.seen().len(),
            path.as_deref() == Some(expected),
        ));
    }
    assert!(
        outcomes
            .iter()
            .all(|&(_, refused, requests, correct_first)| refused
                && requests == 0
                && correct_first),
        "Native inventory/load crossed configured origin: {outcomes:?}"
    );
}

/// Unknown observed expiry cannot authorize an indefinite write to a shared model.
#[test]
fn review_native_ollama_unreadable_expiry_refuses_before_any_mutation() {
    let mut outcomes = Vec::new();
    for expiry in [
        None,
        Some(Value::Null),
        Some(json!(12)),
        Some(json!("")),
        Some(json!("not-a-timestamp")),
    ] {
        let server = Server::new(move |p, b| {
            if p == "/api/ps" {
                let mut model = json!({"name":"synthetic:latest"});
                if let Some(value) = &expiry {
                    model["expires_at"] = value.clone();
                }
                (200, json!({"models":[model]}))
            } else {
                ollama(p, b)
            }
        });
        let pool = LocalModelPool::default();
        let lease = pool
            .reserve(&server.connection(), LocalProvider::Ollama)
            .unwrap();
        let refused = lease.prepare(&AnalysisControl::default()).is_err();
        drop(lease);
        outcomes.push((
            refused,
            server.seen().iter().filter(|(p, _)| p != "/api/ps").count(),
        ));
    }
    assert!(
        outcomes
            .iter()
            .all(|&(refused, mutations)| refused && mutations == 0),
        "Unreadable expiry became native indefinite authority: {outcomes:?}"
    );
}

/// Uses one nonbinding synthetic source and the actual evidence-schema transport path.
fn generate(client: &TextClient) -> anyhow::Result<String> {
    client.generate(
        &[Source {
            id: "S00001".into(),
            source: "microphone".into(),
            start_ms: 0,
            end_ms: 30_000,
            text: "Vielleicht könnten wir einen Bericht schreiben.".into(),
        }],
        &Step {
            id: "summary".into(),
            name: "Summary".into(),
            kind: Kind::Summary,
            prompt: "Fasse belegte Aussagen zusammen.".into(),
        },
    )
}

/// Checks management precedes text and that client clones prevent premature final cleanup.
#[test]
fn ollama_pins_before_text_and_last_user_releases_after_all_client_clones() {
    let server = Server::new(ollama);
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let live = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    let post = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    assert!(
        server.seen().is_empty(),
        "Reservation must not send text or start network"
    );
    live.prepare(&AnalysisControl::default()).unwrap();
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(Arc::clone(&live))
        .unwrap();
    assert_eq!(generate(&client).unwrap(), "{\"items\":[]}");
    let seen = server.seen();
    assert_eq!(
        seen.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        vec!["/api/ps", "/api/generate", "/api/chat"]
    );
    assert_eq!(
        seen[1].1,
        json!({"model":"synthetic","stream":false,"keep_alive":300})
    );
    assert_eq!(seen[2].1["keep_alive"], 300);
    assert_eq!(seen[2].1["options"]["num_predict"], 2048);
    assert!(seen[2].1["format"]["properties"]["items"].is_object());
    assert_eq!(seen[2].1["messages"][0]["role"], "system");
    drop(live);
    drop(post);
    assert_eq!(
        server.seen().len(),
        3,
        "In-flight client still owns the model"
    );
    drop(client);
    let seen = server.seen();
    assert_eq!(
        seen.len(),
        3,
        "Final Ollama release must send no management request"
    );
}

/// Checks instance-specific inference and proves the unrelated loaded instance is never unloaded.
#[test]
fn lm_studio_uses_its_explicit_instance_and_never_unloads_the_foreign_one() {
    let server = Server::new(lm);
    let conn = server.connection();
    let pool = LocalModelPool::default();
    let lease = pool.reserve(&conn, LocalProvider::LmStudio).unwrap();
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(lease)
        .unwrap();
    generate(&client).unwrap();
    let seen = server.seen();
    assert_eq!(
        seen[1],
        ("/api/v1/models/load".into(), json!({"model":"synthetic"}))
    );
    assert_eq!(seen[2].1["model"], "owned");
    assert_eq!(seen[2].1["response_format"]["type"], "json_schema");
    drop(client);
    let seen = server.seen();
    assert_eq!(
        seen[3],
        (
            "/api/v1/models/unload".into(),
            json!({"instance_id":"owned"})
        )
    );
    assert!(!seen
        .iter()
        .any(|(_, b)| b.get("instance_id") == Some(&json!("foreign"))));
}

/// Refuses a reused foreign instance instead of claiming ownership or sending text to it.
#[test]
fn lm_studio_reused_foreign_id_is_explicit_failure_without_unload_or_inference() {
    let server = Server::new(|p, b| {
        if p == "/api/v1/models/load" {
            (
                200,
                json!({"type":"llm","status":"loaded","instance_id":"foreign"}),
            )
        } else {
            lm(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::LmStudio)
        .unwrap();
    assert!(lease
        .prepare(&AnalysisControl::default())
        .unwrap_err()
        .to_string()
        .contains("bereits vorhandene"));
    drop(lease);
    assert_eq!(server.seen().len(), 2);
}

/// Finite and indefinite initial policies remain intact without a final global TTL rewrite.
#[test]
fn borrowed_ollama_policies_remain_without_cached_ttl_restore() {
    for (expiry, seconds) in [("9999-01-01T00:00:00Z", -1), ("2020-01-01T00:00:00Z", 300)] {
        let server = Server::new(move |p, b| {
            if p == "/api/ps" {
                (
                    200,
                    json!({"models":[{"name":"synthetic:latest","expires_at":expiry}]}),
                )
            } else {
                ollama(p, b)
            }
        });
        let pool = LocalModelPool::default();
        let lease = pool
            .reserve(&server.connection(), LocalProvider::Ollama)
            .unwrap();
        lease.prepare(&AnalysisControl::default()).unwrap();
        let prepared = server.seen();
        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[1].1["keep_alive"], seconds);
        drop(lease);
        assert_eq!(
            server.seen(),
            prepared,
            "Final release rewrote shared provider policy"
        );
    }
}

/// Keeps failed work reserved while refusing incomplete answers and allows explicit retry.
#[test]
fn failure_keeps_warm_reservation_for_retry_and_rejects_truncated_native_answers() {
    let failed = Arc::new(AtomicBool::new(true));
    let gate = Arc::clone(&failed);
    let server = Server::new(move |p, b| {
        if p == "/api/chat" && gate.load(Ordering::Acquire) {
            (
                200,
                json!({"done":true,"done_reason":"length","message":{"content":"{\"items\":[]}"}}),
            )
        } else {
            ollama(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let anchor = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(Arc::clone(&anchor))
        .unwrap();
    assert!(generate(&client)
        .unwrap_err()
        .to_string()
        .contains("Antwortlimit"));
    drop(client);
    assert_eq!(server.seen().len(), 4);
    let limits: Vec<_> = server
        .seen()
        .iter()
        .filter(|(p, _)| p == "/api/chat")
        .map(|(_, body)| body["options"]["num_predict"].as_u64().unwrap())
        .collect();
    assert_eq!(
        limits,
        vec![2048, 4096],
        "Only one bounded output retry; the same warm reservation stays owned"
    );
    failed.store(false, Ordering::Release);
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(Arc::clone(&anchor))
        .unwrap();
    generate(&client).unwrap();
    drop(client);
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|(p, _)| p == "/api/generate")
            .count(),
        1,
        "Retry must not reload or unload"
    );
    drop(anchor);
    assert!(!server
        .seen()
        .iter()
        .any(|(path, body)| path == "/api/generate" && body["keep_alive"] == 0));
}

/// Proves cancelled or unconfirmed preparation cannot transmit inference text.
#[test]
fn cancellation_before_prepare_does_not_contact_provider_and_failed_load_sends_no_text() {
    let server = Server::new(|p, b| {
        if p == "/api/generate" {
            (500, json!({"error":"synthetic-private-server-message"}))
        } else {
            ollama(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    let control = AnalysisControl::default();
    control.cancel();
    assert!(meeting_notes::is_cancelled(
        &lease.prepare(&control).unwrap_err()
    ));
    assert!(server.seen().is_empty());
    let error = lease
        .prepare(&AnalysisControl::default())
        .unwrap_err()
        .to_string();
    assert!(!error.contains("synthetic-private"));
    drop(lease);
    assert_eq!(server.seen().len(), 2);
}

/// Final Ollama release sends no global cleanup, and a new reservation reads fresh provider state.
#[test]
fn review_ollama_logical_release_avoids_cleanup_and_next_reservation_reads_fresh_state() {
    let server = Server::new(|p, b| {
        if p == "/api/generate" && b["keep_alive"] == 0 {
            (500, json!({}))
        } else {
            ollama(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let lease = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    drop(lease);
    let retry = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    retry.prepare(&AnalysisControl::default()).unwrap();
    assert_eq!(server.seen()[2].0, "/api/ps");
    assert_eq!(server.seen().len(), 4);
    drop(retry);
}

/// Existing instances of the selected key must never trigger a second weight load.
#[test]
fn review_preexisting_lm_instance_blocks_before_load_and_preserves_foreign_state() {
    let server = Server::new(|path, body| {
        if path == "/api/v1/models" {
            (
                200,
                json!({"models":[{"type":"llm","key":"synthetic","loaded_instances":[{"id":"foreign"}]}]}),
            )
        } else {
            lm(path, body)
        }
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::LmStudio)
        .unwrap();
    assert!(
        lease.prepare(&AnalysisControl::default()).is_err(),
        "Existing model caused another native load"
    );
    drop(lease);
    assert_eq!(
        server.seen().len(),
        1,
        "A foreign loaded model must receive no load/unload/inference"
    );
}

/// An uncertain explicit load must not be submitted again or guessed as owned.
#[test]
fn review_uncertain_lm_load_blocks_repeat_submission_without_unloading_guessed_id() {
    let server = Server::new(|path, body| {
        if path == "/api/v1/models" {
            (
                200,
                json!({"models":[{"type":"llm","key":"synthetic","loaded_instances":[]}]}),
            )
        } else if path == "/api/v1/models/load" {
            (500, json!({}))
        } else {
            lm(path, body)
        }
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::LmStudio)
        .unwrap();
    assert!(lease.prepare(&AnalysisControl::default()).is_err());
    assert!(lease.prepare(&AnalysisControl::default()).is_err());
    drop(lease);
    assert_eq!(
        server.seen().len(),
        2,
        "An unacknowledged model load was retried automatically"
    );
}

/// Exercises route, credential and model boundaries before any provider request.
#[test]
fn pool_rejects_cloud_remote_route_auth_and_model_mismatches_before_network() {
    let server = Server::new(ollama);
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let lease = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    let mut changed = server.connection();
    changed.api_key = "synthetic-other-key".into();
    assert!(pool.reserve(&changed, LocalProvider::Ollama).is_err());
    changed = server.connection();
    changed.model = "other".into();
    assert!(TextClient::new(&changed)
        .unwrap()
        .with_residency(Arc::clone(&lease))
        .is_err());
    changed = server.connection();
    changed.mode = "cloud".into();
    changed.cloud_consent = true;
    changed.endpoint = "https://example.invalid/v1/".into();
    assert!(pool.reserve(&changed, LocalProvider::Ollama).is_err());
    changed.mode = "local".into();
    assert!(pool.reserve(&changed, LocalProvider::Ollama).is_err());
    changed = server.connection();
    changed.endpoint = changed.endpoint.replace("/v1/", "/unrelated/");
    assert!(pool.reserve(&changed, LocalProvider::Ollama).is_err());
    drop(lease);
    assert!(server.seen().is_empty());
}

/// A finite lease bridges a quiet live interval while a retained coordinator owns it.
#[test]
fn bounded_idle_grace_keeps_the_model_between_live_updates() {
    let remaining = Arc::new(std::sync::atomic::AtomicI64::new(0));
    let ttl = Arc::clone(&remaining);
    let server = Server::new(move |path, body| {
        if path == "/api/generate" || path == "/api/chat" {
            ttl.store(body["keep_alive"].as_i64().unwrap(), Ordering::Release);
        }
        ollama(path, body)
    });
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let anchor = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(Arc::clone(&anchor))
        .unwrap();
    generate(&client).unwrap();
    // This is a provider-side virtual quiet interval, not a four-minute device run.
    assert_eq!(remaining.fetch_sub(240, Ordering::AcqRel), 300);
    drop(client);
    assert!(
        remaining.load(Ordering::Acquire) > 0,
        "Idle coordinator awaits finalization"
    );
    let before_release = server.seen();
    drop(anchor);
    assert_eq!(
        server.seen(),
        before_release,
        "Logical release must not alter shared TTL"
    );
    assert_eq!(remaining.load(Ordering::Acquire), 60);
    // Only this fixture's clock advances the remaining server grace to its finite expiry.
    assert_eq!(remaining.fetch_sub(60, Ordering::AcqRel), 60);
    assert_eq!(remaining.load(Ordering::Acquire), 0);
}

/// Treats HTTP success without provider-specific acknowledgement as an ownership failure.
#[test]
fn successful_http_without_load_or_exact_unload_ack_is_not_a_confirmed_lifetime() {
    let server = Server::new(|p, b| {
        if p == "/api/generate" {
            (200, json!({"error":"synthetic"}))
        } else {
            ollama(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    assert!(lease.prepare(&AnalysisControl::default()).is_err());
    drop(lease);
    assert_eq!(server.seen().len(), 2);
    let server = Server::new(|p, b| {
        if p == "/api/v1/models/unload" {
            (200, json!({"instance_id":"foreign"}))
        } else {
            lm(p, b)
        }
    });
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let lease = pool.reserve(&conn, LocalProvider::LmStudio).unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    drop(lease);
    assert!(pool.reserve(&conn, LocalProvider::LmStudio).is_err());
}

/// Waits for a submitted native acknowledgement so cancellation cleans only the known owned instance.
#[test]
fn cancellation_during_native_load_still_tracks_the_created_instance_until_release() {
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let (loading, allow_reply) = (Arc::clone(&entered), Arc::clone(&release));
    let server = Server::new(move |p, b| {
        if p == "/api/v1/models/load" {
            loading.store(true, Ordering::Release);
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !allow_reply.load(Ordering::Acquire) {
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
            }
        }
        lm(p, b)
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::LmStudio)
        .unwrap();
    let control = AnalysisControl::default();
    let cancel = control.clone();
    let worker = thread::spawn(move || {
        let result = lease.prepare(&control);
        drop(lease);
        result
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    cancel.cancel();
    release.store(true, Ordering::Release);
    assert!(meeting_notes::is_cancelled(
        &worker.join().unwrap().unwrap_err()
    ));
    assert_eq!(
        server.seen().last().unwrap(),
        &(
            "/api/v1/models/unload".into(),
            json!({"instance_id":"owned"})
        ),
        "Cancelled load must not leave an untracked owned model"
    );
}

/// Retains failed finalization and verifies saved live-window bytes survive resume before cleanup.
#[test]
fn failed_live_overview_stays_warm_and_resume_preserves_the_window_before_final_release() {
    use meeting_notes::live::{read_latest, LiveRunner, LiveText, Trigger};
    let fail = Arc::new(AtomicBool::new(true));
    let failure = Arc::clone(&fail);
    let quote = "Wir besprechen heute die Dokumentation.";
    let server = Server::new(move |p, b| {
        if p == "/api/chat" {
            let answer = if b["format"]["properties"].get("points").is_some() {
                json!({"points":[{"text":"Dokumentation besprechen","owner":null,"due":null,"evidence":[{"sourceId":"C0","quote":quote}]}],"decisions":[],"tasks":[],"questions":[]})
            } else {
                if failure.load(Ordering::Acquire) {
                    return (500, json!({}));
                }
                json!({"items":[{"text":"Dokumentation besprechen","owner":null,"due":null,"evidence":[{"sourceId":"C0","quote":quote}]}]})
            };
            return (
                200,
                json!({"done":true,"done_reason":"stop","message":{"content":answer.to_string()}}),
            );
        }
        ollama(p, b)
    });
    let folder = tempfile::tempdir().unwrap();
    let conn = server.connection();
    let pool = LocalModelPool::default();
    let anchor = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    let mut runner = LiveRunner::start(
        folder.path(),
        "synthetic-live-residency",
        server.connection(),
        "de",
    )
    .unwrap()
    .with_residency(Arc::clone(&anchor))
    .unwrap();
    let text = [LiveText {
        key: "synthetic-part".into(),
        track: "system".into(),
        start_ms: 0,
        end_ms: 30_000,
        text: quote.into(),
    }];
    let outcome = runner
        .update(
            &runner.pending(&text),
            Trigger::Final,
            &AnalysisControl::default(),
        )
        .unwrap();
    assert!(outcome.overview_error.is_some());
    assert_eq!(outcome.accepted, 1);
    assert!(runner.overview_due());
    let manifest = folder
        .path()
        .join(&runner.run().id)
        .join("window-000001.json");
    // Use the stored run's actual window path, without assuming a private session layout.
    let window = std::fs::read_dir(manifest.parent().unwrap())
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("window-")
        })
        .unwrap();
    let before = std::fs::read(&window).unwrap();
    drop(runner);
    assert!(!server.seen().iter().any(|(_, b)| b["keep_alive"] == 0));
    fail.store(false, Ordering::Release);
    let mut resumed = LiveRunner::start(folder.path(), "synthetic-live-residency", conn, "de")
        .unwrap()
        .with_residency(Arc::clone(&anchor))
        .unwrap();
    assert!(resumed.pending(&text).is_empty());
    resumed
        .update(&[], Trigger::Final, &AnalysisControl::default())
        .unwrap();
    let stored = read_latest(folder.path(), "synthetic-live-residency")
        .unwrap()
        .unwrap();
    assert!(!stored.overview_pending);
    assert_eq!(stored.overview[0].evidence[0].quote, quote);
    assert_eq!(std::fs::read(window).unwrap(), before);
    drop(resumed);
    assert!(!server.seen().iter().any(|(_, b)| b["keep_alive"] == 0));
    drop(anchor);
    assert!(!server
        .seen()
        .iter()
        .any(|(path, body)| path == "/api/generate" && body["keep_alive"] == 0));
}

/// Prevents premature cleanup across aliases while preserving their distinct validated destinations.
#[test]
fn loopback_aliases_defer_cleanup_without_merging_routes_or_model_instances() {
    let server = Server::new(ollama);
    let pool = LocalModelPool::default();
    let first = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    let mut alias = server.connection();
    alias.endpoint = alias.endpoint.replace("127.0.0.1", "localhost");
    alias.model = "synthetic:latest".into();
    let second = pool.reserve(&alias, LocalProvider::Ollama).unwrap();
    first.prepare(&AnalysisControl::default()).unwrap();
    second.prepare(&AnalysisControl::default()).unwrap();
    assert!(
        !first
            .matches_connection(&alias, LocalProvider::Ollama)
            .unwrap(),
        "The selected actual target must remain distinct"
    );
    drop(first);
    assert!(
        !server.seen().iter().any(|(_, b)| b["keep_alive"] == 0),
        "Other loopback spelling is still in use"
    );
    drop(second);
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|(_, b)| b["keep_alive"] == 0)
            .count(),
        0
    );
}

/// Models native Ollama inventory changes caused by actual fixture load/unload requests.
fn stateful_ollama(initial_seconds: i64) -> (Server, Arc<std::sync::atomic::AtomicI64>) {
    let lifetime = Arc::new(std::sync::atomic::AtomicI64::new(initial_seconds));
    let observed = Arc::clone(&lifetime);
    let server = Server::new(move |path, body| {
        if path == "/api/ps" {
            let seconds = observed.load(Ordering::Acquire);
            let expires = chrono::Utc::now()
                + if seconds == -1 {
                    chrono::Duration::days(730)
                } else {
                    chrono::Duration::seconds(seconds)
                };
            let mut models =
                vec![json!({"name":"foreign-key","expires_at":"2999-01-01T00:00:00Z"})];
            if seconds != 0 {
                models.push(json!({"name":"synthetic:latest","expires_at":expires.to_rfc3339()}));
            }
            return (200, json!({"models":models}));
        }
        if path == "/api/generate" {
            assert!(matches!(
                body["model"].as_str(),
                Some("synthetic" | "synthetic:latest")
            ));
            observed.store(body["keep_alive"].as_i64().unwrap(), Ordering::Release);
        }
        ollama(path, body)
    });
    (server, lifetime)
}

/// Final logical release must not overwrite policy another local application changed
/// after our prepare, including finite/indefinite adoption or deliberate removal.
#[test]
fn ollama_final_release_preserves_later_external_model_policy() {
    let (server, lifetime) = stateful_ollama(0);
    let pool = LocalModelPool::default();
    let connection = server.connection();
    let mut outcomes = Vec::new();
    for initial in [0, 1200] {
        for external in [-1, 2400, 0] {
            lifetime.store(initial, Ordering::Release);
            let lease = pool.reserve(&connection, LocalProvider::Ollama).unwrap();
            lease.prepare(&AnalysisControl::default()).unwrap();
            lifetime.store(external, Ordering::Release); // Independent fixture-owned client.
            let before_release = server.seen().len();
            drop(lease);
            outcomes.push((
                initial,
                external,
                lifetime.load(Ordering::Acquire),
                server.seen().len() - before_release,
            ));
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|&(_, external, after, requests)| external == after && requests == 0),
        "Final release changed independently updated shared-server policy: {outcomes:?}"
    );
}

/// Retries real same-port collisions while keeping IPv6 a mandatory precondition:
/// unsupported loopback, permissions and exhausted retries still fail this check.
fn native_loopback_pair(first: TcpListener) -> (TcpListener, TcpListener) {
    let mut first = Some(first);
    for attempt in 0..16 {
        let ipv4 = first.take().unwrap_or_else(|| {
            TcpListener::bind("127.0.0.1:0")
                .expect("IPv4 loopback required by native-origin regression")
        });
        let port = ipv4.local_addr().unwrap().port();
        match TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port)) {
            Ok(ipv6) => return (ipv4, ipv6),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse && attempt < 15 => continue,
            Err(error) => panic!("IPv6 loopback required; paired fixture bind failed: {error}"),
        }
    }
    unreachable!("Every bounded bind attempt returns a real pair or fails")
}

/// An independently occupied IPv6 port must not make a fresh paired fixture fail.
#[test]
fn review_ipv4_ipv6_pair_recovers_from_a_real_occupied_port() {
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let old_port = first.local_addr().unwrap().port();
    let occupied = TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, old_port))
        .expect("IPv6 loopback required by the real occupied-port regression");
    let (ipv4, ipv6) = native_loopback_pair(first);
    let port = ipv4.local_addr().unwrap().port();
    assert_ne!(
        port, old_port,
        "The occupied IPv6 listener must not be reused"
    );
    assert_eq!(ipv6.local_addr().unwrap().port(), port);
    let four = std::net::TcpStream::connect(ipv4.local_addr().unwrap()).unwrap();
    let six = std::net::TcpStream::connect(ipv6.local_addr().unwrap()).unwrap();
    assert!(ipv4.accept().is_ok());
    assert!(ipv6.accept().is_ok());
    assert_eq!(occupied.local_addr().unwrap().port(), old_port);
    assert_eq!(
        TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, old_port))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AddrInUse
    );
    drop((four, six));
}

/// Both alias orders retain bounded own TTLs and preserve initial finite/indefinite foreign policy.
#[test]
fn stateful_ollama_aliases_stop_renewal_without_overwriting_shared_policies() {
    let (server, lifetime) = stateful_ollama(0);
    let pool = LocalModelPool::default();
    let first_connection = server.connection();
    let mut alias = server.connection();
    alias.endpoint = alias.endpoint.replace("127.0.0.1", "localhost");
    alias.model = "synthetic:latest".into();
    let mut outcomes = Vec::new();
    // No new registry entries after the first pair: reversing load ownership must
    // cover both cleanup orders, independent of the registry's random hash seed.
    for initial in [0, 1200, -1] {
        for reverse in [false, true] {
            lifetime.store(initial, Ordering::Release); // Only fixture-owned provider state.
            let first = pool
                .reserve(&first_connection, LocalProvider::Ollama)
                .unwrap();
            let second = pool.reserve(&alias, LocalProvider::Ollama).unwrap();
            let (early, later) = if reverse {
                (second, first)
            } else {
                (first, second)
            };
            early.prepare(&AnalysisControl::default()).unwrap();
            later.prepare(&AnalysisControl::default()).unwrap();
            drop(early);
            assert_ne!(
                lifetime.load(Ordering::Acquire),
                0,
                "Active alias was unloaded"
            );
            drop(later);
            outcomes.push((initial, reverse, lifetime.load(Ordering::Acquire)));
        }
    }
    assert!(
        outcomes.iter().all(|&(initial, _, after)| match initial {
            0 => (1..=300).contains(&after),
            1200 => after >= 1190, // At most ten seconds of fixture/clock rounding, not five minutes.
            -1 => after == -1,
            _ => unreachable!(),
        }),
        "Original Ollama policy changed across aliases: {outcomes:?}"
    );
    assert!(!server
        .seen()
        .iter()
        .any(|(_, body)| body["model"] == "foreign-key"));
}

/// Equal model names on distinct local servers do not authorize borrowing or foreign unloading.
#[test]
fn stateful_ollama_distinct_servers_keep_their_own_original_policies() {
    let (owned, owned_lifetime) = stateful_ollama(0);
    let (foreign, foreign_lifetime) = stateful_ollama(1200);
    let pool = LocalModelPool::default();
    for reverse in [false, true] {
        owned_lifetime.store(0, Ordering::Release);
        foreign_lifetime.store(1200, Ordering::Release);
        let own = pool
            .reserve(&owned.connection(), LocalProvider::Ollama)
            .unwrap();
        let other = pool
            .reserve(&foreign.connection(), LocalProvider::Ollama)
            .unwrap();
        if reverse {
            other.prepare(&AnalysisControl::default()).unwrap();
            own.prepare(&AnalysisControl::default()).unwrap();
        } else {
            own.prepare(&AnalysisControl::default()).unwrap();
            other.prepare(&AnalysisControl::default()).unwrap();
        }
        drop(own);
        assert_ne!(
            owned_lifetime.load(Ordering::Acquire),
            0,
            "Conservative family deferral lost"
        );
        drop(other);
        assert!((1..=300).contains(&owned_lifetime.load(Ordering::Acquire)));
        assert!(foreign_lifetime.load(Ordering::Acquire) >= 1190);
    }
    assert!(!foreign
        .seen()
        .iter()
        .any(|(_, body)| body["keep_alive"] == 0));
}

/// Abruptly exiting our synthetic child must leave a finite server TTL, never an orphan pin.
#[test]
fn terminated_owner_leaves_bounded_ollama_residency_and_restart_can_release() {
    const CHILD: &str = "HANDYMEETING_SYNTHETIC_RESIDENCY_CRASH_ENDPOINT";
    if let Ok(endpoint) = std::env::var(CHILD) {
        let connection = Connection {
            endpoint,
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        };
        let pool = LocalModelPool::default();
        let lease = pool.reserve(&connection, LocalProvider::Ollama).unwrap();
        let client = TextClient::new(&connection)
            .unwrap()
            .with_residency(lease)
            .unwrap();
        generate(&client).unwrap();
        // Deliberately bypass Drop only in this named test child, not the host application.
        std::process::exit(0);
    }
    let server = Server::new(ollama);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "terminated_owner_leaves_bounded_ollama_residency_and_restart_can_release",
            "--test-threads=1",
        ])
        .env(CHILD, &server.endpoint)
        .status()
        .unwrap();
    assert!(status.success());
    let seen = server.seen();
    let lifetimes: Vec<_> = seen
        .iter()
        .filter(|(path, _)| path == "/api/generate" || path == "/api/chat")
        .map(|(_, body)| body["keep_alive"].as_i64().unwrap())
        .collect();
    assert_eq!(lifetimes.len(), 2);
    assert!(
        lifetimes.iter().all(|seconds| (1..=300).contains(seconds)),
        "Terminated app-owned loads and inference must not leave indefinite residency: {lifetimes:?}"
    );
    // Provider-side expiry is simulated by the empty /api/ps response. A new app
    // owns the subsequent load and can release it, rather than borrowing a far-future pin.
    let restarted = LocalModelPool::default();
    let lease = restarted
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    drop(lease);
    assert!(!server
        .seen()
        .iter()
        .any(|(path, body)| path == "/api/generate" && body["keep_alive"] == 0));
}

/// A retained owner renews through an actual idle interval without generating text.
#[test]
fn retained_owner_renews_ollama_without_inference_then_last_owner_releases() {
    let (sent, renewal) = std::sync::mpsc::channel();
    let mut loads = 0;
    let server = Server::new(move |path, body| {
        if path == "/api/generate" && body["keep_alive"] != 0 {
            loads += 1;
            if loads > 1 {
                sent.send(body["keep_alive"].as_i64().unwrap()).unwrap();
            }
        }
        ollama(path, body)
    });
    let pool = LocalModelPool::default();
    let lease = pool
        .reserve(&server.connection(), LocalProvider::Ollama)
        .unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    // This waits for the production 60-second refresher; it is still a synthetic
    // HTTP fixture, not a real model/server, device or long-meeting proof.
    assert_eq!(renewal.recv_timeout(Duration::from_secs(75)).unwrap(), 300);
    assert!(!server.seen().iter().any(|(path, _)| path == "/api/chat"));
    drop(lease);
    let after_release = server.seen();
    assert!(renewal.recv_timeout(Duration::from_secs(65)).is_err());
    assert_eq!(
        server.seen(),
        after_release,
        "Renewal continued after final logical release"
    );
    assert!(!server
        .seen()
        .iter()
        .any(|(path, body)| path == "/api/generate" && body["keep_alive"] == 0));
}

/// Failed background renewal cannot be silently treated as a confirmed warm model.
#[test]
fn failed_background_renewal_blocks_text_until_explicit_retry_succeeds() {
    let fail = Arc::new(AtomicBool::new(true));
    let failing = Arc::clone(&fail);
    let (sent, renewal) = std::sync::mpsc::channel();
    let mut loads = 0;
    let server = Server::new(move |path, body| {
        if path == "/api/generate" && body["keep_alive"] != 0 {
            loads += 1;
            if loads > 1 && failing.load(Ordering::Acquire) {
                sent.send(()).unwrap();
                return (500, json!({}));
            }
        }
        ollama(path, body)
    });
    let pool = LocalModelPool::default();
    let conn = server.connection();
    let lease = pool.reserve(&conn, LocalProvider::Ollama).unwrap();
    lease.prepare(&AnalysisControl::default()).unwrap();
    renewal.recv_timeout(Duration::from_secs(75)).unwrap();
    let client = TextClient::new(&conn)
        .unwrap()
        .with_residency(Arc::clone(&lease))
        .unwrap();
    assert!(generate(&client).is_err());
    assert!(!server.seen().iter().any(|(path, _)| path == "/api/chat"));
    fail.store(false, Ordering::Release);
    assert!(generate(&client).is_ok());
    drop(client);
    drop(lease);
    assert!(!server
        .seen()
        .iter()
        .any(|(path, body)| path == "/api/generate" && body["keep_alive"] == 0));
}
