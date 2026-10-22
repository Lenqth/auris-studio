//! Flat model-facing operations, translated to the same permission-checked live commands.
use auris_session::live_agent::Command;
use serde_json::Value;

/// A small, flat tool definition derived from the live command contract.
pub struct Definition {
    /// Provider-facing operation name.
    pub name: String,
    /// Operation-specific instructions.
    pub description: String,
    /// Flat JSON object schema, without action discriminators or nested command objects.
    pub parameters: Value,
}

/// Model-facing attachment-to-piano request; the transport owns attachment identities.
pub mod piano_cover {
    /// Stable live tool name.
    pub const NAME: &str = "create_piano_cover";
    /// Guidance shared by all live model transports.
    pub const DESCRIPTION: &str = "Transcribe an attached audio file with the bundled Basic Pitch model on local CPU and add all estimated pitched notes as a new editable piano track. Pass attachment_id from this conversation's attached audio metadata as attachment. Supports overlapping notes, source timing and relative dynamics, up to ten minutes. No model setup, Python or audio upload is needed. Does not replace existing tracks, change tempo or save the project; one Undo removes the draft. Piano/isolated pitched recordings work best; dense mixes can produce extra/missing notes and drums are not separated. Review the draft with read_notes/inspect_audio and explain uncertainty instead of claiming faithful transcription or listening. Do not invent notes from filenames, BPM or chord estimates.";
    /// An opaque conversation-local reference and insertion options.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Exact attachment_id supplied by the host; never a file path.
        pub attachment: String,
        /// New track/clip name; defaults to Piano cover.
        pub name: Option<String>,
        /// Zero-based insertion beat; defaults to zero. Source performance timing is retained.
        #[schemars(range(min = 0, max = 1_000_000))]
        pub at_beat: Option<f64>,
    }
}

/// Access to an isolated original-audio copy for local analysis tools.
pub mod audio_attachment {
    /// Stable live tool name.
    pub const NAME: &str = "export_audio_attachment";
    /// Guidance shared by all live transports.
    pub const DESCRIPTION: &str = "Get a separate local analysis copy of an attached original audio file. Supply attachment_id, never a source path. Returns an absolute audio_path, original byte SHA-256 and size without resampling or changing channels. Use local analysis code/tools when the model cannot hear audio. No project edit, automatic model download or audio upload. One current copy per attachment; re-export replaces the previous path, and conversation expiry removes it. Do not modify the returned copy or claim listening from file delivery alone. Use import_transcription for the resulting note JSON; create_piano_cover remains the bundled local recognizer shortcut.";
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
    pub const DESCRIPTION: &str = "Import an already computed piano note draft from any recognizer, as one new editable physical-piano track. Supply exactly one of transcription (inline) or path (absolute local JSON file, at most 32 MiB). JSON has source:{method, optional model/artifact_sha256/audio_sha256}, seconds, notes:[{pitch,start,end,strength}], optional limitations. Times are original source seconds; pitch 21..108, strength 0..1, finite positive duration up to ten minutes, 1..200000 notes. Preserve overlapping pitches. Declare uniform dynamics/simplifications in limitations when uncertain; do not invent a transcription from filenames, tempo or chords. Provenance is a caller claim, not verification. export_audio_attachment provides actual source bytes for local analysis. Keeps existing music and tempo; one Undo, unsaved. Review with read_notes/inspect_audio.";
    /// Inline or file input and insertion options.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Inline draft; omit when path is provided.
        pub transcription: Option<auris_session::NoteTranscription>,
        /// Absolute local JSON path; omit when transcription is provided.
        pub path: Option<String>,
        /// New track/clip name; defaults to Transcription.
        pub name: Option<String>,
        /// Insertion beat, defaults to zero; source-relative timing is preserved.
        pub at_beat: Option<f64>,
    }
}

/// Small per-action schemas replace the model-facing tagged union. The advanced TOML command
/// remains an internal command, but is not offered to the model.
pub fn definitions() -> Vec<Definition> {
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
            if action == piano_cover::NAME {
                tools.push(Definition {
                    name: piano_cover::NAME.into(),
                    description: piano_cover::DESCRIPTION.into(),
                    parameters: super::parameter_schema::<piano_cover::Args>(),
                });
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
    tools.extend(super::runtime::live_references());
    tools
}

/// Translate a flat tool into a validated canonical command. `None` identifies a reference tool.
/// Unknown fields and attempts to supply the action discriminator are rejected.
pub fn command(tool: &str, args: &Value) -> Result<Option<Command>, String> {
    if tool == "edit_notes" {
        let args: super::score_input::Args = serde_json::from_value(args.clone())
            .map_err(|e| format!("Invalid arguments for edit_notes: {e}"))?;
        if args.source.is_some() {
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
            piano_cover::NAME | transcription_import::NAME | audio_attachment::NAME
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
                .unwrap_or_else(|| format!("Expected arguments: {}", definition.parameters));
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
        return Some(format!("{path} must match one of {}", schema["anyOf"]));
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
                        "{path}.{field} is required; expected {}",
                        schema["properties"][field]
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
    fn piano_cover_schema_accepts_only_a_scoped_attachment_and_insertion_options() {
        let definition = definitions()
            .into_iter()
            .find(|d| d.name == piano_cover::NAME)
            .unwrap();
        assert!(
            definition.parameters["properties"]
                .get("transcription")
                .is_none()
        );
        assert!(
            definition.parameters["properties"]
                .get("attachment")
                .is_some()
        );
        assert!(
            command(
                piano_cover::NAME,
                &serde_json::json!({"attachment":"audio:test"})
            )
            .unwrap()
            .is_none()
        );
        assert!(
            serde_json::from_value::<piano_cover::Args>(
                serde_json::json!({"attachment":"audio:test","notes":[]})
            )
            .is_err()
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
