use crate::models::Answer;
use crate::{CheckedAnswer, Fact, Kind, RejectionReason, Rejections, Source, Template};
use anyhow::{ensure, Context, Result};
use std::collections::{HashMap, HashSet};
pub fn validate_template(template: &Template) -> Result<()> {
    ensure!(
        !template.name.trim().is_empty()
            && template.name.chars().count() <= 100
            && !template.steps.is_empty()
            && template.steps.len() <= 100,
        "Vorlage benötigt einen Namen und 1–100 Schritte."
    );
    let mut ids = HashSet::new();
    for step in &template.steps {
        ensure!(
            !step.id.is_empty()
                && ids.insert(&step.id)
                && !step.name.trim().is_empty()
                && step.name.chars().count() <= 100
                && !step.prompt.trim().is_empty()
                && step.prompt.chars().count() <= 8000,
            "Ungültiger oder doppelter Analyseschritt."
        );
    }
    Ok(())
}
/// Maximum number of statements kept from one model answer.
pub const MAX_ITEMS_PER_ANSWER: usize = 20;

/// Screens one model answer. Structurally invalid JSON or a wrong schema fails hard; individual
/// statements that fail the literal source check are discarded and counted instead of failing the
/// whole analysis (a deterministic retry would reproduce the same rejection). Kept statements are
/// unchanged model output that passed every check; nothing is repaired or invented.
pub fn validate_answer(
    json: &str,
    sources: &[Source],
    kind: &Kind,
    template_id: &str,
) -> Result<CheckedAnswer> {
    let answer: Answer =
        serde_json::from_str(json).context("Ungültige oder doppelte Felder in der KI-Antwort.")?;
    Ok(screen_facts(
        answer.items,
        sources,
        kind,
        template_id,
        MAX_ITEMS_PER_ANSWER,
    ))
}

/// Applies the literal source check to already parsed statements and keeps at most `limit`
/// valid ones in their original order. Discarded statements are only counted.
pub(crate) fn screen_facts(
    facts: Vec<Fact>,
    sources: &[Source],
    kind: &Kind,
    template_id: &str,
    limit: usize,
) -> CheckedAnswer {
    screen_facts_with_obligation_check(facts, sources, kind, template_id, limit, true)
}

/// Existing task statuses are not new obligations: "no longer needed" can
/// withdraw a task. Keep all prior literal/number/metadata/uncertainty checks;
/// workspace validation separately requires explicit completion/withdrawal evidence.
pub(crate) fn screen_task_status_facts(
    facts: Vec<Fact>,
    sources: &[Source],
    kind: &Kind,
    template_id: &str,
    limit: usize,
) -> CheckedAnswer {
    screen_facts_with_obligation_check(facts, sources, kind, template_id, limit, false)
}

fn screen_facts_with_obligation_check(
    facts: Vec<Fact>,
    sources: &[Source],
    kind: &Kind,
    template_id: &str,
    limit: usize,
    require_new_commitment: bool,
) -> CheckedAnswer {
    let allowed = allowed_sources(sources);
    let mut checked = CheckedAnswer::default();
    for fact in facts {
        let reason = check_fact(&fact, &allowed, kind)
            .or_else(|| generation_check(&fact, kind, require_new_commitment))
            .or_else(|| {
                crate::name_candidates::check_item(template_id, &fact)
                    .err()
                    .map(|_| RejectionReason::InvalidNameCandidate)
            });
        match reason {
            Some(reason) => *checked.rejected.entry(reason).or_default() += 1,
            None if checked.items.len() >= limit => {
                *checked
                    .rejected
                    .entry(RejectionReason::ItemLimit)
                    .or_default() += 1
            }
            None => checked.items.push(fact),
        }
    }
    checked
}

/// Additional conservative rules for newly generated answers only. Historical checkpoints
/// still use their original literal-source contract; reading never rewrites older work.
fn generation_check(
    fact: &Fact,
    kind: &Kind,
    require_new_commitment: bool,
) -> Option<RejectionReason> {
    let quoted: HashSet<String> = fact
        .evidence
        .iter()
        .flat_map(|e| number_tokens(&e.quote))
        .collect();
    // A due date or owner is checked as a substring of its quote, so "5. Mai" would pass
    // against "bis zum 15. Mai"; its numbers must therefore match whole quoted numbers too.
    let stated = [Some(&fact.text), fact.owner.as_ref(), fact.due.as_ref()];
    if stated
        .into_iter()
        .flatten()
        .flat_map(|text| number_tokens(text))
        .any(|n| !quoted.contains(&n))
    {
        return Some(RejectionReason::NumberNotQuoted);
    }
    if *kind == Kind::Tasks {
        let commitments: Vec<_> = fact
            .evidence
            .iter()
            .map(|e| {
                if require_new_commitment && commitment_refused(&e.quote) {
                    Commitment::Nonbinding
                } else {
                    commitment(&e.quote)
                }
            })
            .collect();
        if commitments.iter().all(|c| *c == Commitment::Nonbinding) {
            return Some(RejectionReason::NonbindingTask);
        }
        // "Ich übernehme das, vielleicht bis Freitag" commits to the task, not to the date:
        // the quote that names the date must itself be binding.
        if let Some(due) = &fact.due {
            let due = due.to_lowercase();
            let committed =
                fact.evidence.iter().zip(&commitments).any(|(e, c)| {
                    *c == Commitment::Binding && e.quote.to_lowercase().contains(&due)
                });
            if !committed {
                return Some(RejectionReason::UnverifiedOwnerOrDue);
            }
        }
    }
    None
}

fn number_tokens(text: &str) -> Vec<String> {
    let chars: Vec<_> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut token = String::new();
    for (index, &c) in chars.iter().enumerate() {
        // A sign belongs to its number ("-100 Euro"), a hyphen between digits does not
        // ("2025-2026", "10-12 Uhr").
        let sign = token.is_empty()
            && matches!(c, '-' | '−')
            && (index == 0 || !chars[index - 1].is_alphanumeric())
            && chars.get(index + 1).is_some_and(|c| c.is_numeric());
        if sign {
            token.push('-');
        } else if c.is_numeric()
            || (!token.is_empty()
                && matches!(c, '.' | ',' | ':' | '/')
                && chars.get(index + 1).is_some_and(|c| c.is_numeric()))
        {
            token.push(c);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Commitment {
    Binding,
    /// The task is accepted, but its date is hedged ("vielleicht bis Freitag").
    UncertainDate,
    Nonbinding,
}

/// Words that can only start a date or deadline, so a preceding hedge qualifies the date.
const DATE_WORDS: &[&str] = &[
    "bis",
    "spätestens",
    "heute",
    "morgen",
    "übermorgen",
    "until",
    "today",
    "tomorrow",
];
/// Words that start a date only when a date follows ("am Freitag", "next week"); otherwise
/// ("Maybe this is …", "Vielleicht diese Aufgabe …") the hedge covers the whole task.
const DATE_PREFIXES: &[&str] = &[
    "am",
    "ab",
    "nächste",
    "nächsten",
    "nächster",
    "nächstes",
    "diese",
    "diesen",
    "dieser",
    "dieses",
    "ende",
    "anfang",
    "by",
    "on",
    "next",
    "this",
    "end",
];
/// Weekdays, months and calendar units that complete a date after a prefix.
const DATE_NOUNS: &[&str] = &[
    "montag",
    "dienstag",
    "mittwoch",
    "donnerstag",
    "freitag",
    "samstag",
    "sonntag",
    "woche",
    "wochenende",
    "monat",
    "jahr",
    "quartal",
    "januar",
    "februar",
    "märz",
    "april",
    "mai",
    "juni",
    "juli",
    "august",
    "september",
    "oktober",
    "november",
    "dezember",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "week",
    "weekend",
    "month",
    "year",
    "quarter",
    "january",
    "february",
    "march",
    "june",
    "july",
    "october",
    "december",
];

/// Whether the words after a hedge form a date ("bis Freitag", "am 3. Mai", "next week").
fn starts_date(words: &[&str]) -> bool {
    let is_date = |word: &&str| {
        DATE_NOUNS.contains(word) || word.chars().next().is_some_and(char::is_numeric)
    };
    match words {
        [first, ..] if DATE_WORDS.contains(first) || DATE_NOUNS.contains(first) => true,
        [first, second, ..] if DATE_PREFIXES.contains(first) => is_date(second),
        _ => false,
    }
}
/// Negated phrases that still promise the action ("das werde ich nicht vergessen").
const AFFIRMING_NEGATIONS: &[&str] = &[
    "nicht vergessen",
    "nicht vergisst",
    "nicht liegen lassen",
    "not forget",
    "won't forget",
];

/// Refusal screen for newly generated obligations, shared with workspace schema
/// admission. Existing literal checkpoints are not reinterpreted by this rule.
pub(crate) fn commitment_refused(quote: &str) -> bool {
    let text = quote.to_lowercase();
    let without_affirmation = AFFIRMING_NEGATIONS
        .iter()
        .fold(text, |text, phrase| text.replace(phrase, " "));
    without_affirmation
        .split(|c: char| !c.is_alphabetic())
        .any(|word| {
            [
                "nicht",
                "nie",
                "niemals",
                "keineswegs",
                "kein",
                "keine",
                "keinen",
                "keinem",
                "keiner",
                "keines",
                "niemand",
                "not",
                "never",
                "nobody",
                "noone",
            ]
            .contains(&word)
        })
}

/// Conservative commitment reading of one literal quote. Hedges and refusals reject the task;
/// a hedge directly before a date only makes the date unverified.
fn commitment(quote: &str) -> Commitment {
    let text = quote.to_lowercase();
    let negation = AFFIRMING_NEGATIONS
        .iter()
        .fold(text.clone(), |text, phrase| text.replace(phrase, " "));
    if [
        "könnten ",
        "könnte ",
        "wäre es",
        "unverbindlich",
        "no commitment",
        "we could",
        "might ",
    ]
    .iter()
    .any(|marker| text.contains(marker))
        || ((negation.contains("werde ") || negation.contains("werden "))
            && negation.contains(" nicht "))
        || negation.contains("will not ")
        || negation.contains("won't ")
    {
        return Commitment::Nonbinding;
    }
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut hedged_date = false;
    for (index, word) in words.iter().enumerate() {
        if ["vielleicht", "maybe", "perhaps"].contains(word) {
            if starts_date(&words[index + 1..]) {
                hedged_date = true;
            } else {
                return Commitment::Nonbinding;
            }
        }
    }
    if hedged_date {
        Commitment::UncertainDate
    } else {
        Commitment::Binding
    }
}

/// Reads a saved per-batch checkpoint strictly: saved items are never filtered or repaired,
/// so a damaged checkpoint blocks instead of silently shrinking confirmed work.
/// Name-candidate rules are checked separately by the caller (legacy batches are set aside).
pub(crate) fn read_checkpoint(
    json: &str,
    sources: &[Source],
    kind: &Kind,
) -> Result<CheckedAnswer> {
    let checkpoint: CheckedAnswer = serde_json::from_str(json)
        .context("Ungültige oder doppelte Felder im gespeicherten Auswertungsabschnitt.")?;
    ensure!(
        checkpoint.items.len() <= MAX_ITEMS_PER_ANSWER,
        "Zu viele KI-Aussagen im gespeicherten Auswertungsabschnitt."
    );
    verify_facts(&checkpoint.items, sources, kind)?;
    validate_rejections(&checkpoint.rejected)?;
    Ok(checkpoint)
}

/// Strict check for stored statements: every item must pass the same literal source check.
pub(crate) fn verify_facts(items: &[Fact], sources: &[Source], kind: &Kind) -> Result<()> {
    let allowed = allowed_sources(sources);
    for fact in items {
        if let Some(reason) = check_fact(fact, &allowed, kind) {
            anyhow::bail!(
                "Eine gespeicherte KI-Aussage besteht die Quellenprüfung nicht ({reason:?})."
            );
        }
    }
    Ok(())
}

/// Recorded counts are bounded, positive and never exceed what one stored run can contain.
pub(crate) fn validate_rejections(rejected: &Rejections) -> Result<()> {
    ensure!(
        rejected
            .values()
            .all(|count| (1..=1_000_000).contains(count)),
        "Ungültige Anzahl verworfener KI-Aussagen."
    );
    Ok(())
}

fn allowed_sources(sources: &[Source]) -> HashMap<&str, &str> {
    sources
        .iter()
        .map(|s| (s.id.as_str(), s.text.as_str()))
        .collect()
}

fn check_fact(fact: &Fact, allowed: &HashMap<&str, &str>, kind: &Kind) -> Option<RejectionReason> {
    if fact.text.trim().is_empty() || fact.text.chars().count() > 1600 {
        return Some(RejectionReason::InvalidText);
    }
    if !(1..=6).contains(&fact.evidence.len()) {
        return Some(RejectionReason::EvidenceCount);
    }
    let literal = fact.evidence.iter().all(|e| {
        (8..=600).contains(&e.quote.chars().count())
            && allowed
                .get(e.source_id.as_str())
                .is_some_and(|text| text.contains(&e.quote))
    });
    if !literal {
        return Some(RejectionReason::QuoteNotLiteral);
    }
    if *kind != Kind::Tasks && (fact.owner.is_some() || fact.due.is_some()) {
        return Some(RejectionReason::OwnerOrDueOutsideTasks);
    }
    let verified = [&fact.owner, &fact.due].into_iter().flatten().all(|value| {
        !value.trim().is_empty()
            && value.chars().count() <= 200
            && fact
                .evidence
                .iter()
                .any(|e| e.quote.to_lowercase().contains(&value.to_lowercase()))
    });
    if !verified {
        return Some(RejectionReason::UnverifiedOwnerOrDue);
    }
    None
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    #[test]
    fn explicit_refusals_are_not_generated_tasks_and_old_checkpoints_are_retained() {
        for quote in [
            "Ich übernehme den Bericht nicht.",
            "Wir übernehmen den Bericht keineswegs.",
            "Ich schreibe den Bericht niemals.",
            "Ich verspreche nicht, den Bericht zu schreiben.",
            "I do not promise to email the report.",
            "We never take the report.",
        ] {
            let checked = check(quote, "Bericht übernehmen", Kind::Tasks);
            assert!(checked.items.is_empty(), "{quote}");
            assert!(!checked.rejected.is_empty(), "{quote}");
            // Saved results use strict literal verification, not a retroactive
            // rerun of changed generation heuristics or a silent history filter.
            let source = Source {
                id: "synthetic".into(),
                source: "system".into(),
                start_ms: 0,
                end_ms: 1000,
                text: quote.into(),
            };
            let saved = CheckedAnswer {
                items: vec![Fact {
                    text: "Bericht übernehmen".into(),
                    owner: None,
                    due: None,
                    evidence: vec![crate::Evidence {
                        source_id: "synthetic".into(),
                        quote: quote.into(),
                    }],
                }],
                rejected: Default::default(),
            };
            let bytes = serde_json::to_string(&saved).unwrap();
            assert_eq!(
                serde_json::to_string(&read_checkpoint(&bytes, &[source], &Kind::Tasks).unwrap())
                    .unwrap(),
                bytes
            );
        }
        for quote in [
            "Ich übernehme den Bericht.",
            "Wir übernehmen den Bericht.",
            "Ich werde den Bericht nicht vergessen.",
            "I will not forget to email the report.",
        ] {
            assert_eq!(
                check(quote, "Bericht übernehmen", Kind::Tasks).items.len(),
                1,
                "{quote}"
            );
        }
    }
    fn check(quote: &str, text: &str, kind: Kind) -> CheckedAnswer {
        check_due(quote, text, None, kind)
    }
    fn check_due(quote: &str, text: &str, due: Option<&str>, kind: Kind) -> CheckedAnswer {
        let source = Source {
            id: "synthetic".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1000,
            text: quote.into(),
        };
        let answer = serde_json::json!({"items":[{"text":text,"owner":null,"due":due,"evidence":[{"sourceId":"synthetic","quote":quote}]}]}).to_string();
        validate_answer(&answer, &[source], &kind, "synthetic").unwrap()
    }
    #[test]
    fn a_sign_is_part_of_the_number_but_a_range_hyphen_is_not() {
        assert!(check(
            "Das Ergebnis liegt bei -100 Euro.",
            "100 Euro",
            Kind::Summary
        )
        .items
        .is_empty());
        assert_eq!(
            check(
                "Das Ergebnis liegt bei -100 Euro.",
                "-100 Euro",
                Kind::Summary
            )
            .items
            .len(),
            1
        );
        assert_eq!(
            check(
                "Der Plan gilt für 2025-2026.",
                "Plan für 2025 bis 2026",
                Kind::Summary
            )
            .items
            .len(),
            1
        );
        assert_eq!(number_tokens("10-12 Uhr, −3 Grad"), ["10", "12", "-3"]);
    }
    #[test]
    fn a_due_date_needs_its_whole_quoted_number() {
        let quote = "Anna schickt den Bericht bis zum 15. Mai.";
        assert!(
            check_due(quote, "Bericht schicken", Some("5. Mai"), Kind::Tasks)
                .items
                .is_empty()
        );
        assert_eq!(
            check_due(quote, "Bericht schicken", Some("15. Mai"), Kind::Tasks)
                .items
                .len(),
            1
        );
    }
    #[test]
    fn a_hedged_date_keeps_the_task_but_not_the_date() {
        let quote = "Ich übernehme den Bericht, vielleicht bis Freitag.";
        assert_eq!(
            check(quote, "Bericht übernehmen", Kind::Tasks).items.len(),
            1
        );
        assert!(
            check_due(quote, "Bericht übernehmen", Some("Freitag"), Kind::Tasks)
                .items
                .is_empty()
        );
        assert_eq!(
            check(
                "Das Protokoll werde ich nicht vergessen, ich schicke es morgen.",
                "Protokoll schicken",
                Kind::Tasks
            )
            .items
            .len(),
            1
        );
        assert!(check(
            "Vielleicht übernehme ich den Bericht.",
            "Bericht übernehmen",
            Kind::Tasks
        )
        .items
        .is_empty());
        for quote in [
            "Maybe this is something Anna should handle.",
            "Vielleicht diese Aufgabe Anna?",
        ] {
            assert!(
                check(quote, "Anna übernimmt", Kind::Tasks).items.is_empty(),
                "{quote}"
            );
        }
        assert_eq!(
            check(
                "Ich prüfe das, vielleicht am Freitag.",
                "Prüfen",
                Kind::Tasks
            )
            .items
            .len(),
            1
        );
        assert_eq!(
            check("I will review it, maybe next week.", "Review", Kind::Tasks)
                .items
                .len(),
            1
        );
    }
    #[test]
    fn a_due_date_needs_a_binding_quote_that_names_it() {
        let source = Source {
            id: "synthetic".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1000,
            text: "Ich übernehme den Bericht. Vielleicht bis Freitag.".into(),
        };
        let answer = serde_json::json!({"items":[{"text":"Bericht übernehmen","owner":null,"due":"Freitag","evidence":[
            {"sourceId":"synthetic","quote":"Ich übernehme den Bericht."},
            {"sourceId":"synthetic","quote":"Vielleicht bis Freitag."}
        ]}]})
        .to_string();
        let checked = validate_answer(&answer, &[source], &Kind::Tasks, "synthetic").unwrap();
        assert!(checked.items.is_empty());
        assert_eq!(
            checked.rejected.get(&RejectionReason::UnverifiedOwnerOrDue),
            Some(&1)
        );
    }
    #[test]
    fn introduced_numbers_are_not_rescued_by_a_literal_quote() {
        for (quote, text) in [
            (
                "Es liegen sechsundvierzig Entwürfe vor.",
                "Es liegen 640 Entwürfe vor.",
            ),
            ("Es liegen 850 Entwürfe vor.", "Es liegen 8 Entwürfe vor."),
            (
                "Der Anteil beträgt 2,5 Prozent.",
                "Der Anteil beträgt 25 Prozent.",
            ),
        ] {
            assert!(check(quote, text, Kind::Summary).items.is_empty());
        }
        assert_eq!(
            check(
                "Es liegen 46 Entwürfe vor.",
                "46 Entwürfe liegen vor.",
                Kind::Summary
            )
            .items
            .len(),
            1
        );
        assert_eq!(
            check(
                "Es liegen sechsundvierzig Entwürfe vor.",
                "Sechsundvierzig Entwürfe liegen vor.",
                Kind::Summary
            )
            .items
            .len(),
            1
        );
    }
    #[test]
    fn an_explicitly_nonbinding_quote_is_not_a_task() {
        for quote in [
            "Vielleicht könnten wir den Entwurf prüfen.",
            "We could review the draft, but no commitment yet.",
            "Ich werde den Entwurf nicht prüfen.",
        ] {
            assert!(check(quote, "Entwurf prüfen", Kind::Tasks).items.is_empty());
            assert_eq!(
                check(quote, "Unverbindlicher Vorschlag", Kind::Summary)
                    .items
                    .len(),
                1
            );
        }
        assert_eq!(
            check(
                "Ich übernehme die Prüfung des Entwurfs.",
                "Entwurf prüfen",
                Kind::Tasks
            )
            .items
            .len(),
            1
        );
    }
    #[test]
    fn stricter_generation_rules_do_not_rewrite_historical_checkpoints() {
        let source = Source {
            id: "old".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1000,
            text: "Es liegen sechsundvierzig Entwürfe vor.".into(),
        };
        let checkpoint = serde_json::json!({"items":[{"text":"640 Entwürfe","owner":null,"due":null,"evidence":[{"sourceId":"old","quote":source.text}]}]}).to_string();
        let old = read_checkpoint(&checkpoint, &[source], &Kind::Summary).unwrap();
        assert_eq!(
            old.items[0].text, "640 Entwürfe",
            "Old evidence is preserved, not silently repaired or hidden"
        );
    }
}
