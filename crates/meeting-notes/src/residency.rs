//! Local model lifetime, independent of sessions, transcripts and audio access.
use crate::{AnalysisControl, Connection, TextClient};
use anyhow::{ensure, Context, Result};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalProvider {
    Ollama,
    LmStudio,
}

#[derive(Default)]
pub struct LocalModelPool {
    slots: Arc<Mutex<HashMap<String, Arc<Slot>>>>,
}
struct Slot {
    state: Mutex<State>,
    /// Serializes reservations and final cleanup only for this provider/model family.
    transition: Arc<Mutex<()>>,
    identity: String,
    auth_digest: [u8; 32],
    provider: LocalProvider,
    model: String,
    users: AtomicUsize,
    cleanup_failed: AtomicBool,
    refresher: OnceLock<Result<std::thread::Thread, ()>>,
}
struct State {
    client: TextClient,
    provider: LocalProvider,
    loaded: Option<Loaded>,
    refresh_failed: bool,
    /// A submitted LM Studio load without a validated ID cannot be retried or guessed owned.
    lm_load_uncertain: bool,
}
enum Loaded {
    /// Initial observed policy controls active retention, never final eviction authority.
    Ollama {
        release_after: i64,
    },
    LmStudio {
        instance: String,
    },
}

/// One logical user's reservation. Arc clones retain an in-flight user's ownership.
pub struct ModelLease {
    slot: Arc<Slot>,
    slots: Arc<Mutex<HashMap<String, Arc<Slot>>>>,
}

// A stopped app cannot renew this finite server-side lease. Live/failed owners
// renew every minute, including idle intervals without inference.
const OLLAMA_KEEP_ALIVE_SECONDS: i64 = 300;
const OLLAMA_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

impl LocalModelPool {
    /// Reserves only the caller's chosen local route/model, without making network requests.
    pub fn reserve(
        &self,
        connection: &Connection,
        provider: LocalProvider,
    ) -> Result<Arc<ModelLease>> {
        let client = TextClient::new(connection)?;
        valid_model_name(&connection.model)?;
        let identity = client.local_identity()?;
        let auth_digest = client.local_auth_digest();
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("Modellreservierungen sind gesperrt."))?;
        ensure!(
            slots.len() < 128 || slots.contains_key(&identity),
            "Zu viele lokale Modellverbindungen; Anwendung vor weiteren Verbindungen neu starten."
        );
        // All routes conservatively grouped for cleanup must share the transition,
        // including a route first inserted while another alias is being released.
        let transition = slots
            .values()
            .find(|slot| {
                slot.provider == provider
                    && cleanup_group_model(provider, &slot.model)
                        == cleanup_group_model(provider, &connection.model)
            })
            .map(|slot| Arc::clone(&slot.transition))
            .unwrap_or_else(|| Arc::new(Mutex::new(())));
        let slot = Arc::clone(slots.entry(identity.clone()).or_insert_with(|| {
            Arc::new(Slot {
                transition,
                identity,
                auth_digest,
                provider,
                model: connection.model.clone(),
                users: AtomicUsize::new(0),
                cleanup_failed: AtomicBool::new(false),
                refresher: OnceLock::new(),
                state: Mutex::new(State {
                    client,
                    provider,
                    loaded: None,
                    refresh_failed: false,
                    lm_load_uncertain: false,
                }),
            })
        }));
        ensure!(
            slot.provider == provider && slot.auth_digest == auth_digest,
            "Die reservierte Modellverbindung wurde geändert. Anwendung vor der neuen Verbindung neu starten."
        );
        drop(slots);
        // A same-model cleanup must finish before this reservation becomes usable.
        // Waiting here never retains the registry required by unrelated models.
        let transition = slot
            .transition
            .lock()
            .map_err(|_| anyhow::anyhow!("Modellreservierungen sind gesperrt."))?;
        if slot.provider == LocalProvider::Ollama && slot.users.load(Ordering::Acquire) == 0 {
            // Finite provider TTLs and a fresh /api/ps query permit a new reservation;
            // no loaded ownership state or inference confirmation is reused.
            slot.cleanup_failed.store(false, Ordering::Release);
        }
        ensure!(!slot.cleanup_failed.load(Ordering::Acquire), "Die vorherige Modellfreigabe konnte nicht bestätigt werden. Lokalen Modelldienst prüfen.");
        slot.users
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_add(1)
            })
            .map_err(|_| anyhow::anyhow!("Zu viele Modellreservierungen."))?;
        // Cleanup observes this user under the same family transition.
        drop(transition);
        Ok(Arc::new(ModelLease {
            slot,
            slots: Arc::clone(&self.slots),
        }))
    }
}

impl ModelLease {
    /// Requires identical validated route/model and credential digest before binding a text client.
    pub(crate) fn matches_client(&self, client: &TextClient) -> bool {
        client
            .local_identity()
            .is_ok_and(|identity| identity == self.slot.identity)
            && client.local_auth_digest() == self.slot.auth_digest
    }
    /// Checks reuse without waiting for provider loading or exposing the credential.
    pub fn matches_connection(
        &self,
        connection: &Connection,
        provider: LocalProvider,
    ) -> Result<bool> {
        Ok(self.slot.provider == provider && self.matches_client(&TextClient::new(connection)?))
    }
    /// Preloads once. The caller runs this outside recording locks and retains the lease
    /// through final persistence; failed/partial jobs may retain it for an explicit retry.
    pub fn prepare(&self, control: &AnalysisControl) -> Result<()> {
        control.check()?;
        let mut state = self
            .slot
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Lokales Modell ist gesperrt."))?;
        control.check()?;
        ensure!(!state.lm_load_uncertain, "Die vorherige Modellladung wurde nicht eindeutig bestätigt. Keine weitere Instanz wird geladen. In LM Studio den Ladezustand prüfen; erst nach abgeschlossenem oder abgebrochenem Laden und manueller Bereinigung eigener Instanzen die Anwendung neu starten. Fremde Modelle nicht entladen.");
        if state.loaded.is_some() {
            if state.refresh_failed {
                renew_ollama(&mut state, Duration::from_secs(300))?;
            }
            if ollama_keep_alive(&state).is_some_and(|seconds| seconds > 0) {
                self.ensure_refresher()?;
            }
            return Ok(());
        }
        let client = &state.client;
        let loaded = match state.provider {
            LocalProvider::Ollama => {
                let before =
                    client.model_request("api/ps", None, control, Duration::from_secs(10))?;
                let models = before["models"]
                    .as_array()
                    .context("Ungültige geladene Modellliste.")?;
                ensure!(models.len() <= 10_000, "Geladene Modellliste ist zu groß.");
                let found = models
                    .iter()
                    .find(|m| model_name_matches(m["name"].as_str(), &client.model));
                let release_after = match found {
                    None => 0,
                    Some(model) => {
                        let expiry = model["expires_at"]
                            .as_str()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .context("Die Ablaufzeit des bereits geladenen Ollama-Modells ist nicht lesbar. Der gemeinsame Modellzustand wird nicht verändert; lokalen Dienst und API-Unterstützung prüfen.")?;
                        if expiry.signed_duration_since(chrono::Utc::now()).num_days() <= 365 {
                            expiry
                                .signed_duration_since(chrono::Utc::now())
                                .num_seconds()
                                .max(300)
                        } else {
                            -1
                        }
                    }
                };
                let response = client.model_request(
                    "api/generate",
                    Some(&json!({"model":client.model,"stream":false,"keep_alive":keep_alive_for(release_after)})),
                    // A native load creates server state. Once submitted, read its bounded
                    // acknowledgement even after cancellation so the owned instance remains
                    // known and can be released. Inference itself remains cancellable.
                    &AnalysisControl::default(),
                    Duration::from_secs(300),
                )?;
                ensure!(
                    response["done"] == true,
                    "Textmodell wurde nicht als geladen bestätigt."
                );
                Loaded::Ollama { release_after }
            }
            LocalProvider::LmStudio => {
                let before = client.model_request(
                    "api/v1/models",
                    None,
                    control,
                    Duration::from_secs(10),
                )?;
                let models = before["models"]
                    .as_array()
                    .context("Ungültige lokale Modellliste.")?;
                ensure!(models.len() <= 10_000, "Lokale Modellliste ist zu groß.");
                let model = models
                    .iter()
                    .find(|m| {
                        m["type"] == "llm"
                            && (m["key"].as_str() == Some(client.model.as_str())
                                || m["loaded_instances"].as_array().is_some_and(|instances| {
                                    instances
                                        .iter()
                                        .any(|i| i["id"].as_str() == Some(client.model.as_str()))
                                }))
                    })
                    .context("Das gewählte Textmodell ist im lokalen Dienst nicht vorhanden.")?;
                let key = model["key"]
                    .as_str()
                    .context("Lokale Modellkennung fehlt.")?;
                valid_model_name(key)?;
                let selected_instances = model["loaded_instances"].as_array().context(
                    "Lokale Instanzliste fehlt; keine zusätzliche Modellkopie wird geladen.",
                )?;
                ensure!(selected_instances.is_empty(), "Dieses Modell ist in LM Studio bereits geladen. Es wird weder eine weitere Kopie geladen noch die fremde Instanz übernommen oder entladen. Für eine eigene Reservierung zuerst den Ladezustand in LM Studio prüfen.");
                let existing: Vec<&str> = models
                    .iter()
                    .filter_map(|m| m["loaded_instances"].as_array())
                    .flatten()
                    .filter_map(|i| i["id"].as_str())
                    .collect();
                // Once submitted, an unknown outcome is sticky: automatic retries
                // may not create another full model or guess a foreign instance ID.
                state.lm_load_uncertain = true;
                let response = state.client.model_request(
                    "api/v1/models/load",
                    Some(&json!({"model":key})),
                    &AnalysisControl::default(),
                    Duration::from_secs(300),
                )?;
                let instance = response["instance_id"]
                    .as_str()
                    .context("Geladene Modellinstanz fehlt.")?;
                valid_model_name(instance)?;
                ensure!(
                    response["status"] == "loaded" && response["type"] == "llm",
                    "Textmodell wurde nicht als geladen bestätigt."
                );
                ensure!(!existing.contains(&instance), "Der Dienst hat eine bereits vorhandene Modellinstanz zurückgegeben; deren dauerhafte Reservierung ist nicht bestätigt und sie wird nicht entladen.");
                Loaded::LmStudio {
                    instance: instance.to_owned(),
                }
            }
        };
        state.loaded = Some(loaded);
        state.lm_load_uncertain = false;
        state.refresh_failed = false;
        if ollama_keep_alive(&state).is_some_and(|seconds| seconds > 0) {
            self.ensure_refresher()?;
        }
        control.check()
    }
    /// One weakly owned worker per bounded slot isolates each provider's delay.
    /// It retains neither meetings nor the slot between refreshes; the registry
    /// already limits the application to 128 distinct routes.
    fn ensure_refresher(&self) -> Result<()> {
        let weak_slot = Arc::downgrade(&self.slot);
        let result = self.slot.refresher.get_or_init(|| {
            std::thread::Builder::new()
                .name("local-model-residency".into())
                .spawn(move || loop {
                    std::thread::park_timeout(OLLAMA_REFRESH_INTERVAL);
                    let Some(slot) = weak_slot.upgrade() else {
                        break;
                    };
                    // A load/release already holds this lock; never wait behind it
                    // or hold the global registry during provider requests.
                    let Ok(mut state) = slot.state.try_lock() else {
                        continue;
                    };
                    if ollama_keep_alive(&state).is_some_and(|seconds| seconds > 0) {
                        state.refresh_failed =
                            renew_ollama(&mut state, Duration::from_secs(10)).is_err();
                    }
                })
                .map(|worker| worker.thread().clone())
                .map_err(|_| ())
        });
        ensure!(result.is_ok(), "Die regelmäßige Modellreservierung konnte nicht gestartet werden. Anwendung vor weiterer Auswertung neu starten.");
        Ok(())
    }
    /// Returns only routing policy for the selected model; never session or audio data.
    pub(crate) fn inference(&self) -> Result<(LocalProvider, String, i64)> {
        let state = self
            .slot
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Lokales Modell ist gesperrt."))?;
        let name = match state
            .loaded
            .as_ref()
            .context("Lokales Modell wurde noch nicht geladen.")?
        {
            Loaded::Ollama { .. } => state.client.model.clone(),
            Loaded::LmStudio { instance } => instance.clone(),
        };
        ensure!(!state.refresh_failed, "Die lokale Modellreservierung konnte nicht erneuert werden. Modelldienst prüfen und Auswertung fortsetzen.");
        Ok((state.provider, name, ollama_keep_alive(&state).unwrap_or(0)))
    }
    /// Compares logical slot identity without merging different routes or credentials.
    pub fn same_model(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.slot, &other.slot)
    }
}

impl Drop for ModelLease {
    /// Releases only the final logical owner, deferring cleanup while any loopback alias remains active.
    fn drop(&mut self) {
        if self.slot.users.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let Ok(_transition) = self.slot.transition.lock() else {
            return;
        };
        let Ok(slots) = self.slots.lock() else {
            return;
        };
        // Different loopback spellings can reach the same provider. Conservatively defer
        // cleanup across all routes to that provider/model, without merging their routes,
        // credentials or owned instance IDs. A distinct server may simply stay warm longer.
        let same_model = |slot: &&Arc<Slot>| {
            slot.provider == self.slot.provider
                && if slot.provider == LocalProvider::Ollama {
                    model_name_matches(Some(&slot.model), &self.slot.model)
                } else {
                    slot.model == self.slot.model
                }
        };
        if slots
            .values()
            .filter(same_model)
            .any(|slot| slot.users.load(Ordering::Acquire) > 0)
        {
            return;
        }
        let releasing: Vec<_> = slots.values().filter(same_model).cloned().collect();
        // Keep the family transition through provider acknowledgement, but release
        // the shared registry before any slot-state wait or management request.
        drop(slots);
        // LM Studio owns explicit instance IDs. Ollama exposes only a shared model
        // TTL, so logical release must not rewrite policy another client changed.
        for slot in releasing {
            slot.release();
        }
    }
}
/// Groups cleanup serialization conservatively; inference and ownership name matching
/// remain exact. Repeated implicit latest suffixes share a gate without broadening unloads.
fn cleanup_group_model(provider: LocalProvider, model: &str) -> &str {
    match provider {
        LocalProvider::Ollama => model.trim_end_matches(":latest"),
        LocalProvider::LmStudio => model,
    }
}
impl Slot {
    /// Serializes the final release against refresh; no subsequent refresh can
    /// recreate a model once its loaded state was removed under this mutex.
    fn release(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(loaded) = state.loaded.take() else {
            return;
        };
        let expected_instance = match &loaded {
            Loaded::LmStudio { instance } => Some(instance.clone()),
            _ => None,
        };
        let control = AnalysisControl::default();
        let result = match loaded {
            Loaded::Ollama { .. } => {
                // Ollama has no per-client instance ownership/CAS. Stop our renewal
                // without an unload or cached-TTL restore: either would overwrite
                // an independent client's later adoption, extension or removal.
                // App-owned loads remain bounded by their existing server TTL.
                self.cleanup_failed.store(false, Ordering::Release);
                return;
            }
            Loaded::LmStudio { instance } => state.client.model_request(
                "api/v1/models/unload",
                Some(&json!({"instance_id":instance})),
                &control,
                Duration::from_secs(10),
            ),
        };
        self.cleanup_failed.store(
            match result {
                Ok(response) => match state.provider {
                    LocalProvider::Ollama => response["done"] != true,
                    LocalProvider::LmStudio => {
                        response["instance_id"].as_str() != expected_instance.as_deref()
                    }
                },
                Err(_) => true,
            },
            Ordering::Release,
        );
    }
}

/// Preserve foreign indefinite reservations; owned loads receive only finite TTLs.
fn keep_alive_for(release_after: i64) -> i64 {
    if release_after == -1 {
        -1
    } else {
        release_after.max(OLLAMA_KEEP_ALIVE_SECONDS)
    }
}

/// Unloaded slots and LM Studio explicit instances never enter the Ollama refresher.
fn ollama_keep_alive(state: &State) -> Option<i64> {
    match state.loaded {
        Some(Loaded::Ollama { release_after }) => Some(keep_alive_for(release_after)),
        _ => None,
    }
}

/// Refresh only the existing slot's policy; failed renewal is surfaced before inference.
fn renew_ollama(state: &mut State, timeout: Duration) -> Result<()> {
    let seconds = ollama_keep_alive(state).context("Lokale Modellreservierung fehlt.")?;
    let response = state.client.model_request(
        "api/generate",
        Some(&json!({"model":state.client.model,"stream":false,"keep_alive":seconds})),
        &AnalysisControl::default(),
        timeout,
    )?;
    ensure!(
        response["done"] == true,
        "Lokale Modellreservierung wurde nicht erneut bestätigt."
    );
    state.refresh_failed = false;
    Ok(())
}

/// Rejects empty, oversized and control-bearing provider identifiers before management requests.
fn valid_model_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 300 && !value.chars().any(char::is_control),
        "Ungültige lokale Modellkennung."
    );
    Ok(())
}
/// Accepts only exact Ollama names and its implicit latest tag; no fuzzy model selection.
fn model_name_matches(value: Option<&str>, model: &str) -> bool {
    value.is_some_and(|value| {
        value == model
            || value.strip_suffix(":latest") == Some(model)
            || model.strip_suffix(":latest") == Some(value)
    })
}
