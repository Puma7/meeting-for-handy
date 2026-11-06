pub(super) use super::assertions::{
    completion_quote_allowed, recorded_decision_quote_allowed, withdrawal_quote_allowed,
};
use super::model::*;
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
                        // Context withdrawal hides an AI draft from the model,
                        // but a still-proven terminal identity remains a private,
                        // read-only replay witness. Never revive or expose it.
                        let excluded_terminal_witness = terminal_identity
                            && !e.evidence.is_empty()
                            && e.evidence.iter().all(|evidence| {
                                !next.superseded_sources.contains(&evidence.source_id)
                                    && next.originals.iter().any(|source| {
                                        source.id == evidence.source_id
                                            && source.text.contains(&evidence.quote)
                                    })
                            });
                        (!next.context_excluded_ids.contains(&e.id) || excluded_terminal_witness)
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
