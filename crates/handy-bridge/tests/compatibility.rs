use handy_bridge::{
    parse_model_list, BridgeError, CommandLimits, HandyBridge, HandyModel, ModelListRunner,
    ModelRoots,
};
use serde_json::json;
use std::cell::Cell;
use std::path::Path;
use tempfile::TempDir;

fn model() -> serde_json::Value {
    json!({"id":"upstream/new-family-v42","name":"Future streaming model","filename":"future.gguf","source":"Local","is_downloaded":true,"is_downloading":false,"supports_streaming":true,"supported_languages":["de","en"]})
}

fn parse(value: serde_json::Value) -> HandyModel {
    parse_model_list(&serde_json::to_vec(&vec![value]).unwrap())
        .unwrap()
        .remove(0)
}

#[test]
fn accepts_new_model_ids_and_additive_fields_without_a_catalog_update() {
    let mut future = model();
    future["new_capability"] = json!({"enabled":true});
    let parsed = parse(future);
    assert_eq!(parsed.id, "upstream/new-family-v42");
    assert!(parsed.ready_for_streaming());
}

#[test]
fn missing_or_wrong_streaming_capability_fails_closed() {
    for value in [None, Some(json!("yes")), Some(serde_json::Value::Null)] {
        let mut broken = model();
        if let Some(value) = value {
            broken["supports_streaming"] = value;
        } else {
            broken.as_object_mut().unwrap().remove("supports_streaming");
        }
        assert!(matches!(
            parse_model_list(&serde_json::to_vec(&[broken]).unwrap()),
            Err(BridgeError::UnsupportedSchema)
        ));
    }
}

#[test]
fn rejects_duplicate_ids_mixed_console_output_and_changed_envelope() {
    assert!(matches!(
        parse_model_list(&serde_json::to_vec(&[model(), model()]).unwrap()),
        Err(BridgeError::UnsupportedSchema)
    ));
    assert!(matches!(
        parse_model_list(b"diagnostic\n[]"),
        Err(BridgeError::UnsupportedSchema)
    ));
    assert!(matches!(
        parse_model_list(br#"{"models":[]}"#),
        Err(BridgeError::UnsupportedSchema)
    ));
}

#[test]
fn accepts_utf8_bom_but_never_infers_streaming_from_the_name() {
    let mut value = model();
    value["supports_streaming"] = json!(false);
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(serde_json::to_vec(&[value]).unwrap());
    let parsed = parse_model_list(&bytes).unwrap();
    assert!(!parsed[0].ready_for_streaming());
}

#[test]
fn unavailable_and_downloading_models_are_not_selectable() {
    for field in ["is_downloaded", "is_downloading"] {
        let mut value = model();
        value[field] = json!(field == "is_downloading");
        let parsed = parse(value);
        assert!(!parsed.ready_for_streaming());
        assert!(matches!(
            ModelRoots::default().resolve(&parsed),
            Err(BridgeError::ModelUnavailable)
        ));
    }
}

#[test]
fn batch_only_model_is_rejected_by_streaming_policy_before_any_path_resolution() {
    let mut value = model();
    value["id"] = json!(handy_bridge::PARAKEET_ULTRA_ID);
    value["supports_streaming"] = json!(false);
    // Deliberately unsafe/unresolvable input would yield another error if the
    // streaming gate were bypassed. Agent starts use this same resolver gate.
    value["filename"] = json!("../not-a-real-model");
    value["source"] = json!({});
    let parsed = parse(value);
    assert!(parsed.ready_for_batch());
    assert!(!parsed.ready_for_streaming());
    assert!(matches!(
        ModelRoots::default().resolve(&parsed),
        Err(BridgeError::ModelUnavailable)
    ));
}

#[test]
fn rejects_cross_platform_filename_traversal() {
    for filename in [
        "../secret.gguf",
        "..\\secret.gguf",
        "C:\\secret.gguf",
        "/secret.gguf",
        "file:stream",
        "..",
        "x.gguf.",
    ] {
        let mut value = model();
        value["filename"] = json!(filename);
        assert!(
            matches!(
                ModelRoots::default().resolve(&parse(value)),
                Err(BridgeError::UnsafePath)
            ),
            "{filename}"
        );
    }
}

#[test]
fn rejects_hf_repo_revision_and_ref_traversal() {
    let temp = TempDir::new().unwrap();
    let roots = ModelRoots {
        hf_caches: vec![temp.path().to_owned()],
        models_dirs: vec![],
    };
    for (repo, revision) in [
        ("../outside", "main"),
        ("a/../../b", "main"),
        ("a/b", "../outside"),
        ("a/b", "C:\\outside"),
    ] {
        let mut value = model();
        value["source"] = json!({"HuggingFace":{"repo_id":repo,"revision":revision}});
        assert!(matches!(
            roots.resolve(&parse(value)),
            Err(BridgeError::UnsafePath)
        ));
    }
    let repo = temp.path().join("models--a--b");
    std::fs::create_dir_all(repo.join("refs")).unwrap();
    std::fs::write(repo.join("refs/main"), "../../outside").unwrap();
    let mut value = model();
    value["source"] = json!({"HuggingFace":{"repo_id":"a/b","revision":"main"}});
    assert!(matches!(
        roots.resolve(&parse(value)),
        Err(BridgeError::UnsafePath)
    ));
}

#[test]
fn resolves_hf_named_ref_before_other_cached_revisions_and_preserves_files() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("models--a--b");
    for rev in ["older", "pinned"] {
        std::fs::create_dir_all(repo.join("snapshots").join(rev)).unwrap();
        std::fs::write(repo.join("snapshots").join(rev).join("future.gguf"), rev).unwrap();
    }
    std::fs::create_dir_all(repo.join("refs")).unwrap();
    std::fs::write(repo.join("refs/main"), "pinned\n").unwrap();
    let roots = ModelRoots {
        hf_caches: vec![temp.path().to_owned()],
        models_dirs: vec![],
    };
    let mut value = model();
    value["source"] = json!({"HuggingFace":{"repo_id":"a/b","revision":"main"}});
    let resolved = roots.resolve(&parse(value)).unwrap();
    assert_eq!(resolved.revision.as_deref(), Some("pinned"));
    assert_eq!(std::fs::read_to_string(&resolved.path).unwrap(), "pinned");
    assert!(repo.join("snapshots/older/future.gguf").is_file());
    assert!(repo.join("refs/main").is_file());
}

#[test]
fn hf_revision_fallback_is_explicit_and_deterministic_like_handy() {
    let temp = TempDir::new().unwrap();
    let snapshot = temp.path().join("models--a--b/snapshots/old");
    std::fs::create_dir_all(&snapshot).unwrap();
    std::fs::write(snapshot.join("future.gguf"), "weights").unwrap();
    let mut value = model();
    value["source"] = json!({"HuggingFace":{"repo_id":"a/b","revision":"new"}});
    let result = ModelRoots {
        hf_caches: vec![temp.path().to_owned()],
        models_dirs: vec![],
    }
    .resolve(&parse(value))
    .unwrap();
    assert_eq!(result.resolution, "cached-snapshot");
    assert_eq!(result.revision.as_deref(), Some("old"));
}

#[test]
fn unknown_source_is_not_guessed_from_a_matching_local_file() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("future.gguf"), "weights").unwrap();
    let mut value = model();
    value["source"] = json!({"FutureCloud":{"url":"https://example.invalid/model"}});
    assert!(matches!(
        ModelRoots {
            hf_caches: vec![],
            models_dirs: vec![temp.path().to_owned()]
        }
        .resolve(&parse(value)),
        Err(BridgeError::UnsupportedSource)
    ));
}

#[test]
fn does_not_remove_or_load_partial_local_models() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("future.gguf"), "weights").unwrap();
    std::fs::write(temp.path().join("future.gguf.partial"), "partial").unwrap();
    let roots = ModelRoots {
        hf_caches: vec![],
        models_dirs: vec![temp.path().to_owned()],
    };
    assert!(matches!(
        roots.resolve(&parse(model())),
        Err(BridgeError::PartialModel)
    ));
    assert_eq!(
        std::fs::read(temp.path().join("future.gguf.partial")).unwrap(),
        b"partial"
    );
}

struct UpdatingRunner {
    calls: Cell<usize>,
}
impl ModelListRunner for UpdatingRunner {
    fn list_models(
        &self,
        _executable: &Path,
        _limits: CommandLimits,
    ) -> handy_bridge::Result<Vec<u8>> {
        let index = self.calls.get();
        self.calls.set(index + 1);
        if index == 2 {
            return Err(BridgeError::UnsupportedSchema);
        }
        let mut value = model();
        value["id"] = json!(format!("new-model-{index}"));
        Ok(serde_json::to_vec(&[value]).unwrap())
    }
}

#[test]
fn refresh_observes_upstream_additions_and_does_not_return_stale_data_on_failure() {
    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("handy-test");
    std::fs::write(&executable, "owned fixture").unwrap();
    let bridge = HandyBridge::new(&executable).unwrap();
    let runner = UpdatingRunner {
        calls: Cell::new(0),
    };
    let first = bridge.refresh_with(&runner).unwrap();
    let second = bridge.refresh_with(&runner).unwrap();
    assert_eq!(first.models[0].id, "new-model-0");
    assert_eq!(second.models[0].id, "new-model-1");
    assert_ne!(first.fingerprint_sha256, second.fingerprint_sha256);
    assert!(bridge.refresh_with(&runner).is_err());
}

#[test]
fn rejects_output_over_limit_before_json_parsing() {
    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("handy-test");
    std::fs::write(&executable, "fixture").unwrap();
    let bridge = HandyBridge::new(&executable)
        .unwrap()
        .with_limits(CommandLimits {
            stdout_bytes: 1,
            ..Default::default()
        });
    assert!(matches!(
        bridge.refresh_with(&UpdatingRunner {
            calls: Cell::new(0)
        }),
        Err(BridgeError::OutputLimit)
    ));
}

#[test]
fn rejects_duplicate_security_relevant_fields() {
    let bytes=br#"[{"id":"x","name":"x","filename":"x.gguf","source":"Local","is_downloaded":true,"is_downloading":false,"supports_streaming":false,"supports_streaming":true}]"#;
    assert!(matches!(
        parse_model_list(bytes),
        Err(BridgeError::UnsupportedSchema)
    ));
}
