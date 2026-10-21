//! Full-length, bounded native audio payload preparation without changing the source.

use crate::{AnalysisControl, SessionError};
use std::{io::Cursor, path::Path};

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
