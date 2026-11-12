//! An explicit, synthetic setup check. It opens no capture device and saves no meeting.
use crate::{blocking, AppResult, Service};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroizing;

const PROBE_SOURCE: u32 = 1;
const CONFIRMED_DAY: &str = "The project meeting has been confirmed for Wednesday.";
const PROPOSED_DAY: &str = "Friday was only suggested.";

fn confirms_setup_day(quotes: &[meeting_notes::questions::Quote]) -> bool {
    !quotes.is_empty()
        && quotes
            .iter()
            .all(|quote| quote.source == PROBE_SOURCE && quote.quote.contains(CONFIRMED_DAY))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetupProbeRequest {
    model_id: String,
    text: Option<ProbeTextModel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeTextModel {
    endpoint: String,
    model: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    use_saved_key: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetupProbeResult {
    audio_roundtrip: bool,
    transcription_worker: bool,
    text_answer: Option<bool>,
}

#[tauri::command]
pub(crate) async fn check_meeting_setup(
    service: tauri::State<'_, Arc<Service>>,
    request: SetupProbeRequest,
) -> AppResult<SetupProbeResult> {
    let service = Arc::clone(service.inner());
    blocking(move || check(&service, request)).await
}

/// Reproduce the same probe from an exact executable with a new isolated profile.
pub(crate) fn check_cli(args: &[String]) -> AppResult<()> {
    let (root, model_id, text) =
        match args {
            [root, model_id] => (root, model_id, None),
            [root, model_id, endpoint, model] => (
                root,
                model_id,
                Some(ProbeTextModel {
                    endpoint: endpoint.clone(),
                    model: model.clone(),
                    api_key: String::new(),
                    use_saved_key: false,
                }),
            ),
            _ => return Err(
                "Use --setup-check-synthetic NEW_OUTPUT_DIR ASR_MODEL [LOCAL_ENDPOINT TEXT_MODEL]"
                    .into(),
            ),
        };
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir(&root).map_err(|_| "A new isolated setup-check directory is required.")?;
    let service = Service::new(root)?;
    let result = check(
        &service,
        SetupProbeRequest {
            model_id: model_id.clone(),
            text,
        },
    )?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|_| "Could not serialize setup-check result.")?
    );
    if !result.audio_roundtrip || !result.transcription_worker || result.text_answer == Some(false)
    {
        return Err("At least one synthetic setup stage did not pass.".into());
    }
    Ok(())
}

fn check(service: &Service, request: SetupProbeRequest) -> AppResult<SetupProbeResult> {
    let _probe = service.reserve_operation(crate::operations::Kind::Selection)?;
    let (bridge, catalog, _) = service.refresh(None)?;
    let model = catalog
        .model(&request.model_id)
        .map_err(|error| error.to_string())?;
    let resolved = if model.ready_for_batch() {
        bridge.resolve_downloaded_model(model)
    } else {
        bridge.resolve_streaming_model(model)
    }
    .map_err(|error| error.to_string())?;
    let backend = crate::backend_directory()?;
    let (audio_roundtrip, transcription_worker) =
        crate::asr::setup_signal_probe(&service.root, model, &resolved, &backend)?;
    let text_answer = request.text.map(|text| {
        crate::history_questions::local_client(
            service,
            text.endpoint,
            text.model,
            Zeroizing::new(text.api_key),
            text.use_saved_key,
        )
        .and_then(|client| {
            let passage = meeting_notes::questions::Passage {
                id: PROBE_SOURCE,
                meeting: "Synthetic setup".into(),
                text: format!("{CONFIRMED_DAY} {PROPOSED_DAY}"),
            };
            meeting_notes::questions::answer(
                &client,
                "Which day was confirmed for the project meeting?",
                &[passage],
            )
            .map(|quotes| confirms_setup_day(&quotes))
            .map_err(|_| "The local model did not pass the synthetic check.".into())
        })
        .unwrap_or(false)
    });
    Ok(SetupProbeResult {
        audio_roundtrip,
        transcription_worker,
        text_answer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use meeting_notes::questions::Quote;

    #[test]
    fn setup_requires_the_confirmed_day_but_accepts_its_literal_context() {
        let quote = |text: &str| Quote {
            source: PROBE_SOURCE,
            quote: text.into(),
        };
        assert!(confirms_setup_day(&[quote(CONFIRMED_DAY)]));
        assert!(confirms_setup_day(&[quote(&format!(
            "{CONFIRMED_DAY} {PROPOSED_DAY}"
        ))]));
        assert!(!confirms_setup_day(&[]));
        assert!(!confirms_setup_day(&[quote(PROPOSED_DAY)]));
        assert!(!confirms_setup_day(&[
            quote(CONFIRMED_DAY),
            quote(PROPOSED_DAY)
        ]));
        assert!(!confirms_setup_day(&[Quote {
            source: PROBE_SOURCE + 1,
            quote: CONFIRMED_DAY.into(),
        }]));
    }
}
