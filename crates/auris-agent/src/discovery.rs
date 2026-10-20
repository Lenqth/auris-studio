//! Discover installed and configured model providers without starting inference.

use crate::*;
use auris_session::AgentPreferences;
use serde::{Deserialize, Serialize};

/// Routing information for a discovered model; API key values never leave the environment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConnection {
    /// Worker runtime, either `codex` or `rig`.
    pub backend: String,
    /// API dialect for rig: `ollama` or `openai`.
    pub provider: String,
    /// API endpoint override; empty uses the provider default.
    pub url: String,
    /// Name of the environment variable containing an API key.
    pub api_key_env: String,
}

impl ModelConnection {
    /// Capture only connection settings, leaving permissions and generation settings live.
    pub fn from_preferences(prefs: &AgentPreferences) -> Self {
        Self {
            backend: if prefs.backend == "codex" {
                "codex"
            } else {
                "rig"
            }
            .into(),
            provider: if prefs.provider == "openai" {
                "openai"
            } else {
                "ollama"
            }
            .into(),
            url: prefs.url.clone(),
            api_key_env: prefs.api_key_env.clone(),
        }
    }

    /// Apply a model's routing without changing the document's permissions or model limits.
    pub fn apply_to(&self, prefs: &mut AgentPreferences) {
        prefs.backend.clone_from(&self.backend);
        // Codex keeps the API configuration available for the next discovery.
        if self.backend != "codex" {
            prefs.provider.clone_from(&self.provider);
            prefs.url.clone_from(&self.url);
            prefs.api_key_env.clone_from(&self.api_key_env);
        }
    }

    /// Provider name displayed next to models in the shared picker.
    pub fn label(&self) -> String {
        let provider = match (self.backend.as_str(), self.provider.as_str()) {
            ("codex", _) => "Codex",
            (_, "openai") => "OpenAI compatible",
            _ => "Ollama",
        };
        if self.backend != "codex"
            && !self.url.is_empty()
            && let Ok(url) = reqwest::Url::parse(&self.url)
            && let Some(host) = url.host_str()
        {
            let port = url
                .port()
                .map(|port| format!(":{port}"))
                .unwrap_or_default();
            return format!("{provider} · {host}{port}");
        }
        provider.to_owned()
    }

    /// Whether this route is the one currently selected, ignoring API settings for Codex.
    pub fn matches(&self, prefs: &AgentPreferences) -> bool {
        let current = Self::from_preferences(prefs);
        if self.backend == "codex" {
            current.backend == "codex"
        } else {
            *self == current
        }
    }
}

fn candidates(prefs: &AgentPreferences, openai_key: bool) -> Vec<AgentPreferences> {
    let mut configured = prefs.clone();
    if let Some(endpoint) = &prefs.discovery_endpoint {
        configured.provider.clone_from(&endpoint.provider);
        configured.url.clone_from(&endpoint.url);
        configured.api_key_env.clone_from(&endpoint.api_key_env);
    }
    let prefs = &configured;
    let mut codex = prefs.clone();
    codex.backend = "codex".into();
    let mut ollama = prefs.clone();
    ollama.backend = "rig".into();
    ollama.provider = "ollama".into();
    ollama.url.clear();
    ollama.api_key_env.clear();
    let mut result = vec![codex, ollama];
    if prefs.provider == "openai" || !prefs.url.is_empty() || !prefs.api_key_env.is_empty() {
        let mut api = prefs.clone();
        api.backend = "rig".into();
        api.provider = if prefs.provider == "openai" {
            "openai"
        } else {
            "ollama"
        }
        .into();
        result.push(api);
    }
    if prefs.provider != "openai" && openai_key {
        let mut openai = prefs.clone();
        openai.backend = "rig".into();
        openai.provider = "openai".into();
        openai.url.clear();
        openai.api_key_env.clear();
        result.push(openai);
    }
    result
}

async fn discover(prefs: AgentPreferences) -> Result<String, String> {
    discover_candidates(candidates(
        &prefs,
        std::env::var_os("OPENAI_API_KEY").is_some(),
    ))
    .await
}

async fn discover_candidates(candidates: Vec<AgentPreferences>) -> Result<String, String> {
    let mut pending = tokio::task::JoinSet::new();
    for candidate in candidates {
        pending.spawn(async move {
            let connection = ModelConnection::from_preferences(&candidate);
            let answer = tokio::time::timeout(MODEL_LIST_PATIENCE, async {
                if candidate.backend == "codex" {
                    backends::codex::list_models(&candidate).await
                } else {
                    let Command::Models(options) = parse_command(
                        &["models".into()],
                        &|name| std::env::var(name).ok(),
                        &candidate,
                    )?
                    else {
                        return Err("Expected model listing settings".into());
                    };
                    list_models(&options).await
                }
            })
            .await
            .map_err(|_| "Model discovery timed out".to_string())
            .and_then(|answer| answer);
            (connection, answer)
        });
    }
    let mut models = Vec::new();
    let mut errors = Vec::new();
    while let Some(result) = pending.join_next().await {
        let (connection, answer) = result.map_err(|error| error.to_string())?;
        match answer.and_then(|line| {
            serde_json::from_str::<serde_json::Value>(&line).map_err(|error| error.to_string())
        }) {
            Ok(listing) => {
                if let Some(entries) = listing["models"].as_array() {
                    for entry in entries {
                        if !entry["name"]
                            .as_str()
                            .is_some_and(|name| !name.trim().is_empty())
                        {
                            continue;
                        }
                        let mut entry = entry.clone();
                        entry["connection"] =
                            serde_json::to_value(&connection).map_err(|error| error.to_string())?;
                        models.push(entry);
                    }
                }
            }
            Err(error) => errors.push(format!("{}: {error}", connection.label())),
        }
    }
    models.sort_by(|a, b| {
        a["connection"]["backend"]
            .as_str()
            .cmp(&b["connection"]["backend"].as_str())
            .then_with(|| {
                a["connection"]["provider"]
                    .as_str()
                    .cmp(&b["connection"]["provider"].as_str())
            })
    });
    if models.is_empty() && !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(serde_json::json!({"models":models}).to_string())
}

/// Probe Codex, local Ollama and the configured API concurrently on a background thread.
/// A failed provider does not hide models returned by another provider.
pub fn discover_models_background(
    prefs: AgentPreferences,
) -> std::sync::mpsc::Receiver<Result<String, String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let failed = sender.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("auris-model-discovery".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())?
                    .block_on(discover(prefs))
            }))
            .unwrap_or_else(|_| Err("Model discovery worker panicked".into()));
            let _ = sender.send(result);
        })
    {
        let _ = failed.send(Err(error.to_string()));
    }
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unavailable_backend_does_not_hide_a_working_provider() {
        let (url, _seen) = crate::tests::mock_server(vec![
            r#"{"models":[{"name":"fixture","details":{"context_length":32768}}]}"#.into(),
        ]);
        let prefs = AgentPreferences {
            provider: "ollama".into(),
            url: url.clone(),
            ..Default::default()
        };
        let unavailable = AgentPreferences {
            backend: "codex".into(),
            codex_path: "/does-not-exist/codex.exe".into(),
            ..Default::default()
        };
        let listed = discover_candidates(vec![unavailable, prefs]).await.unwrap();
        let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
        assert_eq!(listed["models"][0]["name"], "fixture");
        assert_eq!(listed["models"][0]["connection"]["url"], url);
    }

    #[test]
    fn discovery_separates_custom_api_credentials_from_local_ollama() {
        let prefs = AgentPreferences {
            backend: "codex".into(),
            provider: "openai".into(),
            url: "https://example.invalid/v1".into(),
            api_key_env: "PRIVATE_KEY".into(),
            ..Default::default()
        };
        let routes = candidates(&prefs, false);
        assert_eq!(routes.len(), 3);
        assert!(routes[1].url.is_empty() && routes[1].api_key_env.is_empty());
        assert_eq!(routes[2].url, prefs.url);
        let mut selected = prefs.clone();
        ModelConnection::from_preferences(&routes[0]).apply_to(&mut selected);
        assert_eq!(selected, prefs);
    }
}
