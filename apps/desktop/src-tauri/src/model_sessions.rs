//! Orchestration-owned anchors keep failed work warm. Provider transport never sees these IDs.
use crate::AppResult;
use meeting_notes::{
    residency::{LocalModelPool, LocalProvider, ModelLease},
    AnalysisControl, Connection,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, Weak,
    },
    time::Duration,
};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Owner {
    Live(String),
    Analysis(String),
}
struct Work {
    lease: Arc<ModelLease>,
    retained: Vec<Arc<ModelLease>>,
}
#[derive(Default)]
struct Inner {
    pool: LocalModelPool,
    work: Mutex<HashMap<Owner, Arc<Work>>>,
    temporary_question: AtomicBool,
}
#[derive(Clone, Default)]
pub(crate) struct Models(Arc<Inner>);

struct TemporaryQuestion {
    models: Models,
}

impl Drop for TemporaryQuestion {
    fn drop(&mut self) {
        self.models
            .0
            .temporary_question
            .store(false, Ordering::Release);
    }
}
#[derive(Clone)]
pub(crate) struct ModelWork {
    inner: Arc<Inner>,
    owner: Owner,
    expected: Weak<Work>,
}
impl Models {
    /// Own one optional question until its provider finishes. A cancelled caller
    /// releases recording admission while the worker reconciles an admitted load
    /// and releases its exact model lease. Later questions cannot queue behind it.
    pub(crate) fn run_temporary_question<T: Send + 'static>(
        &self,
        control: &AnalysisControl,
        run: impl FnOnce(&Models, &AnalysisControl) -> AppResult<T> + Send + 'static,
    ) -> AppResult<T> {
        const STOPPED: &str = "The optional model check was cancelled. / Die optionale Modellabfrage wurde abgebrochen.";
        const UNAVAILABLE: &str = "The optional model worker is unavailable. / Die optionale Modellabfrage ist nicht verfügbar.";
        control.check().map_err(|_| STOPPED)?;
        self.0
            .temporary_question
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A previous model question is still finishing. Try again shortly. / Eine frühere Modellabfrage wird noch abgeschlossen. Bitte gleich erneut versuchen.")?;
        let permit = TemporaryQuestion {
            models: self.clone(),
        };
        let worker_control = control.clone();
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("temporary-model-question".into())
            .spawn(move || {
                let result = worker_control
                    .check()
                    .map_err(|_| STOPPED.into())
                    .and_then(|()| run(&permit.models, &worker_control));
                drop(permit);
                let _ = send.send(result);
            })
            .map_err(|_| UNAVAILABLE)?;
        loop {
            control.check().map_err(|_| STOPPED)?;
            match receive.recv_timeout(Duration::from_millis(25)) {
                Ok(result) => {
                    control.check().map_err(|_| STOPPED)?;
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(UNAVAILABLE.into()),
            }
        }
    }

    /// Share provider ownership with other work and release it when a transient
    /// read-only query ends. No resumable owner entry is created.
    pub(crate) fn temporary_lease(
        &self,
        connection: &Connection,
        provider: LocalProvider,
    ) -> AppResult<Arc<ModelLease>> {
        self.0
            .pool
            .reserve(connection, provider)
            .map_err(|error| error.to_string())
    }
    /// Reuses failed work on the same route/model. Other meetings and independently
    /// running live/post work retain their anchors until their own completion.
    pub(crate) fn reserve(
        &self,
        owner: Owner,
        connection: &Connection,
        provider: Option<LocalProvider>,
    ) -> AppResult<Option<ModelWork>> {
        let Some(provider) = provider.filter(|_| connection.mode == "local") else {
            // Validate consent/route before replacing any prior local task's anchor.
            meeting_notes::TextClient::new(connection).map_err(|error| error.to_string())?;
            return Ok(None);
        };
        let previous = self
            .0
            .work
            .lock()
            .map_err(|_| "Modellsteuerung ist gesperrt.")?
            .get(&owner)
            .cloned();
        if let Some(work) = previous.filter(|work| {
            work.lease
                .matches_connection(connection, provider)
                .unwrap_or(false)
        }) {
            return Ok(Some(ModelWork {
                inner: Arc::clone(&self.0),
                owner,
                expected: Arc::downgrade(&work),
            }));
        }
        let lease = self
            .0
            .pool
            .reserve(connection, provider)
            .map_err(|e| e.to_string())?;
        let mut cache = self
            .0
            .work
            .lock()
            .map_err(|_| "Modellsteuerung ist gesperrt.")?;
        let previous: Vec<_> = cache.remove(&owner).into_iter().collect();
        // Preserve earlier incomplete tasks even if the new request later fails validation.
        // Flatten/deduplicate by model slot, avoiding an unbounded chain of failed attempts.
        let mut retained: Vec<Arc<ModelLease>> = Vec::new();
        for old in &previous {
            for candidate in std::iter::once(&old.lease).chain(old.retained.iter()) {
                if !lease.same_model(candidate)
                    && !retained.iter().any(|held| held.same_model(candidate))
                {
                    retained.push(Arc::clone(candidate));
                }
            }
        }
        let work = Arc::new(Work { lease, retained });
        cache.insert(owner.clone(), Arc::clone(&work));
        drop(cache);
        // Native cleanup may wait for the provider, never while holding the work-map lock.
        drop(previous);
        Ok(Some(ModelWork {
            inner: Arc::clone(&self.0),
            owner,
            expected: Arc::downgrade(&work),
        }))
    }
    /// Completion/cancellation applies only to this meeting and job kind.
    pub(crate) fn finish_owner(&self, owner: &Owner) {
        self.detach_owner(owner)();
    }
    /// Detaches under the map lock; invoke the returned cleanup after other app locks end.
    pub(crate) fn detach_owner(&self, owner: &Owner) -> impl FnOnce() + Send + 'static {
        let previous = self
            .0
            .work
            .lock()
            .ok()
            .and_then(|mut cache| cache.remove(owner));
        move || drop(previous)
    }
}
impl ModelWork {
    /// Rejects a stale work handle after its exact reservation generation was released.
    pub(crate) fn lease(&self) -> AppResult<Arc<ModelLease>> {
        self.expected
            .upgrade()
            .map(|work| Arc::clone(&work.lease))
            .ok_or_else(|| "Die Modellreservierung wurde beendet.".into())
    }
    /// Removes just this generation's anchor. In-flight clients retain their Arc lease.
    pub(crate) fn finish(&self) {
        let previous = self.inner.work.lock().ok().and_then(|mut cache| {
            if cache
                .get(&self.owner)
                .is_some_and(|current| self.expected.ptr_eq(&Arc::downgrade(current)))
            {
                cache.remove(&self.owner)
            } else {
                None
            }
        });
        drop(previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Instant,
    };
    /// Builds a synthetic non-listening loopback route; coordinator tests make no network requests.
    fn connection(model: &str) -> Connection {
        Connection {
            endpoint: "http://127.0.0.1:9/v1/".into(),
            model: model.into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        }
    }
    fn reserve(models: &Models, owner: Owner, model: &str) -> ModelWork {
        models
            .reserve(owner, &connection(model), Some(LocalProvider::Ollama))
            .unwrap()
            .unwrap()
    }
    /// Proves dropping a transient client does not discard the retry anchor.
    #[test]
    fn failed_work_is_retained_without_a_client_and_reused_by_the_same_meeting() {
        let models = Models::default();
        let owner = Owner::Live("synthetic-a".into());
        let work = reserve(&models, owner.clone(), "synthetic");
        let lease = work.lease().unwrap();
        let weak = Arc::downgrade(&lease);
        drop(lease);
        drop(work);
        assert!(
            weak.upgrade().is_some(),
            "Failed/partial return must not release the model"
        );
        let retry = reserve(&models, owner, "synthetic");
        assert!(weak.ptr_eq(&Arc::downgrade(&retry.lease().unwrap())));
        retry.finish();
        assert!(weak.upgrade().is_none());
    }
    /// Keeps unrelated meetings and independent live/post owners isolated during completion.
    #[test]
    fn another_meetings_completion_preserves_failed_work_and_live_post_owners_are_independent() {
        let models = Models::default();
        let a = Owner::Live("synthetic-a".into());
        let b = Owner::Live("synthetic-b".into());
        let old = reserve(&models, a.clone(), "synthetic");
        let new = reserve(&models, b.clone(), "synthetic");
        let post = reserve(&models, Owner::Analysis("synthetic-a".into()), "synthetic");
        models.finish_owner(&b);
        assert!(old.lease().is_ok());
        assert!(new.lease().is_err());
        assert!(post.lease().is_ok());
        models.finish_owner(&a);
        assert!(old.lease().is_err());
        assert!(post.lease().is_ok());
        post.finish();
        assert!(models.0.work.lock().unwrap().is_empty());
    }
    /// Prevents rejected replacement and stale completion from releasing preserved work.
    #[test]
    fn invalid_replacement_or_stale_worker_cannot_release_earlier_model() {
        let models = Models::default();
        let owner = Owner::Live("synthetic-a".into());
        let first = reserve(&models, owner.clone(), "first");
        let lease = first.lease().unwrap();
        let weak = Arc::downgrade(&lease);
        drop(lease);
        let mut invalid = connection("second");
        invalid.endpoint = "https://example.invalid/v1/".into();
        assert!(models
            .reserve(owner.clone(), &invalid, Some(LocalProvider::Ollama))
            .is_err());
        assert!(first.lease().is_ok());
        let replacement = reserve(&models, owner, "second");
        first.finish();
        assert!(
            weak.upgrade().is_some(),
            "A later snapshot mismatch must keep the old failed work warm"
        );
        assert!(replacement.lease().is_ok());
        replacement.finish();
        assert!(weak.upgrade().is_none());
    }
    /// Separates durable completion from the lifetime of in-flight client references.
    #[test]
    fn completion_keeps_inflight_clients_alive_after_removing_the_anchor() {
        let models = Models::default();
        let owner = Owner::Live("synthetic-a".into());
        let work = reserve(&models, owner.clone(), "first");
        let inflight = work.lease().unwrap();
        let weak = Arc::downgrade(&inflight);
        models.finish_owner(&owner);
        assert!(work.lease().is_err());
        assert!(weak.upgrade().is_some());
        drop(inflight);
        assert!(weak.upgrade().is_none());
    }

    fn native_request(listener: &TcpListener) -> (TcpStream, String, serde_json::Value) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "synthetic model request missing");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut start = String::new();
        reader.read_line(&mut start).unwrap();
        let route = start.split_whitespace().nth(1).unwrap().to_owned();
        let mut length = 0;
        loop {
            let mut header = String::new();
            assert!(reader.read_line(&mut header).unwrap() > 0);
            if header == "\r\n" {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap();
                }
            }
        }
        assert!(length < 8192);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        (
            stream,
            route,
            if body.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            },
        )
    }

    fn native_reply(mut stream: TcpStream, value: serde_json::Value) {
        let body = value.to_string();
        write!(
            stream,
            "HTTP/1.1 200 Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    }

    #[test]
    fn cancelled_optional_question_releases_recording_before_the_native_load_returns() {
        use crate::operations::{Gate, Kind};
        use meeting_notes::{questions, TextClient};
        use serde_json::json;

        for kind in [Kind::HistoryQuestion, Kind::SetupProbe] {
            let models = Models::default();
            let gate = Gate::default();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut connection = connection("synthetic");
            connection.endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
            let (loading, entered) = mpsc::channel();
            let (release, allow_reply) = mpsc::channel();
            let provider = thread::spawn(move || {
                let (stream, route, _) = native_request(&listener);
                assert_eq!(route, "/api/v1/models");
                native_reply(
                    stream,
                    json!({"models":[
                        {"type":"llm", "key":"synthetic", "loaded_instances":[]},
                        {"type":"llm", "key":"unrelated", "loaded_instances":[{"id":"foreign"}]}
                    ]}),
                );
                let (stream, route, body) = native_request(&listener);
                assert_eq!(route, "/api/v1/models/load");
                assert_eq!(body, json!({"model":"synthetic"}));
                loading.send(()).unwrap();
                allow_reply.recv_timeout(Duration::from_secs(5)).unwrap();
                native_reply(
                    stream,
                    json!({"instance_id":"owned", "status":"loaded", "type":"llm"}),
                );
                let (stream, route, body) = native_request(&listener);
                assert_eq!(route, "/api/v1/models/unload");
                assert_eq!(body, json!({"instance_id":"owned"}));
                native_reply(stream, body);
            });

            let operation = gate.try_acquire(kind).unwrap();
            let foreground = models.clone();
            let (completed, result) = mpsc::channel();
            let caller = thread::spawn(move || {
                let control = operation
                    .history_control()
                    .or_else(|| operation.setup_control())
                    .unwrap();
                let answer = foreground.run_temporary_question(control, move |models, control| {
                    let lease = models.temporary_lease(&connection, LocalProvider::LmStudio)?;
                    let client = TextClient::new(&connection)
                        .and_then(|client| client.with_residency(lease))
                        .map_err(|error| error.to_string())?;
                    questions::answer_with_control(
                        &client,
                        "Which day was confirmed?",
                        &[questions::Passage {
                            id: 1,
                            meeting: "Synthetic".into(),
                            text: "Wednesday was confirmed.".into(),
                        }],
                        control,
                    )
                    .map_err(|error| error.to_string())
                });
                drop(operation);
                completed.send(answer).unwrap();
            });
            entered.recv_timeout(Duration::from_secs(5)).unwrap();
            let recording = if kind == Kind::SetupProbe {
                gate.acquire_recording().unwrap()
            } else {
                assert!(gate.try_acquire(Kind::Recording).is_err());
                assert!(result
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap_err()
                    .contains("cancelled"));
                gate.try_acquire(Kind::Recording).unwrap()
            };
            if kind == Kind::SetupProbe {
                assert!(result
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap_err()
                    .contains("cancelled"));
            }
            let next = models.run_temporary_question(
                &AnalysisControl::default(),
                |_, _| -> AppResult<()> {
                    panic!("an optional request must not queue behind the admitted load")
                },
            );
            assert!(next.unwrap_err().contains("still finishing"));
            drop(recording);
            release.send(()).unwrap();
            caller.join().unwrap();
            provider.join().unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while models.0.temporary_question.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(5));
            }
            assert!(models
                .run_temporary_question(&AnalysisControl::default(), |_, _| Ok(()))
                .is_ok());
        }
    }
}
