//! Local file actions only. No model transport, shell strings or overwrite path.
use crate::{validate_id, AppResult};
use meeting_core::SessionData;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn directory(path: &Path) -> AppResult<PathBuf> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| "Sitzungsordner nicht gefunden. / Session folder not found.")?;
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = meta.file_type().is_symlink();
    if linked || !meta.is_dir() {
        return Err("Verknüpfte Sitzungsordner werden nicht geöffnet oder beschrieben. / Linked session folders are not supported.".into());
    }
    path.canonicalize()
        .map_err(|_| "Sitzungsordner ist nicht erreichbar. / Session folder is unavailable.".into())
}

pub(super) fn session_folder(root: &Path, id: &str) -> AppResult<PathBuf> {
    validate_id(id)?;
    let sessions = directory(&root.join("sessions"))?;
    let folder = directory(&sessions.join(id))?;
    if folder.parent() != Some(sessions.as_path()) {
        return Err(
            "Sitzungsordner liegt außerhalb der Ablage. / Session folder escapes storage.".into(),
        );
    }
    Ok(folder)
}

fn export_directory(folder: &Path) -> AppResult<PathBuf> {
    let parent = directory(folder)?;
    let exports = parent.join("exports");
    match fs::create_dir(&exports) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => {
            return Err(
                "Exportordner konnte nicht angelegt werden. / Could not create export folder."
                    .into(),
            )
        }
    }
    let exports = directory(&exports)?;
    if exports.parent() != Some(parent.as_path()) {
        return Err(
            "Exportordner liegt außerhalb der Sitzung. / Export folder escapes session.".into(),
        );
    }
    Ok(exports)
}

/// Translates the capture's driver-interruption notices, in every stored wording, into
/// the same plain text the app shows; any other warning stays as recorded.
fn warning(value: &str, de: bool) -> String {
    const ENDINGS: [&str; 2] = [
        "; affected recording ranges can be incomplete.",
        ". Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.",
    ];
    for (source, label_de, label_en) in [
        ("system", "Computerton", "System audio"),
        ("microphone", "Mikrofon", "Microphone"),
    ] {
        let Some(rest) =
            value.strip_prefix(&format!("{source}: audio driver reported a discontinuity"))
        else {
            continue;
        };
        let Some(ending) = ENDINGS.iter().find(|ending| rest.ends_with(*ending)) else {
            continue;
        };
        let middle = &rest[..rest.len() - ending.len()];
        let at = if middle.is_empty() {
            None
        } else {
            match middle
                .strip_prefix(" at ")
                .and_then(|time| time.strip_suffix(" of the saved track"))
            {
                Some(time) if valid_clock(time) => Some(time),
                _ => continue,
            }
        };
        return match (de, at) {
            (true, Some(at)) => format!("{label_de}: Der Audiotreiber hat bei {at} eine Unterbrechung gemeldet. Ab dort können Teile dieser Spur fehlen."),
            (true, None) => format!("{label_de}: Der Audiotreiber hat eine Unterbrechung gemeldet. Teile dieser Spur können fehlen."),
            (false, Some(at)) => format!("{label_en}: The audio driver reported an interruption at {at}. Parts of this track may be missing from there on."),
            (false, None) => format!("{label_en}: The audio driver reported an interruption. Parts of this track may be missing."),
        };
    }
    value.into()
}

/// `mm:ss` or `h:mm:ss`, as written by the capture.
fn valid_clock(time: &str) -> bool {
    let fields: Vec<&str> = time.split(':').collect();
    let digits = |field: &str, len: Option<usize>| {
        !field.is_empty()
            && field.bytes().all(|b| b.is_ascii_digit())
            && len.is_none_or(|len| field.len() == len)
    };
    match fields.as_slice() {
        [m, s] => digits(m, Some(2)) && digits(s, Some(2)),
        [h, m, s] => digits(h, None) && digits(m, Some(2)) && digits(s, Some(2)),
        _ => false,
    }
}

fn timestamp(samples: u64) -> String {
    let s = samples / u64::from(meeting_core::SAMPLE_RATE);
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Render every section with its stable source ID, track, status and checked nominal sample range.
/// Availability notices distinguish absent/empty/whitespace text without inferring silence or speakers;
/// every stored Some(text) retains its original characters in this plain-text representation.
fn transcript(data: &SessionData, locale: &str) -> AppResult<String> {
    if !["de", "en"].contains(&locale) {
        return Err("Unsupported export language".into());
    }
    let de = locale == "de";
    let mut text = format!(
        "{}\n{} · {}\n{}\n",
        data.title,
        data.started_at,
        data.model_id,
        if de {
            "Nominale Abschnittszeiten, keine exakten Wortzeiten."
        } else {
            "Nominal section times, not exact word timestamps."
        }
    );
    if data.status != "complete" {
        text.push_str(if de {
            "UNVOLLSTÄNDIG: Es stehen Abschnitte aus.\n"
        } else {
            "INCOMPLETE: Some sections remain pending.\n"
        });
    }
    for value in &data.warnings {
        text.push_str(&warning(value, de));
        text.push('\n');
    }
    if data.capture_incomplete && data.warnings.is_empty() {
        text.push_str(if de {
            "Die Aufnahme kann Audiolücken enthalten.\n"
        } else {
            "This recording may contain audio gaps.\n"
        });
    }
    let mut parts: Vec<_> = data.parts.iter().collect();
    parts.sort_by_key(|p| (p.start_sample, &p.source, &p.file));
    for part in parts {
        let end = part
            .start_sample
            .checked_add(part.sample_count)
            .ok_or("Ungültiger Abschnittsbereich. / Invalid section range.")?;
        let label = match (part.source.as_str(), de) {
            ("system", true) => "Computerton",
            ("system", false) => "System audio",
            ("microphone", true) => "Mikrofon",
            ("microphone", false) => "Microphone",
            _ => &part.source,
        };
        text.push_str(&format!(
            "\n[{}–{}] {}\nsourceId: {} | track: {} | status: {}\n\n",
            timestamp(part.start_sample),
            timestamp(end),
            label,
            part.file,
            part.source,
            part.status
        ));
        match part.text.as_deref() {
            None => text.push_str(if de {
                "[Exporthinweis] Kein Transkripttext gespeichert."
            } else {
                "[Export note] No transcript text is stored."
            }),
            Some(value) => {
                if value.is_empty() {
                    text.push_str(if de {
                        "[Exporthinweis] Gespeicherter Transkripttext ist eine leere Zeichenfolge.\n"
                    } else {
                        "[Export note] Stored transcript text is an empty string.\n"
                    });
                } else if value.trim().is_empty() {
                    text.push_str(if de {
                        "[Exporthinweis] Gespeicherter Transkripttext enthält nur Leerraum; Originalzeichen folgen.\n"
                    } else {
                        "[Export note] Stored transcript text contains only whitespace; original characters follow.\n"
                    });
                }
                text.push_str(value);
            }
        }
        text.push('\n');
        if text.len() > 32 * 1024 * 1024 {
            return Err(
                "Transkriptexport überschreitet 32 MiB. / Transcript export exceeds 32 MiB.".into(),
            );
        }
    }
    Ok(text)
}

/// Save compatible TXT output through the same create-new export path used by other formats.
pub(super) fn save_transcript(
    folder: &Path,
    data: &SessionData,
    locale: &str,
) -> AppResult<String> {
    save_transcript_format(folder, data, locale, "txt")
}

// Quote every line as code: transcript text cannot introduce Markdown links,
// images, HTML, headings or a closing fence. The original characters are retained.
/// Return code-indented Markdown with presentation line endings normalized so transcript links,
/// HTML and fence text remain passive; JSON retains raw text.
fn markdown_literal(value: &str) -> String {
    // CommonMark accepts CR, LF and CRLF. Normalize this presentation copy
    // so a lone CR cannot escape the code indentation; JSON keeps raw text.
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(|line| format!("    {line}\n"))
        .collect()
}

/// Render bounded TXT, passive Markdown or JSON from the full snapshot. JSON preserves section
/// provenance and checked confirmed ranges independently of the text formats' nominal ranges.
fn render_export(
    data: &SessionData,
    locale: &str,
    format: &str,
) -> AppResult<(String, &'static str)> {
    if !["de", "en"].contains(&locale) {
        return Err("Unsupported export language".into());
    }
    match format {
        "txt" => Ok((transcript(data, locale)?, "txt")),
        "md" | "markdown" => Ok((
            format!(
                "# Meeting for Handy — {}\n\n{}",
                if locale == "de" {
                    "Originaltranskript"
                } else {
                    "Original transcript"
                },
                markdown_literal(&transcript(data, locale)?)
            ),
            "md",
        )),
        "json" => {
            let mut parts: Vec<_> = data.parts.iter().collect();
            parts.sort_by_key(|p| (p.start_sample, &p.source, &p.file));
            let sections: Vec<_> = parts.into_iter().map(|p| {
                let count = if p.confirmed_sample_count == 0 && data.read_only { p.sample_count } else { p.confirmed_sample_count };
                let end = p.start_sample.checked_add(count).ok_or("Invalid section range")?;
                Ok(serde_json::json!({
                    "sourceId": p.file, "track": p.source,
                    "startSample": p.start_sample, "confirmedEndSample": end,
                    "startMs": p.start_sample.saturating_mul(1000) / u64::from(meeting_core::SAMPLE_RATE),
                    "confirmedEndMs": end.saturating_mul(1000) / u64::from(meeting_core::SAMPLE_RATE),
                    "status": p.status, "text": p.text, "committedText": p.committed_text,
                    "tentativeText": p.tentative_text, "modelId": p.model_id,
                    "transcriptSequence": p.transcript_sequence,
                    "untrustedText": true
                }))
            }).collect::<AppResult<_>>()?;
            let value = serde_json::json!({
                "schemaVersion": 1, "exportKind": "original-transcript",
                "application": "Meeting for Handy", "applicationVersion": crate::VERSION,
                "meetingId": data.id, "title": data.title,
                "startedAt": data.started_at, "endedAt": data.ended_at,
                "status": data.status, "modelId": data.model_id,
                "captureIncomplete": data.capture_incomplete,
                "warnings": data.warnings, "snapshotSequence": data.snapshot_sequence,
                "sampleRate": meeting_core::SAMPLE_RATE,
                "timeSemantics": "section-boundaries-not-word-timestamps",
                "speakerIdentity": "unknown-unless-separately-confirmed",
                "untrustedText": true, "sections": sections
            });
            let output = serde_json::to_string_pretty(&value)
                .map_err(|_| "Export could not be serialized")?;
            if output.len() > 32 * 1024 * 1024 {
                return Err(
                    "Transkriptexport überschreitet 32 MiB. / Transcript export exceeds 32 MiB."
                        .into(),
                );
            }
            Ok((output, "json"))
        }
        _ => Err("Unbekanntes Exportformat. / Unsupported export format.".into()),
    }
}

/// Validate the session folder and create a new transcript export with control-event evidence;
/// retain originals, earlier exports and pending bytes on publication failure.
pub(super) fn save_transcript_format(
    folder: &Path,
    data: &SessionData,
    locale: &str,
    format: &str,
) -> AppResult<String> {
    let folder = directory(folder)?;
    let controls = meeting_core::read_capture_control_events(&folder)
        .map_err(|_| "Aufnahmesteuerung konnte nicht geprüft werden; Originale bleiben erhalten. / Capture control evidence could not be verified; originals are retained.")?;
    let (mut text, extension) = render_export(data, locale, format)?;
    if let Some(controls) = controls {
        if extension == "json" {
            let mut value: serde_json::Value =
                serde_json::from_str(&text).map_err(|_| "Export serialization failed")?;
            value["captureControls"] =
                serde_json::to_value(controls).map_err(|_| "Capture control export failed")?;
            text =
                serde_json::to_string_pretty(&value).map_err(|_| "Export serialization failed")?;
        } else {
            let mut evidence = if locale == "de" {
                "\nAufnahmesteuerung (kein Gesprächsinhalt; ungefähre Abschnittszeiten):\n"
            } else {
                "\nCapture controls (not spoken content; approximate section times):\n"
            }
            .to_owned();
            for event in controls.events {
                evidence.push_str(&format!(
                    "{} | {} | paused={} | microphoneMuted={} | missingSamples={}\n",
                    timestamp(event.requested_sample),
                    event.kind,
                    event.paused,
                    event.microphone_muted,
                    event.missing_samples.unwrap_or(0)
                ));
            }
            if controls.incomplete_tail {
                evidence.push_str(if locale == "de" { "Unvollständiger letzter Steuerungseintrag; nur bestätigter Präfix ausgegeben.\n" } else { "Incomplete final control event; only the confirmed prefix is exported.\n" });
            }
            text.push_str(&if extension == "md" {
                markdown_literal(&evidence)
            } else {
                evidence
            });
        }
    }
    if text.len() > 32 * 1024 * 1024 {
        return Err(
            "Transkriptexport überschreitet 32 MiB. / Transcript export exceeds 32 MiB.".into(),
        );
    }
    save_export_bytes(&folder, "transcript", extension, text.as_bytes())
}

pub(crate) fn save_export_bytes(
    folder: &Path,
    prefix: &str,
    extension: &str,
    bytes: &[u8],
) -> AppResult<String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Der Export überschreitet 32 MiB. / Export exceeds 32 MiB.".into());
    }
    let exports = export_directory(folder)?;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock unavailable")?
        .as_millis();
    for _ in 0..32 {
        let name = format!(
            "{prefix}-{at}-{}.{extension}",
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        // A failed write remains explicitly pending; only synced complete text
        // gets the final .txt name. Linking refuses existing destination files.
        let path = exports.join(format!(".{name}.pending"));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("Textexport konnte nicht angelegt werden. Speicherplatz und Ordnerrechte prüfen. / Could not create transcript export; check free space and folder permissions.".into()),
        };
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| "Textexport nicht vollständig gespeichert. Vorhandene Originale bleiben erhalten. / Export was not fully saved; originals are retained.")?;
        drop(file);
        publish_export(&path, &exports.join(&name))?;
        // Only our own redundant temporary link is removed after publication.
        let _ = fs::remove_file(&path);
        return Ok(format!("exports/{name}"));
    }
    Err("Kein freier Exportname gefunden. Frühere Exporte bleiben erhalten. / Could not reserve a new export name.".into())
}

fn publish_export(pending: &Path, final_path: &Path) -> AppResult<()> {
    fs::hard_link(pending, final_path).map_err(|_| "Textexport konnte nicht abgeschlossen werden. Vorhandene Exporte wurden nicht ersetzt; der neue Schreibstand bleibt als .pending erhalten. / Could not finish export; existing files were not replaced and the new write remains .pending.".into())
}

fn folder_command(folder: &Path) -> Command {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let base = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut command = Command::new(PathBuf::from(base).join("explorer.exe"));
        command.creation_flags(0x08000000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    command.arg(file_manager_path(folder));
    command
}

// Windows canonicalization returns verbatim prefixes, which Explorer does not
// handle consistently. Keep the verified target, expose its ordinary path form.
fn file_manager_path(folder: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut components = folder.components();
        let mut path = match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
                Prefix::VerbatimUNC(server, share) => {
                    PathBuf::from("\\\\").join(server).join(share)
                }
                _ => return folder.to_owned(),
            },
            _ => return folder.to_owned(),
        };
        for component in components {
            if let Component::Normal(name) = component {
                path.push(name);
            }
        }
        path
    }
    #[cfg(not(windows))]
    folder.to_owned()
}

pub(super) fn open_folder(folder: &Path) -> AppResult<()> {
    let folder = directory(folder)?;
    let mut child = folder_command(&folder).spawn().map_err(|_| {
        "Dateimanager konnte nicht geöffnet werden. / Could not open the file manager.".to_string()
    })?;
    // Reap short-lived launchers without waiting for the user's file-manager window.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, meeting_core::SessionStore) {
        let root = std::env::temp_dir().join(format!(
            "hm-export-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = meeting_core::SessionStore::create(
            &root.join("sessions"),
            "<script>synthetic</script>",
            "synthetic",
            "system",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[0; 16000]).unwrap();
        writer.finish().unwrap();
        (root, store)
    }
    #[test]
    fn driver_interruption_notices_are_translated_in_every_stored_wording() {
        let tail = ". Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.";
        assert_eq!(
            warning(&format!("system: audio driver reported a discontinuity at 03:12 of the saved track{tail}"), true),
            "Computerton: Der Audiotreiber hat bei 03:12 eine Unterbrechung gemeldet. Ab dort können Teile dieser Spur fehlen."
        );
        assert_eq!(
            warning(&format!("microphone: audio driver reported a discontinuity at 1:02:05 of the saved track{tail}"), false),
            "Microphone: The audio driver reported an interruption at 1:02:05. Parts of this track may be missing from there on."
        );
        assert_eq!(
            warning(&format!("microphone: audio driver reported a discontinuity{tail}"), true),
            "Mikrofon: Der Audiotreiber hat eine Unterbrechung gemeldet. Teile dieser Spur können fehlen."
        );
        // Anything that does not have exactly the stored shape stays as recorded.
        for other in [
            format!(
                "system: audio driver reported a discontinuity at 3:12 of the saved track{tail}"
            ),
            format!("system: audio driver reported a discontinuity at soon{tail}"),
            "system: audio driver reported a discontinuity".to_string(),
        ] {
            assert_eq!(warning(&other, true), other);
        }
    }
    #[test]
    fn refuses_traversal_and_unknown_session_without_creating_output() {
        let (root, store) = fixture();
        for id in [
            "../outside",
            "",
            "C:\\Windows",
            "00000000000000000000000000000000",
        ] {
            assert!(session_folder(&root, id).is_err());
        }
        assert!(!store.folder().join("exports").exists());
    }
    #[test]
    fn exports_full_snapshot_both_tracks_and_preserves_originals_and_earlier_exports() {
        let (_, store) = fixture();
        let before = fs::read(store.folder().join("session.json")).unwrap();
        let mut data = store.snapshot().unwrap();
        data.parts[0].text = Some("Original one <script>alert(1)</script>".into());
        let mut second = data.parts[0].clone();
        second.source = "microphone".into();
        second.text = Some("Last section ä €".into());
        data.parts.push(second);
        for index in 0..500 {
            let mut part = data.parts[0].clone();
            part.start_sample = (index + 1) * 16000;
            part.text = Some(format!("Synthetic section {index}"));
            data.parts.push(part);
        }
        data.warnings.push("system: audio driver reported a discontinuity; affected recording ranges can be incomplete.".into());
        let first = save_transcript(store.folder(), &data, "de").unwrap();
        let bytes = fs::read(store.folder().join(&first)).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        for expected in [
            "Original one <script>alert(1)</script>",
            "Last section ä €",
            "Synthetic section 499",
            "UNVOLLSTÄNDIG",
            "Computerton: Der Audiotreiber",
            "Mikrofon",
        ] {
            assert!(text.contains(expected));
        }
        let second = save_transcript(store.folder(), &data, "en").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(store.folder().join(&first)).unwrap(), bytes);
        assert_eq!(
            fs::read(store.folder().join("session.json")).unwrap(),
            before
        );
        assert!(fs::read_to_string(store.folder().join(second))
            .unwrap()
            .contains("INCOMPLETE"));
    }
    /// Refuse unsupported languages for every format before creating or replacing export paths.
    #[test]
    fn invalid_language_or_export_file_never_overwrites_any_file() {
        let (_, store) = fixture();
        for format in ["txt", "md", "json"] {
            assert!(save_transcript_format(
                store.folder(),
                &store.snapshot().unwrap(),
                "xx",
                format
            )
            .is_err());
        }
        assert!(!store.folder().join("exports").exists());
        fs::write(store.folder().join("exports"), "keep").unwrap();
        assert!(save_transcript(store.folder(), &store.snapshot().unwrap(), "de").is_err());
        assert_eq!(
            fs::read_to_string(store.folder().join("exports")).unwrap(),
            "keep"
        );
    }
    /// Verify synthetic hostile transcript markup stays passive and all original sections survive
    /// both formats.
    #[test]
    fn json_and_markdown_exports_preserve_original_sections_and_never_activate_transcript_markup() {
        let (_, store) = fixture();
        let manifest = fs::read(store.folder().join("session.json")).unwrap();
        let mut data = store.snapshot().unwrap();
        let original = "<script>alert(1)</script>\n```\n![leak](https://example.invalid/private)\n# fake heading\n[run](javascript:alert(1))\r<img src=\"https://example.invalid/secret\">\r![second](https://example.invalid/secret)";
        data.parts[0].text = Some(original.into());
        data.parts[0].committed_text = original.into();
        data.parts[0].tentative_text = "Unconfirmed tail".into();
        data.parts[0].processed_samples = 16000;
        data.parts[0].transcript_sequence = 37;
        let mut pending = data.parts[0].clone();
        pending.file = "microphone-000000032000.wav".into();
        pending.source = "microphone".into();
        pending.start_sample = 32000;
        pending.text = None;
        pending.committed_text.clear();
        pending.tentative_text.clear();
        pending.transcript_sequence = 0;
        data.parts.push(pending);
        let json_path = save_transcript_format(store.folder(), &data, "en", "json").unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(store.folder().join(&json_path)).unwrap()).unwrap();
        assert_eq!(value["sections"].as_array().unwrap().len(), 2);
        assert_eq!(value["sections"][0]["sourceId"], data.parts[0].file);
        assert_eq!(value["sections"][0]["text"], original);
        assert_eq!(value["sections"][0]["tentativeText"], "Unconfirmed tail");
        assert_eq!(value["sections"][0]["transcriptSequence"], 37);
        assert_eq!(value["sections"][1]["startMs"], 2000);
        assert!(value["sections"][1]["text"].is_null());
        assert_eq!(
            value["timeSemantics"],
            "section-boundaries-not-word-timestamps"
        );
        assert!(value.get("audioPath").is_none());
        let md_path = save_transcript_format(store.folder(), &data, "de", "markdown").unwrap();
        let markdown = fs::read_to_string(store.folder().join(md_path)).unwrap();
        for line in original.replace('\r', "\n").lines() {
            assert!(markdown.contains(&format!("    {line}\n")));
        }
        assert!(!markdown.contains("\n<script>"));
        assert!(!markdown.contains("\n```"));
        assert!(!markdown.contains("\n!["));
        assert!(!markdown.contains('\r'));
        assert!(markdown.contains("\n    <img src=\"https://example.invalid/secret\">\n"));
        let json_before = fs::read(store.folder().join(&json_path)).unwrap();
        assert!(save_transcript_format(store.folder(), &data, "en", "../../exe").is_err());
        assert_eq!(
            fs::read(store.folder().join(json_path)).unwrap(),
            json_before
        );
        assert_eq!(
            fs::read(store.folder().join("session.json")).unwrap(),
            manifest
        );
    }
    #[test]
    fn folder_is_one_literal_argument_even_with_spaces_and_shell_text() {
        let path = Path::new("/synthetic/folder with space; $USER & whoami");
        let command = folder_command(path);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![path.as_os_str()]
        );
    }

    /// Snapshot every synthetic source file after its store has closed, before exporting.
    fn retained_source_files(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut folders = vec![folder.to_owned()];
        let mut files = Vec::new();
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    files.push((path.clone(), fs::read(&path).unwrap()));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    /// Keep every synthetic section visible in text formats, including absent, empty and whitespace text.
    /// JSON retains raw text states and original files remain unchanged across all exports.
    #[test]
    fn text_exports_keep_every_source_identity_status_and_original_text_state() {
        for locale in ["de", "en"] {
            let (_, store) = fixture();
            let folder = store.folder().to_owned();
            let mut data = store.snapshot().unwrap();
            let base = data.parts[0].clone();
            drop(store);
            let originals = retained_source_files(&folder);
            let supplied = [
                ("system", 0, None, "pending"),
                ("microphone", 0, Some(""), "pending"),
                ("system", 16000, Some(" \t "), "error"),
                ("microphone", 16000, Some("\t\n  \r\n"), "complete"),
                (
                    "system",
                    32000,
                    Some("  Original <script>literal</script>\t\n![passive](https://example.invalid/)  "),
                    "complete",
                ),
            ];
            data.parts = supplied
                .into_iter()
                .map(|(track, start, value, status)| {
                    let mut part = base.clone();
                    part.file = format!("{track}-{start:012}.wav");
                    part.source = track.into();
                    part.start_sample = start;
                    part.text = value.map(str::to_owned);
                    part.committed_text = value.unwrap_or_default().into();
                    part.tentative_text = "Separate unconfirmed candidate".into();
                    part.status = status.into();
                    part
                })
                .collect();
            let txt = save_transcript(&folder, &data, locale).unwrap();
            let md = save_transcript_format(&folder, &data, locale, "md").unwrap();
            let json = save_transcript_format(&folder, &data, locale, "json").unwrap();
            let txt = fs::read_to_string(folder.join(txt)).unwrap();
            let md = fs::read_to_string(folder.join(md)).unwrap();
            let json: serde_json::Value =
                serde_json::from_slice(&fs::read(folder.join(json)).unwrap()).unwrap();
            let sections = json["sections"].as_array().unwrap();
            assert_eq!(sections.len(), supplied.len());
            for part in &data.parts {
                let identity = format!(
                    "sourceId: {} | track: {} | status: {}",
                    part.file, part.source, part.status
                );
                assert_eq!(txt.matches(&identity).count(), 1, "{identity}");
                assert_eq!(
                    md.matches(&format!("    {identity}\n")).count(),
                    1,
                    "{identity}"
                );
                let section = sections
                    .iter()
                    .find(|item| item["sourceId"] == part.file)
                    .unwrap();
                assert_eq!(section["status"], part.status);
                assert_eq!(section["track"], part.source);
                assert_eq!(section["text"], serde_json::to_value(&part.text).unwrap());
                assert_eq!(section["committedText"], part.committed_text);
                assert_eq!(section["tentativeText"], part.tentative_text);
                if let Some(value) = &part.text {
                    assert!(txt.contains(&format!("\n{value}\n")));
                }
            }
            for notice in if locale == "de" {
                [
                    "[Exporthinweis] Kein Transkripttext gespeichert.",
                    "[Exporthinweis] Gespeicherter Transkripttext ist eine leere Zeichenfolge.",
                    "[Exporthinweis] Gespeicherter Transkripttext enthält nur Leerraum; Originalzeichen folgen.",
                ]
            } else {
                [
                    "[Export note] No transcript text is stored.",
                    "[Export note] Stored transcript text is an empty string.",
                    "[Export note] Stored transcript text contains only whitespace; original characters follow.",
                ]
            } {
                assert!(txt.contains(notice), "{notice}");
                assert!(md.contains(&format!("    {notice}\n")), "{notice}");
            }
            assert_eq!(txt.matches("[00:00:00–00:00:01]").count(), 2);
            assert_eq!(
                json["timeSemantics"],
                "section-boundaries-not-word-timestamps"
            );
            assert!(!md.contains("\n<script>"));
            assert!(!md.contains("\n!["));
            for (path, before) in originals {
                assert_eq!(fs::read(path).unwrap(), before);
            }
        }
    }

    /// Count whitespace-only section payloads toward the export bound before creating files.
    #[test]
    fn whitespace_sections_cannot_bypass_the_export_size_limit() {
        let (_, store) = fixture();
        let folder = store.folder().to_owned();
        let mut data = store.snapshot().unwrap();
        drop(store);
        let originals = retained_source_files(&folder);
        data.parts[0].text = Some(" ".repeat(32 * 1024 * 1024));
        assert!(save_transcript_format(&folder, &data, "en", "md").is_err());
        assert!(!folder.join("exports").exists());
        for (path, before) in originals {
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }

    /// Reject overflowing ranges even on formerly omitted sections before creating any export file.
    #[test]
    fn invalid_empty_section_ranges_return_errors_without_panics_or_partial_exports() {
        for value in [None, Some(""), Some(" \t "), Some("Original text")] {
            let (_, store) = fixture();
            let folder = store.folder().to_owned();
            let mut data = store.snapshot().unwrap();
            drop(store);
            let originals = retained_source_files(&folder);
            data.parts[0].start_sample = u64::MAX;
            data.parts[0].sample_count = 1;
            data.parts[0].confirmed_sample_count = 1;
            data.parts[0].text = value.map(str::to_owned);
            for format in ["txt", "md", "json"] {
                let result = std::panic::catch_unwind(|| {
                    save_transcript_format(&folder, &data, "en", format)
                });
                assert!(result.is_ok(), "{format} panicked for {value:?}");
                assert!(result.unwrap().is_err(), "{format} accepted {value:?}");
                assert!(!folder.join("exports").exists());
            }
            for (path, before) in originals {
                assert_eq!(fs::read(path).unwrap(), before);
            }
        }
    }

    /// Keep JSON's confirmed-range contract independent from text-only nominal-range validation.
    #[test]
    fn json_retains_null_text_with_a_valid_confirmed_but_overflowing_nominal_range() {
        let (_, store) = fixture();
        let folder = store.folder().to_owned();
        let mut data = store.snapshot().unwrap();
        drop(store);
        let originals = retained_source_files(&folder);
        data.parts[0].start_sample = u64::MAX - 1;
        data.parts[0].sample_count = 2;
        data.parts[0].confirmed_sample_count = 1;
        data.parts[0].text = None;
        for format in ["txt", "md"] {
            let result =
                std::panic::catch_unwind(|| save_transcript_format(&folder, &data, "en", format));
            assert!(result.is_ok(), "{format} panicked");
            assert!(result.unwrap().is_err());
            assert!(!folder.join("exports").exists());
        }
        let json = save_transcript_format(&folder, &data, "en", "json").unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join(json)).unwrap()).unwrap();
        assert_eq!(value["sections"][0]["startSample"], u64::MAX - 1);
        assert_eq!(value["sections"][0]["confirmedEndSample"], u64::MAX);
        assert!(value["sections"][0]["text"].is_null());
        assert_eq!(value["sections"][0]["sourceId"], data.parts[0].file);
        for (path, before) in originals {
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }
    #[cfg(windows)]
    #[test]
    fn explorer_receives_ordinary_drive_or_unc_paths_without_verbatim_prefixes() {
        assert_eq!(
            file_manager_path(Path::new("\\\\?\\C:\\Users\\Synthetic name\\session")),
            Path::new("C:\\Users\\Synthetic name\\session")
        );
        assert_eq!(
            file_manager_path(Path::new("\\\\?\\UNC\\server\\share\\Synthetic name")),
            Path::new("\\\\server\\share\\Synthetic name")
        );
    }
    #[test]
    fn publishing_never_replaces_existing_exports_and_retains_pending_bytes_on_failure() {
        let (_, store) = fixture();
        let folder = export_directory(store.folder()).unwrap();
        let final_path = folder.join("existing.txt");
        let pending = folder.join(".new.pending");
        fs::write(&final_path, "original export").unwrap();
        fs::write(&pending, "new complete bytes").unwrap();
        assert!(publish_export(&pending, &final_path).is_err());
        assert_eq!(fs::read_to_string(&final_path).unwrap(), "original export");
        assert_eq!(fs::read_to_string(&pending).unwrap(), "new complete bytes");
        let new_path = folder.join("new.txt");
        publish_export(&pending, &new_path).unwrap();
        assert_eq!(fs::read_to_string(new_path).unwrap(), "new complete bytes");
    }
    #[cfg(unix)]
    #[test]
    fn linked_export_directory_cannot_redirect_writes() {
        let (root, store) = fixture();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, store.folder().join("exports")).unwrap();
        assert!(save_transcript(store.folder(), &store.snapshot().unwrap(), "en").is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}
