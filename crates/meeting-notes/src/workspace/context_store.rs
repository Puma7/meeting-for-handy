//! Human changes have their own short, durable transaction, independent of slow inference.
use super::{model::*, store};
use anyhow::{ensure, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    #[serde(default)]
    created_at: String,
    revision: u64,
    reset: bool,
    agenda: Option<Vec<Agenda>>,
    shared_notes: Option<Vec<SharedNote>>,
    overrides: Vec<Override>,
}
/// Acquire the short context-write lease without waiting behind inference.
pub(crate) fn lock(folder: &Path) -> Result<File> {
    let f = store::open_lock(folder, "context.lock")?;
    f.try_lock_exclusive().map_err(|_| {
        anyhow::anyhow!("Der Meetingkontext wird gerade geändert. Erneut versuchen.")
    })?;
    Ok(f)
}
/// Replay contiguous human-context revisions; reject gaps or invalid entries without rewriting them.
pub(crate) fn read(folder: &Path, fallback: &WorkspaceContext) -> Result<WorkspaceContext> {
    store::check_directory(folder)?;
    let mut paths = fs::read_dir(folder)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("context-") && s.ends_with(".json"))
    });
    paths.sort();
    if paths.is_empty() {
        return Ok(fallback.clone());
    }
    let mut context = WorkspaceContext::default();
    for (index, path) in paths.iter().enumerate() {
        let bytes = store::read_bounded(path)?;
        let entry: Entry = serde_json::from_slice(&bytes)?;
        if index == 0 {
            ensure!(
                entry.reset
                    && entry.revision > 0
                    && entry.agenda.is_some()
                    && entry.shared_notes.is_some(),
                "Anfang des Meetingkontexts fehlt."
            );
            context.revision = entry.revision - 1;
        } else {
            ensure!(!entry.reset, "Unerwarteter neuer Anfang im Meetingkontext.");
        }
        ensure!(
            entry.revision == context.revision + 1,
            "Meetingkontext ist unvollständig."
        );
        if let Some(agenda) = entry.agenda {
            context.agenda = agenda;
        }
        if let Some(notes) = entry.shared_notes {
            context.shared_notes = notes;
        }
        context.overrides.extend(entry.overrides);
        context.revision = entry.revision;
        context.validate()?;
    }
    Ok(context)
}
/// Append a durable context revision only when its base matches; retain every earlier human override.
pub(crate) fn update(
    folder: &Path,
    incoming: &WorkspaceContext,
) -> Result<(WorkspaceContext, WorkspaceState)> {
    incoming.validate()?;
    let _guard = lock(folder)?;
    let mut state = store::read(folder)?;
    let current = read(folder, &state.context)?;
    ensure!(
        incoming.revision == current.revision,
        "Der Meetingkontext wurde inzwischen geändert. Aktuellen Stand verwenden."
    );
    ensure!(
        incoming.overrides.starts_with(&current.overrides),
        "Bereits bestätigte menschliche Änderungen dürfen nicht entfernt werden."
    );
    let mut next = incoming.clone();
    next.revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Zu viele Kontextfassungen."))?;
    state.apply_context(&next)?;
    let entries = fs::read_dir(folder)?.collect::<std::io::Result<Vec<_>>>()?;
    let reset = !entries.iter().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|s| s.starts_with("context-") && s.ends_with(".json"))
    });
    let entry = Entry {
        created_at: chrono::Utc::now().to_rfc3339(),
        revision: next.revision,
        reset,
        agenda: (reset || next.agenda != current.agenda).then(|| next.agenda.clone()),
        shared_notes: (reset || next.shared_notes != current.shared_notes)
            .then(|| next.shared_notes.clone()),
        overrides: if reset {
            next.overrides.clone()
        } else {
            next.overrides[current.overrides.len()..].to_vec()
        },
    };
    let bytes = serde_json::to_vec(&entry)?;
    ensure!(bytes.len() <= 2_000_000, "Meetingkontext ist zu groß.");
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(folder.join(format!("context-{:08}.json", next.revision)))
        .map_err(|e| e.error)?;
    Ok((next, state))
}
