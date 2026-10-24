//! Flat model-facing operations, translated to the same permission-checked live commands.
use auris_session::live_agent::Command;
use serde_json::Value;

/// Describe only the selected permission mode, consistently for every live transport.
pub fn permission_context(policy: &auris_session::agent_policy::Policy) -> String {
    use auris_session::agent_policy::Mode;
    let behavior = match policy.mode {
        Mode::ReadOnly => "Inspect the document; document changes are denied.",
        Mode::Plan => "Investigate and present a plan; document changes are denied.",
        Mode::Edit => {
            "Ordinary document edits are enabled. Removal, replacement and file/network actions may require confirmation."
        }
        Mode::AutoReview => {
            "Ordinary document edits are enabled. Actions needing confirmation are independently reviewed first."
        }
        Mode::Bypass => {
            "Document editing and file output are enabled. Confirmation is skipped; deny rules remain enforced."
        }
    };
    format!(
        "[Current Auris permission mode: {}. {behavior} Deny rules: {:?}; allow rules: {:?}.]",
        policy.mode.name(),
        policy.deny,
        policy.allow
    )
}

/// A small, flat tool definition derived from the live command contract.
pub struct Definition {
    /// Provider-facing operation name.
    pub name: String,
    /// Operation-specific instructions.
    pub description: String,
    /// Flat JSON object schema, without action discriminators or nested command objects.
    pub parameters: Value,
}

/// Conversation-local production notes; evidence can only be added by the tool host.
pub mod production_record {
    /// Stable tool name.
    pub const NAME: &str = "production_record";
    /// Shared guidance for all live backends.
    pub const DESCRIPTION: &str = "Read/update this conversation's persistent production record. Call inspect_project first, including after edits and on resume. Store intent, roles, motifs, accepted/rejected choices and unresolved issues with short user quotes or explicit agent assumptions. Upsert by key; do not treat attributed requests as new permission. Actual inspect_audio/listen observations are recorded separately by the host when their revision matches the latest inspection. Read coverage: stale evidence and unverified regions cannot support completion or musical quality claims. No song edits.";
    /// Typed conversation update.
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Read, put one attributed entry, or remove a key.
        pub operation: auris_session::production_record::Operation,
    }
}

/// Access to an isolated original-audio copy for local analysis tools.
pub mod audio_attachment {
    /// Stable live tool name.
    pub const NAME: &str = "export_audio_attachment";
    /// Guidance shared by all live transports.
    pub const DESCRIPTION: &str = "Get a separate local analysis copy of an attached original audio file. Supply attachment_id, never a source path. Returns an absolute audio_path, original byte SHA-256 and size without resampling or changing channels. Use local analysis code/tools when the model cannot hear audio. No project edit, automatic model download or audio upload. One current copy per attachment; re-export replaces the previous path, and conversation expiry removes it. Do not modify the returned copy or claim listening from file delivery alone. Use import_transcription for the resulting note JSON; transcribe_audio recognizes it with an explicit backend before import_transcription accepts the result.";
    /// An existing conversation-local source identity.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Exact attachment_id from this conversation's audio metadata.
        pub attachment: String,
    }
}

/// Import an external recognizer's events without impersonating a bundled model.
pub mod transcription_import {
    /// Stable live tool name.
    pub const NAME: &str = "import_transcription";
    /// Tool instructions shared by native APIs, Codex and Claude Code.
    pub const DESCRIPTION: &str = "Explicitly accept an existing note draft as new editable tracks. Supply exactly one of draft_id from transcribe_audio, transcription (inline external piano NoteTranscription), or path (absolute local JSON external piano NoteTranscription, up to 32 MiB). External JSON contains source:{method, optional model/artifact_sha256/audio_sha256}, seconds, notes:[{pitch,start,end,strength}], optional limitations. Piano pitch 21..108, strength 0..1, finite positive source duration up to ten minutes and 1..200000 notes. Times are source seconds; at_tick is an absolute insertion tick (defaults to 0). Provenance claims do not verify musical accuracy. Existing music/tempo and overlapping notes are preserved; review with read_notes/inspect_audio. Draft IDs expire after 30 minutes, restart or bounded-cache eviction.";
    /// Exactly one immutable recognized draft or explicitly claimed external piano draft.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Opaque draft_id returned by transcribe_audio; omit transcription/path.
        pub draft_id: Option<String>,
        /// Inline external piano note draft; omit draft_id/path.
        pub transcription: Option<auris_session::NoteTranscription>,
        /// Absolute local JSON NoteTranscription file, up to 32 MiB; omit draft_id/transcription.
        pub path: Option<String>,
        /// Optional new piano track/clip name, up to 256 bytes.
        pub name: Option<String>,
        /// Nonnegative absolute insertion tick; defaults to zero.
        pub at_tick: Option<i64>,
    }
    impl Args {
        /// Reject ambiguous sources and invalid insertion metadata before file authorization.
        pub fn validate(&self) -> Result<(), String> {
            if [
                self.draft_id.is_some(),
                self.transcription.is_some(),
                self.path.is_some(),
            ]
            .into_iter()
            .filter(|present| *present)
            .count()
                != 1
            {
                return Err("Supply exactly one of draft_id, transcription or path".into());
            }
            if self
                .name
                .as_ref()
                .is_some_and(|name| name.trim().is_empty() || name.len() > 256)
                || self.at_tick.is_some_and(|tick| tick < 0)
            {
                return Err("Use a nonempty name up to 256 bytes and nonnegative at_tick".into());
            }
            Ok(())
        }
        /// Load once and validate before binding permissions to the exact immutable draft.
        pub fn command(
            self,
            control: &auris_session::AnalysisControl,
        ) -> Result<auris_session::live_agent::Command, String> {
            self.validate()?;
            let name = self.name.unwrap_or_else(|| "Transcription".into());
            let at_tick = self.at_tick.unwrap_or(0);
            if name.trim().is_empty() || name.len() > 256 || at_tick < 0 {
                return Err("Use a nonempty name up to 256 bytes and nonnegative at_tick".into());
            }
            let draft = match (self.draft_id, self.transcription, self.path) {
                (Some(id), None, None) => {
                    crate::transcription_tools::resolve(&id)?.as_ref().clone()
                }
                (None, Some(transcription), None) => {
                    auris_session::TranscriptionDraft::Piano { transcription }
                }
                (None, None, Some(path)) => auris_session::TranscriptionDraft::Piano {
                    transcription: auris_session::read_transcription_file(
                        std::path::Path::new(&path),
                        control,
                    )
                    .map_err(|e| e.to_string())?,
                },
                _ => return Err("Supply exactly one of draft_id, transcription or path".into()),
            };
            draft.validate()?;
            Ok(auris_session::live_agent::Command::ImportTranscription {
                draft: Box::new(draft),
                name,
                at_tick,
            })
        }
    }
}

/// Small per-action schemas replace the model-facing tagged union. The advanced TOML command
/// remains an internal command, but is not offered to the model.
pub fn definitions() -> Vec<Definition> {
    definitions_with_arrangement(auris_session::arrangement::enabled())
}

fn definitions_with_arrangement(arrangement_enabled: bool) -> Vec<Definition> {
    // Keep enum simplification, inlining and numeric annotations identical to the MCP catalog.
    let root = super::parameter_schema::<Command>();
    let mut tools = Vec::new();
    if let Some(branches) = root["oneOf"].as_array() {
        for branch in branches {
            let Some(action) = branch["properties"]["action"]["const"].as_str() else {
                continue;
            };
            if action == "compose" {
                continue;
            }
            if action == "arrangement" && !arrangement_enabled {
                continue;
            }
            if action == transcription_import::NAME {
                tools.push(Definition {
                    name: transcription_import::NAME.into(),
                    description: transcription_import::DESCRIPTION.into(),
                    parameters: super::parameter_schema::<transcription_import::Args>(),
                });
                continue;
            }
            let mut parameters = branch.clone();
            let description = parameters["description"]
                .as_str()
                .unwrap_or(action)
                .to_owned();
            parameters.as_object_mut().unwrap().remove("description");
            parameters["properties"]
                .as_object_mut()
                .unwrap()
                .remove("action");
            if let Some(required) = parameters["required"].as_array_mut() {
                required.retain(|value| value != "action");
            }
            if action == "edit_notes" {
                parameters = super::parameter_schema::<super::score_input::Args>();
            }
            tools.push(Definition {
                name: if action == "inspect" {
                    "inspect_project".into()
                } else {
                    action.into()
                },
                description,
                parameters,
            });
        }
    }
    tools.push(Definition {
        name: audio_attachment::NAME.into(),
        description: audio_attachment::DESCRIPTION.into(),
        parameters: super::parameter_schema::<audio_attachment::Args>(),
    });
    tools.push(Definition {
        name: crate::transcription_tools::NAME.into(),
        description: crate::transcription_tools::DESCRIPTION.into(),
        parameters: super::parameter_schema::<crate::transcription_tools::Args>(),
    });
    tools.extend(super::runtime::live_references());
    tools.push(Definition {
        name: production_record::NAME.into(),
        description: production_record::DESCRIPTION.into(),
        parameters: super::parameter_schema::<production_record::Args>(),
    });
    tools
}

/// Translate a flat tool into a validated canonical command. `None` identifies a reference tool.
/// Unknown fields and attempts to supply the action discriminator are rejected.
pub fn command(tool: &str, args: &Value) -> Result<Option<Command>, String> {
    if tool == production_record::NAME {
        serde_json::from_value::<production_record::Args>(args.clone())
            .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    if tool == "edit_notes" {
        let args: super::score_input::Args = serde_json::from_value(args.clone())
            .map_err(|e| format!("Invalid arguments for edit_notes: {e}"))?;
        if args.source_path().is_some() {
            return Err(
                "File input must be loaded and authorized by the tool host before editing".into(),
            );
        }
        return args.command().map(Some);
    }
    if !definitions()
        .iter()
        .any(|definition| definition.name == tool)
        || super::runtime::live_references()
            .iter()
            .any(|definition| definition.name == tool)
        || matches!(
            tool,
            transcription_import::NAME | audio_attachment::NAME | crate::transcription_tools::NAME
        )
    {
        return Ok(None);
    }
    let action = if tool == "inspect_project" {
        "inspect"
    } else {
        tool
    };
    let mut object = args
        .as_object()
        .ok_or("Expected a flat argument object")?
        .clone();
    if object.contains_key("action") || object.contains_key("command") {
        return Err("Pass the fields directly, without action or command wrappers".into());
    }
    object.insert("action".into(), action.into());
    serde_json::from_value(Value::Object(object))
        .map(Some)
        .map_err(|error| {
            let Some(definition) = definitions().into_iter().find(|item| item.name == tool) else {
                return error.to_string();
            };
            let hint = argument_hint(&definition.parameters, args, "arguments")
                .unwrap_or_else(|| format!("Use tool_help with name {tool} for exact arguments"));
            format!("Invalid arguments for {tool}: {hint}. {error}")
        })
}

// Serde's internally tagged enum buffers its fields, losing their paths on failure. These
// schema-derived hints run only after deserialization fails; they are not a schema validator
// or an alternative acceptance gate. Serde and the session remain authoritative.
pub(crate) fn argument_hint(schema: &Value, value: &Value, path: &str) -> Option<String> {
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array)
        && branches
            .iter()
            .all(|branch| argument_hint(branch, value, path).is_some())
    {
        return Some(format!(
            "{path} has an invalid argument type; use tool_help for its schema"
        ));
    }
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        for tag in ["action", "kind"] {
            let choices = branches
                .iter()
                .map(|branch| branch["properties"][tag]["const"].as_str())
                .collect::<Option<Vec<_>>>();
            if let Some(choices) = choices.filter(|choices| !choices.is_empty()) {
                if let Some(index) = value
                    .get(tag)
                    .and_then(Value::as_str)
                    .and_then(|selected| choices.iter().position(|choice| *choice == selected))
                {
                    return argument_hint(&branches[index], value, path);
                }
                return Some(format!(
                    "{path}.{tag} must be one of {}; use a flat tagged object",
                    choices.join(", ")
                ));
            }
        }
    }
    if let Some(kind) = schema.get("type") {
        let matches = |kind: &Value| match kind.as_str() {
            Some("string") => value.is_string(),
            Some("object") => value.is_object(),
            Some("array") => value.is_array(),
            Some("integer") => value.is_u64() || value.is_i64(),
            Some("number") => value.is_number(),
            Some("boolean") => value.is_boolean(),
            Some("null") => value.is_null(),
            _ => true,
        };
        let valid = kind
            .as_array()
            .map_or_else(|| matches(kind), |kinds| kinds.iter().any(matches));
        if !valid {
            let choices = schema
                .get("enum")
                .map(|choices| format!("; choose one of {choices}"))
                .unwrap_or_default();
            return Some(format!("{path} must be {kind}{choices}"));
        }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array)
        && !choices.contains(value)
    {
        return Some(format!("{path} must be one of {}", schema["enum"]));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Some(format!(
                        "{path}.{field} is required; use tool_help for exact fields"
                    ));
                }
            }
        }
        if let Some(properties) = schema["properties"].as_object() {
            for (field, value) in object {
                match properties.get(field) {
                    Some(field_schema) => {
                        if let Some(hint) =
                            argument_hint(field_schema, value, &format!("{path}.{field}"))
                        {
                            return Some(hint);
                        }
                    }
                    None if schema["additionalProperties"] == false => {
                        return Some(format!(
                            "{path}.{field} is not accepted; allowed fields: {}",
                            properties
                                .keys()
                                .map(String::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    None => {}
                }
            }
        }
    }
    if let Some(values) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (index, value) in values.iter().enumerate() {
            if let Some(hint) = argument_hint(items, value, &format!("{path}[{index}]")) {
                return Some(hint);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrangement_catalog_is_opt_in_and_keeps_typed_bounded_operations() {
        assert!(
            !definitions_with_arrangement(false)
                .iter()
                .any(|d| d.name == "arrangement")
        );
        let definition = definitions_with_arrangement(true)
            .into_iter()
            .find(|d| d.name == "arrangement")
            .unwrap();
        let operation = &definition.parameters["properties"]["operation"];
        let branches = operation["oneOf"].as_array().unwrap();
        let kinds: Vec<_> = branches
            .iter()
            .map(|b| b["properties"]["kind"]["const"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            vec!["inspect_breathing", "inspect", "validate", "apply"]
        );
        let validation = branches
            .iter()
            .find(|b| b["properties"]["kind"]["const"] == "validate")
            .unwrap();
        assert_eq!(validation["properties"]["changes"]["maxItems"], 16);
        assert!(
            validation["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("expected_revision"))
        );
        let mut compact = definition.parameters;
        crate::compact_parameters(&mut compact);
        let bytes = serde_json::to_vec(&compact).unwrap().len();
        eprintln!("arrangement compact parameter schema: {bytes} bytes");
        assert!(
            bytes < 32 * 1024,
            "arrangement schema grew beyond 32 KiB: {bytes}"
        );
    }

    #[test]
    fn recipe_constraints_survive_the_native_controller_schema() {
        let mut tool = definitions()
            .into_iter()
            .find(|t| t.name == "edit_recipe")
            .unwrap();
        crate::tool_exposure::compact_parameters(&mut tool.parameters);
        let patch = tool.parameters["properties"]["operation"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["properties"]["kind"]["const"] == "patch")
            .unwrap();
        let settings = &patch["properties"]["settings"]["properties"];
        assert_eq!(
            settings["subdivision"]["enum"],
            serde_json::json!([
                "eighth",
                "sixteenth",
                "eighth-triplet",
                "sixteenth-triplet",
                null
            ])
        );
        assert_eq!(settings["rhythm"]["pattern"], "^[xXo.~ \\t|-]*$");
        assert_eq!(settings["rhythm"]["maxLength"], 4096);
        let description = crate::tool_exposure::concise_description(&tool.description);
        assert!(description.contains("Rhythm must use repeating grid steps"));
        assert!(description.contains("density and gate, not subdivision"));
    }

    #[test]
    fn invalid_nested_operations_return_field_guidance_without_dumping_schemas() {
        let error = command(
            "mixer_graph",
            &serde_json::json!({"operation":{"set_strip":{"track":2,"gain_db":-6}}}),
        )
        .unwrap_err();
        assert!(error.contains("arguments.operation.action"), "{error}");
        assert!(error.contains("set_strip"), "{error}");
        assert!(error.len() < 1000, "{}", error.len());
        let missing = command("mixer_graph", &serde_json::json!({})).unwrap_err();
        assert!(
            missing.contains("arguments.operation is required"),
            "{missing}"
        );
        assert!(missing.len() < 300, "{}", missing.len());
    }

    #[test]
    fn permission_context_explains_only_the_selected_mode() {
        use auris_session::agent_policy::{Mode, Policy};
        for mode in [
            Mode::ReadOnly,
            Mode::Plan,
            Mode::Edit,
            Mode::AutoReview,
            Mode::Bypass,
        ] {
            let text = permission_context(&Policy {
                mode,
                ..Default::default()
            });
            assert!(text.contains(mode.name()));
            assert_eq!(
                text.contains("document changes are denied"),
                matches!(mode, Mode::ReadOnly | Mode::Plan)
            );
            if matches!(mode, Mode::Edit | Mode::AutoReview | Mode::Bypass) {
                assert!(!text.contains("read-only"));
            }
        }
    }

    #[test]
    fn performance_edits_publish_bounded_units_and_partial_typed_settings() {
        let tools = definitions();
        let schema = &tools
            .iter()
            .find(|t| t.name == "set_performance")
            .unwrap()
            .parameters;
        let settings = &schema["properties"]["settings"]["properties"];
        assert_eq!(settings["gate"]["minimum"], 0.05);
        assert_eq!(settings["swing_percent"]["minimum"], 50);
        assert_eq!(settings["swing_percent"]["maximum"], 75);
        assert_eq!(settings["reset"]["maxItems"], 12);
        let parsed = command(
            "set_performance",
            &serde_json::json!({"clip":2,"settings":{"gate":0.5,"expression":{"delay_ms":-12}}}),
        )
        .unwrap()
        .unwrap();
        assert!(matches!(parsed, Command::SetPerformance { .. }));
        assert!(
            command(
                "set_performance",
                &serde_json::json!({"clip":2,"settings":{"pitch":{"unknown":1}}}),
            )
            .is_err()
        );
    }

    #[test]
    fn generated_phrases_are_available_as_flat_live_commands() {
        let args = serde_json::json!({"track":1,"part":"lead","start_bar":3,"bars":4,"seed":42});
        assert!(command("generate_phrase", &args).unwrap().is_some());
        let tools = definitions();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "generate_phrase")
            .unwrap();
        assert_eq!(tool.parameters["properties"]["bars"]["maximum"], 1024);
        assert_eq!(
            tool.parameters["properties"]["part"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
        assert!(tool.parameters["properties"].get("project").is_none());
    }

    #[test]
    fn live_composition_and_vocal_workflows_are_callable_without_saved_project_arguments() {
        for (name, args) in [
            (
                "edit_harmony",
                serde_json::json!({"start_bar":1,"bars":4,"key":"D major","chords":"| I | IV | V | I |"}),
            ),
            (
                "edit_harmony",
                serde_json::json!({"start_bar":5,"bars":8,"section":"Chorus"}),
            ),
            ("list_voices", serde_json::json!({})),
            ("inspect_singer", serde_json::json!({"track":1})),
            (
                "set_singer_voice",
                serde_json::json!({"track":1,"voice":"/voices/test.voicevox.json","speaker":"First"}),
            ),
            (
                "write_lyrics",
                serde_json::json!({"clip":2,"text":"さくら"}),
            ),
            ("sing", serde_json::json!({"track":1,"seed":42})),
        ] {
            let definitions = definitions();
            assert!(
                definitions.iter().any(|tool| tool.name == name),
                "missing {name}"
            );
            assert!(command(name, &args).unwrap().is_some(), "unresolved {name}");
        }
    }

    #[test]
    fn transcription_schema_is_read_only_and_legacy_shortcut_is_absent() {
        assert!(!definitions().iter().any(|d| d.name == "create_piano_cover"));
        let definition = definitions()
            .into_iter()
            .find(|d| d.name == crate::transcription_tools::NAME)
            .unwrap();
        assert!(definition.parameters["properties"].get("backend").is_some());
        assert!(definition.parameters["properties"].get("project").is_none());
        assert!(
            command(
                "create_piano_cover",
                &serde_json::json!({"attachment":"missing"})
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn track_kind_is_a_string_enum_and_array_errors_explain_the_field() {
        let tool = definitions()
            .into_iter()
            .find(|tool| tool.name == "add_track")
            .unwrap();
        assert_eq!(tool.parameters["properties"]["kind"]["type"], "string");
        assert_eq!(
            tool.parameters["properties"]["kind"]["enum"],
            serde_json::json!(["instrument", "drum", "singer", "audio", "bus"])
        );
        assert!(
            command(
                "add_track",
                &serde_json::json!({"name":"Drums", "kind":"drum"})
            )
            .unwrap()
            .is_some()
        );
        let error = command(
            "add_track",
            &serde_json::json!({"name":"Drums", "kind":["drum"]}),
        )
        .unwrap_err();
        assert!(
            error.contains("kind") && error.contains("string") && error.contains("drum"),
            "{error}"
        );
    }

    #[test]
    fn missing_unknown_and_wrongly_wrapped_fields_are_actionable() {
        for (args, expected) in [
            (
                serde_json::json!({"name":"Drums"}),
                "arguments.kind is required",
            ),
            (
                serde_json::json!({"name":"Drums","kind":"drum","instrument":"kit"}),
                "arguments.instrument is not accepted",
            ),
            (
                serde_json::json!({"name":"Drums","kind":"percussion"}),
                "kind must be one of",
            ),
            (
                serde_json::json!({"command":{"name":"Drums","kind":"drum"}}),
                "without action or command wrappers",
            ),
            (
                serde_json::json!([{"name":"Drums","kind":"drum"}]),
                "flat argument object",
            ),
        ] {
            let error = command("add_track", &args).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        assert!(
            command("list_instruments", &serde_json::json!({}))
                .unwrap()
                .is_none()
        );
    }
}
