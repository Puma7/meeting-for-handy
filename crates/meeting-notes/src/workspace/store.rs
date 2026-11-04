use super::model::*;
use crate::{live::LiveSource, service::is_linked, Source};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 2_000_000;

/// Reject linked or non-directory workspace roots before accessing saved revisions.
pub(super) fn check_directory(folder: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(folder)?;
    ensure!(
        metadata.is_dir() && !is_linked(&metadata),
        "Meetingstand ist kein sicherer Ordner."
    );
    Ok(())
}

/// Open a regular coordination file without truncating an existing owner's bytes.
pub(super) fn open_lock(folder: &Path, name: &str) -> Result<File> {
    check_directory(folder)?;
    let path = folder.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !is_linked(&metadata),
            "Meetingstand enthält eine unzulässige Sperrdatei."
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?)
}

/// Read at most one bounded regular revision file; reject links and oversized input.
pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !is_linked(&metadata) && metadata.len() <= MAX_FILE_BYTES,
        "Gespeicherte Meetingfassung ist nicht sicher lesbar."
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "Gespeicherte Meetingfassung ist zu groß."
    );
    Ok(bytes)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Delta {
    #[serde(default)]
    superseded_sources: Vec<String>,
    #[serde(default)]
    replay_cursor: Option<usize>,
    #[serde(default)]
    applied_overrides: Vec<String>,
    #[serde(default)]
    context_excluded_ids: Vec<String>,
    schema: String,
    revision: u64,
    updated_at: Option<String>,
    context: WorkspaceContext,
    entities: Vec<Entity>,
    events: Vec<Event>,
    overview_refs: Vec<String>,
    originals: Vec<Source>,
    processed: Vec<LiveSource>,
}
impl Delta {
    /// Build an append-only delta, including new human events independently of model output.
    pub fn between(
        old: &WorkspaceState,
        new: &WorkspaceState,
        originals: Vec<Source>,
        _events: Vec<Event>,
    ) -> Self {
        Self {
            superseded_sources: new.superseded_sources.clone(),
            replay_cursor: new.replay_cursor,
            applied_overrides: new.applied_overrides.clone(),
            context_excluded_ids: new.context_excluded_ids.clone(),
            schema: super::PROMPT_VERSION.into(),
            revision: new.revision,
            updated_at: new.updated_at.clone(),
            context: new.context.clone(),
            entities: new
                .entities
                .iter()
                .filter(|e| !old.entities.iter().any(|o| o == *e))
                .cloned()
                .collect(),
            events: new
                .events
                .iter()
                .filter(|event| !old.events.iter().any(|prior| prior.id == event.id))
                .cloned()
                .collect(),
            overview_refs: new.overview_refs.clone(),
            originals,
            processed: new.processed[old.processed.len()..].to_vec(),
        }
    }
}
/// Hold the exclusive inference-writer lease and replay its existing durable state.
pub(crate) fn load(folder: &Path) -> Result<(File, WorkspaceState)> {
    fs::create_dir_all(folder)?;
    let lock = open_lock(folder, "writer.lock")?;
    lock.try_lock_exclusive()
        .context("Eine andere Auswertung verwendet diesen Meetingstand.")?;
    Ok((lock, read(folder)?))
}
/// Reconstruct workspace state from contiguous immutable revisions; never repair damaged input silently.
pub(crate) fn read(folder: &Path) -> Result<WorkspaceState> {
    check_directory(folder)?;
    let mut paths = fs::read_dir(folder)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("revision-") && s.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut state = WorkspaceState::default();
    for path in paths {
        let bytes = read_bounded(&path)?;
        let delta: Delta = serde_json::from_slice(&bytes)
            .context("Meetingstand ist beschädigt; Originale bleiben unverändert.")?;
        ensure!(
            delta.schema == super::PROMPT_VERSION && delta.revision == state.revision + 1,
            "Meetingfassungen sind unvollständig oder inkompatibel."
        );
        for e in delta.entities {
            if let Some(old) = state.entities.iter_mut().find(|old| old.id == e.id) {
                *old = e;
            } else {
                state.entities.push(e);
            }
        }
        state.superseded_sources = delta.superseded_sources;
        state.replay_cursor = delta.replay_cursor;
        state.applied_overrides = delta.applied_overrides;
        state.context_excluded_ids = delta.context_excluded_ids;
        state.revision = delta.revision;
        state.context = delta.context;
        state.updated_at = delta.updated_at;
        state.events.extend(delta.events);
        state.overview_refs = delta.overview_refs;
        state.originals.extend(delta.originals);
        state.processed.extend(delta.processed);
    }
    Ok(state)
}
/// Sync a bounded delta and publish it without replacing any existing revision.
pub(crate) fn append(folder: &Path, delta: &Delta) -> Result<()> {
    // Same-directory durable temp + no-clobber publication. A partial temp is never replayed.
    let mut temp = tempfile::NamedTempFile::new_in(folder)?;
    let bytes = serde_json::to_vec(delta)?;
    ensure!(bytes.len() <= 2_000_000, "Meetingänderung ist zu groß.");
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(folder.join(format!("revision-{:08}.json", delta.revision)))
        .map_err(|e| e.error)?;
    Ok(())
}
