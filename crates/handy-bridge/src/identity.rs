//! Immutable model identity keeps a resumed session tied to the same weights
//! and native contract even when Handy's current catalog has changed.
use crate::{BridgeError, HandyModel, ResolvedModel, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(any(feature = "native-asr", test))]
use std::{fs::OpenOptions, io::Write};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

const SNAPSHOT_SCHEMA: u32 = 1;
const MAX_SNAPSHOT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeIdentity {
    pub compiled_version: String,
    pub loaded_version: String,
    pub loaded_commit: String,
    pub public_header_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelIdentity {
    pub canonical_path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub source_model_metadata: HandyModel,
    pub resolved_revision: Option<String>,
    pub native_runtime_identity: RuntimeIdentity,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    schema_version: u32,
    identity: ModelIdentity,
    #[serde(default)]
    provenance: serde_json::Value,
}

fn file_hash(path: &Path) -> Result<(PathBuf, u64, String)> {
    let canonical = path
        .canonicalize()
        .map_err(|_| BridgeError::ModelIdentityChanged)?;
    let mut file = File::open(&canonical).map_err(|_| BridgeError::ModelIdentityChanged)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() == 0 {
        return Err(BridgeError::ModelIdentityChanged);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        count = count
            .checked_add(read as u64)
            .ok_or(BridgeError::ModelIdentityChanged)?;
    }
    let after = file.metadata()?;
    if count != before.len()
        || before.len() != after.len()
        || before.modified()? != after.modified()?
    {
        return Err(BridgeError::ModelIdentityChanged);
    }
    Ok((canonical, count, format!("{:x}", hash.finalize())))
}

pub fn capture_model_identity(
    model: &HandyModel,
    resolved: &ResolvedModel,
    runtime: RuntimeIdentity,
) -> Result<ModelIdentity> {
    if model.id != resolved.model_id || !(model.ready_for_streaming() || model.ready_for_batch()) {
        return Err(BridgeError::ModelUnavailable);
    }
    capture_optional_model_identity(model, resolved, runtime)
}
/// An experimental role has its own identity and never relaxes the ASR resume gate.
pub fn capture_optional_model_identity(
    model: &HandyModel,
    resolved: &ResolvedModel,
    runtime: RuntimeIdentity,
) -> Result<ModelIdentity> {
    if model.id != resolved.model_id || !model.is_downloaded || model.is_downloading {
        return Err(BridgeError::ModelUnavailable);
    }
    let (canonical_path, bytes, sha256) = file_hash(&resolved.path)?;
    Ok(ModelIdentity {
        canonical_path,
        bytes,
        sha256,
        source_model_metadata: model.clone(),
        resolved_revision: resolved.revision.clone(),
        native_runtime_identity: runtime,
    })
}

/// Catalog refresh may add models or change editorial metadata. Only the
/// selected model's identity, filename, source and availability gate resume.
pub fn verify_catalog_model(identity: &ModelIdentity, current: &HandyModel) -> Result<()> {
    let original = &identity.source_model_metadata;
    if !(current.ready_for_streaming() || current.ready_for_batch()) {
        return Err(BridgeError::ModelUnavailable);
    }
    if original.id != current.id
        || original.supports_streaming != current.supports_streaming
        || original.filename != current.filename
        || original.source != current.source
    {
        return Err(BridgeError::ModelIdentityChanged);
    }
    Ok(())
}

pub fn verify_model_identity(
    identity: &ModelIdentity,
    model_path: &Path,
    runtime: &RuntimeIdentity,
) -> Result<()> {
    if &identity.native_runtime_identity != runtime {
        return Err(BridgeError::ModelIdentityChanged);
    }
    let (path, bytes, hash) = file_hash(model_path)?;
    if path != identity.canonical_path || bytes != identity.bytes || hash != identity.sha256 {
        return Err(BridgeError::ModelIdentityChanged);
    }
    Ok(())
}

pub fn read_model_snapshot(session_dir: &Path) -> Result<ModelIdentity> {
    let path = session_dir.join("model-snapshot.json");
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| BridgeError::ModelSnapshotUnavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_SNAPSHOT_BYTES
    {
        return Err(BridgeError::ModelSnapshotUnavailable);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(BridgeError::ModelSnapshotUnavailable);
        }
    }
    let snapshot: Snapshot = serde_json::from_reader(File::open(path)?)
        .map_err(|_| BridgeError::ModelSnapshotUnavailable)?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA
        || !snapshot.identity.canonical_path.is_absolute()
        || snapshot.identity.bytes == 0
        || snapshot.identity.sha256.len() != 64
        || !snapshot
            .identity
            .sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(BridgeError::ModelSnapshotUnavailable);
    }
    Ok(snapshot.identity)
}

#[cfg(any(feature = "native-asr", test))]
fn write_snapshot(
    session_dir: &Path,
    identity: ModelIdentity,
    provenance: &serde_json::Value,
) -> Result<ModelIdentity> {
    let snapshot = Snapshot {
        schema_version: SNAPSHOT_SCHEMA,
        identity,
        provenance: provenance.clone(),
    };
    let bytes =
        serde_json::to_vec_pretty(&snapshot).map_err(|_| BridgeError::ModelSnapshotUnavailable)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(BridgeError::ModelSnapshotUnavailable);
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(session_dir.join("model-snapshot.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(snapshot.identity)
}

#[cfg(feature = "native-asr")]
pub fn create_model_identity(
    model: &HandyModel,
    resolved: &ResolvedModel,
) -> Result<ModelIdentity> {
    capture_model_identity(model, resolved, crate::native::runtime_identity())
}

#[cfg(feature = "native-asr")]
pub fn write_model_snapshot(
    session_dir: &Path,
    model: &HandyModel,
    resolved: &ResolvedModel,
    provenance: &serde_json::Value,
) -> Result<ModelIdentity> {
    write_snapshot(
        session_dir,
        create_model_identity(model, resolved)?,
        provenance,
    )
}

#[cfg(feature = "native-asr")]
pub fn verify_model_snapshot(session_dir: &Path, model_path: &Path) -> Result<ModelIdentity> {
    let identity = read_model_snapshot(session_dir)?;
    verify_model_identity(&identity, model_path, &crate::native::runtime_identity())?;
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> RuntimeIdentity {
        RuntimeIdentity {
            compiled_version: "0.4.1".into(),
            loaded_version: "0.4.1".into(),
            loaded_commit: "fixture".into(),
            public_header_hash: "fixture-abi".into(),
        }
    }
    fn fixture() -> (tempfile::TempDir, HandyModel, ResolvedModel, ModelIdentity) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.gguf");
        fs::write(&path, b"original synthetic weights").unwrap();
        let model = HandyModel {
            id: "same-model-id".into(),
            name: "Fixture".into(),
            filename: "weights.gguf".into(),
            source: serde_json::json!({"HuggingFace":{"repo_id":"fixture/model","revision":"R1"}}),
            is_downloaded: true,
            is_downloading: false,
            supports_streaming: true,
            supported_languages: vec![],
        };
        let resolved = ResolvedModel {
            model_id: model.id.clone(),
            path,
            resolution: "requested-revision".into(),
            revision: Some("R1".into()),
        };
        let identity = capture_model_identity(&model, &resolved, runtime()).unwrap();
        (dir, model, resolved, identity)
    }
    #[test]
    fn unchanged_weight_runtime_and_selected_catalog_survive_snapshot_roundtrip() {
        let (dir, model, resolved, identity) = fixture();
        write_snapshot(
            dir.path(),
            identity.clone(),
            &serde_json::json!({"fixture":true}),
        )
        .unwrap();
        let loaded = read_model_snapshot(dir.path()).unwrap();
        assert_eq!(identity, loaded);
        verify_catalog_model(&loaded, &model).unwrap();
        verify_model_identity(&loaded, &resolved.path, &runtime()).unwrap();
    }
    #[test]
    fn same_id_with_new_handy_revision_is_refused() {
        let (_, mut model, _, identity) = fixture();
        model.source["HuggingFace"]["revision"] = "R2".into();
        assert!(matches!(
            verify_catalog_model(&identity, &model),
            Err(BridgeError::ModelIdentityChanged)
        ));
    }
    #[test]
    fn local_in_place_changes_with_the_same_byte_count_are_detected() {
        let (_dir, _, resolved, identity) = fixture();
        let original = fs::read(&resolved.path).unwrap();
        let mut modified = original.clone();
        modified[0] ^= 1;
        fs::write(&resolved.path, modified).unwrap();
        assert_eq!(
            fs::metadata(&resolved.path).unwrap().len(),
            original.len() as u64
        );
        assert!(matches!(
            verify_model_identity(&identity, &resolved.path, &runtime()),
            Err(BridgeError::ModelIdentityChanged)
        ));
    }
    #[test]
    fn changed_native_runtime_contract_is_refused() {
        let (_dir, _, resolved, identity) = fixture();
        let mut changed = runtime();
        changed.loaded_version = "0.4.2".into();
        assert!(matches!(
            verify_model_identity(&identity, &resolved.path, &changed),
            Err(BridgeError::ModelIdentityChanged)
        ));
    }
    #[test]
    fn unrelated_catalog_additions_and_editorial_changes_do_not_block_resume() {
        let (_dir, mut model, _, identity) = fixture();
        model.name = "New friendly label".into();
        model.supported_languages = vec!["de".into(), "en".into()];
        let mut new_model = model.clone();
        new_model.id = "unrelated-new-model".into();
        let catalog = [new_model, model];
        let current = catalog
            .iter()
            .find(|m| m.id == identity.source_model_metadata.id)
            .unwrap();
        verify_catalog_model(&identity, current).unwrap();
    }
    #[test]
    fn old_unversioned_snapshot_is_not_silently_upgraded_or_overwritten() {
        let (dir, _, _, identity) = fixture();
        let path = dir.path().join("model-snapshot.json");
        fs::write(&path, b"{\"model\":\"unversioned fixture\"}").unwrap();
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            read_model_snapshot(dir.path()),
            Err(BridgeError::ModelSnapshotUnavailable)
        ));
        assert!(write_snapshot(dir.path(), identity, &serde_json::Value::Null).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
