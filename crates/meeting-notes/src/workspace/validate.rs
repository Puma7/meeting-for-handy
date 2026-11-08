pub(super) use super::assertions::{
    completion_quote_allowed, recorded_decision_quote_allowed, withdrawal_quote_allowed,
};
use super::matching::{
    explicit_owned_completion, same_question, shares_topic, task_completion_matches,
    task_subject_matches,
};
use super::model::*;
use anyhow::{ensure, Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};

/// Validate a complete model bundle against its request and sources before mutating any state.
pub(crate) fn apply(
    state: &mut WorkspaceState,
    request: &Request,
    json: &str,
) -> Result<Vec<Event>> {
    let response: Response = serde_json::from_str(json)
        .context("Ungültige oder doppelte Felder in der Meetingantwort.")?;
    apply_response(state, request, response)
}

fn apply_response(
    state: &mut WorkspaceState,
    request: &Request,
    response: Response,
) -> Result<Vec<Event>> {
    ensure!(
        response.contract_version == super::CONTRACT_VERSION
            && response.request_id == request.request_id,
        "Die Meetingantwort gehört nicht zu diesem Auftrag."
    );
    if response.more_changes_possible || response.changes.len() > 20 {
        return Err(crate::transport::BudgetError::WorkspaceChanges.into());
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
    let required_question =
        request.purpose == "reconcile" && initial_question_needed(state, request);
    let required_question_evidence = if required_question {
        initial_question_evidence(state, request)
    } else {
        Vec::new()
    };
    let explicit_question_in_window = request.purpose == "reconcile"
        && request
            .transcript_sources
            .iter()
            .filter(|source| request.required_source_ids().contains(&source.id))
            .flat_map(|source| super::quote_options(&source.text))
            .any(|quote| explicit_question(&quote));
    let mut changes = response.changes;
    if request.purpose == "tasks" {
        add_literal_completions(state, request, &mut changes);
    }
    let mut split_budget = 20 - changes.len();
    let mut changes = VecDeque::from(changes);
    while let Some(mut c) = changes.pop_front() {
        if let Some(text) = c.text.as_mut() {
            *text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            ensure!(
                text.chars().count() <= request.max_title_chars(),
                "Ein Ergebnistitel ist zu lang; einen einzelnen knappen Punkt formulieren."
            );
        }
        acknowledged_open_topic |= request.purpose == "reconcile"
            && c.kind == "agenda"
            && ["touched", "unclear"].contains(&c.status.as_str())
            && c.evidence
                .iter()
                .any(|evidence| explicit_open_topic(&evidence.quote));
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
        // Check every submitted citation before selecting narrower literal witnesses.
        ensure!(
            c.evidence.iter().all(|evidence| {
                (8..=600).contains(&evidence.quote.chars().count())
                    && request.transcript_sources.iter().any(|source| {
                        source.id == evidence.source_id && source.text.contains(&evidence.quote)
                    })
            }),
            "Eine Änderung enthält ein nicht wörtliches Originalzitat."
        );
        if let Some((decision, observation)) = fact_decision_parts(request, &c)
            .filter(|(_, observation)| observation.is_none() || split_budget > 0)
        {
            let split = observation.is_some();
            if let Some(observation) = observation {
                split_budget -= 1;
                changes.push_front(Change {
                    proposal_id: format!("p_native_{}", uuid::Uuid::new_v4().simple()),
                    kind: "fact".into(),
                    target_id: None,
                    target_revision: None,
                    text: Some(observation.quote.clone()),
                    status: c.status.clone(),
                    attention: c.attention.clone(),
                    owner: None,
                    due: None,
                    evidence: vec![observation],
                    related_entity_ids: vec![],
                });
            }
            if (split || request.language == "auto")
                && decision.quote.chars().count() <= request.max_title_chars()
            {
                c.text = Some(decision.quote.clone());
            }
            c.kind = "decision".into();
            c.evidence = vec![decision];
        }
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
                state.entities.iter().any(|current| current == old)
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
            // An unrelated but literal quote cannot rewrite an existing open question.
            // A discarded no-op carries neither a new status nor additional citations.
            if request.purpose == "reconcile"
                && request.language == "auto"
                && c.kind == "impulse"
                && c.status == "unclear"
                && c.text.is_none()
                && !c.evidence.is_empty()
                && c.related_entity_ids.is_empty()
                && !c.evidence.iter().any(|e| {
                    request.required_source_ids().contains(&e.source_id)
                        && question_topic_matches(&old.text, &e.quote)
                })
            {
                aliases.insert(c.proposal_id, old.id.clone());
                continue;
            }
            if !targets.insert(old.id.clone()) {
                let applied = next
                    .entities
                    .iter()
                    .find(|entity| entity.id == old.id)
                    .context("Der bereits abgeglichene Eintrag fehlt.")?;
                // A second copy of an applied status cannot change its
                // text, metadata, attention, or evidence.
                ensure!(
                    c.status == applied.status
                        && c.attention == applied.attention
                        && c.text.as_ref().is_none_or(|text| text == &applied.text)
                        && c.owner
                            .as_ref()
                            .is_none_or(|owner| Some(owner) == applied.owner.as_ref())
                        && c.due
                            .as_ref()
                            .is_none_or(|due| Some(due) == applied.due.as_ref())
                        && c.related_entity_ids.is_empty()
                        && !c.evidence.is_empty()
                        && c.evidence
                            .iter()
                            .all(|item| applied.evidence.contains(item)),
                    "Die Modellantwort enthält widersprüchliche Änderungen für denselben Eintrag."
                );
                aliases.insert(c.proposal_id, old.id.clone());
                continue;
            }
            // An exact echo is unchanged metadata. Its old witness is still
            // required, but the new status quote does not have to repeat it.
            if c.kind == "task" && c.text.is_none() {
                if c.owner == old.owner {
                    c.owner = None;
                }
                if c.due == old.due {
                    c.due = None;
                }
            }
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
            ensure!(
                !c.evidence.iter().any(|e| task_missing_pledge(&e.quote)),
                "Eine fehlende Zusage ist keine Aufgabe."
            );
            if request.purpose == "tasks" {
                let title = c.text.as_deref().unwrap_or_default();
                c.evidence = assertion_witnesses(request, &c.evidence, title, true)?;
                if c.due.is_none() {
                    let mut weekdays = c
                        .evidence
                        .iter()
                        .filter_map(|e| crate::validation::explicit_task_weekday(&e.quote));
                    if let Some(first) = weekdays.next() {
                        if weekdays.all(|other| other.eq_ignore_ascii_case(first)) {
                            c.due = Some(first.to_owned());
                        }
                    }
                }
            }
            ensure!(
                c.text.as_ref().is_some_and(|title| !title.contains(['?', '？'])),
                "Eine neue Aufgabe benötigt einen Handlungstitel. Offene Fragen gehören zu den Impulsen."
            );
            ensure!(c.evidence.iter().any(|e|request.required_source_ids().contains(&e.source_id)),"Eine neue Aufgabe benötigt einen Beleg aus dem aktuellen Quellenfenster; ältere Aufgaben nicht duplizieren.");
            ensure!(c.evidence.iter().any(|e|task_quote_allowed(&e.quote)) && !c.evidence.iter().any(|e|task_missing_pledge(&e.quote)),"Eine neue Aufgabe benötigt eine ausdrückliche Übernahme oder Zuweisung. Eine fehlende Zusage ist keine Aufgabe.");
        }
        let fresh = |e: &crate::Evidence| request.required_source_ids().contains(&e.source_id);
        if let Some(old) = old {
            if c.kind == "fact" {
                ensure!(
                    !decision_backed_fact(old),
                    "Ein als Fakt gespeicherter Beschluss bleibt als Originaleintrag erhalten."
                );
                ensure!(
                    c.evidence
                        .iter()
                        .any(|e| { fresh(e) && shares_topic(&old.text, &e.quote) }),
                    "Eine vorhandene Beobachtung benötigt einen aktuellen Beleg zum selben Thema."
                );
            }
            if c.kind == "task" && c.text.as_ref().is_some_and(|title| title != &old.text) {
                ensure!(
                    c.status == "open"
                        && c.text.as_ref().is_some_and(|title| task_subject_matches(old, title))
                        && c.evidence.iter().any(|e| {
                            fresh(e) && task_quote_allowed(&e.quote)
                                && c.text.as_ref().is_some_and(|title| shares_topic(title, &e.quote))
                        })
                        && !c.evidence.iter().any(|e| task_missing_pledge(&e.quote)),
                    "Eine vorhandene Aufgabe darf nur mit einer aktuellen Zusage zum selben Thema umformuliert werden."
                );
            }
            if c.kind == "impulse" && c.status == "discussed" && old.status != "discussed" {
                ensure!(
                    c.evidence
                        .iter()
                        .any(|e| { fresh(e) && impulse_answer_allowed(&old.text, &e.quote) }),
                    "Der Impuls bleibt offen. Eine aktuelle Antwort zum selben Thema fehlt."
                );
            }
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
        if request.purpose == "facts" && c.kind == "decision" && c.status == "recorded" {
            c.evidence = assertion_witnesses(request, &c.evidence, &text, false)?;
        }
        if request.purpose == "reconcile"
            && c.kind == "impulse"
            && c.evidence
                .iter()
                .any(|e| question_topic_matches(&text, &e.quote))
        {
            c.evidence
                .retain(|e| question_topic_matches(&text, &e.quote));
        }
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
                c.evidence.iter().any(|e| completion_quote_allowed(&e.quote)
                    && (request.purpose != "tasks"
                        || (fresh(e)
                            && old.is_some_and(|task| task_completion_matches(task, &e.quote))))),
                "Eine Zusage ist keine Erledigung."
            );
        }
        if c.status == "withdrawn" {
            ensure!(
                c.evidence
                    .iter()
                    .any(|e| fresh(e) && withdrawal_quote_allowed(&e.quote)),
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
                        let same_impulse = c.kind == "impulse"
                            && e.kind == "impulse"
                            && same_question(e, &text, &c.evidence);
                        // A second proposal for the same literal pledge must
                        // not create an extra task merely by omitting metadata.
                        let same_task_witness = c.kind == "task"
                            && e.kind == "task"
                            && e.text == text
                            && !c.evidence.is_empty()
                            && e.evidence == c.evidence;
                        let same_decision_witness = c.kind == "decision"
                            && (terminal_identity
                                || (e.status == "recorded"
                                    && !c.evidence.is_empty()
                                    && c.evidence.iter().all(|item| e.evidence.contains(item))));
                        // A fresh quote may repeat the same observation without
                        // changing the displayed entry or replacing its original.
                        let repeated_fact = c.kind == "fact"
                            && e.status == c.status
                            && e.text
                                .trim_end_matches(['.', '!'])
                                .eq_ignore_ascii_case(text.trim_end_matches(['.', '!']));
                        (!next.context_excluded_ids.contains(&e.id) || excluded_terminal_witness)
                            && e.kind == c.kind
                            && (e.text == text
                                || same_impulse
                                || repeated_fact
                                || same_decision_witness)
                            && (e.owner == c.owner
                                || ((terminal_identity || same_task_witness) && c.owner.is_none()))
                            && (e.due == c.due
                                || ((terminal_identity || same_task_witness) && c.due.is_none()))
                            && (e.evidence == c.evidence
                                || terminal_identity
                                || same_decision_witness
                                || same_impulse
                                || repeated_fact)
                    })
                    .map(|e| e.id.clone())
            })
            .unwrap_or_else(|| format!("e_{}", uuid::Uuid::new_v4().simple()));
        if c.kind == "impulse" && old.is_none() && !next.entities.iter().any(|e| e.id == target) {
            // Repeating a literal question gives us an exact witness, even if several
            // unrelated topics in the window also have qualifying open language.
            if text.ends_with(['?', '？']) {
                let mut direct_sources = request
                    .transcript_sources
                    .iter()
                    .filter(|source| {
                        request.required_source_ids().contains(&source.id)
                            && source.text.contains(&text)
                    })
                    .peekable();
                if direct_sources.peek().is_some() {
                    ensure!(
                        direct_sources.any(|source| c.evidence.iter().any(|e| {
                            e.source_id == source.id
                                && (e.quote.contains(&text) || shares_topic(&text, &e.quote))
                        })),
                        "Eine wörtlich wiederholte Frage braucht einen passenden Beleg aus ihrem Originalabschnitt."
                    );
                }
            }
            ensure!(
                c.evidence.iter().any(|e| fresh(e) && if explicit_question_in_window {
                    explicit_question(&e.quote)
                } else {
                    new_impulse_evidence_allowed(&e.quote)
                }),
                "Ein neuer Impuls benötigt einen aktuellen Beleg für die offene Frage. Eine reine Zusage oder Entscheidung reicht dafür nicht."
            );
        }
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
    ensure!(
        !required_question
            || next.entities.iter().any(|entity| entity.kind == "impulse"
                && !next.context_excluded_ids.contains(&entity.id)
                && entity
                    .evidence
                    .iter()
                    .any(|evidence| required_question_evidence.contains(evidence))),
        "Der Frageimpuls muss das ausdrücklich offene Thema mit dessen Originalbeleg aufgreifen."
    );
    ensure!(!(acknowledged_open_topic || required_question) || next.entities.iter().any(|entity|
        entity.kind == "impulse" && !next.context_excluded_ids.contains(&entity.id)),
        "Ein als offen belegter Agendapunkt benötigt den geforderten Frageimpuls; die Antwort wird vollständig verworfen.");
    next.events.extend(events.clone());
    if request.purpose == "tasks" && next.entities.len() > state.entities.len() {
        events.extend(complete_new_tasks(
            &mut next,
            request,
            state.entities.len(),
        )?);
    }
    *state = next;
    Ok(events)
}

/// Keep an explicit decision separate from an observation cited in the same fact.
/// Only repair new facts backed by a single unambiguous, current decision sentence.
fn fact_decision_parts(
    request: &Request,
    change: &Change,
) -> Option<(crate::Evidence, Option<crate::Evidence>)> {
    if request.purpose != "facts"
        || change.kind != "fact"
        || change.status != "recorded"
        || change.target_id.is_some()
        || change.target_revision.is_some()
    {
        return None;
    }
    let title = change.text.as_deref()?;
    let mut decision = None;
    let mut observation = None;
    for citation in &change.evidence {
        let source = request
            .transcript_sources
            .iter()
            .find(|s| s.id == citation.source_id)?;
        for sentence in super::evidence_sentences(&source.text)
            .filter(|sentence| {
                citation.quote.contains(sentence) || sentence.contains(&citation.quote)
            })
            .filter(|sentence| shares_topic(title, sentence))
        {
            let literal = crate::Evidence {
                source_id: source.id.clone(),
                quote: sentence.into(),
            };
            if recorded_decision_quote_allowed(sentence) {
                if !request.required_source_ids().contains(&source.id)
                    || !(8..=600).contains(&sentence.chars().count())
                    || decision
                        .as_ref()
                        .is_some_and(|previous| previous != &literal)
                {
                    return None;
                }
                decision = Some(literal);
            } else {
                if observation
                    .as_ref()
                    .is_some_and(|previous| previous != &literal)
                {
                    return None;
                }
                observation = Some(literal);
            }
        }
    }
    let decision = decision?;
    if let Some(other) = &observation {
        // Keep a translated result in its selected language. For a mixed quote
        // the common explicit openings also identify German or English.
        let opening = decision.quote.split_whitespace().next().unwrap_or_default();
        let literal_language = request.language == "auto"
            || (decision.source_id == other.source_id
                && (request.language == "de" && opening.eq_ignore_ascii_case("wir")
                    || request.language == "en" && opening.eq_ignore_ascii_case("we")));
        if !literal_language
            || [&decision, other].into_iter().any(|item| {
                !(8..=request.max_title_chars().min(600)).contains(&item.quote.chars().count())
            })
        {
            return None;
        }
    }
    Some((decision, observation))
}

/// A local second pass can close a newly created task from a later original
/// in the same update. It goes through the same status validation as model output.
fn complete_new_tasks(
    state: &mut WorkspaceState,
    request: &Request,
    first_new: usize,
) -> Result<Vec<Event>> {
    let fresh: Vec<_> = request
        .transcript_sources
        .iter()
        .filter(|source| request.required_source_ids().contains(&source.id))
        .cloned()
        .collect();
    let mut followup = Request::new(
        request.request_id.clone(),
        &state.context,
        state,
        state.entities[first_new..].to_vec(),
        &fresh,
        request.transcript_sources.clone(),
    );
    followup.purpose = "tasks".into();
    let mut changes = Vec::new();
    add_literal_completions(state, &followup, &mut changes);
    if changes.is_empty() {
        return Ok(Vec::new());
    }
    let overview = state.overview_refs.clone();
    let events = apply_response(
        state,
        &followup,
        Response {
            contract_version: super::CONTRACT_VERSION.into(),
            request_id: followup.request_id.clone(),
            changes,
            overview_refs: Vec::new(),
            unresolved_context_ids: Vec::new(),
            more_changes_possible: false,
        },
    )?;
    state.overview_refs = overview;
    Ok(events)
}

/// A mislabeled decision remains an original observation. Later observations
/// and explicit decisions use their own identities instead of replacing it.
pub(super) fn decision_backed_fact(entity: &Entity) -> bool {
    entity.evidence.iter().any(|e| {
        super::evidence_sentences(&e.quote).any(|sentence| {
            shares_topic(&entity.text, sentence) && recorded_decision_quote_allowed(sentence)
        })
    })
}

/// A sentence may only witness its own assertion, even when the model cited
/// the surrounding transcript. Restore a clipped date from that literal sentence.
fn assertion_witnesses(
    request: &Request,
    submitted: &[crate::Evidence],
    title: &str,
    task: bool,
) -> Result<Vec<crate::Evidence>> {
    let allowed = if task {
        task_quote_allowed
    } else {
        recorded_decision_quote_allowed
    };
    let task_matches = |quote: &str| super::matching::task_title_matches(title, quote);
    let matches = |quote: &str| {
        if task {
            task_matches(quote)
        } else {
            shares_topic(title, quote)
        }
    };
    let mut candidates = Vec::new();
    let mut competing_topic = false;
    for citation in submitted {
        let Some(source) = request
            .transcript_sources
            .iter()
            .find(|source| source.id == citation.source_id)
        else {
            continue;
        };
        for sentence in super::evidence_sentences(&source.text).filter(|sentence| {
            citation.quote.contains(sentence) || sentence.contains(&citation.quote)
        }) {
            if !allowed(sentence) {
                competing_topic |= matches(sentence);
                continue;
            }
            for excerpt in super::assertion_excerpts(sentence, &allowed)
                .into_iter()
                .filter(|excerpt| {
                    citation.quote.contains(excerpt) || excerpt.contains(&citation.quote)
                })
            {
                let evidence = crate::Evidence {
                    source_id: source.id.clone(),
                    quote: excerpt.to_owned(),
                };
                if !candidates.contains(&evidence) {
                    candidates.push(evidence);
                }
            }
        }
    }
    if candidates.iter().any(|e| matches(&e.quote)) {
        candidates.retain(|e| matches(&e.quote));
    } else if competing_topic || request.language == "auto" {
        candidates.clear();
    }
    ensure!(
        candidates.len() <= 6 && candidates.iter().any(|e| request.required_source_ids().contains(&e.source_id)),
        "Die Aussage benötigt einen aktuellen Originalsatz zur selben Zusage oder zum selben Beschluss."
    );
    Ok(candidates)
}

pub(super) fn question_topic_matches(title: &str, quote: &str) -> bool {
    shares_topic(title, quote) || {
        // Chinese text often has no spaces. Require a contiguous four-character
        // Han phrase rather than matching isolated, frequently repeated characters.
        let chars: Vec<_> = title.chars().collect();
        chars.windows(4).any(|part| {
            part.iter().all(|c| ('\u{3400}'..='\u{9fff}').contains(c))
                && quote.contains(&part.iter().collect::<String>())
        })
    }
}

/// Recover a missed completion only from a fresh, literal report by the
/// named owner, with a matching action and exactly one possible target.
fn add_literal_completions(state: &WorkspaceState, request: &Request, changes: &mut Vec<Change>) {
    for source in request
        .transcript_sources
        .iter()
        .filter(|source| request.required_source_ids().contains(&source.id))
    {
        for quote in source
            .text
            .split_inclusive(['.', '!', '?', '\n'])
            .map(str::trim)
        {
            if changes.len() == 20 {
                return;
            }
            if !(8..=600).contains(&quote.chars().count()) || !completion_quote_allowed(quote) {
                continue;
            }
            let mut matches = state.entities.iter().filter(|entity| {
                entity.kind == "task"
                    && !state.context_excluded_ids.contains(&entity.id)
                    && explicit_owned_completion(entity, quote)
            });
            let Some(task) = matches.next() else {
                continue;
            };
            if matches.next().is_some()
                || task.status != "open"
                || task.human_locked
                || !request.state.contains(task)
                || changes
                    .iter()
                    .any(|c| c.target_id.as_ref() == Some(&task.id))
                || task.evidence.iter().any(|evidence| {
                    evidence.source_id == source.id
                        || request.transcript_sources.iter().any(|original| {
                            original.id == evidence.source_id
                                && original.start_ms >= source.start_ms
                        })
                })
            {
                continue;
            }
            changes.push(Change {
                proposal_id: format!("p_native_{}", uuid::Uuid::new_v4().simple()),
                kind: "task".into(),
                target_id: Some(task.id.clone()),
                target_revision: Some(task.revision),
                text: None,
                status: "completed".into(),
                attention: task.attention.clone(),
                owner: None,
                due: None,
                evidence: vec![crate::Evidence {
                    source_id: source.id.clone(),
                    quote: quote.to_owned(),
                }],
                related_entity_ids: vec![],
            });
        }
    }
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

/// Detect explicitly unresolved topics even when ASR supplies no question mark.
pub(super) fn explicit_open_topic(quote: &str) -> bool {
    let text = quote.to_lowercase().replace('’', "'");
    missing_pledge(quote)
        || [
            "ich weiß nicht",
            "ich weiß auch nicht",
            "ich weiss nicht",
            "wir wissen nicht",
            "ist unklar",
            "sind unklar",
            "noch zu klären",
            "noch geklärt werden",
            "noch nicht freigegeben",
            "still has no approval",
            "is still unapproved",
            "i don't know",
            "we don't know",
            "i do not know",
            "we do not know",
            "not sure",
            "is unclear",
            "are unclear",
            "still to clarify",
            "мы ещё не знаем",
            "мы еще не знаем",
            "я не знаю",
            "пока не ясно",
            "还没有确定",
            "尚未确定",
            "还不清楚",
            "仍不清楚",
        ]
        .iter()
        .any(|marker| text.contains(marker))
}

/// Prefer a question or a literal signal that information is still missing.
pub(super) fn explicit_question(quote: &str) -> bool {
    quote.contains(['?', '？']) || explicit_open_topic(quote)
}

/// A closed decision or explicit pledge alone is insufficient for a new question.
pub(super) fn new_impulse_evidence_allowed(quote: &str) -> bool {
    explicit_question(quote)
        || (!task_quote_allowed(quote) && !recorded_decision_quote_allowed(quote))
}

pub(super) fn impulse_answer_allowed(title: &str, quote: &str) -> bool {
    let lower = quote.to_lowercase();
    !quote.contains(['?', '？'])
        && !explicit_open_topic(quote)
        && ![
            "weiter offen",
            "bleibt offen",
            "noch offen",
            "still open",
            "still unassigned",
            "not assigned",
            "no approval",
            "noch nicht",
            "has not",
            "have not",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        && shares_topic(title, quote)
}

/// An acknowledged fresh open topic requires an initial question, independently of ASR punctuation.
pub(super) fn initial_question_needed(state: &WorkspaceState, request: &Request) -> bool {
    !initial_question_evidence(state, request).is_empty()
}

/// Bind the initial question to the literal open-topic witnesses, not an unrelated suggestion.
pub(super) fn initial_question_evidence(
    state: &WorkspaceState,
    request: &Request,
) -> Vec<crate::Evidence> {
    if state
        .entities
        .iter()
        .any(|entity| entity.kind == "impulse" && !state.context_excluded_ids.contains(&entity.id))
    {
        return Vec::new();
    }
    let mut witnesses = Vec::new();
    for entity in state.entities.iter().filter(|entity| {
        !state.context_excluded_ids.contains(&entity.id)
            && ["fact", "agenda"].contains(&entity.kind.as_str())
    }) {
        for evidence in &entity.evidence {
            if explicit_open_topic(&evidence.quote)
                && request.required_source_ids().contains(&evidence.source_id)
                && request.transcript_sources.iter().any(|source| {
                    source.id == evidence.source_id && source.text.contains(&evidence.quote)
                })
                && !witnesses.contains(evidence)
            {
                witnesses.push(evidence.clone());
            }
        }
    }
    if witnesses.is_empty() {
        return witnesses;
    }
    // If the transcript itself asks a question and explicitly leaves that
    // same topic open, prefer that question to neighboring tentative ideas.
    // An acknowledged indirect question still works when no such pair exists.
    let mut direct = Vec::new();
    for source in request
        .transcript_sources
        .iter()
        .filter(|source| request.required_source_ids().contains(&source.id))
    {
        for question in super::evidence_sentences(&source.text).filter(|quote| {
            quote.ends_with(['?', '？']) && (8..=600).contains(&quote.chars().count())
        }) {
            if super::evidence_sentences(&source.text).any(|line| {
                !line.ends_with(['?', '？'])
                    && explicit_open_topic(line)
                    && question_topic_matches(question, line)
            }) {
                direct.push(crate::Evidence {
                    source_id: source.id.clone(),
                    quote: question.to_owned(),
                });
            }
        }
    }
    if direct.is_empty() {
        return witnesses;
    }
    // A shorter fact quote about that same question remains a valid witness.
    witnesses.retain(|e| {
        direct
            .iter()
            .any(|q| q.source_id == e.source_id && question_topic_matches(&q.quote, &e.quote))
    });
    for question in direct {
        if !witnesses.contains(&question) {
            witnesses.push(question);
        }
    }
    witnesses
}
/// Missing promises outside the checked subject still block new work; agenda screening is separate.
fn task_missing_pledge(quote: &str) -> bool {
    missing_pledge(&crate::validation::commitment_assertion_text(quote))
}

/// Require a positive obligation in the literal quote; a wish or missing approval is insufficient.
pub(super) fn task_quote_allowed(quote: &str) -> bool {
    // Never hide a refusal, condition or hedge in text before a potential speaker colon.
    let binding = !task_missing_pledge(quote) && crate::validation::has_new_task_commitment(quote);
    // Temporary classification text only: literal evidence and saved IDs are unchanged.
    let quote = super::assertions::assignment_text(quote);
    let text = quote.to_lowercase().replace('’', "'");
    super::assertions::assignment_prefix_allowed(quote)
        && binding
        && !text.contains('?')
        && (crate::validation::task_pledge_end(&text, crate::validation::GERMAN_TASK_PLEDGES)
            .is_some()
            || crate::validation::task_pledge_end(&text, crate::validation::ENGLISH_TASK_PLEDGES)
                .is_some()
            || super::assertions::present_assignment_quote(quote))
}
