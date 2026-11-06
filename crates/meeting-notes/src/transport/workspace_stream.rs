//! Local progress stream without a total-duration cutoff; silence between response reads is bounded.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
static ROUTES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
/// Allow slow prefill and active generation; recover after five minutes without response bytes.
const RESPONSE_IDLE_LIMIT: Duration = Duration::from_secs(300);
/// Share a bounded per-server queue across loopback aliases; discard unused route entries.
pub(super) fn route_gate(base: &Url) -> Result<Arc<Mutex<()>>> {
    let host = base.host_str().unwrap_or_default();
    let host = if ["localhost", "127.0.0.1", "[::1]", "::1"].contains(&host) {
        "loopback"
    } else {
        host
    };
    let key = format!(
        "{}:{host}:{}",
        base.scheme(),
        base.port_or_known_default().unwrap_or(0)
    );
    let mut routes = ROUTES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("Modellwarteschlange ist gesperrt."))?;
    routes.retain(|_, route| Arc::strong_count(route) > 1);
    ensure!(
        routes.len() < 128 || routes.contains_key(&key),
        "Zu viele Modellrouten."
    );
    Ok(Arc::clone(routes.entry(key).or_default()))
}
/// Wait cancellably for the route; a panic by a previous holder does not disable future requests.
pub(super) fn enter<'a>(
    gate: &'a Mutex<()>,
    control: &AnalysisControl,
) -> Result<MutexGuard<'a, ()>> {
    loop {
        control.check()?;
        match gate.try_lock() {
            Ok(g) => return Ok(g),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
        }
    }
}
impl TextClient {
    /// Stream local structured text with proxy/redirect blocks and explicit terminal validation.
    pub(crate) fn workspace_json(
        &self,
        system: &str,
        user: &str,
        schema: Value,
        control: &AnalysisControl,
    ) -> Result<String> {
        self.workspace_json_with_idle(system, user, schema, control, RESPONSE_IDLE_LIMIT)
    }

    /// Apply the same stream contract with a per-read idle limit, without a total request deadline.
    fn workspace_json_with_idle(
        &self,
        system: &str,
        user: &str,
        schema: Value,
        control: &AnalysisControl,
        idle: Duration,
    ) -> Result<String> {
        ensure!(
            self.local,
            "Automatische Meetingaktualisierung ist ausschließlich lokal verfügbar."
        );
        let gate = route_gate(&self.base)?;
        let _guard = enter(&gate, control)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(idle)
            .dns_resolver(Arc::new(NoDns))
            .resolve_to_addrs("localhost", &LOCALHOST_ADDRESSES)
            .tcp_keepalive(Duration::from_secs(30))
            .build()?;
        control.begin_response();
        let messages = json!([{"role":"system","content":system},{"role":"user","content":user}]);
        let (url, body, native) = if let Some(lease) = &self.residency {
            lease.prepare_workspace(control)?;
            let (provider, model, keep_alive, _) = lease.inference()?;
            match provider {
                crate::residency::LocalProvider::Ollama => (
                    self.base.join("../api/chat")?,
                    json!({"model":model,"stream":true,"messages":messages,"format":schema,"keep_alive":keep_alive,"options":{"temperature":0,"num_predict":4096,"num_ctx":crate::workspace::WORKSPACE_CONTEXT_LENGTH}}),
                    true,
                ),
                crate::residency::LocalProvider::LmStudio => (
                    self.base.join("chat/completions")?,
                    json!({"model":model,"stream":true,"messages":messages,"temperature":0,"max_tokens":4096,"response_format":{"type":"json_schema","json_schema":{"name":"meeting_update","strict":true,"schema":schema}}}),
                    false,
                ),
            }
        } else {
            (
                self.target.join("chat/completions")?,
                json!({"model":self.model,"stream":true,"messages":messages,"temperature":0,"max_tokens":4096,"response_format":{"type":"json_schema","json_schema":{"name":"meeting_update","strict":true,"schema":schema}}}),
                false,
            )
        };
        let mut request = client.post(url).json(&body);
        if !self.key.is_empty() {
            request = request.bearer_auth(self.key.as_str());
        }
        self.runtime.block_on(async {
            let mut response=await_cancellable(request.send(),control).await?;
            if !response.status().is_success() {
                let status=response.status().as_u16();
                if [400,413].contains(&status) {
                    let mut bytes=Vec::new();
                    while let Some(chunk)=await_cancellable(response.chunk(),control).await? {if bytes.len()+chunk.len()>16384{break;}bytes.extend_from_slice(&chunk);}
                    if context_limit(&bytes){return Err(BudgetError::Context.into());}
                }
                bail!("KI-Dienst meldet HTTP {status}. Aufnahme und gespeicherte Ergebnisse bleiben erhalten.");
            }
            let mut decoder=StreamAnswer::new(native);let mut bytes=Vec::new();let mut total=0;
            while let Some(chunk)=await_cancellable(response.chunk(),control).await? {
                control.received_response_bytes(chunk.len());
                total+=chunk.len();ensure!(total<=2_000_000,"KI-Antwort überschreitet die erlaubte Größe.");bytes.extend_from_slice(&chunk);
                while let Some(pos)=bytes.iter().position(|b|*b==b'\n') {
                    let line=bytes.drain(..=pos).collect::<Vec<_>>();decoder.line(std::str::from_utf8(&line)?)?;
                }
                ensure!(bytes.len()<=262_144,"KI-Datenrahmen ist zu groß.");
            }
            if !bytes.is_empty(){decoder.line(std::str::from_utf8(&bytes)?)?;}
            control.check()?;decoder.finish()
        })
    }
}
struct StreamAnswer {
    native: bool,
    complete: bool,
    text: String,
}
impl StreamAnswer {
    /// Initialize a native-Ollama or OpenAI-compatible stream decoder with no accepted terminal result.
    fn new(native: bool) -> Self {
        Self {
            native,
            complete: false,
            text: String::new(),
        }
    }
    /// Decode one bounded event, rejecting tool calls, ambiguous choices and incomplete completions.
    fn line(&mut self, line: &str) -> Result<()> {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') || (!self.native && line == "data: [DONE]") {
            return Ok(());
        }
        let payload = if self.native {
            line
        } else if let Some(s) = line.strip_prefix("data:") {
            s.trim()
        } else {
            return Ok(());
        };
        let v: Value = serde_json::from_str(payload)
            .context("KI-Dienst liefert keinen gültigen Datenstrom.")?;
        ensure!(
            v.get("error").is_none(),
            "Lokaler Modelllauf fehlgeschlagen."
        );
        let (text, done, reason) = if self.native {
            ensure!(
                v["message"]["tool_calls"].is_null(),
                "Werkzeugaufrufe sind nicht zugelassen."
            );
            (
                v["message"]["content"].as_str().unwrap_or_default(),
                v["done"] == true,
                v["done_reason"].as_str(),
            )
        } else {
            let choices = v["choices"]
                .as_array()
                .context("KI-Datenstrom ohne Auswahl.")?;
            if choices.is_empty() {
                return Ok(());
            }
            ensure!(choices.len() == 1, "Mehrdeutige Modellauswahl.");
            let c = &choices[0];
            ensure!(
                c["delta"]["tool_calls"].is_null() && c["delta"]["function_call"].is_null(),
                "Werkzeugaufrufe sind nicht zugelassen."
            );
            (
                c["delta"]["content"].as_str().unwrap_or_default(),
                !c["finish_reason"].is_null(),
                c["finish_reason"].as_str(),
            )
        };
        ensure!(
            !self.complete || text.is_empty(),
            "Daten nach dem Modellabschluss."
        );
        self.text.push_str(text);
        ensure!(self.text.len() <= 128_000, "KI-Antwort ist zu groß.");
        if done {
            if reason == Some("length") {
                return Err(BudgetError::Output.into());
            }
            ensure!(
                reason == Some("stop"),
                "KI-Antwort wurde nicht vollständig abgeschlossen."
            );
            self.complete = true;
        }
        Ok(())
    }
    /// Return text only after a successful terminal stop; length stops remain retryable budget errors.
    fn finish(self) -> Result<String> {
        ensure!(self.complete && !self.text.trim().is_empty(),"KI-Antwort wurde nicht vollständig abgeschlossen. Der letzte gültige Stand bleibt erhalten.");
        Ok(self.text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_requires_terminal_stop() {
        let mut d = StreamAnswer::new(false);
        d.line("data: {\"choices\":[{\"delta\":{\"content\":\"{}\"},\"finish_reason\":null}]}")
            .unwrap();
        assert!(d.finish().is_err());
        let mut d = StreamAnswer::new(true);
        d.line("{\"message\":{\"content\":\"{}\"},\"done\":true,\"done_reason\":\"stop\"}")
            .unwrap();
        assert_eq!(d.finish().unwrap(), "{}");
    }
    #[test]
    fn length_and_tools_are_rejected() {
        for line in ["{\"message\":{\"content\":\"{}\"},\"done\":true,\"done_reason\":\"length\"}","{\"message\":{\"content\":\"{}\",\"tool_calls\":[]},\"done\":true,\"done_reason\":\"stop\"}"]{assert!(StreamAnswer::new(true).line(line).is_err());}
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    #[test]
    fn panic_in_a_route_guard_does_not_disable_later_requests() {
        let gate = Arc::new(Mutex::new(()));
        let poisoned = Arc::clone(&gate);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("Synthetic panic while holding a stateless route");
        })
        .join();
        let guard = enter(&gate, &AnalysisControl::default()).unwrap();
        drop(guard);
        assert!(enter(&gate, &AnalysisControl::default()).is_ok());
    }
}

#[cfg(test)]
mod idle_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
    };

    /// Accept only this fixture's bounded JSON request while allowing cleanup to stop its listener.
    fn request(listener: &TcpListener, stopped: &AtomicBool) -> Option<(TcpStream, Value)> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !stopped.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            let mut socket = match listener.accept() {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket.set_nodelay(true).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let (end, size) = loop {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                assert!(bytes.len() < 16_384);
                if let Some(end) = bytes
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|i| i + 4)
                {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let size: usize = head
                        .lines()
                        .filter_map(|l| l.split_once(':'))
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .unwrap()
                        .1
                        .trim()
                        .parse()
                        .unwrap();
                    assert!(size < 8192);
                    break (end, size);
                }
            };
            while bytes.len() < end + size {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            return Some((
                socket,
                serde_json::from_slice(&bytes[end..end + size]).unwrap(),
            ));
        }
        None
    }
    /// Use independent clients on the same local route, without real models, audio, keys or cloud.
    fn connection(port: u16) -> crate::Connection {
        crate::Connection {
            endpoint: format!("http://127.0.0.1:{port}/v1/"),
            model: "synthetic-idle".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        }
    }

    #[test]
    fn silent_headers_body_and_error_body_release_the_route_for_classic_analysis() {
        for phase in ["headers", "body", "error-body"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let port = listener.local_addr().unwrap().port();
            let stopped = Arc::new(AtomicBool::new(false));
            let stop = Arc::clone(&stopped);
            let (entered, started) = mpsc::channel();
            let server = thread::spawn(move || {
                let Some((mut held, body)) = request(&listener, &stop) else {
                    return;
                };
                assert_eq!(body["stream"], true);
                if phase == "body" {
                    held.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n: first bytes\n").unwrap();
                }
                if phase == "error-body" {
                    held.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"synthetic-private-error").unwrap();
                }
                entered.send(()).unwrap();
                if let Some((mut classic, body)) = request(&listener, &stop) {
                    assert_eq!(body["stream"], false);
                    let response =
                        json!({"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]})
                            .to_string();
                    write!(classic,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
                }
                drop(held);
            });
            let control = AnalysisControl::default();
            let owned = control.clone();
            let (sent, result) = mpsc::channel();
            let worker = thread::spawn(move || {
                let client = TextClient::new(&connection(port)).unwrap();
                let value = client.workspace_json_with_idle(
                    "synthetic",
                    "synthetic",
                    json!({}),
                    &owned,
                    Duration::from_millis(200),
                );
                let _ = sent.send(value);
            });
            let saw_first = started.recv_timeout(Duration::from_secs(3)).is_ok();
            let classic_control = AnalysisControl::default();
            let classic_owned = classic_control.clone();
            let (sent, classic_result) = mpsc::channel();
            let classic = thread::spawn(move || {
                let client = TextClient::new(&connection(port)).unwrap();
                let value = client.chat_json_inner(
                    "synthetic",
                    "synthetic",
                    "test",
                    json!({}),
                    32,
                    &classic_owned,
                );
                let _ = sent.send(value);
            });
            let stalled = result.recv_timeout(Duration::from_secs(3)).ok();
            let recovered = if stalled.is_some() {
                classic_result.recv_timeout(Duration::from_secs(3)).ok()
            } else {
                None
            };
            control.cancel();
            classic_control.cancel();
            stopped.store(true, Ordering::Release);
            worker.join().unwrap();
            classic.join().unwrap();
            server.join().unwrap();
            assert!(saw_first, "First {phase} request never reached the fixture");
            let error = stalled
                .expect("Silent stream retained its route until explicit cancellation")
                .unwrap_err();
            assert!(
                matches!(
                    error.downcast_ref::<TransportFailure>(),
                    Some(TransportFailure::Timeout)
                ),
                "{error}"
            );
            assert!(!error.to_string().contains("synthetic-private-error"));
            assert_eq!(
                recovered
                    .expect("Classic request did not recover the route")
                    .unwrap(),
                "{}"
            );
        }
    }

    #[test]
    fn active_response_reads_can_exceed_the_entire_idle_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stopped = AtomicBool::new(false);
        let client = TextClient::new(&connection(port)).unwrap();
        let control = AnalysisControl::default();
        let received = control.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sent_events = Arc::clone(&events);
        let (ready, started) = mpsc::channel();
        let server = thread::spawn(move || {
            ready.send(()).unwrap();
            let (mut socket, body) = request(&listener, &stopped).unwrap();
            assert_eq!(body["stream"], true);
            assert!(socket.nodelay().unwrap());
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            let began = std::time::Instant::now();
            for index in 0..8 {
                socket.write_all(b": progress\n").unwrap();
                let expected = (index + 1) * b": progress\n".len() as u64;
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                while received.response_bytes() < expected
                    && !received.is_cancelled()
                    && std::time::Instant::now() < deadline
                {
                    thread::sleep(Duration::from_millis(2));
                }
                sent_events.lock().unwrap().push((
                    index,
                    began.elapsed().as_millis(),
                    received.response_bytes(),
                ));
                if received.is_cancelled() {
                    return;
                }
                assert!(
                    received.response_bytes() >= expected,
                    "Fixture progress frame was not received"
                );
                thread::sleep(Duration::from_millis(60));
            }
            socket.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"{}\"},\"finish_reason\":\"stop\"}]}\n").unwrap();
        });
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        let before = std::time::Instant::now();
        let result = client.workspace_json_with_idle(
            "synthetic",
            "synthetic",
            json!({}),
            &control,
            Duration::from_millis(200),
        );
        if result.is_err() {
            control.cancel();
        }
        server.join().unwrap();
        assert_eq!(
            result.unwrap_or_else(|error| panic!(
                "{error}; synthetic received-frame timings: {:?}",
                events.lock().unwrap()
            )),
            "{}"
        );
        assert!(before.elapsed() > Duration::from_millis(400));
        assert!(control.response_bytes() > 0);
        assert_eq!(events.lock().unwrap().len(), 8);
    }
}
