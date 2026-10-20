//! Conversation runtimes. Provider-specific messages never cross the worker boundary.

use crate::*;
pub(super) mod codex;

/// A runtime owns the entire model/tool loop, not just a completion API client.
pub(crate) enum Backend {
    Rig(Options),
    Codex(auris_session::AgentPreferences),
}

impl Backend {
    pub(crate) fn from_preferences(
        prefs: &auris_session::AgentPreferences,
    ) -> Result<Self, String> {
        match prefs.backend.as_str() {
            "" | "rig" => {
                let Command::Run(options) =
                    parse_command(&[], &|name| std::env::var(name).ok(), prefs)?
                else {
                    return Err("Expected agent settings".into());
                };
                Ok(Self::Rig(options))
            }
            "codex" => Ok(Self::Codex(prefs.clone())),
            other => Err(format!("Unknown agent backend: {other}")),
        }
    }

    pub(crate) async fn run(
        self,
        bridge: &Bridge,
        history_path: Option<PathBuf>,
        fresh: bool,
        history: Option<HistorySnapshot>,
        cancelled: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), String> {
        match self {
            Self::Codex(prefs) => {
                codex::run(&prefs, bridge, history_path, fresh, history, cancelled).await
            }
            Self::Rig(options) => tokio::select! {
                biased;
                _ = cancelled => Ok(()),
                result = async {
                    let agent = build_for_worker(&options, Some(bridge.clone()))?;
                    let vision = runtime::preflight(&options).await?;
                    bridge.set_vision(vision);
                    json_conversation(&agent, &options, bridge, history_path, fresh, history).await
                } => result,
            },
        }
    }
}
