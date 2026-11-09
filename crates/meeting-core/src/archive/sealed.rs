//! Portable encrypted copies of native sessions. The live store is never replaced.
mod frames;

use super::*;
use std::io::{self, BufReader, BufWriter, Seek};
use zeroize::Zeroizing;

const MAX_ENCRYPTED: u64 = MAX_TOTAL + MAX_MANIFEST + 32 * 1024 * 1024;
const CANCELLED: &str = "Vorgang abgebrochen. / Encrypted backup operation cancelled.";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Inventory {
    meeting_id: String,
    directories: Vec<String>,
    files: Vec<ArchivedFile>,
    sidecars: Vec<Sidecar>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub meeting_id: String,
    pub files: usize,
    pub bytes: u64,
}

pub struct RecoveryKey(Zeroizing<[u8; 32]>);

impl RecoveryKey {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Gib den 64-stelligen Wiederherstellungsschlüssel ein. / Enter the 64-character recovery key.".into());
        }
        let mut key = Zeroizing::new([0u8; 32]);
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[2 * index..2 * index + 2], 16)?;
        }
        Ok(Self(key))
    }

    pub fn encode(&self) -> Zeroizing<String> {
        use std::fmt::Write as _;
        let mut text = Zeroizing::new(String::with_capacity(64));
        for byte in self.0.iter() {
            let _ = write!(&mut *text, "{byte:02x}");
        }
        text
    }
}

#[derive(Clone, Copy)]
pub enum Unlock<'a> {
    Passphrase(&'a [u8]),
    RecoveryKey(&'a RecoveryKey),
}

impl Unlock<'_> {
    fn validate(self) -> Result<()> {
        match self {
            Self::Passphrase(password) => validate_password(password, false),
            Self::RecoveryKey(_) => Ok(()),
        }
    }
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(CANCELLED.into());
    }
    Ok(())
}

fn native_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}

fn validate_password(password: &[u8], creating: bool) -> Result<()> {
    if password.is_empty() || password.len() > 4096 {
        return Err("Ungültiges Kennwort. / Invalid passphrase.".into());
    }
    if creating && std::str::from_utf8(password)?.chars().count() < 12 {
        return Err("Wähle eine Passphrase mit mindestens 12 Zeichen. / Choose a passphrase with at least 12 characters.".into());
    }
    Ok(())
}

fn portable(name: &str) -> bool {
    name.len() <= 1024
        && name.split('/').count() <= 20
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
}

fn validate_inventory(manifest: &Inventory) -> Result<Summary> {
    if !native_id(&manifest.meeting_id)
        || manifest.files.len() > MAX_FILES
        || manifest.directories.len() > MAX_FILES
        || manifest.sidecars.len() > MAX_FILES
    {
        return Err("Invalid encrypted backup inventory".into());
    }
    let mut names = HashSet::new();
    let mut bytes = 0u64;
    for directory in &manifest.directories {
        if !safe_name(directory)
            || !portable(directory)
            || !names.insert(directory.to_ascii_lowercase())
        {
            return Err("Unsafe or duplicate directory in encrypted backup".into());
        }
    }
    let mut has_manifest = false;
    for item in &manifest.files {
        if !safe_name(&item.name)
            || !portable(&item.name)
            || item.name.eq_ignore_ascii_case("session/session.lock")
            || !names.insert(item.name.to_ascii_lowercase())
            || item.sha256.len() != 64
            || !item.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Unsafe or duplicate file in encrypted backup".into());
        }
        has_manifest |= matches!(
            item.name.as_str(),
            "session/session.json" | "session/session.json.bak"
        );
        bytes = bytes
            .checked_add(item.bytes)
            .ok_or("Encrypted backup size overflow")?;
    }
    let mut sidecar_bytes = 0usize;
    for item in &manifest.sidecars {
        if !sidecar_name(&item.name, &manifest.meeting_id)
            || !portable(&item.name)
            || !names.insert(item.name.to_ascii_lowercase())
            || format!("{:x}", Sha256::digest(&item.bytes)) != item.sha256
        {
            return Err("Invalid retained annotation in encrypted backup".into());
        }
        sidecar_bytes = sidecar_bytes
            .checked_add(item.bytes.len())
            .ok_or("Annotation size overflow")?;
    }
    bytes = bytes
        .checked_add(sidecar_bytes as u64)
        .ok_or("Encrypted backup size overflow")?;
    if !has_manifest || bytes > MAX_TOTAL || sidecar_bytes > 16 * 1024 * 1024 {
        return Err(
            "Encrypted backup is missing session metadata or exceeds its size limit".into(),
        );
    }
    Ok(Summary {
        meeting_id: manifest.meeting_id.clone(),
        files: manifest.files.len() + manifest.sidecars.len(),
        bytes,
    })
}

/// The selected destination is external to the live profile. Neither archive deletion
/// nor a later meeting erasure can silently find and remove such user-managed copies.
pub fn external_directory(root: &Path, directory: &Path) -> Result<PathBuf> {
    let root = metadata::checked_directory(root)?;
    let directory = metadata::checked_directory(directory)?;
    #[cfg(windows)]
    let inside = directory.ancestors().any(|ancestor| {
        ancestor.to_string_lossy().to_lowercase() == root.to_string_lossy().to_lowercase()
    });
    #[cfg(not(windows))]
    let inside = directory.starts_with(&root);
    if inside {
        return Err("Wähle einen Ordner außerhalb der aktiven Sitzungsablage. / Choose a folder outside the active meeting storage.".into());
    }
    Ok(directory)
}

/// Stream a verified inactive session directly into a new encrypted file.
pub fn create(
    root: &Path,
    meeting_id: &str,
    output: &Path,
    password: &[u8],
    cancel: &AtomicBool,
) -> Result<(Summary, RecoveryKey)> {
    validate_password(password, true)?;
    cancelled(cancel)?;
    if !native_id(meeting_id) || !output.is_absolute() {
        return Err("Invalid native session or encrypted backup destination".into());
    }
    let output = external_directory(root, output.parent().ok_or("Missing backup directory")?)?
        .join(output.file_name().ok_or("Missing backup name")?);
    if crate::storage::exists_no_link(&output)? {
        return Err("Die Zieldatei existiert bereits. / The destination already exists.".into());
    }
    let _archive = operation_lock(root)?;
    let _publication = publication_lease(root, true)?;
    let sessions = metadata::checked_directory(&root.join("sessions"))?;
    let folder = metadata::checked_directory(&sessions.join(meeting_id))?;
    let store = SessionStore::open(&folder)?;
    let data = store.snapshot()?;
    if data.id != meeting_id || data.read_only {
        return Err("Only native sessions with their local originals can be backed up here".into());
    }
    let mut paths = Vec::new();
    let mut directories = Vec::new();
    inventory(&folder, &folder, &mut paths, &mut directories)?;
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    directories.sort();
    let mut files = Vec::with_capacity(paths.len());
    let mut total = 0u64;
    for (name, path) in &paths {
        cancelled(cancel)?;
        total = total
            .checked_add(regular(path)?.len())
            .ok_or("Backup size overflow")?;
        if total > MAX_TOTAL {
            return Err("Encrypted backup exceeds its size limit".into());
        }
        let (bytes, sha256) = digest_cancellable(path, Some(cancel))?;
        files.push(ArchivedFile {
            name: name.clone(),
            bytes,
            sha256,
        });
    }
    let manifest = Inventory {
        meeting_id: meeting_id.into(),
        directories,
        files,
        sidecars: sidecars(root, meeting_id)?,
    };
    let summary = validate_inventory(&manifest)?;
    let encoded = Zeroizing::new(serde_json::to_vec(&manifest)?);
    if encoded.len() as u64 > MAX_MANIFEST {
        return Err("Encrypted backup manifest is too large".into());
    }
    let mut pending = tempfile::Builder::new()
        .prefix(".handy-sealed-")
        .tempfile_in(output.parent().ok_or("Missing backup directory")?)?;
    let recovery = RecoveryKey::generate()?;
    {
        let mut stream =
            frames::Encrypt::new(BufWriter::new(pending.as_file_mut()), password, &recovery)?;
        stream.write_all(&(encoded.len() as u32).to_be_bytes())?;
        stream.write_all(&encoded)?;
        for ((_, path), entry) in paths.iter().zip(&manifest.files) {
            regular(path)?;
            let mut input = BufReader::new(File::open(path)?);
            transfer(&mut input, &mut stream, entry, cancel)?;
            let mut extra = [0u8; 1];
            if input.read(&mut extra)? != 0 {
                return Err("Source changed while the encrypted copy was being created".into());
            }
        }
        stream.finish()?;
    }
    pending.as_file().sync_all()?;
    pending.as_file_mut().rewind()?;
    let (verified, fingerprint) = scan(
        pending.as_file_mut(),
        Unlock::Passphrase(password),
        None,
        cancel,
    )?;
    if verified.meeting_id != meeting_id || fingerprint[..] != Sha256::digest(&*encoded)[..] {
        return Err("Encrypted backup read-back did not match its source".into());
    }
    pending.as_file_mut().rewind()?;
    // The passphrase scan checked the whole stream. The first authenticated frame
    // independently proves that the recovery slot opens that same stream.
    frames::Decrypt::new(
        BufReader::new(pending.as_file_mut()),
        Unlock::RecoveryKey(&recovery),
    )?
    .read_exact(&mut [0u8; 4])?;
    cancelled(cancel)?;
    pending
        .persist_noclobber(&output)
        .map_err(|error| error.error)?;
    sync_directory(output.parent().ok_or("Missing backup directory")?)?;
    Ok((summary, recovery))
}

/// Authenticate a backup without writing any plaintext or accessing an original session.
pub fn verify(input: &Path, unlock: Unlock<'_>, cancel: &AtomicBool) -> Result<Summary> {
    unlock.validate()?;
    let mut input = open_backup(input)?;
    Ok(scan(&mut input, unlock, None, cancel)?.0)
}

/// Restore into a fresh profile outside the active store. It becomes visible as
/// a profile only after every file is authenticated, synced and reopened.
pub fn recover(
    active_root: &Path,
    input: &Path,
    directory: &Path,
    unlock: Unlock<'_>,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    unlock.validate()?;
    let directory = external_directory(active_root, directory)?;
    let mut input = open_backup(input)?;
    let (expected, digest) = scan(&mut input, unlock, None, cancel)?;
    input.rewind()?;
    cancelled(cancel)?;
    let stage = tempfile::Builder::new()
        .prefix(".handy-recovery-")
        .tempdir_in(&directory)?;
    let profile = stage.path().join("profile");
    fs::create_dir(&profile)?;
    let (_, actual_digest) = scan(&mut input, unlock, Some(&profile), cancel)?;
    if digest != actual_digest {
        return Err("The encrypted backup changed during recovery".into());
    }
    let restored = profile.join("sessions").join(&expected.meeting_id);
    let store = SessionStore::open(&restored)?;
    let data = store.snapshot()?;
    if data.id != expected.meeting_id || data.read_only {
        return Err("The restored native session did not reopen with the expected identity".into());
    }
    drop(store);
    sync_tree(&profile)?;
    // Coordinate lock files created by read-back are empty and excluded from the copy.
    cancelled(cancel)?;
    let destination = tempfile::Builder::new()
        .prefix("HandyMeeting-Recovery-")
        .tempdir_in(&directory)?;
    let published = destination.path().join("profile");
    fs::rename(&profile, &published)?;
    sync_directory(destination.path())?;
    sync_directory(&directory)?;
    let _ = destination.keep();
    Ok(published)
}

fn open_backup(path: &Path) -> Result<File> {
    let metadata = regular(path)?;
    if metadata.len() > MAX_ENCRYPTED {
        return Err("Encrypted backup exceeds its size limit".into());
    }
    Ok(File::open(path)?)
}

fn scan(
    file: &mut File,
    unlock: Unlock<'_>,
    restore: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<(Summary, [u8; 32])> {
    cancelled(cancel)?;
    let mut plaintext = frames::Decrypt::new(BufReader::new(file), unlock)?;
    let mut length = [0; 4];
    plaintext.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length);
    if u64::from(length) > MAX_MANIFEST || length == 0 {
        return Err("Invalid encrypted backup manifest length".into());
    }
    let mut encoded = Zeroizing::new(vec![0u8; length as usize]);
    for block in encoded.chunks_mut(frames::BLOCK) {
        cancelled(cancel)?;
        plaintext.read_exact(block)?;
    }
    let fingerprint = Sha256::digest(&*encoded).into();
    let manifest: Inventory = serde_json::from_slice(&encoded)?;
    let summary = validate_inventory(&manifest)?;
    let session = restore.map(|root| root.join("sessions").join(&manifest.meeting_id));
    if let (Some(root), Some(folder)) = (restore, &session) {
        safe_directories(root, folder.strip_prefix(root)?)?;
        for directory in &manifest.directories {
            cancelled(cancel)?;
            safe_directories(
                folder,
                Path::new(
                    directory
                        .strip_prefix("session/")
                        .ok_or("Unsafe restore path")?,
                ),
            )?;
        }
    }
    for entry in &manifest.files {
        cancelled(cancel)?;
        if let Some(folder) = &session {
            let relative = Path::new(
                entry
                    .name
                    .strip_prefix("session/")
                    .ok_or("Unsafe restore path")?,
            );
            let parent = relative.parent().ok_or("Invalid restore parent")?;
            safe_directories(folder, parent)?;
            let path = folder.join(relative);
            let output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            let mut output = BufWriter::new(output);
            transfer(&mut plaintext, &mut output, entry, cancel)?;
            output.flush()?;
            output.get_ref().sync_all()?;
            drop(output);
            let (length, digest) = digest_cancellable(&path, Some(cancel))?;
            if length != entry.bytes || digest != entry.sha256 {
                return Err("Restored file did not match its authenticated digest on disk".into());
            }
            sync_directory(&folder.join(parent))?;
        } else {
            transfer(&mut plaintext, &mut io::sink(), entry, cancel)?;
        }
    }
    if plaintext.read(&mut [0u8; 1])? != 0 {
        return Err("Unexpected extra data in encrypted backup".into());
    }
    if let Some(root) = restore {
        for sidecar in &manifest.sidecars {
            cancelled(cancel)?;
            let relative = Path::new(&sidecar.name);
            let parent = relative.parent().ok_or("Invalid annotation parent")?;
            safe_directories(root, parent)?;
            let path = root.join(relative);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            output.write_all(&sidecar.bytes)?;
            output.sync_all()?;
            drop(output);
            let (length, digest) = digest_cancellable(&path, Some(cancel))?;
            if length != sidecar.bytes.len() as u64 || digest != sidecar.sha256 {
                return Err(
                    "Restored annotation did not match its authenticated digest on disk".into(),
                );
            }
            sync_directory(&root.join(parent))?;
        }
        sync_directory(root)?;
    }
    Ok((summary, fingerprint))
}

fn transfer(
    source: &mut impl Read,
    destination: &mut impl Write,
    entry: &ArchivedFile,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut block = Zeroizing::new(vec![0u8; entry.bytes.min(frames::BLOCK as u64) as usize]);
    let mut remaining = entry.bytes;
    let mut hash = Sha256::new();
    while remaining > 0 {
        cancelled(cancel)?;
        let count = remaining.min(block.len() as u64) as usize;
        source.read_exact(&mut block[..count])?;
        destination.write_all(&block[..count])?;
        hash.update(&block[..count]);
        remaining -= count as u64;
    }
    if format!("{:x}", hash.finalize()) != entry.sha256 {
        return Err("Source or restored file differs from its authenticated digest".into());
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn sync_tree(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        metadata::checked_directory(path)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            reject_link(&entry.path())?;
            if entry.file_type()?.is_dir() {
                sync_tree(&entry.path())?;
            }
        }
        sync_directory(path)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests;
