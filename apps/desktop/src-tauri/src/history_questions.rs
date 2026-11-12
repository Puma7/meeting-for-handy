//! Resolve indexed locations to current original text and map model citations back locally.
use crate::{blocking, meeting_files, model_sessions::Models, AppResult, Service};
use meeting_core::library::{SearchHit, SearchScope};
use meeting_notes::{
    questions::{self, Passage, DEFAULT_PASSAGES, MAX_PASSAGES, MAX_QUESTION_CHARS},
    AnalysisControl, Connection, TextClient,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryQuestion {
    question: String,
    scope: Option<SearchScope>,
    #[serde(default)]
    meeting_ids: Vec<String>,
    endpoint: String,
    model: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    use_saved_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Citation {
    meeting_id: String,
    meeting_title: String,
    source_id: String,
    start_ms: u64,
    quote: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryAnswer {
    searched_passages: usize,
    citations: Vec<Citation>,
}

struct LocatedPassage {
    passage: Passage,
    meeting_id: String,
    source_id: String,
    start_ms: u64,
}

#[tauri::command]
pub(crate) async fn ask_meeting_history(
    service: tauri::State<'_, Arc<Service>>,
    request: HistoryQuestion,
) -> AppResult<HistoryAnswer> {
    let service = Arc::clone(&service);
    blocking(move || ask(&service, request)).await
}

fn ask(service: &Service, request: HistoryQuestion) -> AppResult<HistoryAnswer> {
    let key = Zeroizing::new(request.api_key);
    let question = request.question.trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err("Enter a question with up to 400 characters. / Gib eine Frage mit bis zu 400 Zeichen ein.".into());
    }
    let scope = request.scope.unwrap_or_default();
    scope.validate().map_err(|_| SEARCH_ERROR)?;
    if request.meeting_ids.len() > MAX_PASSAGES
        || request
            .meeting_ids
            .iter()
            .any(|id| crate::validate_id(id).is_err())
    {
        return Err(
            "Choose up to 12 saved meetings. / Wähle bis zu zwölf gespeicherte Meetings.".into(),
        );
    }
    let operation = service.reserve_operation(crate::operations::Kind::HistoryQuestion)?;
    let control = operation
        .history_control()
        .ok_or("History question controls are unavailable")?;
    // Prevent erasure until the model call and publication of the answer finish,
    // without holding the recording transition or any recording handle.
    let source = Arc::new(
        meeting_core::archive::source_publication_lease(&service.root)
            .map_err(|_| "Meeting data is being erased. / Meetingdaten werden gerade gelöscht.")?,
    );
    let terms = questions::keywords(question);
    let hits = if let Some(index) = service.library_jobs.completed_history_index(&service.root) {
        index
            .question_candidates(&terms, &scope, &request.meeting_ids)
            .map_err(|_| SEARCH_ERROR)?
    } else if has_saved_native_sessions(service)? {
        if service
            .library_jobs
            .snapshot()
            .is_some_and(|job| job.status == "running")
        {
            return Err(SEARCH_INDEXING.into());
        }
        return Err(SEARCH_ERROR.into());
    } else {
        Vec::new()
    };
    // Keep the usual search compact and make room for every explicitly selected meeting.
    let limit = request.meeting_ids.len().max(DEFAULT_PASSAGES);
    let located = locate(service, &scope, &terms, hits, limit, control);
    if control.is_cancelled() {
        return Err(crate::operations::RECORDING_PRIORITY.into());
    }
    if located.is_empty() {
        return Ok(HistoryAnswer {
            searched_passages: 0,
            citations: Vec::new(),
        });
    }
    if service.lock()?.active.is_some() {
        return Err(crate::operations::RECORDING_PRIORITY.into());
    }
    let (passages, locations): (Vec<_>, Vec<_>) = located
        .into_iter()
        .map(|entry| {
            let title = entry.passage.meeting.clone();
            (
                entry.passage,
                (entry.meeting_id, entry.source_id, entry.start_ms, title),
            )
        })
        .unzip();
    let question = question.to_owned();
    let worker_source = Arc::clone(&source);
    let result = service
        .model_sessions
        .run_temporary_question(control, move |models, control| {
            // Recording admission may end during cancellation. Retain the source
            // publication lease while the copied excerpts are still in the worker.
            let _source = worker_source;
            let client = local_client(
                models,
                request.endpoint,
                request.model,
                key,
                request.use_saved_key,
                control,
            )?;
            questions::answer_with_control(&client, &question, &passages, control)
                .map_err(|_| ANSWER_ERROR.into())
        });
    if control.is_cancelled() {
        return Err(crate::operations::RECORDING_PRIORITY.into());
    }
    let quotes = result?;
    let citations = quotes
        .into_iter()
        .map(|item| {
            let index = item.source as usize - 1;
            let (meeting_id, source_id, start_ms, title) = &locations[index];
            Citation {
                meeting_id: meeting_id.clone(),
                meeting_title: title.clone(),
                source_id: source_id.clone(),
                start_ms: *start_ms,
                quote: item.quote,
            }
        })
        .collect();
    Ok(HistoryAnswer {
        searched_passages: locations.len(),
        citations,
    })
}

pub(crate) fn local_client(
    models: &Models,
    endpoint: String,
    model: String,
    key: Zeroizing<String>,
    use_saved_key: bool,
    control: &AnalysisControl,
) -> AppResult<TextClient> {
    control
        .check()
        .map_err(|_| crate::operations::RECORDING_PRIORITY)?;
    let key = if use_saved_key {
        if !key.is_empty() {
            return Err("Choose either the saved key or a temporary key. / Wähle entweder den gespeicherten oder einen vorübergehenden Schlüssel.".into());
        }
        crate::credential_service::load_for_run(&endpoint, "local", false)?
    } else {
        key
    };
    let mut connection = Connection {
        endpoint,
        model,
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    connection.api_key = key.to_string();
    let client = TextClient::new(&connection);
    let lease = if client.is_ok() && !control.is_cancelled() {
        crate::local_models::provider_for_endpoint(&connection.endpoint)
            .map(|provider| models.temporary_lease(&connection, provider))
            .transpose()
    } else {
        Ok(None)
    };
    use zeroize::Zeroize;
    connection.api_key.zeroize();
    control
        .check()
        .map_err(|_| crate::operations::RECORDING_PRIORITY)?;
    let mut client = client.map_err(|_| {
        "Check the local model and its address. / Prüfe das lokale Modell und seine Adresse."
    })?;
    if let Some(lease) = lease? {
        client = client.with_residency(lease).map_err(|_| ANSWER_ERROR)?;
    }
    Ok(client)
}

const SEARCH_ERROR: &str = "Local search is unavailable. Update the search index in Settings. / Lokale Suche ist nicht verfügbar. Aktualisiere den Suchindex in den Einstellungen.";
const SEARCH_INDEXING: &str = "The local search index is still updating. Try again when it finishes. / Der lokale Suchindex wird noch aktualisiert. Versuche es danach erneut.";
const ANSWER_ERROR: &str = "The local model did not return a verifiable answer. Check the model or rephrase the question. / Das lokale Modell lieferte keine überprüfbare Antwort. Prüfe das Modell oder formuliere die Frage anders.";

/// Establish that no native source could have been missed when no complete index is available.
fn has_saved_native_sessions(service: &Service) -> AppResult<bool> {
    let sessions = meeting_core::metadata::checked_directory(&service.root.join("sessions"))
        .map_err(|_| SEARCH_ERROR)?;
    for entry in std::fs::read_dir(sessions).map_err(|_| SEARCH_ERROR)? {
        let entry = entry.map_err(|_| SEARCH_ERROR)?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|id| crate::validate_id(id).is_ok())
        {
            let kind = entry.file_type().map_err(|_| SEARCH_ERROR)?;
            if kind.is_dir() || kind.is_symlink() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn locate(
    service: &Service,
    scope: &SearchScope,
    terms: &[String],
    hits: Vec<SearchHit>,
    limit: usize,
    control: &AnalysisControl,
) -> Vec<LocatedPassage> {
    let mut groups: Vec<Vec<SearchHit>> = Vec::new();
    for hit in hits {
        if let Some(group) = groups.iter_mut().find(|group| group[0].id == hit.id) {
            group.push(hit);
        } else {
            groups.push(vec![hit]);
        }
    }
    let mut first = Vec::new();
    let mut additional = Vec::new();
    for group in groups {
        if control.is_cancelled() {
            break;
        }
        let id = &group[0].id;
        let Ok(folder) = meeting_files::session_folder(&service.root, id) else {
            continue;
        };
        let data = match meeting_core::archive::read_archived_transcript(&folder) {
            Ok(Some(data)) => data,
            Ok(None) => match meeting_core::inspect_journals(&folder) {
                Ok(data) => data,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        if data.id != *id {
            continue;
        }
        let Ok(meta) = meeting_core::metadata::read(&service.root, id) else {
            continue;
        };
        if !scope.permits(&data.started_at, meta.pinned) {
            continue;
        }
        let current_title = meta.title.as_deref().unwrap_or(&data.title);
        let title: String = current_title.chars().take(160).collect();
        let mut matches = group.into_iter().filter_map(|hit| {
            let file = hit.source_id?;
            let mut found = data.parts.iter().filter(|p| p.file == file);
            let part = found.next()?;
            if found.next().is_some() || part.status != "complete" {
                return None;
            }
            let text = questions::excerpt(part.text.as_deref()?, terms);
            if text.is_empty() || !questions::matches_keywords(text, terms) {
                return None;
            }
            Some(LocatedPassage {
                passage: Passage {
                    id: 0,
                    meeting: title.clone(),
                    text: text.into(),
                },
                meeting_id: hit.id,
                source_id: file,
                start_ms: part.start_sample.saturating_mul(1000)
                    / u64::from(meeting_core::SAMPLE_RATE),
            })
        });
        if let Some(entry) = matches.next() {
            first.push(entry);
            if first.len() == limit {
                break;
            }
            additional.extend(matches.take(2));
        }
    }
    first
        .into_iter()
        .chain(additional)
        .take(limit)
        .enumerate()
        .map(|(index, mut entry)| {
            entry.passage.id = index as u32 + 1;
            entry
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use meeting_core::library::LibraryIndex;
    use std::{
        fs,
        path::Path,
        time::{Duration, Instant},
    };

    const FIRST: &str = "0123456789abcdef0123456789abcdef";
    const SECOND: &str = "1123456789abcdef0123456789abcdef";
    const FILE: &str = "system-000000016000.wav";
    const QUESTION: &str =
        "Was wurde in den beiden Meetings zum Liefertermin für Projekt Orion gesagt?";

    struct OwnedFixture(std::path::PathBuf);

    impl OwnedFixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hm-history-question-{}",
                meeting_notes::new_job_id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for OwnedFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture_sessions(root: &Path) {
        for (id, title, text) in [
            (FIRST, "Synthetische Projektbesprechung", "Für Projekt Orion ist der Liefertermin am Mittwoch, dem 12. November. Die Kundenfreigabe ist weiterhin offen."),
            (SECOND, "Synthetische Folgebesprechung", "Für Projekt Orion wurde Freitag, der 14. November, als Liefertermin vorgeschlagen. Die Änderung ist noch nicht bestätigt."),
        ] {
            let folder = root.join("sessions").join(id);
            fs::create_dir_all(&folder).unwrap();
            let manifest = serde_json::json!({
                "schemaVersion":2, "id":id, "title":title, "startedAt":"2026-11-09T08:00:00Z",
                "status":"complete", "modelId":"synthetic", "chunkSeconds":30,
                "parts":[{"file":FILE, "source":"system", "startSample":16000, "sampleCount":16000,
                    "confirmedSampleCount":16000, "text":text, "status":"complete"}]
            });
            fs::write(folder.join("session.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        }
    }

    fn fixtures(root: &Path) -> Service {
        let service = Service::new(root.to_owned()).unwrap();
        fixture_sessions(root);
        let mut index = LibraryIndex::open(root).unwrap();
        for id in [FIRST, SECOND] {
            let mut indexed =
                meeting_core::inspect_journals(&root.join("sessions").join(id)).unwrap();
            indexed.parts[0]
                .text
                .as_mut()
                .unwrap()
                .push_str(" STALE_INDEX_SENTINEL");
            index
                .upsert(&indexed, &Default::default(), "synthetic")
                .unwrap();
        }
        service
    }

    fn wait_for_index(service: &Service, id: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let job = service.library_jobs.snapshot().unwrap();
            assert_eq!(job.job_id, id);
            if job.status != "running" {
                assert_eq!(job.status, "complete", "{:?}", job.error);
                return;
            }
            assert!(Instant::now() < deadline, "index did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn questions_distinguish_empty_storage_from_missing_and_partial_indexes() {
        let fixture = crate::test_profile::TestProfile::new().unwrap();
        let service = Service::new(fixture.path().to_owned()).unwrap();
        let ask_local = |question: &str, meeting_ids| {
            ask(
                &service,
                HistoryQuestion {
                    question: question.into(),
                    scope: None,
                    meeting_ids,
                    endpoint: String::new(),
                    model: String::new(),
                    api_key: String::new(),
                    use_saved_key: false,
                },
            )
        };
        let cache = fixture.path().join("library-index/library.sqlite3");
        assert_eq!(ask_local(QUESTION, vec![]).unwrap().searched_passages, 0);
        assert!(!cache.exists());

        fixture_sessions(fixture.path());
        assert_eq!(
            ask_local(QUESTION, vec![SECOND.into()]).unwrap_err(),
            SEARCH_ERROR
        );
        assert!(!cache.exists());

        let first =
            meeting_core::inspect_journals(&fixture.path().join("sessions").join(FIRST)).unwrap();
        LibraryIndex::open(fixture.path())
            .unwrap()
            .upsert(&first, &Default::default(), "partial")
            .unwrap();
        let (release, waiting) = std::sync::mpsc::channel();
        let initial = service
            .library_jobs
            .start_work("search-index", move |_, _, _| {
                waiting.recv_timeout(Duration::from_secs(10))?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            ask_local(QUESTION, vec![SECOND.into()]).unwrap_err(),
            SEARCH_INDEXING
        );
        release.send(()).unwrap();
        wait_for_index(&service, &initial);

        let full = service
            .library_jobs
            .start_scan(service.root.clone(), false, None)
            .unwrap();
        wait_for_index(&service, &full);
        let no_match = ask_local("Quantenkryptographie", vec![SECOND.into()]).unwrap();
        assert_eq!(no_match.searched_passages, 0);
        assert!(no_match.citations.is_empty());
        assert!(ask_local(QUESTION, vec![SECOND.into()])
            .unwrap_err()
            .contains("local model"));
    }

    #[test]
    fn question_uses_current_sections_and_meeting_identity_even_when_file_names_match() {
        let fixture = OwnedFixture::new();
        let service = fixtures(&fixture.0);
        let terms = questions::keywords(QUESTION);
        let scope = SearchScope::default();
        let index = LibraryIndex::open(&service.root).unwrap();
        let hits = index.question_candidates(&terms, &scope, &[]).unwrap();
        let selected = index
            .question_candidates(&terms, &scope, &[SECOND.into()])
            .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, SECOND);
        assert!(index
            .question_candidates(&terms, &scope, &["0123456789abcdef0123456789abcdee".into()])
            .unwrap()
            .is_empty());
        let control = AnalysisControl::default();
        let passages = locate(
            &service,
            &scope,
            &terms,
            hits.clone(),
            DEFAULT_PASSAGES,
            &control,
        );
        assert_eq!(passages.len(), 2);
        for id in [FIRST, SECOND] {
            let passage = passages
                .iter()
                .find(|entry| entry.meeting_id == id)
                .unwrap();
            assert_eq!(passage.source_id, FILE);
            assert_eq!(passage.start_ms, 1000);
            assert!(!passage.passage.text.contains("STALE_INDEX_SENTINEL"));
        }
        let vip_scope = SearchScope {
            pinned_only: true,
            ..scope
        };
        assert!(locate(
            &service,
            &vip_scope,
            &terms,
            hits,
            DEFAULT_PASSAGES,
            &control
        )
        .is_empty());

        let terms = questions::keywords("Wie ist die anderung?");
        let hits = index.question_candidates(&terms, &scope, &[]).unwrap();
        assert_eq!(hits.len(), 1);
        let passages = locate(&service, &scope, &terms, hits, DEFAULT_PASSAGES, &control);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].meeting_id, SECOND);
        assert!(passages[0].passage.text.contains("Änderung"));
    }

    #[test]
    fn stale_candidates_do_not_consume_verified_meeting_or_passage_limits() {
        let fixture = OwnedFixture::new();
        let service = fixtures(&fixture.0);
        let scope = SearchScope::default();
        let terms = questions::keywords("Liefertermin");
        let index = LibraryIndex::open(&service.root).unwrap();
        let mut ranked = index
            .question_candidates(&terms, &scope, &[FIRST.into()])
            .unwrap();
        let template = ranked.pop().unwrap();
        let sessions = service.root.join("sessions");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(sessions.join(FIRST).join("session.json")).unwrap())
                .unwrap();
        let ids: Vec<_> = (0..(MAX_PASSAGES * 2 + 1))
            .map(|sequence| format!("{sequence:032x}"))
            .collect();
        for (position, id) in ids.iter().enumerate() {
            let folder = sessions.join(id);
            fs::create_dir(&folder).unwrap();
            manifest["id"] = serde_json::json!(id);
            fs::write(
                folder.join("session.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            let hit = SearchHit {
                id: id.clone(),
                ..template.clone()
            };
            let stale = |part| SearchHit {
                source_id: Some(format!("system-stale-{part}.wav")),
                ..hit.clone()
            };
            if position < MAX_PASSAGES {
                ranked.push(stale(0));
            } else {
                if position == MAX_PASSAGES {
                    ranked.extend((0..3).map(stale));
                }
                ranked.push(hit);
            }
        }
        let passages = locate(
            &service,
            &scope,
            &terms,
            ranked,
            MAX_PASSAGES,
            &AnalysisControl::default(),
        );
        assert_eq!(passages.len(), MAX_PASSAGES);
        for (index, passage) in passages.iter().enumerate() {
            assert_eq!(passage.meeting_id, ids[MAX_PASSAGES + index]);
            assert_eq!(passage.source_id, FILE);
            assert_eq!(passage.passage.id, index as u32 + 1);
            assert!(passage.passage.text.contains("Mittwoch"));
        }
    }

    #[test]
    #[ignore = "real existing local Gemma 4 12B; explicitly supply an isolated fixture path"]
    fn gemma12b_answers_from_two_originals_and_abstains_on_missing_information() {
        let root =
            std::path::PathBuf::from(std::env::var_os("HANDYMEETING_QUESTION_TEST_ROOT").unwrap());
        assert!(root.is_absolute());
        fs::create_dir(&root).unwrap();
        let service = fixtures(&root);
        let scan = service
            .library_jobs
            .start_scan(root.clone(), false, None)
            .unwrap();
        wait_for_index(&service, &scan);
        let endpoint = "http://127.0.0.1:1235/v1/".to_owned();
        assert!(matches!(
            crate::local_models::provider_for_endpoint(&endpoint),
            Some(meeting_notes::residency::LocalProvider::LmStudio)
        ));
        let run = |question: &str, meeting_ids: Vec<String>| {
            ask(
                &service,
                HistoryQuestion {
                    question: question.into(),
                    scope: None,
                    meeting_ids,
                    endpoint: endpoint.clone(),
                    model: "google/gemma-4-12b-qat".into(),
                    api_key: String::new(),
                    use_saved_key: false,
                },
            )
            .unwrap()
        };
        let started = std::time::Instant::now();
        let answer = run(QUESTION, vec![]);
        let comparison_seconds = started.elapsed().as_secs_f64();
        assert_eq!(answer.searched_passages, 2);
        for (id, term) in [(FIRST, "Mittwoch"), (SECOND, "vorgeschlagen")] {
            assert!(
                answer.citations.iter().any(|quote| quote.meeting_id == id
                    && quote.source_id == FILE
                    && quote.start_ms == 1000
                    && quote.quote.contains(term)),
                "{answer:?}"
            );
        }
        let started = std::time::Instant::now();
        let missing = run("Wie lautet das Passwort für Projekt Orion?", vec![]);
        let abstention_seconds = started.elapsed().as_secs_f64();
        assert_eq!(missing.searched_passages, 2);
        assert!(missing.citations.is_empty(), "{missing:?}");
        let started = std::time::Instant::now();
        let selected = run(
            "An welchem Wochentag wurde der Liefertermin für Projekt Orion bestätigt?",
            vec![FIRST.into()],
        );
        let selection_seconds = started.elapsed().as_secs_f64();
        assert_eq!(selected.searched_passages, 1);
        assert!(selected
            .citations
            .iter()
            .any(|citation| citation.quote.contains("Mittwoch")));
        assert!(selected
            .citations
            .iter()
            .all(|citation| citation.meeting_id == FIRST && !citation.quote.contains("Freitag")));
        println!(
            "{}",
            serde_json::json!({
                "model":"google/gemma-4-12b-qat", "synthetic":true, "actualLocalProvider":true,
                "comparisonSeconds":comparison_seconds, "abstentionSeconds":abstention_seconds, "selectionSeconds":selection_seconds,
                "twoMeetingsAndOriginals":true, "proposalPreserved":true, "noEvidenceAbstention":true,
            })
        );
    }
}
