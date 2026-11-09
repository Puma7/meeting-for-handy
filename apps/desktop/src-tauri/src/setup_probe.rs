//! An explicit, synthetic setup check. It opens no capture device and saves no meeting.
use crate::{blocking, AppResult, Service};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroizing;

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
    recognition: bool,
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
    if !result.audio_roundtrip || !result.recognition || result.text_answer == Some(false) {
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
    let (audio_roundtrip, recognition) =
        crate::asr::setup_signal_probe(&service.root, model, &resolved, &backend)?;
    let text_answer = request.text.map(|text| {
            crate::history_questions::local_client(
                service, text.endpoint, text.model, Zeroizing::new(text.api_key), text.use_saved_key,
            ).and_then(|client| {
                let passage = meeting_notes::questions::Passage {
                    id: 1,
                    meeting: "Synthetic setup".into(),
                    text: "The project meeting has been confirmed for Wednesday. Friday was only suggested.".into(),
                };
                meeting_notes::questions::answer(&client, "Which day was confirmed for the project meeting?", &[passage])
                    .map(|quotes| quotes.iter().any(|quote| quote.quote.contains("confirmed for Wednesday"))
                        && !quotes.iter().any(|quote| quote.quote.contains("Friday")))
                    .map_err(|_| "The local model did not pass the synthetic check.".into())
            }).unwrap_or(false)
        });
    Ok(SetupProbeResult {
        audio_roundtrip,
        recognition,
        text_answer,
    })
}
