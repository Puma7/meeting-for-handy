use crate::{Evidence, Source};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceContext {
    pub revision: u64,
    pub agenda: Vec<Agenda>,
    pub shared_notes: Vec<SharedNote>,
    pub overrides: Vec<Override>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Agenda {
    pub id: String,
    pub revision: u64,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedNote {
    pub id: String,
    pub revision: u64,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Override {
    pub id: String,
    pub expected_revision: u64,
    pub status: String,
    pub saved: bool,
}
impl WorkspaceContext {
    /// Reserve bounded request space for source text and the active frontier before accepting new shared inputs.
    pub(crate) fn validate_request_budget(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            serde_json::to_string(&(&self.agenda, &self.shared_notes))?.chars().count() <= 8_000,
            "Freigegebene Vorbereitung und Notizen sind zu groß. Bitte Text kürzen oder weniger Themen freigeben; private Entwürfe bleiben erhalten."
        );
        Ok(())
    }
    /// Validate shared text bounds, stable identifiers and permitted manual actions before publication.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.agenda.len() <= 40
                && self.shared_notes.len() <= 40
                && self.overrides.len() <= 2000,
            "Zu großer freigegebener Meetingkontext."
        );
        let mut ids = std::collections::HashSet::new();
        for (id, rev, text, prefix) in self
            .agenda
            .iter()
            .map(|a| (&a.id, a.revision, &a.text, "e_"))
            .chain(
                self.shared_notes
                    .iter()
                    .map(|n| (&n.id, n.revision, &n.text, "h_")),
            )
        {
            ensure!(
                valid_id(id, prefix)
                    && ids.insert(id)
                    && rev > 0
                    && !text.trim().is_empty()
                    && text.chars().count() <= if prefix == "e_" { 1000 } else { 1600 },
                "Ungültiger oder doppelter Meetingkontext."
            );
        }
        Ok(())
    }
}
/// Accept only bounded application identifiers, never paths or arbitrary renderer keys.
pub(crate) fn valid_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix)
        && id.len() <= 96
        && id.len() > prefix.len()
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub revision: u64,
    pub kind: String,
    pub text: String,
    pub status: String,
    pub attention: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub evidence: Vec<Evidence>,
    pub human_locked: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    #[serde(default)]
    pub human: bool,
    pub id: String,
    pub entity_id: String,
    pub revision: u64,
    pub at_ms: u64,
    pub text: String,
    pub status: String,
    pub evidence: Vec<Evidence>,
}

/// Associate a destination revision with its immutable source confirmation, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InheritedConfirmation {
    pub(crate) revision: u64,
    pub(crate) event_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceState {
    #[serde(skip)]
    pub(crate) stored_prompt_version: Option<String>,
    #[serde(skip)]
    pub(crate) context_updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[serde(skip)]
    pub(crate) human_activity:
        std::collections::BTreeMap<String, chrono::DateTime<chrono::FixedOffset>>,
    #[serde(default)]
    pub superseded_sources: Vec<String>,
    #[serde(default, skip_serializing)]
    pub(crate) replay_cursor: Option<usize>,
    #[serde(default)]
    pub context_excluded_ids: Vec<String>,
    #[serde(default, skip_serializing)]
    pub(crate) applied_overrides: Vec<String>,
    pub revision: u64,
    pub updated_at: Option<String>,
    pub context: WorkspaceContext,
    pub entities: Vec<Entity>,
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) inherited_confirmations: BTreeMap<String, InheritedConfirmation>,
    pub overview_refs: Vec<String>,
    pub originals: Vec<Source>,
    #[serde(skip_serializing)]
    pub(crate) processed: Vec<crate::live::LiveSource>,
    #[serde(default, skip_serializing)]
    pub(crate) inherited_sources: Vec<crate::live::LiveSource>,
}
impl WorkspaceState {
    /// Resolve only exact, current human confirmations. Inherited revisions use the
    /// original event identity, while older journals retain their direct revision match.
    pub fn current_confirmations(&self) -> HashMap<&str, &Event> {
        let entries: HashMap<_, _> = self
            .entities
            .iter()
            .filter(|entry| {
                entry.human_locked
                    && !self.context_excluded_ids.contains(&entry.id)
                    && !entry.evidence.is_empty()
                    && entry
                        .evidence
                        .iter()
                        .all(|evidence| !self.superseded_sources.contains(&evidence.source_id))
            })
            .map(|entry| (entry.id.as_str(), entry))
            .collect();
        self.events
            .iter()
            .filter(|event| event.human && event.status == "confirmed")
            .filter_map(|event| {
                let entry = entries.get(event.entity_id.as_str())?;
                let exact_revision = self
                    .inherited_confirmations
                    .get(&entry.id)
                    .filter(|binding| binding.revision == entry.revision)
                    .map_or(event.revision == entry.revision, |binding| {
                        binding.event_id.as_ref() == Some(&event.id)
                    });
                (exact_revision && event.text == entry.text && event.evidence == entry.evidence)
                    .then_some((entry.id.as_str(), event))
            })
            .collect()
    }

    /// Resolve a current original to its internal transcript chunk without exposing its file key.
    pub fn audio_source(&self, id: &str) -> Option<&crate::live::LiveSource> {
        if self.superseded_sources.iter().any(|old| old == id) {
            return None;
        }
        let original = self.originals.iter().find(|source| source.id == id)?;
        let mut candidates =
            self.processed
                .iter()
                .chain(&self.inherited_sources)
                .filter(|source| {
                    source.source == original.source
                        && source.start_ms == original.start_ms
                        && source.end_ms == original.end_ms
                        && source.text == original.text
                        && super::source_id(source) == id
                });
        let source = candidates.next()?;
        candidates.all(|other| other == source).then_some(source)
    }

    /// Preserve the exact prompt generation of immutable historical revisions.
    pub fn prompt_version(&self) -> &str {
        self.stored_prompt_version
            .as_deref()
            .unwrap_or(super::PROMPT_VERSION)
    }
    /// Rank durable human activity separately from the displayed time of the last model inference.
    pub(crate) fn activity_at(&self) -> Option<chrono::DateTime<chrono::FixedOffset>> {
        self.context_updated_at.max(
            self.updated_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok()),
        )
    }
    /// Apply shared inputs and human overrides, preserving original evidence and deterministic history events.
    pub(crate) fn apply_context(&mut self, context: &WorkspaceContext) -> Result<()> {
        for entity in self
            .entities
            .iter()
            .filter(|entity| entity.kind == "agenda")
        {
            if !context.agenda.iter().any(|entry| entry.id == entity.id)
                && !self.context_excluded_ids.contains(&entity.id)
            {
                self.context_excluded_ids.push(entity.id.clone());
            }
        }
        for a in &context.agenda {
            let previously_shared = self.context.agenda.iter().any(|entry| entry.id == a.id);
            self.context_excluded_ids.retain(|id| id != &a.id);
            match self.entities.iter_mut().find(|e| e.id == a.id) {
                Some(e)
                    if e.text != a.text
                        || !previously_shared
                        || e.evidence.iter().any(|evidence| {
                            self.superseded_sources.contains(&evidence.source_id)
                        }) =>
                {
                    e.text = a.text.clone();
                    e.revision += 1;
                    e.status = "open".into();
                    e.attention = "normal".into();
                    e.evidence.clear();
                    e.human_locked = false;
                }
                Some(_) => (),
                None => self.entities.push(Entity {
                    id: a.id.clone(),
                    revision: 1,
                    kind: "agenda".into(),
                    text: a.text.clone(),
                    status: "open".into(),
                    attention: "normal".into(),
                    owner: None,
                    due: None,
                    evidence: vec![],
                    human_locked: false,
                }),
            }
        }
        for action in &context.overrides {
            let key = serde_json::to_string(action)?;
            if self.applied_overrides.contains(&key) {
                continue;
            }
            let e = self
                .entities
                .iter_mut()
                .find(|e| e.id == action.id)
                .ok_or_else(|| anyhow::anyhow!("Der manuell geänderte Eintrag fehlt."))?;
            let confirmation = action.status == "confirmed";
            // A confirmation is separate from the task or decision's lifecycle.
            use sha2::{Digest, Sha256};
            let event_id = format!("h_{:x}", Sha256::digest(key.as_bytes()));
            if confirmation && self.events.iter().any(|event| event.id == event_id) {
                self.applied_overrides.push(key);
                continue;
            }
            if e.human_locked
                && Some(e.revision) == action.expected_revision.checked_add(1)
                && e.status == action.status
            {
                self.applied_overrides.push(key);
                continue;
            }
            ensure!(
                e.revision == action.expected_revision
                    && (confirmation
                        || ["open", "discussed", "completed"].contains(&action.status.as_str())),
                "Der Eintrag wurde inzwischen geändert. Bitte den aktuellen Stand verwenden."
            );
            if confirmation {
                ensure!(
                    !self.context_excluded_ids.contains(&e.id)
                        && !e.evidence.is_empty()
                        && e.evidence.iter().all(|evidence| {
                            !evidence.quote.trim().is_empty()
                                && !self.superseded_sources.contains(&evidence.source_id)
                                && self.originals.iter().any(|source| {
                                    source.id == evidence.source_id
                                        && source.text.contains(&evidence.quote)
                                })
                        }),
                    "Zum Bestätigen wird ein aktueller Originalbeleg benötigt."
                );
            } else {
                e.status = action.status.clone();
            }
            e.revision += 1;
            e.human_locked = true;
            // The persisted override identifies this event deterministically even when
            // a context journal is replayed before its next model-state checkpoint.
            self.events.push(Event {
                human: true,
                id: event_id,
                entity_id: e.id.clone(),
                revision: e.revision,
                at_ms: self
                    .originals
                    .iter()
                    .map(|source| source.end_ms)
                    .max()
                    .unwrap_or(0),
                text: e.text.clone(),
                status: if confirmation {
                    "confirmed".into()
                } else {
                    e.status.clone()
                },
                evidence: if confirmation {
                    e.evidence.clone()
                } else {
                    vec![]
                },
            });
            self.applied_overrides.push(key);
        }
        if self
            .context
            .shared_notes
            .iter()
            .any(|old| !context.shared_notes.iter().any(|new| new.id == old.id))
        {
            for e in &self.entities {
                // Context-only human actions may not yet be part of the model checkpoint.
                // Replay them first; only literal, still-current transcript evidence protects
                // a confirmed item from invalidation of context-dependent AI drafts.
                let confirmed_original = e.human_locked
                    && !e.evidence.is_empty()
                    && e.evidence.iter().all(|evidence| {
                        !self.superseded_sources.contains(&evidence.source_id)
                            && self.originals.iter().any(|source| {
                                source.id == evidence.source_id
                                    && source.text.contains(&evidence.quote)
                            })
                    });
                if e.kind != "agenda"
                    && !confirmed_original
                    && !self.context_excluded_ids.contains(&e.id)
                {
                    self.context_excluded_ids.push(e.id.clone());
                }
            }
        }
        self.overview_refs
            .retain(|id| !self.context_excluded_ids.contains(id));
        self.context = context.clone();
        Ok(())
    }
}

#[test]
fn content_confirmation_is_revision_bound_and_replays_with_its_exact_evidence() {
    let quote = "Robin will send the final agenda on Friday.";
    let evidence = Evidence {
        source_id: "C1".into(),
        quote: quote.into(),
    };
    let mut state = WorkspaceState {
        originals: vec![Source {
            id: "C1".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1_000,
            text: quote.into(),
        }],
        entities: vec![Entity {
            id: "task1".into(),
            revision: 1,
            kind: "task".into(),
            text: "Send the final agenda.".into(),
            status: "open".into(),
            attention: "normal".into(),
            owner: None,
            due: None,
            evidence: vec![evidence.clone()],
            human_locked: false,
        }],
        ..Default::default()
    };
    let context = WorkspaceContext {
        revision: 1,
        overrides: vec![Override {
            id: "task1".into(),
            expected_revision: 1,
            status: "confirmed".into(),
            saved: false,
        }],
        ..Default::default()
    };
    let mut replaced = state.clone();
    replaced.superseded_sources.push("C1".into());
    assert!(replaced.apply_context(&context).is_err());
    assert!(replaced.events.is_empty());
    state.apply_context(&context).unwrap();
    assert_eq!(state.entities[0].status, "open");
    assert_eq!(state.entities[0].revision, 2);
    assert!(state.entities[0].human_locked);
    assert_eq!(state.events[0].status, "confirmed");
    assert_eq!(state.events[0].evidence, vec![evidence]);
    state.applied_overrides.clear();
    state.apply_context(&context).unwrap();
    assert_eq!(state.events.len(), 1);
    let stale = WorkspaceContext {
        overrides: vec![Override {
            saved: true,
            ..context.overrides[0].clone()
        }],
        ..context
    };
    assert!(state.apply_context(&stale).is_err());
    assert_eq!(state.entities[0].revision, 2);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Window {
    source_revision: String,
    start_ms: u64,
    end_ms: u64,
    required_source_ids: Vec<String>,
    context_source_ids: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HumanContext {
    id: String,
    revision: u64,
    kind: String,
    text: String,
    anchor_source_id: Option<String>,
    marker_type: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Budget {
    max_changes: usize,
    max_overview_refs: usize,
    max_title_chars: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Request {
    pub contract_version: String,
    pub request_id: String,
    pub language: String,
    pub presentation_instructions: String,
    pub purpose: String,
    window: Window,
    #[serde(serialize_with = "serialize_sources")]
    pub transcript_sources: Vec<Source>,
    pub state: Vec<Entity>,
    human_context: Vec<HumanContext>,
    overview_refs: Vec<String>,
    output_budget: Budget,
}
impl Request {
    /// Use compact generated titles in the production pipeline; literal evidence has its own budget.
    pub fn compact_titles(&mut self) {
        self.output_budget.max_title_chars = 180;
    }
    /// Read the app-owned title budget; it is never taken from a model response.
    pub fn max_title_chars(&self) -> usize {
        self.output_budget.max_title_chars
    }
    /// Return the exact new-source identifiers the provider must examine in this window.
    pub fn required_source_ids(&self) -> &[String] {
        &self.window.required_source_ids
    }
    /// Construct a path-free provider request containing only released context and selected literal sources.
    pub fn new(
        id: String,
        context: &WorkspaceContext,
        previous: &WorkspaceState,
        state: Vec<Entity>,
        fresh: &[Source],
        sources: Vec<Source>,
    ) -> Self {
        let required: Vec<String> = fresh.iter().map(|s| s.id.clone()).collect();
        let context_ids = sources
            .iter()
            .filter(|s| !required.contains(&s.id))
            .map(|s| s.id.clone())
            .collect();
        Self {
            contract_version: super::CONTRACT_VERSION.into(),
            request_id: id,
            language: "de".into(),
            presentation_instructions: String::new(),
            purpose: "live_update".into(),
            window: Window {
                source_revision: format!("tx_{}", previous.revision),
                start_ms: fresh.iter().map(|s| s.start_ms).min().unwrap_or(0),
                end_ms: fresh.iter().map(|s| s.end_ms).max().unwrap_or(0),
                required_source_ids: required,
                context_source_ids: context_ids,
            },
            transcript_sources: sources,
            state,
            human_context: context
                .shared_notes
                .iter()
                .map(|n| HumanContext {
                    id: n.id.clone(),
                    revision: n.revision,
                    kind: "shared_note".into(),
                    text: n.text.clone(),
                    anchor_source_id: None,
                    marker_type: None,
                })
                .collect(),
            overview_refs: vec![],
            output_budget: Budget {
                max_changes: 20,
                max_overview_refs: 8,
                max_title_chars: 1600,
            },
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Change {
    pub proposal_id: String,
    pub kind: String,
    pub target_id: Option<String>,
    pub target_revision: Option<u64>,
    pub text: Option<String>,
    pub status: String,
    pub attention: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub related_entity_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Response {
    pub contract_version: String,
    pub request_id: String,
    pub changes: Vec<Change>,
    pub overview_refs: Vec<String>,
    pub unresolved_context_ids: Vec<String>,
    pub more_changes_possible: bool,
}

/// Serialize source DTOs without granting the provider audio or session-object access.
fn serialize_sources<S: serde::Serializer>(
    sources: &[Source],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    sources.iter().map(|s|serde_json::json!({"id":s.id,"track":s.source,"startMs":s.start_ms,"endMs":s.end_ms,"text":s.text})).collect::<Vec<_>>().serialize(serializer)
}
