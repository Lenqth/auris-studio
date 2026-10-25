//! Every live backend crosses the same fail-closed permission gate.
use super::*;
use auris_session::agent_policy::{Decision, Operation};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

pub(super) async fn authorize(
    bridge: Option<&Bridge>,
    tool: &str,
    args: &str,
) -> Result<(), String> {
    let correction = |error| {
        format!(
            "{error}. Check the tool schema for {tool} and correct its argument fields before trying again. Do not repeat unchanged arguments."
        )
    };
    let args: serde_json::Value =
        serde_json::from_str(args).map_err(|error| correction(error.to_string()))?;
    // Flat provider calls are approved as canonical commands, preserving saved rules and
    // the host's exact-command/revision permit checks.
    let normalized = toolbox::live_agent::command(tool, &args).map_err(correction)?;
    let (tool, args) = match normalized {
        Some(command) => ("edit_project", serde_json::json!({"command":command})),
        None => (tool, args),
    };
    let operation = Operation::parse(tool, &args).map_err(correction)?;
    let Some(bridge) = bridge else {
        return match auris_session::agent_policy::Policy::default().decide(&operation) {
            Decision::Allow if !operation.mutating => Ok(()),
            Decision::Deny(reason) => Err(reason),
            _ => Err("This operation needs the desktop Agent Panel.".into()),
        };
    };
    let id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let response = bridge
        .exchange(serde_json::json!({"event":"permission", "id":id, "tool":tool, "args":args}))
        .await?;
    if response["event"] != "permission_result" || response["id"] != id {
        bridge.block_tool_recovery();
        return Err("The permission response did not match the pending request.".into());
    }
    if response["ok"] == true {
        Ok(())
    } else {
        bridge.block_tool_recovery();
        Err(response["reason"]
            .as_str()
            .unwrap_or("The user denied this operation. Do not retry it.")
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_permission_denial_blocks_automatic_recovery_until_a_new_user_turn() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let (bridge, commands, events) = crate::worker::test_bridge();
        let denied = authorize(Some(&bridge), "capabilities", "{}");
        let owner = async {
            let request = loop {
                if let Ok(event) = events.try_recv() {
                    break event;
                }
                tokio::task::yield_now().await;
            };
            commands.send(serde_json::json!({"event":"permission_result","id":request["id"],"ok":false,"reason":"fixture denial"}).to_string()).unwrap();
        };
        let (result, ()) = tokio::join!(denied, owner);
        assert!(result.is_err());
        assert!(!bridge.can_recover_tools());
        commands
            .send(serde_json::json!({"say":"A new user request"}).to_string())
            .unwrap();
        bridge.receive().await.unwrap();
        assert!(bridge.can_recover_tools());
        }).await.unwrap();
    }
}
