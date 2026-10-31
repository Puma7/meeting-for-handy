//! User edits are separate from original capture manifests and transcript journals.
use crate::{
    storage::{atomic_json, reject_link},
    Result,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
thread_local! {
    static LOCK_RETRY_OBSERVER: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
/// Observes the first actual OS-lock retry on this test thread. The scoped
/// restoration also runs on panic; unrelated concurrent tests cannot consume it.
pub(crate) fn observe_lock_retry<T>(
    observer: impl FnOnce() + 'static,
    work: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Box<dyn FnOnce()>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            LOCK_RETRY_OBSERVER.with(|slot| slot.replace(self.0.take()));
        }
    }
    let _restore = Restore(LOCK_RETRY_OBSERVER.with(|slot| slot.replace(Some(Box::new(observer)))));
    work()
}

/// Annotation and maintenance locks are short-lived. A child process started elsewhere in
/// this app can briefly inherit an open lock handle between fork and exec, so retry for a
/// moment before reporting a real concurrent edit.
pub(crate) fn lock_soon(file: &fs::File) -> std::io::Result<()> {
    let mut attempts = 0;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if attempts >= 25 => return Err(error),
            Err(_) => {
                #[cfg(test)]
                LOCK_RETRY_OBSERVER.with(|slot| {
                    let observer = slot.borrow_mut().take();
                    if let Some(observer) = observer {
                        observer();
                    }
                });
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

/// Rejects any link at `path`, including a dangling one that `Path::exists` reports as absent.
/// A missing path is accepted so the caller may create it.
pub(crate) fn reject_link_if_present(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => reject_link(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Opens or creates a lock file without following a link, not even a dangling one, so no
/// file is ever created or locked outside the storage folder.
pub(crate) fn open_lock_file(path: &Path) -> Result<fs::File> {
    reject_link_if_present(path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    // A link swapped in between the check and the open must not be used as a lock.
    reject_link(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        let current = fs::symlink_metadata(path)?;
        if opened.dev() != current.dev() || opened.ino() != current.ino() {
            return Err("Lock file changed while opening; existing files retained".into());
        }
    }
    Ok(file)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub revision: u64,
    pub title: Option<String>,
    pub pinned: bool,
    #[serde(default)]
    pub audio_removed: bool,
}
pub fn valid_id(id: &str) -> bool {
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        || (id.len() == 23
            && id.starts_with("legacy-")
            && id[7..].bytes().all(|b| b.is_ascii_hexdigit()))
}
pub fn checked_directory(path: &Path) -> Result<PathBuf> {
    reject_link(path)?;
    if !path.is_dir() {
        return Err("Expected an ordinary directory".into());
    }
    Ok(path.canonicalize()?)
}
pub(crate) fn directory(root: &Path) -> Result<PathBuf> {
    checked_directory(root)?;
    let path = root.join("user-metadata");
    match fs::create_dir(&path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let checked = checked_directory(&path)?;
    if checked.parent() != Some(root.canonicalize()?.as_path()) {
        return Err("Metadata directory escaped storage".into());
    }
    Ok(checked)
}
pub fn read(root: &Path, id: &str) -> Result<Metadata> {
    if !valid_id(id) {
        return Err("Invalid session identifier".into());
    }
    let folder = root.join("user-metadata");
    if !folder.exists() {
        return Ok(Metadata::default());
    }
    let folder = checked_directory(&folder)?;
    let path = folder.join(format!("{id}.json"));
    // A dangling link is not an absent file: it must never read as unpinned defaults.
    reject_link_if_present(&path)?;
    if !path.exists() {
        let history = folder.join(format!("{id}-history"));
        if history.exists() {
            checked_directory(&history)?;
            if fs::read_dir(history)?.next().is_some() {
                return Err("Current meeting metadata is missing but previous revisions exist; protection state must be recovered explicitly".into());
            }
        }
        return Ok(Metadata::default());
    }
    reject_link(&path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("Oversized session metadata; retained unchanged".into());
    }
    let data: Metadata = serde_json::from_slice(&bytes)?;
    validate_title(data.title.as_deref())?;
    Ok(data)
}
fn validate_title(title: Option<&str>) -> Result<()> {
    if let Some(title) = title {
        if title.trim().is_empty()
            || title.chars().count() > 200
            || title.chars().any(char::is_control)
        {
            return Err("Use a nonempty meeting title with at most 200 characters".into());
        }
    }
    Ok(())
}
pub fn edit(
    root: &Path,
    id: &str,
    expected: u64,
    title: Option<String>,
    pinned: bool,
) -> Result<Metadata> {
    if !valid_id(id) {
        return Err("Invalid session identifier".into());
    }
    validate_title(title.as_deref())?;
    let folder = directory(root)?;
    let lock = open_lock_file(&folder.join(format!("{id}.lock")))?;
    lock_soon(&lock).map_err(|_| "Meeting metadata is being edited in another window")?;
    let old = read(root, id)?;
    if old.revision != expected {
        return Err("Meeting metadata changed; reload before saving".into());
    }
    let next = Metadata {
        revision: old
            .revision
            .checked_add(1)
            .ok_or("Metadata revision overflow")?,
        title: title.map(|s| s.trim().to_owned()),
        pinned,
        audio_removed: old.audio_removed,
    };
    let history = folder.join(format!("{id}-history"));
    match fs::create_dir(&history) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    checked_directory(&history)?;
    let archive = history.join(format!(
        "{:020}-{}.json",
        old.revision,
        uuid::Uuid::new_v4().simple()
    ));
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(archive)?;
    output.write_all(&serde_json::to_vec_pretty(&old)?)?;
    output.sync_all()?;
    atomic_json(&folder.join(format!("{id}.json")), &next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "0123456789abcdef0123456789abcdef";
    #[test]
    fn edits_are_versioned_and_never_touch_original_session_files() {
        let root = tempfile::tempdir().unwrap();
        let session = root.path().join("session.json");
        fs::write(&session, b"original capture metadata").unwrap();
        let a = edit(root.path(), ID, 0, Some("Renamed meeting".into()), true).unwrap();
        assert_eq!(a.revision, 1);
        assert!(a.pinned);
        assert_eq!(fs::read(session).unwrap(), b"original capture metadata");
        assert!(edit(root.path(), ID, 0, Some("Stale update".into()), false).is_err());
        assert!(read(root.path(), ID).unwrap().pinned);
        assert_eq!(
            fs::read_dir(
                root.path()
                    .join("user-metadata")
                    .join(format!("{ID}-history"))
            )
            .unwrap()
            .count(),
            1
        );
    }
    #[test]
    fn corrupt_pin_state_is_never_treated_as_unpinned() {
        let root = tempfile::tempdir().unwrap();
        let dir = directory(root.path()).unwrap();
        let file = dir.join(format!("{ID}.json"));
        fs::write(&file, b"broken retained pin state").unwrap();
        assert!(read(root.path(), ID).is_err());
        assert!(edit(root.path(), ID, 0, None, false).is_err());
        assert_eq!(fs::read(file).unwrap(), b"broken retained pin state");
        assert!(read(root.path(), "../other").is_err());
    }
    #[test]
    fn missing_current_pin_state_with_retained_history_cannot_reset_protection() {
        let root = tempfile::tempdir().unwrap();
        edit(root.path(), ID, 0, None, true).unwrap();
        let path = directory(root.path()).unwrap().join(format!("{ID}.json"));
        fs::remove_file(&path).unwrap();
        assert!(read(root.path(), ID).is_err());
        assert!(edit(root.path(), ID, 0, None, false).is_err());
        assert!(!path.exists());
    }
    #[cfg(unix)]
    #[test]
    fn dangling_lock_or_label_links_are_rejected_without_creating_their_target() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dir = directory(root.path()).unwrap();
        let target = outside.path().join("created-through-link");
        std::os::unix::fs::symlink(&target, dir.join(format!("{ID}.lock"))).unwrap();
        assert!(!dir.join(format!("{ID}.lock")).exists(), "the link dangles");
        assert!(edit(root.path(), ID, 0, Some("Title".into()), true).is_err());
        assert!(!target.exists(), "no file is created through the link");
        assert!(!dir.join(format!("{ID}.json")).exists());
        fs::remove_file(dir.join(format!("{ID}.lock"))).unwrap();
        // A dangling current label link is not an absent, unpinned label file.
        std::os::unix::fs::symlink(&target, dir.join(format!("{ID}.json"))).unwrap();
        assert!(read(root.path(), ID).is_err());
        assert!(edit(root.path(), ID, 0, None, false).is_err());
        assert!(!target.exists());
        assert!(fs::symlink_metadata(dir.join(format!("{ID}.json")))
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
