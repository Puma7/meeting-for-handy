use crate::validation::{read_checkpoint, validate_rejections, verify_facts};
use crate::{
    validate_answer, validate_template, Analysis, AnalysisControl, AnalysisProgress, Connection,
    PendingAnalysis, Rejections, Section, Source, Template, TextClient, PROMPT_VERSION,
    RESULT_SCHEMA,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::Path,
};
/// Hashes the ordered serialized source snapshot to bind saved work to its original input text.
pub fn source_hash(sources: &[Source]) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(sources)?)
    ))
}
/// Syncs a new JSON work state and atomically commits it without replacing an existing version.
pub(crate) fn create_json(path: &Path, data: &impl Serialize) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("Auswertungsordner fehlt.")?)?;
    file.write_all(&serde_json::to_vec_pretty(data)?)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|_|anyhow::anyhow!("Auswertungsstand konnte nicht atomar gespeichert werden; bestehende Fassungen bleiben erhalten."))?;
    Ok(())
}

/// A stored file uses a result schema this build does not know, e.g. one written by a newer
/// version. It is left untouched and reported instead of being guessed at.
#[derive(Debug)]
pub struct UnsupportedFormat(pub u32);
impl std::fmt::Display for UnsupportedFormat {
    /// Formats the unsupported-schema error while stating that the saved work remains unchanged.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dieser Auswertungsstand verwendet ein unbekanntes Speicherformat ({}). Er bleibt unverändert erhalten.", self.0)
    }
}
impl std::error::Error for UnsupportedFormat {}

/// Stored prompt and schema versions are labels of the work state, not compatibility gates:
/// earlier prompt versions stay readable, only resuming requires the current one.
fn validate_versions(stored: &Analysis) -> Result<()> {
    if !(1..=RESULT_SCHEMA).contains(&stored.result_schema) {
        return Err(UnsupportedFormat(stored.result_schema).into());
    }
    ensure!(
        !stored.prompt_version.trim().is_empty()
            && stored.prompt_version.len() <= 100
            && !stored.prompt_version.chars().any(char::is_control),
        "Gespeicherte Promptfassung ist ungültig."
    );
    Ok(())
}

/// Recognizes symbolic links and Windows reparse points before reading stored analysis files.
pub(crate) fn is_linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Reads a finished result of any known prompt version. Callers that continue or extend work
/// must compare `prompt_version` with [`PROMPT_VERSION`] themselves.
pub fn read_analysis(path: &Path) -> Result<Analysis> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !is_linked(&metadata) && metadata.len() <= 64_000_000,
        "Auswertungsdatei ist nicht sicher lesbar oder zu groß."
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(64_000_001)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64_000_000, "Auswertungsdatei zu groß.");
    // Read the format label before assuming any of the current payload shapes.
    // A newer file may no longer deserialize as today's Analysis at all.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Header {
        #[serde(default)]
        result_schema: Option<u32>,
    }
    let header: Header = serde_json::from_slice(&bytes)?;
    let schema = header.result_schema.unwrap_or(1);
    if !(1..=RESULT_SCHEMA).contains(&schema) {
        return Err(UnsupportedFormat(schema).into());
    }
    let result: Analysis = serde_json::from_slice(&bytes)?;
    validate_analysis(&result)?;
    Ok(result)
}

/// Checks source-count/text bounds and unique IDs before creating a model request.
fn validate_sources(sources: &[Source]) -> Result<()> {
    ensure!(
        !sources.is_empty()
            && sources.len() <= 100_000
            && sources
                .iter()
                .all(|s| !s.text.is_empty() && s.text.chars().count() <= 1800),
        "Kein gültiger Transkriptstand für die Auswertung vorhanden."
    );
    ensure!(
        sources.iter().map(|s| &s.id).collect::<HashSet<_>>().len() == sources.len(),
        "Doppelte Quellenkennung."
    );
    Ok(())
}

/// Reading a saved request never creates a model client or restores cloud consent.
/// Only snapshots of the current prompt version are returned, because this is the resume gate.
pub fn read_snapshot(path: &Path) -> Result<Analysis> {
    let snapshot = inspect_snapshot(path)?;
    ensure!(
        snapshot.prompt_version == PROMPT_VERSION,
        "Dieser Arbeitsstand wurde mit einer anderen Promptfassung ({}) erstellt. Er bleibt unverändert erhalten, wird aber nicht fortgesetzt. Bitte eine neue Auswertung starten.",
        snapshot.prompt_version
    );
    Ok(snapshot)
}

/// Integrity check of a run snapshot independent of its prompt version (read-only discovery).
fn inspect_snapshot(path: &Path) -> Result<Analysis> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !is_linked(&metadata) && metadata.len() <= 64_000_000,
        "Gespeicherter Arbeitsstand ist nicht sicher lesbar."
    );
    let snapshot: Analysis = serde_json::from_slice(&fs::read(path)?)?;
    validate_versions(&snapshot)?;
    validate_template(&snapshot.template)?;
    validate_sources(&snapshot.sources)?;
    ensure!(
        snapshot.id.len() == 32
            && snapshot.id.bytes().all(|c| c.is_ascii_hexdigit())
            && !snapshot.meeting_id.is_empty()
            && snapshot.meeting_id.len() <= 128
            && snapshot.source_hash == source_hash(&snapshot.sources)?
            && snapshot.sections.is_empty()
            && ["local", "cloud"].contains(&snapshot.mode.as_str())
            && !snapshot.model.is_empty()
            && snapshot.model.len() <= 512
            && !snapshot.endpoint.is_empty()
            && snapshot.endpoint.len() <= 4096,
        "Gespeicherter Arbeitsstand ist beschädigt oder nicht kompatibel."
    );
    Ok(snapshot)
}

/// Inspects saved unfinished work and counts validated checkpoints without creating a client or restoring consent.
pub fn read_pending_analysis(folder: &Path) -> Result<Option<PendingAnalysis>> {
    if folder.join("result.json").exists() || !folder.join("snapshot.json").exists() {
        return Ok(None);
    }
    let snapshot = inspect_snapshot(&folder.join("snapshot.json"))?;
    ensure!(
        folder.file_name().and_then(|s| s.to_str()) == Some(snapshot.id.as_str()),
        "Arbeitsstand und Auswertungsordner passen nicht zusammen."
    );
    let batches = source_batches(&snapshot.sources);
    let mut confirmed_tasks = 0;
    for (step_index, step) in snapshot.template.steps.iter().enumerate() {
        for (batch_index, sources) in batches.iter().enumerate() {
            let path = folder.join(format!("step-{step_index:04}-{batch_index:06}.json"));
            if path.exists() {
                let metadata = fs::symlink_metadata(&path)?;
                ensure!(
                    metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && metadata.len() <= 4_000_000,
                    "Bestätigter Auswertungsabschnitt ist nicht sicher lesbar."
                );
                let parsed = read_checkpoint(&fs::read_to_string(&path)?, sources, &step.kind)?;
                // A rejected name batch is requested again on resume, so it is not confirmed work.
                if crate::name_candidates::validate_items(&snapshot.template.id, &parsed.items)
                    .is_ok()
                {
                    confirmed_tasks += 1;
                }
            }
        }
    }
    Ok(Some(PendingAnalysis {
        resumable: snapshot.prompt_version == PROMPT_VERSION,
        prompt_version: snapshot.prompt_version,
        id: snapshot.id,
        meeting_id: snapshot.meeting_id,
        source_hash: snapshot.source_hash,
        total_tasks: snapshot.template.steps.len() * batches.len(),
        template: snapshot.template,
        created_at: snapshot.created_at,
        model: snapshot.model,
        mode: snapshot.mode,
        endpoint: snapshot.endpoint,
        confirmed_tasks,
    }))
}
/// Checks schema, template, source hashes and literal evidence; these checks do not prove interpretation.
pub(crate) fn validate_analysis(result: &Analysis) -> Result<()> {
    validate_versions(result)?;
    validate_template(&result.template)?;
    crate::name_candidates::validate(result)?;
    ensure!(
        source_hash(&result.sources)? == result.source_hash
            && result.sections.len() == result.template.steps.len(),
        "Auswertungsstand ist beschädigt oder nicht kompatibel."
    );
    for (section, step) in result.sections.iter().zip(&result.template.steps) {
        ensure!(
            section.id == step.id && section.name == step.name && section.kind == step.kind,
            "Auswertung und Vorlagenstand passen nicht zusammen."
        );
        verify_facts(&section.items, &result.sources, &section.kind)?;
        validate_rejections(&section.rejected)?;
        // Schema 1 failed on any invalid statement, so it can never record discarded ones.
        ensure!(
            result.result_schema >= 2 || section.rejected.is_empty(),
            "Auswertungsstand ist beschädigt oder nicht kompatibel."
        );
    }
    ensure!(
        result
            .sources
            .iter()
            .map(|s| &s.id)
            .collect::<HashSet<_>>()
            .len()
            == result.sources.len(),
        "Doppelte Quellenkennung im gespeicherten Ergebnis."
    );
    Ok(())
}

/// Sources and immutable run snapshots are supplied by orchestration, outside transport.
pub fn analyze(
    root: &Path,
    meeting_id: &str,
    sources: Vec<Source>,
    template: Template,
    connection: Connection,
    resume: Option<&str>,
) -> Result<Analysis> {
    analyze_controlled(
        root,
        meeting_id,
        sources,
        template,
        connection,
        resume,
        &AnalysisControl::default(),
        |_| {},
    )
}

/// Retains a rejected checkpoint at a numbered path before the batch is requested again.
fn set_aside_rejected(path: &Path) -> Result<()> {
    for attempt in 0..1000 {
        let target = path.with_extension(format!("rejected-{attempt}.json"));
        if !target.exists() {
            fs::rename(path, target)?;
            return Ok(());
        }
    }
    anyhow::bail!("Zu viele verworfene Antworten für diesen Arbeitsschritt.")
}
#[allow(clippy::too_many_arguments)]
/// Runs cancellable text-only batches against immutable snapshots and checkpoints; callers supply consent.
/// Completion is reported only after the durable result JSON is committed.
pub fn analyze_controlled(
    root: &Path,
    meeting_id: &str,
    sources: Vec<Source>,
    template: Template,
    connection: Connection,
    resume: Option<&str>,
    control: &AnalysisControl,
    on_progress: impl FnMut(AnalysisProgress),
) -> Result<Analysis> {
    analyze_controlled_resident(
        root,
        meeting_id,
        sources,
        template,
        connection,
        resume,
        control,
        None,
        on_progress,
    )
}

/// The coordinator supplies only a local model lease, never session/audio access to transport.
/// The caller retains failed work's lease until retry or explicit cancellation.
#[allow(clippy::too_many_arguments)]
pub fn analyze_controlled_resident(
    root: &Path,
    meeting_id: &str,
    sources: Vec<Source>,
    template: Template,
    connection: Connection,
    resume: Option<&str>,
    control: &AnalysisControl,
    residency: Option<std::sync::Arc<crate::residency::ModelLease>>,
    mut on_progress: impl FnMut(AnalysisProgress),
) -> Result<Analysis> {
    control.check()?;
    validate_template(&template)?;
    validate_sources(&sources)?;
    let client = TextClient::new(&connection)?;
    let client = match residency {
        Some(lease) => client.with_residency(lease)?,
        None => client,
    };
    let id = resume
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    ensure!(
        id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()),
        "Ungültige Auswertungskennung."
    );
    fs::create_dir_all(root)?;
    let folder = root.join(&id);
    let hash = source_hash(&sources)?;
    let mut analysis = Analysis {
        id,
        meeting_id: meeting_id.to_string(),
        source_hash: hash,
        template,
        sections: Vec::new(),
        sources,
        created_at: chrono::Utc::now().to_rfc3339(),
        model: connection.model.clone(),
        mode: connection.mode.clone(),
        endpoint: client.base.to_string(),
        prompt_version: PROMPT_VERSION.to_string(),
        result_schema: RESULT_SCHEMA,
    };
    if resume.is_some() {
        let previous = read_snapshot(&folder.join("snapshot.json"))?;
        ensure!(
            previous.id == analysis.id
                && previous.source_hash == analysis.source_hash
                && previous.meeting_id == analysis.meeting_id
                && previous.model == analysis.model
                && previous.mode == analysis.mode
                && previous.endpoint == analysis.endpoint
                && previous.prompt_version == PROMPT_VERSION
                && serde_json::to_vec(&previous.template)?
                    == serde_json::to_vec(&analysis.template)?,
            "Auswertungsstand passt nicht zu Transkript, Modell, Anbieter oder Vorlage."
        );
        analysis.created_at = previous.created_at;
        if folder.join("result.json").exists() {
            let result = read_analysis(&folder.join("result.json"))?;
            ensure!(
                result.id == analysis.id
                    && result.meeting_id == analysis.meeting_id
                    && result.source_hash == analysis.source_hash
                    && result.model == analysis.model
                    && result.mode == analysis.mode
                    && result.endpoint == analysis.endpoint
                    && result.prompt_version == PROMPT_VERSION
                    && serde_json::to_vec(&result.template)?
                        == serde_json::to_vec(&analysis.template)?,
                "Gespeicherte Auswertung passt nicht zum Arbeitsstand."
            );
            let batches = source_batches(&result.sources);
            let tasks = result.template.steps.len() * batches.len();
            on_progress(AnalysisProgress {
                run_id: result.id.clone(),
                stage: "finished".into(),
                step_name: None,
                step_index: result.template.steps.len(),
                total_steps: result.template.steps.len(),
                batch_index: batches.len(),
                total_batches: batches.len(),
                confirmed_tasks: tasks,
                total_tasks: tasks,
            });
            return Ok(result);
        }
    } else {
        fs::create_dir(&folder)?;
        create_json(&folder.join("snapshot.json"), &analysis)?;
    }
    let batches = source_batches(&analysis.sources);
    let mut progress = AnalysisProgress {
        run_id: analysis.id.clone(),
        stage: "preparing".into(),
        step_name: None,
        step_index: 0,
        total_steps: analysis.template.steps.len(),
        batch_index: 0,
        total_batches: batches.len(),
        confirmed_tasks: 0,
        total_tasks: analysis.template.steps.len() * batches.len(),
    };
    on_progress(progress.clone());
    for (step_index, step) in analysis.template.steps.iter().enumerate() {
        let mut items = Vec::new();
        let mut rejected = Rejections::new();
        for (batch_index, sources) in batches.iter().enumerate() {
            control.check()?;
            progress.step_name = Some(step.name.clone());
            progress.step_index = step_index + 1;
            progress.batch_index = batch_index + 1;
            let path = folder.join(format!("step-{step_index:04}-{batch_index:06}.json"));
            let mut resumed = None;
            if path.exists() {
                progress.stage = "validating".into();
                on_progress(progress.clone());
                let parsed = read_checkpoint(&fs::read_to_string(&path)?, sources, &step.kind)?;
                if crate::name_candidates::validate_items(&analysis.template.id, &parsed.items)
                    .is_ok()
                {
                    resumed = Some(parsed);
                } else {
                    // Keep the rejected answer as evidence and request this batch again.
                    set_aside_rejected(&path)?;
                }
            }
            let parsed = if let Some(parsed) = resumed {
                parsed
            } else {
                progress.stage = "requesting".into();
                on_progress(progress.clone());
                let answer = client.generate_controlled(sources, step, control)?;
                control.check()?;
                progress.stage = "validating".into();
                on_progress(progress.clone());
                // Invalid statements are discarded and counted; only valid ones are saved.
                let parsed = validate_answer(&answer, sources, &step.kind, &analysis.template.id)?;
                crate::name_candidates::validate_items(&analysis.template.id, &parsed.items)?;
                control.check()?;
                progress.stage = "saving".into();
                on_progress(progress.clone());
                create_json(&path, &parsed)?;
                parsed
            };
            progress.confirmed_tasks += 1;
            progress.stage = "saving".into();
            on_progress(progress.clone());
            items.extend(parsed.items);
            for (reason, count) in parsed.rejected {
                let total = rejected.entry(reason).or_default();
                *total = total.saturating_add(count);
            }
        }
        analysis.sections.push(Section {
            id: step.id.clone(),
            name: step.name.clone(),
            kind: step.kind.clone(),
            items,
            rejected,
        });
    }
    control.check()?;
    validate_analysis(&analysis)?;
    create_export(
        &folder.join("result.txt"),
        render_text(&analysis).as_bytes(),
    )?;
    control.check()?;
    create_export(
        &folder.join("result.md"),
        crate::render_markdown(&analysis)?.as_bytes(),
    )?;
    control.check()?;
    // This durable result is the commit point. Completion wins a later cancel.
    create_json(&folder.join("result.json"), &analysis)?;
    progress.stage = "finished".into();
    on_progress(progress);
    Ok(analysis)
}
/// Groups whole source sections under the text budget without splitting a section's quoted evidence.
fn source_batches(sources: &[Source]) -> Vec<Vec<Source>> {
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut size = 0;
    for source in sources {
        if size + source.text.chars().count() > 6500 && !batch.is_empty() {
            batches.push(batch);
            batch = Vec::new();
            size = 0;
        }
        size += source.text.chars().count();
        batch.push(source.clone());
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}
/// Accepts identical saved export bytes or atomically creates a new file; different existing bytes are rejected.
fn create_export(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        ensure!(fs::read(path)?==bytes,"Ein gespeicherter Export weicht vom bestätigten Auswertungsstand ab; vorhandene Daten bleiben erhalten.");
        return Ok(());
    }
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("Auswertungsordner fehlt.")?)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|_| {
        anyhow::anyhow!(
            "Export konnte nicht atomar gesichert werden; vorhandene Fassungen bleiben erhalten."
        )
    })?;
    Ok(())
}
/// Renders saved sections and quoted evidence as plain text with draft and section-timing notices.
pub fn render_text(analysis: &Analysis) -> String {
    let mut text = format!("KI-Entwurf · {} · {}\nAbschnittszeiten sind keine Wortzeiten. Belege bestätigen Originalzitate, nicht automatisch die Deutung.\n", analysis.mode, analysis.model);
    for section in &analysis.sections {
        text.push_str(&format!("\n{}\n", section.name));
        if section.items.is_empty() {
            text.push_str("Keine ausdrücklich belegten Ergebnisse.\n");
        }
        for fact in &section.items {
            text.push_str(&format!("- {}", fact.text));
            if let Some(owner) = &fact.owner {
                text.push_str(&format!(" · {owner}"));
            }
            if let Some(due) = &fact.due {
                text.push_str(&format!(" · {due}"));
            }
            text.push('\n');
            for e in &fact.evidence {
                text.push_str(&format!("  [{}] {}\n", e.source_id, e.quote));
            }
        }
        if let Some(note) = rejected_note(section) {
            text.push_str(&format!("{note}\n"));
        }
    }
    text
}

/// Discarded statements are counted, never shown; this note keeps the gap visible in exports.
pub(crate) fn rejected_note(section: &Section) -> Option<String> {
    let total = section
        .rejected
        .values()
        .fold(0_usize, |sum, count| sum.saturating_add(*count));
    (total > 0).then(|| {
        format!("Hinweis: {total} KI-Aussage(n) wurden verworfen, weil sie die Beleg- oder Umfangsprüfung nicht bestanden haben.")
    })
}
