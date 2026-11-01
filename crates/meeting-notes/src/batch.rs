//! Recovery parts retain their actual binary source partition. Ordinary answers and
//! legacy checkpoints keep their unchanged twenty-item validation contract.
use crate::{validate_answer, validation::read_checkpoint, CheckedAnswer, Kind, Source};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub(crate) const MAX_SPLIT_DEPTH: usize = 4;
pub(crate) const MAX_COMBINED_ANSWER_BYTES: usize = 2_000_000;
const MAX_CHECKPOINT_BYTES: usize = 4_000_000;

/// Transport-created partition metadata is separate from untrusted model JSON.
pub(crate) struct GeneratedPart {
    pub path: String,
    pub answer: String,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum Checkpoint {
    Single(CheckedAnswer),
    Split(SplitCheckpoint),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SplitCheckpoint {
    split_schema: u32,
    parts: Vec<CheckedPart>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedPart {
    path: String,
    answer: CheckedAnswer,
}

/// Reconstructs only the same nonempty, at-most-four-level binary split as transport.
fn source_range(path: &str, count: usize) -> Result<Range<usize>> {
    ensure!(
        path.len() <= MAX_SPLIT_DEPTH,
        "Ungültige Aufteilung des Auswertungsabschnitts."
    );
    let mut range = 0..count;
    for direction in path.bytes() {
        ensure!(
            range.len() > 1,
            "Ungültige Aufteilung des Auswertungsabschnitts."
        );
        let middle = range.start + range.len() / 2;
        match direction {
            b'L' => range.end = middle,
            b'R' => range.start = middle,
            _ => anyhow::bail!("Ungültige Aufteilung des Auswertungsabschnitts."),
        }
    }
    ensure!(!range.is_empty(), "Leerer Auswertungsabschnitt.");
    Ok(range)
}

/// Combines already screened facts and bounded rejection counts without reapplying one
/// answer's item limit to independently screened answers.
fn append(total: &mut CheckedAnswer, part: &CheckedAnswer) -> Result<()> {
    total.items.extend(part.items.iter().cloned());
    for (reason, count) in &part.rejected {
        let sum = total.rejected.entry(*reason).or_default();
        *sum = sum
            .checked_add(*count)
            .context("Ungültige Anzahl verworfener KI-Aussagen.")?;
    }
    crate::validation::validate_rejections(&total.rejected)
}

/// Checks each generated leaf against only its sent sources and the ordinary twenty-item
/// ceiling. No checkpoint is committed until the entire original batch is valid.
pub(crate) fn validate_generated(
    parts: Vec<GeneratedPart>,
    sources: &[Source],
    kind: &Kind,
    template: &str,
) -> Result<(Checkpoint, CheckedAnswer)> {
    ensure!(
        !parts.is_empty() && parts.len() <= 1 << MAX_SPLIT_DEPTH,
        "Ungültige Aufteilung des Auswertungsabschnitts."
    );
    let single = parts.len() == 1;
    let mut cursor = 0;
    let mut total = CheckedAnswer::default();
    let mut checked = Vec::new();
    for part in parts {
        ensure!(
            single == part.path.is_empty(),
            "Ungültige Aufteilung des Auswertungsabschnitts."
        );
        let range = source_range(&part.path, sources.len())?;
        ensure!(
            range.start == cursor,
            "Unvollständige oder überlappende Auswertungsabschnitte."
        );
        let answer = validate_answer(&part.answer, &sources[range.clone()], kind, template)?;
        cursor = range.end;
        append(&mut total, &answer)?;
        checked.push(CheckedPart {
            path: part.path,
            answer,
        });
    }
    ensure!(
        cursor == sources.len(),
        "Unvollständige Auswertungsabschnitte."
    );
    let checkpoint = if single {
        Checkpoint::Single(total.clone())
    } else {
        Checkpoint::Split(SplitCheckpoint {
            split_schema: 1,
            parts: checked,
        })
    };
    ensure!(
        serde_json::to_vec_pretty(&checkpoint)?.len() <= MAX_CHECKPOINT_BYTES,
        "Auswertungsabschnitt ist zu groß."
    );
    Ok((checkpoint, total))
}

/// Reads legacy checkpoints unchanged or a versioned split checkpoint. Every leaf retains
/// its original source partition and twenty-item bound; malformed evidence is never repaired.
pub(crate) fn read_checkpoint_batch(
    json: &str,
    sources: &[Source],
    kind: &Kind,
) -> Result<CheckedAnswer> {
    ensure!(
        json.len() <= MAX_CHECKPOINT_BYTES,
        "Auswertungsabschnitt ist zu groß."
    );
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Header {
        split_schema: Option<u32>,
    }
    let header: Header = serde_json::from_str(json)?;
    let Some(version) = header.split_schema else {
        return read_checkpoint(json, sources, kind);
    };
    ensure!(version == 1, "Unbekanntes Speicherformat des geteilten Auswertungsabschnitts; der Arbeitsstand bleibt erhalten.");
    let checkpoint: SplitCheckpoint = serde_json::from_str(json)?;
    ensure!(
        (2..=1 << MAX_SPLIT_DEPTH).contains(&checkpoint.parts.len()),
        "Ungültige Aufteilung des Auswertungsabschnitts."
    );
    let mut total = CheckedAnswer::default();
    let mut cursor = 0;
    for part in checkpoint.parts {
        ensure!(
            !part.path.is_empty(),
            "Ungültige Aufteilung des Auswertungsabschnitts."
        );
        let range = source_range(&part.path, sources.len())?;
        ensure!(
            range.start == cursor,
            "Unvollständige oder überlappende Auswertungsabschnitte."
        );
        let checked = read_checkpoint(
            &serde_json::to_string(&part.answer)?,
            &sources[range.clone()],
            kind,
        )?;
        cursor = range.end;
        append(&mut total, &checked)?;
    }
    ensure!(
        cursor == sources.len(),
        "Unvollständige Auswertungsabschnitte."
    );
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// Supplies four distinct synthetic sections for exact binary-partition checks.
    fn sources() -> Vec<Source> {
        (0..4)
            .map(|i| Source {
                id: format!("S{i}"),
                source: "microphone".into(),
                start_ms: i * 1000,
                end_ms: (i + 1) * 1000,
                text: format!("Der Bericht {i} bleibt lokal gespeichert."),
            })
            .collect()
    }
    /// Constructs a model statement with literal evidence only from its supplied section.
    fn answer(source: &Source) -> String {
        json!({"items":[{"text":source.text,"owner":null,"due":null,"evidence":[{"sourceId":source.id,"quote":source.text}]}]}).to_string()
    }
    /// Generates a real checked split checkpoint without allowing model JSON to set paths.
    fn split() -> (Vec<Source>, Value) {
        let sources = sources();
        let parts = ["LL", "LR", "RL", "RR"]
            .into_iter()
            .zip(&sources)
            .map(|(path, source)| GeneratedPart {
                path: path.into(),
                answer: answer(source),
            })
            .collect();
        let (checkpoint, checked) =
            validate_generated(parts, &sources, &Kind::Summary, "synthetic").unwrap();
        assert_eq!(checked.items.len(), 4);
        (sources, serde_json::to_value(checkpoint).unwrap())
    }

    #[test]
    fn nested_split_checkpoints_reopen_all_original_leaf_sources() {
        let (sources, value) = split();
        let checked = read_checkpoint_batch(&value.to_string(), &sources, &Kind::Summary).unwrap();
        assert_eq!(checked.items.len(), 4);
        for (item, source) in checked.items.iter().zip(&sources) {
            assert_eq!(item.evidence[0].source_id, source.id);
        }
    }

    #[test]
    fn malformed_split_paths_versions_fields_and_child_limits_are_rejected() {
        let (sources, original) = split();
        let mut cases = Vec::new();
        for path in ["LL", "LLLLL", "X", ""] {
            let mut value = original.clone();
            value["parts"][1]["path"] = path.into();
            cases.push(value);
        }
        let mut gap = original.clone();
        gap["parts"].as_array_mut().unwrap().remove(1);
        cases.push(gap);
        let mut reordered = original.clone();
        reordered["parts"].as_array_mut().unwrap().swap(0, 1);
        cases.push(reordered);
        let mut future = original.clone();
        future["splitSchema"] = 2.into();
        cases.push(future);
        let mut extra = original.clone();
        extra["untrusted"] = true.into();
        cases.push(extra);
        let mut oversized = original.clone();
        oversized["parts"][0]["answer"]["items"] =
            json!(vec![original["parts"][0]["answer"]["items"][0].clone(); 21]);
        cases.push(oversized);
        let mut sibling = original.clone();
        sibling["parts"][0]["answer"]["items"][0] =
            original["parts"][1]["answer"]["items"][0].clone();
        cases.push(sibling);
        for value in cases {
            let text = value.to_string();
            assert!(
                read_checkpoint_batch(&text, &sources, &Kind::Summary).is_err(),
                "{text}"
            );
        }
        let duplicate = original.to_string().replacen(
            "\"splitSchema\":1",
            "\"splitSchema\":1,\"splitSchema\":1",
            1,
        );
        assert!(read_checkpoint_batch(&duplicate, &sources, &Kind::Summary).is_err());
    }

    #[test]
    fn ordinary_checkpoint_bytes_and_twenty_item_limit_stay_unchanged() {
        let sources = sources();
        let (checkpoint, checked) = validate_generated(
            vec![GeneratedPart {
                path: String::new(),
                answer: answer(&sources[0]),
            }],
            &sources,
            &Kind::Summary,
            "synthetic",
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec_pretty(&checkpoint).unwrap(),
            serde_json::to_vec_pretty(&checked).unwrap()
        );
        let mut value = serde_json::to_value(&checked).unwrap();
        value["items"] = json!(vec![value["items"][0].clone(); 21]);
        assert!(read_checkpoint_batch(&value.to_string(), &sources, &Kind::Summary).is_err());
    }
}
