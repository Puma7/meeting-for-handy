//! Explicit official-release downloads. No automatic installation or shell command strings.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
const LATEST: &str = "https://api.github.com/repos/Puma7/meeting-for-handy/releases/latest";
const MAX_INSTALLER: u64 = 1024 * 1024 * 1024;
const RELEASES: &str = "https://github.com/Puma7/meeting-for-handy/releases";
/// Release asset prefixes, preferred first: the branded name, then the name used up to 1.0.6.
const ASSET_PREFIXES: [&str; 2] = ["MeetingForHandy-", "HandyMeeting-"];
/// Unfinished downloads never carry an executable extension.
const PARTIAL_SUFFIX: &str = "-setup.partial";
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct View {
    pub status: String,
    pub version: Option<String>,
    pub release_url: Option<String>,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    /// Increases with every check or download, so the UI can scope a dismissal to one attempt.
    pub attempt: u64,
    /// Operating system of this build ("windows", "linux", "macos", ...).
    pub platform: String,
    /// Official release asset for this platform, shown when no in-app download exists.
    pub asset_name: Option<String>,
    /// Only a verified Windows setup can be downloaded inside the app.
    pub can_download: bool,
}
#[derive(Clone, Copy)]
struct Platform {
    os: &'static str,
    suffix: Option<&'static str>,
    downloads: bool,
}
fn platform() -> Platform {
    let os = std::env::consts::OS;
    let suffix = match (os, std::env::consts::ARCH) {
        ("windows", _) => Some("windows-x64-setup.exe"),
        ("linux", "x86_64") => Some("linux-amd64.deb"),
        ("macos", "aarch64") => Some("osx-arm64.dmg"),
        ("macos", "x86_64") => Some("osx-x64.dmg"),
        _ => None,
    };
    Platform {
        os,
        suffix,
        downloads: os == "windows",
    }
}
#[derive(Clone)]
struct Installer {
    url: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone)]
struct Candidate {
    version: String,
    asset_name: Option<String>,
    installer: Option<Installer>,
}
#[derive(Default)]
struct Inner {
    view: View,
    candidate: Option<Candidate>,
    ready: Option<PathBuf>,
    started: Option<Instant>,
}
#[derive(Default)]
pub(super) struct Updater {
    inner: Mutex<Inner>,
    cancel: AtomicBool,
}
pub(super) struct InstallerLease {
    pub path: PathBuf,
    _file: fs::File,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    html_url: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}
fn version(value: &str) -> Result<semver::Version> {
    semver::Version::parse(value).map_err(|_| "Unsupported release version".into())
}
fn candidate(release: Release, current: &str, platform: Platform) -> Result<Option<Candidate>> {
    let tag = release
        .tag_name
        .strip_prefix('v')
        .ok_or("Unexpected release tag")?;
    let target = version(tag)?;
    // The installed app may be a beta, but this channel only offers stable tags.
    // Build metadata has no SemVer precedence and must never create an upgrade.
    let newer = target.cmp_precedence(&version(current)?) == std::cmp::Ordering::Greater;
    if release.draft
        || release.prerelease
        || !target.pre.is_empty()
        || !target.build.is_empty()
        || release.html_url != format!("{RELEASES}/tag/{}", release.tag_name)
    {
        return Err("Unexpected official release metadata".into());
    }
    if !newer {
        return Ok(None);
    }
    let Some(suffix) = platform.suffix else {
        return Ok(Some(Candidate {
            version: tag.into(),
            asset_name: None,
            installer: None,
        }));
    };
    // The branded asset name is preferred; releases up to 1.0.6 use the former prefix.
    let name = ASSET_PREFIXES
        .iter()
        .map(|prefix| format!("{prefix}{tag}-{suffix}"))
        .find(|name| release.assets.iter().any(|a| &a.name == name))
        .unwrap_or_else(|| format!("{}{tag}-{suffix}", ASSET_PREFIXES[0]));
    let expected_url = format!("{RELEASES}/download/{}/{}", release.tag_name, name);
    let mut matches = release.assets.into_iter().filter(|a| a.name == name);
    let first = matches.next();
    let unique = matches.next().is_none();
    if !platform.downloads {
        // Display only: the release page stays the download source on this platform.
        let listed = first.is_some_and(|a| unique && a.browser_download_url == expected_url);
        return Ok(Some(Candidate {
            version: tag.into(),
            asset_name: listed.then_some(name),
            installer: None,
        }));
    }
    let asset = first.ok_or(
        "The latest release has no Windows setup yet. Try again after its package checks finish.",
    )?;
    if !unique || asset.size == 0 || asset.size > MAX_INSTALLER {
        return Err("Conflicting or oversized release installer".into());
    }
    if asset.browser_download_url != expected_url {
        return Err("Unexpected installer origin".into());
    }
    let digest = asset
        .digest
        .and_then(|s| s.strip_prefix("sha256:").map(str::to_owned))
        .ok_or("GitHub did not provide an installer SHA256; download is blocked")?;
    if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid installer digest".into());
    }
    Ok(Some(Candidate {
        version: tag.into(),
        asset_name: Some(name),
        installer: Some(Installer {
            url: expected_url,
            sha256: digest.to_ascii_lowercase(),
            bytes: asset.size,
        }),
    }))
}
/// Removes only the updater's own unfinished downloads. On Windows a download that
/// another app instance still writes is open without delete sharing and is skipped.
fn remove_stale_partials(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let own = entry.file_name().to_str().is_some_and(own_partial);
        if own && entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}
fn own_partial(name: &str) -> bool {
    name.strip_prefix("HandyMeeting-")
        .and_then(|rest| rest.strip_suffix(PARTIAL_SUFFIX))
        .and_then(|rest| rest.rsplit_once('-'))
        .is_some_and(|(release, id)| {
            version(release).is_ok_and(|v| v.pre.is_empty() && v.build.is_empty())
                && id.len() == 32
                && id.bytes().all(|b| b.is_ascii_hexdigit())
        })
}
/// Deletes its unfinished download unless the verified file was published.
struct PartialFile {
    path: PathBuf,
    file: Option<fs::File>,
    keep: bool,
}
impl Drop for PartialFile {
    fn drop(&mut self) {
        // Close first: Windows cannot delete a file that is still open here.
        self.file.take();
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// Streams into a `.partial` file and publishes `-setup.exe` only after size and SHA256 match.
fn receive(
    folder: &Path,
    version: &str,
    installer: &Installer,
    input: &mut impl Read,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<PathBuf> {
    let stem = format!("HandyMeeting-{version}-{}", meeting_notes::new_job_id());
    let target = folder.join(format!("{stem}-setup.exe"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let path = folder.join(format!("{stem}{PARTIAL_SUFFIX}"));
    let file = options
        .open(&path)
        .map_err(|_| "Could not reserve a new installer file")?;
    let mut partial = PartialFile {
        path,
        file: Some(file),
        keep: false,
    };
    let mut received = 0u64;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Installer download cancelled and the incomplete file was removed. Recordings and installed versions are unchanged. / Download abgebrochen und unvollständige Datei entfernt; Aufnahmen und installierte Versionen bleiben erhalten.".into());
        }
        let count = input.read(&mut buffer).map_err(|_| {
            "The installer connection was interrupted and the incomplete file was removed. Check for updates again to retry."
        })?;
        if count == 0 {
            break;
        }
        received = received
            .checked_add(count as u64)
            .ok_or("Download size overflow")?;
        if received > installer.bytes {
            return Err("The installer was larger than the verified release size and was removed. Check for updates again to retry.".into());
        }
        let output = partial.file.as_mut().ok_or("Installer download closed")?;
        output.write_all(&buffer[..count]).map_err(|_| {
            "Could not save the installer download; the incomplete file was removed"
        })?;
        hasher.update(&buffer[..count]);
        progress(received);
    }
    partial
        .file
        .take()
        .ok_or("Installer download closed")?
        .sync_all()
        .map_err(|_| "Could not confirm saved installer")?;
    if received != installer.bytes || format!("{:x}", hasher.finalize()) != installer.sha256 {
        return Err("Installer size or SHA256 did not match GitHub. The file was removed and installation is blocked; check for updates again to retry. / Installerprüfung fehlgeschlagen; Datei entfernt und Installation gesperrt.".into());
    }
    if fs::symlink_metadata(&target).is_ok() {
        return Err("Could not reserve a new installer file".into());
    }
    fs::rename(&partial.path, &target)
        .map_err(|_| "Could not finish the verified installer file")?;
    partial.keep = true;
    Ok(target)
}
fn allowed(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some("github.com" | "release-assets.githubusercontent.com")
        )
}
fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("Unexpected installer redirect")
            }
        }))
        .user_agent("HandyMeeting release updater")
        .build()
        .map_err(|_| "Could not initialize the update connection".into())
}
fn present(state: &Inner) -> View {
    let mut view = state.view.clone();
    view.platform = platform().os.into();
    view
}
impl Updater {
    pub fn snapshot(&self) -> Result<View> {
        let state = self.inner.lock().map_err(|_| "Update state unavailable")?;
        let mut view = present(&state);
        if matches!(view.status.as_str(), "checking" | "downloading") {
            view.elapsed_ms = state.started.map_or(0, |s| s.elapsed().as_millis() as u64);
        }
        Ok(view)
    }
    fn begin(&self, status: &str) -> Result<()> {
        let mut s = self.inner.lock().map_err(|_| "Update state unavailable")?;
        if matches!(
            s.view.status.as_str(),
            "checking" | "downloading" | "launched"
        ) {
            return Err("An update operation is already active".into());
        }
        s.view.status = status.into();
        s.view.error = None;
        s.view.elapsed_ms = 0;
        s.view.attempt += 1;
        s.started = Some(Instant::now());
        self.cancel.store(false, Ordering::Release);
        Ok(())
    }
    fn finish<T>(&self, result: Result<T>) -> Result<T> {
        if let Err(error) = &result {
            if let Ok(mut s) = self.inner.lock() {
                s.view.status = if self.cancel.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                s.view.error = Some(error.clone());
                s.view.elapsed_ms = s.started.map_or(0, |v| v.elapsed().as_millis() as u64);
            }
        }
        result
    }
    pub fn check(&self, current: &str) -> Result<View> {
        self.begin("checking")?;
        let result = (|| {
            let response=client()?.get(LATEST).timeout(Duration::from_secs(30)).header("Accept","application/vnd.github+json").send().and_then(reqwest::blocking::Response::error_for_status).map_err(|_|"The official update check failed. Check your internet connection and retry. / Die offizielle Updateprüfung ist fehlgeschlagen. Verbindung prüfen und erneut versuchen.")?;
            let mut bytes = Vec::new();
            response
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Could not read release metadata")?;
            if bytes.len() > 2 * 1024 * 1024 {
                return Err("Release metadata exceeds the limit".into());
            }
            let release: Release =
                serde_json::from_slice(&bytes).map_err(|_| "Invalid release metadata")?;
            let found = candidate(release, current, platform())?;
            let mut state = self.inner.lock().map_err(|_| "Update state unavailable")?;
            state.ready = None;
            state.view.received_bytes = 0;
            let installer = found.as_ref().and_then(|c| c.installer.as_ref());
            state.view.total_bytes = installer.map_or(0, |i| i.bytes);
            state.view.can_download = installer.is_some();
            state.view.asset_name = found.as_ref().and_then(|c| c.asset_name.clone());
            state.view.version = found.as_ref().map(|c| c.version.clone());
            state.view.release_url = found
                .as_ref()
                .map(|c| format!("{RELEASES}/tag/v{}", c.version));
            state.view.status = if found.is_some() {
                "available"
            } else {
                "current"
            }
            .into();
            state.candidate = found;
            state.view.elapsed_ms = state.started.map_or(0, |v| v.elapsed().as_millis() as u64);
            Ok(present(&state))
        })();
        self.finish(result)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    /// The official page for the checked release, or the release list before any check.
    pub fn release_page(&self) -> Result<String> {
        let state = self.inner.lock().map_err(|_| "Update state unavailable")?;
        let page = state.candidate.as_ref().map_or_else(
            || RELEASES.to_owned(),
            |c| format!("{RELEASES}/tag/v{}", c.version),
        );
        Ok(page)
    }
    pub fn download(&self, root: &Path) -> Result<View> {
        if !cfg!(target_os = "windows") {
            return Err("In-app installer downloads currently support Windows; use the official release page on this platform".into());
        }
        self.begin("downloading")?;
        let result = (|| {
            let candidate = self
                .inner
                .lock()
                .map_err(|_| "Update state unavailable")?
                .candidate
                .clone()
                .ok_or("Check for an official update first")?;
            let installer = candidate
                .installer
                .as_ref()
                .ok_or("Use the official release page for this platform")?;
            meeting_core::metadata::checked_directory(root)
                .map_err(|_| "Update storage is unavailable")?;
            let folder = root.join("updates");
            match fs::create_dir(&folder) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err("Could not create update folder".into()),
            }
            meeting_core::metadata::checked_directory(&folder)
                .map_err(|_| "Linked update folders are rejected")?;
            remove_stale_partials(&folder);
            let mut response = client()?
                .get(&installer.url)
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .map_err(|_| "Installer download failed; retry from the update check")?;
            let file = receive(
                &folder,
                &candidate.version,
                installer,
                &mut response,
                &self.cancel,
                |received| {
                    if let Ok(mut state) = self.inner.lock() {
                        state.view.received_bytes = received;
                    }
                },
            )?;
            let mut state = self.inner.lock().map_err(|_| "Update state unavailable")?;
            state.ready = Some(file);
            state.view.status = "ready".into();
            state.view.elapsed_ms = state.started.map_or(0, |v| v.elapsed().as_millis() as u64);
            Ok(present(&state))
        })();
        self.finish(result)
    }
    pub fn installer(&self) -> Result<InstallerLease> {
        let state = self.inner.lock().map_err(|_| "Update state unavailable")?;
        if state.view.status != "ready" {
            return Err("No verified installer is ready".into());
        }
        let candidate = state
            .candidate
            .as_ref()
            .and_then(|c| c.installer.as_ref())
            .ok_or("Missing verified release")?;
        let path = state.ready.as_ref().ok_or("Missing verified installer")?;
        let meta = fs::symlink_metadata(path).map_err(|_| "Verified installer is missing")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Linked installer rejected".into());
            }
        }
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != candidate.bytes {
            return Err("Installer changed; check and download again".into());
        }
        meeting_core::metadata::checked_directory(path.parent().ok_or("Missing installer folder")?)
            .map_err(|_| "Installer folder changed")?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut input = options
            .open(path)
            .map_err(|_| "Installer is unavailable or being changed")?;
        let mut hash = Sha256::new();
        let mut bytes = [0u8; 65536];
        loop {
            let n = input
                .read(&mut bytes)
                .map_err(|_| "Installer recheck failed")?;
            if n == 0 {
                break;
            }
            hash.update(&bytes[..n]);
        }
        if format!("{:x}", hash.finalize()) != candidate.sha256 {
            return Err("Installer changed; installation blocked".into());
        }
        Ok(InstallerLease {
            path: path.clone(),
            _file: input,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Release {
        Release{tag_name:"v1.0.2".into(),draft:false,prerelease:false,html_url:"https://github.com/Puma7/meeting-for-handy/releases/tag/v1.0.2".into(),assets:vec![Asset{name:"HandyMeeting-1.0.2-windows-x64-setup.exe".into(),size:123,digest:Some(format!("sha256:{}","a".repeat(64))),browser_download_url:"https://github.com/Puma7/meeting-for-handy/releases/download/v1.0.2/HandyMeeting-1.0.2-windows-x64-setup.exe".into()}]}
    }
    const WINDOWS: Platform = Platform {
        os: "windows",
        suffix: Some("windows-x64-setup.exe"),
        downloads: true,
    };
    const LINUX: Platform = Platform {
        os: "linux",
        suffix: Some("linux-amd64.deb"),
        downloads: false,
    };
    #[test]
    fn beta_current_version_can_select_the_stable_release_without_metadata_upgrades() {
        let stable = || {
            let mut r = release();
            r.tag_name = "v1.1.0".into();
            r.html_url = format!("{RELEASES}/tag/v1.1.0");
            r
        };
        for current in ["1.1.0-beta.1", "1.1.0-beta.10+build.3", "1.0.7"] {
            let found = candidate(stable(), current, LINUX).unwrap().unwrap();
            assert_eq!(found.version, "1.1.0");
            assert!(found.installer.is_none());
        }
        for current in ["1.1.0", "1.1.0+build.3", "1.1.1-beta.1", "2.0.0"] {
            assert!(candidate(stable(), current, LINUX).unwrap().is_none());
        }
        for current in ["1.1", "01.1.0", "1.1.0-01", "1.1.0-", "1.1.0+"] {
            assert!(candidate(stable(), current, LINUX).is_err());
        }
        for tag in ["1.2.0-beta.1", "1.2.0+build.3", "01.2.0"] {
            let mut r = stable();
            r.tag_name = format!("v{tag}");
            r.html_url = format!("{RELEASES}/tag/v{tag}");
            // Stable-channel checks reject even incorrectly labelled prerelease tags.
            assert!(candidate(r, "1.1.0-beta.1", LINUX).is_err());
        }
    }
    #[test]
    fn release_selection_rejects_drafts_bad_digests_and_foreign_origins() {
        assert!(candidate(release(), "1.0.1", WINDOWS).unwrap().is_some());
        assert!(candidate(release(), "1.0.2", WINDOWS).unwrap().is_none());
        let mut r = release();
        r.draft = true;
        assert!(candidate(r, "1.0.1", WINDOWS).is_err());
        let mut r = release();
        r.assets[0].digest = None;
        assert!(candidate(r, "1.0.1", WINDOWS).is_err());
        let mut r = release();
        r.assets[0].browser_download_url = "https://example.com/program.exe".into();
        assert!(candidate(r, "1.0.1", WINDOWS).is_err());
    }
    #[test]
    fn other_platforms_see_new_releases_without_the_windows_setup() {
        // The Windows setup alone must not fail a Linux check or become downloadable there.
        let found = candidate(release(), "1.0.1", LINUX).unwrap().unwrap();
        assert_eq!(found.version, "1.0.2");
        assert!(found.installer.is_none());
        assert!(found.asset_name.is_none());
        let mut r = release();
        r.assets.push(Asset {
            name: "HandyMeeting-1.0.2-linux-amd64.deb".into(),
            size: 5,
            digest: None,
            browser_download_url: format!(
                "{RELEASES}/download/v1.0.2/HandyMeeting-1.0.2-linux-amd64.deb"
            ),
        });
        let found = candidate(r, "1.0.1", LINUX).unwrap().unwrap();
        assert_eq!(
            found.asset_name.as_deref(),
            Some("HandyMeeting-1.0.2-linux-amd64.deb")
        );
        assert!(found.installer.is_none());
        let mut r = release();
        r.assets[0].name = "HandyMeeting-1.0.2-linux-amd64.deb".into();
        assert!(candidate(r, "1.0.1", LINUX)
            .unwrap()
            .unwrap()
            .asset_name
            .is_none());
        let mut r = release();
        r.assets.clear();
        assert!(candidate(r, "1.0.1", WINDOWS).is_err());
        let unknown = Platform {
            os: "freebsd",
            suffix: None,
            downloads: false,
        };
        assert!(candidate(release(), "1.0.1", unknown).unwrap().is_some());
    }
    #[test]
    fn branded_setup_name_is_found_and_preferred_over_the_former_name() {
        // A rename of the release asset must not cut installed apps off from updates.
        let branded = |r: &mut Release| {
            r.assets[0].name = "MeetingForHandy-1.0.2-windows-x64-setup.exe".into();
            r.assets[0].browser_download_url =
                format!("{RELEASES}/download/v1.0.2/MeetingForHandy-1.0.2-windows-x64-setup.exe");
        };
        let mut r = release();
        branded(&mut r);
        let found = candidate(r, "1.0.1", WINDOWS).unwrap().unwrap();
        assert_eq!(
            found.installer.unwrap().url,
            format!("{RELEASES}/download/v1.0.2/MeetingForHandy-1.0.2-windows-x64-setup.exe")
        );
        let mut r = release();
        let former = release().assets.remove(0);
        branded(&mut r);
        r.assets.push(former);
        let found = candidate(r, "1.0.1", WINDOWS).unwrap().unwrap();
        assert_eq!(
            found.asset_name.as_deref(),
            Some("MeetingForHandy-1.0.2-windows-x64-setup.exe")
        );
        // A branded name with a foreign origin is still rejected, not replaced by the other name.
        let mut r = release();
        let former = release().assets.remove(0);
        branded(&mut r);
        r.assets[0].browser_download_url = "https://example.com/program.exe".into();
        r.assets.push(former);
        assert!(candidate(r, "1.0.1", WINDOWS).is_err());
    }
    fn folder() -> PathBuf {
        let root = std::env::temp_dir().join(meeting_notes::new_job_id());
        fs::create_dir(&root).unwrap();
        root
    }
    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
    fn synthetic(bytes: &[u8]) -> Installer {
        Installer {
            url: String::new(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len() as u64,
        }
    }
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::ConnectionReset.into())
        }
    }
    #[test]
    fn only_verified_downloads_become_setup_files() {
        let root = folder();
        let payload = b"synthetic installer bytes, not an executable";
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let file = receive(
            &root,
            "1.0.3",
            &synthetic(payload),
            &mut &payload[..],
            &cancel,
            |n| seen = n,
        )
        .unwrap();
        assert_eq!(seen, payload.len() as u64);
        assert_eq!(fs::read(&file).unwrap(), payload);
        let name = file.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(name.starts_with("HandyMeeting-1.0.3-") && name.ends_with("-setup.exe"));
        assert_eq!(names(&root), vec![name]);
        fs::remove_file(file).unwrap();
        // Mismatched digest, oversize, truncation, interruption and cancellation leave nothing.
        let mut wrong = synthetic(payload);
        wrong.sha256 = "0".repeat(64);
        assert!(receive(&root, "1.0.3", &wrong, &mut &payload[..], &cancel, |_| ()).is_err());
        let mut short = synthetic(payload);
        short.bytes -= 1;
        assert!(receive(&root, "1.0.3", &short, &mut &payload[..], &cancel, |_| ()).is_err());
        let truncated = &payload[..10];
        assert!(receive(
            &root,
            "1.0.3",
            &synthetic(payload),
            &mut &truncated[..],
            &cancel,
            |_| ()
        )
        .is_err());
        assert!(receive(
            &root,
            "1.0.3",
            &synthetic(payload),
            &mut Broken,
            &cancel,
            |_| ()
        )
        .is_err());
        cancel.store(true, Ordering::Release);
        assert!(receive(
            &root,
            "1.0.3",
            &synthetic(payload),
            &mut &payload[..],
            &cancel,
            |_| ()
        )
        .is_err());
        assert!(names(&root).is_empty());
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn stale_cleanup_touches_only_own_partial_downloads() {
        let root = folder();
        let id = meeting_notes::new_job_id();
        let own = format!("HandyMeeting-1.0.3-{id}{PARTIAL_SUFFIX}");
        let kept = [
            format!("HandyMeeting-1.0.3-{id}-setup.exe"),
            "notes.partial".to_owned(),
            format!("HandyMeeting-1.0.3-short{PARTIAL_SUFFIX}"),
            format!("Other-1.0.3-{id}{PARTIAL_SUFFIX}"),
            format!("HandyMeeting-1.1.0-beta.1-{id}{PARTIAL_SUFFIX}"),
            format!("HandyMeeting-1.1.0+build.3-{id}{PARTIAL_SUFFIX}"),
        ];
        fs::write(root.join(&own), b"stale").unwrap();
        for name in &kept {
            fs::write(root.join(name), b"keep").unwrap();
        }
        fs::create_dir(root.join(format!("HandyMeeting-1.0.4-{id}{PARTIAL_SUFFIX}"))).unwrap();
        remove_stale_partials(&root);
        let mut expected: Vec<String> = kept.to_vec();
        expected.push(format!("HandyMeeting-1.0.4-{id}{PARTIAL_SUFFIX}"));
        expected.sort();
        assert_eq!(names(&root), expected);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_release_listed_by_github_for_this_repository_is_accepted() {
        // Shape and addresses as the GitHub API lists the published v1.0.1 release of
        // Puma7/meeting-for-handy. The earlier repository name redirects, and the
        // updater refuses both the redirect and a release page under another name.
        let listed: Release = serde_json::from_str(
            r#"{"tag_name":"v1.0.4","draft":false,"prerelease":false,
                "html_url":"https://github.com/Puma7/meeting-for-handy/releases/tag/v1.0.4",
                "assets":[{"name":"HandyMeeting-1.0.4-windows-x64-setup.exe","size":123,
                  "digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  "browser_download_url":"https://github.com/Puma7/meeting-for-handy/releases/download/v1.0.4/HandyMeeting-1.0.4-windows-x64-setup.exe"}]}"#,
        )
        .unwrap();
        let found = candidate(listed, "1.0.3", WINDOWS).unwrap().unwrap();
        assert_eq!(found.version, "1.0.4");
        assert_eq!(
            LATEST,
            "https://api.github.com/repos/Puma7/meeting-for-handy/releases/latest"
        );
    }
    #[test]
    fn release_page_stays_on_the_official_repository() {
        let updater = Updater::default();
        assert_eq!(updater.release_page().unwrap(), RELEASES);
        updater.inner.lock().unwrap().candidate = candidate(release(), "1.0.1", LINUX).unwrap();
        assert_eq!(
            updater.release_page().unwrap(),
            "https://github.com/Puma7/meeting-for-handy/releases/tag/v1.0.2"
        );
    }
    #[test]
    fn redirects_cannot_escape_https_official_asset_hosts() {
        for url in [
            "http://github.com/x",
            "https://evil.example/x",
            "https://github.com.evil.example/x",
            "https://user@github.com/x",
            "https://github.com:444/x",
        ] {
            assert!(!allowed(&url::Url::parse(url).unwrap()));
        }
        assert!(allowed(
            &url::Url::parse("https://release-assets.githubusercontent.com/file").unwrap()
        ));
    }
    #[test]
    fn install_recheck_blocks_modified_bytes() {
        let root = std::env::temp_dir().join(meeting_notes::new_job_id());
        fs::create_dir(&root).unwrap();
        let file = root.join("synthetic-not-executable.exe");
        fs::write(&file, b"old").unwrap();
        let updater = Updater::default();
        {
            let mut s = updater.inner.lock().unwrap();
            s.view.status = "ready".into();
            s.candidate = Some(Candidate {
                version: "1.0.2".into(),
                asset_name: None,
                installer: Some(synthetic(b"old")),
            });
            s.ready = Some(file.clone());
        }
        assert_eq!(updater.installer().unwrap().path, file);
        fs::write(&file, b"new").unwrap();
        assert!(updater.installer().is_err());
        fs::remove_file(file).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
