//! Bounded local capability discovery, before any audio upload.

use super::{AudioReviewOptions, ReviewerSelection, post_json, response_json};
use serde_json::{Value, json};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const BRIDGE: &str = "http://127.0.0.1:11435/v1";
const OLLAMA: &str = "http://127.0.0.1:11434";
const MAX_MODELS: usize = 24;

fn loopback(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    })
}

struct Discovery<'a> {
    deadline: Instant,
    cancel: &'a AtomicBool,
    reachable: Cell<bool>,
}

impl Discovery<'_> {
    fn agent(&self) -> Result<ureq::Agent, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Audio review cancelled".into());
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Audio reviewer discovery timed out".into());
        }
        Ok(ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(1).min(remaining)))
            .timeout_global(Some(Duration::from_secs(5).min(remaining)))
            .max_redirects(0)
            .build()
            .into())
    }

    fn get(&self, url: &str, key: Option<&str>) -> Result<Value, String> {
        let agent = self.agent()?;
        let mut request = agent.get(url);
        if let Some(key) = key {
            request = request.header("Authorization", &format!("Bearer {key}"));
        }
        let response = request
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .map_err(|_| "Audio reviewer metadata is unavailable".to_string())?;
        let result = response_json(response, key);
        if result.is_ok() {
            self.reachable.set(true);
        }
        result
    }

    fn audio_model(&self, options: &AudioReviewOptions, model: &str) -> bool {
        let Some(root) = &options.ollama_url else {
            return false;
        };
        let Ok(agent) = self.agent() else {
            return false;
        };
        let shown = post_json(
            &agent,
            &format!("{root}/api/show"),
            &json!({"model":model}),
            options.api_key.as_deref(),
        );
        if shown.is_ok() {
            self.reachable.set(true);
        }
        shown.is_ok_and(|shown| advertises_audio(&shown) && !is_remote_model(&shown))
    }

    fn discover(&self, options: &AudioReviewOptions) -> Option<AudioReviewOptions> {
        if let Some(root) = &options.ollama_url {
            if self.audio_model(options, &options.model) {
                return Some(options.clone());
            }
            let tags = self
                .get(&format!("{root}/api/tags"), options.api_key.as_deref())
                .ok()?;
            let mut models: Vec<_> = tags.get("models")?.as_array()?.iter().collect();
            // Smaller installed models are more likely to fit local memory. This is not
            // a ranking of musical quality; every candidate still needs audio capability.
            models.sort_by_key(|entry| {
                (
                    entry["size"].as_u64().unwrap_or(u64::MAX),
                    entry["name"].as_str().unwrap_or(""),
                )
            });
            for entry in models
                .into_iter()
                .filter(|entry| !is_remote_model(entry))
                .take(MAX_MODELS)
            {
                let Some(model) = entry["name"]
                    .as_str()
                    .filter(|name| !name.is_empty() && name.len() <= 256)
                else {
                    continue;
                };
                if model != options.model && self.audio_model(options, model) {
                    let mut selected = options.clone();
                    selected.model = model.into();
                    return Some(selected);
                }
            }
            None
        } else {
            let models = self
                .get(
                    &format!("{}/models", options.url),
                    options.api_key.as_deref(),
                )
                .ok()?;
            let mut models: Vec<_> = models
                .get("data")?
                .as_array()?
                .iter()
                .filter(|entry| advertises_audio(entry))
                .filter_map(|entry| entry["id"].as_str())
                .filter(|name| !name.is_empty() && name.len() <= 256)
                .collect();
            models.sort_by_key(|name| (*name != options.model, *name));
            let mut selected = options.clone();
            selected.model = models.first()?.to_string();
            Some(selected)
        }
    }
}

fn is_remote_model(value: &Value) -> bool {
    ["remote_host", "remote_model"].iter().any(|key| {
        value
            .get(key)
            .is_some_and(|field| !field.is_null() && field.as_str() != Some(""))
    })
}

fn advertises_audio(value: &Value) -> bool {
    ["capabilities", "input_modalities"].iter().any(|key| {
        value
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value == "audio"))
    })
}

pub(super) fn select(
    options: &AudioReviewOptions,
    cancel: &AtomicBool,
) -> Result<(AudioReviewOptions, ReviewerSelection), String> {
    select_with_defaults(options, cancel, BRIDGE, OLLAMA)
}

fn select_with_defaults(
    options: &AudioReviewOptions,
    cancel: &AtomicBool,
    bridge: &str,
    ollama: &str,
) -> Result<(AudioReviewOptions, ReviewerSelection), String> {
    let automatic = options.auto_select
        && loopback(&options.url)
        && options.ollama_url.as_deref().is_none_or(loopback);
    let mut report = ReviewerSelection {
        automatic,
        fallback: false,
        requested_model: options.model.clone(),
    };
    if !automatic {
        return Ok((options.clone(), report));
    }
    let discovery = Discovery {
        deadline: Instant::now() + Duration::from_secs(20),
        cancel,
        reachable: Cell::new(false),
    };
    let candidates = [
        options.clone(),
        AudioReviewOptions {
            configured: false,
            url: bridge.into(),
            model: options.model.clone(),
            api_key: None,
            ollama_url: None,
            auto_select: false,
        },
        AudioReviewOptions {
            configured: false,
            url: format!("{ollama}/v1"),
            model: options.model.clone(),
            api_key: None,
            ollama_url: Some(ollama.into()),
            auto_select: false,
        },
    ];
    for (index, candidate) in candidates.iter().enumerate() {
        // A configured endpoint is not retried anonymously after an authentication error.
        if index > 0 && same_endpoint(&candidate.url, &options.url) {
            continue;
        }
        if let Some(selected) = discovery.discover(candidate) {
            discovery.agent()?; // Cancellation/deadline checks after the last response.
            report.fallback =
                !same_endpoint(&selected.url, &options.url) || selected.model != options.model;
            return Ok((selected, report));
        }
    }
    discovery.agent()?;
    Err(discovery_failure(
        options.configured,
        discovery.reachable.get(),
    ))
}

fn discovery_failure(configured: bool, reachable: bool) -> String {
    if reachable {
        "[audio_critic_no_audio_model] A local server responded, but no installed model advertised audio input. Choose an audio-input model or configure a compatible audio critic endpoint."
    } else if configured {
        "[audio_critic_connection_failed] Could not connect to the configured audio critic or find a local fallback. Check that its server is running and its URL is correct."
    } else {
        "[audio_critic_not_configured] No audio critic is configured and none was discovered locally. Configure AURIS_AUDIO_URL and AURIS_AUDIO_MODEL, or start a local server with an audio-input model."
    }.into()
}

pub(super) fn needs_load_confirmation(options: &AudioReviewOptions, cancel: &AtomicBool) -> bool {
    if !loopback(&options.url) {
        return false;
    }
    let discovery = Discovery {
        deadline: Instant::now() + Duration::from_secs(5),
        cancel,
        reachable: Cell::new(false),
    };
    if let Some(root) = &options.ollama_url {
        let resident = discovery.get(&format!("{root}/api/ps"), options.api_key.as_deref());
        !resident.is_ok_and(|value| {
            value["models"].as_array().is_some_and(|models| {
                models.iter().any(|entry| {
                    ["name", "model"].iter().any(|key| {
                        entry[*key].as_str().is_some_and(|name| {
                            name == options.model || name == format!("{}:latest", options.model)
                        })
                    })
                })
            })
        })
    } else {
        let root = options.url.trim_end_matches('/').trim_end_matches("/v1");
        !discovery
            .get(&format!("{root}/healthz"), options.api_key.as_deref())
            .is_ok_and(|value| {
                value["loaded"] == true && value["model"].as_str() == Some(options.model.as_str())
            })
    }
}

fn same_endpoint(left: &str, right: &str) -> bool {
    let (Ok(mut left), Ok(mut right)) = (url::Url::parse(left), url::Url::parse(right)) else {
        return false;
    };
    if loopback(left.as_str()) {
        let _ = left.set_host(Some("localhost"));
    }
    if loopback(right.as_str()) {
        let _ = right.set_host(Some("localhost"));
    }
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_review::tests::{response, server, wav};

    #[test]
    fn missing_configuration_connection_failure_and_missing_capability_are_distinct() {
        assert!(discovery_failure(false, false).contains("[audio_critic_not_configured]"));
        assert!(discovery_failure(true, false).contains("[audio_critic_connection_failed]"));
        assert!(discovery_failure(false, true).contains("[audio_critic_no_audio_model]"));
        assert!(discovery_failure(true, true).contains("[audio_critic_no_audio_model]"));
    }

    #[test]
    fn unavailable_servers_report_whether_a_reviewer_was_configured() {
        for configured in [false, true] {
            let (primary, p) = server(vec![(503, json!({}))]);
            let (local_bridge, b) = server(vec![(503, json!({}))]);
            let (ollama, o) = server(vec![(503, json!({})), (503, json!({}))]);
            let options = AudioReviewOptions {
                configured,
                ..bridge(&primary)
            };
            let error = select_with_defaults(
                &options,
                &AtomicBool::new(false),
                &format!("{local_bridge}/v1"),
                &ollama,
            )
            .err()
            .unwrap();
            assert_eq!(error, discovery_failure(configured, false));
            for worker in [p, b, o] {
                assert!(
                    worker
                        .join()
                        .unwrap()
                        .iter()
                        .all(|(line, _)| !line.contains("chat/completions"))
                );
            }
        }
    }

    #[test]
    fn load_consent_checks_only_metadata_and_pins_the_selected_model() {
        for loaded in [false, true] {
            let (url, worker) = server(vec![
                (
                    200,
                    json!({"data":[{"id":"preferred","capabilities":["audio"]}]}),
                ),
                (200, json!({"model":"preferred","loaded":loaded})),
            ]);
            let (selected, report, confirm) =
                bridge(&url).prepare(&AtomicBool::new(false)).unwrap();
            assert_eq!(confirm, !loaded);
            assert!(!selected.auto_select);
            assert_eq!(selected.model, "preferred");
            assert!(report.automatic);
            let requests = worker.join().unwrap();
            assert!(requests.iter().all(|(line, _)| line.starts_with("GET ")));
        }
    }

    #[test]
    fn ollama_residency_and_unknown_status_control_load_consent() {
        for (status, body, confirm) in [
            (200, json!({"models":[{"name":"preferred"}]}), false),
            (200, json!({"models":[{"name":"another-model"}]}), true),
            (200, json!({"models":[]}), true),
            (404, json!({}), true),
        ] {
            let (url, worker) = server(vec![(status, body)]);
            let options = AudioReviewOptions {
                ollama_url: Some(url.clone()),
                ..bridge(&url)
            };
            assert_eq!(
                needs_load_confirmation(&options, &AtomicBool::new(false)),
                confirm
            );
            assert!(worker.join().unwrap()[0].0.starts_with("GET /api/ps "));
        }
        assert!(!needs_load_confirmation(
            &bridge("https://example.com"),
            &AtomicBool::new(false)
        ));
    }

    fn bridge(url: &str) -> AudioReviewOptions {
        AudioReviewOptions {
            configured: true,
            url: format!("{url}/v1"),
            model: "preferred".into(),
            ollama_url: None,
            api_key: Some("configured-secret".into()),
            auto_select: true,
        }
    }

    #[test]
    fn unavailable_bridge_falls_back_to_installed_audio_model_without_forwarding_credentials() {
        let (primary, primary_worker) = server(vec![(503, json!({}))]);
        let (local_bridge, bridge_worker) = server(vec![(503, json!({}))]);
        let (ollama, ollama_worker) = server(vec![
            (404, json!({})),
            (
                200,
                json!({"models":[{"name":"vision-only","size":1},{"name":"audio-model","size":2}]}),
            ),
            (200, json!({"capabilities":["vision","completion"]})),
            (200, json!({"capabilities":["audio","completion"]})),
        ]);
        let (selected, report) = select_with_defaults(
            &bridge(&primary),
            &AtomicBool::new(false),
            &format!("{local_bridge}/v1"),
            &ollama,
        )
        .unwrap();
        assert_eq!(selected.model, "audio-model");
        assert!(selected.api_key.is_none());
        assert!(report.automatic && report.fallback);
        assert_eq!(primary_worker.join().unwrap().len(), 1);
        assert_eq!(bridge_worker.join().unwrap().len(), 1);
        let requests = ollama_worker.join().unwrap();
        assert!(requests[1].0.starts_with("GET /api/tags "));
        assert_eq!(requests[2].1["model"], "vision-only");
        assert_eq!(requests[3].1["model"], "audio-model");
        assert!(
            requests
                .iter()
                .all(|(line, _)| !line.contains("chat/completions"))
        );
    }

    #[test]
    fn selected_bridge_model_receives_audio_and_is_reported_verbatim() {
        let (url, worker) = server(vec![
            (
                200,
                json!({"data":[{"id":"text-only"},{"id":"available-audio","input_modalities":["audio"]}]}),
            ),
            (200, response("A short electronic phrase.")),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("excerpt.wav");
        std::fs::write(&path, wav(123)).unwrap();
        let result = crate::audio_review::review_audio(
            &[("Excerpt", &path)],
            "Describe the sound",
            &bridge(&url),
        )
        .unwrap();
        assert_eq!(result.model, "available-audio");
        assert!(result.selection.fallback && result.audio_sent);
        let requests = worker.join().unwrap();
        assert_eq!(requests[1].1["model"], "available-audio");
        assert_eq!(
            requests[1].1["messages"][0]["content"][1]["type"],
            "input_audio"
        );
    }

    #[test]
    fn no_audio_candidate_returns_actionable_error_without_uploading() {
        let (primary, p) = server(vec![(
            200,
            json!({"data":[{"id":"vision","capabilities":["vision"]}]}),
        )]);
        let (local_bridge, b) = server(vec![(200, json!({"data":[]}))]);
        let (ollama, o) = server(vec![(404, json!({})), (200, json!({"models":[]}))]);
        let result = select_with_defaults(
            &bridge(&primary),
            &AtomicBool::new(false),
            &format!("{local_bridge}/v1"),
            &ollama,
        );
        assert!(
            result
                .err()
                .unwrap()
                .contains("[audio_critic_no_audio_model]")
        );
        for worker in [p, b, o] {
            assert!(
                worker
                    .join()
                    .unwrap()
                    .iter()
                    .all(|(line, _)| !line.contains("chat/completions"))
            );
        }
    }

    #[test]
    fn remote_and_fixed_endpoints_do_not_trigger_discovery() {
        for options in [
            bridge("https://example.com"),
            AudioReviewOptions {
                auto_select: false,
                ..bridge("http://127.0.0.1:9")
            },
        ] {
            let (selected, report) = select(&options, &AtomicBool::new(false)).unwrap();
            assert_eq!(selected.url, options.url);
            assert_eq!(selected.api_key, options.api_key);
            assert!(!report.automatic && !report.fallback);
        }
    }

    #[test]
    fn cancellation_and_deadline_stop_discovery_before_network_io() {
        let discovery = Discovery {
            deadline: Instant::now(),
            cancel: &AtomicBool::new(false),
            reachable: Cell::new(false),
        };
        assert!(discovery.agent().is_err());
        let result = select(&bridge("http://127.0.0.1:9"), &AtomicBool::new(true));
        assert_eq!(result.err().unwrap(), "Audio review cancelled");
        assert!(!advertises_audio(&json!({"output_modalities":["audio"]})));
        assert!(is_remote_model(
            &json!({"remote_host":"https://ollama.com"})
        ));
    }

    #[test]
    fn cloud_alias_is_rejected_even_when_it_advertises_audio() {
        let (url, worker) = server(vec![
            (
                200,
                json!({"capabilities":["audio"],"remote_model":"cloud-audio"}),
            ),
            (200, json!({"models":[{"name":"local-audio","size":10}]})),
            (200, json!({"capabilities":["audio"]})),
        ]);
        let options = AudioReviewOptions {
            ollama_url: Some(url.clone()),
            ..bridge(&url)
        };
        let (selected, report) = select(&options, &AtomicBool::new(false)).unwrap();
        assert_eq!(selected.model, "local-audio");
        assert!(report.fallback);
        assert_eq!(worker.join().unwrap().len(), 3);
    }

    #[test]
    fn generation_failure_is_not_retried_on_another_model() {
        let (url, worker) = server(vec![
            (
                200,
                json!({"data":[{"id":"preferred","capabilities":["audio"]}]}),
            ),
            (503, json!({"error":{"message":"model is busy"}})),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("excerpt.wav");
        std::fs::write(&path, wav(123)).unwrap();
        let error =
            crate::audio_review::review_audio(&[("Excerpt", &path)], "Describe", &bridge(&url))
                .unwrap_err();
        assert!(error.contains("model is busy"));
        assert_eq!(worker.join().unwrap().len(), 2);
    }
}
