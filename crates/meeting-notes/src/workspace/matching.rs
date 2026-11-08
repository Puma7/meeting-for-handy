//! Conservative literal links between an existing entry and a new utterance.
use super::model::Entity;

fn terms(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .filter(|word| {
            (word.chars().count() >= 4 || word.bytes().all(|ch| ch.is_ascii_digit()))
                && ![
                    "does",
                    "what",
                    "when",
                    "where",
                    "which",
                    "with",
                    "will",
                    "would",
                    "could",
                    "should",
                    "have",
                    "been",
                    "done",
                    "finished",
                    "still",
                    "open",
                    "discussed",
                    "clarify",
                    "approval",
                    "approve",
                    "approved",
                    "about",
                    "that",
                    "this",
                    "from",
                    "need",
                    "wird",
                    "wurde",
                    "soll",
                    "sollen",
                    "kann",
                    "können",
                    "noch",
                    "offen",
                    "fertig",
                    "erledigt",
                    "geklärt",
                    "klären",
                    "zuständig",
                    "zuständigkeit",
                    "freigeben",
                    "freigegeben",
                    "freigabe",
                ]
                .contains(&word.as_str())
        })
        .map(|word| match word.as_str() {
            "bericht" | "berichte" | "reports" => "report".into(),
            _ => word,
        })
        .collect()
}

fn related_word(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    // The shared part must be a whole term or a compound boundary.
    // Short numbers, generic words and shared internal substrings are insufficient.
    if left.chars().count().min(right.chars().count()) < 5 {
        return false;
    }
    if left.starts_with(right)
        || left.ends_with(right)
        || right.starts_with(left)
        || right.ends_with(left)
    {
        return true;
    }
    [left, right].into_iter().zip([right, left]).any(|(a, b)| {
        a.char_indices()
            .skip(1)
            .map(|(start, _)| &a[start..])
            .take_while(|part| part.chars().count() >= 6)
            .any(|part| part.chars().any(char::is_alphabetic) && b.starts_with(part))
    })
}

pub(super) fn shares_topic(title: &str, quote: &str) -> bool {
    let spoken = terms(quote);
    terms(title).iter().any(|word| {
        word.chars().any(char::is_alphabetic)
            && spoken.iter().any(|other| related_word(word, other))
    })
}

pub(super) fn task_subject_matches(task: &Entity, quote: &str) -> bool {
    task_object_matches(
        task_action_text(task),
        task.owner.as_deref().unwrap_or_default(),
        quote,
    )
}

/// A translated title need not share words with the source. In that case
/// require the original commitment's action and object in the completion.
pub(super) fn task_completion_matches(task: &Entity, quote: &str) -> bool {
    if task_subject_matches(task, quote) || explicit_owned_completion(task, quote) {
        return true;
    }
    let report = if let Some(owner) = task.owner.as_deref() {
        let Some(report) = owned_report(owner, quote) else {
            return false;
        };
        report
    } else {
        quote
    };
    task.evidence.iter().any(|evidence| {
        super::validate::task_quote_allowed(&evidence.quote)
            && completion_action_matches(
                super::assertions::assignment_text(&evidence.quote),
                report,
            )
    })
}

/// A model may echo the verified deadline in the title. It is metadata,
/// so a completion of the same action need not repeat it.
fn task_action_text(task: &Entity) -> &str {
    let Some(due) = &task.due else {
        return &task.text;
    };
    for marker in [" bis ", " by "] {
        if let Some((action, suffix)) = task.text.rsplit_once(marker) {
            if !action.is_empty()
                && suffix.trim_end_matches(['.', '!']).to_lowercase() == due.to_lowercase()
            {
                return action;
            }
        }
    }
    &task.text
}

pub(super) fn task_title_matches(title: &str, quote: &str) -> bool {
    task_object_matches(title, "", quote)
}

fn task_object_matches(title: &str, owner: &str, quote: &str) -> bool {
    let mut object = terms(title);
    let owner = terms(owner);
    object.retain(|word| {
        word.chars().any(char::is_alphabetic)
            && !owner.contains(word)
            && ![
                "send",
                "sent",
                "email",
                "emailed",
                "submit",
                "submitted",
                "check",
                "checked",
                "review",
                "reviewed",
                "complete",
                "completed",
                "write",
                "draft",
                "covering",
                "prüfen",
                "geprüft",
                "abholen",
                "abgeholt",
                "schreiben",
                "erstellen",
                "fertigstellen",
                "liefern",
                "verschicken",
                "verschickt",
                "übernehmen",
                "vorbereiten",
            ]
            .contains(&word.as_str())
    });
    object.sort_unstable();
    object.dedup();
    let spoken = terms(quote);
    !object.is_empty()
        && object
            .iter()
            .filter(|word| spoken.iter().any(|other| related_word(word, other)))
            .count()
            >= object.len().min(2)
}

pub(super) fn same_question(existing: &Entity, title: &str, evidence: &[crate::Evidence]) -> bool {
    let left = terms(&existing.text);
    let right = terms(title);
    let same_topic = !left.is_empty()
        && !right.is_empty()
        && left
            .iter()
            .all(|a| right.iter().any(|b| related_word(a, b)))
        && right
            .iter()
            .all(|b| left.iter().any(|a| related_word(a, b)));
    let same_witness = evidence.iter().any(|new| {
        existing.evidence.iter().any(|old| {
            old.source_id == new.source_id
                && (old.quote.contains(&new.quote) || new.quote.contains(&old.quote))
        })
    });
    // A shared literal question also survives a translated or paraphrased title.
    same_topic
        || evidence.iter().any(|new| {
            existing
                .evidence
                .iter()
                .any(|old| repeated_open_topic(&old.quote, &new.quote))
        })
        || (same_witness
            && (existing.text.eq_ignore_ascii_case(title)
                || existing.evidence.iter().any(|old| {
                    old.quote.ends_with('?')
                        && !old.quote[..old.quote.len() - 1].contains('?')
                        && evidence.iter().any(|new| {
                            new.source_id == old.source_id
                                && new.quote.matches('?').count() == 1
                                && new.quote.contains(&old.quote)
                        })
                })))
}

fn repeated_open_topic(old: &str, new: &str) -> bool {
    let Some((question, _)) = old.split_once('?') else {
        return false;
    };
    let lower = new.to_lowercase();
    if new.contains('?')
        || ![
            "has not changed",
            "still open",
            "remains open",
            "still unassigned",
            "weiter offen",
            "bleibt offen",
            "unverändert",
            "steht weiterhin aus",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return false;
    }
    let original = terms(question);
    let restated = terms(new);
    !original.is_empty()
        && original
            .iter()
            .all(|word| restated.iter().any(|other| related_word(word, other)))
}

pub(super) fn explicit_owned_completion(task: &Entity, quote: &str) -> bool {
    task.owner
        .as_deref()
        .and_then(|owner| owned_report(owner, quote))
        .is_some_and(|report| completion_action_matches(task_action_text(task), report))
}

fn owned_report<'a>(owner: &str, quote: &'a str) -> Option<&'a str> {
    let (speaker, report) = quote.split_once(':')?;
    let speaker = speaker.trim();
    let speaker = speaker
        .strip_suffix(" says")
        .or_else(|| speaker.strip_suffix(" sagt"))
        .unwrap_or(speaker);
    speaker.eq_ignore_ascii_case(owner).then_some(report)
}

fn completion_action_matches(action_text: &str, report: &str) -> bool {
    let title = action_text.to_lowercase();
    let report = report.trim().to_lowercase();
    let same_object = |actions: &[&str]| {
        let spoken = terms(&report);
        let object = terms(&title)
            .into_iter()
            .filter(|word| {
                !["werde", "werden", "promise", "commit"].contains(&word.as_str())
                    && !actions.iter().any(|action| word.contains(action))
            })
            .collect::<Vec<_>>();
        !object.is_empty()
            && object
                .iter()
                .all(|word| spoken.iter().any(|other| related_word(word, other)))
    };
    if title.contains("abhol")
        || (title
            .split_whitespace()
            .any(|word| ["hole", "holen"].contains(&word))
            && title.trim_end_matches(['.', '!']).ends_with(" ab"))
    {
        return same_object(&["abhol", "hole"])
            && report.contains("abgeholt")
            && (report.contains("habe ich") || report.contains("ich habe"));
    }
    // Confine automatic suggestions to matching actions. For a creation
    // task, a direct statement that its named result is ready also suffices.
    [
        (&["send"][..], &["i sent", "i have sent"][..]),
        (&["email"], &["i emailed", "i have emailed"]),
        (&["submit"], &["i submitted", "i have submitted"]),
        (&["prüf"], &["prüfung ist erledigt"]),
        (&["verschick"], &["wurde verschickt"]),
        (
            &["erstell", "schreib", "writ", "draft"],
            &["ist fertig", "is complete"],
        ),
    ]
    .iter()
    .any(|(action, completed)| {
        action.iter().any(|verb| title.contains(verb))
            && same_object(action)
            && completed.iter().any(|verb| report.contains(verb))
    })
}
