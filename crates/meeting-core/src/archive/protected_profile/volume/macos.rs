use crate::{storage, Result};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const HDIUTIL: &str = "/usr/bin/hdiutil";
const PLUTIL: &str = "/usr/bin/plutil";
const ID: &str = "/usr/bin/id";
const IMAGE_NAME: &str = "profile.sparsebundle";
const CREATE_FAILED: &str = "Could not create the encrypted macOS profile. Check the available disk space and folder access. / Das verschlüsselte macOS-Profil konnte nicht erstellt werden. Bitte freien Speicherplatz und Ordnerzugriff prüfen.";
const OPEN_FAILED: &str = "Could not open the encrypted macOS profile. Check the password and the disk image. / Das verschlüsselte macOS-Profil konnte nicht geöffnet werden. Bitte Passwort und Laufwerksabbild prüfen.";
const ALREADY_OPEN: &str = "This encrypted disk image is already open. Eject it in macOS before opening the profile. / Dieses verschlüsselte Laufwerksabbild ist bereits geöffnet. Bitte in macOS auswerfen und das Profil erneut öffnen.";
const MOUNT_CHANGED: &str = "The encrypted macOS profile is no longer mounted at its expected location. / Das verschlüsselte macOS-Profil ist nicht mehr am erwarteten Ort eingebunden.";
const EJECT_FAILED: &str = "Could not eject the encrypted macOS profile. Close any files open from this profile and retry. / Das verschlüsselte macOS-Profil konnte nicht ausgeworfen werden. Bitte dort geöffnete Dateien schließen und erneut versuchen.";

pub(super) fn available() -> bool {
    Path::new(HDIUTIL).is_file() && Path::new(PLUTIL).is_file() && non_root_user()
}

fn non_root_user() -> bool {
    // The unmounted directory's permissions are only a write barrier for ordinary users.
    Command::new(ID)
        .arg("-u")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|uid| uid.trim().parse::<u32>().ok())
        .is_some_and(|uid| uid != 0)
}

pub(super) fn create(vault: &Path, password: &str) -> Result<()> {
    let image = vault.join(IMAGE_NAME);
    if storage::exists_no_link(&image)? {
        return Err(CREATE_FAILED.into());
    }
    let mut command = Command::new(HDIUTIL);
    command
        .args([
            "create",
            "-type",
            "SPARSEBUNDLE",
            "-size",
            "1t",
            "-fs",
            "APFS",
            "-volname",
            "HandyMeeting",
            "-mode",
            "700",
            "-encryption",
            "AES-256",
            "-stdinpass",
        ])
        .arg(&image)
        .stdout(Stdio::null());
    if !with_password(command, password).map_err(|_| CREATE_FAILED)? {
        return Err(CREATE_FAILED.into());
    }
    storage::reject_link(&image)?;
    if !image.is_dir() {
        return Err(CREATE_FAILED.into());
    }
    Ok(())
}

pub(super) fn attach(vault: &Path, mount: &Path, password: &str) -> Result<Attachment> {
    let image = vault.join(IMAGE_NAME);
    let image_identity = directory_identity(&image)?;
    let image = image.canonicalize()?;
    let underlying = directory_identity(mount)?;
    if mount.canonicalize()? != mount || !is_password_usable(password) {
        return Err(OPEN_FAILED.into());
    }
    if find_image(&image, image_identity, mount)?.is_some() {
        return Err(ALREADY_OPEN.into());
    }

    let mut command = Command::new(HDIUTIL);
    command
        .args([
            "attach",
            "-stdinpass",
            "-nobrowse",
            "-noautoopen",
            "-owners",
            "off",
            "-mountpoint",
        ])
        .arg(mount)
        .arg(&image)
        .stdout(Stdio::null());
    let attached = with_password(command, password).unwrap_or(false);
    // Check the backing image even after an error in case the OS mounted it before returning.
    let Some(ListedImage {
        encrypted,
        device: Some(device),
    }) = find_image(&image, image_identity, mount)?
    else {
        return Err(OPEN_FAILED.into());
    };
    let mounted = directory_identity(mount).ok().filter(|mounted| {
        attached
            && encrypted
            && mounted.device != underlying.device
            && directory_identity(&image).ok() == Some(image_identity)
    });
    if let Some(mounted) = mounted {
        if let Err(error) = exclude_from_spotlight(mount) {
            eject(mount)?;
            return Err(error);
        }
        return Ok(Attachment {
            image,
            image_identity,
            mount: mount.to_owned(),
            mounted,
            device,
        });
    }
    // This association was absent before the command and belongs to our private mount.
    eject(mount)?;
    Err(OPEN_FAILED.into())
}

fn exclude_from_spotlight(mount: &Path) -> Result<()> {
    // Add the volume's exclusion marker before the first session write or search index rebuild.
    let marker = mount.join(".metadata_never_index");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(file) => file.sync_all()?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(marker)?.is_file() {
                return Err(OPEN_FAILED.into());
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(super) struct Attachment {
    image: PathBuf,
    image_identity: DirectoryIdentity,
    mount: PathBuf,
    mounted: DirectoryIdentity,
    device: String,
}

impl Attachment {
    pub(super) fn check(&self, mount: &Path) -> Result<()> {
        if mount != self.mount || directory_identity(mount).ok() != Some(self.mounted) {
            return Err(MOUNT_CHANGED.into());
        }
        Ok(())
    }

    pub(super) fn detach(&mut self, mount: &Path) -> Result<()> {
        if mount != self.mount {
            return Err(MOUNT_CHANGED.into());
        }
        let Some(listed) = find_image(&self.image, self.image_identity, mount)? else {
            if directory_identity(mount).ok() == Some(self.mounted) {
                return Err(MOUNT_CHANGED.into());
            }
            return Ok(());
        };
        self.check(mount)?;
        if listed.device.as_ref() != Some(&self.device) || !listed.encrypted {
            return Err(MOUNT_CHANGED.into());
        }
        eject(mount)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

fn directory_identity(path: &Path) -> Result<DirectoryIdentity> {
    storage::reject_link(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(MOUNT_CHANGED.into());
    }
    Ok(DirectoryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

struct ListedImage {
    encrypted: bool,
    device: Option<String>,
}

fn find_image(
    image: &Path,
    identity: DirectoryIdentity,
    mount: &Path,
) -> Result<Option<ListedImage>> {
    let output = Command::new(HDIUTIL)
        .args(["info", "-plist"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| MOUNT_CHANGED)?;
    if !output.status.success() || output.stdout.len() > 4 * 1024 * 1024 {
        return Err(MOUNT_CHANGED.into());
    }
    listed_image(&output.stdout, image, identity, mount)
}

fn listed_image(
    plist: &[u8],
    image: &Path,
    identity: DirectoryIdentity,
    mount: &Path,
) -> Result<Option<ListedImage>> {
    let mut result = None;
    for index in 0..array_count(plist, "images", 256)? {
        let key = format!("images.{index}");
        let Some(path) = raw_plist_value(plist, &format!("{key}.image-path"), "string")? else {
            continue;
        };
        let path = Path::new(&path);
        if path != image && directory_identity(path).ok() != Some(identity) {
            continue;
        }
        if result.is_some() {
            return Err(MOUNT_CHANGED.into());
        }
        let encrypted = raw_plist_value(plist, &format!("{key}.image-encrypted"), "bool")?
            .is_some_and(|value| value == "true");
        let mut device = None;
        let key = format!("{key}.system-entities");
        for index in 0..array_count(plist, &key, 64)? {
            let entity = format!("{key}.{index}");
            let point = raw_plist_value(plist, &format!("{entity}.mount-point"), "string")?;
            if point.as_deref().map(Path::new) != Some(mount) {
                continue;
            }
            if device.is_some() {
                return Err(MOUNT_CHANGED.into());
            }
            let entry = raw_plist_value(plist, &format!("{entity}.dev-entry"), "string")?
                .ok_or(MOUNT_CHANGED)?;
            if !entry.starts_with("/dev/disk") {
                return Err(MOUNT_CHANGED.into());
            }
            device = Some(entry);
        }
        result = Some(ListedImage { encrypted, device });
    }
    Ok(result)
}

fn array_count(plist: &[u8], key: &str, limit: usize) -> Result<usize> {
    let count = raw_plist_value(plist, key, "array")?
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count <= limit)
        .ok_or(MOUNT_CHANGED)?;
    Ok(count)
}

fn raw_plist_value(plist: &[u8], key: &str, kind: &str) -> Result<Option<String>> {
    // The full plist can contain binary alias data. Extract typed values without a JSON conversion.
    let mut command = Command::new(PLUTIL);
    command
        .args(["-extract", key, "raw", "-expect", kind, "-n", "--", "-"])
        .stdout(Stdio::piped());
    let output = with_input(command, |input| input.write_all(plist)).map_err(|_| MOUNT_CHANGED)?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8(output.stdout).map_err(|_| MOUNT_CHANGED)?,
    ))
}

fn eject(mount: &Path) -> Result<()> {
    let status = Command::new(HDIUTIL)
        .arg("detach")
        .arg(mount)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| EJECT_FAILED)?;
    if !status.success() {
        return Err(EJECT_FAILED.into());
    }
    Ok(())
}

fn is_password_usable(password: &str) -> bool {
    !password.is_empty() && !password.as_bytes().contains(&0)
}

fn with_password(command: Command, password: &str) -> io::Result<bool> {
    if !is_password_usable(password) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let output = with_input(command, |input| {
        input.write_all(password.as_bytes())?;
        input.write_all(&[0])
    })?;
    Ok(output.status.success())
}

fn with_input(
    mut command: Command,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let written = write(&mut child.stdin.take().ok_or(io::ErrorKind::BrokenPipe)?);
    // Always reap the system helper, including when it closes stdin after an error.
    let output = child.wait_with_output()?;
    written?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTACHED_IMAGES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>images</key><array>
<dict>
  <key>image-path</key><string>/fixture/other.sparsebundle</string>
  <key>image-encrypted</key><false/>
  <key>system-entities</key><array><dict>
    <key>dev-entry</key><string>/dev/disk5s1</string>
    <key>mount-point</key><string>/fixture/foreign</string>
  </dict></array>
</dict>
<dict>
  <key>image-path</key><string>/fixture/Meeting &amp; notes/profile.sparsebundle</string>
  <key>image-alias</key><data>YWxpYXM=</data>
  <key>image-encrypted</key><true/>
  <key>system-entities</key><array>
    <dict><key>dev-entry</key><string>/dev/disk6</string></dict>
    <dict>
      <key>dev-entry</key><string>/dev/disk7s1</string>
      <key>mount-point</key><string>/fixture/private mount</string>
    </dict>
  </array>
</dict>
</array></dict></plist>"#;

    #[test]
    fn mac_image_listing_requires_exact_backing_image_and_mount() {
        let image = Path::new("/fixture/Meeting & notes/profile.sparsebundle");
        let mount = Path::new("/fixture/private mount");
        let foreign = Path::new("/fixture/foreign");
        let identity = DirectoryIdentity {
            device: 0,
            inode: 0,
        };
        let own = listed_image(ATTACHED_IMAGES, image, identity, mount)
            .unwrap()
            .unwrap();
        assert!(own.encrypted);
        assert_eq!(own.device.as_deref(), Some("/dev/disk7s1"));
        let elsewhere = listed_image(ATTACHED_IMAGES, image, identity, foreign)
            .unwrap()
            .unwrap();
        assert!(elsewhere.device.is_none());
        assert!(listed_image(ATTACHED_IMAGES, foreign, identity, mount)
            .unwrap()
            .is_none());
        let unencrypted = listed_image(
            ATTACHED_IMAGES,
            Path::new("/fixture/other.sparsebundle"),
            identity,
            foreign,
        )
        .unwrap()
        .unwrap();
        assert!(!unencrypted.encrypted);
    }
}
