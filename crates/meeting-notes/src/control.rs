use anyhow::Result;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

pub fn new_job_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[derive(Clone, Debug, Default)]
pub struct AnalysisControl {
    cancelled: Arc<AtomicBool>,
    response_bytes: Arc<AtomicU64>,
    validation_retries: Arc<AtomicU64>,
    last_response: Arc<Mutex<Option<std::time::Instant>>>,
}

#[derive(Debug)]
pub struct AnalysisCancelled;
impl std::fmt::Display for AnalysisCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auswertung abgebrochen. Bereits bestätigte Arbeitsschritte bleiben erhalten und können exakt fortgesetzt werden.")
    }
}
impl std::error::Error for AnalysisCancelled {}

impl AnalysisControl {
    pub(crate) fn begin_workspace_update(&self) {
        self.validation_retries.store(0, Ordering::Release);
    }
    pub(crate) fn record_validation_retry(&self) {
        self.validation_retries.fetch_add(1, Ordering::Relaxed);
    }
    pub fn validation_retries(&self) -> u64 {
        self.validation_retries.load(Ordering::Relaxed)
    }
    pub(crate) fn begin_response(&self) {
        self.response_bytes.store(0, Ordering::Release);
        if let Ok(mut last) = self.last_response.lock() {
            *last = Some(std::time::Instant::now());
        }
    }
    pub(crate) fn received_response_bytes(&self, bytes: usize) {
        self.response_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
        if let Ok(mut last) = self.last_response.lock() {
            *last = Some(std::time::Instant::now());
        }
    }
    pub fn response_bytes(&self) -> u64 {
        self.response_bytes.load(Ordering::Relaxed)
    }
    pub fn response_idle_ms(&self) -> u64 {
        self.last_response
            .lock()
            .ok()
            .and_then(|last| *last)
            .map_or(0, |at| at.elapsed().as_millis() as u64)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(AnalysisCancelled.into())
        } else {
            Ok(())
        }
    }
}
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<AnalysisCancelled>().is_some()
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgress {
    pub run_id: String,
    pub stage: String,
    pub step_name: Option<String>,
    /// One based during work; zero while preparing.
    pub step_index: usize,
    pub total_steps: usize,
    pub batch_index: usize,
    pub total_batches: usize,
    pub confirmed_tasks: usize,
    pub total_tasks: usize,
}
