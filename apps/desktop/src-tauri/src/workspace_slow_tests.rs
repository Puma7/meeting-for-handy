//! Explicit real-time synthetic test of the actual worker and durable audio writer.
use super::*;
use meeting_core::SessionStore;
use std::io::{Read, Write};
use std::sync::atomic::AtomicUsize;

fn body(stream: &mut std::net::TcpStream) -> serde_json::Value {
    // Accepted sockets may inherit a nonblocking listener on BSD/Windows.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let header;
    loop {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0 && bytes.len() < 1_000_000);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            header = p + 4;
            break;
        }
    }
    let text = String::from_utf8_lossy(&bytes[..header]);
    let size = text
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|s| s.trim().parse::<usize>().ok())
        })
        .unwrap();
    while bytes.len() < header + size {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    serde_json::from_slice(&bytes[header..header + size]).unwrap()
}
fn reply(stream: &mut std::net::TcpStream, request: serde_json::Value, slow: bool) {
    let input: serde_json::Value =
        serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
    let source = input["transcriptSources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| {
            input["window"]["requiredSourceIds"]
                .as_array()
                .unwrap()
                .contains(&s["id"])
        })
        .unwrap();
    let reconcile = input["purpose"] == "reconcile";
    let tasks = input["purpose"] == "tasks";
    let quote = if tasks {
        "Robin sagt: Ich schreibe den Bericht bis Freitag."
    } else {
        source["text"].as_str().unwrap()
    };
    let prior_task = input["state"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "task");
    let existing = input["state"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "impulse");
    let changes = if reconcile && existing {
        serde_json::json!([])
    } else {
        serde_json::json!([{
            "proposalId":"p_synthetic","kind":if reconcile{"impulse"}else if tasks{"task"}else{"fact"},"targetId":if tasks{prior_task.map(|e|e["id"].clone()).unwrap_or(serde_json::Value::Null)}else{serde_json::Value::Null},"targetRevision":if tasks{prior_task.map(|e|e["revision"].clone()).unwrap_or(serde_json::Value::Null)}else{serde_json::Value::Null},
            "text":if tasks && prior_task.is_some(){serde_json::Value::Null}else{serde_json::Value::String(if reconcile{"Welche Freigabe fehlt noch?"}else if tasks{"Bericht schreiben"}else{source["text"].as_str().unwrap()}.into())},"status":if reconcile || tasks{"open"}else{"recorded"},"attention":"current","owner":null,"due":null,
            "evidence":[{"sourceId":source["id"],"quote":quote}],"relatedEntityIds":[]
        }])
    };
    let answer=serde_json::json!({"contractVersion":meeting_notes::workspace::CONTRACT_VERSION,"requestId":input["requestId"],"changes":changes,"overviewRefs":if reconcile{serde_json::json!([])}else{serde_json::json!(["p_synthetic"])},"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
    let data = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
        serde_json::json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
    );
    // Simulate a compute-bound local model: no headers or response bytes for 210s.
    // This must not trigger an interval, response-idle or total-generation timeout.
    if slow {
        for tick in 0..14 {
            thread::sleep(Duration::from_secs(15));
            println!(
                "synthetic silent model: {} seconds, not cancelled",
                (tick + 1) * 15
            );
        }
    }
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len()).unwrap();
    stream.flush().unwrap();
    stream.write_all(data.as_bytes()).unwrap();
    stream.flush().unwrap();
}

#[test]
#[ignore = "210-second in-flight refresh plus real-time native PCM; run explicitly"]
fn slow_refresh_catches_up_without_overlap_and_audio_keeps_growing() {
    let setup_started = Instant::now();
    println!("slow test: starting setup");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let times = Arc::new(Mutex::new(Vec::new()));
    let (r, a, p, c, t) = (
        requests.clone(),
        active.clone(),
        peak.clone(),
        completed.clone(),
        times.clone(),
    );
    let server = thread::spawn(move || {
        let mut handlers = vec![];
        let deadline = Instant::now() + Duration::from_secs(320);
        while r.load(Ordering::Acquire) < 6 && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let index = r.fetch_add(1, Ordering::AcqRel);
                    t.lock().unwrap().push(Instant::now());
                    let (a, p, c) = (a.clone(), p.clone(), c.clone());
                    handlers.push(thread::spawn(move || {
                        let count = a.fetch_add(1, Ordering::AcqRel) + 1;
                        p.fetch_max(count, Ordering::AcqRel);
                        let request = body(&mut stream);
                        reply(&mut stream, request, index == 0);
                        a.fetch_sub(1, Ordering::AcqRel);
                        c.fetch_add(1, Ordering::AcqRel);
                    }));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("{e}"),
            }
        }
        for h in handlers {
            h.join().unwrap();
        }
    });
    let root = std::env::temp_dir().join(format!(
        "handy-workspace-slow-{}",
        meeting_notes::new_job_id()
    ));
    let service = Arc::new(Service::new(root.clone()).unwrap());
    println!(
        "slow test: service ready after {:.3}s",
        setup_started.elapsed().as_secs_f64()
    );
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic slow workspace",
        "synthetic-no-asr",
        "generated-system",
        Some("generated-microphone"),
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    service.lock().unwrap().view = Some(store.clone());
    let producing = Arc::new(AtomicBool::new(true));
    let flag = producing.clone();
    let writing = store.clone();
    let producer = thread::spawn(move || {
        let mut writers = [
            meeting_core::PcmWriter::new(writing.clone(), "system").unwrap(),
            meeting_core::PcmWriter::new(writing.clone(), "microphone").unwrap(),
        ];
        let samples = (0..320)
            .map(|i| ((i as f64 * std::f64::consts::TAU * 440.0 / 16000.0).sin() * 4096.0) as i16)
            .collect::<Vec<_>>();
        let mut frames = 0_u64;
        while flag.load(Ordering::Acquire) {
            for w in &mut writers {
                w.append(&samples).unwrap();
            }
            frames += 1;
            if frames.is_multiple_of(50) {
                for part in writing
                    .snapshot()
                    .unwrap()
                    .parts
                    .iter()
                    .filter(|p| p.audio_closed && p.status != "complete")
                {
                    let text="Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget fehlt weiterhin eine verbindliche Zusage. Die Freigabe muss noch geklärt werden. Wir besprechen heute die nächsten Schritte für den gemeinsamen Pilotversuch.";
                    writing
                        .append_transcript(meeting_core::TranscriptCheckpoint {
                            sequence: 0,
                            at: String::new(),
                            file: part.file.clone(),
                            model_id: "synthetic-no-asr".into(),
                            full: text.into(),
                            committed: text.into(),
                            tentative: String::new(),
                            processed_samples: part.confirmed_sample_count,
                            final_text: true,
                            revision_after_restart: false,
                        })
                        .unwrap();
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        for mut w in writers {
            w.finish().unwrap();
        }
        frames * 320
    });
    let connection = meeting_notes::Connection {
        endpoint,
        model: "synthetic-slow".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    println!(
        "slow test: audio producer started after {:.3}s",
        setup_started.elapsed().as_secs_f64()
    );
    let runner = LiveRunner::start(&live_root(&service, &id), &id, connection, "de")
        .unwrap()
        .with_workspace(Default::default())
        .unwrap();
    let workspace_folder = runner.workspace_folder().unwrap();
    let shared = Arc::new(Mutex::new(Shared {
        status: "starting".into(),
        update_started: None,
        waiting_for_transcript: false,
        error: None,
        pending_chars: 0,
        last_update: Instant::now() - Duration::from_secs(120),
        last_trigger: None,
        retry_at: None,
        view: None,
    }));
    let control = AnalysisControl::default();
    let worker = Worker {
        model_work: None,
        model_lease: None,
        _lease: service
            .reserve_operation(crate::operations::Kind::LiveNotes)
            .unwrap(),
        _source_lease: meeting_core::archive::source_publication_lease(&root).unwrap(),
        service: service.clone(),
        meeting_id: id,
        control: control.clone(),
        manual: Arc::new(AtomicBool::new(false)),
        interval_ms: Arc::new(AtomicU64::new(120000)),
        shared: shared.clone(),
    };
    println!(
        "slow test: worker ready after {:.3}s",
        setup_started.elapsed().as_secs_f64()
    );
    let handle = thread::spawn(move || worker.run(runner));
    let deadline = Instant::now() + Duration::from_secs(320);
    while Instant::now() < deadline {
        let saved = shared
            .lock()
            .unwrap()
            .view
            .as_ref()
            .and_then(|v| v.workspace.as_ref())
            // The worker first persists its empty human-context revision. Only the
            // next two revisions contain model windows; wait for both publications.
            .is_some_and(|w| w.revision >= 3);
        if completed.load(Ordering::Acquire) >= 6 && saved {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    control.cancel();
    handle.join().unwrap();
    let before_stop = store
        .snapshot()
        .unwrap()
        .parts
        .iter()
        .map(|p| p.confirmed_sample_count)
        .sum::<u64>();
    thread::sleep(Duration::from_secs(2));
    producing.store(false, Ordering::Release);
    let samples = producer.join().unwrap();
    server.join().unwrap();
    assert_eq!(
        completed.load(Ordering::Acquire),
        6,
        "Native worker failed to catch up: {:?}",
        shared.lock().unwrap().error
    );
    assert_eq!(
        peak.load(Ordering::Acquire),
        1,
        "Overlapping local model requests"
    );
    let at = times.lock().unwrap();
    let delay = at[3].duration_since(at[0]);
    assert!(
        delay >= Duration::from_secs(210) && delay < Duration::from_secs(220),
        "The next refresh waited a new interval after slow completion: {delay:?}"
    );
    let view = shared.lock().unwrap().view.clone().unwrap();
    let workspace = view.workspace.unwrap();
    assert_eq!(
        workspace.revision, 3,
        "Initial context plus both model windows must actually be durably committed"
    );
    let revisions = std::fs::read_dir(&workspace_folder)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("revision-")
                .then(|| {
                    serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap())
                        .unwrap()
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(revisions.len(), 3);
    assert_eq!(
        revisions
            .iter()
            .filter(|v| !v["processed"].as_array().unwrap().is_empty())
            .count(),
        2,
        "Exactly two source windows must have been durably published"
    );
    assert_eq!(
        workspace
            .entities
            .iter()
            .filter(|e| e.kind == "impulse")
            .count(),
        1
    );
    let data = store.snapshot().unwrap();
    let counts = ["system", "microphone"].map(|source| {
        data.parts
            .iter()
            .filter(|p| p.source == source)
            .map(|p| p.confirmed_sample_count)
            .sum::<u64>()
    });
    assert_eq!(counts, [samples; 2]);
    assert!(
        counts.iter().sum::<u64>() > before_stop,
        "Stopping notes affected the independent PCM writers"
    );
    let result = serde_json::json!({"synthetic":true,"actualNativeWorker":true,"model":"controlled SSE fixture","realRequestDelaySeconds":210,"refreshStartDistanceSeconds":delay.as_secs_f64(),"peakConcurrentModelRequests":1,"samplesPerTrack":counts,"retainedImpulses":1,"audioContinuedAfterNotesStop":true,"physicalDeviceOrAsr":false,"evidenceRoot":root});
    std::fs::write(
        root.join("slow-workspace-result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{result}");
}

#[test]
fn request_fixture_reads_delayed_bytes_from_an_inherited_nonblocking_stream() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    // Emulate platforms which inherit the listener's mode on accepted sockets.
    stream.set_nonblocking(true).unwrap();
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        client
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 13\r\n\r\n{\"slow\":true}")
            .unwrap();
    });
    assert_eq!(body(&mut stream), serde_json::json!({"slow":true}));
    writer.join().unwrap();
}
