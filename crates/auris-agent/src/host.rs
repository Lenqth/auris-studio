//! The transport-independent live tool path. Only the desktop owns the session.

use crate::{Bridge, permissions, toolbox};
use serde_json::{Value, json};

/// One window's tool endpoint, shared by rig and the embedded MCP server.
#[derive(Clone)]
pub(crate) struct ToolHost {
    bridge: Bridge,
}

impl ToolHost {
    pub(crate) fn new(bridge: Bridge) -> Self {
        Self { bridge }
    }

    pub(crate) fn definitions() -> Vec<toolbox::live_agent::Definition> {
        toolbox::live_agent::definitions()
            .into_iter()
            .filter(|definition| definition.name != "list_instruments")
            .collect()
    }

    pub(crate) fn contains(name: &str) -> bool {
        static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        NAMES
            .get_or_init(|| {
                Self::definitions()
                    .into_iter()
                    .map(|tool| tool.name)
                    .collect()
            })
            .iter()
            .any(|tool| tool == name)
    }

    pub(crate) async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        // Keep approval and execution together even when an MCP client calls in parallel.
        let _guard = self.bridge.tool_lock().await;
        if !Self::contains(name) {
            return Err(format!("Unknown live tool: {name}"));
        }
        let command = toolbox::live_agent::command(name, &args)?
            .ok_or_else(|| format!("Unknown live tool: {name}"))?;
        permissions::authorize(Some(&self.bridge), name, &args.to_string()).await?;
        let reply = self
            .bridge
            .exchange(json!({"event":"edit", "command":command}))
            .await?;
        if reply["event"] != "edit_result" {
            return Err("The edit response did not match the pending request".into());
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn denied_and_mismatched_approvals_never_request_an_edit() {
        for response in [
            json!({"ok":false,"reason":"Denied by the current policy"}),
            json!({"ok":true,"id":0}),
        ] {
            let (bridge, commands, events) = crate::worker::test_bridge();
            let host = ToolHost::new(bridge);
            let call = host.call("add_track", json!({"name":"Denied","kind":"instrument"}));
            let owner = async {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                let request = loop {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "No permission request arrived"
                    );
                    match events.try_recv() {
                        Ok(event) => break event,
                        Err(std::sync::mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                        Err(error) => panic!("{error}"),
                    }
                };
                assert_eq!(request["event"], "permission");
                assert_eq!(request["tool"], "edit_project");
                let mut reply = response.clone();
                reply["event"] = json!("permission_result");
                if reply.get("id").is_none() {
                    reply["id"] = request["id"].clone();
                }
                commands.send(reply.to_string()).unwrap();
            };
            let (result, ()) = tokio::join!(call, owner);
            assert!(result.is_err());
            assert!(events.try_recv().is_err());
        }
    }
}
