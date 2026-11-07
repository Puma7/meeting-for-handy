//! Stable, source-backed workspace updates. Models receive only explicit text DTOs.
mod assertions;
mod context_store;
mod matching;
pub(crate) mod memory;
mod model;
mod store;
#[cfg(test)]
mod tests;
mod validate;

use crate::live::{self, LiveSource, LiveText};
use crate::{AnalysisControl, Source, TextClient};
use anyhow::{ensure, Context as _, Result};
pub use model::*;
use std::{
    collections::HashSet,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

pub const CONTRACT_VERSION: &str = "meeting-update-1";
pub const PROMPT_VERSION: &str = "handymeeting-workspace-13";
const V12_PROMPT_VERSION: &str = "handymeeting-workspace-12";
const V11_PROMPT_VERSION: &str = "handymeeting-workspace-11";
pub const PREVIOUS_PROMPT_VERSION: &str = "handymeeting-workspace-10";
pub const LEGACY_PROMPT_VERSION: &str = "handymeeting-workspace-9";
pub const WORKSPACE_FOLDER: &str = "workspace-v13";
pub const PREVIOUS_WORKSPACE_FOLDER: &str = "workspace-v10";
pub const LEGACY_WORKSPACE_FOLDER: &str = "workspace-v9";
/// Read shared-input chronology without treating model output or confirmations as new releases.
pub(crate) fn shared_context_timestamp(
    folder: &Path,
    context: &WorkspaceContext,
) -> Result<Option<chrono::DateTime<chrono::FixedOffset>>> {
    context_store::shared_timestamp(folder, context)
}
pub(crate) const GENERATIONS: [(&str, &str); 5] = [
    (PROMPT_VERSION, WORKSPACE_FOLDER),
    (V12_PROMPT_VERSION, "workspace-v12"),
    (V11_PROMPT_VERSION, "workspace-v11"),
    (PREVIOUS_PROMPT_VERSION, PREVIOUS_WORKSPACE_FOLDER),
    (LEGACY_PROMPT_VERSION, LEGACY_WORKSPACE_FOLDER),
];
const PROMPT: &str = include_str!("prompts/system.en.txt");
const PRESENTATION: &str = include_str!("prompts/presentation.en.txt");

pub fn supports_output_language(language: &str) -> bool {
    matches!(language, "auto" | "de" | "en" | "ru" | "zh")
}

/// Validate editable instructions and canonicalize the bundled default to an empty override.
pub fn normalize_prompt(text: &str) -> Result<&str> {
    let text = text.trim();
    ensure!(
        text.chars().count() <= 600
            && !text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Eigene Auswertungsanweisungen dürfen höchstens 600 Zeichen und keinen Steuertext enthalten. / Custom analysis instructions must be at most 600 characters with no control characters."
    );
    Ok(if text == PRESENTATION.trim() {
        ""
    } else {
        text
    })
}
const MAX_REQUEST_CHARS: usize = 24_000;
pub(crate) const WORKSPACE_CONTEXT_LENGTH: u64 = 8192;

pub struct WorkspaceRunner {
    folder: std::path::PathBuf,
    state: WorkspaceState,
    pub context: Arc<Mutex<WorkspaceContext>>,
    _writer: File,
    presentation: String,
    observed: std::cell::RefCell<Option<Vec<LiveSource>>>,
}

impl Drop for WorkspaceRunner {
    fn drop(&mut self) {
        // Explicitly end this owner's lease: an inherited/duplicated handle may
        // otherwise retain the OS lock after our File has been closed.
        let _ = fs2::FileExt::unlock(&self._writer);
    }
}

impl WorkspaceRunner {
    /// Acquire an isolated writer and restore model revisions plus the independent human-context journal.
    pub fn open(folder: &Path) -> Result<Self> {
        let (writer, state) = store::load(folder)?;
        let input = context_store::read(folder, &state.context)?;
        Ok(Self {
            folder: folder.into(),
            context: Arc::new(Mutex::new(input)),
            state,
            _writer: writer,
            presentation: String::new(),
            observed: Default::default(),
        })
    }

    pub(crate) fn set_presentation(&mut self, presentation: String) {
        self.presentation = presentation;
    }

    /// Import newer human confirmations on both fresh and resumed model runs,
    /// retaining destination history and immutable original evidence.
    pub fn inherit_manual(&mut self, previous: &WorkspaceState) -> Result<()> {
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?
            .clone();
        let mut next = self.state.clone();
        ensure!(
            context_store::project(&self.folder, &mut next)? == context,
            "Der Meetingkontext wurde inzwischen geändert. Aktuellen Stand verwenden."
        );
        let candidates: Vec<_> = previous
            .entities
            .iter()
            .filter(|entity| entity.human_locked)
            .filter(|entity| !previous.context_excluded_ids.contains(&entity.id))
            .filter(|entity| {
                !entity
                    .evidence
                    .iter()
                    .any(|evidence| next.superseded_sources.contains(&evidence.source_id))
            })
            .filter(|entity| {
                entity.kind != "agenda"
                    || context
                        .agenda
                        .iter()
                        .any(|a| a.id == entity.id && a.text == entity.text)
            })
            .cloned()
            .collect();
        let mut selected = HashSet::new();
        for mut entity in candidates {
            let incoming = previous.human_activity.get(&entity.id);
            let current = next.human_activity.get(&entity.id);
            if current.is_some_and(|at| incoming.is_none_or(|new| new <= at)) {
                continue;
            }
            if let Some(old) = next.entities.iter().find(|old| old.id == entity.id) {
                if incoming.is_none() && current.is_none() && old.human_locked {
                    let mut same_confirmation = entity.clone();
                    same_confirmation.revision = old.revision;
                    ensure!(old == &same_confirmation,
                        "Die Reihenfolge älterer menschlicher Änderungen ist unbekannt. Den gewünschten Stand im aktuellen Lauf neu bestätigen.");
                    continue;
                }
                entity.revision = entity.revision.max(
                    old.revision
                        .checked_add(1)
                        .context("Zu viele manuelle Eintragsfassungen.")?,
                );
            }
            if let Some(at) = incoming {
                next.human_activity.insert(entity.id.clone(), *at);
            }
            selected.insert(entity.id.clone());
            // Journal replay replaces existing IDs in place. Keep the same
            // order in memory; only genuinely new inherited IDs are appended.
            if let Some(slot) = next.entities.iter_mut().find(|old| old.id == entity.id) {
                *slot = entity;
            } else {
                next.entities.push(entity);
            }
        }
        if selected.is_empty() {
            return Ok(());
        }
        for event in previous
            .events
            .iter()
            .filter(|event| selected.contains(&event.entity_id))
        {
            if !next.events.iter().any(|old| old.id == event.id) {
                next.events.push(event.clone());
            }
        }
        let source_ids: HashSet<_> = next
            .entities
            .iter()
            .flat_map(|entity| entity.evidence.iter())
            .chain(next.events.iter().flat_map(|event| event.evidence.iter()))
            .map(|evidence| evidence.source_id.clone())
            .collect();
        let mut originals = Vec::new();
        for source in previous
            .originals
            .iter()
            .filter(|source| source_ids.contains(&source.id))
        {
            if let Some(old) = next.originals.iter().find(|old| old.id == source.id) {
                ensure!(
                    old == source,
                    "Originalbeleg hat eine widersprüchliche Identität."
                );
            } else {
                next.originals.push(source.clone());
                originals.push(source.clone());
            }
        }
        ensure!(
            source_ids
                .iter()
                .all(|id| next.originals.iter().any(|source| &source.id == id)),
            "Originalbelege der manuellen Änderungen fehlen."
        );
        for source in previous
            .processed
            .iter()
            .chain(&previous.inherited_sources)
            .filter(|source| source_ids.contains(&source_id(source)))
        {
            // Historical event evidence can refer to a withdrawn original.
            // Import its retirement as well, so matching text cannot revive its authority.
            let id = source_id(source);
            if previous.superseded_sources.contains(&id) && !next.superseded_sources.contains(&id) {
                next.superseded_sources.push(id);
            }
            if !next
                .processed
                .iter()
                .chain(&next.inherited_sources)
                .any(|old| source_id(old) == source_id(source))
            {
                next.inherited_sources.push(source.clone());
            }
        }
        next.context_updated_at = Some(chrono::Utc::now().fixed_offset());
        next.context = context.clone();
        self.commit(
            next,
            originals,
            vec![],
            &AnalysisControl::default(),
            &context,
        )
    }

    /// Detect latest transcript chunks not yet processed, including corrections and reversions.
    pub fn pending(&self, texts: &[LiveText]) -> Vec<LiveSource> {
        *self.observed.borrow_mut() = Some(live::pending(&[], texts));
        let latest = self.active_sources();
        let mut pending = live::pending(&latest, texts);
        // A reversion to an earlier wording is still a new current revision. Retain all
        // original IDs in the ledger while comparing only against the latest slice.
        let current = live::pending(&[], texts);
        for source in &current {
            if source.chunk == 1
                && latest.iter().filter(|s| s.key == source.key).count()
                    > current.iter().filter(|s| s.key == source.key).count()
                && !pending.iter().any(|s| s.key == source.key)
            {
                pending.push(source.clone());
            }
        }
        let mut used = self
            .state
            .processed
            .iter()
            .chain(&self.state.inherited_sources)
            .map(|s| s.id.clone())
            .collect::<HashSet<_>>();
        let mut assigned = HashSet::new();
        for source in &mut pending {
            // A new model still processes this text, but unchanged inherited
            // evidence retains its original identity for human-task deduplication.
            if let Some(inherited) = self.state.inherited_sources.iter().find(|old| {
                old.key == source.key
                    && old.chunk == source.chunk
                    && old.source == source.source
                    && old.start_ms == source.start_ms
                    && old.end_ms == source.end_ms
                    && old.text == source.text
                    && !assigned.contains(&old.id)
                    && !self.state.superseded_sources.contains(&source_id(old))
                    && !self
                        .state
                        .processed
                        .iter()
                        .any(|seen| seen.key == old.key && seen.chunk == old.chunk)
            }) {
                source.id.clone_from(&inherited.id);
                assigned.insert(source.id.clone());
                continue;
            }
            while used.contains(&source.id) {
                source.id.push('r');
            }
            used.insert(source.id.clone());
            assigned.insert(source.id.clone());
        }
        pending.sort_by(|a, b| {
            (a.start_ms, &a.source, &a.key, a.chunk).cmp(&(b.start_ms, &b.source, &b.key, b.chunk))
        });
        pending
    }

    /// Select the newest processed revision of each independent transcript chunk.
    fn active_sources(&self) -> Vec<LiveSource> {
        latest_sources(&self.state.processed)
            .into_iter()
            .filter(|s| !self.state.superseded_sources.contains(&source_id(s)))
            .collect()
    }
    /// Identify prior source authorities withdrawn by the current observed transcript.
    fn changed_source_ids(&self) -> HashSet<String> {
        self.observed
            .borrow()
            .as_ref()
            .map(|current| {
                self.state
                    .processed
                    .iter()
                    .chain(&self.state.inherited_sources)
                    .filter(|old| {
                        !self.state.superseded_sources.contains(&source_id(old))
                            && !current.iter().any(|new| {
                                new.key == old.key && new.chunk == old.chunk && new.text == old.text
                            })
                    })
                    .map(source_id)
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Project durable state together with the latest human context for the renderer.
    pub fn view(&self) -> WorkspaceState {
        let mut state = self.state.clone();
        let mut projected = state.clone();
        if context_store::project(&self.folder, &mut projected).is_ok() {
            state = projected;
        }
        state
    }
    /// Report whether shared inputs changed or an original-source replay remains unfinished.
    pub fn context_dirty(&self) -> bool {
        !self.changed_source_ids().is_empty()
            || self.state.replay_cursor.is_some()
            || self.context.lock().is_ok_and(|c| *c != self.state.context)
    }
    /// Return the isolated workspace persistence directory for controlled context transactions.
    pub fn folder(&self) -> std::path::PathBuf {
        self.folder.clone()
    }

    /// Initialize context for a new run without overwriting a newer native revision.
    pub fn set_context(&mut self, context: WorkspaceContext) -> Result<()> {
        context.validate()?;
        let mut current = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?;
        // A restart may supply an older browser cache. Never overwrite the native revision.
        if self.state.revision == 0 && current.revision == 0 {
            *current = context_store::update(&self.folder, &context)?.0;
        }
        Ok(())
    }

    /// Append current native shared inputs without importing another run's override lineage.
    pub(crate) fn restore_shared_context(&mut self, native: &WorkspaceContext) -> Result<()> {
        native.validate()?;
        let mut current = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?;
        if current.agenda == native.agenda && current.shared_notes == native.shared_notes {
            return Ok(());
        }
        let mut next = current.clone();
        next.agenda.clone_from(&native.agenda);
        next.shared_notes.clone_from(&native.shared_notes);
        *current = context_store::update(&self.folder, &next)?.0;
        Ok(())
    }

    /// One bounded source window and its whole active frontier are committed together.
    /// No publication occurs until the source quotes and context revision all pass.
    pub fn update(
        &mut self,
        client: &TextClient,
        pending: &[LiveSource],
        control: &AnalysisControl,
        language: &str,
    ) -> Result<usize> {
        control.begin_workspace_update();
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?
            .clone();
        let changed_inputs = context.agenda != self.state.context.agenda
            || context.shared_notes != self.state.context.shared_notes;
        let changed_sources = self.changed_source_ids();
        // Removing or shortening a source shifts the retained replay sequence.
        // Restart its bounded scan rather than reusing an index into the old prefix.
        // The restart is published only with a fully validated window below.
        let cursor = if !self.state.processed.is_empty()
            && (changed_inputs
                || (self.state.replay_cursor.is_some() && !changed_sources.is_empty()))
        {
            Some(0)
        } else {
            self.state.replay_cursor
        };
        let replay = pending.is_empty() && cursor.is_some();
        // New text has priority. A changed shared agenda/note is then reconciled against
        // every retained latest transcript slice in bounded windows, never summaries alone.
        let prior = self
            .active_sources()
            .into_iter()
            .filter(|s| !changed_sources.contains(&source_id(s)))
            .collect::<Vec<_>>();
        let input = if replay {
            &prior[cursor.unwrap().min(prior.len())..]
        } else {
            pending
        };
        let mut count = live::next_window(input).len();
        for depth in 0..=4 {
            match self.update_window(client, &input[..count], control, cursor, replay, language) {
                Err(error)
                    if crate::transport::is_budget_error(&error) && count > 1 && depth < 4 =>
                {
                    count = count.div_ceil(2)
                }
                result => return result,
            }
        }
        unreachable!()
    }
    /// Validate all phases against one bounded source window and commit only their complete transaction.
    fn update_window(
        &mut self,
        client: &TextClient,
        pending: &[LiveSource],
        control: &AnalysisControl,
        cursor: Option<usize>,
        replay: bool,
        language: &str,
    ) -> Result<usize> {
        control.check()?;
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?
            .clone();
        context.validate_request_budget()?;
        let mut next = self.state.clone();
        let durable = context_store::project(&self.folder, &mut next)?;
        ensure!(
            durable == context,
            "Der Meetingkontext wurde inzwischen geändert. Aktuellen Stand verwenden."
        );
        next.replay_cursor = cursor;
        let window = live::next_window(pending);
        // Preserve earlier originals and events while withdrawing their active authority
        // after a recognition correction, reversion or removed trailing paragraph.
        let old_ids = self.changed_source_ids();
        for id in &old_ids {
            if !next.superseded_sources.contains(id) {
                next.superseded_sources.push(id.clone());
            }
        }
        for e in &next.entities {
            if e.evidence.iter().any(|v| old_ids.contains(&v.source_id))
                && !next.context_excluded_ids.contains(&e.id)
            {
                next.context_excluded_ids.push(e.id.clone());
            }
        }
        // Agenda titles remain shared human input, but assessments grounded in a
        // superseded source must reopen before the reconciliation frontier is built.
        next.apply_context(&context)?;
        next.overview_refs
            .retain(|id| !next.context_excluded_ids.contains(id));
        if window.is_empty() {
            next.replay_cursor = None;
            if next.context != self.state.context
                || next.replay_cursor != self.state.replay_cursor
                || next.superseded_sources != self.state.superseded_sources
            {
                self.commit(next, vec![], vec![], control, &context)?;
            }
            return Ok(0);
        }
        let mut originals = next.originals.clone();
        let mut fresh = Vec::new();
        let mut required = Vec::new();
        for s in &window {
            let id = source_id(s);
            let source = Source {
                id,
                source: s.source.clone(),
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: s.text.clone(),
            };
            required.push(source.clone());
            if let Some(old) = originals.iter().find(|old| old.id == source.id) {
                ensure!(
                    old == &source,
                    "Quellenkennung kollidiert; keine Änderung übernommen."
                );
            } else {
                originals.push(source.clone());
                fresh.push(source);
            }
        }
        // Active questions are never discarded for an input cap. Process the entire frontier
        // in bounded groups, while the durable state retains every older object.
        let mut committed_changes = Vec::new();
        // One coordinated transaction with two small tasks. A small local model should not
        // have to classify obligations and generate/resolve questions in the same output.
        let mut overview_selection: Option<Vec<String>> = None;
        let incoming_overview: HashSet<_> = next.overview_refs.iter().cloned().collect();
        for phase in ["tasks", "facts", "reconcile"] {
            let frontier: Vec<Entity> = next
                .entities
                .iter()
                .filter(|e| !next.context_excluded_ids.contains(&e.id))
                .filter(|e| match phase {
                    // Terminal identities relevant to this window are read-only
                    // deduplication context, including their completion evidence.
                    "tasks" => {
                        e.kind == "task"
                            && (e.status == "open"
                                || e.evidence.iter().any(|evidence| {
                                    required.iter().any(|s| s.id == evidence.source_id)
                                }))
                    }
                    "facts" => {
                        (e.kind == "fact" && incoming_overview.contains(&e.id))
                            || (e.kind == "decision" && e.status != "withdrawn")
                    }
                    _ => {
                        e.kind == "impulse"
                            || (e.kind == "agenda" && context.agenda.iter().any(|a| a.id == e.id))
                    }
                })
                .cloned()
                .collect();
            let mut groups: std::collections::VecDeque<(Vec<Entity>, bool)> = if frontier.is_empty()
            {
                vec![(vec![], true)].into()
            } else {
                frontier.chunks(6).map(|g| (g.to_vec(), true)).collect()
            };
            'groups: while let Some((mut group, seeds)) = groups.pop_front() {
                // A preceding group's optional context may have updated one of
                // these queued IDs. Requests must use its current revision.
                for entity in &mut group {
                    *entity = next
                        .entities
                        .iter()
                        .find(|current| current.id == entity.id)
                        .context("Ein Eintrag der Abgleichsfront fehlt.")?
                        .clone();
                }
                group.retain(|entity| !next.context_excluded_ids.contains(&entity.id));
                let base_group = group.clone();
                control.check()?;
                if ["facts", "reconcile"].contains(&phase) && seeds {
                    group.extend(
                        next.entities
                            .iter()
                            .rev()
                            .filter(|e| !next.context_excluded_ids.contains(&e.id))
                            .filter(|e| ["fact", "task", "decision"].contains(&e.kind.as_str()))
                            .take(6)
                            .cloned(),
                    );
                }
                let mut seen_ids = HashSet::new();
                group.retain(|e| seen_ids.insert(e.id.clone()));
                let mut source_ids: HashSet<String> =
                    required.iter().map(|s| s.id.clone()).collect();
                for e in &group {
                    source_ids.extend(e.evidence.iter().map(|v| v.source_id.clone()));
                }
                let selected: Vec<Source> = originals
                    .iter()
                    .filter(|s| source_ids.contains(&s.id))
                    .cloned()
                    .collect();
                ensure!(selected.len() == source_ids.len(), "Originalbelege des bisherigen Stands fehlen; gespeicherte Daten bleiben erhalten.");
                let mut req = Request::new(
                    format!("r_{}", uuid::Uuid::new_v4().simple()),
                    &context,
                    &next,
                    group,
                    &required,
                    selected,
                );
                req.purpose = phase.into();
                req.language = language.into();
                req.presentation_instructions = if self.presentation.is_empty() {
                    PRESENTATION.trim().into()
                } else {
                    self.presentation.clone()
                };
                req.compact_titles();
                let input = serde_json::to_string(&req)?;
                if input.chars().count() > MAX_REQUEST_CHARS {
                    if ["facts", "reconcile"].contains(&phase) && seeds {
                        groups.push_front((base_group, false));
                        continue;
                    }
                    if base_group.len() > 1 {
                        let at = base_group.len() / 2;
                        groups.push_front((base_group[at..].to_vec(), false));
                        groups.push_front((base_group[..at].to_vec(), false));
                        continue;
                    }
                    return Err(crate::transport::BudgetError::Context.into());
                }
                let mut schema: serde_json::Value =
                    serde_json::from_str(include_str!("response.schema.json"))?;
                schema["properties"]["changes"]["items"]["properties"]["text"]["maxLength"] =
                    req.max_title_chars().into();
                let evidence_options = req.transcript_sources.iter().filter_map(|source| {
                    let quotes = quote_options(&source.text);
                    (!quotes.is_empty()).then(|| serde_json::json!({
                        "type":"object", "additionalProperties":false, "required":["sourceId","quote"],
                        "properties":{"sourceId":{"type":"string","enum":[source.id]},"quote":{"type":"string","enum":quotes}}
                    }))
                }).collect::<Vec<_>>();
                if evidence_options.is_empty() {
                    continue;
                }
                schema["properties"]["changes"]["items"]["properties"]["evidence"]["items"] =
                    serde_json::json!({"anyOf":evidence_options});
                schema["properties"]["changes"]["items"]["properties"]["relatedEntityIds"]
                    ["maxItems"] = 0.into();
                let extra = if phase == "tasks" {
                    let base = schema["properties"]["changes"]["items"].clone();
                    let mut branches = Vec::new();
                    let task_evidence = fresh_assertion_schema(&req, validate::task_quote_allowed);
                    if !task_evidence.is_empty() {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!(["task"]);
                        branch["properties"]["targetId"] = serde_json::json!({"type":"null"});
                        branch["properties"]["targetRevision"] = serde_json::json!({"type":"null"});
                        branch["properties"]["status"]["enum"] = serde_json::json!(["open"]);
                        branch["properties"]["evidence"]["items"] =
                            serde_json::json!({"anyOf":task_evidence});
                        branches.push(branch);
                    }
                    for e in req
                        .state
                        .iter()
                        .filter(|e| e.kind == "task" && e.status == "open" && !e.human_locked)
                    {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!(["task"]);
                        branch["properties"]["targetId"] =
                            serde_json::json!({"type":"string","enum":[e.id]});
                        branch["properties"]["targetRevision"] =
                            serde_json::json!({"type":"integer","enum":[e.revision]});
                        for (status, allowed_quote) in [
                            ("open", validate::task_quote_allowed as fn(&str) -> bool),
                            ("completed", validate::completion_quote_allowed),
                            ("withdrawn", validate::withdrawal_quote_allowed),
                        ] {
                            let evidence = fresh_quote_schema(&req, |quote| {
                                allowed_quote(quote) && matching::task_subject_matches(e, quote)
                            });
                            if evidence.is_empty() {
                                continue;
                            }
                            let mut status_branch = branch.clone();
                            status_branch["properties"]["status"]["enum"] =
                                serde_json::json!([status]);
                            status_branch["properties"]["evidence"]["items"] =
                                serde_json::json!({"anyOf": evidence});
                            if status != "open" {
                                status_branch["properties"]["text"]["type"] = "null".into();
                            }
                            branches.push(status_branch);
                        }
                    }
                    schema["properties"]["changes"]["items"] =
                        serde_json::json!({"anyOf":branches});
                    if branches.is_empty() {
                        continue;
                    }
                    include_str!("prompts/tasks.en.txt").trim_end()
                } else if phase == "facts" {
                    let base = schema["properties"]["changes"]["items"].clone();
                    let mut variants = Vec::new();
                    for kind in ["fact", "decision"] {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!([kind]);
                        branch["properties"]["targetId"] = serde_json::json!({"type":"null"});
                        branch["properties"]["targetRevision"] = serde_json::json!({"type":"null"});
                        branch["properties"]["text"]["type"] = "string".into();
                        branch["properties"]["status"]["enum"] =
                            serde_json::json!(validate::allowed_statuses(kind)
                                .iter()
                                .filter(|status| !["completed", "withdrawn", "discussed"]
                                    .contains(status))
                                .collect::<Vec<_>>());
                        for field in ["owner", "due"] {
                            branch["properties"][field] = serde_json::json!({"type":"null"});
                        }
                        if kind == "decision" {
                            let evidence = fresh_assertion_schema(
                                &req,
                                validate::recorded_decision_quote_allowed,
                            );
                            if evidence.is_empty() {
                                continue;
                            }
                            branch["properties"]["evidence"]["items"] =
                                serde_json::json!({"anyOf": evidence});
                        }
                        variants.push(branch);
                    }
                    // Tasks may be present as overview context but cannot be retagged as facts.
                    for entity in req.state.iter().filter(|entity| {
                        !entity.human_locked
                            && ["fact", "decision"].contains(&entity.kind.as_str())
                            && !(entity.kind == "decision" && entity.status == "withdrawn")
                    }) {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!([entity.kind]);
                        branch["properties"]["targetId"] =
                            serde_json::json!({"type":"string","enum":[entity.id]});
                        branch["properties"]["targetRevision"] =
                            serde_json::json!({"type":"integer","enum":[entity.revision]});
                        branch["properties"]["status"]["enum"] =
                            serde_json::json!(validate::allowed_statuses(&entity.kind)
                                .iter()
                                .filter(|status| **status != "withdrawn")
                                .collect::<Vec<_>>());
                        for field in ["owner", "due"] {
                            branch["properties"][field] = serde_json::json!({"type":"null"});
                        }
                        if entity.kind == "decision" {
                            let evidence = fresh_quote_schema(&req, |quote| {
                                validate::withdrawal_quote_allowed(quote)
                                    && matching::shares_topic(&entity.text, quote)
                            });
                            if !evidence.is_empty() {
                                let mut withdrawal = branch.clone();
                                withdrawal["properties"]["status"]["enum"] =
                                    serde_json::json!(["withdrawn"]);
                                withdrawal["properties"]["text"]["type"] = "null".into();
                                withdrawal["properties"]["evidence"]["items"] =
                                    serde_json::json!({"anyOf": evidence});
                                variants.push(withdrawal);
                            }
                            let evidence = fresh_assertion_schema(&req, |quote| {
                                validate::recorded_decision_quote_allowed(quote)
                                    && matching::shares_topic(&entity.text, quote)
                            });
                            if evidence.is_empty() {
                                continue;
                            }
                            branch["properties"]["evidence"]["items"] =
                                serde_json::json!({"anyOf": evidence});
                        }
                        variants.push(branch);
                    }
                    schema["properties"]["changes"]["items"] =
                        serde_json::json!({"anyOf":variants});
                    include_str!("prompts/facts.en.txt").trim_end()
                } else {
                    let base = schema["properties"]["changes"]["items"].clone();
                    let mut variants = Vec::new();
                    for e in req.state.iter().filter(|e| {
                        !e.human_locked && ["agenda", "impulse"].contains(&e.kind.as_str())
                    }) {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!([e.kind]);
                        branch["properties"]["targetId"] =
                            serde_json::json!({"type":"string","enum":[e.id]});
                        branch["properties"]["targetRevision"] =
                            serde_json::json!({"type":"integer","enum":[e.revision]});
                        for field in ["text", "owner", "due"] {
                            branch["properties"][field] = serde_json::json!({"type":"null"});
                        }
                        branch["properties"]["status"]["enum"] = if e.kind == "agenda" {
                            serde_json::json!(["touched", "discussed", "unclear"])
                        } else {
                            let evidence = fresh_quote_schema(&req, |quote| {
                                validate::impulse_answer_allowed(&e.text, quote)
                            });
                            if !evidence.is_empty() {
                                let mut discussed = branch.clone();
                                discussed["properties"]["status"]["enum"] =
                                    serde_json::json!(["discussed"]);
                                discussed["properties"]["evidence"]["items"] =
                                    serde_json::json!({"anyOf": evidence});
                                variants.push(discussed);
                            }
                            if req.language == "auto" {
                                let evidence = fresh_quote_schema(&req, |quote| {
                                    validate::question_topic_matches(&e.text, quote)
                                });
                                if evidence.is_empty() {
                                    continue;
                                }
                                branch["properties"]["evidence"]["items"] =
                                    serde_json::json!({"anyOf": evidence});
                            }
                            serde_json::json!(["unclear"])
                        };
                        variants.push(branch);
                    }
                    let mut new_impulse = base;
                    new_impulse["properties"]["kind"]["enum"] = serde_json::json!(["impulse"]);
                    new_impulse["properties"]["status"]["enum"] = serde_json::json!(["open"]);
                    new_impulse["properties"]["text"]["type"] = "string".into();
                    for field in ["targetId", "targetRevision", "owner", "due"] {
                        new_impulse["properties"][field] = serde_json::json!({"type":"null"});
                    }
                    let mut fresh = fresh_quote_schema(&req, validate::explicit_question);
                    if fresh.is_empty() {
                        fresh = fresh_quote_schema(&req, validate::new_impulse_evidence_allowed);
                    }
                    if !fresh.is_empty() {
                        new_impulse["properties"]["evidence"]["items"] =
                            serde_json::json!({"anyOf": &fresh});
                    }
                    // A small model can otherwise satisfy this phase by touching the agenda
                    // and omit the already acknowledged open question. Give that question
                    // one constrained request before the ordinary agenda/status reconciliation.
                    let needs_question = validate::initial_question_needed(&next, &req);
                    if needs_question {
                        variants.clear();
                        schema["properties"]["changes"]["minItems"] = 1.into();
                        let witnesses = validate::initial_question_evidence(&next, &req).into_iter().map(|evidence| serde_json::json!({
                            "type":"object", "additionalProperties":false, "required":["sourceId","quote"],
                            "properties":{"sourceId":{"type":"string","enum":[evidence.source_id]},"quote":{"type":"string","enum":[evidence.quote]}}
                        })).collect::<Vec<_>>();
                        new_impulse["properties"]["evidence"]["items"] =
                            serde_json::json!({"anyOf":witnesses});
                    }
                    if needs_question || !fresh.is_empty() {
                        variants.push(new_impulse);
                    }
                    if variants.is_empty() {
                        schema["properties"]["changes"]["maxItems"] = 0.into();
                    } else {
                        schema["properties"]["changes"]["items"] =
                            serde_json::json!({"anyOf":variants});
                    }
                    schema["properties"]["overviewRefs"]["maxItems"] = 0.into();
                    include_str!("prompts/reconcile.en.txt").trim_end()
                };
                let question_only =
                    phase == "reconcile" && schema["properties"]["changes"]["minItems"] == 1;
                for attempt in 0..2 {
                    let repair = if attempt == 0 {
                        ""
                    } else {
                        "\nPrevious answer rejected. Retry the entire phase. Use its allowed kinds only, unique proposalIds, exact state IDs and revisions, literal source quotes and relatedEntityIds=[]. New tasks require a fresh explicit commitment."
                    };
                    let answer = match client.workspace_json(
                        &format!("{PROMPT}\n{extra}{repair}"),
                        &input,
                        schema.clone(),
                        control,
                    ) {
                        Ok(answer) => answer,
                        Err(error)
                            if matches!(
                                error.downcast_ref::<crate::transport::BudgetError>(),
                                Some(crate::transport::BudgetError::Context)
                            ) =>
                        {
                            if seeds && ["facts", "reconcile"].contains(&phase) {
                                groups.push_front((base_group, false));
                                continue 'groups;
                            }
                            if base_group.len() > 1 {
                                let at = base_group.len() / 2;
                                groups.push_front((base_group[at..].to_vec(), false));
                                groups.push_front((base_group[..at].to_vec(), false));
                                continue 'groups;
                            }
                            return Err(error);
                        }
                        Err(error) => return Err(error),
                    };
                    match validate::apply(&mut next, &req, &answer) {
                        Ok(events) => {
                            ensure!(!question_only || next.entities.iter().any(|entity|
                                entity.kind == "impulse" && !next.context_excluded_ids.contains(&entity.id)),
                                "Der angeforderte Frageimpuls fehlt; keine Änderung wurde veröffentlicht.");
                            if phase == "facts" {
                                let selected = overview_selection.get_or_insert_with(Vec::new);
                                for id in &next.overview_refs {
                                    if !selected.contains(id) {
                                        selected.push(id.clone());
                                    }
                                }
                            }
                            committed_changes.extend(events);
                            if question_only {
                                let mut followup = base_group.clone();
                                followup.extend(
                                    next.entities
                                        .iter()
                                        .filter(|entity| {
                                            entity.kind == "impulse"
                                                && !next.context_excluded_ids.contains(&entity.id)
                                        })
                                        .cloned(),
                                );
                                groups.push_front((followup, true));
                            }
                            break;
                        }
                        Err(error)
                            if attempt == 0
                                && !crate::transport::is_budget_error(&error)
                                && !crate::is_cancelled(&error) =>
                        {
                            control.record_validation_retry();
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        if let Some(selected) = overview_selection {
            next.overview_refs = selected;
        } else if next.overview_refs.is_empty() {
            next.overview_refs = next
                .entities
                .iter()
                .rev()
                .filter(|e| !next.context_excluded_ids.contains(&e.id))
                .filter(|e| {
                    ["fact", "task", "decision"].contains(&e.kind.as_str())
                        && !e.evidence.is_empty()
                })
                .take(8)
                .map(|e| e.id.clone())
                .collect();
            next.overview_refs.reverse();
        }
        next.originals = originals;
        if replay {
            let at = cursor.unwrap_or(0) + window.len();
            next.replay_cursor = (at
                < latest_sources(&next.processed)
                    .iter()
                    .filter(|s| !next.superseded_sources.contains(&source_id(s)))
                    .count())
            .then_some(at);
        } else {
            next.processed.extend(window.clone());
        }
        next.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self.commit(next, fresh, committed_changes, control, &context)?;
        Ok(if replay { 0 } else { window.len() })
    }

    /// Recheck both native context versions under the short disk lease before durable publication.
    fn commit(
        &mut self,
        mut next: WorkspaceState,
        originals: Vec<Source>,
        events: Vec<Event>,
        control: &AnalysisControl,
        expected: &WorkspaceContext,
    ) -> Result<()> {
        // Hold the small context lock only across the final durable commit, never inference.
        let current = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?;
        let shared_same = |context: &WorkspaceContext| {
            context.revision >= expected.revision
                && context.agenda == expected.agenda
                && context.shared_notes == expected.shared_notes
                && context.overrides.starts_with(&expected.overrides)
        };
        ensure!(shared_same(&current), "Der Meetingkontext wurde während der Auswertung geändert. Der neue Stand wird erneut abgeglichen.");
        let _disk_guard = context_store::lock(&self.folder)?;
        let durable = context_store::human_suffix(&self.folder, expected)?;
        ensure!(durable == *current && shared_same(&durable), "Der Meetingkontext wurde in einem anderen Fenster geändert. Der aktuelle Stand bleibt erhalten.");
        next.apply_context(&durable)?;
        control.check()?;
        next.revision = self.state.revision + 1;
        let delta = store::Delta::between(&self.state, &next, originals, events);
        store::append(&self.folder, &delta).context(
            "Meetingstand konnte nicht gesichert werden. Der letzte gültige Stand bleibt erhalten.",
        )?;
        self.state = next;
        Ok(())
    }
}

/// Bind an original identifier to exact recognized text as well as chunk identity.
fn source_id(source: &LiveSource) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{}\0{}", source.id, source.text).as_bytes());
    format!("s_{}", &format!("{digest:x}")[..16])
}

/// Offer bounded literal quote alternatives from the actual original source.
fn quote_options(text: &str) -> Vec<String> {
    let mut options = Vec::new();
    let mut add = |s: &str| {
        let s = s.trim();
        if (8..=600).contains(&s.chars().count()) && !options.iter().any(|old| old == s) {
            options.push(s.to_owned());
        }
    };
    add(text);
    for sentence in evidence_sentences(text) {
        add(sentence);
    }
    let chars = text.chars().collect::<Vec<_>>();
    for start in (0..chars.len()).step_by(300) {
        let part = chars[start..(start + 500).min(chars.len())]
            .iter()
            .collect::<String>();
        add(&part);
    }
    options
}

/// Keep decimal points and spoken ordinal dates inside their literal sentence.
fn evidence_sentences(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    text.char_indices()
        .chain(std::iter::once((text.len(), '\n')))
        .filter_map(move |(index, ch)| {
            if !matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
                return None;
            }
            let end = (index + ch.len_utf8()).min(text.len());
            if ch == '.'
                && text[..index].ends_with(|c: char| c.is_ascii_digit())
                && (text[end..].starts_with(|c: char| c.is_ascii_digit())
                    || crate::validation::starts_named_month(&text[end..]))
            {
                return None;
            }
            let sentence = text[start..end].trim();
            start = end;
            (!sentence.is_empty()).then_some(sentence)
        })
}

fn fresh_assertion_schema(
    request: &Request,
    allowed: impl Fn(&str) -> bool,
) -> Vec<serde_json::Value> {
    fresh_evidence_schema(request, |text| {
        evidence_sentences(text)
            .filter(|quote| (8..=600).contains(&quote.chars().count()) && allowed(quote))
            .map(str::to_owned)
            .collect()
    })
}

/// Limit a structured status change to witnesses from the current window.
fn fresh_quote_schema(request: &Request, allowed: impl Fn(&str) -> bool) -> Vec<serde_json::Value> {
    fresh_evidence_schema(request, |text| {
        quote_options(text)
            .into_iter()
            .filter(|quote| allowed(quote))
            .collect()
    })
}

fn fresh_evidence_schema(
    request: &Request,
    quotes_for: impl Fn(&str) -> Vec<String>,
) -> Vec<serde_json::Value> {
    request
        .transcript_sources
        .iter()
        .filter(|source| request.required_source_ids().contains(&source.id))
        .filter_map(|source| {
            let quotes = quotes_for(&source.text);
            (!quotes.is_empty()).then(|| serde_json::json!({
                "type": "object", "additionalProperties": false, "required": ["sourceId", "quote"],
                "properties": {
                    "sourceId": {"type": "string", "enum": [source.id]},
                    "quote": {"type": "string", "enum": quotes},
                },
            }))
        })
        .collect()
}

/// Loads only immutable complete revisions; no client, writer lock or directory creation.
pub fn read_state(folder: &Path) -> Result<WorkspaceState> {
    let mut state = store::read(folder)?;
    context_store::project(folder, &mut state)?;
    Ok(state)
}
/// Explicit human input update. Caller binds this folder to its admitted meeting/run.
pub fn update_context(
    folder: &Path,
    context: &WorkspaceContext,
) -> Result<(WorkspaceContext, WorkspaceState)> {
    let state = store::read(folder)?;
    ensure!(state.prompt_version() == PROMPT_VERSION,
        "Diese Auswertung verwendet ältere Prompts und bleibt unverändert lesbar. Eine neue lokale Auswertung starten, um mit der aktuellen Fassung weiterzuarbeiten.");
    context_store::update(folder, context)
}

/// Keep the last processed revision independently for each transcript key and chunk.
fn latest_sources(processed: &[LiveSource]) -> Vec<LiveSource> {
    let mut latest = std::collections::BTreeMap::new();
    for source in processed {
        latest.insert((&source.key, source.chunk), source.clone());
    }
    let mut sources = latest.into_values().collect::<Vec<_>>();
    sources.sort_by(|a, b| {
        (a.start_ms, &a.source, &a.key, a.chunk).cmp(&(b.start_ms, &b.source, &b.key, b.chunk))
    });
    sources
}
