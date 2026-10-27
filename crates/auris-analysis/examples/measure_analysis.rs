//! Deterministic CPU analysis baseline; no file downloads or learned scoring.

use auris_analysis::{
    AnalysisControl,
    audio::{ANALYSIS_RATE, analyze_audio},
};
use auris_core::AudioBuffer;
use std::time::Instant;

fn main() {
    let seconds = 180.0;
    // Repeated A4, 120 BPM, 300 ms sounding notes. The second channel is phase-inverted.
    let samples: Vec<f32> = (0..(seconds * ANALYSIS_RATE) as usize)
        .map(|i| {
            let t = i as f64 / ANALYSIS_RATE;
            let phase = t % 0.5;
            let envelope =
                ((0.3 - phase) * 200.0).clamp(0.0, 1.0) * (phase * 200.0).clamp(0.0, 1.0);
            (std::f64::consts::TAU * 440.0 * t).sin() as f32 * envelope as f32 * 0.4
        })
        .collect();
    let audio = AudioBuffer::from_planar(
        vec![samples.iter().map(|x| -*x).collect(), samples],
        ANALYSIS_RATE,
    )
    .unwrap();
    let began = Instant::now();
    let result = analyze_audio(&audio, &AnalysisControl::default()).unwrap();
    let elapsed = began.elapsed().as_secs_f64();
    println!(
        "audio_s={seconds} elapsed_s={elapsed:.3} rtf={:.4} bpm={:?}",
        elapsed / seconds,
        result
            .tempo
            .candidates
            .iter()
            .map(|c| c.bpm)
            .collect::<Vec<_>>()
    );
}
