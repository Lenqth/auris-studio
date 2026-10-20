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
        if name == toolbox::live_agent::piano_cover::NAME {
            let args: toolbox::live_agent::piano_cover::Args =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let at_beat = args.at_beat.unwrap_or(0.0);
            let name = args.name.unwrap_or_else(|| "Piano cover".into());
            if !at_beat.is_finite()
                || !(0.0..=1_000_000.0).contains(&at_beat)
                || name.trim().is_empty()
                || name.len() > 256
            {
                return Err(
                    "Use a nonempty name up to 256 bytes and at_beat within 0..1000000".into(),
                );
            }
            let source = self.bridge.audio_reference(&args.attachment)?;
            let control = self.bridge.analysis_control();
            let transcription = tokio::task::spawn_blocking(move || {
                auris_session::transcribe_piano_file(source.path(), &control)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())??;
            transcription.validate().map_err(|e| e.to_string())?;
            let command = auris_session::live_agent::Command::CreatePianoCover {
                transcription,
                name,
                at_beat,
            };
            // Approval binds the actual immutable note draft to the current document revision.
            permissions::authorize(
                Some(&self.bridge),
                "edit_project",
                &json!({"command":command}).to_string(),
            )
            .await?;
            let reply = self
                .bridge
                .exchange(json!({"event":"edit","command":command}))
                .await?;
            if reply["event"] != "edit_result" {
                return Err("The edit response did not match the pending request".into());
            }
            return Ok(reply);
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
    async fn live_vocal_and_harmony_calls_reach_the_canonical_permission_gate_before_edits() {
        for (name, args) in [
            (
                "edit_harmony",
                json!({"start_bar":1,"bars":4,"chords":"| I | IV | V | I |"}),
            ),
            (
                "set_singer_voice",
                json!({"track":1,"voice":"/voice/test.voicevox.json"}),
            ),
            ("write_lyrics", json!({"clip":2,"text":"さくら"})),
            ("sing", json!({"track":1})),
        ] {
            let (bridge, commands, events) = crate::worker::test_bridge();
            let host = ToolHost::new(bridge);
            let call = host.call(name, args);
            let owner = async {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                let event = loop {
                    assert!(std::time::Instant::now() < deadline);
                    match events.try_recv() {
                        Ok(event) => break event,
                        Err(std::sync::mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                        Err(e) => panic!("{e}"),
                    }
                };
                assert_eq!(event["event"], "permission");
                let operation = auris_session::agent_policy::Operation::parse(
                    event["tool"].as_str().unwrap(),
                    &event["args"],
                )
                .unwrap();
                assert_eq!(operation.name, format!("edit_project.{name}"));
                assert!(operation.mutating);
                commands.send(json!({"event":"permission_result","id":event["id"],"ok":false,"reason":"Read-only"}).to_string()).unwrap();
            };
            let (result, ()) = tokio::join!(call, owner);
            assert!(result.is_err());
            assert!(events.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn piano_cover_uses_retained_audio_and_the_canonical_permission_gate() {
        for allowed in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("piano.wav");
            crate::attachments::tests::reference_wav(&path, false);
            let (bridge, commands, events) = crate::worker::test_bridge();
            let prepared = crate::attachments::cli_input(
                "Piano cover".into(),
                vec![path.display().to_string()],
                &bridge,
                false,
            )
            .await
            .unwrap();
            let metadata: Value = serde_json::from_str(
                prepared[1]["text"]
                    .as_str()
                    .unwrap()
                    .lines()
                    .nth(1)
                    .unwrap(),
            )
            .unwrap();
            std::fs::write(path, b"replaced after sending").unwrap();
            let host = ToolHost::new(bridge.clone());
            let call = host.call(
                "create_piano_cover",
                json!({"attachment":metadata["attachment_id"],"at_beat":4.0}),
            );
            let owner = async {
                let mut session =
                    auris_session::Session::new(auris_session::SessionOptions::headless()).unwrap();
                session.add_default_instrument_track("Existing").unwrap();
                let before = session.project().clone();
                let mut edits = 0;
                loop {
                    let event = loop {
                        match events.try_recv() {
                            Ok(value) => break value,
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                tokio::task::yield_now().await
                            }
                            Err(error) => panic!("{error}"),
                        }
                    };
                    if event["event"] == "permission" {
                        let operation = auris_session::agent_policy::Operation::parse(
                            event["tool"].as_str().unwrap(),
                            &event["args"],
                        )
                        .unwrap();
                        assert_eq!(operation.name, "edit_project.create_piano_cover");
                        assert!(operation.mutating);
                        let policy = auris_session::agent_policy::Policy {
                            mode: if allowed {
                                auris_session::agent_policy::Mode::Edit
                            } else {
                                auris_session::agent_policy::Mode::ReadOnly
                            },
                            ..Default::default()
                        };
                        assert_eq!(
                            policy.decide(&operation)
                                == auris_session::agent_policy::Decision::Allow,
                            allowed
                        );
                        commands
                            .send(
                                json!({"event":"permission_result","id":event["id"],"ok":allowed})
                                    .to_string(),
                            )
                            .unwrap();
                        if !allowed {
                            break;
                        }
                    } else if event["event"] == "edit" {
                        edits += 1;
                        let text = session
                            .agent_command(
                                serde_json::from_value(event["command"].clone()).unwrap(),
                            )
                            .unwrap();
                        commands
                            .send(json!({"event":"edit_result","ok":true,"text":text}).to_string())
                            .unwrap();
                        let piano = session
                            .project()
                            .tracks
                            .last()
                            .unwrap()
                            .kind
                            .as_instrument()
                            .unwrap();
                        assert_eq!(piano.instrument_id, "auris.physical.piano");
                        for pitch in [60, 64, 67] {
                            assert!(piano.clips[0].notes.iter().any(|note| note.pitch == pitch));
                        }
                        assert!(session.path().is_none());
                        session.undo();
                        break;
                    }
                }
                assert_eq!(edits, usize::from(allowed));
                assert_eq!(session.project(), &before);
            };
            let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::join!(call, owner)
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), allowed);
            assert!(events.try_recv().is_err());
            let other = ToolHost::new(crate::worker::test_bridge().0);
            assert!(
                other
                    .call(
                        "create_piano_cover",
                        json!({"attachment":metadata["attachment_id"]})
                    )
                    .await
                    .unwrap_err()
                    .contains("Unknown or expired")
            );
            bridge.analysis_control().cancel();
            assert!(
                host.call(
                    "create_piano_cover",
                    json!({"attachment":metadata["attachment_id"]})
                )
                .await
                .unwrap_err()
                .contains("cancelled")
            );
        }
    }

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
