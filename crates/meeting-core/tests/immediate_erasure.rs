//! Synthetic erasure fixtures only. These tests never inspect real sessions or audio devices.
use meeting_core::{
    archive, library::LibraryIndex, metadata, PcmWriter, SessionStore, TranscriptCheckpoint,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const PRIVATE: &str = "syntheticerasurecanaryneverretainthisphrase";

/// A readable competing identity is a conflict even if it cannot be a native
/// deletion target. Ignoring it must not authorize removal of foreign evidence.
#[test]
fn ineligible_competing_identities_block_erasure_and_preserve_all_bytes() {
    let mut accepted = Vec::new();
    for foreign in ["legacy-synthetic-other", "not/a/native/id"] {
        for kind in ["orphan-source", "maintenance-log", "temporary-log"] {
            let root = tempfile::tempdir().unwrap();
            let id = fixture(root.path(), PRIVATE, 1);
            if kind == "orphan-source" {
                let folder = root
                    .path()
                    .join("archives")
                    .join(format!("{}.source", "a".repeat(32)));
                extra(
                    &folder.join("session.json"),
                    &fs::read(root.path().join("sessions").join(&id).join("session.json")).unwrap(),
                );
                extra(
                    &folder.join("session.json.bak"),
                    &serde_json::to_vec(
                        &serde_json::json!({"id":foreign,"title":"Foreign backup evidence"}),
                    )
                    .unwrap(),
                );
            } else {
                let mut bytes = serde_json::to_vec(&serde_json::json!({"meetingId":id,"records":[{"meetingId":foreign,"title":"Foreign retained evidence"}]})).unwrap();
                if kind == "temporary-log" {
                    bytes.pop();
                }
                extra(
                    &root
                        .path()
                        .join("maintenance-log")
                        .join(if kind == "temporary-log" {
                            ".tmpMixed"
                        } else {
                            "mixed.json"
                        }),
                    &bytes,
                );
            }
            let before = tree(root.path());
            if archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_ok() {
                accepted.push(format!("{kind}:{foreign}"));
            }
            let after = tree(root.path());
            for (path, bytes) in before {
                assert_eq!(after.get(&path), Some(&bytes));
            }
            assert!(!root.path().join("immediate-deletions").exists());
        }
    }
    assert!(
        accepted.is_empty(),
        "Competing readable identities were ignored: {accepted:?}"
    );
}

/// Ambiguous shared logs are not single-meeting deletion authority, even at EOF.
/// Rebuildable multi-meeting retention plans remain disposable derived previews.
#[test]
fn conflicting_maintenance_identities_block_without_removing_foreign_records() {
    for temporary in [true, false] {
        for truncated in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let id = fixture(root.path(), PRIVATE, 1);
            let other = fixture(root.path(), "Foreign original retained", 1);
            let foreign = tree(&root.path().join("sessions").join(&other));
            let path = root.path().join("maintenance-log").join(if temporary {
                ".tmpMixed"
            } else {
                "mixed.json"
            });
            let mut bytes = serde_json::to_vec(&serde_json::json!({"meetingId":id,"records":[{"meetingId":other,"title":"Foreign maintenance evidence"}]})).unwrap();
            if truncated {
                bytes.pop();
            }
            extra(&path, &bytes);
            let before = tree(root.path());
            assert!(archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_err(), "Mixed maintenance identities authorized erasure: temporary={temporary},truncated={truncated}");
            let after = tree(root.path());
            for (path, bytes) in before {
                assert_eq!(after.get(&path), Some(&bytes));
            }
            assert!(!root.path().join("immediate-deletions").exists());
            fs::remove_file(&path).unwrap();
            let shared_plan = root.path().join("retention-plans").join(if temporary {
                ".tmpPlan"
            } else {
                "plan.json"
            });
            extra(&shared_plan, &serde_json::to_vec(&serde_json::json!({"candidates":[{"meetingId":id,"title":PRIVATE},{"meetingId":other,"title":"Rebuildable foreign plan row"}]})).unwrap());
            erase(root.path(), &id);
            assert!(!shared_plan.exists());
            assert_eq!(tree(&root.path().join("sessions").join(&other)), foreign);
        }
    }
}

/// An orphan retained source has no receipt to arbitrate two contradictory
/// readable generations. Preserve both identities and all original bytes.
#[test]
fn orphan_source_reconciles_both_manifest_generations_before_filtering() {
    for selected_primary in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let other = fixture(root.path(), "Foreign original must remain", 1);
        let source = root
            .path()
            .join("archives")
            .join(format!("{}.source", "a".repeat(32)));
        fs::create_dir_all(&source).unwrap();
        for (name, owner) in [
            ("session.json", if selected_primary { &id } else { &other }),
            (
                "session.json.bak",
                if selected_primary { &other } else { &id },
            ),
        ] {
            fs::copy(
                root.path()
                    .join("sessions")
                    .join(owner)
                    .join("session.json"),
                source.join(name),
            )
            .unwrap();
        }
        extra(&source.join("retained.txt"), PRIVATE.as_bytes());
        let before = tree(root.path());
        assert!(
            archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_err(),
            "An orphan source hid a conflicting readable manifest generation"
        );
        let after = tree(root.path());
        for (path, bytes) in before {
            assert_eq!(after.get(&path), Some(&bytes));
        }
    }
}

/// Matching orphan generations are attributable; consistent foreign generations
/// remain unrelated and must survive the confirmed selected-meeting erasure.
#[test]
fn consistent_orphan_sources_are_erased_or_preserved_by_exact_identity() {
    for selected in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let other = fixture(root.path(), "Foreign original must remain", 1);
        let source = root
            .path()
            .join("archives")
            .join(format!("{}.source", "a".repeat(32)));
        fs::create_dir_all(&source).unwrap();
        for name in ["session.json", "session.json.bak"] {
            fs::copy(
                root.path()
                    .join("sessions")
                    .join(if selected { &id } else { &other })
                    .join("session.json"),
                source.join(name),
            )
            .unwrap();
        }
        extra(&source.join("retained.txt"), b"synthetic retained source");
        let before = tree(&source);
        let foreign_before = tree(&root.path().join("sessions").join(&other));
        erase(root.path(), &id);
        if selected {
            assert!(!source.exists());
        } else {
            assert_eq!(tree(&source), before);
        }
        assert_eq!(
            tree(&root.path().join("sessions").join(&other)),
            foreign_before
        );
    }
}

/// Shared publication does not serialize independent writers, but erasure cannot
/// inspect/remove content until every publisher's final write has finished.
#[test]
fn publication_leases_allow_parallel_writers_and_exclude_erasure_until_all_end() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let cancel = AtomicBool::new(false);
    let preview = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    let first = archive::source_publication_lease(root.path()).unwrap();
    let second = archive::source_publication_lease(root.path()).unwrap();
    let before = tree(&root.path().join("sessions").join(&id));
    let assert_readable = |message: String| {
        assert!(
            message.contains("wait") && message.contains("bitte"),
            "No actionable bilingual contention message: {message}"
        );
        assert!(
            !message.contains("os error"),
            "Raw OS contention leaked: {message}"
        );
    };
    assert_readable(
        archive::preview_erasure(root.path(), &id, &cancel)
            .err()
            .unwrap()
            .to_string(),
    );
    assert_readable(
        archive::erase_meeting(
            root.path(),
            &id,
            &preview.fingerprint,
            &format!("DELETE {id}"),
            &cancel,
            |_, _| panic!("Blocked erasure must not start removal"),
        )
        .err()
        .unwrap()
        .to_string(),
    );
    drop(first);
    assert_readable(
        archive::preview_erasure(root.path(), &id, &cancel)
            .err()
            .unwrap()
            .to_string(),
    );
    assert_eq!(tree(&root.path().join("sessions").join(&id)), before);
    drop(second);
    erase(root.path(), &id);
}
/// Builds only a fresh synthetic PCM/transcript meeting under the supplied temporary root.
fn fixture(root: &Path, text: &str, seconds: usize) -> String {
    fs::create_dir_all(root.join("sessions")).unwrap();
    let store = SessionStore::create(
        &root.join("sessions"),
        text,
        "synthetic-model",
        "synthetic-output",
        Some("synthetic-input"),
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    for track in ["system", "microphone"] {
        let mut writer = PcmWriter::new(store.clone(), track).unwrap();
        writer.append(&vec![42; seconds * 16000]).unwrap();
        writer.finish().unwrap();
        drop(writer);
    }
    for part in store.snapshot().unwrap().parts {
        store
            .append_transcript(TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file,
                model_id: "synthetic-model".into(),
                full: text.into(),
                committed: text.into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
    }
    store.set_status("recorded").unwrap();
    store.set_status("complete").unwrap();
    drop(store);
    id
}
/// Snapshots synthetic content bytes for preservation checks, excluding open coordination locks.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == "session.lock") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
/// Adds one synthetic ordinary file with explicit bytes, never using a real session path.
fn extra(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A foreign receipt cannot mask this meeting's ZIP or retained-source identity.
#[test]
fn reverse_archive_assignment_conflicts_block_before_any_erasure() {
    for source_copy in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let other = fixture(root.path(), "Foreign preserved meeting", 1);
        let cancel = AtomicBool::new(false);
        let receipt =
            archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {}).unwrap();
        let folder = root.path().join("archives");
        // A completed/recovered archive can retain its receipt after creation's
        // transient pending record is gone; that receipt alone must not mask a copy.
        fs::remove_file(folder.join(format!("{}.pending.json", receipt.archive_id))).unwrap();
        if source_copy {
            fs::remove_file(folder.join(format!("{}.zip", receipt.archive_id))).unwrap();
            let source = folder.join(format!("{}.source", receipt.archive_id));
            fs::create_dir(&source).unwrap();
            fs::copy(
                root.path().join("sessions").join(&id).join("session.json"),
                source.join("session.json"),
            )
            .unwrap();
            extra(&source.join("retained.txt"), PRIVATE.as_bytes());
        }
        let path = folder.join(format!("{}.receipt.json", receipt.archive_id));
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["meetingId"] = other.into();
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let before = tree(root.path());
        assert!(
            archive::preview_erasure(root.path(), &id, &cancel).is_err(),
            "A foreign receipt masked selected-meeting content in an archive copy"
        );
        let after = tree(root.path());
        for (path, bytes) in &before {
            assert!(
                after.get(path) == Some(bytes),
                "Existing file changed: {}",
                path.display()
            );
        }
        assert!(
            after.iter().all(|(path, bytes)| before.contains_key(path)
                || (bytes.is_empty() && path.extension().is_some_and(|ext| ext == "lock"))),
            "Preview created non-coordination data"
        );
    }
}

/// Shared atomic catalogs must remove exact receipt identities and disclose unassignable annotations.
#[test]
fn archive_and_annotation_atomic_remnants_are_inventoried_or_disclosed() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let other = fixture(root.path(), "Foreign retained meeting", 1);
    let owned = root.path().join("archives/.tmpOwned");
    extra(
        &owned,
        format!("{{\"meetingId\":\"{id}\",\"title\":\"{PRIVATE}").as_bytes(),
    );
    let foreign = root.path().join("archives/.tmpForeign");
    let foreign_bytes = format!("{{\"meetingId\":\"{other}\",\"title\":\"Foreign\"}}").into_bytes();
    extra(&foreign, &foreign_bytes);
    let unknown = [
        ("archives/.tmpUnknown", format!("{{\"title\":\"{PRIVATE}\"}}")),
        ("user-metadata/.tmpUnknown", format!("{{\"revision\":0,\"title\":\"{PRIVATE}\",\"pinned\":false}}")),
        ("speaker-labels/.tmpUnknown", format!("{{\"revision\":0,\"enabled\":true,\"assignments\":[{{\"evidence\":\"{PRIVATE}\"}}]}}")),
    ];
    for (name, bytes) in &unknown {
        extra(&root.path().join(name), bytes.as_bytes());
    }
    let preview = archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        preview.preserved_atomic_remnants, 3,
        "Unknown shared-catalog contents were omitted from the disclosure"
    );
    erase(root.path(), &id);
    assert!(
        !owned.exists(),
        "A structurally attributed receipt retained meeting content"
    );
    assert_eq!(fs::read(foreign).unwrap(), foreign_bytes);
    for (name, bytes) in &unknown {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), bytes.as_bytes());
    }
}

/// Only documented empty relocation locks may remain in the shared coordination directory.
#[test]
fn archive_coordination_namespace_cannot_hide_unknown_or_nonempty_content() {
    for (name, bytes) in [
        ("unexpected.txt".to_owned(), PRIVATE.as_bytes()),
        (format!("{}.lock", "a".repeat(64)), PRIVATE.as_bytes()),
    ] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let path = root.path().join("archives/.leases").join(name);
        extra(&path, bytes);
        assert!(archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(root.path().join("sessions").join(id).exists());
    }
}
/// Exercises the public preview and exact-meeting confirmation APIs to verified completion.
fn erase(root: &Path, id: &str) {
    let cancel = AtomicBool::new(false);
    let plan = archive::preview_erasure(root, id, &cancel).unwrap();
    let result = archive::erase_meeting(
        root,
        id,
        &plan.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {},
    )
    .unwrap();
    assert!(result.complete);
}

/// Partial atomic records require structural identity, never title text or raw ID bytes.
#[test]
fn partial_atomic_records_use_exact_identity_and_preserve_unknown_or_foreign_bytes() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let other = fixture(root.path(), "Foreign original retained", 1);
    let mut preserved = Vec::new();
    for parent in ["maintenance-log", "retention-plans", "audio-copies"] {
        let folder = root.path().join(parent);
        extra(
            &folder.join(".tmpOwned"),
            format!("{{\"meetingId\":\"{id}\",\"title\":\"{PRIVATE}").as_bytes(),
        );
        for (name, bytes) in [
            (
                ".tmpForeign",
                format!("{{\"meetingId\":\"{other}\",\"title\":\"{id}\"}}"),
            ),
            (
                ".tmpUnknown",
                "{\"title\":\"unfinished unassigned metadata".into(),
            ),
            (
                ".tmpQuoted",
                format!("{{\"title\":\"\\\"meetingId\\\":\\\"{id}\\\""),
            ),
        ] {
            let path = folder.join(name);
            extra(&path, bytes.as_bytes());
            preserved.push((path, bytes.into_bytes()));
        }
        if parent != "audio-copies" {
            let path = folder.join("foreign.json");
            let bytes=serde_json::to_vec(&serde_json::json!({"meetingId":other,"title":id,"path":format!("sessions/{id}/audio")})).unwrap();
            extra(&path, &bytes);
            preserved.push((path, bytes));
        }
    }
    extra(
        &root.path().join("retention-plans/.tmpNested"),
        format!("{{\"candidates\":[{{\"meetingId\":\"{id}\",\"title\":\"{PRIVATE}").as_bytes(),
    );
    let cancel = AtomicBool::new(false);
    let malformed = root.path().join("maintenance-log/broken.json");
    extra(&malformed, b"{ incomplete ordinary record");
    assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
    fs::remove_file(malformed).unwrap();
    let preview = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    assert_eq!(
        serde_json::to_value(&preview).unwrap()["preservedAtomicRemnants"],
        6
    );
    let changed = root.path().join("maintenance-log/.tmpChangedScope");
    extra(&changed, b"{ unfinished unrelated metadata");
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &preview.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {}
    )
    .is_err());
    assert!(root.path().join("sessions").join(&id).exists());
    fs::remove_file(changed).unwrap();
    erase(root.path(), &id);
    for (path, bytes) in preserved {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    for parent in ["maintenance-log", "retention-plans", "audio-copies"] {
        assert!(!root.path().join(parent).join(".tmpOwned").exists());
    }
    assert!(!root.path().join("retention-plans/.tmpNested").exists());
}

/// An actual archive cancellation leaves an attributed ZIP without its final manifest.
#[test]
fn cancelled_archive_creation_does_not_block_explicit_meeting_erasure() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let other = fixture(root.path(), "Other original retained", 1);
    let other_folder = root.path().join("sessions").join(other);
    let before = tree(&other_folder);
    let cancel = AtomicBool::new(false);
    assert!(
        archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |n, _| {
            if n >= 1 {
                cancel.store(true, Ordering::Release);
            }
        })
        .is_err()
    );
    assert!(fs::read_dir(root.path().join("archives"))
        .unwrap()
        .any(|e| e.unwrap().path().extension().is_some_and(|s| s == "zip")));
    erase(root.path(), &id);
    assert_eq!(tree(&other_folder), before);
    assert!(!root.path().join("sessions").join(&id).exists());
    assert_eq!(
        fs::read_dir(root.path().join("archives")).unwrap().count(),
        0
    );
}

/// Pending attribution never authorizes a readable foreign ZIP manifest or an unattributed ZIP.
#[test]
fn incomplete_zip_identity_conflicts_and_missing_attribution_still_block_erasure() {
    use std::io::Write;
    for name in [
        "ARCHIVE_MANIFEST.json",
        "session/session.json",
        "session/session.json.bak",
    ] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let foreign = fixture(root.path(), "Foreign ZIP identity preserved", 1);
        let aid = "a".repeat(32);
        let folder = root.path().join("archives");
        fs::create_dir(&folder).unwrap();
        let path = folder.join(format!("{aid}.zip"));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        let value = if name == "ARCHIVE_MANIFEST.json" {
            serde_json::json!({"meetingId":foreign,"session":{"id":foreign}})
        } else {
            serde_json::json!({"id":foreign})
        };
        zip.write_all(&serde_json::to_vec(&value).unwrap()).unwrap();
        zip.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        let cancel = AtomicBool::new(false);
        assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
        extra(
            &folder.join(format!("{aid}.pending.json")),
            &serde_json::to_vec(&serde_json::json!({"meetingId":id,"kind":"whole-meeting"}))
                .unwrap(),
        );
        assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

/// Even a hash-valid ZIP cannot authorize removing a different embedded meeting.
#[test]
fn verified_and_orphan_zips_reject_foreign_embedded_session_identities() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    for ineligible in [false, true] {
        for orphan in [false, true] {
            for name in ["session/session.json", "session/session.json.bak"] {
                let root = tempfile::tempdir().unwrap();
                let id = fixture(root.path(), PRIVATE, 1);
                let foreign = if ineligible {
                    "legacy-synthetic-foreign".to_string()
                } else {
                    fixture(root.path(), "Foreign ZIP contents preserved", 1)
                };
                let cancel = AtomicBool::new(false);
                let receipt =
                    archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {})
                        .unwrap();
                let archives = root.path().join("archives");
                let path = archives.join(format!("{}.zip", receipt.archive_id));
                let mut entries = BTreeMap::new();
                {
                    let mut reader = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
                    for n in 0..reader.len() {
                        let mut entry = reader.by_index(n).unwrap();
                        let mut bytes = Vec::new();
                        entry.read_to_end(&mut bytes).unwrap();
                        entries.insert(entry.name().unwrap().into_owned(), bytes);
                    }
                }
                let mut embedded: serde_json::Value =
                    serde_json::from_slice(&entries[name]).unwrap();
                embedded["id"] = foreign.into();
                let bytes = serde_json::to_vec(&embedded).unwrap();
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&entries["ARCHIVE_MANIFEST.json"]).unwrap();
                let item = manifest["files"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|item| item["name"] == name)
                    .unwrap();
                item["bytes"] = (bytes.len() as u64).into();
                item["sha256"] = format!("{:x}", Sha256::digest(&bytes)).into();
                entries.insert(name.into(), bytes);
                entries.insert(
                    "ARCHIVE_MANIFEST.json".into(),
                    serde_json::to_vec(&manifest).unwrap(),
                );
                let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
                for (name, bytes) in entries {
                    if name.ends_with('/') {
                        writer
                            .add_directory(name, zip::write::SimpleFileOptions::default())
                            .unwrap();
                    } else {
                        writer
                            .start_file(name, zip::write::SimpleFileOptions::default())
                            .unwrap();
                        writer.write_all(&bytes).unwrap();
                    }
                }
                writer.finish().unwrap();
                assert!(
                    archive::verify(&path, Some(&cancel), |_, _| {}).is_ok(),
                    "Fixture must pass full inventory/hash verification"
                );
                if orphan {
                    for suffix in ["pending.json", "receipt.json"] {
                        fs::remove_file(archives.join(format!("{}.{suffix}", receipt.archive_id)))
                            .unwrap();
                    }
                }
                let before = tree(root.path());
                assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err(), "Hash-valid ZIP concealed a foreign embedded identity: orphan={orphan}, record={name}");
                let after = tree(root.path());
                for (path, bytes) in before {
                    assert!(
                        after.get(&path) == Some(&bytes),
                        "Existing synthetic file changed: {}",
                        path.display()
                    );
                }
            }
        }
    }
}

/// The confirmed interruption marker must survive loss of the original creation record.
#[test]
fn cancelled_zip_erasure_can_resume_after_pending_file_was_removed() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let cancel = AtomicBool::new(false);
    assert!(
        archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |n, _| {
            if n >= 1 {
                cancel.store(true, Ordering::Release);
            }
        })
        .is_err()
    );
    let folder = root.path().join("archives");
    let zip = fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|ext| ext == "zip"))
        .unwrap();
    let pending = zip.with_extension("pending.json");
    cancel.store(false, Ordering::Release);
    let preview = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    let result = archive::erase_meeting(
        root.path(),
        &id,
        &preview.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {
            if !pending.exists() && zip.exists() {
                cancel.store(true, Ordering::Release);
            }
        },
    );
    assert!(result.is_err());
    assert!(
        !pending.exists() && zip.exists(),
        "Fixture must stop after pending removal but before ZIP removal"
    );
    assert_eq!(archive::pending_erasures(root.path()).unwrap().len(), 1);
    cancel.store(false, Ordering::Release);
    let fresh = archive::preview_erasure(root.path(), &id, &cancel).expect(
        "Confirmed archive identity must remain usable after the original pending file is removed",
    );
    archive::erase_meeting(
        root.path(),
        &id,
        &fresh.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {},
    )
    .unwrap();
    assert!(!zip.exists());
    assert!(!root.path().join("sessions").join(id).exists());
    assert!(archive::pending_erasures(root.path()).unwrap().is_empty());
}

/// A short native fixture is erasable directly while automatic retention remains archive-only.
#[test]
fn a_new_two_second_meeting_can_be_erased_without_archival_age_or_model_service() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 2);
    let other = fixture(root.path(), "synthetic other retained", 1);
    let other_folder = root.path().join("sessions").join(&other);
    let before = tree(&other_folder);
    let cancel = AtomicBool::new(false);
    assert!(
        archive::deletion_fingerprint(root.path(), &id, &cancel).is_err(),
        "Automatic retention must remain archive-only"
    );
    erase(root.path(), &id);
    assert!(!root.path().join("sessions").join(&id).exists());
    assert_eq!(tree(&other_folder), before);
    assert!(archive::pending_erasures(root.path()).unwrap().is_empty());
}

/// A synthetic content canary disappears from all attributed stores while other originals remain byte-equal.
#[test]
fn every_app_managed_source_copy_annotation_and_search_cache_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 2);
    let other = fixture(root.path(), "other original preserved", 1);
    let folder = root.path().join("sessions").join(&id);
    let other_folder = root.path().join("sessions").join(&other);
    let other_before = tree(&other_folder);
    for name in [
        "analyses/run/result.json",
        "analyses/run/snapshot.json",
        "live-notes/run/window.json",
        "speaker-experiments/result.json",
        "converted-audio/run/audio.flac",
        "exports/transcript.html",
        "extra-user-notes.txt",
    ] {
        extra(&folder.join(name), PRIVATE.as_bytes());
    }
    let cancel = AtomicBool::new(false);
    let receipt =
        archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {}).unwrap();
    metadata::edit(root.path(), &id, 0, Some(PRIVATE.into()), false).unwrap();
    metadata::edit(
        root.path(),
        &id,
        1,
        Some(format!("{PRIVATE} revised")),
        true,
    )
    .unwrap();
    extra(
        &root
            .path()
            .join("speaker-labels")
            .join(format!("{id}.json")),
        format!("{{\"meetingId\":\"{id}\",\"quote\":\"{PRIVATE}\"}}").as_bytes(),
    );
    let run = "a".repeat(32);
    extra(
        &root.path().join("audio-copies").join(format!("{run}.json")),
        format!("{{\"meetingId\":\"{id}\",\"runId\":\"{run}\",\"title\":\"{PRIVATE}\"}}")
            .as_bytes(),
    );
    for parent in ["retention-plans", "maintenance-log"] {
        extra(
            &root.path().join(parent).join("synthetic.json"),
            format!("{{\"meetingId\":\"{id}\",\"title\":\"{PRIVATE}\"}}").as_bytes(),
        );
    }
    let data = meeting_core::inspect_manifest(&folder).unwrap();
    {
        let mut index = LibraryIndex::open(root.path()).unwrap();
        index
            .upsert(
                &data,
                &metadata::read(root.path(), &id).unwrap(),
                "synthetic",
            )
            .unwrap();
        assert!(index.search(PRIVATE, 0, 10).unwrap().total > 0);
    }
    extra(
        &root
            .path()
            .join("library-index/unreadable-synthetic/library.sqlite3"),
        PRIVATE.as_bytes(),
    );
    extra(
        &root.path().join("settings.json"),
        b"other settings unchanged",
    );
    let settings = fs::read(root.path().join("settings.json")).unwrap();
    erase(root.path(), &id);
    assert!(!folder.exists());
    assert!(!root
        .path()
        .join("archives")
        .join(format!("{}.zip", receipt.archive_id))
        .exists());
    assert!(!root.path().join("library-index").exists());
    assert_eq!(tree(&other_folder), other_before);
    assert_eq!(
        fs::read(root.path().join("settings.json")).unwrap(),
        settings
    );
    for (path, bytes) in tree(root.path()) {
        assert!(
            !bytes
                .windows(PRIVATE.len())
                .any(|part| part == PRIVATE.as_bytes()),
            "Retained private content: {path:?}"
        );
    }
    let other_data = meeting_core::inspect_manifest(&other_folder).unwrap();
    let mut index = LibraryIndex::open(root.path()).unwrap();
    index
        .upsert(&other_data, &Default::default(), "other")
        .unwrap();
    assert_eq!(index.search(PRIVATE, 0, 10).unwrap().total, 0);
    assert!(index
        .search("other", 0, 10)
        .unwrap()
        .items
        .iter()
        .all(|hit| hit.id == other));
}

/// Wrong identity/confirmation and post-preview changes cannot grant destructive authority.
#[test]
fn stale_preview_or_wrong_confirmation_preserves_all_original_bytes() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let folder = root.path().join("sessions").join(&id);
    let cancel = AtomicBool::new(false);
    let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    let before = tree(&folder);
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &plan.fingerprint,
        "DELETE another",
        &cancel,
        |_, _| {}
    )
    .is_err());
    assert_eq!(tree(&folder), before);
    extra(&folder.join("new-user-data.txt"), b"Added after preview");
    let changed = tree(&folder);
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &plan.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {}
    )
    .is_err());
    assert_eq!(tree(&folder), changed);
    assert!(archive::pending_erasures(root.path()).unwrap().is_empty());
}

/// Cancellation preserves only minimal recovery authority and allows a newly confirmed remaining inventory.
#[test]
fn cancellation_leaves_a_minimal_retry_marker_and_remaining_data_can_be_confirmed_again() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let cancel = AtomicBool::new(false);
    let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &plan.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |n, _| {
            if n == 1 {
                cancel.store(true, Ordering::Release);
            }
        }
    )
    .is_err());
    let pending = archive::pending_erasures(root.path()).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].meeting_id, id);
    let marker = fs::read_to_string(
        root.path()
            .join("immediate-deletions")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert!(!marker.contains(PRIVATE));
    assert!(!marker.contains("inventory"));
    erase(root.path(), &id);
    assert!(!root.path().join("sessions").join(id).exists());
    assert!(archive::pending_erasures(root.path()).unwrap().is_empty());
}

/// Real filesystem leases exclude a conflicting source reader/writer or SQLite connection.
#[test]
fn held_session_or_index_lease_prevents_removal_before_any_data_is_deleted() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let folder = root.path().join("sessions").join(&id);
    let cancel = AtomicBool::new(false);
    let writer = SessionStore::open(&folder).unwrap();
    let before = tree(&folder);
    assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
    assert_eq!(tree(&folder), before);
    drop(writer);
    let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    let index = LibraryIndex::open(root.path()).unwrap();
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &plan.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |_, _| {}
    )
    .is_err());
    assert!(folder.exists());
    drop(index);
    erase(root.path(), &id);
}

/// Unsafe identities, external originals and unknown cache content remain outside erasure authority.
#[test]
fn traversal_imports_and_unknown_cache_files_are_refused_without_widening_scope() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let folder = root.path().join("sessions").join(&id);
    let before = tree(&folder);
    let cancel = AtomicBool::new(false);
    for value in ["../outside", "legacy-0123456789abcdef", "", "/absolute"] {
        assert!(archive::preview_erasure(root.path(), value, &cancel).is_err());
    }
    extra(
        &root.path().join("library-index/user-original.txt"),
        b"Do not delete an unrecognized cache entry",
    );
    assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
    assert_eq!(tree(&folder), before);
    assert!(root.path().join("library-index/user-original.txt").exists());
}

/// Empty/failed source folders are removable without a model service or replayable transcript.
#[test]
fn empty_failed_and_corrupt_owned_meetings_need_neither_asr_nor_valid_transcripts() {
    for state in ["recorded", "error", "corrupt"] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 0);
        let folder = root.path().join("sessions").join(&id);
        if state == "corrupt" {
            fs::write(folder.join("session.json"), b"broken JSON").unwrap();
        } else {
            let store = SessionStore::open(&folder).unwrap();
            store.set_status(state).unwrap();
        }
        erase(root.path(), &id);
        assert!(!folder.exists());
    }
}

/// Every archived file boundary, including lost manifests/receipts, remains recoverable by a fresh preview.
#[test]
fn every_archived_removal_boundary_can_be_confirmed_again_after_interruption() {
    // Exercise every file boundary, including the loss of receipts/manifests and
    // the last file before empty-directory cleanup. Every case is a new fixture.
    let mut boundary = 1;
    loop {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let cancel = AtomicBool::new(false);
        let receipt =
            archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {}).unwrap();
        let retained =
            archive::move_source_into_archive(root.path(), &receipt, 0, &cancel).unwrap();
        extra(&retained.join("later-analysis.json"), PRIVATE.as_bytes());
        let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
        if boundary > plan.files {
            break;
        }
        let result = archive::erase_meeting(
            root.path(),
            &id,
            &plan.fingerprint,
            &format!("DELETE {id}"),
            &cancel,
            |n, _| {
                if n == boundary as u64 {
                    cancel.store(true, Ordering::Release);
                }
            },
        );
        assert!(result.is_err(), "boundary {boundary}");
        assert_eq!(archive::pending_erasures(root.path()).unwrap().len(), 1);
        erase(root.path(), &id);
        assert!(!retained.exists(), "boundary {boundary}");
        assert!(!root
            .path()
            .join("archives")
            .join(format!("{}.zip", receipt.archive_id))
            .exists());
        for (_, bytes) in tree(root.path()) {
            assert!(!bytes
                .windows(PRIVATE.len())
                .any(|p| p == PRIVATE.as_bytes()));
        }
        boundary += 1;
    }
    assert!(boundary > 10);
}

/// Concurrent unconfirmed bytes are preserved and prevent a false complete result.
#[test]
fn late_files_and_changed_bytes_survive_with_an_explicit_partial_failure() {
    for change in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let folder = root.path().join("sessions").join(&id);
        extra(&folder.join("zz-late.txt"), b"original");
        let cancel = AtomicBool::new(false);
        let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
        let path = folder.join(if change {
            "zz-late.txt"
        } else {
            "new-after-start.txt"
        });
        assert!(archive::erase_meeting(
            root.path(),
            &id,
            &plan.fingerprint,
            &format!("DELETE {id}"),
            &cancel,
            |n, _| {
                if n == 1 {
                    fs::write(&path, b"late bytes must survive").unwrap();
                }
            }
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"late bytes must survive");
        erase(root.path(), &id);
        assert!(!folder.exists());
    }
}

/// Unlinking one app-owned hardlink never overwrites an external copy of its bytes.
#[test]
fn deleting_a_link_name_never_overwrites_an_external_hardlink_copy() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let original = external.path().join("external-copy.txt");
    fs::write(&original, b"external copy retained").unwrap();
    fs::hard_link(
        &original,
        root.path().join("sessions").join(&id).join("shared.txt"),
    )
    .unwrap();
    erase(root.path(), &id);
    assert_eq!(fs::read(original).unwrap(), b"external copy retained");
}

/// Audio-only and independently attributable orphan archives join the same confirmed deletion group.
#[test]
fn audio_only_archives_and_unreceipted_source_copies_are_included() {
    for audio_only in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let cancel = AtomicBool::new(false);
        if audio_only {
            archive::archive_audio(root.path(), &id, 0, &cancel, |_, _| {}).unwrap();
        } else {
            let receipt =
                archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {}).unwrap();
            archive::move_source_into_archive(root.path(), &receipt, 0, &cancel).unwrap();
            // Simulate a recovered retained source whose creation receipt is absent.
            fs::remove_file(
                root.path()
                    .join("archives")
                    .join(format!("{}.receipt.json", receipt.archive_id)),
            )
            .unwrap();
        }
        erase(root.path(), &id);
        assert!(!root.path().join("sessions").join(&id).exists());
        for (_, bytes) in tree(root.path()) {
            assert!(!bytes
                .windows(PRIVATE.len())
                .any(|p| p == PRIVATE.as_bytes()));
        }
    }
}

/// Valid foreign identities defeat stale catalog/path attribution before any file is removed.
#[test]
fn conflicting_source_and_archive_identities_never_authorize_foreign_data_removal() {
    for archive_conflict in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let other = fixture(root.path(), "Other meeting stays", 1);
        let cancel = AtomicBool::new(false);
        if archive_conflict {
            let receipt =
                archive::create(root.path(), &other, 0, "whole-meeting", &cancel, |_, _| {})
                    .unwrap();
            let path = root
                .path()
                .join("archives")
                .join(format!("{}.receipt.json", receipt.archive_id));
            let mut record: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            record["meetingId"] = serde_json::json!(id);
            fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
        } else {
            let path = root.path().join("sessions").join(&id).join("session.json");
            let mut data: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            data["id"] = serde_json::json!(other);
            fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
        }
        let before = tree(root.path());
        assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
        for (path, bytes) in before {
            assert_eq!(fs::read(root.path().join(path)).unwrap(), bytes);
        }
    }
}

/// Unpublished atomic-write remnants stay inert while valid continuation markers remain discoverable.
#[test]
fn interrupted_marker_temp_files_do_not_hide_valid_pending_erasures() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let cancel = AtomicBool::new(false);
    let plan = archive::preview_erasure(root.path(), &id, &cancel).unwrap();
    assert!(archive::erase_meeting(
        root.path(),
        &id,
        &plan.fingerprint,
        &format!("DELETE {id}"),
        &cancel,
        |n, _| {
            if n == 1 {
                cancel.store(true, Ordering::Release)
            }
        }
    )
    .is_err());
    let orphan = root.path().join("immediate-deletions/.tmpAbC123");
    extra(
        &orphan,
        b"unfinished atomic marker bytes; never erasure authority",
    );
    let pending = archive::pending_erasures(root.path()).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].meeting_id, id);
    erase(root.path(), &id);
    assert_eq!(
        fs::read(&orphan).unwrap(),
        b"unfinished atomic marker bytes; never erasure authority"
    );
    assert!(archive::pending_erasures(root.path()).unwrap().is_empty());
}

/// Neither attributed nor orphan restore staging may hide a foreign meeting under the selected folder name.
#[test]
fn restoration_staging_must_not_contain_a_valid_foreign_meeting_manifest() {
    for attributed in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), PRIVATE, 1);
        let other = fixture(root.path(), "Foreign staged meeting retained", 1);
        let cancel = AtomicBool::new(false);
        let aid = if attributed {
            archive::create(root.path(), &id, 0, "whole-meeting", &cancel, |_, _| {})
                .unwrap()
                .archive_id
        } else {
            "a".repeat(32)
        };
        let stage = root
            .path()
            .join("archives")
            .join(format!("{aid}.restoring-synthetic"))
            .join(&id);
        extra(
            &stage.join("session.json"),
            &fs::read(
                root.path()
                    .join("sessions")
                    .join(&other)
                    .join("session.json"),
            )
            .unwrap(),
        );
        extra(&stage.join("audio/foreign.wav"), b"foreign staged audio");
        let before = tree(&stage);
        assert!(archive::preview_erasure(root.path(), &id, &cancel).is_err());
        assert_eq!(tree(&stage), before);
    }
}

#[cfg(windows)]
/// A real synthetic Windows junction is rejected without touching its external target.
#[test]
fn windows_directory_junction_never_authorizes_outside_removal() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let link = root.path().join("sessions").join(&id).join("junction");
    fs::write(outside.path().join("secret.txt"), b"external retained").unwrap();
    let result = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_err());
    assert_eq!(
        fs::read(outside.path().join("secret.txt")).unwrap(),
        b"external retained"
    );
    // Remove only our synthetic junction itself; never traverse its target.
    fs::remove_dir(link).unwrap();
}

#[cfg(unix)]
/// Unix link fixtures must preserve outside bytes and refuse the entire unsafe preview.
#[test]
fn links_inside_a_meeting_never_authorize_removing_outside_files() {
    let root = tempfile::tempdir().unwrap();
    let id = fixture(root.path(), PRIVATE, 1);
    let outside = tempfile::tempdir().unwrap();
    extra(&outside.path().join("secret.txt"), b"outside retained");
    std::os::unix::fs::symlink(
        outside.path(),
        root.path().join("sessions").join(&id).join("linked"),
    )
    .unwrap();
    assert!(archive::preview_erasure(root.path(), &id, &AtomicBool::new(false)).is_err());
    assert_eq!(
        fs::read(outside.path().join("secret.txt")).unwrap(),
        b"outside retained"
    );
}
