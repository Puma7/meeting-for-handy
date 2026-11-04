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
    replay(folder, fallback, |_, _| Ok(()))
}
/// Reconstruct every context-only transition after the model checkpoint, including withdrawn agenda entries and their human history.
pub(crate) fn project(folder: &Path, state: &mut WorkspaceState) -> Result<WorkspaceContext> {
    let base = state.context.clone();
    replay(folder, &base, |context, at| {
        state.context_updated_at = state.context_updated_at.max(at);
        if context.revision > base.revision {
            state.apply_context(context)?;
        }
        Ok(())
    })
}
/// Accept only an uninterrupted suffix of human-only revisions; a revoked and restored shared input still invalidates inference.
pub(crate) fn human_suffix(folder: &Path, expected: &WorkspaceContext) -> Result<WorkspaceContext> {
    replay(folder, expected, |context, _| {
        if context.revision >= expected.revision {
            ensure!(
                context.agenda == expected.agenda
                    && context.shared_notes == expected.shared_notes
                    && context.overrides.starts_with(&expected.overrides),
                "Freigegebene Eingaben wurden während der Auswertung geändert."
            );
            if context.revision == expected.revision {
                ensure!(
                    context == expected,
                    "Die Kontextbasis wurde inzwischen geändert."
                );
            }
        }
        Ok(())
    })
}
fn replay(
    folder: &Path,
    fallback: &WorkspaceContext,
    mut apply: impl FnMut(
        &WorkspaceContext,
        Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<()>,
) -> Result<WorkspaceContext> {
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
        let activity = if entry.created_at.is_empty() {
            None // Older context revisions did not store an activity timestamp.
        } else {
            Some(
                chrono::DateTime::parse_from_rfc3339(&entry.created_at).map_err(|_| {
                    anyhow::anyhow!("Zeitstempel des Meetingkontexts ist beschädigt.")
                })?,
            )
        };
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
        apply(&context, activity)?;
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
    let current = project(folder, &mut state)?;
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
    #[cfg(unix)]
    File::open(folder)?.sync_all()?;
    state.context_updated_at = state
        .context_updated_at
        .max(chrono::DateTime::parse_from_rfc3339(&entry.created_at).ok());
    Ok((next, state))
}
