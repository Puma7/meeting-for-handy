use super::*;
use crate::{library::LibraryIndex, PcmWriter, SessionStore, TranscriptCheckpoint};

struct TestProtection {
    reject: Option<&'static str>,
}

impl Protection for TestProtection {
    fn enable_directory(&self, path: &Path) -> Result<()> {
        assert!(fs::read_dir(path)?.next().is_none());
        Ok(())
    }

    fn check(&self, path: &Path) -> Result<()> {
        if self
            .reject
            .is_some_and(|name| path.file_name().is_some_and(|file| file == name))
        {
            return Err(NOT_PROTECTED.into());
        }
        Ok(())
    }
}

fn session(root: &Path) -> String {
    let store = SessionStore::create(
        &root.join("sessions"),
        "Migration fixture",
        "synthetic",
        "system",
        Some("microphone"),
    )
    .unwrap();
    for (track, text) in [
        ("system", "migrationalpha"),
        ("microphone", "migrationbeta"),
    ] {
        let mut writer = PcmWriter::new(store.clone(), track).unwrap();
        writer.append(&[400i16; 256]).unwrap();
        writer.finish().unwrap();
        let part = store
            .snapshot()
            .unwrap()
            .parts
            .into_iter()
            .find(|part| part.source == track)
            .unwrap();
        store
            .append_transcript(TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file,
                model_id: "synthetic".into(),
                full: text.into(),
                committed: text.into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
    }
    store.snapshot().unwrap().id
}

fn reindex(root: &Path, id: &str) -> LibraryIndex {
    let store = SessionStore::open(&root.join("sessions").join(id)).unwrap();
    let snapshot = store.snapshot().unwrap();
    let mut index = LibraryIndex::open(root).unwrap();
    index
        .upsert(&snapshot, &Default::default(), "synthetic")
        .unwrap();
    for term in ["migrationalpha", "migrationbeta"] {
        assert_eq!(index.search(term, 0, 10).unwrap().total, 1);
    }
    index
}

#[test]
fn copies_sessions_annotations_and_recovery_records_and_rebuilds_search() {
    let source = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let id = session(source.path());
    for (folder, file) in [
        ("archives", "archive-copy"),
        ("immediate-deletions", "partial-deletion"),
        ("user-metadata", "annotation"),
    ] {
        fs::create_dir(source.path().join(folder)).unwrap();
        fs::write(
            source.path().join(folder).join(file),
            b"synthetic durable bytes",
        )
        .unwrap();
    }
    fs::write(
        source.path().join("retention-settings.json.bak"),
        b"synthetic backup",
    )
    .unwrap();
    drop(reindex(source.path(), &id));
    let original_index = fs::read(source.path().join("library-index/library.sqlite3")).unwrap();
    let before = inventory(source.path()).unwrap();
    let protection = TestProtection { reject: None };
    let result = migrate_with(source.path(), parent.path(), &protection).unwrap();
    assert!(result.files > 7 && result.bytes > 100);
    assert_eq!(before, inventory(&result.path).unwrap());
    assert_eq!(before, inventory(source.path()).unwrap());
    assert_eq!(
        original_index,
        fs::read(source.path().join("library-index/library.sqlite3")).unwrap()
    );
    assert!(!result.path.join("library-index").exists());
    drop(reindex(&result.path, &id));
    verify_with(&result.path, &protection).unwrap();
    fs::remove_file(result.path.join(MARKER)).unwrap();
    assert!(needs_verification(&result.path).unwrap());
    assert!(verify_with(&result.path, &protection).is_err());
}

#[test]
fn failed_protection_and_active_sessions_leave_the_original_untouched() {
    let source = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let id = session(source.path());
    let before = inventory(source.path()).unwrap();
    let blocked = TestProtection {
        reject: Some("session.json"),
    };
    assert!(migrate_with(source.path(), parent.path(), &blocked).is_err());
    assert!(fs::read_dir(parent.path()).unwrap().next().is_none());

    let active = SessionStore::open(&source.path().join("sessions").join(id)).unwrap();
    let protection = TestProtection { reject: None };
    assert!(migrate_with(source.path(), parent.path(), &protection).is_err());
    drop(active);
    assert!(fs::read_dir(parent.path()).unwrap().next().is_none());
    assert_eq!(inventory(source.path()).unwrap(), before);
    assert!(migrate_with(source.path(), source.path(), &protection).is_err());

    let lease = reserve_runtime(source.path()).unwrap();
    assert!(reserve_runtime(source.path()).is_err());
    drop(lease);
    assert!(reserve_runtime(source.path()).is_ok());
}

#[test]
fn source_changes_during_copy_prevent_publication() {
    struct ChangingSource(PathBuf);
    impl Protection for ChangingSource {
        fn enable_directory(&self, _: &Path) -> Result<()> {
            Ok(())
        }
        fn check(&self, path: &Path) -> Result<()> {
            if path.file_name().is_some_and(|name| name == "session.json") {
                fs::write(&self.0, b"a concurrently added user file")?;
            }
            Ok(())
        }
    }

    let source = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let id = session(source.path());
    let late = source.path().join("sessions").join(id).join("user-file");
    assert!(migrate_with(source.path(), parent.path(), &ChangingSource(late.clone())).is_err());
    assert!(fs::read_dir(parent.path()).unwrap().next().is_none());
    assert_eq!(fs::read(late).unwrap(), b"a concurrently added user file");
}

#[test]
fn renamed_encrypted_copy_requires_verified_launch() {
    let root = tempfile::tempdir().unwrap();
    let renamed = root.path().join("restored-vault");
    fs::create_dir(&renamed).unwrap();
    assert!(!needs_verification(&renamed).unwrap());
    fs::write(renamed.join(VAULT_MARKER), b"encrypted vault marker").unwrap();
    assert!(needs_verification(&renamed).unwrap());
}

#[cfg(windows)]
#[test]
#[ignore = "Requires an isolated EFS-capable Windows test account and directory"]
fn native_efs_inherits_for_new_audio_and_sqlite_index() {
    let parent = PathBuf::from(std::env::var_os("HANDYMEETING_EFS_TEST_PARENT").unwrap());
    let owned = tempfile::tempdir_in(parent).unwrap();
    let source = tempfile::tempdir().unwrap();
    let id = session(source.path());
    let result = migrate(source.path(), owned.path()).unwrap();
    let _index = reindex(&result.path, &id);
    let new = session(&result.path);
    assert_ne!(id, new);
    for name in [
        "library.sqlite3",
        "library.sqlite3-wal",
        "library.sqlite3-shm",
    ] {
        assert!(platform::encrypted(&result.path.join("library-index").join(name)).unwrap());
    }
    verify(&result.path).unwrap();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "Requires the native encrypted-volume provider and a disposable local test environment"]
fn native_volume_migrates_and_recovers_from_a_detached_ciphertext_copy() {
    fn copy_ciphertext(from: &Path, to: &Path) {
        reject_link(from).unwrap();
        if from.is_dir() {
            fs::create_dir(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                copy_ciphertext(&entry.path(), &to.join(entry.file_name()));
            }
        } else {
            assert!(from.is_file());
            let bytes = fs::read(from).unwrap();
            assert!(!bytes
                .windows(b"migrationalpha".len())
                .any(|data| data == b"migrationalpha"));
            fs::copy(from, to).unwrap();
        }
    }

    use ring::rand::{generate, SystemRandom};
    let random_password = || {
        generate::<[u8; 16]>(&SystemRandom::new())
            .unwrap()
            .expose()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let password = zeroize::Zeroizing::new(random_password());
    let wrong = zeroize::Zeroizing::new(random_password());
    let source = tempfile::tempdir().unwrap();
    let vaults = tempfile::tempdir().unwrap();
    let copies = tempfile::tempdir().unwrap();
    let id = session(source.path());
    drop(reindex(source.path(), &id));
    let original_index = fs::read(source.path().join("library-index/library.sqlite3")).unwrap();
    let before = inventory(source.path()).unwrap();
    let active = SessionStore::open(&source.path().join("sessions").join(&id)).unwrap();
    assert!(migrate_volume(source.path(), vaults.path(), &password).is_err());
    assert!(fs::read_dir(vaults.path()).unwrap().next().is_none());
    drop(active);
    let unexpected = source.path().join("templates");
    fs::write(&unexpected, b"owned synthetic file in place of directory").unwrap();
    assert!(migrate_volume(source.path(), vaults.path(), &password).is_err());
    assert!(fs::read_dir(vaults.path()).unwrap().next().is_none());
    assert_eq!(
        fs::read(&unexpected).unwrap(),
        b"owned synthetic file in place of directory"
    );
    fs::remove_file(unexpected).unwrap();
    assert_eq!(before, inventory(source.path()).unwrap());
    let receipt = migrate_volume(source.path(), vaults.path(), &password).unwrap();
    assert!(receipt.files > 0 && receipt.bytes > 0);
    assert_eq!(before, inventory(source.path()).unwrap());
    assert_eq!(
        original_index,
        fs::read(source.path().join("library-index/library.sqlite3")).unwrap()
    );
    assert!(open_volume(&receipt.path, &wrong).is_err());

    let opened = open_volume(&receipt.path, &password).unwrap();
    assert_eq!(before, inventory(opened.root()).unwrap());
    assert!(!opened.root().join("library-index").exists());
    let index = reindex(opened.root(), &id);
    let new_id = session(opened.root());
    for name in [
        "library.sqlite3",
        "library.sqlite3-wal",
        "library.sqlite3-shm",
    ] {
        assert!(opened.root().join("library-index").join(name).is_file());
    }
    opened.verify().unwrap();
    assert!(open_volume(&receipt.path, &password).is_err());
    drop(index);
    let after = inventory(opened.root()).unwrap();
    opened.unmount().unwrap();

    let copied = copies.path().join("restored-vault");
    copy_ciphertext(&receipt.path, &copied);
    assert!(needs_verification(&copied).unwrap());
    let recovered = open_volume(&copied, &password).unwrap();
    assert_eq!(after, inventory(recovered.root()).unwrap());
    assert_eq!(
        LibraryIndex::open(recovered.root())
            .unwrap()
            .search("migrationalpha", 0, 10)
            .unwrap()
            .total,
        1
    );
    let newly_written =
        SessionStore::open(&recovered.root().join("sessions").join(new_id)).unwrap();
    assert_eq!(newly_written.snapshot().unwrap().parts.len(), 2);
    drop(newly_written);
    recovered.unmount().unwrap();
}
