//! Audio-model review of rendered excerpts, shared by every frontend.
//!
//! This blocking command uploads WAV bytes to a configured audio-capable model. It never
//! supplies editing tools to the reviewer; the calling frontend decides how to act on its
//! observations. Run it on a worker thread, independently of realtime audio processing.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};

pub mod assessment;
mod selection;

const MAX_AUDIO_BYTES: u64 = 25 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Connection settings for a separate model that accepts actual audio input.
#[derive(Clone)]
pub struct AudioReviewOptions {
    /// Whether a reviewer endpoint or model was explicitly configured.
    pub configured: bool,
    /// Discover a reachable audio-input model when the configured endpoint is loopback.
    pub auto_select: bool,
    /// OpenAI-compatible API base URL, including `/v1` when required.
    pub url: String,
    /// Exact model identifier used by the audio endpoint.
    pub model: String,
    /// Optional bearer credential; never included in diagnostic output.
    pub api_key: Option<String>,
    /// Ollama root URL for explicit audio-capability preflight, when using Ollama.
    pub ollama_url: Option<String>,
}

impl Default for AudioReviewOptions {
    fn default() -> Self {
        Self {
            configured: false,
            auto_select: true,
            url: "http://localhost:11434/v1".into(),
            model: "gemma4:e2b".into(),
            api_key: None,
            ollama_url: Some("http://localhost:11434".into()),
        }
    }
}

impl AudioReviewOptions {
    /// Reads `AURIS_AUDIO_URL`, `AURIS_AUDIO_MODEL`, `AURIS_AUDIO_API_KEY_ENV` and
    /// `AURIS_AUDIO_AUTO_SELECT` (defaults to true for local capability discovery).
    ///
    /// The key setting names an environment variable holding the credential. An Ollama URL
    /// on port 11434 gets capability preflight automatically; `AURIS_AUDIO_OLLAMA_URL` can
    /// supply a different Ollama origin. Other compatible endpoints must support audio input.
    pub fn from_env() -> Result<Self, String> {
        Self::from_values(&|name| std::env::var(name).ok())
    }

    fn from_values(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut options = Self {
            configured: [
                "AURIS_AUDIO_URL",
                "AURIS_AUDIO_MODEL",
                "AURIS_AUDIO_OLLAMA_URL",
            ]
            .iter()
            .any(|name| env(name).is_some_and(|value| !value.trim().is_empty())),
            ..Self::default()
        };
        if let Some(url) = env("AURIS_AUDIO_URL") {
            options.url = url.trim().trim_end_matches('/').to_string();
            let parsed = url::Url::parse(&options.url).map_err(|error| error.to_string())?;
            options.ollama_url = (parsed.port() == Some(11434)
                && parsed.path().trim_end_matches('/') == "/v1")
                .then(|| parsed.origin().ascii_serialization());
        }
        if let Some(model) = env("AURIS_AUDIO_MODEL") {
            options.model = model.trim().to_string();
        }
        if let Some(url) = env("AURIS_AUDIO_OLLAMA_URL") {
            options.ollama_url = Some(url.trim().trim_end_matches('/').to_string());
        }
        if let Some(variable) = env("AURIS_AUDIO_API_KEY_ENV") {
            options.api_key = Some(
                env(&variable)
                    .ok_or_else(|| format!("audio API key variable '{variable}' is not set"))?,
            );
        }
        if let Some(value) = env("AURIS_AUDIO_AUTO_SELECT") {
            options.auto_select = match value.trim() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => return Err("AURIS_AUDIO_AUTO_SELECT must be true or false".into()),
            };
        }
        options.validate()?;
        Ok(options)
    }

    fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("AURIS_AUDIO_MODEL must name an audio-capable model".into());
        }
        for url in std::iter::once(&self.url).chain(self.ollama_url.iter()) {
            let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
            if !matches!(parsed.scheme(), "http" | "https")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
            {
                return Err(
                    "audio API URL must use HTTP(S), without credentials, query or fragment".into(),
                );
            }
        }
        Ok(())
    }

    pub(crate) fn prepare(
        &self,
        cancel: &AtomicBool,
    ) -> Result<(Self, ReviewerSelection, bool), String> {
        self.validate()?;
        let mut fixed = self.clone();
        if crate::audio_input::AudioInputContract::from_env()?.is_some() {
            fixed.auto_select = false;
        }
        let (mut selected, report) = selection::select(&fixed, cancel)?;
        let confirm = selection::needs_load_confirmation(&selected, cancel);
        // Keep the approved endpoint/model fixed through rendering and upload.
        selected.auto_select = false;
        Ok((selected, report, confirm))
    }
}

/// Selection metadata; audio support does not establish musical accuracy.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReviewerSelection {
    /// Whether local capability discovery was used.
    pub automatic: bool,
    /// Whether a different endpoint or model was chosen.
    pub fallback: bool,
    /// Model requested before discovery.
    pub requested_model: String,
}

/// A successful response to an audio upload, independent of any project edits.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioReview {
    /// Configured model that produced the response.
    pub model: String,
    /// How the endpoint was selected, including whether a fallback was used.
    pub selection: ReviewerSelection,
    /// The model's observations and suggested local changes.
    pub review: String,
    /// True when WAV bytes were submitted and a nonempty response was returned.
    /// This records delivery, not a measurement of the model's perceptual accuracy.
    pub audio_sent: bool,
    /// Negotiated transfer metadata; absent for legacy endpoints with no precise agreement.
    pub transfer: Option<Value>,
}

/// Submits one excerpt or a labeled pair for auditory comparison.
///
/// WAV payloads have a combined 25 MiB ceiling. For local Ollama, advertised audio capability
/// is required before uploading anything. Requests have a five-minute timeout, bounded response
/// bodies and no redirects. Unsupported audio and empty/refused responses are errors, never a
/// text-only substitute for listening. Paths themselves are not supplied to the reviewer.
pub fn review_audio(
    audio: &[(&str, &Path)],
    prompt: &str,
    options: &AudioReviewOptions,
) -> Result<AudioReview, String> {
    review_audio_cancellable(audio, prompt, options, &AtomicBool::new(false))
}

/// Submits audio with cancellation checks before and after network stages.
/// An already running HTTP request is bounded by the normal request timeout.
pub fn review_audio_cancellable(
    audio: &[(&str, &Path)],
    prompt: &str,
    options: &AudioReviewOptions,
    cancel: &AtomicBool,
) -> Result<AudioReview, String> {
    let contract = crate::audio_input::AudioInputContract::from_env()?;
    review_audio_with_contract(audio, prompt, options, contract.as_ref(), cancel)
}

/// Upload complete WAV inputs under an explicit agreement, fixing the endpoint/model.
/// No automatic fallback may substitute a model with a different waveform contract.
pub fn review_audio_with_contract(
    audio: &[(&str, &Path)],
    prompt: &str,
    options: &AudioReviewOptions,
    contract: Option<&crate::audio_input::AudioInputContract>,
    cancel: &AtomicBool,
) -> Result<AudioReview, String> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err("Audio review cancelled".to_string())
        } else {
            Ok(())
        }
    };
    check()?;
    options.validate()?;
    let (content, transfer) = audio_content_agreed(audio, prompt, contract)?;
    let mut fixed = options.clone();
    if contract.is_some() {
        fixed.auto_select = false;
    }
    let (selected, selection) = selection::select(&fixed, cancel)?;
    let options = &selected;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(300)))
        .max_redirects(0)
        .build()
        .into();
    if let Some(root) = &options.ollama_url
        && !selection.automatic
    {
        check()?;
        let shown = post_json(
            &agent,
            &format!("{}/api/show", root.trim_end_matches('/')),
            &json!({"model":options.model}),
            options.api_key.as_deref(),
        )?;
        check()?;
        if !shown
            .get("capabilities")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value == "audio"))
        {
            return Err(format!(
                "audio reviewer '{}' does not advertise audio input; configure AURIS_AUDIO_MODEL with an audio-capable model",
                options.model
            ));
        }
    }
    let mut request = json!({
        "model":options.model,
        "messages":[
            {"role":"user","content":content}
        ],
        "stream":false,
        "temperature":0,
        "max_tokens":384
    });
    if options.ollama_url.is_some() {
        request["reasoning_effort"] = "none".into();
    }
    check()?;
    let response = post_json(
        &agent,
        &format!("{}/chat/completions", options.url.trim_end_matches('/')),
        &request,
        options.api_key.as_deref(),
    )?;
    check()?;
    let review = review_text(&response)?;
    Ok(AudioReview {
        model: options.model.clone(),
        selection,
        review,
        audio_sent: true,
        transfer,
    })
}

#[cfg(test)]
fn audio_content(audio: &[(&str, &Path)], prompt: &str) -> Result<Vec<Value>, String> {
    audio_content_agreed(audio, prompt, None).map(|(content, _)| content)
}

fn audio_content_agreed(
    audio: &[(&str, &Path)],
    prompt: &str,
    contract: Option<&crate::audio_input::AudioInputContract>,
) -> Result<(Vec<Value>, Option<Value>), String> {
    if audio.is_empty() || audio.len() > 2 {
        return Err("audio review needs one excerpt or two labeled excerpts for comparison".into());
    }
    if prompt.trim().is_empty() || prompt.len() > 32_000 {
        return Err("audio review prompt must contain 1 to 32000 bytes of text".into());
    }
    if let Some(contract) = contract {
        contract.validate()?;
        if audio.len() > usize::from(contract.max_inputs) {
            return Err("Audio count exceeds the agreed context limit".into());
        }
    }
    let mut remaining = contract.map_or(MAX_AUDIO_BYTES, |c| c.max_total_bytes);
    let mut content = Vec::new();
    let mut transfers = Vec::new();
    for (label, path) in audio {
        if label.trim().is_empty() || label.len() > 256 {
            return Err("audio excerpt labels must contain 1 to 256 bytes of text".into());
        }
        let bytes = if let Some(contract) = contract {
            let (bytes, metadata) = contract.read(path, remaining)?;
            transfers.push(json!({"label":label,"transfer":metadata}));
            bytes
        } else {
            let file = std::fs::File::open(path).map_err(|error| {
                format!("cannot read audio excerpt {}: {error}", path.display())
            })?;
            if file.metadata().map_err(|error| error.to_string())?.len() > remaining {
                return Err("audio excerpts exceed 25 MiB combined; use shorter previews".into());
            }
            let mut bytes = Vec::new();
            file.take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > remaining {
                return Err("audio excerpts exceed 25 MiB combined; use shorter previews".into());
            }
            bytes
        };
        if bytes.len() < 44 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WAVE" {
            return Err(
                "audio review accepts WAV excerpts; render or convert the audio to WAV first"
                    .into(),
            );
        }
        remaining -= bytes.len() as u64;
        content.push(json!({"type":"input_audio","input_audio":{
            "data":base64::engine::general_purpose::STANDARD.encode(bytes),"format":"wav"
        }}));
    }
    // Text before audio matches Gemma4's model-card guidance and the local speech control.
    // Ordinals preserve the A/B labels without putting text between the two audio inputs.
    let labels = audio
        .iter()
        .enumerate()
        .map(|(index, (label, _))| format!("{}. {label}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let text = if audio.len() == 1 {
        prompt.to_string()
    } else {
        format!("Audio excerpts in order:\n{labels}\n\n{prompt}")
    };
    content.insert(0, json!({"type":"text","text":text}));
    let transfer=contract.map(|contract|json!({"contract":contract,"inputs":transfers,"context":"one_request",
        "comparison_mode":if audio.len()==2 && contract.simultaneous_comparison {"simultaneous"}else if audio.len()==2 {"independent_reviews"}else{"single"}}));
    Ok((content, transfer))
}

fn post_json(
    agent: &ureq::Agent,
    url: &str,
    body: &Value,
    key: Option<&str>,
) -> Result<Value, String> {
    let mut request = agent.post(url);
    if let Some(key) = key {
        request = request.header("Authorization", &format!("Bearer {key}"));
    }
    let response = request
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(body)
        .map_err(|_| "[audio_critic_connection_failed] The audio critic request could not complete. Check the server and connection, then retry.".to_string())?;
    response_json(response, key)
}

fn response_json(
    response: ureq::http::Response<ureq::Body>,
    key: Option<&str>,
) -> Result<Value, String> {
    let status = response.status().as_u16();
    if response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err("audio reviewer response exceeds 1 MiB".into());
    }
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read audio review: {error}"))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("audio reviewer response exceeds 1 MiB".into());
    }
    let parsed: Result<Value, _> = serde_json::from_slice(&bytes);
    if !(200..300).contains(&status) {
        let detail = parsed.as_ref().ok().and_then(|body| {
            body.pointer("/error/message")
                .or_else(|| body.get("error"))
                .or_else(|| body.get("message"))
                .and_then(Value::as_str)
        });
        let detail = detail
            .map(|text| {
                let text = match key.filter(|key| !key.is_empty()) {
                    Some(key) => text.replace(key, "[redacted]"),
                    None => text.to_string(),
                };
                let text = text
                    .split_whitespace()
                    .map(|word| {
                        if word.contains("://") {
                            "[URL omitted]"
                        } else {
                            word
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                text.chars().take(512).collect::<String>()
            })
            .unwrap_or_default();
        return Err(format!(
            "audio reviewer returned HTTP {status}{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ));
    }
    parsed.map_err(|error| {
        format!("[audio_critic_invalid_response] invalid audio reviewer response: {error}")
    })
}

fn review_text(response: &Value) -> Result<String, String> {
    let choice = response
        .pointer("/choices/0")
        .ok_or("[audio_critic_invalid_response] audio reviewer returned no choice")?;
    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str)
        && reason != "stop"
    {
        return Err(match reason {
            "length" => "[audio_critic_invalid_response] audio reviewer reached its output-token limit before finishing; no complete listening result is available. Retry with a shorter focus or a request for a briefer review, or increase the audio server's output-token limit.".into(),
            _ => "[audio_critic_invalid_response] audio reviewer did not complete its review; no listening result is available".into(),
        });
    }
    let message = choice
        .get("message")
        .ok_or("[audio_critic_invalid_response] audio reviewer returned no message")?;
    if message
        .get("refusal")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("[audio_critic_invalid_response] audio reviewer refused the request".into());
    }
    let review = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    if review.trim().is_empty() {
        return Err(
            "[audio_critic_invalid_response] audio reviewer returned no review; reasoning text alone is not a listening result"
                .into(),
        );
    }
    Ok(review)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, Write};

    type Requests = std::thread::JoinHandle<Vec<(String, Value)>>;

    pub(crate) fn server(responses: Vec<(u16, Value)>) -> (String, Requests) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, response) in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut socket);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                drop(reader);
                seen.push((
                    first,
                    if body.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(&body).unwrap()
                    },
                ));
                let body = response.to_string();
                write!(socket, "HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            seen
        });
        (url, worker)
    }

    pub(super) fn wav(sample: i16) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(38u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(sample.to_le_bytes());
        bytes
    }

    pub(crate) fn response(text: &str) -> Value {
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":text}}]})
    }

    #[test]
    fn two_labeled_wav_excerpts_reach_the_reviewer_as_actual_audio_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let before = directory.path().join("before.wav");
        let after = directory.path().join("after.wav");
        std::fs::write(&before, wav(321)).unwrap();
        std::fs::write(&after, wav(654)).unwrap();
        let (url, worker) = server(vec![
            (200, json!({"capabilities":["completion","audio"]})),
            (200, response("The second excerpt has less prominent bass.")),
        ]);
        let options = AudioReviewOptions {
            auto_select: false,
            url: format!("{url}/v1"),
            ollama_url: Some(url),
            ..Default::default()
        };
        let result = review_audio(
            &[("Before", &before), ("After", &after)],
            "Compare the bass balance",
            &options,
        )
        .unwrap();
        assert!(result.audio_sent);
        assert_eq!(result.review, "The second excerpt has less prominent bass.");
        let requests = worker.join().unwrap();
        assert!(requests[0].0.starts_with("POST /api/show "));
        assert!(requests[1].0.starts_with("POST /v1/chat/completions "));
        assert!(requests[1].1.get("tools").is_none());
        assert_eq!(requests[1].1["reasoning_effort"], "none");
        let parts = requests[1].1["messages"][0]["content"].as_array().unwrap();
        assert_eq!(
            parts[0]["text"],
            "Audio excerpts in order:\n1. Before\n2. After\n\nCompare the bass balance"
        );
        for (index, bytes) in [(1, wav(321)), (2, wav(654))] {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(parts[index]["input_audio"]["data"].as_str().unwrap())
                .unwrap();
            assert_eq!(decoded, bytes);
            assert_eq!(parts[index]["input_audio"]["format"], "wav");
        }
    }

    #[test]
    fn a_model_without_audio_capability_is_refused_before_uploading_audio() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clip.wav");
        std::fs::write(&path, wav(321)).unwrap();
        let (url, worker) = server(vec![(200, json!({"capabilities":["completion","tools"]}))]);
        let options = AudioReviewOptions {
            auto_select: false,
            url: format!("{url}/v1"),
            ollama_url: Some(url),
            ..Default::default()
        };
        let error = review_audio(&[("Mix", &path)], "Review the mix", &options).unwrap_err();
        assert!(error.contains("does not advertise audio input"));
        assert_eq!(worker.join().unwrap().len(), 1);
    }

    #[test]
    fn missing_oversized_and_non_wav_audio_never_make_a_model_request() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clip.wav");
        assert!(audio_content(&[("Mix", &path)], "Review").is_err());
        std::fs::write(&path, b"not a WAV").unwrap();
        assert!(
            audio_content(&[("Mix", &path)], "Review")
                .unwrap_err()
                .contains("WAV")
        );
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_AUDIO_BYTES + 1)
            .unwrap();
        assert!(
            audio_content(&[("Mix", &path)], "Review")
                .unwrap_err()
                .contains("25 MiB")
        );
        assert!(audio_content(&[], "Review").is_err());
    }

    #[test]
    fn request_failures_and_unfinished_or_empty_reviews_are_not_listening_results() {
        let (url, worker) = server(vec![(
            503,
            json!({"error":{"message":"model audio-9b unavailable for secret-token at https://user:password@example.com"}}),
        )]);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .into();
        let error = post_json(&agent, &url, &json!({}), Some("secret-token")).unwrap_err();
        assert!(error.contains("503") && error.contains("model audio-9b unavailable"));
        assert!(!error.contains("secret-token") && !error.contains("password"));
        worker.join().unwrap();
        for value in [
            json!({}),
            response(""),
            json!({"choices":[{"message":{"refusal":"cannot provide a review","content":""}}]}),
            json!({"choices":[{"message":{"reasoning":"I should listen","content":null}}]}),
        ] {
            assert!(review_text(&value).is_err());
        }
        let partial = "The bass is too loud and you should";
        let error = review_text(&json!({"choices":[{
            "finish_reason":"length","message":{"content":partial}
        }]}))
        .unwrap_err();
        assert!(error.contains("output-token limit"), "{error}");
        assert!(error.contains("shorter focus") && error.contains("briefer review"));
        assert!(error.contains("increase the audio server's output-token limit"));
        assert!(!error.contains(partial));
        let unknown = "unrecognized-provider-detail";
        let error = review_text(&json!({"choices":[{
            "finish_reason":unknown,"message":{"content":partial}
        }]}))
        .unwrap_err();
        assert!(error.contains("did not complete its review"));
        assert!(!error.contains(unknown) && !error.contains(partial));
        // Delivery is a separate fact from perception. Keep a model's stated inability to hear
        // visible to the caller, instead of substituting measurements and claiming a review.
        assert_eq!(
            review_text(&response("I cannot hear this audio.")).unwrap(),
            "I cannot hear this audio."
        );
    }

    #[test]
    fn audio_connection_options_use_explicit_environment_values_without_leaking_keys() {
        assert!(
            !AudioReviewOptions::from_values(&|_| None)
                .unwrap()
                .configured
        );
        let options = AudioReviewOptions::from_values(&|name| match name {
            "AURIS_AUDIO_URL" => Some("http://localhost:11434/v1/".into()),
            "AURIS_AUDIO_MODEL" => Some("audio-model:9b".into()),
            "AURIS_AUDIO_API_KEY_ENV" => Some("REVIEW_SECRET".into()),
            "REVIEW_SECRET" => Some("secret-token".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(options.url, "http://localhost:11434/v1");
        assert!(options.configured);
        assert_eq!(
            options.ollama_url.as_deref(),
            Some("http://localhost:11434")
        );
        assert_eq!(options.model, "audio-model:9b");
        assert_eq!(options.api_key.as_deref(), Some("secret-token"));
        assert!(
            AudioReviewOptions::from_values(
                &|name| (name == "AURIS_AUDIO_API_KEY_ENV").then(|| "MISSING".into())
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod training_transfer_tests {
    use super::*;
    use crate::audio_input::tests::{contract, stereo};

    #[test]
    fn complete_five_minute_pair_reaches_one_model_context_in_order() {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a.wav");
        let b = directory.path().join("b.wav");
        let frames = 48_000 * 300;
        stereo(&a, frames, [0.75, -0.75], [0.25, -0.5]);
        stereo(&b, frames, [-0.5, 0.75], [-0.25, 0.5]);
        let (url, worker) = super::tests::server(vec![(
            200,
            super::tests::response("Compared both complete recordings"),
        )]);
        let options = AudioReviewOptions {
            url: format!("{url}/v1"),
            model: "custom-stereo-model".into(),
            ollama_url: None,
            auto_select: false,
            ..Default::default()
        };
        let reply = review_audio_with_contract(
            &[("A", &a), ("B", &b)],
            "Compare the entire two recordings",
            &options,
            Some(&contract()),
            &AtomicBool::new(false),
        )
        .unwrap();
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1);
        let content = requests[0].1["messages"][0]["content"].as_array().unwrap();
        let report = reply.transfer;
        assert_eq!(content.len(), 3);
        assert_eq!(report.as_ref().unwrap()["comparison_mode"], "simultaneous");
        assert_eq!(report.unwrap()["inputs"][1]["label"], "B");
        for (index, first, last) in [
            (1, [0.75, -0.75], [0.25, -0.5]),
            (2, [-0.5, 0.75], [-0.25, 0.5]),
        ] {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content[index]["input_audio"]["data"].as_str().unwrap())
                .unwrap();
            let mut decoded = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
            assert_eq!(decoded.spec().sample_rate, 48_000);
            assert_eq!(decoded.spec().channels, 2);
            assert_eq!(decoded.duration(), frames);
            assert_eq!(
                decoded
                    .samples::<f32>()
                    .take(2)
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
                first
            );
            decoded.seek(frames - 1).unwrap();
            assert_eq!(
                decoded
                    .samples::<f32>()
                    .take(2)
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
                last
            );
        }
    }

    #[test]
    fn comparison_capability_is_reported_and_byte_limits_do_not_shorten_audio() {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a.wav");
        stereo(&a, 4800, [0.5, -0.5], [0.25, -0.25]);
        let mut agreement = contract();
        agreement.simultaneous_comparison = false;
        let (_, report) =
            audio_content_agreed(&[("A", &a), ("B", &a)], "Compare", Some(&agreement)).unwrap();
        assert_eq!(report.unwrap()["comparison_mode"], "independent_reviews");
        agreement.max_total_bytes = 40_000;
        agreement.max_bytes = 40_000;
        assert!(
            audio_content_agreed(&[("A", &a), ("B", &a)], "Compare", Some(&agreement)).is_err()
        );
    }
}
