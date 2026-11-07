//! Explicit read-only access to separate saved workspace generations. No client or writer is opened.
use super::{load_run, store, LoadedRun};
use crate::workspace::{self, WorkspaceState};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHistoryEntry {
    pub run_id: String,
    pub prompt_version: String,
    pub model: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub entity_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHistoryListing {
    pub entries: Vec<WorkspaceHistoryEntry>,
    pub unreadable_entries: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedWorkspace {
    #[serde(flatten)]
    pub entry: WorkspaceHistoryEntry,
    pub workspace: WorkspaceState,
}

/// Map only known generations; no client-supplied path component is used.
fn generations() -> [(&'static str, &'static str); 3] {
    [
        (workspace::PROMPT_VERSION, workspace::WORKSPACE_FOLDER),
        (
            workspace::PREVIOUS_PROMPT_VERSION,
            workspace::PREVIOUS_WORKSPACE_FOLDER,
        ),
        (
            workspace::LEGACY_PROMPT_VERSION,
            workspace::LEGACY_WORKSPACE_FOLDER,
        ),
    ]
}

/// Validate the parent independently so a linked history root cannot redirect a run lookup.
fn checked_root(root: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(root)?;
    ensure!(
        meta.is_dir() && !crate::service::is_linked(&meta),
        "Auswertungsordner ist nicht sicher lesbar."
    );
    Ok(())
}

/// Read exactly the selected generation, retaining its original unlocked entities and full events.
fn generation(root: &Path, run: &LoadedRun, version: &str) -> Result<SavedWorkspace> {
    let name = generations()
        .into_iter()
        .find(|(v, _)| *v == version)
        .map(|(_, name)| name)
        .context("Unbekannte Auswertungsfassung.")?;
    let state = workspace::read_state(&root.join(&run.run.id).join(name))?;
    ensure!(
        state.prompt_version() == version,
        "Auswertungsordner und Promptfassung passen nicht zusammen."
    );
    Ok(SavedWorkspace {
        entry: WorkspaceHistoryEntry {
            run_id: run.run.id.clone(),
            prompt_version: version.into(),
            model: run.run.model.clone(),
            created_at: run.run.created_at.clone(),
            updated_at: state.activity_at().map(|at| at.to_rfc3339()),
            entity_count: state.entities.len(),
        },
        workspace: state,
    })
}

/// List all readable generations only on explicit request; report unreadable data without removing it.
pub fn list_workspace_history(root: &Path, meeting_id: &str) -> Result<WorkspaceHistoryListing> {
    match std::fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceHistoryListing {
                entries: Vec::new(),
                unreadable_entries: 0,
            })
        }
        Err(error) => return Err(error.into()),
        Ok(_) => checked_root(root)?,
    }
    let (runs, mut unreadable_entries) = super::list_runs(root)?;
    let mut entries = Vec::new();
    for run in runs {
        if run.run.meeting_id != meeting_id {
            unreadable_entries += 1;
            continue;
        }
        for (version, name) in generations() {
            match std::fs::symlink_metadata(root.join(&run.run.id).join(name)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    unreadable_entries += 1;
                    continue;
                }
                Ok(_) => {}
            }
            match generation(root, &run, version) {
                Ok(saved)
                    if saved.workspace.revision > 0 || saved.workspace.context.revision > 0 =>
                {
                    entries.push(saved.entry)
                }
                Ok(_) => {}
                Err(_) => unreadable_entries += 1,
            }
        }
    }
    let activity = |e: &WorkspaceHistoryEntry| {
        chrono::DateTime::parse_from_rfc3339(e.updated_at.as_deref().unwrap_or(&e.created_at)).ok()
    };
    entries.sort_by(|a, b| {
        (activity(b), &b.run_id, &b.prompt_version).cmp(&(
            activity(a),
            &a.run_id,
            &a.prompt_version,
        ))
    });
    Ok(WorkspaceHistoryListing {
        entries,
        unreadable_entries,
    })
}

/// Load one validated run and generation, independently of whichever live job is currently displayed.
pub fn read_workspace_history(
    root: &Path,
    meeting_id: &str,
    run_id: &str,
    version: &str,
) -> Result<SavedWorkspace> {
    ensure!(store::is_hex_id(run_id), "Ungültige Auswertungskennung.");
    checked_root(root)?;
    let run = load_run(&root.join(run_id))?;
    ensure!(
        run.run.meeting_id == meeting_id,
        "Die Auswertung gehört zu einem anderen Meeting."
    );
    generation(root, &run, version)
}
