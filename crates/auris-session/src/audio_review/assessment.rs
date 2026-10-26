//! Short, fallible audio observations. Comparison requests carry no edit narrative.

use super::{AudioReviewOptions, review_audio_cancellable};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// A neutral comparison criterion, shared by every agent transport.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonAspect {
    /// Foreground audibility relative to supporting parts.
    Balance,
    /// Note articulation and breathing between musical phrases.
    Phrasing,
    /// Cooperation between simultaneous musical parts.
    Ensemble,
    /// Audible distortion, noise and tonal comfort.
    Sound,
    /// Coherence of the musical passage as a whole.
    MusicalCoherence,
}

/// Distinct diagnostic and blind comparison requests.
pub enum AssessmentRequest<'a> {
    /// One excerpt and an optional descriptive question.
    Diagnostic(&'a str),
    /// Two anonymously labeled excerpts and a neutral, typed criterion.
    Comparison(ComparisonAspect),
}

/// Validation state for a review response.
///
/// Delivery of WAV bytes is tracked separately by `audio_sent`; it never upgrades this
/// unvalidated response to an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStatus {
    /// Non-empty plain text was received, but this API does not validate claims or judgments.
    UnvalidatedResponse,
}

/// Audio delivery and untrusted prose returned by the reviewer.
#[derive(Debug, Serialize)]
pub struct AssessmentReply {
    /// Configured reviewer model.
    pub model: String,
    /// WAV bytes were delivered; not proof of correct listening.
    pub audio_sent: bool,
    /// Validation state for the review response; this is never a preference or quality score.
    pub status: AssessmentStatus,
    /// Capability discovery and fallback metadata.
    pub selection: super::ReviewerSelection,
    /// Original response, without extracting a verdict or confidence.
    pub raw_response: String,
}

fn prompt(request: &AssessmentRequest<'_>) -> String {
    match request {
        AssessmentRequest::Diagnostic(question) => {
            format!(
                "{} Reply in 2–4 short sentences, stating uncertainty where needed.",
                question.trim()
            )
        }
        AssessmentRequest::Comparison(aspect) => {
            let focus = match aspect {
                ComparisonAspect::Balance => "the balance of foreground and supporting parts",
                ComparisonAspect::Phrasing => "articulation and breathing between phrases",
                ComparisonAspect::Ensemble => "how the musical parts work together",
                ComparisonAspect::Sound => "distortion, noise and tonal comfort",
                ComparisonAspect::MusicalCoherence => "musical coherence",
            };
            format!(
                "Describe {focus} in each excerpt. Reply in 2–4 short sentences, stating uncertainty where needed."
            )
        }
    }
}

/// Submits audio for a short free-text diagnosis or anonymous comparison.
///
/// The response is retained verbatim and never interpreted as a machine-readable verdict.
pub fn assess_audio_cancellable(
    audio: &[(&str, &Path)],
    request: &AssessmentRequest<'_>,
    seconds: f64,
    options: &AudioReviewOptions,
    cancel: &AtomicBool,
) -> Result<AssessmentReply, String> {
    if !seconds.is_finite() || !(0.0..=30.0).contains(&seconds) || seconds == 0.0 {
        return Err("Assessment duration must be positive and at most 30 seconds".into());
    }
    let labels: Vec<_> = audio.iter().map(|(label, _)| *label).collect();
    let valid = match request {
        AssessmentRequest::Diagnostic(question) => {
            labels == ["Excerpt"] && !question.trim().is_empty() && question.len() <= 4000
        }
        AssessmentRequest::Comparison(_) => labels == ["A", "B"],
    };
    if !valid {
        return Err(
            "Use Excerpt for diagnosis or anonymous A/B for comparison, and a bounded question"
                .into(),
        );
    }
    let reply = review_audio_cancellable(audio, &prompt(request), options, cancel)?;
    // The current API intentionally asks for bounded free text and does not validate a
    // structured assessment schema. Preserve the prose, but never call it an observation.
    Ok(AssessmentReply {
        model: reply.model,
        selection: reply.selection,
        audio_sent: reply.audio_sent,
        raw_response: reply.review,
        status: AssessmentStatus::UnvalidatedResponse,
    })
}
