use crate::{AudioPart, Result, SessionStore};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::Path,
    time::{Duration, Instant},
};

pub const SAMPLE_RATE: u32 = 16_000;
// At least twenty seconds per section, then 450 ms of low amplitude can
// close it early. The existing thirty-second hard limit still applies.
const QUIET_SEARCH_AFTER: u64 = SAMPLE_RATE as u64 * 20;
const QUIET_SAMPLES: usize = SAMPLE_RATE as usize * 450 / 1000;
const QUIET_AMPLITUDE: u16 = 160;
const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);

/// Hound serializes WAVE. Its flush updates the header and reaches sync_all here.
struct SyncFile(File);
impl Write for SyncFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.sync_all()
    }
}
impl Seek for SyncFile {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(from)
    }
}

type WaveWriter = hound::WavWriter<BufWriter<SyncFile>>;
struct Chunk {
    writer: WaveWriter,
    part: AudioPart,
    digest: Sha256,
    quiet_samples: usize,
}

/// One bounded writer per track. No model handles or ASR queues are involved.
pub struct PcmWriter {
    store: SessionStore,
    source: String,
    chunk_samples: u64,
    written: u64,
    chunk: Option<Chunk>,
    last_flush: Instant,
}

impl PcmWriter {
    /// Capture-controller-only continuation of a closed microphone track. Existing WAVs
    /// are read and verified, never reopened for writing. Normal `new` still refuses reuse.
    /// `progress` is called after each verified section so the caller can tell a long
    /// verification from a stalled device.
    pub(crate) fn for_capture(
        store: SessionStore,
        source: &str,
        progress: &dyn Fn(),
    ) -> Result<Self> {
        let snapshot = store.snapshot()?;
        let mut parts: Vec<_> = snapshot
            .parts
            .iter()
            .filter(|p| p.source == source)
            .collect();
        if parts.is_empty() {
            return Self::new(store, source);
        }
        if source != "microphone" {
            return Err("Only an ended microphone may reconnect in a running capture".into());
        }
        parts.sort_by_key(|p| p.start_sample);
        let mut end = 0;
        for part in parts {
            if !part.audio_closed
                || part.sample_count != part.confirmed_sample_count
                || part.start_sample != end
            {
                return Err("Microphone audio is not completely closed and contiguous; original files are retained".into());
            }
            verify_confirmed(&store.audio_path(part)?, part)?;
            progress();
            end = end
                .checked_add(part.confirmed_sample_count)
                .ok_or("Microphone timeline overflow")?;
        }
        Ok(Self {
            chunk_samples: u64::from(snapshot.chunk_seconds) * u64::from(SAMPLE_RATE),
            store,
            source: source.into(),
            written: end,
            chunk: None,
            last_flush: Instant::now(),
        })
    }
    pub fn new(store: SessionStore, source: &str) -> Result<Self> {
        if !["system", "microphone"].contains(&source) {
            return Err("Invalid audio source".into());
        }
        let snapshot = store.snapshot()?;
        if snapshot.parts.iter().any(|part| part.source == source) {
            return Err("Track already contains audio; recording resumption requires an explicit new session".into());
        }
        Ok(Self {
            chunk_samples: u64::from(snapshot.chunk_seconds) * u64::from(SAMPLE_RATE),
            store,
            source: source.into(),
            written: 0,
            chunk: None,
            last_flush: Instant::now(),
        })
    }

    pub fn written_samples(&self) -> u64 {
        self.written
    }

    pub fn append(&mut self, pcm: &[i16]) -> Result<()> {
        if pcm.len() > SAMPLE_RATE as usize * 5 {
            return Err("Audio packet exceeds the bounded five-second limit".into());
        }
        let mut input = pcm;
        while !input.is_empty() {
            if self.chunk.is_none() {
                self.create_chunk()?;
            }
            let chunk = self.chunk.as_mut().ok_or("Missing audio chunk")?;
            let count = input
                .len()
                .min((self.chunk_samples - chunk.part.sample_count) as usize);
            let mut quiet_boundary = None;
            if self.store.quiet_sections {
                let search_from = QUIET_SEARCH_AFTER.saturating_sub(chunk.part.sample_count);
                for (index, sample) in input[..count].iter().enumerate().skip(search_from as usize)
                {
                    if sample.unsigned_abs() <= QUIET_AMPLITUDE {
                        chunk.quiet_samples += 1;
                        if chunk.quiet_samples == QUIET_SAMPLES {
                            quiet_boundary = Some(index + 1);
                            break;
                        }
                    } else {
                        chunk.quiet_samples = 0;
                    }
                }
            }
            let count = quiet_boundary.unwrap_or(count);
            for sample in &input[..count] {
                chunk.writer.write_sample(*sample)?;
                chunk.digest.update(sample.to_le_bytes());
                chunk.part.has_signal |= *sample != 0;
            }
            chunk.part.sample_count += count as u64;
            self.written += count as u64;
            input = &input[count..];
            if quiet_boundary.is_some() || chunk.part.sample_count == self.chunk_samples {
                self.close_chunk()?;
            } else {
                self.checkpoint_if_due()?;
            }
        }
        Ok(())
    }

    pub fn append_pcm_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if !bytes.len().is_multiple_of(2) || bytes.len() > SAMPLE_RATE as usize * 2 * 5 {
            return Err("Invalid or oversized PCM packet".into());
        }
        // WASAPI conversion already outputs i16. This helper is for native pipes/tests.
        let values: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|p| i16::from_le_bytes([p[0], p[1]]))
            .collect();
        self.append(&values)
    }

    pub fn pad_to(&mut self, sample: u64) -> Result<()> {
        if sample > self.written.saturating_add(u64::from(SAMPLE_RATE) * 300) {
            return Err("Unexpectedly large timestamp gap; recording requires attention".into());
        }
        let zeros = [0_i16; 1600];
        while self.written < sample {
            let count = (sample - self.written).min(zeros.len() as u64) as usize;
            self.append(&zeros[..count])?;
        }
        Ok(())
    }

    pub fn checkpoint_if_due(&mut self) -> Result<()> {
        if self.last_flush.elapsed() >= CHECKPOINT_INTERVAL {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        if let Some(chunk) = &mut self.chunk {
            chunk.writer.flush()?;
            chunk.part.confirmed_sample_count = chunk.part.sample_count;
            chunk.part.confirmed_sha256 = format!("{:x}", chunk.digest.clone().finalize());
            self.store.audio_checkpoint(chunk.part.clone())?;
        }
        self.last_flush = Instant::now();
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.close_chunk()
    }

    fn create_chunk(&mut self) -> Result<()> {
        let file_name = format!("{}-{:012}.wav", self.source, self.written);
        let part = AudioPart {
            file: file_name,
            source: self.source.clone(),
            start_sample: self.written,
            sample_count: 0,
            confirmed_sample_count: 0,
            has_signal: false,
            audio_closed: false,
            status: "pending".into(),
            text: None,
            committed_text: String::new(),
            tentative_text: String::new(),
            processed_samples: 0,
            model_id: None,
            error: None,
            confirmed_sha256: String::new(),
            transcript_sequence: 0,
        };
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(self.store.audio_path(&part)?)?;
        let writer = hound::WavWriter::new(
            BufWriter::with_capacity(64 * 1024, SyncFile(file)),
            hound::WavSpec {
                channels: 1,
                sample_rate: SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        self.chunk = Some(Chunk {
            writer,
            part,
            digest: Sha256::new(),
            quiet_samples: 0,
        });
        Ok(())
    }

    fn close_chunk(&mut self) -> Result<()> {
        if let Some(mut chunk) = self.chunk.take() {
            // Propagate finalization failures. Drop is intentionally not relied on for acknowledgments.
            chunk.writer.finalize()?;
            chunk.part.audio_closed = true;
            chunk.part.confirmed_sample_count = chunk.part.sample_count;
            chunk.part.confirmed_sha256 = format!("{:x}", chunk.digest.finalize());
            self.store.audio_checkpoint(chunk.part)?;
        }
        self.last_flush = Instant::now();
        Ok(())
    }
}

pub fn read_pcm_samples(path: &Path, offset: u64, count: usize) -> Result<Vec<f32>> {
    if count > SAMPLE_RATE as usize {
        return Err("ASR may read at most one second per packet".into());
    }
    let mut reader = hound::WavReader::open(path)?;
    verify_spec(reader.spec())?;
    reader.seek(u32::try_from(offset)?)?;
    Ok(reader
        .samples::<i16>()
        .take(count)
        .map(|value| value.map(|sample| f32::from(sample) / 32768.0))
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(crate) fn verify_confirmed(path: &Path, part: &AudioPart) -> Result<()> {
    let mut reader = hound::WavReader::open(path)?;
    verify_spec(reader.spec())?;
    if u64::from(reader.duration()) < part.confirmed_sample_count {
        return Err("Confirmed audio is shorter than its checkpoint".into());
    }
    let mut digest = Sha256::new();
    for sample in reader
        .samples::<i16>()
        .take(usize::try_from(part.confirmed_sample_count)?)
    {
        digest.update(sample?.to_le_bytes());
    }
    if format!("{:x}", digest.finalize()) != part.confirmed_sha256 {
        return Err("Confirmed audio SHA-256 mismatch".into());
    }
    Ok(())
}

fn verify_spec(spec: hound::WavSpec) -> Result<()> {
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("Audio must be 16-kHz mono signed 16-bit PCM".into());
    }
    Ok(())
}
