//! Full-length, bounded native audio payload preparation without changing the source.

use crate::{AnalysisControl, SessionError};
use std::{io::Cursor, path::Path};

/// A private analysis copy whose temporary file is removed when its owner drops it.
pub struct AudioAttachmentCopy {
    file: tempfile::NamedTempFile,
    /// Number of exact original bytes copied.
    pub bytes: u64,
    /// SHA-256 of the copied original bytes, not a decoded/resampled representation.
    pub sha256: String,
}

impl AudioAttachmentCopy {
    /// Local analysis path; this is separate from the host's immutable snapshot.
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

/// Copy an attached original on a worker, retaining its format and bounded memory.
///
/// The caller owns access and lifetime. No resampling or channel conversion occurs.
/// At most 512 MiB are copied in cancellable chunks; errors remove the partial copy.
pub fn copy_audio_attachment(
    path: &Path,
    control: &AnalysisControl,
) -> Result<AudioAttachmentCopy, SessionError> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let failure = |message: &str| SessionError::MusicAnalysis(message.into());
    let check = || {
        if control.is_cancelled() {
            Err(failure("Audio reference export cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let mut source = std::fs::File::open(path).map_err(|e| failure(&e.to_string()))?;
    const LIMIT: u64 = 512 * 1024 * 1024;
    let metadata = source.metadata().map_err(|e| failure(&e.to_string()))?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(failure(
            "audio reference must be a regular file at most 512 MiB",
        ));
    }
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("wav");
    let mut file = tempfile::Builder::new()
        .prefix("auris-analysis-")
        .suffix(&format!(".{extension}"))
        .tempfile()
        .map_err(|e| failure(&e.to_string()))?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        check()?;
        let count = source
            .read(&mut buffer)
            .map_err(|e| failure(&e.to_string()))?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > LIMIT {
            return Err(failure("audio reference exceeds 512 MiB"));
        }
        file.write_all(&buffer[..count])
            .map_err(|e| failure(&e.to_string()))?;
        hash.update(&buffer[..count]);
    }
    file.flush().map_err(|e| failure(&e.to_string()))?;
    check()?;
    Ok(AudioAttachmentCopy {
        file,
        bytes,
        sha256: hex::encode(hash.finalize()),
    })
}

/// Prepares a full-length 16 kHz PCM16 WAV for an audio-capable model on a worker thread.
///
/// Preserves mono/stereo channels and source timing; no excerpt is silently selected. The
/// original file is never modified. The ordinary decoder's 512 MiB sample allocation budget
/// applies, followed by a thirty-minute/two-channel bound before resampling. Cancellation is
/// checked before and after synchronous decoding/resampling and during sample encoding.
pub fn prepare_audio_attachment(
    path: &Path,
    control: &AnalysisControl,
) -> Result<Vec<u8>, SessionError> {
    let failure = |message: &str| SessionError::MusicAnalysis(message.to_string());
    let check = || {
        if control.is_cancelled() {
            Err(failure("Audio attachment preparation cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let source = auris_io::decode_audio_file(path)?.buffer;
    check()?;
    if source.channel_count() > 2 || source.duration_seconds() > 1800.0 {
        return Err(failure(
            "Native audio preparation accepts at most thirty minutes and mono/stereo audio; use a local CLI reference or convert the source",
        ));
    }
    let audio = auris_io::resample_buffer(&source, 16_000.0)?;
    drop(source);
    check()?;
    let mut output = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut output,
        hound::WavSpec {
            channels: audio.channel_count() as u16,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| failure(&e.to_string()))?;
    for frame in 0..audio.frame_count() {
        if frame.is_multiple_of(4096) {
            check()?;
        }
        for channel in audio.iter_channels() {
            let sample = channel[frame];
            if !sample.is_finite() {
                return Err(failure("Audio attachment contains non-finite samples"));
            }
            writer
                .write_sample((sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
                .map_err(|e| failure(&e.to_string()))?;
        }
    }
    writer.finalize().map_err(|e| failure(&e.to_string()))?;
    check()?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(channels: u16, rate: u32, frames: u32) -> tempfile::NamedTempFile {
        let mut file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let mut writer = hound::WavWriter::new(
            file.as_file_mut(),
            hound::WavSpec {
                channels,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..frames * u32::from(channels) {
            writer.write_sample(12000_i16).unwrap();
        }
        writer.finalize().unwrap();
        file
    }

    #[test]
    fn analysis_copy_preserves_original_bytes_and_cannot_modify_the_snapshot() {
        let source = wav(2, 48000, 48000);
        let original = std::fs::read(source.path()).unwrap();
        let copy = copy_audio_attachment(source.path(), &Default::default()).unwrap();
        assert_ne!(copy.path(), source.path());
        assert_eq!(std::fs::read(copy.path()).unwrap(), original);
        assert_eq!(copy.bytes, original.len() as u64);
        assert_eq!(copy.sha256.len(), 64);
        let path = copy.path().to_owned();
        std::fs::write(&path, b"changed by external analysis").unwrap();
        assert_eq!(std::fs::read(source.path()).unwrap(), original);
        drop(copy);
        assert!(!path.exists());
        let control = AnalysisControl::default();
        control.cancel();
        assert!(copy_audio_attachment(source.path(), &control).is_err());
    }

    #[test]
    fn preparation_preserves_full_mono_duration_and_signal_without_changing_the_file() {
        let file = wav(1, 32000, 16000);
        let before = std::fs::read(file.path()).unwrap();
        let bytes = prepare_audio_attachment(file.path(), &Default::default()).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 16000);
        assert_eq!(reader.duration(), 8000);
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert!((i32::from(samples[4000]) - 12000).abs() < 4);
        assert_eq!(std::fs::read(file.path()).unwrap(), before);
    }

    #[test]
    fn preparation_refuses_incompatible_duration_channels_corrupt_files_and_cancellation() {
        for file in [wav(3, 16000, 16), wav(1, 16, 16 * 1801)] {
            let error = prepare_audio_attachment(file.path(), &Default::default()).unwrap_err();
            assert!(error.to_string().contains("thirty minutes"), "{error}");
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(prepare_audio_attachment(file.path(), &Default::default()).is_err());
        let control = AnalysisControl::default();
        control.cancel();
        assert!(
            prepare_audio_attachment(Path::new("missing.wav"), &control)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
}
