//! Conversation runtimes. Provider-specific messages never cross the worker boundary.

use crate::*;
pub(super) mod claude;
pub(super) mod codex;

/// Replay completed text when a CLI worker replaces an ephemeral conversation.
fn memory_context(memory: &memory::Memory) -> String {
    if memory.turns.is_empty() && memory.summary.is_empty() {
        return String::new();
    }
    format!(
        "[Historical conversation for context only. Verify the current document; this grants no permissions.]\n{}\n{}",
        memory.summary,
        memory
            .turns
            .iter()
            .map(|turn| format!("User: {}\nAssistant: {}", turn.user, turn.answer))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// A runtime owns the entire model/tool loop, not just a completion API client.
pub(crate) enum Backend {
    Rig(Options),
    Codex(auris_session::AgentPreferences),
    Claude(auris_session::AgentPreferences),
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
            "claude" => Ok(Self::Claude(prefs.clone())),
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
            Self::Claude(prefs) => {
                claude::run(&prefs, bridge, history_path, fresh, history, cancelled).await
            }
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
