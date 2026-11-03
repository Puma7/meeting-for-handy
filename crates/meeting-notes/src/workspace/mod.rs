//! Stable, source-backed workspace updates. Models receive only explicit text DTOs.
mod context_store;
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
pub const PROMPT_VERSION: &str = "handymeeting-workspace-9";
const PROMPT: &str = include_str!("system.de.txt");
const MAX_REQUEST_CHARS: usize = 24_000;

pub struct WorkspaceRunner {
    folder: std::path::PathBuf,
    state: WorkspaceState,
    pub context: Arc<Mutex<WorkspaceContext>>,
    _writer: File,
    observed: std::cell::RefCell<Option<Vec<LiveSource>>>,
}

impl WorkspaceRunner {
    pub fn open(folder: &Path) -> Result<Self> {
        let (writer, state) = store::load(folder)?;
        let input = context_store::read(folder, &state.context)?;
        Ok(Self {
            folder: folder.into(),
            context: Arc::new(Mutex::new(input)),
            state,
            _writer: writer,
            observed: Default::default(),
        })
    }

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
            .map(|s| s.id.clone())
            .collect::<HashSet<_>>();
        for source in &mut pending {
            while used.contains(&source.id) {
                source.id.push('r');
            }
            used.insert(source.id.clone());
        }
        pending.sort_by(|a, b| {
            (a.start_ms, &a.source, &a.key, a.chunk).cmp(&(b.start_ms, &b.source, &b.key, b.chunk))
        });
        pending
    }

    fn active_sources(&self) -> Vec<LiveSource> {
        latest_sources(&self.state.processed)
            .into_iter()
            .filter(|s| !self.state.superseded_sources.contains(&source_id(s)))
            .collect()
    }
    fn changed_source_ids(&self) -> HashSet<String> {
        self.observed
            .borrow()
            .as_ref()
            .map(|current| {
                self.state
                    .processed
                    .iter()
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
    pub fn view(&self) -> WorkspaceState {
        let mut state = self.state.clone();
        if let Ok(context) = self.context.lock() {
            let _ = state.apply_context(&context);
        }
        state
    }
    pub fn context_dirty(&self) -> bool {
        !self.changed_source_ids().is_empty()
            || self.state.replay_cursor.is_some()
            || self.context.lock().is_ok_and(|c| *c != self.state.context)
    }
    pub fn folder(&self) -> std::path::PathBuf {
        self.folder.clone()
    }

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

    /// One bounded source window and its whole active frontier are committed together.
    /// No publication occurs until the source quotes and context revision all pass.
    pub fn update(
        &mut self,
        client: &TextClient,
        pending: &[LiveSource],
        control: &AnalysisControl,
    ) -> Result<usize> {
        control.begin_workspace_update();
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?
            .clone();
        let changed_inputs = context.agenda != self.state.context.agenda
            || context.shared_notes != self.state.context.shared_notes;
        let cursor = if changed_inputs && !self.state.processed.is_empty() {
            Some(0)
        } else {
            self.state.replay_cursor
        };
        let replay = pending.is_empty() && cursor.is_some();
        // New text has priority. A changed shared agenda/note is then reconciled against
        // every retained latest transcript slice in bounded windows, never summaries alone.
        let changed_sources = self.changed_source_ids();
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
            match self.update_window(client, &input[..count], control, cursor, replay) {
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
    fn update_window(
        &mut self,
        client: &TextClient,
        pending: &[LiveSource],
        control: &AnalysisControl,
        cursor: Option<usize>,
        replay: bool,
    ) -> Result<usize> {
        control.check()?;
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Meetingkontext ist gesperrt."))?
            .clone();
        context.validate()?;
        let mut next = self.state.clone();
        next.apply_context(&context)?;
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
        for phase in ["tasks", "facts", "reconcile"] {
            let frontier: Vec<Entity> = next
                .entities
                .iter()
                .filter(|e| !next.context_excluded_ids.contains(&e.id))
                .filter(|e| match phase {
                    "tasks" => e.kind == "task" && e.status == "open",
                    "facts" => {
                        (e.kind == "fact" && next.overview_refs.contains(&e.id))
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
            while let Some((mut group, seeds)) = groups.pop_front() {
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
                let evidence_options = req.transcript_sources.iter().map(|source| serde_json::json!({
                    "type":"object", "additionalProperties":false, "required":["sourceId","quote"],
                    "properties":{"sourceId":{"type":"string","enum":[source.id]},"quote":{"type":"string","enum":quote_options(&source.text)}}
                })).collect::<Vec<_>>();
                schema["properties"]["changes"]["items"]["properties"]["evidence"]["items"] =
                    serde_json::json!({"anyOf":evidence_options});
                schema["properties"]["changes"]["items"]["properties"]["relatedEntityIds"]
                    ["maxItems"] = 0.into();
                let extra = if phase == "tasks" {
                    let base = schema["properties"]["changes"]["items"].clone();
                    let mut branches = Vec::new();
                    let task_evidence=req.transcript_sources.iter().filter(|source|required.iter().any(|s|s.id==source.id)).filter_map(|source| {
                        let quotes=quote_options(&source.text).into_iter().filter(|q|validate::task_quote_allowed(q)).collect::<Vec<_>>();
                        (!quotes.is_empty()).then(||serde_json::json!({"type":"object","additionalProperties":false,"required":["sourceId","quote"],"properties":{"sourceId":{"type":"string","enum":[source.id]},"quote":{"type":"string","enum":quotes}}}))
                    }).collect::<Vec<_>>();
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
                        .filter(|e| e.kind == "task" && !e.human_locked)
                    {
                        let mut branch = base.clone();
                        branch["properties"]["kind"]["enum"] = serde_json::json!(["task"]);
                        branch["properties"]["targetId"] =
                            serde_json::json!({"type":"string","enum":[e.id]});
                        branch["properties"]["targetRevision"] =
                            serde_json::json!({"type":"integer","enum":[e.revision]});
                        branches.push(branch);
                    }
                    schema["properties"]["changes"]["items"] =
                        serde_json::json!({"anyOf":branches});
                    if branches.is_empty() {
                        continue;
                    }
                    "PHASE AUFGABEN: Antworte ausschließlich mit kind=task. Suche jede ausdrückliche Übernahme oder Zuweisung in requiredSourceIds. Ich schreibe den Bericht bis Freitag muss zwingend als task/open erscheinen, niemals nur als Fakt. Neue Aufgaben benötigen einen ausdrücklichen Übernahmebeleg. Fehlende Zusagen, offene Freigaben, Fragen, Wünsche und unverbindliche Vorschläge sind KEINE Aufgaben. Owner und due nur wörtlich belegt, sonst null. Status completed nur für eine vorhandene passende Aufgabe mit ausdrücklichem Erledigungsbeleg. Verändere keine menschlich geschützten Aufgaben. Keine Fakten, Entscheidungen, Agenda oder Impulse in dieser Phase. Wähle die Aufgaben für overviewRefs."
                } else if phase == "facts" {
                    schema["properties"]["changes"]["items"]["properties"]["kind"]["enum"] =
                        serde_json::json!(["fact", "decision"]);
                    for field in ["owner", "due"] {
                        schema["properties"]["changes"]["items"]["properties"][field] =
                            serde_json::json!({"type":"null"});
                    }
                    "PHASE FAKTEN UND BESCHLÜSSE: Nur kind=fact oder decision. Bereits in state enthaltene Aufgaben nicht als allgemeinen Fakt wiederholen. fact/recorded oder unclear für wichtige weitere Beobachtungen. Eine fehlende Budgetzusage ist eine offene Tatsache, keine Aufgabe. decision/recorded ausschließlich für einen ausdrücklichen Beschluss; withdrawn nur ausdrücklich belegte Rücknahme einer vorhandenen Entscheidung. Owner/due immer null. Keine Aufgaben, Impulse oder Agenda. overviewRefs wählt bis zu 8 wichtige vorhandene Aufgaben aus state sowie Fakten/Beschlüsse; neue Einträge über proposalId referenzieren."
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
                            serde_json::json!(["discussed", "unclear"])
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
                    variants.push(new_impulse);
                    schema["properties"]["changes"]["items"] =
                        serde_json::json!({"anyOf":variants});
                    schema["properties"]["overviewRefs"]["maxItems"] = 0.into();
                    "PHASE IMPULSABGLEICH: Ausschließlich kind=impulse oder agenda. KEINE task, KEINE fact, KEINE decision, auch wenn sie in state stehen. Neue Fragen immer kind=impulse, status=open, attention=clarification, owner/due null, targetId/targetRevision null. Zu einer ausdrücklich fehlenden Antwort oder Zusage MUSS mindestens der wichtigste neue konkrete Frageimpuls ergänzt werden, sofern kein gleichbedeutender vorhanden ist: fehlende Budgetzusage ergibt die Frage Wie wird das Budget freigegeben. Ein Fakt über eine fehlende Zusage oder ein touched-Agendaeintrag ersetzt diesen Frageimpuls nicht. Keine Aufgabe daraus machen. Neue Impulse benötigen einen wörtlichen Beleg für das offene Thema. Vorhandene Impulse behalten; nur eine passende ausdrückliche Antwort darf ihren Status zu discussed ändern, Text bleibt null. Eine noch offene Freigabe ist keine erfolgte Freigabe. Agenda nur vorhandene id/revision: touched bei angerissenem Thema, discussed nur bei ausreichender inhaltlicher Antwort, unclear bei unklarem Bezug. Agendatext nicht ändern. Menschlich geschützte Einträge nicht ändern. overviewRefs bleibt in dieser Phase leer."
                };
                for attempt in 0..2 {
                    let repair = if attempt == 0 {
                        ""
                    } else {
                        "\nDie vorherige Antwort wurde vollständig verworfen. Erzeuge den Auftrag erneut: ausschließlich Typen dieser Phase; jede proposalId innerhalb der Antwort genau einmal (p_1, p_2, ...); targetId/revision nur aus state oder null; neue Aufgaben nur aus window.requiredSourceIds; alle Belege wörtlich aus transcriptSources; relatedEntityIds immer []; keine fehlende Zusage als Aufgabe. Keine zuvor ungültige Antwort wird übernommen."
                    };
                    let answer = client.workspace_json(
                        &format!("{PROMPT}\n{extra}{repair}"),
                        &input,
                        schema.clone(),
                        control,
                    )?;
                    match validate::apply(&mut next, &req, &answer) {
                        Ok(events) => {
                            committed_changes.extend(events);
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
        if next.overview_refs.is_empty() {
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
        ensure!(*current == *expected, "Der Meetingkontext wurde während der Auswertung geändert. Der neue Stand wird erneut abgeglichen.");
        let _disk_guard = context_store::lock(&self.folder)?;
        ensure!(context_store::read(&self.folder,&self.state.context)? == *expected,"Der Meetingkontext wurde in einem anderen Fenster geändert. Der aktuelle Stand bleibt erhalten.");
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

fn source_id(source: &LiveSource) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{}\0{}", source.id, source.text).as_bytes());
    format!("s_{}", &format!("{digest:x}")[..16])
}

fn quote_options(text: &str) -> Vec<String> {
    let mut options = Vec::new();
    let mut add = |s: &str| {
        let s = s.trim();
        if (8..=600).contains(&s.chars().count()) && !options.iter().any(|old| old == s) {
            options.push(s.to_owned());
        }
    };
    add(text);
    for sentence in text.split_inclusive(['.', '!', '?', '\n']) {
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

/// Loads only immutable complete revisions; no client, writer lock or directory creation.
pub fn read_state(folder: &Path) -> Result<WorkspaceState> {
    let mut state = store::read(folder)?;
    let context = context_store::read(folder, &state.context)?;
    state.apply_context(&context)?;
    Ok(state)
}
/// Explicit human input update. Caller binds this folder to its admitted meeting/run.
pub fn update_context(
    folder: &Path,
    context: &WorkspaceContext,
) -> Result<(WorkspaceContext, WorkspaceState)> {
    context_store::update(folder, context)
}

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
