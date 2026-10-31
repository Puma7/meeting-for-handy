//! One-click local text model setup: known local servers, a model suggestion from installed
//! memory and an optional Ollama model download. Only fixed loopback addresses are contacted;
//! no transcript, session or key is involved.
use crate::AppResult;
use meeting_notes::AnalysisControl;
use serde::Serialize;
use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) const OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434/v1/";
pub(crate) const LM_STUDIO_ENDPOINT: &str = "http://127.0.0.1:1234/v1/";
/// Ollama's native API root; downloads go to `api/pull` below it.
const OLLAMA_ROOT: &str = "http://127.0.0.1:11434/";
pub(crate) const SMALL_MODEL: &str = "gemma4:e2b";
pub(crate) const STANDARD_MODEL: &str = "gemma4:e4b";
const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recommendation {
    /// none | small | standard | large | unknown
    tier: &'static str,
    model: Option<&'static str>,
    /// Name to search for in LM Studio's model catalog.
    lm_studio_search: Option<&'static str>,
}

/// Suggests a model size from physical memory. The thresholds leave room for the meeting
/// app, the speech model and the operating system next to the text model.
pub(crate) fn recommend(total_memory: Option<u64>) -> Recommendation {
    let (tier, model) = match total_memory {
        None => ("unknown", Some(STANDARD_MODEL)),
        Some(bytes) if bytes < 7 * GIB => ("none", None),
        Some(bytes) if bytes < 15 * GIB => ("small", Some(SMALL_MODEL)),
        Some(bytes) if bytes < 31 * GIB => ("standard", Some(STANDARD_MODEL)),
        Some(_) => ("large", Some(STANDARD_MODEL)),
    };
    Recommendation {
        tier,
        model,
        lm_studio_search: model.map(|m| {
            if m == SMALL_MODEL {
                "Gemma 4 E2B"
            } else {
                "Gemma 4 E4B"
            }
        }),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Provider {
    id: &'static str,
    name: &'static str,
    endpoint: String,
    reachable: bool,
    models: Vec<String>,
    recommended_installed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelSetup {
    total_memory_bytes: Option<u64>,
    recommendation: Recommendation,
    providers: Vec<Provider>,
}

/// Installed physical memory, read without extra libraries. `None` when it cannot be read.
pub(crate) fn total_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let info = std::fs::read_to_string("/proc/meminfo").ok()?;
        parse_meminfo(&info)
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let root = std::env::var_os("SystemRoot")?;
        let shell = std::path::Path::new(&root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let output = std::process::Command::new(shell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[string](Get-CimInstance -ClassName Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(info: &str) -> Option<u64> {
    let line = info.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kib.checked_mul(1024)
}

fn installed(models: &[String], wanted: Option<&str>) -> bool {
    wanted.is_some_and(|wanted| {
        models
            .iter()
            .any(|m| m == wanted || m.strip_suffix(":latest") == Some(wanted))
    })
}

/// The documented Ollama bind setting is used only for loopback. Wildcard binds are
/// contacted through loopback too; remote hosts, credentials and URL paths are rejected.
fn ollama_root_from(value: Option<&str>) -> String {
    let Some(value) = value.filter(|v| v.len() <= 4096).map(str::trim) else {
        return OLLAMA_ROOT.into();
    };
    let configured_url = value.split_once("://");
    let address = if configured_url.is_some() {
        value.to_owned()
    } else {
        format!("http://{value}")
    };
    let Ok(mut url) = url::Url::parse(&address) else {
        return OLLAMA_ROOT.into();
    };
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return OLLAMA_ROOT.into();
    }
    let host = match url.host() {
        Some(url::Host::Domain("localhost")) => "localhost".to_owned(),
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => ip.to_string(),
        Some(url::Host::Ipv4(ip)) if ip.is_unspecified() => "127.0.0.1".to_owned(),
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => format!("[{ip}]"),
        Some(url::Host::Ipv6(ip)) if ip.is_unspecified() => "[::1]".to_owned(),
        _ => return OLLAMA_ROOT.into(),
    };
    // URL parsing erases an explicit HTTP :80. Preserve the original port, and
    // match Ollama's defaults: bare hosts use 11434; HTTP URLs use 80.
    let authority = configured_url
        .map_or(value, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let explicit_port = if authority.starts_with('[') {
        authority
            .split_once(']')
            .and_then(|(_, tail)| tail.strip_prefix(':'))
    } else {
        authority.rsplit_once(':').map(|(_, port)| port)
    };
    let port = match explicit_port {
        Some(port) => match port.parse::<u16>() {
            Ok(port) => port,
            Err(_) => return OLLAMA_ROOT.into(),
        },
        None => url
            .port()
            .unwrap_or(if configured_url.is_some() { 80 } else { 11434 }),
    };
    if port == 0 {
        return OLLAMA_ROOT.into();
    }
    if url.set_host(Some(&host)).is_err() || url.set_port(Some(port)).is_err() {
        return OLLAMA_ROOT.into();
    }
    url.to_string()
}

fn ollama_root() -> String {
    ollama_root_from(std::env::var("OLLAMA_HOST").ok().as_deref())
}

/// Only the configured loopback provider addresses opt into native model management.
/// Other OpenAI-compatible local servers retain their existing transport behavior.
pub(crate) fn provider_for_endpoint(
    endpoint: &str,
) -> Option<meeting_notes::residency::LocalProvider> {
    let ollama = format!("{}v1/", ollama_root());
    let lm = dirs::home_dir()
        .map(|home| lm_studio_endpoint(&home))
        .unwrap_or_else(|| LM_STUDIO_ENDPOINT.into());
    provider_for_addresses(endpoint, &ollama, &lm)
}

/// Matches configured provider addresses without changing process environment or probing a server.
fn provider_for_addresses(
    endpoint: &str,
    ollama: &str,
    lm: &str,
) -> Option<meeting_notes::residency::LocalProvider> {
    /// Extracts only credential-free loopback origins and paths for configured-provider matching.
    fn address(value: &str) -> Option<(String, String, u16, String)> {
        let url = url::Url::parse(value).ok()?;
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let host = match url.host()? {
            url::Host::Domain("localhost") => "127.0.0.1".to_owned(),
            url::Host::Ipv4(ip) if ip.is_loopback() => ip.to_string(),
            url::Host::Ipv6(ip) if ip.is_loopback() => ip.to_string(),
            _ => return None,
        };
        Some((
            url.scheme().into(),
            host,
            url.port_or_known_default()?,
            url.path().trim_end_matches('/').into(),
        ))
    }
    let selected = address(endpoint)?;
    if address(ollama).as_ref() == Some(&selected) {
        return Some(meeting_notes::residency::LocalProvider::Ollama);
    }
    (address(lm).as_ref() == Some(&selected))
        .then_some(meeting_notes::residency::LocalProvider::LmStudio)
}

/// Reads just LM Studio's bounded server configuration, never conversations or credentials.
/// A custom bind address does not grant remote access: only its port is used on loopback.
fn lm_studio_endpoint(home: &Path) -> String {
    let path = home.join(".lmstudio/.internal/http-server-config.json");
    let fallback = || LM_STUDIO_ENDPOINT.to_owned();
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return fallback();
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return fallback();
        }
    }
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 65_536 {
        return fallback();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return fallback();
    };
    let mut bytes = Vec::new();
    if file.take(65_537).read_to_end(&mut bytes).is_err() || bytes.len() > 65_536 {
        return fallback();
    }
    let Ok(config) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return fallback();
    };
    let Some(port) = config
        .get("port")
        .and_then(|v| v.as_u64())
        .filter(|p| (1..=65_535).contains(p))
    else {
        return fallback();
    };
    format!("http://127.0.0.1:{port}/v1/")
}

/// Checks both known local servers on loopback and suggests a model.
pub(crate) fn setup() -> LocalModelSetup {
    let total_memory_bytes = total_memory();
    let recommendation = recommend(total_memory_bytes);
    let root = ollama_root();
    let ollama = if root == OLLAMA_ROOT {
        OLLAMA_ENDPOINT.into()
    } else {
        format!("{root}v1/")
    };
    let lm_studio = dirs::home_dir()
        .map(|home| lm_studio_endpoint(&home))
        .unwrap_or_else(|| LM_STUDIO_ENDPOINT.into());
    let providers = [
        ("ollama", "Ollama", ollama),
        ("lmstudio", "LM Studio", lm_studio),
    ]
    .into_iter()
    .map(|(id, name, endpoint)| {
        let found = meeting_notes::local_model_ids(&endpoint);
        let models = found.as_ref().cloned().unwrap_or_default();
        Provider {
            id,
            name,
            endpoint,
            reachable: found.is_ok(),
            recommended_installed: installed(&models, recommendation.model),
            models,
        }
    })
    .collect();
    LocalModelSetup {
        total_memory_bytes,
        recommendation,
        providers,
    }
}

/// Ollama model names: registry-style names with an optional tag, nothing else.
pub(crate) fn valid_model_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._:-/".contains(&c))
        && name
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && !name.contains("..")
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PullProgress {
    model: String,
    /// running | complete | failed | cancelled
    status: String,
    detail: String,
    completed: u64,
    total: u64,
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Pulls {
    state: Mutex<Option<PullProgress>>,
    cancel: Mutex<AnalysisControl>,
}

impl Pulls {
    pub(crate) fn snapshot(&self) -> Option<PullProgress> {
        self.state.lock().ok().and_then(|s| s.clone())
    }

    pub(crate) fn cancel(&self) -> bool {
        let running = self
            .state
            .lock()
            .is_ok_and(|s| s.as_ref().is_some_and(|p| p.status == "running"));
        if running {
            if let Ok(control) = self.cancel.lock() {
                control.cancel();
            }
            // Reads "cancelled" as soon as the download thread has closed the connection.
            if let Ok(mut state) = self.state.lock() {
                if let Some(progress) = state.as_mut() {
                    progress.detail = "wird abgebrochen …".into();
                }
            }
        }
        running
    }

    /// Asks the local Ollama to download `model` and follows its progress in the background.
    pub(crate) fn start(self: &Arc<Self>, model: String) -> AppResult<PullProgress> {
        if !valid_model_name(&model) {
            return Err("Ungültiger Modellname.".into());
        }
        let mut state = self.state.lock().map_err(|_| "Download ist gesperrt.")?;
        if state.as_ref().is_some_and(|p| p.status == "running") {
            return Err("Ein Modell-Download läuft bereits.".into());
        }
        let control = AnalysisControl::default();
        *self.cancel.lock().map_err(|_| "Download ist gesperrt.")? = control.clone();
        let progress = PullProgress {
            model: model.clone(),
            status: "running".into(),
            ..PullProgress::default()
        };
        *state = Some(progress.clone());
        drop(state);
        let pulls = Arc::clone(self);
        std::thread::Builder::new()
            .name("ollama-pull".into())
            .spawn(move || {
                let result = pull(&model, &control, |update| pulls.update(update));
                pulls.finish(result, control.is_cancelled());
            })
            .map_err(|_| "Der Download konnte nicht gestartet werden.")?;
        Ok(progress)
    }

    fn update(&self, line: PullLine) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(progress) = state.as_mut().filter(|p| p.status == "running") {
                progress.detail = line.status.chars().take(200).collect();
                if let (Some(completed), Some(total)) = (line.completed, line.total) {
                    progress.completed = completed;
                    progress.total = total;
                }
            }
        }
    }

    fn finish(&self, result: AppResult<()>, cancelled: bool) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(progress) = state.as_mut() {
                match result {
                    Ok(()) => progress.status = "complete".into(),
                    Err(_) if cancelled => progress.status = "cancelled".into(),
                    Err(error) => {
                        progress.status = "failed".into();
                        progress.error = Some(error);
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize, Default)]
struct PullLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

/// Longest silence of a running Ollama stream (it reports progress continuously, also while
/// verifying). It bounds a stalled server; a cancel ends the connection at once.
const PULL_IDLE: Duration = Duration::from_secs(300);

fn pull(
    model: &str,
    control: &AnalysisControl,
    mut progress: impl FnMut(PullLine),
) -> AppResult<()> {
    pull_from(&ollama_root(), model, control, PULL_IDLE, &mut progress)
}

/// Follows Ollama's streamed pull answer line by line through the loopback-only text model
/// transport. A cancel drops the connection immediately, also while the server is silent.
fn pull_from(
    endpoint: &str,
    model: &str,
    control: &AnalysisControl,
    idle: Duration,
    progress: &mut impl FnMut(PullLine),
) -> AppResult<()> {
    let body = serde_json::json!({"model": model, "stream": true});
    let mut reported: Option<String> = None;
    let mut finished = false;
    let result =
        meeting_notes::local_json_lines(endpoint, "api/pull", &body, control, idle, |line| {
            let Ok(update) = serde_json::from_str::<PullLine>(line) else {
                return Ok(true);
            };
            if let Some(error) = &update.error {
                reported = Some(format!(
                    "Ollama meldet: {}",
                    error.chars().take(300).collect::<String>()
                ));
                return Ok(false);
            }
            finished = update.status == "success";
            progress(update);
            Ok(!finished)
        });
    if let Some(error) = reported {
        return Err(error);
    }
    match result {
        Ok(()) if finished => Ok(()),
        Ok(()) => Err("Ollama hat den Download ohne Abschluss beendet.".into()),
        Err(error) if meeting_notes::is_cancelled(&error) => Err("Download abgebrochen.".into()),
        Err(error) => Err(format!(
            "Ollama ist nicht erreichbar oder der Download wurde unterbrochen: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    /// A native management route belongs to its configured bind, even when another local
    /// server uses the same port and path on a different loopback address.
    fn provider_matching_keeps_distinct_loopback_binds_separate() {
        for (selected, configured) in [
            ("http://127.0.0.1:2345/v1/", "http://127.0.0.2:2345/v1/"),
            ("http://127.23.45.67:2345/v1/", "http://127.0.0.1:2345/v1/"),
            ("http://[::1]:2345/v1/", "http://127.0.0.1:2345/v1/"),
        ] {
            assert_eq!(
                provider_for_addresses(selected, configured, configured),
                None,
                "selected {selected}, configured {configured}"
            );
        }
    }

    #[test]
    /// Provider precedence cannot turn an LM Studio bind into Ollama merely because
    /// both configured services use the same numeric port on different local addresses.
    fn provider_matching_routes_same_port_to_its_own_configured_provider() {
        use meeting_notes::residency::LocalProvider;
        let ollama = "http://127.0.0.2:2345/v1/";
        let lm = "http://127.0.0.1:2345/v1/";
        assert_eq!(
            provider_for_addresses(lm, ollama, lm),
            Some(LocalProvider::LmStudio)
        );
        assert_eq!(
            provider_for_addresses(ollama, ollama, lm),
            Some(LocalProvider::Ollama)
        );
        assert_eq!(
            provider_for_addresses("http://[::1]:2345/v1/", ollama, lm),
            None
        );
    }

    #[test]
    /// Normalization retains localhost's existing IPv4 alias and the same IPv6 bind,
    /// while unsafe URL forms and different paths or ports never acquire a native route.
    fn provider_matching_normalizes_only_the_same_safe_origin_and_path() {
        use meeting_notes::residency::LocalProvider;
        for (selected, configured) in [
            ("http://localhost:2345/v1", "http://127.0.0.1:2345/v1/"),
            ("http://127.0.0.1:2345/v1/", "http://localhost:2345/v1"),
            ("http://[0:0:0:0:0:0:0:1]:2345/v1/", "http://[::1]:2345/v1/"),
            ("http://127.0.0.2:2345/v1/", "http://127.0.0.2:2345/v1"),
        ] {
            assert_eq!(
                provider_for_addresses(selected, configured, "http://127.0.0.1:1234/v1/"),
                Some(LocalProvider::Ollama),
                "{selected}"
            );
        }
        for selected in [
            "http://127.0.0.1:2346/v1/",
            "http://127.0.0.1:2345/other/",
            "https://127.0.0.1:2345/v1/",
            "http://user:secret@127.0.0.1:2345/v1/",
            "http://127.0.0.1:2345/v1/?key=secret",
            "http://127.0.0.1:2345/v1/#fragment",
            "http://example.com:2345/v1/",
        ] {
            assert_eq!(
                provider_for_addresses(
                    selected,
                    "http://127.0.0.1:2345/v1/",
                    "http://127.0.0.1:1234/v1/"
                ),
                None,
                "{selected}"
            );
        }
    }

    #[test]
    /// Reads only a valid bounded port setting and rejects oversized or invalid
    /// configuration while retaining a loopback endpoint even for a remote bind setting.
    fn configured_lm_studio_port_is_loopback_only_and_bounded() {
        let root =
            std::env::temp_dir().join(format!("handy-lm-config-{}", meeting_notes::new_job_id()));
        let folder = root.join(".lmstudio/.internal");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("http-server-config.json");
        for (input, wanted) in [
            (
                r#"{"port":1235,"networkInterface":"0.0.0.0"}"#,
                "http://127.0.0.1:1235/v1/",
            ),
            (r#"{"port":0}"#, LM_STUDIO_ENDPOINT),
            (r#"{"port":65536}"#, LM_STUDIO_ENDPOINT),
            (r#"{"port":"1235"}"#, LM_STUDIO_ENDPOINT),
            ("invalid", LM_STUDIO_ENDPOINT),
        ] {
            std::fs::write(&path, input).unwrap();
            assert_eq!(lm_studio_endpoint(&root), wanted);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), input);
        }
        std::fs::write(&path, vec![b' '; 65_537]).unwrap();
        assert_eq!(lm_studio_endpoint(&root), LM_STUDIO_ENDPOINT);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    /// Preserves explicit IPv4, IPv6 and localhost bind identities instead of probing a
    /// different loopback address that may have no server listening on it.
    fn explicit_ollama_loopback_addresses_keep_their_bind_identity() {
        for (input, expected) in [
            ("127.0.0.2:2345", "http://127.0.0.2:2345/"),
            ("http://127.23.45.67:2345", "http://127.23.45.67:2345/"),
            ("localhost:2345", "http://localhost:2345/"),
            ("[::1]:2345", "http://[::1]:2345/"),
        ] {
            assert_eq!(ollama_root_from(Some(input)), expected, "{input}");
        }
    }

    #[test]
    /// Covers URL normalization of explicit port 80, including leading zeros and wildcard
    /// binds; all resulting requests must still target that same local HTTP service.
    fn explicit_ollama_http_default_ports_are_not_replaced_by_11434() {
        for input in [
            "127.0.0.1:80",
            "http://127.0.0.1:80/",
            "127.0.0.1:00080",
            "http://0.0.0.0:80",
        ] {
            assert_eq!(
                ollama_root_from(Some(input)),
                "http://127.0.0.1/",
                "{input}"
            );
        }
        assert_eq!(ollama_root_from(Some("localhost:80")), "http://localhost/");
        assert_eq!(ollama_root_from(Some("[::1]:80")), "http://[::1]/");
        assert_eq!(ollama_root_from(Some("http://[::]:80/")), "http://[::1]/");
    }

    #[test]
    fn omitted_ollama_ports_follow_the_configured_address_form() {
        assert_eq!(ollama_root_from(Some("127.0.0.1")), OLLAMA_ROOT);
        assert_eq!(
            ollama_root_from(Some("http://127.0.0.1")),
            "http://127.0.0.1/"
        );
        assert_eq!(
            ollama_root_from(Some("http://localhost/")),
            "http://localhost/"
        );
        assert_eq!(ollama_root_from(Some("[::1]")), "http://[::1]:11434/");
        assert_eq!(ollama_root_from(Some("http://[::1]")), "http://[::1]/");
    }

    #[test]
    /// Accepts valid local binds while rejecting remote hosts, credentials, non-root paths,
    /// query data and zero ports without forwarding any of those values.
    fn ollama_bind_settings_never_contact_remote_hosts_or_forward_credentials() {
        for address in ["127.0.0.1:2345", "http://0.0.0.0:2345"] {
            assert_eq!(ollama_root_from(Some(address)), "http://127.0.0.1:2345/");
        }
        assert_eq!(ollama_root_from(Some("[::1]:2345")), "http://[::1]:2345/");
        for address in [
            "https://example.com",
            "http://example.com:1234",
            "http://user:secret@127.0.0.1:2345",
            "http://127.0.0.1:2345/other",
            "http://127.0.0.1:2345/?key=secret",
            "http://127.0.0.1:0",
        ] {
            assert_eq!(ollama_root_from(Some(address)), OLLAMA_ROOT);
        }
    }

    #[test]
    fn memory_tiers_follow_the_documented_thresholds() {
        assert_eq!(recommend(Some(4 * GIB)).tier, "none");
        assert_eq!(recommend(Some(4 * GIB)).model, None);
        assert_eq!(recommend(Some(8 * GIB)).model, Some(SMALL_MODEL));
        assert_eq!(recommend(Some(16 * GIB)).model, Some(STANDARD_MODEL));
        assert_eq!(recommend(Some(16 * GIB)).tier, "standard");
        assert_eq!(recommend(Some(64 * GIB)).tier, "large");
        assert_eq!(recommend(None).tier, "unknown");
        assert_eq!(
            recommend(Some(8 * GIB)).lm_studio_search,
            Some("Gemma 4 E2B")
        );
    }

    #[test]
    fn meminfo_is_read_in_bytes() {
        let info = "MemTotal:       16303428 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_meminfo(info), Some(16_303_428 * 1024));
        assert_eq!(parse_meminfo("nothing"), None);
    }

    #[test]
    fn only_plain_model_names_are_sent_to_ollama() {
        for name in ["gemma4:e4b", "gemma4:e2b", "library/qwen3:4b", "llama3.2"] {
            assert!(valid_model_name(name), "{name}");
        }
        for name in ["", "Gemma4", "../x", "a b", "gemma4:e4b\n", "-flag", "x\"}"] {
            assert!(!valid_model_name(name), "{name:?}");
        }
    }

    #[test]
    fn a_recommended_model_counts_as_installed_with_or_without_latest() {
        let models = vec!["gemma4:e4b".to_string()];
        assert!(installed(&models, Some("gemma4:e4b")));
        assert!(!installed(&models, Some("gemma4:e2b")));
        assert!(installed(&["gemma4:e2b:latest".into()], Some("gemma4:e2b")));
        assert!(!installed(&models, None));
    }

    /// Synthetic Ollama: streams pull progress lines, then success.
    fn serve(lines: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![0_u8; 4096];
            let count = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{lines}",
                lines.len()
            )
            .unwrap();
            String::from_utf8_lossy(&request[..count]).into_owned()
        });
        (url, handle)
    }

    #[test]
    fn pull_progress_is_followed_until_success() {
        let (url, server) = serve(
            "{\"status\":\"pulling manifest\"}\n{\"status\":\"downloading\",\"completed\":5,\"total\":10}\n{\"status\":\"success\"}\n",
        );
        let mut seen = Vec::new();
        pull_from(
            &url,
            "gemma4:e4b",
            &AnalysisControl::default(),
            PULL_IDLE,
            &mut |l| seen.push((l.status, l.completed)),
        )
        .unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[1], ("downloading".into(), Some(5)));
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /api/pull HTTP/1.1"));
        assert!(request.contains("\"model\":\"gemma4:e4b\""));
    }

    #[test]
    fn a_cancel_ends_a_download_whose_server_stalls_and_closes_its_connection() {
        // CodeRabbit reviews 5682803580 and 5686078293: a cancelled pull left its reader
        // blocked on a stalled stream, first without a limit, then until the idle limit.
        for headers_first in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = vec![0_u8; 4096];
                let _ = stream.read(&mut request).unwrap();
                if headers_first {
                    // Synthetic stall between progress lines of a chunked stream.
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
                    let line = "{\"status\":\"downloading\",\"completed\":1,\"total\":10}\n";
                    write!(stream, "{:x}\r\n{line}\r\n", line.len()).unwrap();
                    stream.flush().unwrap();
                }
                // Synthetic stall: no further bytes. The client must close the connection.
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut rest = [0_u8; 64];
                matches!(stream.read(&mut rest), Ok(0))
            });
            let cancel = AnalysisControl::default();
            let flag = cancel.clone();
            let canceller = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                flag.cancel();
            });
            let started = std::time::Instant::now();
            // With the production idle limit: a cancel must not wait for it.
            let error = pull_from(&url, "gemma4:e4b", &cancel, PULL_IDLE, &mut |_| {}).unwrap_err();
            assert!(error.contains("abgebrochen"), "{error}");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "cancel must not wait for the server"
            );
            canceller.join().unwrap();
            assert!(
                server.join().unwrap(),
                "the cancelled pull kept its connection open"
            );
        }
    }

    #[test]
    fn a_pull_error_or_an_unfinished_stream_is_reported_not_treated_as_success() {
        let (url, server) = serve("{\"error\":\"pull model manifest: file does not exist\"}\n");
        let error = pull_from(
            &url,
            "gemma4:e9b",
            &AnalysisControl::default(),
            PULL_IDLE,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.contains("file does not exist"), "{error}");
        server.join().unwrap();
        let (url, server) = serve("{\"status\":\"downloading\"}\n");
        assert!(pull_from(
            &url,
            "gemma4:e4b",
            &AnalysisControl::default(),
            PULL_IDLE,
            &mut |_| {}
        )
        .is_err());
        server.join().unwrap();
    }
}
