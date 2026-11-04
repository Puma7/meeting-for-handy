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
        let allowed = match c.kind.as_str() {
            "fact" => ["recorded", "unclear"].as_slice(),
            "decision" => ["recorded", "withdrawn"].as_slice(),
            "task" => ["open", "completed", "withdrawn"].as_slice(),
            "agenda" => ["touched", "discussed", "unclear"].as_slice(),
            "impulse" => ["open", "discussed", "unclear"].as_slice(),
            _ => &[],
        };
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
                !old.human_locked,
                "Eine Modellantwort darf menschlich bestätigte Einträge nicht verändern."
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
            ensure!(c.evidence.iter().any(|e|task_quote_allowed(&e.quote)) && !c.evidence.iter().any(|e|missing_pledge(&e.quote)),"Eine neue Aufgabe benötigt eine ausdrückliche Übernahme oder Zuweisung. Eine fehlende Zusage ist keine Aufgabe.");
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
        let fact = crate::Fact {
            text: text.clone(),
            owner: c.owner.clone(),
            due: c.due.clone(),
            evidence: c.evidence.clone(),
        };
        let checked = crate::validation::screen_facts(
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
        let quotes = c
            .evidence
            .iter()
            .map(|e| e.quote.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
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
                ["zurück", "widerruf", "nicht mehr", "withdraw", "cancel"]
                    .iter()
                    .any(|s| quotes.contains(s)),
                "Eine Rücknahme benötigt einen ausdrücklichen Beleg."
            );
        }
        if c.kind == "decision" && c.status == "recorded" {
            ensure!(
                [
                    "entscheiden",
                    "beschließen",
                    "beschlossen",
                    "entschieden",
                    "we decide",
                    "we decided",
                    "agreed to"
                ]
                .iter()
                .any(|s| quotes.contains(s)),
                "Ein Vorschlag ist kein Beschluss."
            );
        }
        let target = old
            .map(|e| e.id.clone())
            .or_else(|| {
                next.entities
                    .iter()
                    .find(|e| {
                        !next.context_excluded_ids.contains(&e.id)
                            && e.kind == c.kind
                            && e.text == text
                            && e.owner == c.owner
                            && e.due == c.due
                            && e.evidence == c.evidence
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
            for evidence in &before.evidence {
                if (existing
                    .owner
                    .as_ref()
                    .is_some_and(|v| evidence.quote.to_lowercase().contains(&v.to_lowercase()))
                    || existing
                        .due
                        .as_ref()
                        .is_some_and(|v| evidence.quote.to_lowercase().contains(&v.to_lowercase())))
                    && !existing.evidence.contains(evidence)
                {
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

/// Explicit past or completed-state wording, with negation and uncertainty rejected.
pub(super) fn completion_quote_allowed(quote: &str) -> bool {
    let lower = quote.to_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| !c.is_alphabetic())
        .filter(|s| !s.is_empty())
        .collect();
    if quote.contains('?')
        || words.iter().any(|word| {
            [
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
            ]
            .contains(word)
        })
    {
        return false;
    }
    let phrase = |sequence: &[&str]| words.windows(sequence.len()).any(|part| part == sequence);
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
    if words
        .windows(2)
        .any(|part| ["i", "we"].contains(&part[0]) && english.contains(&part[1]))
        || words.windows(3).any(|part| {
            ["i", "we"].contains(&part[0]) && part[1] == "have" && english.contains(&part[2])
        })
    {
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
    lower.split(['.', '!', ';', '\n']).any(|sentence| {
        let mut words = sentence.split(|c: char| !c.is_alphabetic());
        words.any(|word| ["habe", "haben"].contains(&word))
            && words.any(|word| german.contains(&word))
    })
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
/// Require a positive obligation in the literal quote; a wish or missing approval is insufficient.
pub(super) fn task_quote_allowed(quote: &str) -> bool {
    let text = quote.to_lowercase();
    !missing_pledge(quote)
        && !text.contains('?')
        && ![
            "niemand", "keiner", "nobody", "falls ", "wenn ", "unless ", "if ",
        ]
        .iter()
        .any(|s| text.contains(s))
        && [
            "übernehme",
            "übernimmt",
            "übernehmen",
            "ich werde",
            "wir werden",
            "ich schreibe",
            "wir schreiben",
            "schreibt ",
            "ich erstelle",
            "wir erstellen",
            "erstellt ",
            "ich prüfe",
            "wir prüfen",
            "prüft ",
            "ich gebe",
            "wir geben",
            "ich kümmere",
            "wir kümmern",
            "ich erledige",
            "wir erledigen",
            "ich mache",
            "wir machen",
            "ich liefere",
            "wir liefern",
            "ich schicke",
            "wir schicken",
            "ich sende",
            "wir senden",
            "ich aktualisiere",
            "wir aktualisieren",
            "ich bestätige",
            "wir bestätigen",
            "ich verspreche",
            "wir versprechen",
            "beauftragt",
            "i promise to ",
            "we promise to ",
            "i commit to ",
            "we commit to ",
            "i will ",
            "we will ",
            "i'll ",
            "we'll ",
            "i take ",
            "we take ",
            "takes on ",
            "will handle",
            "will prepare",
            "will send",
            "is assigned",
            "are assigned",
            "commits to",
        ]
        .iter()
        .any(|s| text.contains(s))
}
