//! Development worker for anonymous audio-critic calibration through the production path.

use auris_session::audio_review::AudioReviewOptions;
use auris_session::audio_review::assessment::{
    AssessmentRequest, ComparisonAspect, assess_audio_cancellable,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    a: PathBuf,
    b: PathBuf,
    seconds: f64,
    aspect: ComparisonAspect,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("Expected request JSON path")?;
    let output = args.next().ok_or("Expected response JSON path")?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let result = AudioReviewOptions::from_env().and_then(|options| {
        assess_audio_cancellable(
            &[("A", &request.a), ("B", &request.b)],
            &AssessmentRequest::Comparison(request.aspect),
            request.seconds,
            &options,
            &AtomicBool::new(false),
        )
    });
    let value = match result {
        Ok(reply) => serde_json::to_value(reply)?,
        Err(error) => serde_json::json!({"error":error}),
    };
    std::fs::write(output, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}
