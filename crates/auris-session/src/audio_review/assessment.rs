//! Structured, fallible audio judgments. Comparison requests carry no edit narrative.

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

/// The recording supporting an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcerptLabel {
    /// The single diagnostic recording.
    Excerpt,
    /// The first anonymous recording.
    A,
    /// The second anonymous recording.
    B,
    /// Both anonymous recordings.
    Both,
}

/// A model-reported audible observation, with excerpt-relative evidence times.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Which recording supports this observation.
    pub excerpt: ExcerptLabel,
    /// Start in seconds relative to that recording, not project time.
    pub start_seconds: f64,
    /// End in seconds relative to that recording.
    pub end_seconds: f64,
    /// Audible evidence, not editing instructions.
    pub detail: String,
}

/// Whether a diagnostic response reports any perception.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    /// The model reports audible evidence; this is not verified perception.
    Observed,
    /// The model cannot assess the recording.
    CannotJudge,
}

/// A preference is explicitly distinct from a tie or an inability to judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The model prefers anonymous A on the requested criterion.
    PreferA,
    /// The model prefers anonymous B on the requested criterion.
    PreferB,
    /// The model can assess both and reports no preference.
    Tie,
    /// Insufficient perceptual evidence to make a comparison.
    CannotJudge,
}

/// Self-reported confidence, not a calibrated probability.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Weak or ambiguous evidence.
    Low,
    /// Some supporting evidence.
    Medium,
    /// Strong evidence according to the model, still requiring calibration.
    High,
}

/// Validated response shape, separate from the truth of its musical claims.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assessment {
    /// Descriptive feedback on one recording.
    Diagnostic {
        /// Whether the model reports usable observations.
        status: DiagnosticStatus,
        /// At most eight bounded observations.
        observations: Vec<Observation>,
        /// Remaining uncertainty, which may be empty for a confident observation.
        uncertainty: String,
        /// Optional local change, never an instruction to execute automatically.
        suggestion: Option<String>,
    },
    /// Blind comparison on a typed criterion.
    Comparison {
        /// Preference, tie or inability to judge.
        verdict: Verdict,
        /// Uncalibrated self-report.
        confidence: Confidence,
        /// At most eight bounded observations.
        observations: Vec<Observation>,
        /// Reason for the verdict.
        reason: String,
        /// Remaining uncertainty.
        uncertainty: String,
    },
}

/// Audio delivery and response validity are reported independently.
#[derive(Debug, Serialize)]
pub struct AssessmentReply {
    /// Configured reviewer model.
    pub model: String,
    /// WAV bytes were delivered; not proof of correct listening.
    pub audio_sent: bool,
    /// Parsed judgment, absent for an invalid response.
    pub assessment: Option<Assessment>,
    /// A schema or evidence validation failure, never a musical verdict.
    pub assessment_error: Option<String>,
    /// Original untrusted response retained for diagnosis and calibration.
    pub raw_response: String,
}

fn prompt(request: &AssessmentRequest<'_>, seconds: f64) -> String {
    let common = format!(
        "Return only one JSON object, with no markdown. Listen to the supplied audio. Times are relative to each excerpt and must satisfy 0 <= start_seconds < end_seconds <= {seconds:.6}. Use at most eight observations. Each observation has exactly {{\"excerpt\":LABEL,\"start_seconds\":NUMBER,\"end_seconds\":NUMBER,\"detail\":STRING}}. Report uncertainty. If audio perception is unavailable, use cannot_judge; do not infer music from this prompt. "
    );
    match request {
        AssessmentRequest::Diagnostic(question) => format!(
            "{common}Describe one excerpt. Use label \"excerpt\". Schema: {{\"kind\":\"diagnostic\",\"status\":\"observed\" or \"cannot_judge\",\"observations\":[],\"uncertainty\":STRING,\"suggestion\":STRING or null}}. Observed requires audible evidence. A suggestion is optional and must be supported by observations. Question: {question}"
        ),
        AssessmentRequest::Comparison(aspect) => format!(
            "{common}Compare anonymous A and B on {}. Neither position implies a newer or preferred version. Do not propose edits or guess their history. Use labels \"a\", \"b\", or \"both\". Schema: {{\"kind\":\"comparison\",\"verdict\":\"prefer_a\" or \"prefer_b\" or \"tie\" or \"cannot_judge\",\"confidence\":\"low\" or \"medium\" or \"high\",\"observations\":[],\"reason\":STRING,\"uncertainty\":STRING}}. Preferences require audible evidence; tie means assessable but no preference. Confidence is a self-report, not a calibrated probability.",
            serde_json::to_string(aspect).expect("enum serializes")
        ),
    }
}

fn parse(text: &str, request: &AssessmentRequest<'_>, seconds: f64) -> Result<Assessment, String> {
    let assessment: Assessment = serde_json::from_str(text)
        .map_err(|_| "Critic returned invalid assessment JSON".to_string())?;
    let (observations, text_fields) = match (&assessment, request) {
        (
            Assessment::Diagnostic {
                status,
                observations,
                uncertainty,
                suggestion,
            },
            AssessmentRequest::Diagnostic(_),
        ) => {
            if *status == DiagnosticStatus::Observed && observations.is_empty() {
                return Err("Observed diagnosis requires timestamped evidence".into());
            }
            if *status == DiagnosticStatus::CannotJudge
                && (uncertainty.trim().is_empty() || suggestion.is_some())
            {
                return Err("Cannot-judge diagnosis requires uncertainty and no suggestion".into());
            }
            (
                observations,
                vec![uncertainty.as_str(), suggestion.as_deref().unwrap_or("")],
            )
        }
        (
            Assessment::Comparison {
                verdict,
                observations,
                reason,
                uncertainty,
                ..
            },
            AssessmentRequest::Comparison(_),
        ) => {
            if reason.trim().is_empty()
                || (matches!(verdict, Verdict::PreferA | Verdict::PreferB)
                    && observations.is_empty())
                || (*verdict == Verdict::CannotJudge && uncertainty.trim().is_empty())
            {
                return Err("Comparison requires a reason, evidence for preference and uncertainty when unable to judge".into());
            }
            (observations, vec![reason.as_str(), uncertainty.as_str()])
        }
        _ => return Err("Critic response does not match the requested assessment mode".into()),
    };
    if observations.len() > 8 || text_fields.iter().any(|text| text.len() > 4000) {
        return Err("Critic assessment exceeds its evidence or text limit".into());
    }
    for observation in observations {
        let label_valid = match request {
            AssessmentRequest::Diagnostic(_) => observation.excerpt == ExcerptLabel::Excerpt,
            AssessmentRequest::Comparison(_) => observation.excerpt != ExcerptLabel::Excerpt,
        };
        if !label_valid
            || !observation.start_seconds.is_finite()
            || !observation.end_seconds.is_finite()
            || observation.start_seconds < 0.0
            || observation.end_seconds <= observation.start_seconds
            || observation.end_seconds > seconds
            || observation.detail.trim().is_empty()
            || observation.detail.len() > 4000
        {
            return Err("Critic evidence has an invalid excerpt, time range or detail".into());
        }
    }
    Ok(assessment)
}

/// Submits audio for structured diagnosis or anonymous comparison.
///
/// Invalid model JSON remains a delivered response with no usable judgment. This preserves
/// evidence for calibration without converting a schema failure into a tie or preference.
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
    let reply = review_audio_cancellable(audio, &prompt(request, seconds), options, cancel)?;
    let (assessment, assessment_error) = match parse(&reply.review, request, seconds) {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    Ok(AssessmentReply {
        model: reply.model,
        audio_sent: reply.audio_sent,
        assessment,
        assessment_error,
        raw_response: reply.review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comparison() -> serde_json::Value {
        json!({"kind":"comparison","verdict":"prefer_a","confidence":"low",
            "observations":[{"excerpt":"both","start_seconds":0.0,"end_seconds":2.0,"detail":"A leaves clearer gaps."}],
            "reason":"Less overlap.","uncertainty":"Timbre may affect the judgment."})
    }

    #[test]
    fn preferences_require_valid_evidence_and_cannot_judge_is_distinct_from_tie() {
        let request = AssessmentRequest::Comparison(ComparisonAspect::Phrasing);
        let valid = comparison();
        assert!(parse(&valid.to_string(), &request, 2.0).is_ok());
        for change in [
            json!({"observations":[]}),
            json!({"reason":""}),
            json!({"verdict":"better"}),
            json!({"unknown":true}),
        ] {
            let mut invalid = valid.clone();
            invalid
                .as_object_mut()
                .unwrap()
                .extend(change.as_object().unwrap().clone());
            assert!(parse(&invalid.to_string(), &request, 2.0).is_err());
        }
        for verdict in ["tie", "cannot_judge"] {
            let mut value = valid.clone();
            value["verdict"] = verdict.into();
            value["observations"] = json!([]);
            assert!(parse(&value.to_string(), &request, 2.0).is_ok());
        }
        for end in [-1.0, 0.0, 2.1] {
            let mut value = valid.clone();
            value["observations"][0]["end_seconds"] = end.into();
            assert!(parse(&value.to_string(), &request, 2.0).is_err());
        }
        assert!(parse("The revised version is better.", &request, 2.0).is_err());
        assert!(
            parse(
                &valid.to_string(),
                &AssessmentRequest::Diagnostic("Describe"),
                2.0
            )
            .is_err()
        );
    }
}
