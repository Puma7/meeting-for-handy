//! Orchestration-owned anchors keep failed work warm. Provider transport never sees these IDs.
use crate::AppResult;
use meeting_notes::{
    residency::{LocalModelPool, LocalProvider, ModelLease},
    Connection,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
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
}
#[derive(Clone, Default)]
pub(crate) struct Models(Arc<Inner>);
#[derive(Clone)]
pub(crate) struct ModelWork {
    inner: Arc<Inner>,
    owner: Owner,
    expected: Weak<Work>,
}
impl Models {
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
}
