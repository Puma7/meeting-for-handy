//! Owns an unlocked encrypted volume and prevents writes to its mountpoint after detachment.
use super::{
    checked_root, copy_profile, metadata, migration_paths, reject_link, verify_marked, Protection,
    Receipt, DIRECTORY_PREFIX,
};
use crate::Result;
use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[cfg(target_os = "linux")]
#[path = "volume/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "volume/macos.rs"]
mod platform;

const VAULT_MARKER: &str = "protected-volume.v1";
const VAULT_MARKER_CONTENT: &[u8] = b"HandyMeeting encrypted volume v1\n";
const PROFILE_MARKER_CONTENT: &[u8] = b"HandyMeeting encrypted volume profile v1\n";
const UNAVAILABLE: &str = "The encrypted volume provider is unavailable. / Die Verschlüsselung für dieses Profil ist nicht verfügbar.";
const INVALID_VOLUME: &str = "The protected volume is incomplete, disconnected or belongs to another profile. / Das geschützte Profil ist unvollständig, getrennt oder gehört zu einer anderen Ablage.";
const BUSY: &str = "The encrypted volume is already in use. Close its other window before opening or copying it. / Das verschlüsselte Profil ist bereits in Gebrauch. Bitte vor dem Öffnen oder Kopieren das andere Fenster schließen.";

pub(super) fn available() -> bool {
    platform::available()
}

fn validate_password(password: &str, creating: bool) -> Result<()> {
    if password.is_empty()
        || password.len() > 1024
        || password.contains(['\0', '\r', '\n'])
        || (creating && password.chars().count() < 12)
    {
        return Err("Use a passphrase of at least 12 characters when creating a profile, without line breaks and at most 1024 bytes. / Beim Erstellen mindestens zwölf Zeichen verwenden, ohne Zeilenumbrüche und mit höchstens 1024 Bytes.".into());
    }
    Ok(())
}

struct MountedProtection(u64);

impl Protection for MountedProtection {
    fn enable_directory(&self, directory: &Path) -> Result<()> {
        self.check(directory)
    }

    fn check(&self, path: &Path) -> Result<()> {
        reject_link(path)?;
        if fs::symlink_metadata(path)?.dev() != self.0 {
            return Err(INVALID_VOLUME.into());
        }
        Ok(())
    }
}

/// Hold this object for the lifetime of the desktop profile and its workers.
pub struct MountedVolume {
    root: PathBuf,
    mount: PathBuf,
    anchor: PathBuf,
    attachment: Option<platform::Attachment>,
    hold: Option<File>,
    protection: MountedProtection,
    _use_lock: File,
}

impl MountedVolume {
    fn attach(vault: &Path, password: &str, use_lock: File) -> Result<Self> {
        let temp = checked_root(&std::env::temp_dir())?;
        if temp.starts_with(vault) {
            return Err("The system temporary directory is inside the vault. / Der temporäre Systemordner liegt im verschlüsselten Speicher.".into());
        }
        let anchor = tempfile::Builder::new()
            .prefix("handymeeting-mount-")
            .suffix(".noindex")
            .tempdir_in(temp)?
            .keep();
        let mount = anchor.join("volume");
        // Never recursively delete a mount directory. A failed attach can leave a live mount.
        let prepared = (|| -> Result<File> {
            fs::set_permissions(&anchor, Permissions::from_mode(0o700))?;
            fs::create_dir(&mount)?;
            fs::set_permissions(&mount, Permissions::from_mode(0o700))?;
            Ok(File::open(&mount)?)
        })();
        let underlying = match prepared {
            Ok(file) => file,
            Err(error) => {
                remove_empty_anchor(&anchor, &mount);
                return Err(error);
            }
        };
        let attachment = match platform::attach(vault, &mount, password) {
            Ok(attachment) => attachment,
            Err(error) => {
                // If the provider is unable to prove it detached, still deny fallback writes.
                let _ = underlying.set_permissions(Permissions::from_mode(0o000));
                remove_empty_anchor(&anchor, &mount);
                return Err(error);
            }
        };
        let mut volume = Self {
            root: mount.join("profile"),
            mount,
            anchor,
            attachment: Some(attachment),
            hold: None,
            protection: MountedProtection(0),
            _use_lock: use_lock,
        };
        underlying.set_permissions(Permissions::from_mode(0o000))?;
        volume.check_attachment()?;
        let held = File::open(&volume.mount)?;
        let mounted_device = held.metadata()?.dev();
        if mounted_device == underlying.metadata()?.dev() {
            return Err(INVALID_VOLUME.into());
        }
        volume.protection = MountedProtection(mounted_device);
        volume.hold = Some(held);
        Ok(volume)
    }

    fn check_attachment(&self) -> Result<()> {
        self.attachment
            .as_ref()
            .ok_or(INVALID_VOLUME)?
            .check(&self.mount)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn verify(&self) -> Result<()> {
        self.check_attachment()?;
        verify_marked(&self.root, &self.protection, PROFILE_MARKER_CONTENT)
    }

    fn close(&mut self) -> Result<()> {
        if let Some(attachment) = &mut self.attachment {
            drop(self.hold.take());
            attachment.detach(&self.mount)?;
            self.attachment = None;
            remove_empty_anchor(&self.anchor, &self.mount);
        }
        Ok(())
    }

    pub fn unmount(mut self) -> Result<()> {
        self.close()
    }
}

impl Drop for MountedVolume {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn remove_empty_anchor(anchor: &Path, mount: &Path) {
    let _ = fs::remove_dir(mount);
    let _ = fs::remove_dir(anchor);
}

fn reserve_vault(vault: &Path) -> Result<File> {
    let lock = metadata::open_lock_file(&vault.join("vault-use.lock"))?;
    metadata::lock_soon(&lock).map_err(|_| BUSY)?;
    Ok(lock)
}

fn verify_vault_marker(vault: &Path) -> Result<()> {
    let path = vault.join(VAULT_MARKER);
    reject_link(&path).map_err(|_| INVALID_VOLUME)?;
    let mut marker = Vec::new();
    File::open(path)?
        .take(VAULT_MARKER_CONTENT.len() as u64 + 1)
        .read_to_end(&mut marker)?;
    if marker != VAULT_MARKER_CONTENT {
        return Err(INVALID_VOLUME.into());
    }
    Ok(())
}

pub fn migrate_volume(source: &Path, parent: &Path, password: &str) -> Result<Receipt> {
    validate_password(password, true)?;
    if !available() {
        return Err(UNAVAILABLE.into());
    }
    let (source, parent) = migration_paths(source, parent)?;
    let vault = tempfile::Builder::new()
        .prefix(DIRECTORY_PREFIX)
        .tempdir_in(parent)?
        .keep();
    fs::set_permissions(&vault, Permissions::from_mode(0o700))?;
    let use_lock = reserve_vault(&vault)?;
    platform::create(&vault, password)?;
    let mounted = MountedVolume::attach(&vault, password, use_lock)?;
    fs::create_dir(mounted.root())?;
    let (files, bytes) = copy_profile(
        &source,
        mounted.root(),
        &mounted.protection,
        PROFILE_MARKER_CONTENT,
    )?;
    mounted.verify()?;
    // Publish the outer marker only after all writes and a normal unmount succeed.
    mounted.unmount()?;
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(vault.join(VAULT_MARKER))?;
    marker.write_all(VAULT_MARKER_CONTENT)?;
    marker.sync_all()?;
    File::open(&vault)?.sync_all()?;
    Ok(Receipt {
        path: vault,
        files,
        bytes,
    })
}

pub fn open_volume(vault: &Path, password: &str) -> Result<MountedVolume> {
    validate_password(password, false)?;
    if !available() {
        return Err(UNAVAILABLE.into());
    }
    let vault = checked_root(vault).map_err(|_| INVALID_VOLUME)?;
    verify_vault_marker(&vault)?;
    let lock = reserve_vault(&vault)?;
    let mounted = MountedVolume::attach(&vault, password, lock)?;
    mounted.verify()?;
    Ok(mounted)
}
