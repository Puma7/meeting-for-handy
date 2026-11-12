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

/// Date the latest actual shared-input change, excluding override-only activity.
/// An untimestamped final change stays unknown; run creation is not sharing authority.
pub(crate) fn shared_timestamp(
    folder: &Path,
    expected: &WorkspaceContext,
) -> Result<Option<chrono::DateTime<chrono::FixedOffset>>> {
    let mut previous = None;
    let mut stamp = None;
    let current = replay(folder, expected, |context, at| {
        if previous.as_ref().is_none_or(|(agenda, notes)| {
            agenda != &context.agenda || notes != &context.shared_notes
        }) {
            stamp = at;
        }
        previous = Some((context.agenda.clone(), context.shared_notes.clone()));
        Ok(())
    })?;
    ensure!(
        current == *expected,
        "Der native Meetingkontext wurde inzwischen geändert. Erneut versuchen."
    );
    Ok(stamp)
}
/// Reconstruct every context-only transition after the model checkpoint, including withdrawn agenda entries and their human history.
pub(crate) fn project(folder: &Path, state: &mut WorkspaceState) -> Result<WorkspaceContext> {
    let base = state.context.clone();
    let mut prior_agenda: Option<Vec<Agenda>> = None;
    let mut prior_overrides = 0;
    replay(folder, &base, |context, at| {
        record_activity(state, context, prior_agenda.as_deref(), prior_overrides, at);
        if context.revision > base.revision {
            state.apply_context(context)?;
        }
        prior_agenda = Some(context.agenda.clone());
        prior_overrides = context.overrides.len();
        Ok(())
    })
}

/// Retain per-entity human chronology independently of unrelated note or model activity.
fn record_activity(
    state: &mut WorkspaceState,
    context: &WorkspaceContext,
    prior_agenda: Option<&[Agenda]>,
    prior_overrides: usize,
    at: Option<chrono::DateTime<chrono::FixedOffset>>,
) {
    state.context_updated_at = state.context_updated_at.max(at);
    let Some(at) = at else {
        return;
    };
    let mut ids = context
        .overrides
        .iter()
        .skip(prior_overrides)
        .map(|action| action.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(prior) = prior_agenda {
        for entry in prior.iter().chain(&context.agenda) {
            if prior.iter().find(|a| a.id == entry.id)
                != context.agenda.iter().find(|a| a.id == entry.id)
            {
                ids.insert(entry.id.clone());
            }
        }
    }
    for id in ids {
        state
            .human_activity
            .entry(id)
            .and_modify(|old| *old = (*old).max(at))
            .or_insert(at);
    }
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
    let mut checkpoint_reached = fallback.revision == 0;
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
        if index == 0 && context.revision > fallback.revision {
            // Older checkpoints can have no separate journal. Its first full reset
            // must then be the immediate successor and retain confirmed actions.
            ensure!(
                fallback.revision.checked_add(1) == Some(context.revision)
                    && context.overrides.starts_with(&fallback.overrides),
                "Der Meetingkontext schließt nicht an den gespeicherten Stand an."
            );
            checkpoint_reached = true;
        }
        if context.revision == fallback.revision {
            ensure!(
                context == *fallback,
                "Der Meetingkontext widerspricht dem gespeicherten Stand."
            );
            checkpoint_reached = true;
        }
        apply(&context, activity)?;
    }
    ensure!(
        checkpoint_reached && context.revision >= fallback.revision,
        "Der Meetingkontext endet vor dem gespeicherten Stand. Vorhandene Dateien bleiben erhalten."
    );
    Ok(context)
}
/// Append a durable context revision only when its base matches; retain every earlier human override.
pub(crate) fn update(
    folder: &Path,
    incoming: &WorkspaceContext,
) -> Result<(WorkspaceContext, WorkspaceState)> {
    update_checked(folder, incoming, |_, _| Ok(()))
}

pub(crate) fn update_checked(
    folder: &Path,
    incoming: &WorkspaceContext,
    verify: impl FnOnce(&WorkspaceState, &[Event]) -> Result<()>,
) -> Result<(WorkspaceContext, WorkspaceState)> {
    incoming.validate_request_budget()?;
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
    let prior_events = state.events.len();
    state.apply_context(&next)?;
    verify(&state, &state.events[prior_events..])?;
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
    record_activity(
        &mut state,
        &next,
        (!reset).then_some(current.agenda.as_slice()),
        if reset { 0 } else { current.overrides.len() },
        chrono::DateTime::parse_from_rfc3339(&entry.created_at).ok(),
    );
    Ok((next, state))
}
