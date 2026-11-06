use super::model::*;
use crate::validation::embedded_english_question;
use anyhow::{ensure, Context, Result};
use std::collections::{HashMap, HashSet};

/// Validate a complete model bundle against its request and sources before mutating any state.
pub(crate) fn apply(
    state: &mut WorkspaceState,
    request: &Request,
    json: &str,
) -> Result<Vec<Event>> {
    let response: Response = serde_json::from_str(json)
        .context("Ungültige oder doppelte Felder in der Meetingantwort.")?;
    ensure!(
        response.contract_version == super::CONTRACT_VERSION
            && response.request_id == request.request_id,
        "Die Meetingantwort gehört nicht zu diesem Auftrag."
    );
    if response.more_changes_possible || response.changes.len() > 20 {
        return Err(crate::transport::BudgetError::Output.into());
    }
    ensure!(
        response.unresolved_context_ids.is_empty(),
        "Zum Abgleich fehlt früherer Kontext. Der letzte gültige Stand bleibt erhalten."
    );
    ensure!(
        response.overview_refs.len() <= 8,
        "Zu viele Überblickseinträge."
    );
    ensure!(
        request.purpose != "reconcile" || response.overview_refs.is_empty(),
        "Im Impulsabgleich sind keine Überblicksreferenzen erlaubt."
    );
    let mut next = state.clone();
    let mut aliases = HashMap::new();
    let mut targets = HashSet::new();
    let mut events = vec![];
    let mut acknowledged_open_topic = false;
    for mut c in response.changes {
        acknowledged_open_topic |= request.purpose == "reconcile"
            && c.kind == "agenda"
            && ["touched", "unclear"].contains(&c.status.as_str())
            && c.evidence
                .iter()
                .any(|evidence| missing_pledge(&evidence.quote));
        let phase_allowed = match request.purpose.as_str() {
            "tasks" => c.kind == "task",
            "facts" => ["fact", "decision"].contains(&c.kind.as_str()),
            "reconcile" => ["agenda", "impulse"].contains(&c.kind.as_str()),
            "live_update" => ["fact", "task", "decision"].contains(&c.kind.as_str()),
            _ => false,
        };
        ensure!(
            phase_allowed,
            "Der Ergebnistyp gehört nicht zur aktuellen Auswertungsphase."
        );
        ensure!(
            c.kind == "task" || (c.owner.is_none() && c.due.is_none()),
            "Zuständige und Termine sind nur bei ausdrücklich belegten Aufgaben erlaubt."
        );
        ensure!(c.evidence.len() <= 6, "Zu viele Belege.");
        c.evidence
            .sort_by(|a, b| (&a.source_id, &a.quote).cmp(&(&b.source_id, &b.quote)));
        c.evidence.dedup();
        ensure!(
            valid_id(&c.proposal_id, "p_") && !aliases.contains_key(&c.proposal_id),
            "Doppelte Vorschlagskennung."
        );
        let allowed = allowed_statuses(&c.kind);
        ensure!(
            allowed.contains(&c.status.as_str())
                && ["normal", "current", "clarification"].contains(&c.attention.as_str()),
            "Ungültiger Meetingstatus."
        );
        let old = c
            .target_id
            .as_ref()
            .map(|id| {
                request
                    .state
                    .iter()
                    .find(|e| &e.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Unbekannte Zielkennung."))
            })
            .transpose()?;
        if let Some(old) = old {
            ensure!(
                next.entities.iter().any(|current| current == old)
                    && !next.context_excluded_ids.contains(&old.id),
                "Die Modellantwort verwendet einen inzwischen geänderten Eintrag."
            );
            ensure!(
                !old.human_locked,
                "Eine Modellantwort darf menschlich bestätigte Einträge nicht verändern."
            );
            ensure!(
                old.kind != "task" || !["completed", "withdrawn"].contains(&old.status.as_str()),
                "Abgeschlossene oder zurückgenommene Aufgaben sind nur Lesekontext."
            );
            ensure!(
                old.kind != "decision" || old.status != "withdrawn",
                "Zurückgenommene Beschlüsse sind nur Lesekontext; ein neuer Beschluss benötigt eine eigene Identität."
            );
            ensure!(
                Some(old.revision) == c.target_revision,
                "Die Modellantwort verwendet eine veraltete Eintragsfassung."
            );
            ensure!(
                old.kind == c.kind,
                "Die Modellantwort verwechselt die Art des Zieleintrags."
            );
            ensure!(
                targets.insert(old.id.clone()),
                "Die Modellantwort ändert denselben Eintrag mehrfach."
            );
        } else {
            ensure!(
                c.target_revision.is_none()
                    && c.text.as_ref().is_some_and(|t| !t.trim().is_empty())
                    && c.kind != "agenda"
                    && !["completed", "withdrawn", "discussed"].contains(&c.status.as_str()),
                "Ungültiger neuer Meetingeintrag."
            );
        }
        if c.kind == "task" && old.is_none() {
            ensure!(c.evidence.iter().any(|e|request.required_source_ids().contains(&e.source_id)),"Eine neue Aufgabe benötigt einen Beleg aus dem aktuellen Quellenfenster; ältere Aufgaben nicht duplizieren.");
            ensure!(c.evidence.iter().any(|e|task_quote_allowed(&e.quote)) && !c.evidence.iter().any(|e|task_missing_pledge(&e.quote)),"Eine neue Aufgabe benötigt eine ausdrückliche Übernahme oder Zuweisung. Eine fehlende Zusage ist keine Aufgabe.");
        }
        if c.kind == "agenda" {
            ensure!(c.text.is_none(), "Agendatext gehört dem Menschen.");
        }
        ensure!(c.related_entity_ids.is_empty(),"Zusätzliche Zuordnungen sind in diesem Aktualisierungsvertrag nicht verfügbar; relatedEntityIds muss leer bleiben.");
        let text = c
            .text
            .clone()
            .or_else(|| old.map(|e| e.text.clone()))
            .unwrap_or_default();
        let kind = if c.kind == "task" {
            crate::Kind::Tasks
        } else {
            crate::Kind::Summary
        };
        let screen = if c.kind == "task" && old.is_some() {
            crate::validation::screen_task_status_facts
        } else {
            crate::validation::screen_facts
        };
        let retained = old
            .map(|old| retained_update_evidence(state, old, &c, request))
            .transpose()?
            .unwrap_or_default();
        let mut text_evidence = c.evidence.clone();
        if c.text.is_none() && c.kind != "agenda" && old.is_some() {
            // Old text evidence may support its unchanged numbers, but it
            // cannot rescue an unsupported/nonbinding new status claim.
            let status = crate::Fact {
                text: c
                    .evidence
                    .first()
                    .map(|e| e.quote.clone())
                    .unwrap_or_default(),
                owner: c.owner.clone(),
                due: c.due.clone(),
                evidence: c.evidence.clone(),
            };
            let status = screen(
                vec![status],
                &request.transcript_sources,
                &kind,
                "workspace",
                1,
            );
            ensure!(
                status.items.len() == 1 && status.rejected.is_empty(),
                "Der neue Status hat keinen eigenständig zulässigen Originalbeleg."
            );
            for evidence in &retained {
                if !text_evidence.contains(evidence) {
                    text_evidence.push(evidence.clone());
                }
            }
        }
        let fact = crate::Fact {
            // Agenda titles are unchanged human input, not generated claims.
            // Screen the status evidence itself, retaining every literal-source
            // and metadata check without requiring title numbering in the quote.
            text: if c.kind == "agenda" {
                c.evidence
                    .first()
                    .map(|e| e.quote.clone())
                    .unwrap_or_default()
            } else {
                text.clone()
            },
            owner: c.owner.clone(),
            due: c.due.clone(),
            evidence: text_evidence,
        };
        let checked = screen(
            vec![fact],
            &request.transcript_sources,
            &kind,
            "workspace",
            1,
        );
        ensure!(
            checked.items.len() == 1 && checked.rejected.is_empty(),
            "Eine Änderung ({}) hat keinen zulässigen Originalbeleg: {:?}.",
            c.kind,
            checked.rejected
        );
        // Promise/completion/withdrawal are different transitions. Narrow, explicit evidence
        // guards complement the semantic model judgement; uncertainty never closes an item.
        if c.status == "completed" {
            ensure!(
                c.evidence
                    .iter()
                    .any(|e| completion_quote_allowed(&e.quote)),
                "Eine Zusage ist keine Erledigung."
            );
        }
        if c.status == "withdrawn" {
            ensure!(
                c.evidence
                    .iter()
                    .any(|e| withdrawal_quote_allowed(&e.quote)),
                "Eine Rücknahme benötigt einen ausdrücklichen Beleg."
            );
        }
        if c.kind == "decision" && c.status == "recorded" {
            ensure!(
                c.evidence
                    .iter()
                    .any(|e| recorded_decision_quote_allowed(&e.quote)),
                "Ein Vorschlag ist kein Beschluss."
            );
        }
        let target = old
            .map(|e| e.id.clone())
            .or_else(|| {
                next.entities
                    .iter()
                    .find(|e| {
                        // Terminal identities retain their original assertion plus
                        // later status evidence. Replay must not revive a withdrawn
                        // decision or duplicate a completed/withdrawn task.
                        let terminal_identity = ((c.kind == "task"
                            && ["completed", "withdrawn"].contains(&e.status.as_str()))
                            || (c.kind == "decision" && e.status == "withdrawn"))
                            && !c.evidence.is_empty()
                            && c.evidence
                                .iter()
                                .all(|v| retained_terminal_witness(&next, e, v));
                        !next.context_excluded_ids.contains(&e.id)
                            && e.kind == c.kind
                            && e.text == text
                            && (e.owner == c.owner || (terminal_identity && c.owner.is_none()))
                            && (e.due == c.due || (terminal_identity && c.due.is_none()))
                            && (e.evidence == c.evidence || terminal_identity)
                    })
                    .map(|e| e.id.clone())
            })
            .unwrap_or_else(|| format!("e_{}", uuid::Uuid::new_v4().simple()));
        aliases.insert(c.proposal_id.clone(), target.clone());
        if let Some(existing) = next.entities.iter_mut().find(|e| e.id == target) {
            if old.is_none() {
                continue;
            }
            let before = existing.clone();
            existing.text = text;
            existing.status = c.status;
            existing.attention = c.attention;
            existing.evidence = c.evidence.clone();
            for evidence in &retained {
                if !existing.evidence.contains(evidence) {
                    existing.evidence.push(evidence.clone());
                }
            }
            ensure!(existing.evidence.len()<=6,"Metadaten und Status benötigen zu viele Originalbelege; der vorherige Stand bleibt erhalten.");
            if c.owner.is_some() {
                existing.owner = c.owner;
            }
            if c.due.is_some() {
                existing.due = c.due;
            }
            if existing.kind != "agenda" {
                let checked = screen(
                    vec![crate::Fact {
                        text: existing.text.clone(),
                        owner: existing.owner.clone(),
                        due: existing.due.clone(),
                        evidence: existing.evidence.clone(),
                    }],
                    &request.transcript_sources,
                    &kind,
                    "workspace",
                    1,
                );
                ensure!(checked.items.len() == 1 && checked.rejected.is_empty(), "Der aktualisierte Eintrag benötigt zulässige Originalbelege für Text und Metadaten.");
            }
            if *existing == before {
                continue;
            }
            existing.revision += 1;
        } else {
            next.entities.push(Entity {
                id: target.clone(),
                revision: 1,
                kind: c.kind,
                text,
                status: c.status,
                attention: c.attention,
                owner: c.owner,
                due: c.due,
                evidence: c.evidence.clone(),
                human_locked: false,
            });
        }
        let e = next.entities.iter().find(|e| e.id == target).unwrap();
        let at_ms = request
            .transcript_sources
            .iter()
            .filter(|s| e.evidence.iter().any(|v| v.source_id == s.id))
            .map(|s| s.end_ms)
            .max()
            .unwrap_or(0);
        events.push(Event {
            human: false,
            id: format!("v_{}", uuid::Uuid::new_v4().simple()),
            entity_id: target,
            revision: e.revision,
            at_ms,
            text: e.text.clone(),
            status: e.status.clone(),
            evidence: e.evidence.clone(),
        });
    }
    let mut refs = vec![];
    for id in response.overview_refs {
        let target = aliases.get(&id).cloned().unwrap_or(id);
        ensure!(
            request.state.iter().any(|e| e.id == target) || aliases.values().any(|v| v == &target),
            "Unbekannter Überblickseintrag."
        );
        ensure!(
            next.entities
                .iter()
                .any(|e| e.id == target && ["fact", "task", "decision"].contains(&e.kind.as_str())),
            "Agenda oder Impuls ist kein belegtes Ergebnis."
        );
        if !refs.contains(&target) {
            refs.push(target);
        }
    }
    if request.purpose != "reconcile" {
        next.overview_refs = refs;
    }
    // Enforce the existing prompt contract for a gap the model itself acknowledged.
    // A retained human-reviewed impulse satisfies it; never auto-reopen or invent a question.
    ensure!(!acknowledged_open_topic || next.entities.iter().any(|entity|
        entity.kind == "impulse" && !next.context_excluded_ids.contains(&entity.id)),
        "Ein als offen belegter Agendapunkt benötigt den geforderten Frageimpuls; die Antwort wird vollständig verworfen.");
    next.events.extend(events.clone());
    *state = next;
    Ok(events)
}

/// Match an original promise/decision without enlarging the bounded current evidence set.
/// Only this identity's uninterrupted current-text history can supply an omitted witness.
fn retained_terminal_witness(
    state: &WorkspaceState,
    entity: &Entity,
    evidence: &crate::Evidence,
) -> bool {
    entity.evidence.contains(evidence)
        || ((match entity.kind.as_str() {
            "task" => task_quote_allowed(&evidence.quote),
            "decision" => recorded_decision_quote_allowed(&evidence.quote),
            _ => false,
        }) && !state.superseded_sources.contains(&evidence.source_id)
            && state
                .originals
                .iter()
                .any(|s| s.id == evidence.source_id && s.text.contains(&evidence.quote))
            && state
                .events
                .iter()
                .rev()
                .filter(|event| event.entity_id == entity.id)
                .take_while(|event| event.text == entity.text)
                .any(|event| {
                    event.evidence.contains(evidence)
                        && (entity.kind != "decision" || event.status == "recorded")
                }))
}

/// Select text and metadata witnesses within the unchanged combined six-evidence budget.
/// Every new status quote is kept; complete older evidence remains in immutable history.
fn retained_update_evidence(
    state: &WorkspaceState,
    old: &Entity,
    change: &Change,
    request: &Request,
) -> Result<Vec<crate::Evidence>> {
    if change.kind == "agenda" {
        return Ok(Vec::new());
    }
    let keep_text = change.text.is_none();
    let text_evidence = if keep_text {
        state
            .events
            .iter()
            .rev()
            .filter(|event| event.entity_id == old.id)
            .take_while(|event| event.text == old.text)
            .last()
            .map(|event| &event.evidence)
            .unwrap_or(&old.evidence)
            .as_slice()
    } else {
        &[]
    };
    ensure!(text_evidence.len() <= 6, "Ungültige frühere Beleganzahl.");
    let kind = if change.kind == "task" {
        crate::Kind::Tasks
    } else {
        crate::Kind::Summary
    };
    let screen = if change.kind == "task" {
        crate::validation::screen_task_status_facts
    } else {
        crate::validation::screen_facts
    };
    let mut candidates = text_evidence.to_vec();
    for evidence in &old.evidence {
        let quote = evidence.quote.to_lowercase();
        let supports_metadata = [
            old.owner.as_ref().filter(|_| change.owner.is_none()),
            old.due.as_ref().filter(|_| change.due.is_none()),
        ]
        .into_iter()
        .flatten()
        .any(|value| quote.contains(&value.to_lowercase()));
        if supports_metadata
            && !candidates.contains(evidence)
            && request.transcript_sources.iter().any(|source| {
                source.id == evidence.source_id && source.text.contains(&evidence.quote)
            })
        {
            candidates.push(evidence.clone());
        }
    }
    candidates.retain(|evidence| !change.evidence.contains(evidence));
    ensure!(candidates.len() <= 12, "Ungültiger früherer Belegumfang.");
    let mut best: Option<Vec<crate::Evidence>> = None;
    let remaining = 6usize.saturating_sub(change.evidence.len());
    // At most 4096 subsets of six original text and six current metadata witnesses.
    // Prune oversized/nonminimal subsets; never drop a submitted quote or raise the limit.
    for mask in 0usize..(1usize << candidates.len()) {
        let count = mask.count_ones() as usize;
        if count > remaining || best.as_ref().is_some_and(|prior| count >= prior.len()) {
            continue;
        }
        let retained = candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, evidence)| evidence.clone())
            .collect::<Vec<_>>();
        if keep_text
            && !change
                .evidence
                .iter()
                .chain(&retained)
                .any(|e| text_evidence.contains(e))
        {
            continue;
        }
        let mut evidence = change.evidence.clone();
        for previous in &retained {
            if !evidence.contains(previous) {
                evidence.push(previous.clone());
            }
        }
        if evidence.len() > 6 {
            continue;
        }
        let checked = screen(
            vec![crate::Fact {
                text: change.text.clone().unwrap_or_else(|| old.text.clone()),
                owner: change.owner.clone().or_else(|| old.owner.clone()),
                due: change.due.clone().or_else(|| old.due.clone()),
                evidence,
            }],
            &request.transcript_sources,
            &kind,
            "workspace",
            1,
        );
        if checked.items.len() == 1
            && checked.rejected.is_empty()
            && best
                .as_ref()
                .is_none_or(|prior| retained.len() < prior.len())
        {
            best = Some(retained);
        }
    }
    best.context("Text, Metadaten und neuer Status benötigen gemeinsam zulässige Originalbelege innerhalb der unveränderten Grenze.")
}

// Shared exact-word polarity screen. Apostrophes tokenize "we'll"/"we’ll"
// into "we", "ll", so their future marker must be checked as well.
const UNCERTAIN_TERMINAL_WORDS: &[&str] = &[
    "nicht",
    "kein",
    "keine",
    "keinen",
    "noch",
    "falls",
    "wenn",
    "würde",
    "werde",
    "vielleicht",
    "not",
    "never",
    "nichts",
    "nothing",
    "none",
    "neither",
    "no",
    "if",
    "unless",
    "would",
    "will",
    "might",
    "maybe",
    "could",
    "should",
    "haven",
    "hasn",
    "didn",
    "isn",
    "wasn",
    "won",
];

const ADDITIONAL_UNCERTAIN_TERMINAL_WORDS: &[&str] = &[
    "ll",
    "nobody",
    "noone",
    "keineswegs",
    "nie",
    "niemals",
    "keinem",
    "keiner",
    "keines",
    "niemand",
    "unsicher",
    "eventuell",
    "möglicherweise",
    "könnte",
    "könnten",
    "sollte",
    "sollten",
    "würden",
    "werden",
    "cannot",
    "dont",
    "don",
    "doesn",
    "weren",
    "hadn",
    "wouldn",
    "shouldn",
    "couldn",
    "mustn",
    "ob",
    "whether",
];

const SUBORDINATE_CLAUSE_MARKERS: &[&str] = &[
    "dass", "ob", "weil", "damit", "wenn", "falls", "sofern", "that", "whether", "because", "when",
    "if", "unless",
];

/// Temporary screening tokens never change the literal source or its quote.
fn assertion_words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphabetic() && c != '\'')
        .flat_map(|token| match token {
            "i've" => vec!["i", "have"],
            "we've" => vec!["we", "have"],
            _ => token.split('\'').filter(|word| !word.is_empty()).collect(),
        })
        .collect()
}

/// Preserve list/parenthetical commas and decimal/version separators, while
/// isolating a following finite or dependent clause from an earlier auxiliary.
fn assertion_clauses(text: &str) -> Vec<std::ops::Range<usize>> {
    assertion_ranges(text, true)
}

/// Keep matrix verbs in scope across commas when checking dependent assertions.
fn assertion_ranges(text: &str, split_commas: bool) -> Vec<std::ops::Range<usize>> {
    let boundaries: Vec<_> = text
        .char_indices()
        .filter(|(_, ch)| ['.', '!', '?', ';', ',', ':', '\n'].contains(ch))
        .chain(std::iter::once((text.len(), '\n')))
        .collect();
    let mut result = Vec::new();
    let mut segment = 0;
    let mut first_word = 0;
    for (boundary, &(at, ch)) in boundaries.iter().enumerate() {
        let numeric = ['.', ','].contains(&ch)
            && at
                .checked_sub(1)
                .and_then(|i| text.as_bytes().get(i))
                .is_some_and(|c| c.is_ascii_digit())
            && text
                .as_bytes()
                .get(at + 1)
                .is_some_and(|c| c.is_ascii_digit());
        if numeric || (ch == ',' && !split_commas) {
            continue;
        }
        if ch == ',' {
            let next_end = boundaries.get(boundary + 1).map_or(text.len(), |v| v.0);
            let next = assertion_words(&text[at + 1..next_end]);
            let mut previous_words = text[segment..at]
                .rsplit(|c: char| !c.is_alphabetic())
                .filter(|word| !word.is_empty());
            let previous = previous_words.next();
            let parenthetical = previous_words
                .next()
                .is_some_and(|word| ["as", "wie"].contains(&word));
            let decision_complement = !parenthetical
                && previous.is_some_and(|word| {
                    [
                        "beschlossen",
                        "entschieden",
                        "beschließen",
                        "entscheiden",
                        "decided",
                        "agreed",
                    ]
                    .contains(&word)
                });
            let dependent = next
                .first()
                .is_some_and(|word| SUBORDINATE_CLAUSE_MARKERS.contains(word));
            let finite = next.iter().enumerate().any(|(index, word)| {
                [
                    "habe", "haben", "hat", "hatte", "hatten", "ist", "sind", "war", "waren",
                    "wird", "werden", "wurde", "wurden", "muss", "müssen", "kann", "können", "am",
                    "is", "are", "was", "were", "have", "has", "had", "will", "would", "can",
                    "could", "should", "must",
                ]
                .contains(word)
                    && !(index > 0 && ["to", "zu"].contains(&next[index - 1]))
            });
            if !decision_complement && !dependent && !finite {
                continue;
            }
        }
        {
            let count = text[segment..at]
                .split(|c: char| !c.is_alphabetic())
                .filter(|word| !word.is_empty())
                .count();
            result.push(first_word..first_word + count);
            first_word += count;
            segment = (at + ch.len_utf8()).min(text.len());
        }
    }
    result
}

/// Locate a dependent clause before a predicate, even when ASR omitted its comma.
fn subordinate_start(words: &[&str], start: usize, predicate: usize) -> Option<usize> {
    words[start..predicate]
        .iter()
        .rposition(|word| SUBORDINATE_CLAUSE_MARKERS.contains(word))
        .map(|at| start + at + 1)
}

const AFFIRMATIVE_PREDICATE_ADVERBS: &[&str] = &[
    "already",
    "just",
    "now",
    "today",
    "finally",
    "unanimously",
    "explicitly",
];

/// A following infinitive can make the participle part of a proposal rather
/// than its asserted outcome. Distinguish it from "zu den Akten"/"zu 100 Prozent".
fn infinitive_after(words: &[&str], predicate: usize, clause_end: usize) -> bool {
    predicate + 2 < clause_end && words[predicate + 1] == "zu" && {
        let verb = words[predicate + 2];
        ["sein", "tun"].contains(&verb)
            || ((verb.ends_with("en") || verb.ends_with("ern") || verb.ends_with("eln"))
                && ![
                    "den", "dem", "der", "des", "einen", "einem", "einer", "ihnen", "meinen",
                    "deinen", "seinen", "unseren", "euren",
                ]
                .contains(&verb))
    }
}

const MODAL_PREDICATE_WORDS: &[&str] = &[
    "can",
    "could",
    "may",
    "might",
    "must",
    "shall",
    "should",
    "will",
    "would",
    "kann",
    "können",
    "könnte",
    "könnten",
    "muss",
    "müssen",
    "müsste",
    "müssten",
    "darf",
    "dürfen",
    "dürfte",
    "dürften",
    "soll",
    "sollen",
    "sollte",
    "sollten",
    "wollen",
    "möchte",
    "möchten",
    "kannst",
    "könnt",
    "konnte",
    "konntest",
    "konnten",
    "konntet",
    "könntest",
    "könntet",
    "musst",
    "müsst",
    "musste",
    "musstest",
    "mussten",
    "musstet",
    "müsstest",
    "müsstet",
    "darfst",
    "dürft",
    "durfte",
    "durftest",
    "durften",
    "durftet",
    "dürftest",
    "dürftet",
    "sollst",
    "sollt",
    "solltest",
    "solltet",
    "willst",
    "wollt",
    "wollte",
    "wolltest",
    "wollten",
    "wolltet",
    "mag",
    "magst",
    "mögen",
    "mögt",
    "mochte",
    "mochtest",
    "mochten",
    "mochtet",
    "möchtest",
    "möchtet",
];

/// German perfect modal constructions use a following replacement infinitive.
fn modal_after(words: &[&str], predicate: usize, end: usize) -> bool {
    predicate + 1 < end
        && ["wollen", "müssen", "sollen", "können", "dürfen", "mögen"]
            .contains(&words[predicate + 1])
}

fn modal_before(words: &[&str], start: usize, predicate: usize) -> bool {
    // The initial token can be a name ("May cancelled"); an auxiliary follows
    // its subject. The clause scope excludes a separate confirmed that-clause.
    words[start..predicate]
        .iter()
        .skip(1)
        .any(|word| MODAL_PREDICATE_WORDS.contains(word))
}

fn unasserted_auxiliary_before(words: &[&str], start: usize, predicate: usize) -> bool {
    if modal_before(words, start, predicate) {
        return true;
    }
    (start..predicate).any(|index| {
        if !["to", "zu"].contains(&words[index]) {
            return false;
        }
        let mut auxiliary = index + 1;
        while auxiliary < predicate && AFFIRMATIVE_PREDICATE_ADVERBS.contains(&words[auxiliary]) {
            auxiliary += 1;
        }
        auxiliary < predicate && ["have", "be", "haben", "sein"].contains(&words[auxiliary])
    })
}

fn finite_subject_before(
    words: &[&str],
    original: &[&str],
    start: usize,
    predicate: usize,
) -> bool {
    if modal_before(words, start, predicate) {
        return false;
    }
    let mut subject = predicate;
    while subject > start
        && (AFFIRMATIVE_PREDICATE_ADVERBS.contains(&words[subject - 1])
            || ["hereby", "hiermit", "jedenfalls"].contains(&words[subject - 1]))
    {
        subject -= 1;
    }
    if subject <= start {
        return false;
    }
    if [
        "i", "we", "you", "they", "he", "she", "it", "ich", "wir", "du", "ihr", "er", "sie", "es",
    ]
    .contains(&words[subject - 1])
    {
        // A pronoun can also be an object ("we want it cancelled"). Only
        // the subject prefix, optionally after a time adverb, is finite here.
        return words[start..subject - 1].iter().all(|word| {
            [
                "today",
                "now",
                "yesterday",
                "heute",
                "jetzt",
                "gestern",
                "hereby",
                "hiermit",
            ]
            .contains(word)
        });
    }
    // A bounded nominal subject occupies the same finite-clause prefix; it
    // cannot skip a planning verb, object, or infinitive marker to find a name.
    let nominal = &words[start..subject];
    let names = &original[start..subject];
    if nominal.iter().any(|word| ["to", "zu"].contains(word))
        || (nominal.len() > 1
            && [
                "i", "we", "you", "they", "he", "she", "it", "ich", "wir", "du", "ihr", "er",
                "sie", "es",
            ]
            .contains(&nominal[0]))
    {
        return false;
    }
    nominal.len() <= 3
        && (names
            .iter()
            .all(|word| word.chars().next().is_some_and(char::is_uppercase))
            || (nominal.len() >= 2
                && [
                    "the", "our", "your", "their", "his", "her", "der", "die", "das", "unser",
                    "unsere", "ihr", "ihre",
                ]
                .contains(&nominal[0])))
}

/// Reject a desired, expected or believed outcome even when its dependent
/// clause uses a completed tense. A separate finite assertion remains eligible.
fn unasserted_embedding(
    words: &[&str],
    original: &[&str],
    sentences: &[std::ops::Range<usize>],
    predicate: usize,
) -> bool {
    let Some(sentence) = sentences.iter().find(|range| range.contains(&predicate)) else {
        return true;
    };
    let mut start = sentence.start;
    for conjunction in sentence.start..predicate {
        if !["and", "but", "und", "aber"].contains(&words[conjunction]) {
            continue;
        }
        let mut next = conjunction + 1;
        while next < predicate
            && [
                "now",
                "today",
                "yesterday",
                "heute",
                "jetzt",
                "gestern",
                "nun",
            ]
            .contains(&words[next])
        {
            next += 1;
        }
        let shared_perfect = next == predicate
            && !words[start..conjunction]
                .iter()
                .any(|word| SUBORDINATE_CLAUSE_MARKERS.contains(word))
            && (start..conjunction).any(|index| {
                ["habe", "haben", "have", "has", "had"].contains(&words[index])
                    && finite_subject_before(words, original, start, index)
            });
        if shared_perfect
            || (next..=predicate).any(|index| {
                (index == predicate
                    || [
                        "habe", "haben", "hast", "hat", "habt", "hatte", "hatten", "ist", "sind",
                        "war", "waren", "wurde", "wurden", "have", "has", "had", "am", "is", "are",
                        "was", "were",
                    ]
                    .contains(&words[index]))
                    && finite_subject_before(words, original, next, index)
            })
        {
            start = next;
        }
    }
    (start..predicate).any(|index| {
        if ![
            "hope",
            "hopes",
            "hoped",
            "hoping",
            "wish",
            "wishes",
            "wished",
            "wishing",
            "want",
            "wants",
            "wanted",
            "wanting",
            "plan",
            "plans",
            "planned",
            "planning",
            "propose",
            "proposes",
            "proposed",
            "proposing",
            "suggest",
            "suggests",
            "suggested",
            "suggesting",
            "expect",
            "expects",
            "expected",
            "expecting",
            "assume",
            "assumes",
            "assumed",
            "assuming",
            "guess",
            "guesses",
            "guessed",
            "guessing",
            "believe",
            "believes",
            "believed",
            "believing",
            "think",
            "thinks",
            "thought",
            "thinking",
            "hoffe",
            "hoffst",
            "hofft",
            "hoffen",
            "hoffte",
            "hofftest",
            "hofften",
            "hofftet",
            "gehofft",
            "wünsche",
            "wünschst",
            "wünscht",
            "wünschen",
            "wünschte",
            "wünschtest",
            "wünschten",
            "wünschtet",
            "gewünscht",
            "plane",
            "planst",
            "plant",
            "planen",
            "plante",
            "plantest",
            "planten",
            "plantet",
            "geplant",
            "erwarte",
            "erwartest",
            "erwartet",
            "erwarten",
            "erwartete",
            "erwarteten",
            "glaube",
            "glaubst",
            "glaubt",
            "glauben",
            "glaubte",
            "glaubten",
            "geglaubt",
            "vermute",
            "vermutest",
            "vermutet",
            "vermuten",
            "vermutete",
            "vermuteten",
            "vorgeschlagen",
        ]
        .contains(&words[index])
        {
            return false;
        }
        // An adjective or noun in the actual subject/object is not a matrix
        // predicate ("the proposed plan", "the hope report").
        if index > start
            && [
                "the", "a", "an", "this", "that", "our", "your", "their", "his", "her", "my",
                "its", "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem",
                "einer", "eines", "unser", "unsere", "mein", "meine", "ihr", "ihre",
            ]
            .contains(&words[index - 1])
        {
            return false;
        }
        // Modifiers can separate a determiner from the noun "plan". Use the
        // same bounded nominal-subject proof as the terminal predicate.
        if (index + 1..predicate).any(|auxiliary| {
            [
                "is", "are", "was", "were", "has", "have", "ist", "sind", "wurde", "wurden",
            ]
            .contains(&words[auxiliary])
                && finite_subject_before(words, original, start, auxiliary)
        }) {
            return false;
        }
        // Preserve a proper name or its possessive rather than blacklisting
        // names such as Hope when the following action really is asserted.
        let proper_name = original[index]
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
            && (words.get(index + 1) == Some(&"s")
                || (index + 1 == predicate
                    && finite_subject_before(words, original, start, predicate)));
        !proper_name
    })
}

/// Require an affirmative decision assertion. Its infinitive/that-clause may
/// itself be negative ("decided not to ..."); that does not negate the decision.
pub(super) fn recorded_decision_quote_allowed(quote: &str) -> bool {
    // Expand only the complete affirmative contraction, never haven/not forms.
    // These temporary tokens do not rewrite the literal source or its quote.
    let lower = quote.to_lowercase().replace('’', "'");
    let words = assertion_words(&lower);
    let original = quote.replace('’', "'");
    let original_words = assertion_words(&original);
    // Bind German auxiliaries to the same clause, including ASR without commas.
    // Token counts also match the temporary two-token expansion of "we've".
    let clauses = assertion_clauses(&lower);
    let sentences = assertion_ranges(&lower, false);
    let german = words.iter().enumerate().find_map(|(index, word)| {
        if !["entscheiden", "beschließen", "beschlossen", "entschieden"].contains(word) {
            return None;
        }
        let clause = clauses.iter().find(|range| range.contains(&index))?;
        if unasserted_embedding(&words, &original_words, &sentences, index)
            || modal_after(&words, index, clause.end)
        {
            return None;
        }
        let subordinate = subordinate_start(&words, clause.start, index);
        let start = subordinate.unwrap_or(clause.start);
        let finite = subordinate.is_none()
            && ((index > start && words[index - 1] == "wir")
                || (index + 1 < clause.end && words[index + 1] == "wir"));
        let auxiliary = |word: &str| {
            [
                "habe", "haben", "hat", "hatte", "hatten", "ist", "war", "waren", "wurde", "wurden",
            ]
            .contains(&word)
        };
        let asserted = match *word {
            // These spellings can be finite plural verbs or bare infinitives.
            // A modal/intention followed by an infinitive is not a decision.
            "entscheiden" | "beschließen" => finite,
            "beschlossen" | "entschieden" => {
                !infinitive_after(&words, index, clause.end)
                    && (finite
                        || words[start..index].iter().any(|w| auxiliary(w))
                        // A finite subordinate auxiliary ends that same clause.
                        || (subordinate.is_some()
                            && index + 2 == clause.end
                            && auxiliary(words[index + 1])))
            }
            _ => false,
        };
        asserted.then_some((index, clause.end))
    });
    let english = words.iter().enumerate().find_map(|(index, word)| {
        let next = words.get(index + 1).copied();
        let clause = clauses.iter().find(|range| range.contains(&index))?;
        let subordinate = subordinate_start(&words, clause.start, index);
        let start = subordinate.unwrap_or(clause.start);
        if *word == "agreed" && matches!(next, Some("to" | "not")) {
            let asserted = (subordinate.is_none() && index == start)
                || finite_subject_before(&words, &original_words, start, index)
                || words[start..index]
                    .iter()
                    .any(|word| ["have", "has", "had", "is", "are", "was", "were"].contains(word));
            return (asserted
                && !unasserted_auxiliary_before(&words, start, index)
                && !unasserted_embedding(&words, &original_words, &sentences, index))
            .then_some(index + 1);
        }
        if *word != "we" {
            return None;
        }
        if matches!(next, Some("decide" | "decided")) {
            return ((next == Some("decided") || subordinate.is_none())
                && !unasserted_embedding(&words, &original_words, &sentences, index + 1))
            .then_some(index + 2);
        }
        if !matches!(next, Some("have" | "had")) {
            return None;
        }
        let mut predicate = index + 2;
        while predicate < index + 5
            && words
                .get(predicate)
                .is_some_and(|w| AFFIRMATIVE_PREDICATE_ADVERBS.contains(w))
        {
            predicate += 1;
        }
        (words.get(predicate) == Some(&"decided")
            && !unasserted_embedding(&words, &original_words, &sentences, predicate))
        .then_some(predicate + 1)
    });
    let assertion_end = if let Some((index, clause_end)) = german {
        if clause_end < words.len() {
            clause_end
        } else if let Some(negation) = words.windows(2).position(|p| p == ["nicht", "zu"]) {
            // ASR may omit the comma before an infinitive clause. Require an
            // intervening object and an actual infinitive after "nicht zu".
            if negation > index + 1 && words.len() > negation + 2 {
                negation
            } else {
                words.len()
            }
        } else if let Some(object) = words.windows(2).position(|p| p == ["nichts", "zu"]) {
            // "Nichts" is the infinitive's object, not a denial that a decision
            // was taken. Preserve that complement when ASR omits its comma.
            if object > index && words.len() > object + 2 {
                object
            } else {
                words.len()
            }
        } else {
            words.len()
        }
    } else if let Some(predicate_end) = english {
        let complement = &words[predicate_end..];
        if complement.starts_with(&["to"])
            || complement.starts_with(&["that"])
            || complement.starts_with(&["not", "to"])
        {
            predicate_end
        } else {
            words.len()
        }
    } else {
        return false;
    };
    // Conditions qualify the asserted decision even after its predicate. An
    // explicit English "check if"/"ask if" complement is an embedded question,
    // like "whether/ob", not a condition on taking the decision. Inspect every
    // marker independently so a later separate condition still rejects.
    if quote.contains('?')
        || words.iter().enumerate().any(|(index, word)| {
            let embedded_question = embedded_english_question(
                &words,
                index,
                (german.is_none() && english.is_some()).then_some(assertion_end),
            );
            ["if", "unless", "falls", "wenn", "sofern"].contains(word) && !embedded_question
        })
        || words[..assertion_end].iter().any(|word| {
            UNCERTAIN_TERMINAL_WORDS.contains(word)
                || ADDITIONAL_UNCERTAIN_TERMINAL_WORDS.contains(word)
        })
    {
        return false;
    }
    true
}

/// Require explicit withdrawal wording in one literal quote, not a negation,
/// conditional, question, unrelated return or a phrase assembled across sources.
pub(super) fn withdrawal_quote_allowed(quote: &str) -> bool {
    let lower = quote.to_lowercase().replace('’', "'");
    let tokenize = |s: &str| {
        s.split(|c: char| !c.is_alphabetic())
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let words = tokenize(&lower);
    if quote.contains('?')
        || words.iter().any(|word| {
            (word != "nicht" && UNCERTAIN_TERMINAL_WORDS.contains(&word.as_str()))
                || ADDITIONAL_UNCERTAIN_TERMINAL_WORDS.contains(&word.as_str())
        })
    {
        return false;
    }
    let clauses: Vec<_> = lower
        .split(['.', '!', ';', '\n', ','])
        .map(tokenize)
        .collect();
    if words.iter().any(|w| w == "nicht") {
        // "No longer needed" withdraws a requirement. A bare "nicht mehr" is
        // insufficient: "nicht mehr zurücknehmen" states the opposite intent.
        return words.iter().filter(|w| *w == "nicht").count() == 1
            && clauses.iter().any(|p| {
                p.len() >= 5
                    && ["ich", "wir"].contains(&p[0].as_str())
                    && ["brauche", "brauchen", "benötige", "benötigen"].contains(&p[1].as_str())
                    && p[p.len() - 2] == "nicht"
                    && p[p.len() - 1] == "mehr"
            });
    }
    let words = assertion_words(&lower);
    let original = quote.replace('’', "'");
    let original_words = assertion_words(&original);
    let clauses = assertion_clauses(&lower);
    let sentences = assertion_ranges(&lower, false);
    words.iter().enumerate().any(|(index, word)| {
        let Some(clause) = clauses.iter().find(|range| range.contains(&index)) else {
            return false;
        };
        let subordinate = subordinate_start(&words, clause.start, index);
        let start = subordinate.unwrap_or(clause.start);
        if infinitive_after(&words, index, clause.end)
            || unasserted_auxiliary_before(&words, start, index)
            || modal_after(&words, index, clause.end)
            || unasserted_embedding(&words, &original_words, &sentences, index)
        {
            return false;
        }
        let finite =
            subordinate.is_none() && finite_subject_before(&words, &original_words, start, index);
        let auxiliary = |v: &str| {
            [
                "habe", "haben", "hat", "hatte", "hatten", "ist", "sind", "war", "waren", "wurde",
                "wurden", "have", "has", "had", "am", "is", "are", "was", "were",
            ]
            .contains(&v)
        };
        let past = words[start..index].iter().any(|v| auxiliary(v))
            || (subordinate.is_some() && index + 2 == clause.end && auxiliary(words[index + 1]));
        match *word {
            "withdraw" | "withdraws" | "cancel" | "cancels" | "revoke" | "revokes" | "rescind"
            | "rescinds" | "widerrufe" | "widerruft" | "widerrief" | "widerriefen"
            | "storniere" | "stornieren" | "stornierte" | "stornierten" => finite,
            "withdrawn" | "withdrew" | "cancelled" | "canceled" | "revoked" | "rescinded"
            | "widerrufen" | "storniert" => finite || past,
            "zurückgezogen" | "zurückgenommen" => past,
            "ziehen" | "ziehe" | "zieht" | "nehmen" | "nehme" | "nimmt" => {
                finite
                    && words.get(clause.end.saturating_sub(1)) == Some(&"zurück")
                    && !words[index + 1..clause.end].iter().any(|v| {
                        SUBORDINATE_CLAUSE_MARKERS.contains(v)
                            || [
                                "kommen", "komme", "kommt", "gehen", "gehe", "geht", "kehre",
                                "kehren", "kehrt",
                            ]
                            .contains(v)
                    })
            }
            _ => false,
        }
    })
}

/// Compare explicit decimal quantities without rounding a partial value up.
fn completion_number(raw: &str) -> Option<(&str, &str)> {
    let mut pieces = raw.split(['.', ',']);
    let integer = pieces.next()?;
    let decimal = pieces.next();
    let fraction = decimal.unwrap_or("");
    if integer.is_empty()
        || !integer.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || decimal.is_some_and(str::is_empty)
        || pieces.next().is_some()
    {
        return None;
    }
    Some((
        integer.trim_start_matches('0'),
        fraction.trim_end_matches('0'),
    ))
}

/// Reject explicit partial progress, rather than percentages in an object's name.
/// This bounded evidence guard complements, rather than replaces, model judgement.
fn incomplete_quantity(text: &str) -> bool {
    let mut words = Vec::new();
    let mut start = None;
    for (at, ch) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if ch.is_alphabetic() {
            start.get_or_insert(at);
        } else if let Some(from) = start.take() {
            words.push((from, at, &text[from..at]));
        }
    }
    if words.windows(2).any(|pair| {
        (pair[0].2 == "half" && ["of", "the", "a"].contains(&pair[1].2))
            || (pair[0].2 == "zur" && ["hälfte", "haelfte"].contains(&pair[1].2))
            || (["ein", "zwei"].contains(&pair[0].2) && pair[1].2 == "drittel")
            || (["ein", "zwei", "drei"].contains(&pair[0].2) && pair[1].2 == "viertel")
    }) || words.windows(3).any(|part| {
        ((["one", "two"].contains(&part[0].2) && ["third", "thirds"].contains(&part[1].2))
            || (["one", "two", "three"].contains(&part[0].2)
                && ["quarter", "quarters"].contains(&part[1].2)))
            && part[2].2 == "of"
    }) {
        return true;
    }
    let left_number = |at: usize| {
        let prefix = text[..at].trim_end();
        let start = prefix
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit() || ['.', ','].contains(c))
            .last()
            .map_or(prefix.len(), |(at, _)| at);
        (start, prefix.len())
    };
    let right_number = |at: usize| {
        let suffix = &text[at..];
        let start = at + suffix.len() - suffix.trim_start().len();
        let end = text[start..]
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_digit() || ['.', ','].contains(c))
            .last()
            .map_or(start, |(at, c)| start + at + c.len_utf8());
        // A following sentence stop is not a decimal digit ("2/2.").
        (
            start,
            start + text[start..end].trim_end_matches(['.', ',']).len(),
        )
    };
    let mut ratios = text
        .match_indices('/')
        .map(|(at, _)| (at, at + 1))
        .collect::<Vec<_>>();
    ratios.extend(
        words
            .iter()
            .enumerate()
            .filter(|(_, w)| ["of", "von"].contains(&w.2))
            .map(|(index, w)| {
                // "out of" is one ratio operator; its numerator precedes
                // "out", not the immediately following "of" token.
                let start = if w.2 == "of"
                    && index > 0
                    && words[index - 1].2 == "out"
                    && text[words[index - 1].1..w.0]
                        .chars()
                        .all(char::is_whitespace)
                {
                    words[index - 1].0
                } else {
                    w.0
                };
                (start, w.1)
            }),
    );
    if ratios.into_iter().any(|(at, end)| {
        let (left, left_end) = left_number(at);
        let (right, right_end) = right_number(end);
        if left == left_end || right == right_end {
            return false;
        }
        // Q1/2026 and other embedded identifiers are not standalone quantities.
        if text[..left]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            return false;
        }
        // Date notation is not a progress ratio (including chained YYYY/MM/DD).
        let before = text[..left].trim_end();
        let mut prior = words.iter().rposition(|w| w.1 == before.len());
        // Keep the link to the predicate across adjacent quantity adverbs,
        // without crossing an object noun or a punctuation boundary.
        while let Some(index) = prior {
            if ![
                "exactly",
                "genau",
                "precisely",
                "exakt",
                "only",
                "nur",
                "just",
                "erst",
                "already",
                "bereits",
                "schon",
                "approximately",
                "circa",
                "roughly",
                "about",
                "around",
                "etwa",
                "ungefähr",
                "knapp",
                "vollständig",
                "fully",
            ]
            .contains(&words[index].2)
            {
                break;
            }
            prior = index.checked_sub(1).filter(|previous| {
                text[words[*previous].1..words[index].0]
                    .chars()
                    .all(char::is_whitespace)
            });
        }
        let preceding = prior.map(|index| &words[index]);
        if before.ends_with('/')
            || text[right_end..].trim_start().starts_with('/')
            || preceding
                .is_some_and(|w| ["on", "am", "at", "date", "datum", "dated", "vom"].contains(&w.2))
        {
            return false;
        }
        // Require partitive wording or a quantity immediately after a progress
        // predicate/preposition. A fiscal year or "report 1 of 2" names work;
        // it does not say that only part of that work was completed.
        let following = text[right_end..]
            .trim_start()
            .split(|c: char| !c.is_alphabetic())
            .next()
            .unwrap_or("");
        let partitive = [
            "of",
            "von",
            "der",
            "des",
            "part",
            "parts",
            "teil",
            "teile",
            "teilen",
            "item",
            "items",
            "step",
            "steps",
            "schritte",
            "schritten",
            "prozent",
            "percent",
            "per",
        ]
        .contains(&following);
        let after_progress = preceding.is_some_and(|w| {
            [
                "completed",
                "finished",
                "done",
                "have",
                "habe",
                "haben",
                "zu",
                "to",
            ]
            .contains(&w.2)
        });
        if !partitive && !after_progress {
            return false;
        }
        let numerator = completion_number(&text[left..left_end]);
        let denominator = completion_number(&text[right..right_end]);
        numerator.is_none() || numerator != denominator || denominator == Some(("", ""))
    }) {
        return true;
    }
    let mut units = text
        .match_indices(['%', '％', '٪'])
        .map(|(at, unit)| (at, at + unit.len()))
        .collect::<Vec<_>>();
    for (i, (at, end, word)) in words.iter().enumerate() {
        if ["prozent", "prozente", "percent"].contains(word) {
            units.push((*at, *end));
        } else if *word == "per" && words.get(i + 1).is_some_and(|w| w.2 == "cent") {
            units.push((*at, words[i + 1].1));
        }
    }
    units.into_iter().any(|(at, end)| {
        // A hyphen can join a number to a word unit in an object name,
        // e.g. "the 50-percent-discount report".
        let prefix = text[..at].trim_end().trim_end_matches('-').trim_end();
        let number_at = prefix
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit() || ['.', ','].contains(c))
            .last()
            .map_or(prefix.len(), |(at, _)| at);
        let before = prefix[..number_at].trim_end();
        let suffix = text[end..].trim_start_matches(|c: char| c.is_whitespace() || c == '-');
        let next = suffix
            .split(|c: char| !c.is_alphabetic())
            .next()
            .unwrap_or("");
        // Only an explicit object introduction plus a following descriptor
        // exempts a percentage. An unknown following word alone may be an
        // adverb ("50% today"), never evidence of an object name.
        if !next.is_empty()
            && words
                .iter()
                .rev()
                .find(|w| w.1 == before.len())
                .is_some_and(|w| {
                    [
                        "the", "a", "an", "der", "die", "das", "den", "dem", "des", "zur", "zum",
                    ]
                    .contains(&w.2)
                })
            && ![
                "of",
                "von",
                "der",
                "des",
                "die",
                "den",
                "complete",
                "completed",
                "done",
                "finished",
                "abgeschlossen",
                "erledigt",
                "fertig",
                "already",
                "bereits",
                "so",
                "in",
                "by",
            ]
            .contains(&next)
        {
            return false;
        }
        // Compare decimal digits exactly; floating rounding must not turn
        // 99.9999999999999999999 percent into a completed task.
        if completion_number(&prefix[number_at..]) != Some(("100", "")) {
            return true;
        }
        if before.ends_with(['-', '–', '—', '/']) {
            return true;
        }
        words
            .iter()
            .rev()
            .find(|w| w.1 == before.len())
            .is_some_and(|w| {
                ["to", "bis", "and", "und", "of", "von"].contains(&w.2)
                    && text[..w.0]
                        .trim_end()
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_ascii_digit())
            })
    })
}

/// Explicit past or completed-state wording, with negation and uncertainty rejected.
pub(super) fn completion_quote_allowed(quote: &str) -> bool {
    // Expand only complete affirmative first-person contractions for screening.
    // Other apostrophe tokens still split as before, preserving haven/not guards;
    // original sources and evidence are never normalized or rewritten.
    let lower = quote.to_lowercase().replace('’', "'");
    let words = assertion_words(&lower);
    let original = quote.replace('’', "'");
    let original_words = assertion_words(&original);
    // German "fast" means almost; English "fast" can describe a fully
    // completed action. Require a German predicate before treating it as partial.
    let german_partial = words.contains(&"fast")
        && words
            .iter()
            .any(|word| ["habe", "haben", "ist"].contains(word));
    if quote.contains('?')
        || german_partial
        || incomplete_quantity(&lower)
        || words.iter().any(|word| {
            UNCERTAIN_TERMINAL_WORDS.contains(word)
                || ADDITIONAL_UNCERTAIN_TERMINAL_WORDS.contains(word)
                || [
                    "beinahe",
                    "nahezu",
                    "teilweise",
                    "weitgehend",
                    "almost",
                    "nearly",
                    "partly",
                    "partially",
                    "mostly",
                ]
                .contains(word)
        })
    {
        return false;
    }
    let clauses = assertion_clauses(&lower);
    let sentences = assertion_ranges(&lower, false);
    let phrase = |sequence: &[&str]| {
        clauses.iter().any(|range| {
            words[range.clone()]
                .windows(sequence.len())
                .enumerate()
                .any(|(offset, part)| {
                    part == sequence
                        && !unasserted_embedding(
                            &words,
                            &original_words,
                            &sentences,
                            range.start + offset + sequence.len() - 1,
                        )
                })
        })
    };
    if [
        &["ist", "erledigt"][..],
        &["ist", "erteilt"],
        &["ist", "abgeschlossen"],
        &["is", "complete"],
        &["is", "done"],
        &["has", "been", "approved"],
    ]
    .iter()
    .any(|sequence| phrase(sequence))
    {
        return true;
    }
    let english = [
        "sent",
        "emailed",
        "completed",
        "finished",
        "submitted",
        "approved",
        "delivered",
        "uploaded",
        "published",
    ];
    if words.iter().enumerate().any(|(index, word)| {
        if !["i", "we"].contains(word) {
            return false;
        }
        let Some(clause) = clauses.iter().find(|range| range.contains(&index)) else {
            return false;
        };
        let mut predicate = index + 1;
        if words.get(predicate) == Some(&"have") {
            predicate += 1;
            while predicate < (index + 5).min(clause.end)
                && words
                    .get(predicate)
                    .is_some_and(|w| AFFIRMATIVE_PREDICATE_ADVERBS.contains(w))
            {
                predicate += 1;
            }
        }
        predicate < clause.end
            && english.contains(&words[predicate])
            && !unasserted_embedding(&words, &original_words, &sentences, predicate)
    }) {
        return true;
    }
    let german = [
        "erledigt",
        "freigegeben",
        "abgeschlossen",
        "verschickt",
        "gesendet",
        "eingereicht",
        "geliefert",
        "hochgeladen",
        "veröffentlicht",
    ];
    words.iter().enumerate().any(|(index, word)| {
        if !german.contains(word) {
            return false;
        }
        let Some(clause) = clauses.iter().find(|range| range.contains(&index)) else {
            return false;
        };
        // A perfect infinitive states an intention, not its completion.
        if infinitive_after(&words, index, clause.end)
            || modal_after(&words, index, clause.end)
            || unasserted_embedding(&words, &original_words, &sentences, index)
        {
            return false;
        }
        let subordinate = subordinate_start(&words, clause.start, index);
        let start = subordinate.unwrap_or(clause.start);
        words[start..index]
            .iter()
            .any(|w| ["habe", "haben"].contains(w))
            || (subordinate.is_some()
                && index + 2 == clause.end
                && ["habe", "haben"].contains(&words[index + 1]))
    })
}

/// Share validator status rules with constrained output schemas; new entities
/// additionally exclude terminal states that require an existing target.
pub(super) fn allowed_statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "fact" => &["recorded", "unclear"],
        "decision" => &["recorded", "withdrawn"],
        "task" => &["open", "completed", "withdrawn"],
        "agenda" => &["touched", "discussed", "unclear"],
        "impulse" => &["open", "discussed", "unclear"],
        _ => &[],
    }
}

/// Detect an explicitly missing commitment so it cannot be promoted into an assigned task.
pub(super) fn missing_pledge(quote: &str) -> bool {
    let text = quote.to_lowercase();
    [
        "fehlt",
        "fehlen",
        "noch offen",
        "bleibt offen",
        "keine zusage",
        "keine verbindliche",
        "noch keine",
        "ohne zusage",
        "no commitment",
        "not committed",
        "not yet assigned",
    ]
    .iter()
    .any(|s| text.contains(s))
}
/// Missing promises outside the checked subject still block new work; agenda screening is separate.
fn task_missing_pledge(quote: &str) -> bool {
    missing_pledge(&crate::validation::commitment_assertion_text(quote))
}

/// Require a positive obligation in the literal quote; a wish or missing approval is insufficient.
pub(super) fn task_quote_allowed(quote: &str) -> bool {
    // Temporary classification text only: literal evidence and saved IDs are unchanged.
    let text = quote.to_lowercase().replace('’', "'");
    !task_missing_pledge(quote)
        && crate::validation::has_new_task_commitment(quote)
        && !text.contains('?')
        && (crate::validation::task_pledge_end(&text, crate::validation::GERMAN_TASK_PLEDGES)
            .is_some()
            || crate::validation::task_pledge_end(&text, crate::validation::ENGLISH_TASK_PLEDGES)
                .is_some())
}
