//! Read-only Handy integration. Handy remains the source of truth for model IDs,
//! installed state and advertised capabilities; this crate has no model catalog
//! or downloader. Model additions are discovered by a new CLI query.

mod companion;
mod discovery;
mod identity;
mod paths;
mod process;
mod version;

#[cfg(feature = "native-asr")]
pub mod native;

pub use companion::{companion_capabilities, CompanionCapabilities, COMPANION_CONTRACT_VERSION};
pub use discovery::discover_handy;
pub use identity::{
    capture_model_identity, capture_optional_model_identity, read_model_snapshot,
    verify_catalog_model, verify_model_identity, ModelIdentity, RuntimeIdentity,
};
#[cfg(feature = "native-asr")]
pub use identity::{
    create_model_identity, verify_model_snapshot, write_model_snapshot,
    write_model_snapshot_cancellable,
};
pub use paths::ModelRoots;
pub use process::{CommandLimits, ModelListRunner, SystemModelListRunner};
pub use version::installed_version;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Handy was not found. Install Handy or select its executable.")]
    NotFound,
    #[error(
        "Handy could not be queried. Check the installation and try refreshing the model list."
    )]
    QueryFailed,
    #[error("Handy did not respond within the time limit. Existing recordings remain available.")]
    Timeout,
    #[error("The optional Handy check was cancelled.")]
    Cancelled,
    #[error("Handy returned more data than the permitted model-list limit.")]
    OutputLimit,
    #[error("Handy's model-list format is not supported. Update HandyMeeting before using this installation.")]
    UnsupportedSchema,
    #[error(
        "The selected model is unavailable, downloading, or does not advertise streaming support."
    )]
    ModelUnavailable,
    #[error("The selected model uses a source type this HandyMeeting version cannot resolve.")]
    UnsupportedSource,
    #[error("A model path or cache reference is unsafe and has been rejected.")]
    UnsafePath,
    #[error("The complete installed model file was not found. Open Handy and verify this model.")]
    ModelFileMissing,
    #[error("The selected model file is still partial. Finish its download in Handy first.")]
    PartialModel,
    #[error("Modellgewichte, Modellquelle oder Erkennungslaufzeit unterscheiden sich vom gespeicherten Sitzungsstand. Die bisherigen Audio- und Textstände bleiben erhalten; es wird kein anderes Modell eingesetzt.")]
    ModelIdentityChanged,
    #[error("Diese Sitzung besitzt keinen prüfbaren Modell-Snapshot dieser Beta. Originaldaten bleiben erhalten; zum Fortsetzen die ursprüngliche Programmversion verwenden.")]
    ModelSnapshotUnavailable,
    #[error("The native recognition runtime could not use this model: {0}")]
    Native(String),
    #[error("Handy integration could not read a required local file.")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;

/// The fields used by the bridge are required rather than defaulted. New
/// upstream fields are tolerated, but missing capability flags fail closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandyModel {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub source: serde_json::Value,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub supports_streaming: bool,
    #[serde(default)]
    pub supported_languages: Vec<String>,
}

pub const PARAKEET_ULTRA_ID: &str = "parakeet-ultra";

impl HandyModel {
    pub fn ready_for_batch(&self) -> bool {
        self.is_downloaded
            && !self.is_downloading
            && !self.supports_streaming
            && self.id == PARAKEET_ULTRA_ID
    }

    pub fn ready_for_streaming(&self) -> bool {
        self.is_downloaded && !self.is_downloading && self.supports_streaming
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationIdentity {
    pub executable: PathBuf,
    pub file_bytes: u64,
    pub modified_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub installation: InstallationIdentity,
    pub fetched_at_unix_ms: u64,
    pub fingerprint_sha256: String,
    pub models: Vec<HandyModel>,
}

impl ModelCatalog {
    pub fn installed_streaming_models(&self) -> impl Iterator<Item = &HandyModel> {
        self.models
            .iter()
            .filter(|model| model.ready_for_streaming())
    }

    pub fn model(&self, id: &str) -> Result<&HandyModel> {
        self.models
            .iter()
            .find(|model| model.id == id)
            .ok_or(BridgeError::ModelUnavailable)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedModel {
    pub model_id: String,
    pub path: PathBuf,
    pub resolution: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HandyBridge {
    executable: PathBuf,
    roots: ModelRoots,
    limits: CommandLimits,
}

impl HandyBridge {
    pub fn discover(explicit: Option<&Path>) -> Result<Self> {
        Self::new(discover_handy(explicit)?)
    }

    pub fn new(executable: impl AsRef<Path>) -> Result<Self> {
        let executable = std::fs::canonicalize(executable).map_err(|_| BridgeError::NotFound)?;
        if !executable.is_file() {
            return Err(BridgeError::NotFound);
        }
        let roots = ModelRoots::for_installation(&executable);
        Ok(Self {
            executable,
            roots,
            limits: CommandLimits::default(),
        })
    }

    /// Test/custom portable installations can provide explicitly selected roots.
    /// This does not alter Handy's environment or its settings.
    pub fn with_roots(mut self, roots: ModelRoots) -> Self {
        self.roots = roots;
        self
    }

    pub fn with_limits(mut self, limits: CommandLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn refresh(&self) -> Result<ModelCatalog> {
        self.refresh_with(&SystemModelListRunner)
    }

    /// Cancel and reap the owned catalogue query used by the optional setup probe.
    pub fn refresh_cancellable(&self, cancelled: impl Fn() -> bool) -> Result<ModelCatalog> {
        if cancelled() {
            return Err(BridgeError::Cancelled);
        }
        self.refresh_using(|executable, limits| {
            process::run_query_cancellable(
                executable,
                &["--list-models", "--json"],
                limits,
                &cancelled,
            )
        })
    }

    /// Each refresh executes a fresh query. A failed refresh returns an error;
    /// callers must not relabel an earlier catalog as current or selectable.
    pub fn refresh_with(&self, runner: &impl ModelListRunner) -> Result<ModelCatalog> {
        self.refresh_using(|executable, limits| runner.list_models(executable, limits))
    }

    fn refresh_using(
        &self,
        list_models: impl FnOnce(&Path, CommandLimits) -> Result<Vec<u8>>,
    ) -> Result<ModelCatalog> {
        let before = self.identity()?;
        let bytes = list_models(&self.executable, self.limits)?;
        if bytes.len() > self.limits.stdout_bytes {
            return Err(BridgeError::OutputLimit);
        }
        let models = parse_model_list(&bytes)?;
        let after = self.identity()?;
        if before != after {
            return Err(BridgeError::QueryFailed);
        }
        Ok(ModelCatalog {
            installation: after,
            fetched_at_unix_ms: unix_ms(SystemTime::now()),
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            models,
        })
    }

    pub fn resolve_streaming_model(&self, model: &HandyModel) -> Result<ResolvedModel> {
        self.roots.resolve(model)
    }
    pub fn resolve_downloaded_model(&self, model: &HandyModel) -> Result<ResolvedModel> {
        self.roots.resolve_downloaded(model)
    }

    pub fn identity(&self) -> Result<InstallationIdentity> {
        let metadata = std::fs::metadata(&self.executable)?;
        if !metadata.is_file() {
            return Err(BridgeError::NotFound);
        }
        Ok(InstallationIdentity {
            executable: self.executable.clone(),
            file_bytes: metadata.len(),
            modified_unix_ms: unix_ms(metadata.modified()?),
        })
    }
}

pub fn parse_model_list(bytes: &[u8]) -> Result<Vec<HandyModel>> {
    // Some Windows console writers emit UTF-8 BOM. No log-line stripping or
    // scanning for '[' is allowed: mixed diagnostic/output is not valid JSON.
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let models: Vec<HandyModel> =
        serde_json::from_slice(bytes).map_err(|_| BridgeError::UnsupportedSchema)?;
    if models.len() > 10_000 {
        return Err(BridgeError::OutputLimit);
    }
    let mut ids = HashSet::new();
    for model in &models {
        if model.id.trim().is_empty()
            || model.id.len() > 4096
            || model.name.len() > 4096
            || model.id.chars().any(char::is_control)
            || model.name.chars().any(char::is_control)
            || !ids.insert(&model.id)
        {
            return Err(BridgeError::UnsupportedSchema);
        }
        // Noninstalled catalog entries can have metadata-only placeholders.
        // A selected/installed model is validated in paths::resolve.
    }
    Ok(models)
}

fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}
