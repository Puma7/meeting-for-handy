//! Explicit command-line diagnostics, isolated from renderer commands.
use super::*;

/// Dispatch explicit diagnostics or worker modes before GUI startup; return None for
/// desktop-handled arguments.
pub(crate) fn run_cli(args: &[String]) -> Option<AppResult<()>> {
    match args.first().map(String::as_str) {
        Some("--agent-protocol-check-stdio") => Some(if args.len() == 1 {
            agent::check_stdio()
        } else {
            Err("The synthetic protocol check accepts no arguments".into())
        }),
        Some("--credential-checks") => Some((|| {
            if args.len() != 2 {
                return Err("Exactly one new credential-check directory is required".into());
            }
            credential_service::native_checks(Path::new(&args[1]))
        })()),
        Some("--companion-capabilities") => Some((|| {
            let version = match &args[1..] {
                [] => 1,
                [flag, value] if flag == "--contract-version" => value
                    .parse::<u32>()
                    .map_err(|_| "Invalid contract version")?,
                _ => return Err("Use --companion-capabilities [--contract-version 1]".into()),
            };
            if version != 1 {
                return Err("Unsupported companion contract version".into());
            }
            let bridge = HandyBridge::discover(None).map_err(|error| error.to_string())?;
            let catalog = bridge.refresh().map_err(|error| error.to_string())?;
            let capabilities = handy_bridge::companion_capabilities(
                version,
                &catalog,
                handy_bridge::installed_version(bridge.executable()),
            )
            .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&capabilities).map_err(|_| "Capabilities unavailable")?
            );
            Ok(())
        })()),
        Some("--agent") if args == ["--agent", "serve"] => None,
        Some("--agent") => Some((|| {
            if args != ["--agent", "capabilities"] {
                return Err("Use --agent capabilities or --agent serve; recording requires visible in-app consent.".into());
            }
            println!("{}", agent::capabilities());
            Ok(())
        })()),
        Some("--asr-worker") => Some(asr::worker_main(&args[1..])),
        Some("--speaker-worker") => Some(speaker_jobs::worker_main(&args[1..])),
        Some("--synthetic-smoke") => Some(asr::synthetic_smoke(&args[1..])),
        Some("--setup-check-synthetic") => Some(setup_probe::check_cli(&args[1..])),
        Some("--list-text-models") => Some((|| {
            if args.len() != 2 {
                return Err("Genau eine lokale KI-Adresse wird benötigt.".into());
            }
            let models = meeting_notes::local_model_ids(&args[1]).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&models)
                    .map_err(|_| "Modellliste konnte nicht ausgegeben werden.")?
            );
            Ok(())
        })()),
        Some("--speaker-name-reference-check") => Some(name_reference::check(&args[1..])),
        Some("--model-probe") => Some((|| {
            if args.len() != 3 {
                return Err("Invalid model probe arguments".into());
            }
            let _ = handy_bridge::native::NativeEngine::load(
                Path::new(&args[1]),
                true,
                Some(Path::new(&args[2])),
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })()),
        Some("--integration-checks") => Some(integration_checks(args.get(1).map(String::as_str))),
        Some("--release-checks") => {
            Some(application_checks(args.get(1).map(String::as_str), false))
        }
        _ => None,
    }
}
/// Run isolated application checks plus read-only installed-model and audio-device queries.
fn integration_checks(output: Option<&str>) -> AppResult<()> {
    application_checks(output, true)
}

/// Release mode never discovers Handy, enumerates devices, loads weights or
/// creates a window. Installed-metadata mode adds read-only local queries.
fn application_checks(output: Option<&str>, include_installed: bool) -> AppResult<()> {
    let output = PathBuf::from(output.ok_or("Integration check output directory is required")?);
    if output.exists() {
        return Err("Integration check output already exists; preserve previous evidence.".into());
    }
    fs::create_dir_all(&output).map_err(|_| "Cannot create integration check directory")?;
    // Only our newly created diagnostic fixture gains an absolute path. Native/IPC
    // selections still require their own absolute, purpose-bound human approval.
    let output = output
        .canonicalize()
        .map_err(|_| "Cannot resolve the newly created integration check directory")?;
    let service = Service::new(output.join("application-data"))?;
    let installed = if include_installed {
        let (_, catalog, _) = service.refresh(None)?;
        let devices =
            meeting_core::list_devices().map_err(|_| "Read-only device enumeration failed")?;
        Some((
            catalog.models.len(),
            catalog.installed_streaming_models().count(),
            devices.len(),
        ))
    } else {
        None
    };
    let runtime = handy_bridge::native::runtime_identity();
    if runtime.compiled_version != "0.4.1"
        || runtime.loaded_version != "0.4.1"
        || runtime.public_header_hash.is_empty()
    {
        return Err("Packaged native runtime does not match the pinned 0.4.1 bindings".into());
    }
    let legacy = output.join("legacy-fixture");
    fs::create_dir_all(legacy.join("audio"))
        .map_err(|_| "Cannot create synthetic legacy fixture")?;
    let mut parts = vec![];
    for index in 0..25 {
        let start = index * 16_000;
        let file = format!("system-{start:012}.wav");
        fs::write(
            legacy.join("audio").join(&file),
            b"synthetic existing audio fixture",
        )
        .map_err(|_| "Cannot create fixture")?;
        parts.push(serde_json::json!({"File":file,"Source":"system","StartSample":start,"SampleCount":16000,"HasSignal":true,"AudioClosed":true,"Status":"complete","Text":format!("Originalabschnitt {index}: Dieser Text bleibt unverändert."),"ModelId":"synthetic-fixture","ProcessedSamples":16000}));
    }
    let manifest = legacy.join("session.json");
    create_json(
        &manifest,
        &serde_json::json!({"SchemaVersion":1,"Id":"0123456789abcdef0123456789abcdef","Title":"Synthetische Originalfassung","StartedAt":"2026-10-25T00:00:00+00:00","EndedAt":"2026-10-25T00:00:25+00:00","Status":"complete","ModelId":"synthetic-fixture","ChunkSeconds":30,"DurationMs":25000,"Parts":parts}),
    )?;
    let before = fs::read(&manifest).map_err(|_| "Cannot read fixture")?;
    service
        .selections
        .approve_generated_legacy_fixture(&manifest)?;
    let imported = service.import(manifest.to_string_lossy().into_owned())?;
    if !imported.data.read_only
        || !imported.data.id.starts_with("legacy-")
        || imported.data.parts.len() != 25
    {
        return Err("Legacy read-only import invariant failed".into());
    }
    let full = service.data(&imported.data.id)?;
    let preview = meeting_view(full.clone(), true);
    if preview.data.parts.len() != 16
        || !preview.live_preview_limited
        || preview.saved_audio_ms.system != 25000
    {
        return Err("Bounded preview or global saved-audio calculation failed".into());
    }
    if serde_json::to_string(&meeting_view(full.clone(), false))
        .map_err(|_| "Cannot serialize full copy")?
        .matches("Originalabschnitt")
        .count()
        != 25
    {
        return Err("Full transcript copy lost older sections".into());
    }
    let sources = note_sources(&full)?;
    if sources.len() != 25 || sources[0].id != "S00001" {
        return Err("Stable source mapping failed".into());
    }
    if fs::read(&manifest).map_err(|_| "Cannot reread fixture")? != before {
        return Err("Legacy original was modified".into());
    }
    for part in &full.parts {
        if fs::read(legacy.join("audio").join(&part.file))
            .map_err(|_| "Cannot reread synthetic audio fixture")?
            != b"synthetic existing audio fixture"
        {
            return Err("Legacy synthetic audio original was modified".into());
        }
    }
    if service.data("../../outside").is_ok() {
        return Err("Session traversal was accepted".into());
    }
    let reloaded = Service::new(service.root.clone())?;
    if reloaded.data(&imported.data.id)?.parts.len() != 25 {
        return Err("Explicit legacy mapping did not survive restart".into());
    }
    let _ = service.state()?;
    create_json(
        &output.join(if include_installed {
            "integration-checks.json"
        } else {
            "release-checks.json"
        }),
        &serde_json::json!({"success":true,"applicationVersion":VERSION,"checks":8,"syntheticChecks":7,"nativeRuntimeChecks":1,"checkNames":["native-runtime-abi","read-only-legacy-import","bounded-preview-global-progress","full-transcript-copy","stable-source-ids","original-manifest-and-audio-bytes","session-path-traversal","import-mapping-after-restart"],"installedMetadataQueried":include_installed,"installedHandyModelCount":installed.map(|s|s.0),"installedStreamingModels":installed.map(|s|s.1),"audioDeviceCount":installed.map(|s|s.2),"legacyOriginalUnchanged":true,"legacyMappingSurvivedRestart":true,"previewParts":16,"fullCopyParts":25,"stableSourceCount":sources.len(),"nativeRuntimeMatchesPinnedBindings":true,"nativeRuntime":runtime,"synthetic":true,"realCaptureOpened":false}),
    )
}
