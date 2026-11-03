//! Thin orchestration around Handy's maintained safe Rust bindings. No local
//! FFI declarations, model parser or recognition algorithm are implemented.
use crate::{BridgeError, Result};
use std::path::Path;
use transcribe_cpp::{
    Backend, BackendMask, Model, ModelOptions, RunOptions, Stream, StreamOptions,
};

pub use transcribe_cpp::{Capabilities, StreamText, StreamUpdate};

pub use crate::RuntimeIdentity;

pub fn runtime_identity() -> RuntimeIdentity {
    RuntimeIdentity {
        compiled_version: transcribe_cpp::compiled_version(),
        loaded_version: transcribe_cpp::version(),
        loaded_commit: transcribe_cpp::version_commit(),
        public_header_hash: transcribe_cpp::header_hash().into(),
    }
}

fn verify_runtime() -> Result<()> {
    let identity = runtime_identity();
    let base = identity
        .loaded_version
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()
        .unwrap_or("")
        .trim_end_matches('.');
    if base != identity.compiled_version {
        return Err(BridgeError::Native("The packaged recognition runtime version does not match its Rust bindings. Use a complete matching HandyMeeting installation.".into()));
    }
    Ok(())
}

#[derive(Debug)]
pub struct NativeEngine {
    model: Model,
    capabilities: Capabilities,
}

impl NativeEngine {
    pub fn load(path: &Path, cpu_only: bool, backend_directory: Option<&Path>) -> Result<Self> {
        Self::load_mode(path, cpu_only, backend_directory, false)
    }

    pub fn load_batch(
        path: &Path,
        cpu_only: bool,
        backend_directory: Option<&Path>,
    ) -> Result<Self> {
        Self::load_mode(path, cpu_only, backend_directory, true)
    }

    fn load_mode(
        path: &Path,
        cpu_only: bool,
        backend_directory: Option<&Path>,
        batch: bool,
    ) -> Result<Self> {
        verify_runtime()?;
        // CPU replacement workers never register GPU modules; a broken GPU
        // driver must not be loaded merely to request Backend::Cpu afterward.
        transcribe_cpp::init_backends_with(
            backend_directory,
            if cpu_only {
                BackendMask::CPU
            } else {
                BackendMask::ALL
            },
        )
        .map_err(native_error)?;
        let model = Model::load_with(
            path,
            &ModelOptions {
                backend: if cpu_only {
                    Backend::Cpu
                } else {
                    Backend::Auto
                },
                ..Default::default()
            },
        )
        .map_err(native_error)?;
        let capabilities = model.capabilities().map_err(native_error)?;
        if capabilities.supports_streaming == batch
            || capabilities.native_sample_rate != 16_000
            || (batch && !model.roles().contains(transcribe_cpp::Role::Asr))
        {
            return Err(BridgeError::ModelUnavailable);
        }
        Ok(Self {
            model,
            capabilities,
        })
    }

    /// Alignment is used only to identify a possible repeated prefix.
    /// Section times remain the only timestamps published to the meeting.
    pub fn transcribe_batch(&self, pcm: &[f32], context_samples: usize) -> Result<(String, usize)> {
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        let result = self
            .model
            .session_with(&transcribe_cpp::SessionOptions {
                n_threads: (available / 2).clamp(1, 4) as i32,
                ..Default::default()
            })
            .map_err(native_error)?
            .run(pcm, &RunOptions::default())
            .map_err(native_error)?;
        let context_ms = (context_samples as i64) * 1000 / 16_000;
        let normalize = |value: &str| {
            value
                .trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_lowercase()
        };
        let overlap = result
            .words
            .iter()
            .zip(result.text.split_whitespace())
            .take_while(|(word, text)| {
                word.t0_ms >= 0
                    && word.t1_ms > word.t0_ms
                    && word.t0_ms < context_ms
                    && word.t1_ms - word.t0_ms <= (context_ms - word.t0_ms) * 2
                    && normalize(&word.text) == normalize(text)
            })
            .count();
        Ok((result.text, overlap))
    }

    pub fn backend(&self) -> String {
        self.model.backend()
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Keeps the model loaded across bounded recognition windows. An active
    /// upstream Stream exclusively leases this model. Two concurrent tracks
    /// therefore use separate worker/model instances, not a cloned Model.
    pub fn with_stream<T>(
        &self,
        run: &RunOptions,
        operation: impl FnOnce(&mut Stream<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut session = self.model.session().map_err(native_error)?;
        let mut stream = session
            .stream(run, &StreamOptions::default())
            .map_err(native_error)?;
        operation(&mut stream)
    }
}

pub fn native_error(error: transcribe_cpp::Error) -> BridgeError {
    // Do not include the load error's path or potentially sensitive input in
    // persisted/public logs. Callers can present the selected model separately.
    let _ = error;
    BridgeError::Native("The installed model or selected compute backend is not supported by this runtime. Audio capture is independent and can continue.".into())
}

pub fn default_run_options() -> RunOptions {
    RunOptions::default()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeakerTurn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: u32,
    pub confidence: Option<f32>,
}
/// Independent optional CPU model. A failure never controls the recording or ASR worker.
pub struct NativeSpeakerEngine {
    model: Model,
    standalone: bool,
}
impl NativeSpeakerEngine {
    pub fn load(path: &Path, backend_directory: &Path) -> Result<Self> {
        verify_runtime()?;
        transcribe_cpp::init_backends_with(Some(backend_directory), BackendMask::CPU)
            .map_err(native_error)?;
        let model = Model::load_with(
            path,
            &ModelOptions {
                backend: Backend::Cpu,
                ..Default::default()
            },
        )
        .map_err(native_error)?;
        let standalone = model.roles().contains(transcribe_cpp::Role::Diarize);
        if standalone {
            if model.diarize_info().map_err(native_error)?.sample_rate != 16_000 {
                return Err(BridgeError::ModelUnavailable);
            }
        } else if !model.roles().contains(transcribe_cpp::Role::Asr)
            || !model.supports(transcribe_cpp::Feature::Diarization)
            || model
                .capabilities()
                .map_err(native_error)?
                .native_sample_rate
                != 16_000
        {
            return Err(BridgeError::Native("This installed model does not support local speaker separation. Choose a compatible diarization model; recording and normal transcription remain available.".into()));
        }
        Ok(Self { model, standalone })
    }
    pub fn run(&self, pcm: &[f32]) -> Result<Vec<SpeakerTurn>> {
        if pcm.is_empty() || pcm.len() > 16_000 * 120 || pcm.iter().any(|v| !v.is_finite()) {
            return Err(BridgeError::Native(
                "Speaker audio section exceeds its safe limits".into(),
            ));
        }
        let rows = if self.standalone {
            self.model
                .diarize_session()
                .map_err(native_error)?
                .run(pcm, &transcribe_cpp::DiarizeOptions::default())
                .map_err(native_error)?
        } else {
            self.model
                .session()
                .map_err(native_error)?
                .run(
                    pcm,
                    &RunOptions {
                        diarize: transcribe_cpp::Diarize::On,
                        ..Default::default()
                    },
                )
                .map_err(native_error)?
                .speaker_segments
        };
        if rows.len() > 10000 {
            return Err(BridgeError::OutputLimit);
        }
        let duration = (pcm.len() as u64) * 1000 / 16000;
        let mut turns = Vec::new();
        for row in rows {
            // Untimed text attribution cannot become invented word or turn times.
            if row.t0_ms == 0 && row.t1_ms == 0 {
                continue;
            }
            if row.t0_ms < 0
                || row.t1_ms <= row.t0_ms
                || row.t1_ms as u64 > duration
                || !(1..=1000).contains(&row.speaker_id)
            {
                return Err(BridgeError::Native(
                    "Speaker model returned invalid section bounds".into(),
                ));
            }
            turns.push(SpeakerTurn {
                start_ms: row.t0_ms as u64,
                end_ms: row.t1_ms as u64,
                speaker_id: row.speaker_id as u32,
                confidence: (row.p.is_finite() && (0.0..=1.0).contains(&row.p)).then_some(row.p),
            });
        }
        Ok(turns)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn packaged_runtime_matches_the_pinned_safe_bindings() {
        let identity = super::runtime_identity();
        assert_eq!(identity.compiled_version, "0.4.1");
        super::verify_runtime().unwrap();
        assert!(!identity.public_header_hash.is_empty());
    }
}
