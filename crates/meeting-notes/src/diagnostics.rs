//! Bounded, append-only request evidence. No free-form server data, prompts, replies,
//! endpoint, key, transcript, source IDs or paths belong in this format.
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequestResult {
    #[default]
    Transport,
    Http,
    ContextLimit,
    Envelope,
    Incomplete,
    Validation,
    Persistence,
    Stored,
    Cancelled,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestDiagnostic {
    pub elapsed_ms: u64,
    pub http_status: Option<u16>,
    pub finish: Option<Finish>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub output_limit: u32,
    pub result: RequestResult,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_attempts: Vec<RequestAttempt>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted_attempts: u32,
}
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// Scalar evidence of the discarded length-limited attempt, without server text.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestAttempt {
    pub http_status: Option<u16>,
    pub finish: Option<Finish>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub output_limit: u32,
    pub result: RequestResult,
}
impl From<RequestDiagnostic> for RequestAttempt {
    fn from(value: RequestDiagnostic) -> Self {
        Self {
            http_status: value.http_status,
            finish: value.finish,
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            output_limit: value.output_limit,
            result: value.result,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Finish {
    Stop,
    Length,
    Filtered,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    Window,
    Overview,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveDiagnostic {
    pub operation: Operation,
    pub window: u32,
    pub accepted: usize,
    pub rejected: usize,
    pub request: RequestDiagnostic,
}

const LIMIT: u32 = 1024;
/// Exclusive files preserve every previous attempt. Exhaustion or storage failure is
/// reported separately and must never turn a successful model result into a failure.
pub(crate) fn save(folder: &Path, value: &LiveDiagnostic) -> anyhow::Result<()> {
    for index in 1..=LIMIT {
        let path = folder.join(format!("diagnostic-{index:06}.json"));
        match fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return crate::service::create_json(&path, value);
    }
    anyhow::bail!(
        "Das begrenzte Diagnoseprotokoll ist voll; gespeicherte Ergebnisse bleiben erhalten."
    )
}

/// Reads only owned bounded regular files, without repairing malformed evidence.
pub(crate) fn latest(folder: &Path) -> Option<LiveDiagnostic> {
    let mut latest = None;
    for index in 1..=LIMIT {
        let path = folder.join(format!("diagnostic-{index:06}.json"));
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => continue,
        };
        if !meta.is_file() || meta.len() > 4096 {
            continue;
        }
        let mut bytes = Vec::new();
        if fs::File::open(&path)
            .and_then(|file| file.take(4097).read_to_end(&mut bytes))
            .is_ok()
            && bytes.len() <= 4096
        {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                latest = Some(value);
            }
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_are_exclusive_bounded_and_read_back_without_content() {
        let dir = tempfile::tempdir().unwrap();
        let value = LiveDiagnostic {
            operation: Operation::Overview,
            window: 3,
            accepted: 0,
            rejected: 2,
            request: RequestDiagnostic {
                result: RequestResult::Incomplete,
                finish: Some(Finish::Length),
                ..Default::default()
            },
        };
        save(dir.path(), &value).unwrap();
        let first = fs::read(dir.path().join("diagnostic-000001.json")).unwrap();
        save(dir.path(), &value).unwrap();
        assert_eq!(
            first,
            fs::read(dir.path().join("diagnostic-000001.json")).unwrap()
        );
        assert_eq!(latest(dir.path()).unwrap().window, 3);
        for i in 3..=LIMIT {
            fs::write(dir.path().join(format!("diagnostic-{i:06}.json")), b"{}").unwrap();
        }
        assert!(save(dir.path(), &value).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), LIMIT as usize);
    }
}
