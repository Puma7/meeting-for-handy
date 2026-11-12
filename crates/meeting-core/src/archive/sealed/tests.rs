use super::*;

#[test]
fn publication_sync_failure_removes_only_the_owned_recovery() {
    for fail_on_parent in [false, true] {
        let staging = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let unrelated = directory.path().join("keep.txt");
        fs::write(&unrelated, b"unrelated destination bytes").unwrap();
        let profile = staging.path().join("profile");
        fs::create_dir(&profile).unwrap();
        fs::write(profile.join("synthetic-audio"), b"generated audio").unwrap();
        let error = publish_recovery(&profile, directory.path(), |path| {
            if (path == directory.path()) == fail_on_parent {
                Err("synthetic publication sync failure".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "synthetic publication sync failure");
        assert_eq!(
            fs::read(&unrelated).unwrap(),
            b"unrelated destination bytes"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[cfg(windows)]
#[test]
fn blocked_publication_cleanup_reports_where_the_plaintext_remains() {
    use std::{cell::RefCell, os::windows::fs::OpenOptionsExt};

    let staging = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let profile = staging.path().join("profile");
    fs::create_dir(&profile).unwrap();
    fs::write(profile.join("synthetic-audio"), b"generated audio").unwrap();
    let held = RefCell::new(None);
    let error = publish_recovery(&profile, directory.path(), |path| {
        let published = path.join("profile");
        let file = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(published.join("synthetic-audio"))?;
        *held.borrow_mut() = Some((published, file));
        Err("synthetic publication sync failure".into())
    })
    .unwrap_err()
    .to_string();
    let (published, file) = held.into_inner().unwrap();
    assert!(error.contains("Could not remove the unfinished recovery"));
    assert!(error.contains(&published.display().to_string()));
    drop(file);
    assert_eq!(
        fs::read(published.join("synthetic-audio")).unwrap(),
        b"generated audio"
    );
}

const TITLE: &str = "SYNTHETIC_PRIVATE_BACKUP_TITLE";

fn synthetic_passphrase() -> Zeroizing<String> {
    RecoveryKey::generate().unwrap().encode()
}

fn fixture(root: &Path) -> (String, PathBuf) {
    let store = SessionStore::create(
        &root.join("sessions"),
        TITLE,
        "synthetic",
        "system",
        Some("microphone"),
    )
    .unwrap();
    for (track, sample, words) in [
        ("system", 1_410, "SYNTHETIC_SYSTEM_TRANSCRIPT"),
        ("microphone", -2_310, "SYNTHETIC_MICROPHONE_TRANSCRIPT"),
    ] {
        let mut writer = crate::PcmWriter::new(store.clone(), track).unwrap();
        writer.append(&vec![sample; 40_000]).unwrap();
        writer.finish().unwrap();
        let part = store
            .snapshot()
            .unwrap()
            .parts
            .into_iter()
            .find(|part| part.source == track)
            .unwrap();
        store
            .append_transcript(crate::TranscriptCheckpoint {
                sequence: 0,
                at: String::new(),
                file: part.file,
                model_id: "synthetic".into(),
                full: words.into(),
                committed: words.into(),
                tentative: String::new(),
                processed_samples: part.confirmed_sample_count,
                final_text: true,
                revision_after_restart: false,
            })
            .unwrap();
    }
    let folder = store.folder().to_owned();
    fs::create_dir(folder.join("analyses")).unwrap();
    fs::create_dir(folder.join("exports")).unwrap();
    fs::write(
        folder.join("analyses/revision-00000001.json"),
        b"SYNTHETIC_ANALYSIS",
    )
    .unwrap();
    fs::write(
        folder.join("exports/transcript-42.txt"),
        b"SYNTHETIC_EXPORT",
    )
    .unwrap();
    let id = store.snapshot().unwrap().id;
    drop(store);
    fs::create_dir(root.join("user-metadata")).unwrap();
    fs::write(
        root.join("user-metadata").join(format!("{id}.json")),
        b"{\"title\":\"SYNTHETIC_LABEL\"}",
    )
    .unwrap();
    (id, folder)
}

fn fingerprints(folder: &Path) -> Vec<ArchivedFile> {
    let mut files = Vec::new();
    inventory(folder, folder, &mut files, &mut Vec::new()).unwrap();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
        .into_iter()
        .map(|(name, file)| {
            let (bytes, sha256) = digest(&file).unwrap();
            ArchivedFile {
                name,
                bytes,
                sha256,
            }
        })
        .collect()
}

fn seal_for_test(path: &Path, manifest: &Inventory, content: &[u8], passphrase: &[u8]) {
    let mut output = File::create(path).unwrap();
    let key = RecoveryKey::generate().unwrap();
    let mut stream = frames::Encrypt::new(&mut output, passphrase, &key).unwrap();
    let bytes = serde_json::to_vec(manifest).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    stream.write_all(content).unwrap();
    stream.finish().unwrap();
}

#[test]
fn backup_and_recovery_preserve_every_source_byte_in_an_independent_profile() {
    let source = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let (id, folder) = fixture(source.path());
    let before = fingerprints(&folder);
    let backup = elsewhere.path().join("copy.hmvault");
    let stop = AtomicBool::new(false);
    let passphrase = synthetic_passphrase();
    let (summary, key) = create(source.path(), &id, &backup, passphrase.as_bytes(), &stop).unwrap();
    assert_eq!(summary.files, before.len() + 1);
    let encoded = key.encode();
    assert_eq!(encoded.len(), 64);
    drop(key);
    let key = RecoveryKey::parse(&encoded.to_ascii_uppercase()).unwrap();
    assert_eq!(fingerprints(&folder), before);
    assert!(create(source.path(), &id, &backup, passphrase.as_bytes(), &stop).is_err());
    let encrypted = fs::read(&backup).unwrap();
    for secret in [
        TITLE,
        "SYNTHETIC_SYSTEM_TRANSCRIPT",
        "SYNTHETIC_ANALYSIS",
        "SYNTHETIC_LABEL",
        "session/session.json",
        &id,
    ] {
        assert!(!encrypted
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()));
    }
    assert!(!encrypted
        .windows(encoded.len())
        .any(|window| window == encoded.as_bytes()));
    assert_eq!(
        verify(&backup, Unlock::Passphrase(passphrase.as_bytes()), &stop)
            .unwrap()
            .meeting_id,
        id
    );
    let preserved = source.path().join("source-preserved-separately");
    fs::rename(&folder, &preserved).unwrap();
    let restored = recover(
        active.path(),
        &backup,
        elsewhere.path(),
        Unlock::RecoveryKey(&key),
        &stop,
    )
    .unwrap();
    let restored_folder = restored.join("sessions").join(&id);
    assert_eq!(fingerprints(&restored_folder), before);
    let label = Path::new("user-metadata").join(format!("{id}.json"));
    assert_eq!(
        fs::read(restored.join(&label)).unwrap(),
        fs::read(source.path().join(label)).unwrap()
    );
    assert_eq!(fs::read(&backup).unwrap(), encrypted);
    assert_eq!(fingerprints(&preserved), before);
    assert!(fs::read_dir(active.path()).unwrap().next().is_none());
    let reopened = SessionStore::open(&restored_folder)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(reopened.id, id);
    assert_eq!(reopened.parts.len(), 2);
    let mut index = crate::library::LibraryIndex::open(&restored).unwrap();
    assert!(index
        .search("SYNTHETIC_MICROPHONE_TRANSCRIPT", 0, 20)
        .unwrap()
        .items
        .is_empty());
    index
        .upsert(&reopened, &Default::default(), "restored-synthetic")
        .unwrap();
    assert_eq!(
        index
            .search("SYNTHETIC_MICROPHONE_TRANSCRIPT", 0, 20)
            .unwrap()
            .total,
        1
    );
}

#[test]
fn backup_coexists_with_another_publisher_and_keeps_erasure_excluded() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let (id, folder) = fixture(source.path());
    let before = fingerprints(&folder);
    let passphrase = synthetic_passphrase();
    let stop = AtomicBool::new(false);
    let backup = destination.path().join("parallel-publication.hmvault");
    let publisher = source_publication_lease(source.path()).unwrap();

    let (summary, _) = create(source.path(), &id, &backup, passphrase.as_bytes(), &stop).unwrap();
    assert_eq!(summary.meeting_id, id);
    assert_eq!(fingerprints(&folder), before);
    drop(publisher);

    let erasure = publication_lease(source.path(), true).unwrap();
    let second = destination.path().join("blocked-by-erasure.hmvault");
    assert!(create(source.path(), &id, &second, passphrase.as_bytes(), &stop).is_err());
    assert!(!second.exists());
    drop(erasure);
}

#[test]
fn wrong_password_corruption_truncation_trailing_bytes_and_cancellation_publish_nothing() {
    let source = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let (id, folder) = fixture(source.path());
    let before = fingerprints(&folder);
    let backup = elsewhere.path().join("copy.hmvault");
    let ready = AtomicBool::new(false);
    let passphrase = synthetic_passphrase();
    let unlock_with_passphrase = Unlock::Passphrase(passphrase.as_bytes());
    let (_, key) = create(source.path(), &id, &backup, passphrase.as_bytes(), &ready).unwrap();
    let wrong = RecoveryKey::generate().unwrap();
    let wrong_passphrase = synthetic_passphrase();
    assert!(passphrase.as_bytes() != wrong_passphrase.as_bytes());
    let valid = fs::read(&backup).unwrap();
    assert!(recover(
        source.path(),
        &backup,
        destination.path(),
        Unlock::Passphrase(wrong_passphrase.as_bytes()),
        &ready
    )
    .is_err());
    assert!(recover(
        source.path(),
        &backup,
        destination.path(),
        Unlock::RecoveryKey(&wrong),
        &ready
    )
    .is_err());
    assert!(RecoveryKey::parse(&passphrase[..12]).is_err());
    for (name, data) in [
        ("tampered", {
            let mut bytes = valid.clone();
            let middle = bytes.len() / 2;
            bytes[middle] ^= 0x40;
            bytes
        }),
        ("truncated", valid[..valid.len() - 1].to_vec()),
        ("trailing", [valid.as_slice(), b"unexpected"].concat()),
        ("changed-password-slot", {
            let mut bytes = valid.clone();
            bytes[40] ^= 1;
            bytes
        }),
        ("changed-recovery-slot", {
            let mut bytes = valid.clone();
            bytes[frames::HEADER_LEN - 1] ^= 1;
            bytes
        }),
        ("duplicated-frame", {
            let end = frames::HEADER_LEN + 4 + frames::BLOCK + 16;
            [
                &valid[..end],
                &valid[frames::HEADER_LEN..end],
                &valid[end..],
            ]
            .concat()
        }),
    ] {
        let modified = elsewhere.path().join(name);
        fs::write(&modified, data).unwrap();
        for unlock in [unlock_with_passphrase, Unlock::RecoveryKey(&key)] {
            assert!(
                recover(source.path(), &modified, destination.path(), unlock, &ready).is_err(),
                "{name}"
            );
        }
    }
    let stop = AtomicBool::new(true);
    assert!(recover(
        source.path(),
        &backup,
        destination.path(),
        unlock_with_passphrase,
        &stop
    )
    .is_err());
    assert!(create(
        source.path(),
        &id,
        &destination.path().join("cancelled.hmvault"),
        passphrase.as_bytes(),
        &stop
    )
    .is_err());
    assert!(fs::read_dir(destination.path()).unwrap().next().is_none());
    assert_eq!(fingerprints(&folder), before);
    assert_eq!(fs::read(backup).unwrap(), valid);
}

#[test]
fn rejects_unsafe_authenticated_paths_and_output_inside_the_active_profile() {
    let source = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let (id, folder) = fixture(source.path());
    let ready = AtomicBool::new(false);
    let passphrase = synthetic_passphrase();
    assert!(create(
        source.path(),
        &id,
        &source.path().join("not-external.hmvault"),
        passphrase.as_bytes(),
        &ready
    )
    .is_err());
    let active = SessionStore::open(&folder).unwrap();
    let attempted = elsewhere.path().join("active.hmvault");
    assert!(create(
        source.path(),
        &id,
        &attempted,
        passphrase.as_bytes(),
        &ready
    )
    .is_err());
    assert!(!attempted.exists());
    drop(active);
    let malformed = Inventory {
        meeting_id: id,
        directories: vec!["session/../escape".into()],
        files: vec![],
        sidecars: vec![],
    };
    let path = elsewhere.path().join("malformed");
    seal_for_test(&path, &malformed, &[], passphrase.as_bytes());
    assert!(recover(
        source.path(),
        &path,
        elsewhere.path(),
        Unlock::Passphrase(passphrase.as_bytes()),
        &ready
    )
    .is_err());
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 1);
}

#[test]
fn failed_restore_after_a_plaintext_write_removes_only_its_own_stage() {
    let source = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let original = destination.path().join("existing-user-file");
    fs::write(&original, b"unrelated existing data").unwrap();
    let data = b"synthetic plain session data";
    let file = |name: &str, content: &[u8]| ArchivedFile {
        name: name.into(),
        bytes: content.len() as u64,
        sha256: format!("{:x}", Sha256::digest(content)),
    };
    let manifest = Inventory {
        meeting_id: "0123456789abcdef0123456789abcdef".into(),
        directories: vec!["session/collision/nested".into()],
        files: vec![
            file("session/session.json", data),
            file("session/collision", &[]),
        ],
        sidecars: vec![],
    };
    let backup = elsewhere.path().join("authenticated-conflict.hmvault");
    let passphrase = synthetic_passphrase();
    let unlock = Unlock::Passphrase(passphrase.as_bytes());
    seal_for_test(&backup, &manifest, data, passphrase.as_bytes());
    let stop = AtomicBool::new(false);
    assert_eq!(verify(&backup, unlock, &stop).unwrap().files, 2);
    assert!(recover(source.path(), &backup, destination.path(), unlock, &stop).is_err());
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
    assert_eq!(fs::read(&original).unwrap(), b"unrelated existing data");
}

#[test]
fn reads_an_independently_encoded_version_one_backup() {
    #[derive(Deserialize)]
    struct Vector {
        public_passphrase: String,
        archive_hex: Vec<String>,
    }
    // Node independently encoded this fixed v1 compatibility vector.
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sealed-v1.json");
    let vector: Vector = serde_json::from_reader(File::open(fixture_path).unwrap()).unwrap();
    let encoded = vector.archive_hex.concat();
    let encrypted: Vec<_> = (0..encoded.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16).unwrap())
        .collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.hmvault");
    fs::write(&path, encrypted).unwrap();
    let ready = AtomicBool::new(false);
    assert_eq!(
        verify(
            &path,
            Unlock::Passphrase(vector.public_passphrase.as_bytes()),
            &ready,
        )
        .unwrap()
        .files,
        1
    );
    let key = RecoveryKey::generate().unwrap();
    assert!(verify(&path, Unlock::RecoveryKey(&key), &ready).is_err());
}

#[test]
fn repeated_sealing_uses_a_fresh_salt_and_both_archives_reopen() {
    const SALT: std::ops::Range<usize> = 8..40;
    let passphrase = synthetic_passphrase();
    let recovery = RecoveryKey::generate().unwrap();
    let payload = b"synthetic repeatable plaintext";
    let seal = || {
        let mut archive = Vec::new();
        let mut writer =
            frames::Encrypt::new(&mut archive, passphrase.as_bytes(), &recovery).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
        archive
    };
    let first = seal();
    let second = seal();
    assert!(first[SALT] != second[SALT]);
    for archive in [&first, &second] {
        for unlock in [
            Unlock::Passphrase(passphrase.as_bytes()),
            Unlock::RecoveryKey(&recovery),
        ] {
            let mut recovered = Vec::new();
            frames::Decrypt::new(archive.as_slice(), unlock)
                .unwrap()
                .read_to_end(&mut recovered)
                .unwrap();
            assert_eq!(recovered, payload);
        }
    }
}

#[test]
fn transfer_stops_after_a_cancellation_observed_between_blocks() {
    struct Cancelling<'a>(&'a AtomicBool, usize);
    impl Write for Cancelling<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.store(true, Ordering::Release);
            self.1 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let cancelled = AtomicBool::new(false);
    let plaintext = vec![77u8; frames::BLOCK * 2];
    let entry = ArchivedFile {
        name: "session/synthetic".into(),
        bytes: plaintext.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&plaintext)),
    };
    let mut sink = Cancelling(&cancelled, 0);
    assert!(transfer(&mut plaintext.as_slice(), &mut sink, &entry, &cancelled).is_err());
    assert_eq!(sink.1, frames::BLOCK);
}
