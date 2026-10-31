//! Live notes while a meeting is still running: confirmed transcript text is summarized in
//! append-only windows, and a compact rolling overview is rebuilt from verified statements.
//! Every statement passes the same literal source check as post-meeting analysis.
mod prompt;
mod sources;
mod store;
mod trigger;

pub use prompt::LIVE_PROMPT_VERSION;
pub use sources::{next_window, pending, pending_chars, LiveSource, LiveText, WINDOW_CHARS};
pub use store::{
    list_runs, load_run, LiveOverview, LiveRun, LiveWindow, LoadedRun, MAX_OVERVIEW_ITEMS,
};
pub use trigger::{decide, Trigger, MAX_INTERVAL_MS, MIN_INTERVAL_MS};

use crate::diagnostics::{self, LiveDiagnostic, Operation, RequestResult};
use crate::service::create_json;
use crate::validation::screen_facts;
use crate::{
    AnalysisControl, Connection, Fact, Kind, Rejections, TextClient, MAX_ITEMS_PER_ANSWER,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The raw model answer of one window, parsed strictly before any statement is checked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowAnswer {
    points: Vec<Fact>,
    decisions: Vec<Fact>,
    tasks: Vec<Fact>,
    questions: Vec<Fact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverviewAnswer {
    items: Vec<Fact>,
}

/// What one update stored.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    /// Index of the stored window, if text was summarized.
    pub window: Option<u32>,
    pub accepted: usize,
    pub rejected: usize,
    /// Index of the stored overview, if it was rebuilt.
    pub overview: Option<u32>,
    /// The window was saved but the overview could not be rebuilt; it is retried later.
    pub overview_error: Option<String>,
    pub diagnostic_warning: Option<String>,
}

fn add_rejections(total: &mut Rejections, more: &Rejections) {
    for (reason, count) in more {
        let entry = total.entry(*reason).or_default();
        *entry = entry.saturating_add(*count);
    }
}

fn rejected_total(rejected: &Rejections) -> usize {
    rejected.values().fold(0, |sum, n| sum.saturating_add(*n))
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A running live notes session for one meeting. Only orchestration creates it and supplies
/// confirmed transcript text; the model client receives text only.
pub struct LiveRunner {
    client: TextClient,
    folder: PathBuf,
    state: LoadedRun,
    damaged_runs: usize,
    diagnostic_warning: std::cell::RefCell<Option<String>>,
    overview_attempt: std::cell::Cell<(u32, usize, usize)>,
}

impl LiveRunner {
    /// Keeps the chosen local model available across windows and the final stored overview.
    pub fn with_residency(
        mut self,
        lease: std::sync::Arc<crate::residency::ModelLease>,
    ) -> Result<Self> {
        self.client = self.client.with_residency(lease)?;
        Ok(self)
    }
    /// Continues the newest verified run with the same model, route, prompt and language, or
    /// starts a new one. Damaged runs stay untouched and are counted.
    pub fn start(
        root: &Path,
        meeting_id: &str,
        connection: Connection,
        language: &str,
    ) -> Result<Self> {
        anyhow::ensure!(
            ["de", "en"].contains(&language),
            "Unbekannte Sprache der Live-Zusammenfassung."
        );
        let client = TextClient::new(&connection)?;
        let identity = LiveRun {
            id: uuid::Uuid::new_v4().simple().to_string(),
            meeting_id: meeting_id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            model: connection.model.clone(),
            mode: connection.mode.clone(),
            endpoint: client.base.to_string(),
            prompt_version: LIVE_PROMPT_VERSION.to_string(),
            language: language.to_string(),
        };
        let (runs, damaged_runs) = list_runs(root)?;
        let existing = runs.into_iter().find(|loaded| {
            let run = &loaded.run;
            run.meeting_id == identity.meeting_id
                && run.model == identity.model
                && run.mode == identity.mode
                && run.endpoint == identity.endpoint
                && run.prompt_version == identity.prompt_version
                && run.language == identity.language
        });
        let state = match existing {
            Some(state) => state,
            None => store::create_run(root, &identity)?,
        };
        Ok(Self {
            client,
            folder: root.join(&state.run.id),
            state,
            damaged_runs,
            diagnostic_warning: Default::default(),
            overview_attempt: Default::default(),
        })
    }

    pub fn run(&self) -> &LiveRun {
        &self.state.run
    }

    /// Confirmed text not yet covered by a stored window.
    pub fn pending(&self, texts: &[LiveText]) -> Vec<LiveSource> {
        let processed: Vec<LiveSource> = self.state.sources().cloned().collect();
        sources::pending(&processed, texts)
    }

    /// True when stored windows have statements the newest overview does not include yet.
    pub fn overview_due(&self) -> bool {
        let covered = self.state.overviews.last().map_or(0, |o| o.window as usize);
        self.state.windows[covered..]
            .iter()
            .any(|w| w.statement_count() > 0)
    }

    /// Summarizes the next window of `pending` (if any) and then rebuilds the overview when
    /// new statements exist. The window file is the commit point; a failed overview is
    /// reported in the outcome and retried by the next update.
    pub fn update(
        &mut self,
        pending: &[LiveSource],
        trigger: Trigger,
        control: &AnalysisControl,
    ) -> Result<UpdateOutcome> {
        let mut outcome = UpdateOutcome::default();
        // The warning describes this update only; a later successful save clears it.
        self.diagnostic_warning.replace(None);
        if !pending.is_empty() {
            let window = self.summarize_window(pending, trigger, control)?;
            outcome.window = Some(window.index);
            outcome.accepted = window.statement_count();
            outcome.rejected = rejected_total(&window.rejected);
            self.state.windows.push(window);
        }
        // A large backlog of statements is folded in batch by batch until it is covered.
        while self.overview_due() {
            match self.rebuild_overview(control) {
                Ok(index) => outcome.overview = index,
                Err(error) if crate::is_cancelled(&error) => return Err(error),
                Err(error) => {
                    outcome.overview_error = Some(error.to_string());
                    break;
                }
            }
        }
        outcome.diagnostic_warning = self.diagnostic_warning.borrow().clone();
        Ok(outcome)
    }

    /// Requests and validates one bounded text window before exclusively storing it.
    /// Task deduplication includes owner and due date; every retained fact still needs
    /// an existing source ID and literal quote from this window's original sources.
    fn summarize_window(
        &self,
        pending: &[LiveSource],
        trigger: Trigger,
        control: &AnalysisControl,
    ) -> Result<LiveWindow> {
        let result = self.summarize_window_inner(pending, trigger, control);
        let (accepted, rejected) = result
            .as_ref()
            .map(|w| (w.statement_count(), rejected_total(&w.rejected)))
            .unwrap_or_default();
        self.record_diagnostic(
            Operation::Window,
            self.state.windows.len() as u32 + 1,
            accepted,
            rejected,
            result.is_ok(),
        );
        result
    }

    fn record_diagnostic(
        &self,
        operation: Operation,
        window: u32,
        accepted: usize,
        rejected: usize,
        success: bool,
    ) {
        let mut request = self.client.diagnostic();
        if success {
            request.result = RequestResult::Stored;
        } else if request.result == RequestResult::Envelope {
            request.result = RequestResult::Validation;
        }
        let value = LiveDiagnostic {
            operation,
            window,
            accepted,
            rejected,
            request,
        };
        if diagnostics::save(&self.folder, &value).is_err() {
            *self.diagnostic_warning.borrow_mut() = Some(
                "Diagnose konnte nicht gespeichert werden; Ergebnisdaten bleiben erhalten.".into(),
            );
        }
    }

    fn summarize_window_inner(
        &self,
        pending: &[LiveSource],
        trigger: Trigger,
        control: &AnalysisControl,
    ) -> Result<LiveWindow> {
        let window_sources = next_window(pending);
        let sources: Vec<_> = window_sources.iter().map(LiveSource::as_source).collect();
        let known: Vec<&str> = self
            .state
            .windows
            .iter()
            .flat_map(|w| w.lists().into_iter().flat_map(|(_, _, items)| items.iter()))
            .map(|f| f.text.as_str())
            .collect();
        let (system, user) = prompt::window_request(&self.state.run.language, &known, &sources)?;
        let answer = self.client.chat_json(
            &system,
            &user,
            "live_meeting_notes",
            prompt::window_schema(),
            4096,
            control,
        )?;
        control.check()?;
        let answer: WindowAnswer = serde_json::from_str(&answer)
            .context("Ungültige oder doppelte Felder in der KI-Antwort.")?;
        let mut rejected = Rejections::new();
        let mut screen = |facts: Vec<Fact>, kind: Kind| {
            let checked = screen_facts(facts, &sources, &kind, "", MAX_ITEMS_PER_ANSWER);
            add_rejections(&mut rejected, &checked.rejected);
            // Only identical commitments add nothing: task wording alone does not identify
            // an assignment. Different owners or deadlines must keep their own evidence.
            let mut seen = HashSet::new();
            checked
                .items
                .into_iter()
                .filter(|f| {
                    seen.insert((
                        normalized(&f.text),
                        (kind == Kind::Tasks).then(|| f.owner.clone()),
                        (kind == Kind::Tasks).then(|| f.due.clone()),
                    ))
                })
                .collect::<Vec<_>>()
        };
        let points = screen(answer.points, Kind::Summary);
        let decisions = screen(answer.decisions, Kind::Decisions);
        let tasks = screen(answer.tasks, Kind::Tasks);
        let questions = screen(answer.questions, Kind::Analysis);
        let window = LiveWindow {
            index: self.state.windows.len() as u32 + 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            trigger,
            sources: window_sources,
            points,
            decisions,
            tasks,
            questions,
            rejected,
        };
        control.check()?;
        self.client.diagnostic_stage(RequestResult::Persistence);
        create_json(&store::window_path(&self.folder, window.index), &window)?;
        Ok(window)
    }

    fn rebuild_overview(&mut self, control: &AnalysisControl) -> Result<Option<u32>> {
        self.overview_attempt
            .set((self.state.windows.len() as u32, 0, 0));
        let result = self.rebuild_overview_inner(control);
        let (window, accepted, rejected) = self.overview_attempt.get();
        self.record_diagnostic(
            Operation::Overview,
            window,
            accepted,
            rejected,
            result.is_ok(),
        );
        result
    }

    fn rebuild_overview_inner(&mut self, control: &AnalysisControl) -> Result<Option<u32>> {
        let previous = self.state.overviews.last();
        let covered = previous.map_or(0, |o| o.window as usize);
        // Whole windows in order, as many as fit into one request (at least one). Only the
        // windows actually sent count as covered; later ones follow in the next batch.
        let mut batch_end = covered;
        let mut batch_size = 0;
        for window in &self.state.windows[covered..] {
            let size = window.statement_count();
            if batch_end > covered && batch_size + size > prompt::OVERVIEW_INPUT_STATEMENTS {
                break;
            }
            batch_size += size;
            batch_end += 1;
        }
        let statements: Vec<prompt::Statement<'_>> = self.state.windows[covered..batch_end]
            .iter()
            .flat_map(|w| {
                w.lists()
                    .into_iter()
                    .flat_map(|(list, _, items)| items.iter().map(move |fact| (list, fact)))
            })
            .map(|(list, fact)| prompt::Statement { list, fact })
            .collect();
        let previous_items = previous.map_or(&[][..], |o| o.items.as_slice());
        let (system, user) =
            prompt::overview_request(&self.state.run.language, previous_items, &statements)?;
        self.overview_attempt.set((batch_end as u32, 0, 0));
        let answer = self.client.chat_json(
            &system,
            &user,
            "live_meeting_overview",
            prompt::overview_schema(),
            2048,
            control,
        )?;
        control.check()?;
        let answer: OverviewAnswer = serde_json::from_str(&answer)
            .context("Ungültige oder doppelte Felder in der KI-Antwort.")?;
        // The overview may cite only sections that existed up to the windows it covers.
        let sources: Vec<_> = self.state.windows[..batch_end]
            .iter()
            .flat_map(|w| w.sources.iter().map(LiveSource::as_source))
            .collect();
        let checked = screen_facts(
            answer.items,
            &sources,
            &Kind::Summary,
            "",
            MAX_OVERVIEW_ITEMS,
        );
        self.overview_attempt.set((
            batch_end as u32,
            checked.items.len(),
            rejected_total(&checked.rejected),
        ));
        if checked.items.is_empty() {
            // An empty answer must not replace a verified overview; the next update retries.
            anyhow::bail!("Die Kurzfassung konnte nicht belegt aktualisiert werden. Die bisherige Fassung bleibt sichtbar.");
        }
        let overview = LiveOverview {
            index: self.state.overviews.len() as u32 + 1,
            window: batch_end as u32,
            created_at: chrono::Utc::now().to_rfc3339(),
            items: checked.items,
            rejected: checked.rejected,
        };
        control.check()?;
        self.client.diagnostic_stage(RequestResult::Persistence);
        create_json(
            &store::overview_path(&self.folder, overview.index),
            &overview,
        )?;
        let index = overview.index;
        self.state.overviews.push(overview);
        Ok(Some(index))
    }

    pub fn view(&self) -> LiveView {
        let mut view = LiveView::from_run(&self.state, self.damaged_runs);
        view.last_request = diagnostics::latest(&self.folder);
        view
    }
}

/// The newest readable run of a meeting for display, without creating any model client.
pub fn read_latest(root: &Path, meeting_id: &str) -> Result<Option<LiveView>> {
    let (mut runs, mut damaged) = list_runs(root)?;
    runs.retain(|run| {
        let matches = run.run.meeting_id == meeting_id;
        if !matches {
            damaged += 1;
        }
        matches
    });
    Ok(runs
        .first()
        .map(|run| {
            let mut view = LiveView::from_run(run, damaged);
            view.last_request = diagnostics::latest(&root.join(&run.run.id));
            view
        })
        .or_else(|| (damaged > 0).then(|| LiveView::damaged_only(damaged))))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvidence {
    pub source_id: String,
    pub quote: String,
    pub track: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveItem {
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub window: u32,
    /// Audio tracks of the cited sections, derived from the evidence, never from the model.
    pub tracks: Vec<String>,
    pub evidence: Vec<LiveEvidence>,
}

/// Display state of one run. Rejected model text is only counted, never shown.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveView {
    pub run_id: Option<String>,
    pub model: String,
    pub mode: String,
    pub endpoint: String,
    pub language: String,
    pub prompt_version: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub windows: usize,
    pub summarized_sections: usize,
    pub overview: Vec<LiveItem>,
    pub overview_window: u32,
    pub overview_updated_at: Option<String>,
    pub overview_pending: bool,
    pub last_request: Option<LiveDiagnostic>,
    pub points: Vec<LiveItem>,
    pub decisions: Vec<LiveItem>,
    pub tasks: Vec<LiveItem>,
    pub questions: Vec<LiveItem>,
    pub rejected: usize,
    pub damaged_runs: usize,
}

impl LiveView {
    fn damaged_only(damaged_runs: usize) -> Self {
        Self {
            run_id: None,
            model: String::new(),
            mode: String::new(),
            endpoint: String::new(),
            language: String::new(),
            prompt_version: String::new(),
            created_at: None,
            updated_at: None,
            windows: 0,
            summarized_sections: 0,
            overview: Vec::new(),
            overview_window: 0,
            overview_updated_at: None,
            overview_pending: false,
            last_request: None,
            points: Vec::new(),
            decisions: Vec::new(),
            tasks: Vec::new(),
            questions: Vec::new(),
            rejected: 0,
            damaged_runs,
        }
    }

    fn from_run(state: &LoadedRun, damaged_runs: usize) -> Self {
        let sources: HashMap<&str, &LiveSource> =
            state.sources().map(|s| (s.id.as_str(), s)).collect();
        let item = |fact: &Fact, window: u32| {
            let evidence: Vec<LiveEvidence> = fact
                .evidence
                .iter()
                .map(|e| {
                    let source = sources.get(e.source_id.as_str());
                    LiveEvidence {
                        source_id: e.source_id.clone(),
                        quote: e.quote.clone(),
                        track: source.map(|s| s.source.clone()).unwrap_or_default(),
                        start_ms: source.map_or(0, |s| s.start_ms),
                        end_ms: source.map_or(0, |s| s.end_ms),
                    }
                })
                .collect();
            let mut tracks: Vec<String> = evidence.iter().map(|e| e.track.clone()).collect();
            tracks.sort();
            tracks.dedup();
            LiveItem {
                text: fact.text.clone(),
                owner: fact.owner.clone(),
                due: fact.due.clone(),
                window,
                tracks,
                evidence,
            }
        };
        let mut lists: [Vec<LiveItem>; 4] = Default::default();
        let mut rejected = 0_usize;
        for window in &state.windows {
            for (slot, (_, _, items)) in lists.iter_mut().zip(window.lists()) {
                slot.extend(items.iter().map(|f| item(f, window.index)));
            }
            rejected = rejected.saturating_add(rejected_total(&window.rejected));
        }
        let latest = state.overviews.last();
        let [points, decisions, tasks, questions] = lists;
        let updated_at = [
            state.windows.last().map(|w| w.created_at.clone()),
            latest.map(|o| o.created_at.clone()),
        ]
        .into_iter()
        .flatten()
        .max();
        Self {
            run_id: Some(state.run.id.clone()),
            model: state.run.model.clone(),
            mode: state.run.mode.clone(),
            endpoint: state.run.endpoint.clone(),
            language: state.run.language.clone(),
            prompt_version: state.run.prompt_version.clone(),
            created_at: Some(state.run.created_at.clone()),
            updated_at,
            windows: state.windows.len(),
            summarized_sections: state.sources().count(),
            overview: latest
                .map(|o| o.items.iter().map(|f| item(f, o.window)).collect())
                .unwrap_or_default(),
            overview_window: latest.map_or(0, |o| o.window),
            overview_updated_at: latest.map(|o| o.created_at.clone()),
            last_request: None,
            overview_pending: state.windows[latest.map_or(0, |o| o.window as usize)..]
                .iter()
                .any(|w| w.statement_count() > 0),
            points,
            decisions,
            tasks,
            questions,
            rejected,
            damaged_runs,
        }
    }
}

#[cfg(test)]
mod tests;
