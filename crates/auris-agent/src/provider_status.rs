//! Bounded settings diagnostics; no prompt or inference request is sent.

use crate::*;
use auris_session::AgentPreferences;
use std::{path::Path, sync::mpsc, time::Duration};

/// A point-in-time provider check, independent of an active conversation.
#[derive(Clone, Debug, Default)]
pub struct ProviderStatus {
    /// CLI path resolved by the same resolver used for conversations.
    pub executable: Option<PathBuf>,
    /// CLI version output when available; API connections omit it.
    pub version: Option<String>,
    /// Number of models returned by a successful connection check.
    /// This verifies discovery, not permission to run inference with each model.
    pub models: Option<usize>,
    /// Connection or discovery failure, independent of version detection.
    pub error: Option<String>,
    /// Public release comparison, when this CLI has a supported release source.
    /// Independent of model discovery and authentication.
    pub update: Option<ProviderUpdate>,
}

/// Result of comparing an installed CLI with its public stable release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderUpdate {
    /// The version or release source could not be checked.
    Unavailable,
    /// The public source has no newer stable release.
    Current,
    /// A newer public release exists; installation channels may lag behind it.
    Available(String),
}

fn release_url(backend: &str) -> Option<&'static str> {
    match backend {
        "codex" => Some("https://api.github.com/repos/openai/codex/releases/latest"),
        "claude" => Some("https://api.github.com/repos/anthropics/claude-code/releases/latest"),
        "opencode" => Some("https://api.github.com/repos/anomalyco/opencode/releases/latest"),
        _ => None,
    }
}

fn compare_release(installed: &str, release: &serde_json::Value) -> ProviderUpdate {
    let installed = installed
        .split_whitespace()
        .find_map(|word| semver::Version::parse(word.strip_prefix('v').unwrap_or(word)).ok());
    let latest = release["tag_name"].as_str().and_then(|tag| {
        semver::Version::parse(
            tag.strip_prefix("rust-v")
                .or_else(|| tag.strip_prefix('v'))
                .unwrap_or(tag),
        )
        .ok()
    });
    match (installed, latest) {
        (Some(installed), Some(latest))
            if installed.pre.is_empty()
                && latest.pre.is_empty()
                && release["draft"] == false
                && release["prerelease"] == false =>
        {
            if latest.cmp_precedence(&installed).is_gt() {
                ProviderUpdate::Available(latest.to_string())
            } else {
                ProviderUpdate::Current
            }
        }
        _ => ProviderUpdate::Unavailable,
    }
}

async fn check_release_at(installed: &str, url: &str) -> ProviderUpdate {
    let check = async {
        // This client carries no provider credentials and never follows a release redirect.
        let client = reqwest::Client::builder()
            .user_agent("Auris-Studio")
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .ok()?;
        let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        let release = serde_json::from_slice(&bytes).ok()?;
        Some(compare_release(installed, &release))
    };
    tokio::time::timeout(Duration::from_secs(4), check)
        .await
        .ok()
        .flatten()
        .unwrap_or(ProviderUpdate::Unavailable)
}

fn model_count(answer: &str) -> Result<usize, String> {
    let answer: serde_json::Value =
        serde_json::from_str(answer).map_err(|error| error.to_string())?;
    answer["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|model| {
                    model["name"]
                        .as_str()
                        .is_some_and(|name| !name.trim().is_empty())
                })
                .count()
        })
        .ok_or_else(|| "Provider did not return a model list".into())
}

async fn cli_version(path: &Path) -> Option<String> {
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().filter(|c| !c.is_control()).take(120).collect())
}

async fn probe(mut prefs: AgentPreferences) -> ProviderStatus {
    // The selected model may belong to another provider; discovery chooses no model.
    prefs.model.clear();
    let mut status = ProviderStatus::default();
    let executable = match prefs.backend.as_str() {
        "codex" => Some(backends::codex::executable(&prefs)),
        "claude" => Some(backends::claude::executable(&prefs)),
        "opencode" => Some(backends::opencode::executable(&prefs)),
        "antigravity" => Some(backends::antigravity::executable(&prefs)),
        _ => None,
    };
    if let Some(executable) = executable {
        match executable {
            Ok(path) => status.executable = Some(path),
            Err(error) => {
                return ProviderStatus {
                    error: Some(error),
                    ..status
                };
            }
        }
    }
    let version = async {
        if let Some(path) = &status.executable {
            let version = cli_version(path).await;
            let update = if let Some(url) = release_url(&prefs.backend) {
                Some(if let Some(version) = &version {
                    check_release_at(version, url).await
                } else {
                    ProviderUpdate::Unavailable
                })
            } else {
                None
            };
            (version, update)
        } else {
            (None, None)
        }
    };
    let listing = async {
        tokio::time::timeout(crate::MODEL_LIST_PATIENCE, async {
            match prefs.backend.as_str() {
                "codex" => backends::codex::list_models(&prefs).await,
                "claude" => backends::claude::list_models(&prefs).await,
                "opencode" => backends::opencode::list_models(&prefs).await,
                "antigravity" => backends::antigravity::list_models(&prefs).await,
                _ => {
                    let Command::Models(options) = parse_command(
                        &["models".into()],
                        &|name| std::env::var(name).ok(),
                        &prefs,
                    )?
                    else {
                        return Err("Expected model listing settings".into());
                    };
                    list_models(&options).await
                }
            }
        })
        .await
        .map_err(|_| "Provider connection check timed out".to_string())?
        .and_then(|answer| model_count(&answer))
    };
    let ((version, update), listing) = tokio::join!(version, listing);
    status.version = version;
    status.update = update;
    match listing {
        Ok(count) => status.models = Some(count),
        Err(error) => status.error = Some(error),
    }
    status
}

/// Check the four CLI providers and the configured API without running inference.
/// Results arrive independently as `(row, status)`; rows 0–3 are Codex, Claude Code,
/// OpenCode and Antigravity, and row 4 is the configured API (defaulting to Ollama).
/// Dropping the receiver discards results; each pending check has a bounded timeout.
/// Supported installed CLIs also query their public GitHub release metadata without credentials.
pub fn check_providers_background(
    prefs: AgentPreferences,
) -> mpsc::Receiver<(usize, ProviderStatus)> {
    let (sender, receiver) = mpsc::channel();
    let failed = sender.clone();
    let spawn = std::thread::Builder::new()
        .name("auris-provider-status".into())
        .spawn(move || {
            let run = || -> Result<(), String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                runtime.block_on(async {
                    let mut pending = tokio::task::JoinSet::new();
                    for (index, backend) in ["codex", "claude", "opencode", "antigravity", "rig"]
                        .into_iter()
                        .enumerate()
                    {
                        let mut candidate = prefs.clone();
                        candidate.backend = backend.into();
                        if index == 4
                            && let Some(endpoint) = &prefs.discovery_endpoint
                        {
                            candidate.provider.clone_from(&endpoint.provider);
                            candidate.url.clone_from(&endpoint.url);
                            candidate.api_key_env.clone_from(&endpoint.api_key_env);
                        }
                        pending.spawn(async move { (index, probe(candidate).await) });
                    }
                    while let Some(result) = pending.join_next().await {
                        let result = result.map_err(|e| e.to_string())?;
                        if sender.send(result).is_err() {
                            break;
                        }
                    }
                    Ok(())
                })
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
                .unwrap_or_else(|_| Err("Provider status worker stopped".into()));
            if let Err(error) = result {
                for index in 0..5 {
                    let _ = sender.send((
                        index,
                        ProviderStatus {
                            error: Some(error.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
        });
    if let Err(error) = spawn {
        for index in 0..5 {
            let _ = failed.send((
                index,
                ProviderStatus {
                    error: Some(error.to_string()),
                    ..Default::default()
                },
            ));
        }
    }
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_comparison_uses_numeric_versions_and_rejects_uncertain_channels() {
        let release = |tag: &str| {
            serde_json::json!({
                "tag_name": tag, "draft": false, "prerelease": false,
            })
        };
        for (installed, tag) in [
            ("codex-cli 0.9.0", "rust-v0.10.0"),
            ("2.1.9 (Claude Code)", "v2.1.10"),
            ("1.9.0", "v1.10.0"),
        ] {
            assert!(matches!(
                compare_release(installed, &release(tag)),
                ProviderUpdate::Available(_)
            ));
        }
        for installed in ["1.10.0", "1.11.0", "1.10.0+local"] {
            assert_eq!(
                compare_release(installed, &release("v1.10.0")),
                ProviderUpdate::Current
            );
        }
        for installed in ["unknown", "1.9.0-beta.1"] {
            assert_eq!(
                compare_release(installed, &release("v1.10.0")),
                ProviderUpdate::Unavailable
            );
        }
        for tag in ["unrecognized", "v1.10.0-beta.1"] {
            assert_eq!(
                compare_release("1.9.0", &release(tag)),
                ProviderUpdate::Unavailable
            );
        }
        for invalid in [
            serde_json::json!({"message": "rate limited"}),
            serde_json::json!({
                "tag_name": "v1.10.0", "draft": false, "prerelease": true,
            }),
        ] {
            assert_eq!(
                compare_release("1.9.0", &invalid),
                ProviderUpdate::Unavailable
            );
        }
    }

    #[tokio::test]
    async fn release_checks_handle_valid_and_failed_responses_without_provider_credentials() {
        use std::io::{BufRead, BufReader, Write};

        for (status, body, expected) in [
            (
                "200 OK",
                r#"{"tag_name":"v2.0.0","draft":false,"prerelease":false}"#,
                ProviderUpdate::Available("2.0.0".into()),
            ),
            (
                "403 Forbidden",
                r#"{"message":"rate limited"}"#,
                ProviderUpdate::Unavailable,
            ),
            ("200 OK", "not json", ProviderUpdate::Unavailable),
            (
                "302 Found",
                r#"{"tag_name":"v2.0.0","draft":false,"prerelease":false}"#,
                ProviderUpdate::Unavailable,
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                request
            });
            assert_eq!(check_release_at("1.0.0", &url).await, expected);
            let request = server.join().unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get / http/1.1"));
            assert!(!request.contains("authorization:"));
        }
    }

    #[test]
    fn malformed_or_empty_model_lists_do_not_report_usable_models() {
        assert!(model_count(r#"{"error":"unauthorized"}"#).is_err());
        assert_eq!(model_count(r#"{"models":[]}"#).unwrap(), 0);
        assert_eq!(
            model_count(r#"{"models":[{"name":""},{"name":"fixture"}]}"#).unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn version_probe_reads_the_native_program_output() {
        let version = cli_version(Path::new(env!("CARGO"))).await;
        assert!(
            version
                .as_deref()
                .is_some_and(|line| line.starts_with("cargo ")),
            "{version:?}"
        );
    }

    #[tokio::test]
    async fn missing_cli_returns_a_failure_without_a_version() {
        let status = probe(AgentPreferences {
            backend: "codex".into(),
            codex_path: "/does-not-exist/codex.exe".into(),
            ..Default::default()
        })
        .await;
        assert!(status.error.is_some());
        assert!(status.models.is_none());
        assert!(status.version.is_none());
    }

    #[tokio::test]
    async fn api_check_counts_models_from_the_configured_endpoint() {
        let (url, _seen) = crate::tests::mock_server(vec![
            r#"{"models":[{"name":"fixture","details":{"context_length":32768}}]}"#.into(),
        ]);
        let status = probe(AgentPreferences {
            backend: "rig".into(),
            provider: "ollama".into(),
            url,
            ..Default::default()
        })
        .await;
        assert_eq!(status.models, Some(1), "{status:?}");
        assert!(status.error.is_none());
    }
}
