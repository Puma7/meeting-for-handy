//! Stable, append-only selection of confirmed transcript text for live notes.
use crate::Source;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Longest transcript excerpt handed to the model as one source, as in post-meeting analysis.
pub const SOURCE_CHARS: usize = 1600;
/// Text budget of one live update. Larger backlogs are processed in further windows.
pub const WINDOW_CHARS: usize = 6500;

/// One confirmed transcript part supplied by orchestration. `key` identifies the audio part
/// (its file name) and stays the same when recognition later revises the part's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveText {
    pub key: String,
    pub track: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A transcript excerpt exactly as it was shown to the model in one live window.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSource {
    pub id: String,
    pub key: String,
    pub chunk: u32,
    pub source: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}
impl LiveSource {
    /// Check the current text at this exact chunk, including prior identical wording elsewhere.
    pub fn matches_transcript(&self, text: &str) -> bool {
        chunks(text)
            .into_iter()
            .any(|(index, value)| index == self.chunk && value == self.text)
    }

    pub fn as_source(&self) -> Source {
        Source {
            id: self.id.clone(),
            source: self.source.clone(),
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            text: self.text.clone(),
        }
    }
}

/// Splits one part like post-meeting analysis does and drops chunks without text.
fn chunks(text: &str) -> Vec<(u32, String)> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(SOURCE_CHARS)
        .enumerate()
        .map(|(index, chunk)| (index as u32 + 1, chunk.iter().collect::<String>()))
        .filter(|(_, value)| !value.trim().is_empty())
        .collect()
}

/// Short identifier the model copies into its evidence: track letter, start second and,
/// when needed, the chunk number. A revised text or a clash gets a revision suffix, so an
/// identifier never names two different texts within one run.
fn base_id(track: &str, start_ms: u64, chunk: u32) -> String {
    let letter = if track == "microphone" { 'M' } else { 'C' };
    let second = start_ms / 1000;
    if chunk > 1 {
        format!("{letter}{second}-{chunk}")
    } else {
        format!("{letter}{second}")
    }
}

/// Confirmed text that no stored window has covered yet, in transcript order. A part whose
/// text was revised after it was summarized is offered again under a new identifier; the
/// stored window keeps the text the model actually saw.
pub fn pending(processed: &[LiveSource], texts: &[LiveText]) -> Vec<LiveSource> {
    let mut seen: HashMap<(&str, u32), Vec<&str>> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for source in processed {
        seen.entry((source.key.as_str(), source.chunk))
            .or_default()
            .push(source.text.as_str());
        used.insert(source.id.clone());
    }
    let mut ordered: Vec<&LiveText> = texts.iter().collect();
    ordered.sort_by(|a, b| (a.start_ms, &a.track, &a.key).cmp(&(b.start_ms, &b.track, &b.key)));
    let mut result = Vec::new();
    for text in ordered {
        for (chunk, value) in chunks(&text.text) {
            if seen
                .get(&(text.key.as_str(), chunk))
                .is_some_and(|texts| texts.contains(&value.as_str()))
            {
                continue;
            }
            let base = base_id(&text.track, text.start_ms, chunk);
            let mut id = base.clone();
            let mut revision = 1;
            while used.contains(&id) {
                revision += 1;
                id = format!("{base}r{revision}");
            }
            used.insert(id.clone());
            result.push(LiveSource {
                id,
                key: text.key.clone(),
                chunk,
                source: text.track.clone(),
                start_ms: text.start_ms,
                end_ms: text.end_ms,
                text: value,
            });
        }
    }
    result
}

/// Number of characters still waiting for a live update.
pub fn pending_chars(pending: &[LiveSource]) -> usize {
    pending.iter().map(|s| s.text.chars().count()).sum()
}

/// The next window: whole sources in order up to the text budget, at least one source.
pub fn next_window(pending: &[LiveSource]) -> Vec<LiveSource> {
    let mut window = Vec::new();
    let mut size = 0;
    for source in pending {
        let chars = source.text.chars().count();
        if !window.is_empty() && size + chars > WINDOW_CHARS {
            break;
        }
        size += chars;
        window.push(source.clone());
    }
    window
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(key: &str, track: &str, start_s: u64, value: &str) -> LiveText {
        LiveText {
            key: key.into(),
            track: track.into(),
            start_ms: start_s * 1000,
            end_ms: start_s * 1000 + 30_000,
            text: value.into(),
        }
    }

    #[test]
    fn identifiers_stay_stable_when_tracks_complete_out_of_order() {
        let late = text("system-0", "system", 0, "Wir beginnen mit dem Budget.");
        let early = text(
            "microphone-30",
            "microphone",
            30,
            "Ich habe eine Frage dazu.",
        );
        let first = pending(&[], std::slice::from_ref(&early));
        assert_eq!(first[0].id, "M30");
        // The earlier system part completes after the microphone part was summarized.
        let second = pending(&first, &[early.clone(), late.clone()]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, "C0");
        assert_eq!(second[0].text, late.text);
    }

    #[test]
    fn a_revised_text_is_offered_again_under_a_new_identifier() {
        let original = text("system-0", "system", 0, "Wir nehmen Variante A.");
        let processed = pending(&[], std::slice::from_ref(&original));
        let mut revised = original.clone();
        revised.text = "Wir nehmen Variante B.".into();
        let again = pending(&processed, &[revised]);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].id, "C0r2");
        assert_eq!(again[0].key, "system-0");
        // Unchanged text is never offered twice.
        assert!(pending(&processed, &[original]).is_empty());
    }

    #[test]
    fn long_parts_are_chunked_and_windows_keep_whole_sources_within_budget() {
        let long = "a".repeat(SOURCE_CHARS + 10);
        let parts: Vec<_> = (0..10)
            .map(|i| text(&format!("system-{i}"), "system", i * 30, &long))
            .collect();
        let all = pending(&[], &parts);
        assert_eq!(all.len(), 20);
        assert_eq!(all[1].id, "C0-2");
        assert_eq!(all[1].text.chars().count(), 10);
        let window = next_window(&all);
        assert!(pending_chars(&window) <= WINDOW_CHARS);
        assert!(window.len() < all.len(), "a backlog needs several windows");
        let oversized = vec![LiveSource {
            text: "b".repeat(WINDOW_CHARS + 1),
            ..all[0].clone()
        }];
        assert_eq!(
            next_window(&oversized).len(),
            1,
            "progress is always possible"
        );
    }

    #[test]
    fn empty_text_is_never_offered() {
        assert!(pending(&[], &[text("system-0", "system", 0, "   ")]).is_empty());
    }
}
