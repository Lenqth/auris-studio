//! Permission decisions for live agents, including embedded MCP. Standalone saved-file MCP
//! does not consult this policy.

use serde::{Deserialize, Serialize};

/// User-selected behavior for agent operations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Read freely; deny document changes and ask before network access unless allow-listed.
    ReadOnly,
    /// Allow ordinary document edits; ask before removal, replacement or network access.
    #[default]
    Edit,
    /// Review operations needing confirmation with an independent model before asking the user.
    AutoReview,
    /// Investigate and propose a plan; document mutations are denied.
    Plan,
    /// Skip human confirmation and automatic review, while still enforcing deny rules.
    Bypass,
}

impl Mode {
    /// Stable name for commands and model context.
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Edit => "edit",
            Self::AutoReview => "auto_review",
            Self::Plan => "plan",
            Self::Bypass => "bypass",
        }
    }

    /// Cycle ordinary modes. Auto-review and bypass require an explicit selection.
    pub fn next(self) -> Self {
        match self {
            Self::ReadOnly => Self::Edit,
            Self::Edit => Self::Plan,
            Self::Plan | Self::AutoReview | Self::Bypass => Self::ReadOnly,
        }
    }
}

/// Saved permission rules. Exact operation names, `edit_project.*`, or `*` are accepted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// The selected permission mode.
    pub mode: Mode,
    /// Operations that may run without confirmation, except mutations in read-only and plan mode.
    pub allow: Vec<String>,
    /// Operations that are denied in every mode, including bypass.
    pub deny: Vec<String>,
}

/// The permission decision before an operation runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Execute without a confirmation.
    Allow,
    /// Wait for an explicit user answer.
    Ask,
    /// Ask an independent reviewer; uncertainty falls back to user confirmation.
    Review,
    /// Refuse and explain the constraint to the model.
    Deny(String),
}

/// The classified, validated operation behind a tool call.
#[derive(Clone, Debug)]
pub struct Operation {
    /// Exact rule name, such as `edit_project.remove_track`.
    pub name: String,
    /// Whether it changes the document.
    pub mutating: bool,
    /// Whether it removes existing content, replaces the arrangement, or uses the network.
    pub confirm: bool,
}

/// Every operation exposed by the live agent, for rule editing and validation.
pub const OPERATIONS: &[&str] = &[
    "edit_project.edit_harmony",
    "edit_project.list_voices",
    "edit_project.inspect_singer",
    "edit_project.set_singer_voice",
    "edit_project.write_lyrics",
    "edit_project.sing",
    "edit_project.create_piano_cover",
    "edit_project.import_transcription",
    "export_audio_attachment",
    "edit_project.read_mixer_graph",
    "edit_project.connect_mixer",
    "edit_project.disconnect_mixer",
    "edit_project.add_mixer_effect",
    "edit_project.remove_mixer_effect",
    "edit_project.move_mixer_effect",
    "edit_project.enable_mixer_effect",
    "edit_project.set_mixer_send",
    "edit_project.mixer_effect_parameters",
    "edit_project.set_mixer_effect_parameter",
    "edit_project.set_mixer_track",
    "edit_project.inspect",
    "edit_project.inspect_audio",
    "edit_project.listen",
    "edit_project.read_notes",
    "edit_project.compose",
    "edit_project.add_track",
    "edit_project.rename_track",
    "edit_project.remove_track",
    "edit_project.set_instrument",
    "edit_project.set_level",
    "edit_project.normalize_mix",
    "edit_project.set_track_state",
    "edit_project.add_clip",
    "edit_project.add_note",
    "edit_project.add_notes",
    "edit_project.replace_notes",
    "edit_project.set_tempo",
    "edit_project.set_loop",
    "edit_project.remove_notes",
    "list_instruments",
    "search_instruments",
    "instrument_diagnostics",
    "similar_instruments",
    "list_presets",
    "list_progressions",
    "spec_reference",
    "search_documentation",
    "search_internet",
    "cli_command",
    "cli_file_change",
    "cli_file_read",
];

impl Operation {
    /// Validate the command before assigning permissions; unknown tools fail closed.
    pub fn parse(tool: &str, args: &serde_json::Value) -> Result<Self, String> {
        if tool == "edit_project" {
            let object = args.as_object().ok_or("Expected tool arguments")?;
            let command = object.get("command").ok_or("missing field `command`")?;
            if object.len() != 1 {
                return Err("Expected only the command field".into());
            }
            let parsed: crate::live_agent::Command =
                serde_json::from_value(command.clone()).map_err(|e| e.to_string())?;
            let action = command["action"].as_str().ok_or("Missing action")?;
            if let crate::live_agent::Command::MixerGraph { operation } = &parsed {
                return Ok(Self {
                    name: format!("edit_project.{}", operation.tool_name()),
                    mutating: !operation.is_read_only(),
                    confirm: operation.requires_confirmation(),
                });
            }
            let mutating = !matches!(
                parsed,
                crate::live_agent::Command::Inspect {}
                    | crate::live_agent::Command::ListVoices { .. }
                    | crate::live_agent::Command::InspectSinger { .. }
                    | crate::live_agent::Command::InspectAudio { .. }
                    | crate::live_agent::Command::Listen { .. }
                    | crate::live_agent::Command::ReadNotes { .. }
                    | crate::live_agent::Command::ListInstruments { .. }
                    | crate::live_agent::Command::SearchInstruments { .. }
                    | crate::live_agent::Command::InstrumentDiagnostics { .. }
                    | crate::live_agent::Command::SimilarInstruments { .. }
            );
            let confirm = matches!(
                parsed,
                crate::live_agent::Command::Listen { .. }
                    | crate::live_agent::Command::RemoveTrack { .. }
                    | crate::live_agent::Command::RemoveNotes { .. }
                    | crate::live_agent::Command::ReplaceNotes { .. }
                    | crate::live_agent::Command::Compose { replace: true, .. }
            );
            Ok(Self {
                name: if matches!(
                    action,
                    "list_instruments"
                        | "search_instruments"
                        | "instrument_diagnostics"
                        | "similar_instruments"
                ) {
                    action.into()
                } else {
                    format!("edit_project.{action}")
                },
                mutating,
                confirm,
            })
        } else if !tool.contains('.') && OPERATIONS.contains(&tool) {
            Ok(Self {
                name: tool.into(),
                mutating: matches!(tool, "cli_command" | "cli_file_change"),
                // A native read is free only inside the folder the CLI works in. The worker
                // decides that from the path the CLI asked for, and says so here; anything
                // else — another project, a home directory's keys — asks like a network call.
                confirm: matches!(tool, "search_internet" | "cli_command" | "cli_file_change")
                    || (tool == "cli_file_read" && args["within_project"] != true),
            })
        } else {
            Err(format!("Tool '{tool}' is not available to the live agent"))
        }
    }
}

impl Policy {
    /// Enforce deny rules first, then the mode, allow rules, and ordinary defaults.
    pub fn decide(&self, operation: &Operation) -> Decision {
        let matches = |rules: &[String]| {
            rules.iter().any(|rule| {
                rule == "*"
                    || rule == &operation.name
                    || (rule == "edit_project.*" && operation.name.starts_with("edit_project."))
            })
        };
        if matches(&self.deny) {
            return Decision::Deny(format!(
                "{} is prohibited by the user's deny list. Do not retry it.",
                operation.name
            ));
        }
        if self.mode == Mode::ReadOnly && operation.mutating {
            return Decision::Deny("Read-only mode does not permit changes or command approvals. Switch to Edit mode before requesting this operation.".into());
        }
        if self.mode == Mode::Plan && operation.mutating {
            return Decision::Deny("Plan mode does not permit changes or command approvals. Inspect and present a plan; the user must switch modes before execution.".into());
        }
        if self.mode == Mode::Bypass
            || matches(&self.allow)
            || (!operation.mutating && !operation.confirm)
            || (matches!(self.mode, Mode::Edit | Mode::AutoReview)
                && operation.mutating
                && !operation.confirm)
        {
            Decision::Allow
        } else if self.mode == Mode::AutoReview {
            Decision::Review
        } else {
            Decision::Ask
        }
    }

    /// Set one exact operation to default, allowed or denied. Removes the opposite exact rule.
    pub fn set_rule(&mut self, operation: &str, allow: Option<bool>) -> Result<(), String> {
        if !OPERATIONS.contains(&operation) && !["*", "edit_project.*"].contains(&operation) {
            return Err("Unknown operation name".into());
        }
        self.allow.retain(|name| name != operation);
        self.deny.retain(|name| name != operation);
        match allow {
            Some(true) => self.allow.push(operation.into()),
            Some(false) => self.deny.push(operation.into()),
            None => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_is_a_read_with_network_confirmation_and_deny_rules() {
        let operation = Operation::parse(
            "edit_project",
            &serde_json::json!({"command":{
                "action":"listen","start_bar":1,"bars":4,"focus":"balance","compare_to":null
            }}),
        )
        .unwrap();
        assert!(!operation.mutating);
        assert!(operation.confirm);
        let mut policy = Policy::default();
        assert_eq!(policy.decide(&operation), Decision::Ask);
        policy.mode = Mode::AutoReview;
        assert_eq!(policy.decide(&operation), Decision::Review);
        policy.set_rule("edit_project.listen", Some(false)).unwrap();
        policy.mode = Mode::Bypass;
        assert!(matches!(policy.decide(&operation), Decision::Deny(_)));
        policy.set_rule("edit_project.listen", Some(true)).unwrap();
        policy.mode = Mode::ReadOnly;
        assert_eq!(policy.decide(&operation), Decision::Allow);
    }

    #[test]
    fn harmony_and_vocals_respect_edit_read_only_plan_and_deny_rules() {
        for command in [
            serde_json::json!({"action":"edit_harmony","start_bar":1,"bars":4,"chords":"| I | V |"}),
            serde_json::json!({"action":"set_singer_voice","track":1,"voice":"/voice/test.voicevox.json"}),
            serde_json::json!({"action":"write_lyrics","clip":2,"text":"さくら"}),
            serde_json::json!({"action":"sing","track":1}),
        ] {
            let operation =
                Operation::parse("edit_project", &serde_json::json!({"command":command})).unwrap();
            assert!(operation.mutating);
            assert_eq!(Policy::default().decide(&operation), Decision::Allow);
            for mode in [Mode::ReadOnly, Mode::Plan] {
                assert!(matches!(
                    Policy {
                        mode,
                        ..Default::default()
                    }
                    .decide(&operation),
                    Decision::Deny(_)
                ));
            }
            let policy = Policy {
                mode: Mode::Bypass,
                deny: vec![operation.name.clone()],
                ..Default::default()
            };
            assert!(matches!(policy.decide(&operation), Decision::Deny(_)));
            assert!(OPERATIONS.contains(&operation.name.as_str()));
        }
        for command in [
            serde_json::json!({"action":"list_voices"}),
            serde_json::json!({"action":"inspect_singer","track":1}),
        ] {
            let operation =
                Operation::parse("edit_project", &serde_json::json!({"command":command})).unwrap();
            assert!(!operation.mutating);
            assert_eq!(
                Policy {
                    mode: Mode::ReadOnly,
                    ..Default::default()
                }
                .decide(&operation),
                Decision::Allow
            );
        }
    }

    #[test]
    fn native_approvals_require_consent_and_auto_review_never_overrides_denials() {
        for tool in ["cli_command", "cli_file_change"] {
            let operation =
                Operation::parse(tool, &serde_json::json!({"backend":"codex"})).unwrap();
            let mut policy = Policy::default();
            assert_eq!(policy.decide(&operation), Decision::Ask);
            policy.mode = Mode::AutoReview;
            assert_eq!(policy.decide(&operation), Decision::Review);
            policy.set_rule(tool, Some(false)).unwrap();
            assert!(matches!(policy.decide(&operation), Decision::Deny(_)));
            policy.set_rule(tool, Some(true)).unwrap();
            for mode in [Mode::ReadOnly, Mode::Plan] {
                policy.mode = mode;
                assert!(matches!(policy.decide(&operation), Decision::Deny(_)));
            }
            policy.mode = Mode::AutoReview;
            assert_eq!(policy.decide(&operation), Decision::Allow);
            assert_eq!(
                serde_json::to_string(&policy.mode).unwrap(),
                "\"auto_review\""
            );
        }
        let policy = Policy {
            mode: Mode::AutoReview,
            ..Policy::default()
        };
        assert_eq!(policy.decide(&edit()), Decision::Allow);
        assert_eq!(
            policy.decide(&Operation::parse("search_internet", &serde_json::json!({})).unwrap()),
            Decision::Review
        );
    }

    #[test]
    fn graph_reads_edits_and_removals_use_the_live_permission_boundary() {
        for (operation, read_only, removal) in [
            (serde_json::json!({"action":"read"}), true, false),
            (
                serde_json::json!({"action":"parameters","node":"effect:3"}),
                true,
                false,
            ),
            (
                serde_json::json!({"action":"connect","source":"track:1","target":"track:2","kind":"send"}),
                false,
                false,
            ),
            (
                serde_json::json!({"action":"disconnect","source":"track:1","target":"track:2","kind":"send"}),
                false,
                true,
            ),
            (
                serde_json::json!({"action":"remove_effect","node":"effect:3"}),
                false,
                true,
            ),
        ] {
            let parsed = Operation::parse(
                "edit_project",
                &serde_json::json!({"command":{"action":"mixer_graph","operation":operation}}),
            )
            .unwrap();
            assert!(OPERATIONS.contains(&parsed.name.as_str()));
            assert_eq!(parsed.mutating, !read_only);
            assert_eq!(parsed.confirm, removal);
            for mode in [Mode::ReadOnly, Mode::Plan] {
                let policy = Policy {
                    mode,
                    allow: vec!["edit_project.*".into()],
                    deny: vec![],
                };
                assert_eq!(matches!(policy.decide(&parsed), Decision::Allow), read_only);
            }
            assert_eq!(
                Policy::default().decide(&parsed),
                if removal {
                    Decision::Ask
                } else {
                    Decision::Allow
                }
            );
        }
    }
    fn edit() -> Operation {
        Operation::parse("edit_project", &serde_json::json!({"command":{"action":"add_track","name":"Lead","kind":"instrument"}})).unwrap()
    }

    #[test]
    fn modes_and_rules_have_explicit_precedence() {
        let mut policy = Policy::default();
        assert_eq!(policy.decide(&edit()), Decision::Allow);
        policy.mode = Mode::ReadOnly;
        assert!(matches!(policy.decide(&edit()), Decision::Deny(_)));
        policy.set_rule("edit_project.*", Some(true)).unwrap();
        assert!(matches!(policy.decide(&edit()), Decision::Deny(_)));
        policy.mode = Mode::Plan;
        assert!(matches!(policy.decide(&edit()), Decision::Deny(_)));
        policy.mode = Mode::Bypass;
        policy
            .set_rule("edit_project.add_track", Some(false))
            .unwrap();
        assert!(matches!(policy.decide(&edit()), Decision::Deny(_)));
    }

    #[test]
    fn read_only_denies_allow_listed_document_changes() {
        let mut policy = Policy {
            mode: Mode::ReadOnly,
            ..Policy::default()
        };
        policy.set_rule("edit_project.*", Some(true)).unwrap();

        assert!(matches!(policy.decide(&edit()), Decision::Deny(_)));
    }

    #[test]
    fn read_only_keeps_network_confirmation_and_exact_allow_rules() {
        let operation = Operation::parse(
            "search_internet",
            &serde_json::json!({"query":"music theory"}),
        )
        .unwrap();
        let mut policy = Policy {
            mode: Mode::ReadOnly,
            ..Policy::default()
        };
        assert_eq!(policy.decide(&operation), Decision::Ask);

        policy.set_rule("search_internet", Some(true)).unwrap();

        assert_eq!(policy.decide(&operation), Decision::Allow);
    }

    #[test]
    fn removal_and_network_require_confirmation_in_edit_mode() {
        for (tool, args) in [
            (
                "edit_project",
                serde_json::json!({"command":{"action":"remove_track","track":1}}),
            ),
            (
                "edit_project",
                serde_json::json!({"command":{"action":"compose","preset":"game-loop","replace":true}}),
            ),
            ("search_internet", serde_json::json!({"query":"music"})),
        ] {
            assert_eq!(
                Policy::default().decide(&Operation::parse(tool, &args).unwrap()),
                Decision::Ask
            );
        }
        assert!(Operation::parse("render", &serde_json::json!({})).is_err());
        assert!(Operation::parse("edit_project.remove_track", &serde_json::json!({})).is_err());
    }

    #[test]
    fn native_reads_are_free_only_inside_the_working_folder() {
        let inside = Operation::parse(
            "cli_file_read",
            &serde_json::json!({"backend":"claude","within_project":true}),
        )
        .unwrap();
        let outside = Operation::parse(
            "cli_file_read",
            &serde_json::json!({"backend":"claude","within_project":false}),
        )
        .unwrap();
        let unsaid = Operation::parse("cli_file_read", &serde_json::json!({})).unwrap();
        for mode in [Mode::ReadOnly, Mode::Edit, Mode::Plan] {
            let policy = Policy {
                mode,
                ..Policy::default()
            };
            assert_eq!(policy.decide(&inside), Decision::Allow, "{mode:?}");
            assert_eq!(policy.decide(&outside), Decision::Ask, "{mode:?}");
            assert_eq!(policy.decide(&unsaid), Decision::Ask, "{mode:?}");
        }
        let review = Policy {
            mode: Mode::AutoReview,
            ..Policy::default()
        };
        assert_eq!(review.decide(&outside), Decision::Review);
    }

    #[test]
    fn instrument_diagnostics_has_an_exact_user_editable_rule() {
        let operation = Operation::parse(
            "edit_project",
            &serde_json::json!({"command":{"action":"instrument_diagnostics","offset":0,"limit":20}}),
        )
        .unwrap();
        assert_eq!(operation.name, "instrument_diagnostics");
        assert!(!operation.mutating);

        let mut policy = Policy::default();
        policy
            .set_rule("instrument_diagnostics", Some(false))
            .unwrap();
        assert!(matches!(policy.decide(&operation), Decision::Deny(_)));
    }
}
