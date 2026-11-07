//! Append-only storage of live notes. Every file is created once and never replaced; reading
//! checks every stored statement again and blocks instead of silently shrinking saved work.
use super::sources::LiveSource;
use super::trigger::Trigger;
use crate::service::{create_json, is_linked};
use crate::validation::{validate_rejections, verify_facts};
use crate::{Fact, Kind, Rejections, Source, MAX_ITEMS_PER_ANSWER};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// At most this many points are kept in one rolling overview.
pub const MAX_OVERVIEW_ITEMS: usize = 8;
const MAX_FILE_BYTES: u64 = 16_000_000;
const MAX_WINDOWS: usize = 100_000;

/// Immutable identity of one live notes run. Never contains credentials or transcript text.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveRun {
    pub id: String,
    pub meeting_id: String,
    pub created_at: String,
    pub model: String,
    pub mode: String,
    pub endpoint: String,
    pub prompt_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_profile_hash: Option<String>,
    pub language: String,
}

/// One model window: the exact sources the model saw and the statements that passed the check.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveWindow {
    pub index: u32,
    pub created_at: String,
    pub trigger: Trigger,
    pub sources: Vec<LiveSource>,
    pub points: Vec<Fact>,
    pub decisions: Vec<Fact>,
    pub tasks: Vec<Fact>,
    pub questions: Vec<Fact>,
    #[serde(default)]
    pub rejected: Rejections,
}
impl LiveWindow {
    /// The statement lists with the validation kind each one is checked against.
    pub(crate) fn lists(&self) -> [(&'static str, Kind, &Vec<Fact>); 4] {
        [
            ("points", Kind::Summary, &self.points),
            ("decisions", Kind::Decisions, &self.decisions),
            ("tasks", Kind::Tasks, &self.tasks),
            ("questions", Kind::Analysis, &self.questions),
        ]
    }
    pub(crate) fn statement_count(&self) -> usize {
        self.lists().iter().map(|(_, _, items)| items.len()).sum()
    }
}

/// A rolling overview of the meeting after window `window`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveOverview {
    pub index: u32,
    pub window: u32,
    pub created_at: String,
    pub items: Vec<Fact>,
    #[serde(default)]
    pub rejected: Rejections,
}

/// A fully verified run as read from disk.
#[derive(Clone, Debug)]
pub struct LoadedRun {
    pub run: LiveRun,
    pub windows: Vec<LiveWindow>,
    pub overviews: Vec<LiveOverview>,
}
impl LoadedRun {
    pub(crate) fn sources(&self) -> impl Iterator<Item = &LiveSource> {
        self.windows.iter().flat_map(|w| w.sources.iter())
    }
}

pub(crate) fn is_hex_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}

pub(crate) fn validate_run(run: &LiveRun) -> Result<()> {
    let workspace = crate::workspace::GENERATIONS
        .iter()
        .any(|(version, _)| *version == run.prompt_version);
    ensure!(
        is_hex_id(&run.id)
            && !run.meeting_id.is_empty()
            && run.meeting_id.len() <= 128
            && !run.meeting_id.chars().any(char::is_control)
            && ["local", "cloud"].contains(&run.mode.as_str())
            && !run.model.trim().is_empty()
            && run.model.len() <= 512
            && !run.endpoint.is_empty()
            && run.endpoint.len() <= 4096
            && !run.prompt_version.trim().is_empty()
            && run.prompt_version.len() <= 100
            && !run.prompt_version.chars().any(char::is_control)
            && (["de", "en"].contains(&run.language.as_str())
                || (workspace && crate::workspace::supports_output_language(&run.language)))
            && run.prompt_profile_hash.as_ref().is_none_or(|hash| {
                workspace && hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())
            })
            && chrono::DateTime::parse_from_rfc3339(&run.created_at).is_ok(),
        "Gespeicherte Live-Zusammenfassung ist beschädigt oder nicht kompatibel."
    );
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !is_linked(&metadata) && metadata.len() <= MAX_FILE_BYTES,
        "Gespeicherte Live-Zusammenfassung ist nicht sicher lesbar."
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "Gespeicherte Live-Zusammenfassung ist zu groß."
    );
    Ok(bytes)
}

fn checked_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !is_linked(&metadata),
        "Ordner der Live-Zusammenfassung ist kein sicherer Ordner."
    );
    Ok(())
}

pub(crate) fn window_path(folder: &Path, index: u32) -> PathBuf {
    folder.join(format!("window-{index:06}.json"))
}
pub(crate) fn overview_path(folder: &Path, index: u32) -> PathBuf {
    folder.join(format!("overview-{index:06}.json"))
}

/// Numbered files of one kind; temporary files and unrelated names are ignored.
fn numbered(folder: &Path, prefix: &str) -> Result<Vec<u32>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder)? {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(number) = name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".json"))
        else {
            continue;
        };
        if number.len() == 6 && number.bytes().all(|c| c.is_ascii_digit()) {
            found.push(number.parse::<u32>()?);
        }
    }
    found.sort_unstable();
    ensure!(
        found.len() <= MAX_WINDOWS,
        "Zu viele gespeicherte Live-Aktualisierungen."
    );
    ensure!(
        found.iter().enumerate().all(|(i, n)| *n as usize == i + 1),
        "Eine gespeicherte Live-Aktualisierung fehlt. Vorhandene Dateien bleiben erhalten."
    );
    Ok(found)
}

fn check_window(window: &LiveWindow, index: u32, ids: &mut HashSet<String>) -> Result<()> {
    ensure!(
        window.index == index
            && !window.sources.is_empty()
            && chrono::DateTime::parse_from_rfc3339(&window.created_at).is_ok(),
        "Gespeicherte Live-Aktualisierung ist beschädigt."
    );
    for source in &window.sources {
        ensure!(
            !source.id.is_empty()
                && source.id.len() <= 32
                && source
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                && ids.insert(source.id.clone())
                && !source.key.is_empty()
                && source.key.len() <= 256
                && source.chunk >= 1
                && !source.text.trim().is_empty()
                && source.text.chars().count() <= super::sources::SOURCE_CHARS,
            "Gespeicherte Live-Quelle ist beschädigt oder doppelt."
        );
    }
    let sources: Vec<Source> = window.sources.iter().map(LiveSource::as_source).collect();
    for (_, kind, items) in window.lists() {
        ensure!(
            items.len() <= MAX_ITEMS_PER_ANSWER,
            "Zu viele Aussagen in einer gespeicherten Live-Aktualisierung."
        );
        verify_facts(items, &sources, &kind)?;
    }
    validate_rejections(&window.rejected)
}

fn check_overview(overview: &LiveOverview, index: u32, windows: &[LiveWindow]) -> Result<()> {
    ensure!(
        overview.index == index
            && overview.window >= 1
            && overview.window as usize <= windows.len()
            && !overview.items.is_empty()
            && overview.items.len() <= MAX_OVERVIEW_ITEMS
            && chrono::DateTime::parse_from_rfc3339(&overview.created_at).is_ok(),
        "Gespeicherte Live-Kurzfassung ist beschädigt."
    );
    // An overview may only cite sources that existed when it was written.
    let sources: Vec<Source> = windows[..overview.window as usize]
        .iter()
        .flat_map(|w| w.sources.iter().map(LiveSource::as_source))
        .collect();
    verify_facts(&overview.items, &sources, &Kind::Summary)?;
    validate_rejections(&overview.rejected)
}

/// Reads and verifies one run folder completely.
pub fn load_run(folder: &Path) -> Result<LoadedRun> {
    checked_directory(folder)?;
    let run: LiveRun = serde_json::from_slice(&read_bounded(&folder.join("run.json"))?)
        .context("Gespeicherte Live-Zusammenfassung ist beschädigt.")?;
    validate_run(&run)?;
    ensure!(
        folder.file_name().and_then(|n| n.to_str()) == Some(run.id.as_str()),
        "Live-Zusammenfassung und Ordner passen nicht zusammen."
    );
    let mut ids = HashSet::new();
    let mut windows = Vec::new();
    for index in numbered(folder, "window-")? {
        let window: LiveWindow =
            serde_json::from_slice(&read_bounded(&window_path(folder, index))?)
                .context("Gespeicherte Live-Aktualisierung ist beschädigt.")?;
        check_window(&window, index, &mut ids)?;
        windows.push(window);
    }
    let mut overviews: Vec<LiveOverview> = Vec::new();
    for index in numbered(folder, "overview-")? {
        let overview: LiveOverview =
            serde_json::from_slice(&read_bounded(&overview_path(folder, index))?)
                .context("Gespeicherte Live-Kurzfassung ist beschädigt.")?;
        check_overview(&overview, index, &windows)?;
        if let Some(previous) = overviews.last() {
            ensure!(
                overview.window >= previous.window,
                "Gespeicherte Live-Kurzfassungen sind nicht in Reihenfolge."
            );
        }
        overviews.push(overview);
    }
    Ok(LoadedRun {
        run,
        windows,
        overviews,
    })
}

/// Runs of a meeting, newest first. Unreadable runs stay on disk and are only counted.
pub fn list_runs(root: &Path) -> Result<(Vec<LoadedRun>, usize)> {
    let mut runs = Vec::new();
    let mut damaged = 0;
    if !root.exists() {
        return Ok((runs, damaged));
    }
    checked_directory(root)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_hex_id(&name) {
            continue;
        }
        match load_run(&entry.path()) {
            Ok(run) => runs.push(run),
            Err(_) => damaged += 1,
        }
    }
    // The run worked on last comes first, so a continued older run is not hidden behind a
    // newer one that was only tried briefly.
    let activity = |run: &LoadedRun| -> Option<chrono::DateTime<chrono::FixedOffset>> {
        std::iter::once(run.run.created_at.as_str())
            .chain(run.windows.iter().map(|w| w.created_at.as_str()))
            .chain(run.overviews.iter().map(|o| o.created_at.as_str()))
            .filter_map(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .max()
    };
    runs.sort_by(|a, b| (activity(b), b.run.id.as_str()).cmp(&(activity(a), a.run.id.as_str())));
    Ok((runs, damaged))
}

/// Creates a new run folder with its identity file. Existing runs are never touched.
pub(crate) fn create_run(root: &Path, run: &LiveRun) -> Result<LoadedRun> {
    validate_run(run)?;
    fs::create_dir_all(root)?;
    checked_directory(root)?;
    let folder = root.join(&run.id);
    if folder.exists() {
        bail!("Diese Live-Zusammenfassung existiert bereits.");
    }
    fs::create_dir(&folder)?;
    create_json(&folder.join("run.json"), run)?;
    Ok(LoadedRun {
        run: run.clone(),
        windows: Vec::new(),
        overviews: Vec::new(),
    })
}
