//! Synthetic service-state regressions; ordinary tests never open audio devices.
use super::*;
use std::time::Duration;

/// A parked read-only retention scan may coexist with ordinary native edits.
#[test]
fn retention_preview_allows_metadata_and_speaker_edits_during_the_scan() {
    let root = fixture_root("retention-preview-mutations");
    let service = Service::new(root.clone()).unwrap();
    let store =
        SessionStore::create(&root.join("sessions"), "Synthetic preview", "m", "s", None).unwrap();
    let id = store.snapshot().unwrap().id;
    store.set_status("complete").unwrap();
    drop(store);
    let _index = service.reserve_operation(operations::Kind::Index).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker_root = root.clone();
    let worker = std::thread::spawn(move || {
        let mut announced = false;
        retention_commands::publish_preview(
            &worker_root,
            &"e".repeat(32),
            &AtomicBool::new(false),
            |_, _| {
                if !announced {
                    announced = true;
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                }
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let metadata = library_commands::edit_metadata(
        &service,
        &id,
        0,
        Some("Synthetic renamed while scanning".into()),
        true,
    );
    let speakers = speaker_commands::save_labels(&service, &id, 0, false, vec![]);
    // Release the ordinary writer-capable view so it cannot mask a missing
    // publication fence through the independent session writer lock.
    service.lock().unwrap().view = None;
    let erasure_blocked =
        meeting_core::archive::preview_erasure(&root, &id, &AtomicBool::new(false)).is_err();
    release_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert!(
        metadata.is_ok(),
        "Read-only retention scan blocked metadata: {metadata:?}"
    );
    assert!(
        speakers.is_ok(),
        "Read-only retention scan blocked speaker labels: {speakers:?}"
    );
    assert!(
        erasure_blocked,
        "Retention publication allowed concurrent erasure"
    );
    assert_eq!(
        meeting_core::metadata::read(&root, &id)
            .unwrap()
            .title
            .as_deref(),
        Some("Synthetic renamed while scanning")
    );
}

/// A second Service's erasure lease must fence both the title snapshot and plan publication.
#[test]
fn retention_plan_publication_obeys_cross_service_erasure_exclusion() {
    let root = fixture_root("retention-publication-erasure");
    let service = Service::new(root.clone()).unwrap();
    let other = Service::new(root.clone()).unwrap();
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic retention title canary",
        "m",
        "s",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    store.set_status("complete").unwrap();
    drop(store);
    let manifest = root.join("sessions").join(&id).join("session.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["startedAt"] = "2020-01-01T00:00:00Z".into();
    value["endedAt"] = "2020-01-01T00:01:00Z".into();
    fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let policy = meeting_core::retention::Policy {
        archive_after_days: Some(365),
        ..Default::default()
    };
    meeting_core::retention::save_policy(&root, 0, policy).unwrap();
    let job = "d".repeat(32);
    let plan = root.join("retention-plans").join(format!("{job}.json"));
    let cancel = AtomicBool::new(false);
    let preview = meeting_core::archive::preview_erasure(&other.root, &id, &cancel).unwrap();
    let worker_root = other.root.clone();
    let worker_id = id.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let erasure = std::thread::spawn(move || {
        let stop = AtomicBool::new(false);
        meeting_core::archive::erase_meeting(
            &worker_root,
            &worker_id,
            &preview.fingerprint,
            &format!("DELETE {worker_id}"),
            &stop,
            |_, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                stop.store(true, Ordering::Release);
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let publication = retention_commands::publish_preview(&service.root, &job, &cancel, |_, _| {});
    release_tx.send(()).unwrap();
    assert!(
        erasure.join().unwrap().is_err(),
        "Synthetic erasure should stop before first removal"
    );
    assert!(
        publication.is_err(),
        "A content-bearing retention plan bypassed another Service's erasure exclusion"
    );
    assert!(!plan.exists());
    retention_commands::publish_preview(&service.root, &job, &cancel, |_, _| {}).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert!(value["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["meetingId"] == id
            && candidate["title"] == "Synthetic retention title canary"));
    let preview = meeting_core::archive::preview_erasure(&root, &id, &cancel).unwrap();
    meeting_core::archive::erase_meeting(
        &root,
        &id,
        &preview.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {},
    )
    .unwrap();
    assert!(
        !plan.exists(),
        "Completed erasure left the title-bearing retention preview"
    );
}

/// A maintenance snapshot delayed until after erasure must not reinsert its saved text.
#[test]
fn delayed_maintenance_snapshots_cannot_repopulate_an_erased_index() {
    for archived in [false, true] {
        let root = fixture_root("late-maintenance-index");
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic delayed maintenance",
            "m",
            "s",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[42; 1600]).unwrap();
        writer.finish().unwrap();
        drop(writer);
        let part = store.snapshot().unwrap().parts[0].clone();
        store
            .append_transcript(meeting_core::TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file,
                model_id: "m".into(),
                full: "syntheticlatemaintenancecanary".into(),
                committed: "syntheticlatemaintenancecanary".into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
        store.set_status("complete").unwrap();
        let folder = store.folder().to_owned();
        drop(store);
        let cancel = AtomicBool::new(false);
        let stale = if archived {
            meeting_core::archive::archive_audio(
                &root,
                &folder.file_name().unwrap().to_string_lossy(),
                0,
                &cancel,
                |_, _| {},
            )
            .unwrap();
            meeting_core::archive::read_archived_transcript(&folder)
                .unwrap()
                .unwrap()
        } else {
            SessionStore::open(&folder).unwrap().snapshot().unwrap()
        };
        // Both real maintenance source forms can be indexed before deletion.
        library_service::update(&root, &stale).unwrap();
        let preview = meeting_core::archive::preview_erasure(&root, &stale.id, &cancel).unwrap();
        meeting_core::archive::erase_meeting(
            &root,
            &stale.id,
            &preview.fingerprint,
            &format!("DELETE {}", stale.id),
            &cancel,
            |_, _| {},
        )
        .unwrap();
        assert!(
            library_service::update(&root, &stale).is_err(),
            "A delayed maintenance snapshot recreated erased transcript content"
        );
        let index = meeting_core::library::LibraryIndex::open(&root).unwrap();
        assert_eq!(
            index
                .search("syntheticlatemaintenancecanary", 0, 25)
                .unwrap()
                .total,
            0
        );
        assert!(!folder.exists());
    }
}

#[test]
fn recording_transitions_keep_saved_text_readable_without_maintenance_admission() {
    let root = fixture_root("read-during-capture-transition");
    let service = Service::new(root.clone()).unwrap();
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic readable during stop",
        "m",
        "s",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    service.lock().unwrap().view = Some(store);
    // Models the existing stop/reconnect transition; no device is opened here.
    service.lock().unwrap().transitioning = true;
    assert_eq!(
        service.data(&id).unwrap().title,
        "Synthetic readable during stop"
    );
    service.lock().unwrap().transitioning = false;
}

#[test]
fn annotation_commands_fence_the_source_snapshot_and_commit_with_one_os_lease() {
    for speaker in [false, true] {
        let root = fixture_root("annotation-erasure-fence");
        let service = Service::new(root.clone()).unwrap();
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic archived source",
            "m",
            "s",
            None,
        )
        .unwrap();
        let id = store.snapshot().unwrap().id;
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[42; 1600]).unwrap();
        writer.finish().unwrap();
        drop(writer);
        let part = store.snapshot().unwrap().parts[0].clone();
        store
            .append_transcript(meeting_core::TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file,
                model_id: "m".into(),
                full: "Synthetic source text".into(),
                committed: "Synthetic source text".into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
        store.set_status("complete").unwrap();
        drop(store);
        meeting_core::archive::archive_audio(&root, &id, 0, &AtomicBool::new(false), |_, _| {})
            .unwrap();
        let parent = root.join(if speaker {
            "speaker-labels"
        } else {
            "user-metadata"
        });
        fs::create_dir_all(&parent).unwrap();
        let remote = Service::new(root.clone()).unwrap();
        let source = root.join("sessions").join(&id).join("session.json");
        let before = fs::read(&source).unwrap();
        // Deterministic ordering: erasure already owns its OS lease before the
        // second Service attempts either mutation. No probe competes with a writer.
        let fence = meeting_core::archive::source_mutation_lease(&root).unwrap();
        let mutate = || {
            if speaker {
                speaker_commands::save_labels(&remote, &id, 0, false, vec![]).map(|_| ())
            } else {
                library_commands::edit_metadata(
                    &remote,
                    &id,
                    0,
                    Some("Synthetic edited title".into()),
                    false,
                )
                .map(|_| ())
            }
        };
        assert!(
            mutate().is_err(),
            "Mutation bypassed process-shared erasure admission"
        );
        assert_eq!(fs::read(&source).unwrap(), before);
        assert!(!parent.join(format!("{id}.json")).exists());
        assert!(!root.join("library-index").exists());
        drop(fence);
        mutate().unwrap();
        drop(remote);
        drop(service);
        let cancel = AtomicBool::new(false);
        let preview = meeting_core::archive::preview_erasure(&root, &id, &cancel).unwrap();
        meeting_core::archive::erase_meeting(
            &root,
            &id,
            &preview.fingerprint,
            &format!("DELETE {id}"),
            &cancel,
            |_, _| {},
        )
        .unwrap();
        assert!(!parent.join(format!("{id}.json")).exists());
        assert!(!root.join("sessions").join(&id).exists());
        assert!(!root.join("library-index").exists());
    }
}

#[test]
fn meeting_erasure_reservation_refuses_all_writers_and_releases_only_an_inactive_view() {
    let root = fixture_root("erasure-admission");
    let service = Arc::new(Service::new(root.clone()).unwrap());
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic retained original",
        "m",
        "s",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    let path = store.folder().join("session.json");
    let before = fs::read(&path).unwrap();
    service.lock().unwrap().view = Some(store);
    for kind in [
        operations::Kind::Recording,
        operations::Kind::Analysis,
        operations::Kind::Speakers,
        operations::Kind::Index,
        operations::Kind::Export,
        operations::Kind::Mutation,
        operations::Kind::LiveNotes,
    ] {
        let writer = service.reserve_operation(kind).unwrap();
        assert!(retention_commands::reserve(&service).is_err());
        assert!(service.lock().unwrap().view.is_some());
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(writer);
    }
    let maintenance = retention_commands::reserve(&service).unwrap();
    assert!(service.lock().unwrap().view.is_none());
    assert!(service.data(&id).is_err());
    assert!(service
        .reserve_operation(operations::Kind::Recording)
        .is_err());
    assert!(meeting_core::archive::preview_erasure(&root, &id, &AtomicBool::new(false)).is_ok());
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "A preview must never erase original content"
    );
    drop(maintenance);
    assert!(service.data(&id).is_ok());
}

#[test]
fn audio_level_poll_is_idle_without_capture_and_never_waits_for_app_state() {
    let root = fixture_root("idle-meter");
    let service = Service::new(root.clone()).unwrap();
    assert!(service.audio_levels().unwrap().is_none());
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic meter view",
        "m",
        "s",
        None,
    )
    .unwrap();
    service.lock().unwrap().view = Some(store);
    // Browsing a saved session must not produce levels or start an idle device.
    assert!(service.audio_levels().unwrap().is_none());
    let held = service.lock().unwrap();
    assert!(service.audio_levels().is_err());
    drop(held);
    assert!(service.audio_levels().unwrap().is_none());
}

/// Verify exclusive reservations block label, policy and import writes before worker startup;
/// synthetic files stay unchanged until admission is released.
#[test]
fn exclusive_reservation_blocks_annotation_policy_and_import_writes_before_worker_start() {
    for exclusive in [
        operations::Kind::Maintenance,
        operations::Kind::Installation,
        operations::Kind::Selection,
    ] {
        let root = fixture_root("exclusive-mutation");
        let service = Service::new(root.clone()).unwrap();
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic protected mutation",
            "m",
            "s",
            None,
        )
        .unwrap();
        let id = store.snapshot().unwrap().id;
        store.set_status("complete").unwrap();
        drop(store);
        let legacy = root.join("legacy-fixture");
        fs::create_dir(&legacy).unwrap();
        let manifest = legacy.join("session.json");
        let original=serde_json::to_vec(&serde_json::json!({"SchemaVersion":1,"Id":"0123456789abcdef0123456789abcdef","Title":"Synthetic old session","StartedAt":"2026-10-27T00:00:00Z","EndedAt":"2026-10-27T00:00:01Z","Status":"complete","ModelId":"m","Parts":[]})).unwrap();
        fs::write(&manifest, &original).unwrap();
        service
            .selections
            .approve_generated_legacy_fixture(&manifest)
            .unwrap();
        // Model the actual gap after a reservation, before start_work sets job progress.
        let reservation = service.reserve_operation(exclusive).unwrap();
        assert!(
            library_commands::edit_metadata(&service, &id, 0, Some("Must not save".into()), true)
                .is_err(),
            "metadata admitted during {exclusive:?}"
        );
        assert!(
            speaker_commands::save_labels(&service, &id, 0, false, vec![]).is_err(),
            "labels admitted during {exclusive:?}"
        );
        assert!(
            retention_commands::save_policy(&service, 0, Default::default()).is_err(),
            "policy admitted during {exclusive:?}"
        );
        assert!(
            service
                .import(manifest.to_string_lossy().into_owned())
                .is_err(),
            "import admitted during {exclusive:?}"
        );
        assert!(!root.join("user-metadata").exists());
        assert!(!root.join("speaker-labels").exists());
        assert!(!root.join("retention-settings.json").exists());
        assert_eq!(fs::read_dir(root.join("legacy-map")).unwrap().count(), 0);
        assert_eq!(fs::read(&manifest).unwrap(), original);
        drop(reservation);
        assert_eq!(
            library_commands::edit_metadata(&service, &id, 0, Some("Allowed later".into()), true)
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            speaker_commands::save_labels(&service, &id, 0, false, vec![])
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            retention_commands::save_policy(&service, 0, Default::default())
                .unwrap()
                .revision,
            1
        );
        assert!(
            service
                .import(manifest.to_string_lossy().into_owned())
                .unwrap()
                .data
                .read_only
        );
        assert_eq!(fs::read_dir(root.join("legacy-map")).unwrap().count(), 1);
        assert_eq!(fs::read(&manifest).unwrap(), original);
    }
}

/// Verify separate synthetic user labels can change during recording admission without changing
/// the capture manifest.
#[test]
fn metadata_rename_remains_available_while_the_recording_operation_is_owned() {
    let root = fixture_root("live-mutation");
    let service = Service::new(root.clone()).unwrap();
    let store = SessionStore::create(
        &root.join("sessions"),
        "Original capture title",
        "m",
        "s",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    let before = fs::read(store.folder().join("session.json")).unwrap();
    let lease = service
        .reserve_operation(operations::Kind::Recording)
        .unwrap();
    let (finish, wait) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = wait.recv();
    });
    service.lock().unwrap().active = Some(Active {
        _operation: Some(lease),
        owner: None,
        store: store.clone(),
        capture: None,
        stop: Arc::new(AtomicBool::new(false)),
        workers: vec![crate::asr::RecognitionWorker {
            source: "system".into(),
            handle: worker,
        }],
    });
    let saved = library_commands::edit_metadata(
        &service,
        &id,
        0,
        Some("Renamed while running".into()),
        false,
    )
    .unwrap();
    assert_eq!(saved.title.as_deref(), Some("Renamed while running"));
    assert_eq!(
        fs::read(store.folder().join("session.json")).unwrap(),
        before,
        "The original recording manifest must remain unchanged"
    );
    assert!(service
        .reserve_operation(operations::Kind::Maintenance)
        .is_err());
    finish.send(()).unwrap();
}

/// Verify rejected recording admission does not leave the service in a transitional state.
#[test]
fn a_busy_operation_cannot_leave_recording_transition_stuck() {
    let root = fixture_root("operation-admission");
    let service = Service::new(root).unwrap();
    let lease = service
        .reserve_operation(operations::Kind::Maintenance)
        .unwrap();
    let result = service.start(StartRequest {
        title: "Synthetic blocked start".into(),
        model_id: "never-loaded".into(),
        system_device_id: "never-opened".into(),
        microphone_device_id: None,
    });
    assert!(result.is_err());
    assert!(!service.lock().unwrap().transitioning);
    assert!(service.lock().unwrap().active.is_none());
    drop(lease);
    assert!(service
        .reserve_operation(operations::Kind::Recording)
        .is_ok());
}

/// Verify stale control, ownership and unapproved import requests fail using synthetic state
/// without device access.
#[test]
fn active_or_stale_control_and_unapproved_import_requests_never_touch_devices() {
    let root = fixture_root("capture-control-identity");
    let service = Service::new(root.clone()).unwrap();
    assert!(service
        .set_capture_control("stale", Some(false), None)
        .is_err());
    assert!(service.reconnect_microphone("stale").is_err());
    let path = root.join("session.json");
    fs::write(&path, b"synthetic manifest; deliberately never parsed").unwrap();
    let error = service
        .import(path.to_string_lossy().into_owned())
        .err()
        .unwrap();
    assert!(error.contains("Systemdialog"));
    assert_eq!(
        fs::read(&path).unwrap(),
        b"synthetic manifest; deliberately never parsed"
    );
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic agent session",
        "m",
        "s",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    service.lock().unwrap().active = Some(Active {
        _operation: None,
        owner: Some("synthetic-owner".into()),
        store,
        capture: None,
        stop: Arc::new(AtomicBool::new(false)),
        workers: vec![],
    });
    assert!(service
        .set_capture_control("other-meeting", Some(false), None)
        .is_err());
    assert!(service.reconnect_microphone("other-meeting").is_err());
    assert!(service
        .reconnect_microphone(&id)
        .err()
        .unwrap()
        .contains("Agenten"));
    let error = service
        .set_capture_control(&id, None, Some(false))
        .err()
        .unwrap();
    assert!(error.contains("Agenten"));
}

/// Create a uniquely named synthetic service root without capture or model loading.
fn fixture_root(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "handymeeting-{label}-{}-{nonce}",
        std::process::id()
    ))
}

/// Verify explicit executable-name screening rejects unrelated programs and accepts supported
/// Handy names.
#[test]
fn only_handy_named_programs_are_accepted_as_explicit_executables() {
    for ok in [
        "/usr/bin/handy",
        "C:/Program Files/Handy/Handy.exe",
        "/Applications/Handy.app",
        "/home/u/Applications/Handy_0.9.8_amd64.AppImage",
    ] {
        assert!(plausible_handy_path(Path::new(ok)), "{ok}");
    }
    for bad in [
        "/bin/sh",
        "C:/Windows/System32/cmd.exe",
        "/tmp/handy.sh",
        "/tmp/payload",
    ] {
        assert!(!plausible_handy_path(Path::new(bad)), "{bad}");
    }
}
/// Verify a failed explicit installation retains its origin instead of restoring an older cached
/// executable.
#[test]
fn invalid_explicit_installation_never_reverts_to_an_earlier_executable() {
    let root = fixture_root("installation-choice");
    let service = Service::new(root.clone()).unwrap();
    service.lock().unwrap().executable = Some(root.join("previous-handy.exe"));
    let selected = root.join("new-installation-missing.exe");
    assert!(service
        .refresh(Some(selected.to_string_lossy().into_owned()))
        .is_err());
    assert_eq!(
        service.lock().unwrap().executable.as_deref(),
        Some(selected.as_path())
    );
    assert!(service.refresh(None).is_err());
    assert_eq!(
        service.lock().unwrap().executable.as_deref(),
        Some(selected.as_path())
    );
    assert!(service
        .lock()
        .unwrap()
        .handy
        .as_ref()
        .unwrap()
        .models
        .is_empty());
}

/// Verify a synthetic manifest with a different folder identity is refused and preserved
/// byte-for-byte.
#[test]
fn mismatching_session_folder_identity_is_rejected_without_rewriting_it() {
    let root = fixture_root("session-identity");
    let service = Service::new(root).unwrap();
    let store = SessionStore::create(
        &service.root.join("sessions"),
        "Synthetic identity fixture",
        "fixture-model",
        "fixture-output",
        None,
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    let manifest = store.folder().join("session.json");
    drop(store);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["id"] = serde_json::json!("ffffffffffffffffffffffffffffffff");
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(&manifest, &bytes).unwrap();
    assert!(service.data(&id).is_err());
    assert_eq!(fs::read(&manifest).unwrap(), bytes);
}

/// Verify stop returns before a parked synthetic recognizer and later polling collects its
/// completion.
#[test]
fn stop_returns_while_recognition_catches_up_and_a_poll_collects_it_later() {
    let root = fixture_root("stop-catch-up");
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic slow backlog",
        "fixture-model",
        "fixture-output",
        None,
    )
    .unwrap();
    let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
    writer.append(&[1_i16, 2, 3, 4]).unwrap();
    writer.finish().unwrap();
    drop(writer);
    store.set_status("recorded").unwrap();
    let service = Service::new(root.clone()).unwrap();
    // A recognition worker that is still busy with the backlog when the user stops.
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let _ = wait.recv();
    });
    service.lock().unwrap().active = Some(Active {
        _operation: None,
        owner: None,
        store: store.clone(),
        capture: None,
        stop: Arc::new(AtomicBool::new(false)),
        workers: vec![crate::asr::RecognitionWorker {
            source: "system".into(),
            handle: worker,
        }],
    });
    let started = std::time::Instant::now();
    let view = service.stop().unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(view.data.status, "catching-up");
    assert!(service.audio_levels().unwrap().is_none());
    {
        let inner = service.lock().unwrap();
        assert!(!inner.transitioning, "stop must not hold the transition");
        assert!(inner.active.as_ref().is_some_and(|a| a.capture.is_none()));
    }
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while service.lock().unwrap().active.is_some() {
        assert!(std::time::Instant::now() < deadline);
        service.reap_finished().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Synthetic: the worker ended without recognising the open part, so the catch-up
    // must give way to a resumable state instead of blocking new recordings.
    let data = store.snapshot().unwrap();
    assert_eq!(data.status, "recorded");
    assert!(data
        .warnings
        .iter()
        .any(|w| w.contains("resume to process them")));
}

/// Verify a synthetic catch-up worker failure preserves saved session data for later recovery.
#[test]
fn a_failed_catch_up_worker_leaves_a_resumable_session() {
    let root = fixture_root("catch-up-panic");
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic failed catch-up",
        "fixture-model",
        "fixture-output",
        None,
    )
    .unwrap();
    let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
    writer.append(&[1_i16, 2, 3, 4]).unwrap();
    writer.finish().unwrap();
    drop(writer);
    store.set_status("catching-up").unwrap();
    let service = Service::new(root.clone()).unwrap();
    // Synthetic: a recognition worker that panics after the user stopped.
    let worker = std::thread::spawn(|| panic!("synthetic recognition failure"));
    while !worker.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    service.lock().unwrap().active = Some(Active {
        _operation: None,
        owner: None,
        store: store.clone(),
        capture: None,
        stop: Arc::new(AtomicBool::new(true)),
        workers: vec![crate::asr::RecognitionWorker {
            source: "system".into(),
            handle: worker,
        }],
    });
    service.reap_finished().unwrap();
    assert!(service.lock().unwrap().active.is_none());
    let data = store.snapshot().unwrap();
    assert_eq!(data.status, "recorded");
    assert!(data.warnings.iter().any(|w| w.contains("unterbrochen")));
    // Audio is untouched.
    assert_eq!(data.parts[0].confirmed_sample_count, 4);
}

/// Verify failed catch-up persistence remains retryable and derived-index failure is reported
/// separately.
#[test]
fn a_failed_catch_up_status_write_is_retried_and_an_index_failure_is_reported() {
    let root = fixture_root("catch-up-retry");
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic blocked status write",
        "fixture-model",
        "fixture-output",
        None,
    )
    .unwrap();
    let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
    writer.append(&[1_i16, 2, 3, 4]).unwrap();
    writer.finish().unwrap();
    drop(writer);
    store.set_status("catching-up").unwrap();
    let service = Service::new(root.clone()).unwrap();
    let worker = std::thread::spawn(|| {});
    while !worker.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    service.lock().unwrap().active = Some(Active {
        _operation: None,
        owner: None,
        store: store.clone(),
        capture: None,
        stop: Arc::new(AtomicBool::new(true)),
        workers: vec![crate::asr::RecognitionWorker {
            source: "system".into(),
            handle: worker,
        }],
    });
    // Polling leaves the session alone while a transition holds the gate.
    {
        let _gate = service.transition.lock().unwrap();
        service.reap_when_idle().unwrap();
        assert!(service.lock().unwrap().active.is_some());
    }
    // Synthetic: a directory in place of session.json makes the status write fail.
    let manifest = store.folder().join("session.json");
    let saved = std::fs::read(&manifest).unwrap();
    std::fs::remove_file(&manifest).unwrap();
    std::fs::create_dir(&manifest).unwrap();
    assert!(service.reap_when_idle().is_err());
    assert!(service.lock().unwrap().active.is_some());
    assert_eq!(store.snapshot().unwrap().status, "catching-up");
    std::fs::remove_dir(&manifest).unwrap();
    std::fs::write(&manifest, saved).unwrap();
    // Synthetic: a file in place of the index folder makes the search refresh fail.
    let index = root.join("library-index");
    assert!(!index.exists());
    std::fs::write(&index, b"synthetic, blocks the search index").unwrap();
    service.reap_when_idle().unwrap();
    assert!(service.lock().unwrap().active.is_none());
    let data = store.snapshot().unwrap();
    assert_eq!(data.status, "recorded");
    assert_eq!(data.parts[0].confirmed_sample_count, 4);
    assert!(data.warnings.iter().any(|w| w.contains("Suchindex")));
}

/// Verify restart recovery settles already-confirmed final text from synthetic journals without
/// loading a model.
#[test]
fn resume_finalizes_durable_final_text_after_host_exit_without_loading_a_model() {
    for global_status in ["recorded", "catching-up"] {
        let root = fixture_root("resume-final-checkpoint");
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic host-exit window",
            "fixture-model",
            "fixture-output",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[1_i16, 2, 3, 4]).unwrap();
        writer.finish().unwrap();
        drop(writer);
        store.set_status("recorded").unwrap();
        let data = store.snapshot().unwrap();
        let id = data.id.clone();
        let part = &data.parts[0];
        store
            .append_transcript(meeting_core::TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file.clone(),
                model_id: data.model_id.clone(),
                full: "Bestätigter Originaltext.".into(),
                committed: "Bestätigter Originaltext.".into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
        if global_status == "catching-up" {
            store.set_status("catching-up").unwrap();
        }
        let folder = store.folder().to_owned();
        let audio = fs::read(folder.join("audio").join(&part.file)).unwrap();
        let journal = fs::read(folder.join("transcript-journal.jsonl")).unwrap();
        drop(store);
        // Reopen the exact durable state a killed host leaves between its
        // final checkpoint and global completion write. No capture occurs.
        let service = Service::new(root.clone()).unwrap();
        service.lock().unwrap().executable = Some(root.join("intentionally-missing-handy.exe"));
        let resumed = service.resume(id.clone()).unwrap();
        assert_eq!(resumed.data.status, "complete");
        assert_eq!(
            resumed.data.parts[0].text.as_deref(),
            Some("Bestätigter Originaltext.")
        );
        assert!(service.lock().unwrap().handy.is_none());
        assert!(service.lock().unwrap().active.is_none());
        assert_eq!(
            fs::read(folder.join("audio").join(&part.file)).unwrap(),
            audio
        );
        assert_eq!(
            fs::read(folder.join("transcript-journal.jsonl")).unwrap(),
            journal
        );
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("session.json")).unwrap()).unwrap();
        assert_eq!(persisted["status"], "complete");
    }
}

/// Verify synthetic failed or open audio sections cannot be falsely marked complete on resume.
#[test]
fn resume_does_not_finalize_failed_or_open_audio_parts() {
    for (part_status, open) in [("error", false), ("complete", true)] {
        let root = fixture_root("resume-incomplete");
        let store = SessionStore::create(
            &root.join("sessions"),
            "Synthetic incomplete state",
            "fixture-model",
            "fixture-output",
            None,
        )
        .unwrap();
        let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
        writer.append(&[1_i16, 2, 3, 4]).unwrap();
        if open {
            writer.checkpoint().unwrap();
        } else {
            writer.finish().unwrap();
        }
        drop(writer);
        store.set_status("recorded").unwrap();
        let id = store.snapshot().unwrap().id;
        let manifest = store.folder().join("session.json");
        drop(store);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        value["parts"][0]["status"] = part_status.into();
        value["parts"][0]["modelId"] = "fixture-model".into();
        value["parts"][0]["processedSamples"] = 4.into();
        fs::write(&manifest, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let service = Service::new(root.clone()).unwrap();
        service.lock().unwrap().executable = Some(root.join("intentionally-missing-handy.exe"));
        assert!(service.resume(id.clone()).is_err());
        assert_eq!(service.data(&id).unwrap().status, "recorded");
    }
}

#[test]
fn stopping_ends_a_running_microphone_reconnect_before_closing_the_recording() {
    // Synthetic: the canceller stands in for a reconnect that waits on a slow device.
    let root = fixture_root("reconnect-cancel");
    let service = Arc::new(Service::new(root).unwrap());
    let cancel = Arc::new(AtomicBool::new(false));
    service.lock().unwrap().reconnect_cancel = Some(Arc::clone(&cancel));
    let reconnect = {
        let service = Arc::clone(&service);
        let cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            while !cancel.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            service.lock().unwrap().reconnect_cancel = None;
        })
    };
    assert!(service.cancel_microphone_reconnect().unwrap());
    reconnect.join().unwrap();
    assert!(!service.cancel_microphone_reconnect().unwrap());
    assert!(cancel.load(Ordering::Acquire));
    assert!(service.lock().unwrap().reconnect_cancel.is_none());
}

#[test]
fn a_stop_during_a_finishing_reconnect_waits_and_then_closes_the_recording() {
    // Synthetic: a reconnect thread clears its canceller and only then releases the
    // transition, the harder order for the stop. No audio device is used.
    let root = fixture_root("reconnect-handover");
    let store = SessionStore::create(
        &root.join("sessions"),
        "Synthetic reconnect handover",
        "fixture-model",
        "fixture-output",
        None,
    )
    .unwrap();
    let mut writer = meeting_core::PcmWriter::new(store.clone(), "system").unwrap();
    writer.append(&[1_i16, 2, 3, 4]).unwrap();
    writer.finish().unwrap();
    drop(writer);
    let service = Arc::new(Service::new(root).unwrap());
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut inner = service.lock().unwrap();
        inner.active = Some(Active {
            _operation: None,
            owner: None,
            store: store.clone(),
            capture: None,
            stop: Arc::new(AtomicBool::new(false)),
            workers: vec![],
        });
        inner.reconnecting_microphone = true;
        inner.reconnect_cancel = Some(Arc::clone(&cancel));
    }
    let (ready, held) = std::sync::mpsc::channel::<()>();
    let reconnect = {
        let service = Arc::clone(&service);
        let cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let gate = service.try_transition().unwrap();
            ready.send(()).unwrap();
            while !cancel.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            {
                let mut inner = service.lock().unwrap();
                inner.reconnecting_microphone = false;
                inner.reconnect_cancel = None;
            }
            // The stop request now sees no canceller while the transition is still held.
            std::thread::sleep(std::time::Duration::from_millis(150));
            drop(gate);
        })
    };
    held.recv().unwrap();
    let stopped = service.stop_for(None, None);
    reconnect.join().unwrap();
    assert!(stopped.is_ok(), "{:?}", stopped.err());
    assert!(cancel.load(Ordering::Acquire));
    assert!(service.lock().unwrap().active.is_none());
    assert_eq!(store.snapshot().unwrap().parts[0].confirmed_sample_count, 4);
}
