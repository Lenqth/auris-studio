//! Attenuation before delivery encoding, measured from the captured waveform.

use super::*;
use auris_core::param::{db_to_gain, gain_to_db};
use auris_gpu::analysis::analyze_loudness_cpu;

/// Render each source once, then encode all files with one measured attenuation.
pub(super) fn stage(
    job: &mut crate::RenderJob,
    targets: Vec<(PathBuf, Option<TrackId>)>,
    options: &OfflineOptions,
    settings: WavExportSettings,
    ceiling_db: f32,
    cancelled: &AtomicBool,
) -> Result<Vec<StagedAudio>, String> {
    let check = || {
        if cancelled.load(Ordering::Relaxed) {
            Err("File operation cancelled".to_string())
        } else {
            Ok(())
        }
    };
    let mut captures = Vec::with_capacity(targets.len());
    let mut peak = 0.0f32;
    for (path, track) in targets {
        check()?;
        let mut progress = RenderProgress::default().cancelled_by(cancelled);
        let audio = job
            .render_target(track, options, &mut progress)
            .map_err(|e| e.to_string())?;
        check()?;
        if audio
            .iter_channels()
            .flatten()
            .any(|sample| !sample.is_finite())
        {
            return Err("Rendered audio contains non-finite samples".into());
        }
        let measured = analyze_loudness_cpu(&audio);
        peak = peak.max(measured.true_peak_estimate);
        let source_true_peak_db = measured.true_peak_db();
        // Preserve over-range floats, not already clipped PCM. Private stages avoid
        // rerendering stateful plugins and keep only one duration-sized buffer in memory.
        let float_settings = WavExportSettings {
            bit_depth: WavBitDepth::Float32,
            sample_rate: audio.sample_rate().round() as u32,
            dither: false,
        };
        let mut writer = auris_io::AudioExportWriter::create(
            &path,
            audio.channel_count(),
            audio.frame_count(),
            &float_settings.into(),
        )
        .map_err(|e| e.to_string())?;
        writer.write(&audio).map_err(|e| e.to_string())?;
        check()?;
        captures.push((
            path,
            writer.finish_staged().map_err(|e| e.to_string())?,
            source_true_peak_db,
        ));
    }
    let step = match settings.bit_depth {
        WavBitDepth::Int16 => 1.0 / 32768.0,
        WavBitDepth::Int24 => 1.0 / 8388608.0,
        WavBitDepth::Float32 => f32::EPSILON,
    };
    // Reserve a few encoding steps for quantization/dither, float multiplication
    // and the true-peak interpolator's signed weights. This is not a limiter.
    let ceiling = db_to_gain(ceiling_db) - 4.0 * step.max(f32::EPSILON);
    let gain = if peak > ceiling { ceiling / peak } else { 1.0 };
    let attenuation_db = gain_to_db(gain);
    let mut staged = Vec::with_capacity(captures.len());
    for (path, captured, source_true_peak_db) in captures {
        check()?;
        let mut audio = auris_io::decode_audio_file(captured.staged_path())
            .map_err(|e| e.to_string())?
            .buffer;
        check()?;
        audio.apply_gain(gain);
        let settings = WavExportSettings {
            sample_rate: audio.sample_rate().round() as u32,
            ..settings
        };
        let mut writer = auris_io::AudioExportWriter::create(
            &path,
            audio.channel_count(),
            audio.frame_count(),
            &settings.into(),
        )
        .map_err(|e| e.to_string())?;
        writer.write(&audio).map_err(|e| e.to_string())?;
        let encoded = writer.finish_staged().map_err(|e| e.to_string())?;
        drop(audio);
        check()?;
        let inspection = crate::inspect_export(encoded.staged_path()).map_err(|e| e.to_string())?;
        let source_inspection =
            Some(crate::inspect_export(captured.staged_path()).map_err(|e| e.to_string())?);
        if inspection.true_peak_db > ceiling_db || inspection.full_scale_samples != 0 {
            return Err(
                "Encoded delivery exceeds its peak ceiling; no files were published".into(),
            );
        }
        check()?;
        staged.push(StagedAudio {
            path,
            encoded,
            inspection,
            source_true_peak_db,
            attenuation_db,
            ceiling_db,
            source_inspection,
        });
    }
    Ok(staged)
}
