//! Local progress stream without an elapsed-time cutoff; no deadlines change elsewhere.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
static ROUTES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
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
            .dns_resolver(Arc::new(NoDns))
            .resolve_to_addrs("localhost", &LOCALHOST_ADDRESSES)
            .tcp_keepalive(Duration::from_secs(30))
            .build()?;
        control.begin_response();
        let messages = json!([{"role":"system","content":system},{"role":"user","content":user}]);
        let (url, body, native) = if let Some(lease) = &self.residency {
            lease.prepare_workspace(control)?;
            let (provider, model, keep_alive) = lease.inference()?;
            match provider {
                crate::residency::LocalProvider::Ollama => (
                    self.base.join("../api/chat")?,
                    json!({"model":model,"stream":true,"messages":messages,"format":schema,"keep_alive":keep_alive,"options":{"temperature":0,"num_predict":4096}}),
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
