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
                if require_new_commitment {
                    new_task_commitment(&e.quote)
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

pub(crate) fn starts_named_month(text: &str) -> bool {
    let word = text
        .trim_start()
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default();
    named_month(word).is_some()
}

/// Recover only an unambiguous literal weekday attached to this binding sentence.
pub(crate) fn explicit_task_weekday(quote: &str) -> Option<&str> {
    if new_task_commitment(quote) != Commitment::Binding {
        return None;
    }
    let spans = commitment_word_spans(quote);
    let lower: Vec<_> = spans.iter().map(|(_, word)| word.to_lowercase()).collect();
    if lower.iter().any(|word| {
        matches!(
            word.as_str(),
            "and" | "or" | "und" | "oder" | "then" | "dann"
        )
    }) {
        return None;
    }
    let mut weekdays = lower.iter().enumerate().filter_map(|(index, marker)| {
        if !matches!(marker.as_str(), "by" | "bis") {
            return None;
        }
        let mut next = index + 1;
        if lower.get(next).is_some_and(|word| word == "zum") {
            next += 1;
        }
        if !matches!(
            lower.get(next)?.as_str(),
            "monday"
                | "tuesday"
                | "wednesday"
                | "thursday"
                | "friday"
                | "saturday"
                | "sunday"
                | "montag"
                | "dienstag"
                | "mittwoch"
                | "donnerstag"
                | "freitag"
                | "samstag"
                | "sonntag"
        ) {
            return None;
        }
        let (day_start, day) = spans[next];
        let between = &quote[spans[index].0 + spans[index].1.len()..day_start];
        let tail = quote[day_start + day.len()..]
            .trim()
            .trim_end_matches(['.', '!']);
        // Additional date detail or alternatives would make the bare weekday
        // incomplete. Keep an ordinary German separable verb particle.
        (!between.contains(|c: char| !c.is_whitespace() && !c.is_alphabetic())
            && (tail.is_empty()
                || (marker == "bis"
                    && matches!(
                        tail,
                        "ab" | "an" | "auf" | "ein" | "mit" | "vor" | "zu" | "fertig"
                    ))))
        .then_some(day)
    });
    let weekday = weekdays.next()?;
    weekdays.next().is_none().then_some(weekday)
}

/// Recover a calendar date only when a single binding deadline ends the quote.
/// Keep the literal phrase, including any stated year, for the existing evidence
/// validation and locale-specific presentation.
pub(crate) fn explicit_task_calendar_date(quote: &str) -> Option<&str> {
    if new_task_commitment(quote) != Commitment::Binding {
        return None;
    }
    let spans = commitment_word_spans(quote);
    if spans.iter().any(|(_, word)| {
        matches!(
            word.to_lowercase().as_str(),
            "and" | "or" | "und" | "oder" | "then" | "dann"
        )
    }) {
        return None;
    }
    let mut markers = spans
        .iter()
        .filter(|(_, word)| word.eq_ignore_ascii_case("by") || word.eq_ignore_ascii_case("bis"));
    let &(start, marker) = markers.next()?;
    if markers.next().is_some() {
        return None;
    }
    let german = marker.eq_ignore_ascii_case("bis");
    let after = quote[start + marker.len()..].trim_end();
    let after = after.trim_end_matches('!').trim_end();
    // In German, the terminal point of a numeric month belongs to the date.
    let numeric_month = german
        && after
            .strip_suffix('.')
            .and_then(|date| date.split_whitespace().next_back())
            .is_some_and(|token| {
                (1..=2).contains(&token.len()) && token.bytes().all(|byte| byte.is_ascii_digit())
            });
    let after = if numeric_month {
        after
    } else {
        after.trim_end_matches('.').trim_end()
    };
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    let mut date = after.trim_start();
    let mut words = date.split_whitespace();
    let mut first = words.next()?;
    if german && first.eq_ignore_ascii_case("zum") {
        date = date[first.len()..].trim_start();
        first = words.next()?;
    }
    let mut second = words.next()?;
    if !german && second.eq_ignore_ascii_case("the") {
        second = words.next()?;
    }
    let trailing = words.next();
    let stated_year =
        trailing.filter(|word| word.len() == 4 && word.bytes().all(|value| value.is_ascii_digit()));
    let (day, month) = if german {
        (
            german_day(first)?,
            named_month(second).or_else(|| german_day(second))?,
        )
    } else {
        if let Some(day) = second.strip_suffix(',') {
            stated_year?;
            second = day;
        }
        (english_day(second)?, named_month(first)?)
    };
    let particle = if stated_year.is_some() {
        words.next()
    } else {
        trailing
    };
    if words.next().is_some()
        || particle.is_some_and(|word| {
            !german
                || !matches!(
                    word,
                    "ab" | "an" | "auf" | "ein" | "mit" | "vor" | "zu" | "fertig"
                )
        })
    {
        return None;
    }
    let year = stated_year
        .and_then(|word| word.parse().ok())
        .unwrap_or(2000);
    if year == 0 {
        return None;
    }
    chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    match particle {
        Some(particle) => date.strip_suffix(particle).map(str::trim_end),
        None => Some(date),
    }
}

#[test]
fn literal_calendar_deadlines_reject_uncertainty_and_calendar_overflow() {
    for (quote, expected) in [
        (
            "Jana holt die Technik bis zum dreiundzwanzigsten neunten ab.",
            Some("dreiundzwanzigsten neunten"),
        ),
        (
            "Jana will collect the equipment by September twenty-third.",
            Some("September twenty-third"),
        ),
        (
            "I will finish the guide by September 23rd 2026.",
            Some("September 23rd 2026"),
        ),
        (
            "I will finish the guide by September 23rd, 2026.",
            Some("September 23rd, 2026"),
        ),
        ("I will finish the guide by September 23rd, 0000.", None),
        ("I will finish the guide by September 23rd, 26.", None),
        (
            "Ich bringe die Unterlagen bis zum 23. September 2026 mit.",
            Some("23. September 2026"),
        ),
        ("Jana bringt die Unterlagen bis zum 23. 9.", Some("23. 9.")),
        ("Jana bringt die Unterlagen bis zum 23. 13.", None),
        (
            "I will write the guide, maybe by September twenty-third.",
            None,
        ),
        (
            "Maybe I will write the guide by September twenty-third.",
            None,
        ),
        ("I will write the guide by September thirty-first.", None),
        (
            "I will write the guide by February twenty-ninth 2025.",
            None,
        ),
        (
            "I will write the guide by September twenty-third or twenty-fourth.",
            None,
        ),
        ("I will write the guide by September 23th.", None),
        ("Jana holt die Technik am dritten September ab.", None),
    ] {
        assert_eq!(explicit_task_calendar_date(quote), expected, "{quote}");
    }
}

fn named_month(word: &str) -> Option<u32> {
    const MONTHS: [(&str, &str); 12] = [
        ("januar", "january"),
        ("februar", "february"),
        ("märz", "march"),
        ("april", "april"),
        ("mai", "may"),
        ("juni", "june"),
        ("juli", "july"),
        ("august", "august"),
        ("september", "september"),
        ("oktober", "october"),
        ("november", "november"),
        ("dezember", "december"),
    ];
    let lower = word.to_lowercase();
    MONTHS
        .iter()
        .position(|&(de, en)| lower == de || lower == en)
        .map(|index| index as u32 + 1)
}

fn german_day(word: &str) -> Option<u32> {
    const STEMS: [&str; 31] = [
        "erst",
        "zweit",
        "dritt",
        "viert",
        "fünft",
        "sechst",
        "siebt",
        "acht",
        "neunt",
        "zehnt",
        "elft",
        "zwölft",
        "dreizehnt",
        "vierzehnt",
        "fünfzehnt",
        "sechzehnt",
        "siebzehnt",
        "achtzehnt",
        "neunzehnt",
        "zwanzigst",
        "einundzwanzigst",
        "zweiundzwanzigst",
        "dreiundzwanzigst",
        "vierundzwanzigst",
        "fünfundzwanzigst",
        "sechsundzwanzigst",
        "siebenundzwanzigst",
        "achtundzwanzigst",
        "neunundzwanzigst",
        "dreißigst",
        "einunddreißigst",
    ];
    if let Some(number) = word.strip_suffix('.') {
        return number.parse().ok();
    }
    let lower = word.to_lowercase();
    let stem = ["en", "er", "em", "es", "e"]
        .iter()
        .find_map(|ending| lower.strip_suffix(ending))?;
    STEMS
        .iter()
        .position(|&value| value == stem)
        .map(|index| index as u32 + 1)
}

fn english_day(word: &str) -> Option<u32> {
    const ORDINALS: [&str; 31] = [
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
        "twenty-first",
        "twenty-second",
        "twenty-third",
        "twenty-fourth",
        "twenty-fifth",
        "twenty-sixth",
        "twenty-seventh",
        "twenty-eighth",
        "twenty-ninth",
        "thirtieth",
        "thirty-first",
    ];
    let lower = word.to_lowercase();
    if let Some(index) = ORDINALS.iter().position(|&value| value == lower) {
        return Some(index as u32 + 1);
    }
    let suffix = lower.trim_start_matches(|value: char| value.is_ascii_digit());
    let number: u32 = lower.strip_suffix(suffix)?.parse().ok()?;
    let expected = match number {
        1 | 21 | 31 => "st",
        2 | 22 => "nd",
        3 | 23 => "rd",
        _ => "th",
    };
    (suffix.is_empty() || suffix == expected).then_some(number)
}

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
    "kein problem",
    "keine sorge",
    "not a problem",
    "no problem",
    "no later than",
    "no earlier than",
    "don't worry",
    "do not worry",
    "don't forget",
    "can't wait",
    "cannot wait",
    "don't mind",
    "doesn't matter",
    "not forget",
    "won't forget",
];

/// Remove whole affirming phrases while keeping every independent refusal word.
/// Apostrophe normalization is only for screening; original evidence stays literal.
/// A longer word such as "Sorgepflicht" must not turn a refusal into a promise.
fn without_affirming_negations(text: &str) -> String {
    let mut text = text.replace('’', "'");
    let part_of_word = |c: char| c.is_alphanumeric() || c == '_';
    for phrase in AFFIRMING_NEGATIONS {
        let ranges: Vec<_> = text
            .match_indices(*phrase)
            .filter(|(start, phrase)| {
                !text[..*start].chars().next_back().is_some_and(part_of_word)
                    && !text[*start + phrase.len()..]
                        .chars()
                        .next()
                        .is_some_and(part_of_word)
            })
            .map(|(start, phrase)| (start, start + phrase.len()))
            .collect();
        for (start, end) in ranges.into_iter().rev() {
            text.replace_range(start..end, " ");
        }
    }
    text
}

/// Existing explicit task markers shared by admission and commitment scoping.
pub(crate) const ENGLISH_TASK_PLEDGES: &[&str] = &[
    "i promise to ",
    "we promise to ",
    "i commit to ",
    "we commit to ",
    "i will ",
    "we will ",
    "i'll ",
    "we'll ",
    "i won't forget to ",
    "we won't forget to ",
    "i take ",
    "we take ",
    "takes on ",
    "will handle",
    "will prepare",
    "will send",
    "will check ",
    "will ask ",
    "is assigned",
    "are assigned",
    "commits to",
];

/// Existing explicit task markers shared by admission and commitment scoping.
pub(crate) const GERMAN_TASK_PLEDGES: &[&str] = &[
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
];

/// Auxiliaries that can make "erstellt" a completed-state participle.
const COMPLETED_CREATION_AUXILIARIES: &[&str] = &[
    "wurde", "wurden", "ist", "sind", "war", "waren", "hat", "haben", "hatte", "hatten",
];

/// Read completion within the current clause, including intervening modifiers.
/// A clause boundary or coordination stops the backward screen before another assertion.
fn completed_creation_before(text: &str) -> bool {
    let clause = text
        .rsplit([',', ';', '.', '!', '?', '\n', ':'])
        .next()
        .unwrap_or(text);
    for word in clause
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .rev()
    {
        if ["und", "oder", "aber", "doch", "sondern", "jedoch"].contains(&word) {
            return false;
        }
        if COMPLETED_CREATION_AUXILIARIES.contains(&word) {
            return true;
        }
    }
    false
}

/// Locate a supported assertion at whole-word boundaries, retaining required continuations.
pub(crate) fn task_pledge_end(text: &str, markers: &[&str]) -> Option<usize> {
    let mut earliest: Option<usize> = None;
    let word_char = |c: char| c.is_alphanumeric() || c == '_';
    for marker in markers {
        let continues = marker.ends_with(' ');
        let marker = marker.trim_end();
        for (start, found) in text.match_indices(marker) {
            let end = start + found.len();
            if text[..start].chars().next_back().is_some_and(word_char)
                || text[end..].chars().next().is_some_and(word_char)
            {
                continue;
            }
            if continues
                && !text[end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace() || c == ',')
            {
                continue;
            }
            if marker == "erstellt" && completed_creation_before(&text[..start]) {
                continue;
            }
            let count = text[..end]
                .split(|c: char| !c.is_alphabetic())
                .filter(|word| !word.is_empty())
                .count();
            earliest = Some(earliest.map_or(count, |old| old.min(count)));
        }
    }
    earliest
}

/// Recognize an embedded question only after an explicit English assertion.
pub(crate) fn embedded_english_question(
    words: &[&str],
    index: usize,
    assertion_end: Option<usize>,
) -> bool {
    assertion_end.is_some_and(|end| index >= end)
        && index > 0
        && words[index] == "if"
        && ["check", "ask"].contains(&words[index - 1])
}

/// Keep independent conditions distinct from a promised check-if/ask-if question.
pub(crate) fn task_condition_present(text: &str) -> bool {
    let words: Vec<_> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    let english = task_pledge_end(text, ENGLISH_TASK_PLEDGES);
    words.iter().enumerate().any(|(index, word)| {
        ["if", "unless", "falls", "wenn", "sofern"].contains(word)
            && !embedded_english_question(&words, index, english)
    })
}

/// Borrow word spans from normalized admission text, preserving UTF-8 slice boundaries.
fn commitment_word_spans(text: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, character) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if character.is_alphabetic() {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            spans.push((begin, &text[begin..index]));
        }
    }
    spans
}

/// Screen the promise rather than a negated subject of its first subordinate clause.
/// Conditions and questions retain whole-quote screening. Later clauses/coordination
/// stay in the screen conservatively, so an independent refusal cannot be hidden.
pub(crate) fn commitment_assertion_text(quote: &str) -> String {
    let text = quote.to_lowercase().replace('’', "'");
    if text.contains('?') || task_condition_present(&text) {
        return text;
    }
    let spans = commitment_word_spans(&text);
    let words: Vec<_> = spans.iter().map(|(_, word)| *word).collect();
    let english = task_pledge_end(&text, ENGLISH_TASK_PLEDGES);
    let german = task_pledge_end(&text, GERMAN_TASK_PLEDGES);
    let subject = words.iter().enumerate().find_map(|(index, word)| {
        ((["that", "whether"].contains(word) && english.is_some_and(|end| index >= end))
            || embedded_english_question(&words, index, english)
            || (english.is_some_and(|end| index >= end)
                && *word == "nothing"
                && (words[..index].ends_with(&["make", "sure"])
                    || words[..index].ends_with(&["ensure"])))
            || (german.is_some_and(|end| index >= end)
                && *word == "nichts"
                && (words[..index].ends_with(&["darauf", "achten"])
                    || words[..index].ends_with(&["sicherstellen"])))
            || (["ob", "dass"].contains(word) && german.is_some_and(|end| index >= end)))
        .then_some(index)
    });
    let Some(subject) = subject else {
        return text;
    };
    let prefix = &text[..spans[subject].0];
    let suffix = ((subject + 1)..spans.len()).find_map(|index| {
        let previous_end = spans[index - 1].0 + spans[index - 1].1.len();
        let separator = &text[previous_end..spans[index].0];
        (separator
            .chars()
            .any(|c| [',', ';', '\n', '.', '!'].contains(&c))
            || [
                "and",
                "or",
                "but",
                "however",
                "though",
                "although",
                "und",
                "oder",
                "aber",
                "jedoch",
                "doch",
                "sondern",
                "allerdings",
            ]
            .contains(&spans[index].1))
        .then_some(spans[index].0)
    });
    match suffix {
        Some(start) => format!("{prefix} {}", &text[start..]),
        None => prefix.to_owned(),
    }
}

/// Share the actual generation screen with workspace schema admission.
pub(crate) fn has_new_task_commitment(quote: &str) -> bool {
    new_task_commitment(quote) != Commitment::Nonbinding
}

/// New work must reject independent conditions in both classic and workspace paths.
fn new_task_commitment(quote: &str) -> Commitment {
    let text = quote.to_lowercase().replace('’', "'");
    let assertion = commitment_assertion_text(quote);
    let completed_creation = commitment_word_spans(&assertion)
        .iter()
        .any(|(start, word)| {
            *word == "erstellt" && completed_creation_before(&assertion[..*start])
        });
    let explicit_pledge = task_pledge_end(&assertion, GERMAN_TASK_PLEDGES).is_some()
        || task_pledge_end(&assertion, ENGLISH_TASK_PLEDGES).is_some();
    if task_condition_present(&text)
        || commitment_refused(quote)
        || (completed_creation && !explicit_pledge)
    {
        Commitment::Nonbinding
    } else {
        commitment(quote)
    }
}

/// Refusal screen for newly generated obligations, shared with workspace schema
/// admission. Existing literal checkpoints are not reinterpreted by this rule.
pub(crate) fn commitment_refused(quote: &str) -> bool {
    let text = commitment_assertion_text(quote);
    let without_affirmation = without_affirming_negations(&text);
    let words: Vec<_> = without_affirmation
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    // A negative actor before the pledge cannot accept work. Scope is already
    // limited to the assertion; "check that no one ..." keeps its positive promise.
    let predicate = words
        .iter()
        .position(|word| ["will", "shall", "ll"].contains(word))
        .or_else(|| task_pledge_end(&without_affirmation, ENGLISH_TASK_PLEDGES));
    let negative_actor = predicate.is_some_and(|end| words[..end].contains(&"no"));
    negative_actor
        || without_affirmation.contains("n't")
        || without_affirmation
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
                    "nichts",
                    "not",
                    "never",
                    "nobody",
                    "nothing",
                    "noone",
                    "none",
                    "neither",
                    "cannot",
                ]
                .contains(&word)
            })
}

/// Conservative commitment reading of one literal quote. Hedges and refusals reject the task;
/// a hedge directly before a date only makes the date unverified.
fn commitment(quote: &str) -> Commitment {
    let text = commitment_assertion_text(quote);
    let negation = without_affirming_negations(&text);
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
        if TASK_HEDGES.contains(word) {
            if starts_date(&words[index + 1..]) {
                hedged_date = true;
            } else {
                return Commitment::Nonbinding;
            }
        }
    }
    // A deadline hedge remains unverified even when it occurs in the subject.
    let literal = quote.to_lowercase();
    let literal_words: Vec<_> = literal
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    hedged_date |= literal_words.iter().enumerate().any(|(index, word)| {
        TASK_HEDGES.contains(word) && starts_date(&literal_words[index + 1..])
    });
    if hedged_date {
        Commitment::UncertainDate
    } else {
        Commitment::Binding
    }
}

/// A predicate hedge weakens work itself; the same word before a date weakens only its due.
const TASK_HEDGES: &[&str] = &[
    "vielleicht",
    "maybe",
    "perhaps",
    "wahrscheinlich",
    "vermutlich",
    "hoffentlich",
    "probably",
    "hopefully",
];

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
    /// Classic generation must share the safeguard and hedge rules without rewriting evidence.
    fn safeguards_and_predicate_hedges_preserve_only_binding_tasks() {
        let mut mismatches = Vec::new();
        for (quote, allowed) in [
            ("I will make sure nothing is missed.", true),
            ("Ich werde darauf achten, nichts zu übersehen.", true),
            ("Ich werde sicherstellen, dass nichts übersehen wird.", true),
            ("I will collect nothing.", false),
            ("Ich übernehme nichts.", false),
            ("I will not make sure nothing is missed.", false),
            ("I will probably write the report.", false),
            ("Ich werde wahrscheinlich den Bericht schreiben.", false),
            ("I will write the report, probably by Friday.", true),
            (
                "Ich werde den Bericht schreiben, wahrscheinlich bis Freitag.",
                true,
            ),
        ] {
            let result = check(quote, "Prepare the report", Kind::Tasks);
            if result.items.len() != usize::from(allowed) {
                mismatches.push(format!("{quote}: retained {}", result.items.len()));
            } else if let Some(item) = result.items.first() {
                assert_eq!(item.evidence[0].quote, quote);
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
        for quote in [
            "I will write the report, probably by Friday.",
            "Ich werde den Bericht schreiben, wahrscheinlich bis Freitag.",
        ] {
            let due = if quote.starts_with("I ") {
                "Friday"
            } else {
                "Freitag"
            };
            assert!(
                check_due(quote, "Prepare the report", Some(due), Kind::Tasks)
                    .items
                    .is_empty()
            );
        }
    }

    #[test]
    fn negative_subjects_keep_real_tasks_without_admitting_refusals_or_hedged_dates() {
        let mut mismatches = Vec::new();
        for (quote, allowed) in [
            ("I will check that the report is not public.", true),
            (
                "Ich übernehme die Prüfung, ob der Bericht nicht öffentlich ist.",
                true,
            ),
            ("I will check if the report is not public.", true),
            ("I’ll ask whether there is no commitment yet.", true),
            ("Ich prüfe, ob der Bericht nicht öffentlich ist.", true),
            (
                "Ich werde prüfen, ob der Bericht nicht öffentlich ist.",
                true,
            ),
            ("I will ensure that the report is not public.", true),
            ("I will not check that the report is public.", false),
            (
                "Ich übernehme die Prüfung nicht, ob der Bericht öffentlich ist.",
                false,
            ),
            (
                "I will check that the report is not public, but I will not do the check.",
                false,
            ),
            (
                "I will check that the report is not public; no commitment.",
                false,
            ),
            (
                "I will check that the report is not public and I will not do the check.",
                false,
            ),
            (
                "I will check if the report is not public if approval arrives.",
                false,
            ),
            ("I will write the report if approval arrives.", false),
            ("Ich schreibe den Bericht, sofern Freigabe kommt.", false),
            (
                "I won’t forget to check that the report is not public.",
                true,
            ),
            ("I won’t forget to check if the report is ready.", true),
            ("I won’t forget to not check the report.", false),
            (
                "I won’t forget to check that the report is not public, but I won’t do the check.",
                false,
            ),
            ("Robin will check if the report is ready.", true),
            ("Robin will check that the report is not public.", true),
            ("Robin will ask if the report is ready.", true),
            ("Robin will not check if the report is ready.", false),
            ("Robin will check the report if approval arrives.", false),
            ("No one will check the report.", false),
            ("None of us will send the report.", false),
            ("Neither Robin nor Casey will send the report.", false),
            (
                "I will check that none of us has published the report.",
                true,
            ),
            ("I will check whether neither copy is public.", true),
            ("No employee will send the report.", false),
            ("Today no participant will prepare the report.", false),
            ("Not a problem, no employee will send the report.", false),
            ("I will check that no one has published the report.", true),
            (
                "I will ask whether no employee has published the report.",
                true,
            ),
            (
                "No problem, I will check that no one has published the report.",
                true,
            ),
            ("No later than Friday, I will send the report.", true),
            ("I will send the report no later than Friday.", true),
            ("Das Protokoll wurde erstellt.", false),
            ("Der Bericht ist erstellt.", false),
            ("Das Protokoll wurde bereits erstellt.", false),
            ("Der Bericht war heute schon vollständig erstellt.", false),
            ("Das Protokoll ist bereits erstellt, wie besprochen.", false),
            ("Robin ist bereit und erstellt den Bericht.", true),
            ("Robin war krank, erstellt jetzt den Bericht.", true),
            (
                "Ich übernehme die Prüfung, ob das Protokoll bereits erstellt ist.",
                true,
            ),
            ("Der Bericht wurde erstellt .", false),
            ("Das Protokoll ist erstellt, wie besprochen.", false),
            ("Robin erstellt, wie vereinbart, den Bericht.", true),
            ("Robin erstellt den Bericht.", true),
            (
                "Ich übernehme die Prüfung, ob das Protokoll erstellt ist.",
                true,
            ),
            (
                "Das Protokoll wurde erstellt, ich übernehme die Prüfung.",
                true,
            ),
            ("Maybe I will check that the report is not public.", false),
        ] {
            let checked = check(quote, "Check the report", Kind::Tasks);
            if checked.items.len() != usize::from(allowed) {
                mismatches.push(format!("{quote}: kept={}", checked.items.len()));
            }
            if let Some(item) = checked.items.first() {
                assert_eq!(item.evidence[0].quote, quote);
                assert!(item.owner.is_none() && item.due.is_none());
            }
        }
        let quote = "I will check that the report is not public, maybe by Friday.";
        let without_date = check(quote, "Check the report", Kind::Tasks);
        if without_date.items.len() != 1 {
            mismatches.push("Hedged date discarded the unconditional checking task".into());
        }
        assert!(
            check_due(quote, "Check the report", Some("Friday"), Kind::Tasks)
                .items
                .is_empty()
        );
        assert!(
            mismatches.is_empty(),
            "Classic commitment scope mismatches: {mismatches:?}"
        );
    }
    #[test]
    fn explicit_refusals_are_not_generated_tasks_and_old_checkpoints_are_retained() {
        for quote in [
            "Ich übernehme den Bericht nicht.",
            "Wir übernehmen den Bericht keineswegs.",
            "Ich schreibe den Bericht niemals.",
            "Ich verspreche nicht, den Bericht zu schreiben.",
            "I do not promise to email the report.",
            "We never take the report.",
            "Kein Problem, ich übernehme den Bericht nicht.",
            "Keine Sorge, ich verspreche nicht, den Bericht zu schreiben.",
            "Not a problem, I will not email the report.",
            "Ich übernehme keine Sorgepflicht für den Bericht.",
            "I won’t email the report.",
            "Not a problem, I won’t email the report.",
            "I don't promise to email the report.",
            "I don’t promise to email the report.",
            "I can't take the report.",
            "I can’t take the report.",
            "I cannot write the report.",
            "Don't worry, I can't take the report.",
            "We can't promise that we will send the report.",
            "We can’t promise that we will send the report.",
            "I can't wait, but I won't email the report.",
            "I cannot wait, but I cannot write the report.",
            "I don't mind, but I won't email the report.",
            "It doesn't matter, I cannot write the report.",
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
            "Kein Problem, ich übernehme den Bericht.",
            "KEIN PROBLEM – ich übernehme den Bericht.",
            "Keine Sorge, ich schreibe den Bericht.",
            "Not a problem, I will email the report.",
            "Kein Problem: Ich werde den Bericht nicht vergessen.",
            "I won’t forget to email the report.",
            "Don't worry, I will email the report.",
            "Do not worry, I will email the report.",
            "Don't forget, I will email the report.",
            "I can't wait, I will email the report.",
            "I cannot wait, I will email the report.",
            "I don't mind, I will email the report.",
            "It doesn't matter, I will email the report.",
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
