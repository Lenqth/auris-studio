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

/// List exact live instrument identifiers without saved-project-only arguments.
pub fn instruments() -> String {
    match super::headless() {
        Ok(session) => {
            let mut lines =
                vec!["Instrument IDs for set_instrument's instrument field:".to_string()];
            lines.extend(
                session
                    .registry()
                    .instruments()
                    .map(|instrument| format!("{}: {}", instrument.id, instrument.name)),
            );
            lines.join("\n")
        }
        Err(error) => format!("Cannot list instruments: {error}"),
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
            if matches!(action, "compose" | "mixer_graph") {
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
    let root = super::parameter_schema::<auris_session::mixer_graph::commands::GraphAction>();
    if let Some(branches) = root["oneOf"].as_array() {
        for branch in branches {
            let Some(action) = branch["properties"]["action"]["const"].as_str() else {
                continue;
            };
            let Some((_, name)) = auris_session::mixer_graph::commands::GRAPH_TOOLS
                .iter()
                .find(|(key, _)| *key == action)
            else {
                continue;
            };
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
            tools.push(Definition {
                name: (*name).into(),
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
    tools
}

/// Translate a flat tool into a validated canonical command. `None` identifies a reference tool.
/// Unknown fields and attempts to supply the action discriminator are rejected.
pub fn command(tool: &str, args: &Value) -> Result<Option<Command>, String> {
    if let Some((action, _)) = auris_session::mixer_graph::commands::GRAPH_TOOLS
        .iter()
        .find(|(_, name)| *name == tool)
    {
        let mut object = args
            .as_object()
            .ok_or("Expected a flat argument object")?
            .clone();
        if object.contains_key("action") || object.contains_key("command") {
            return Err("Pass graph fields directly, without action or command wrappers".into());
        }
        object.insert("action".into(), (*action).into());
        let operation = serde_json::from_value(Value::Object(object)).map_err(|error| {
            format!(
                "Invalid arguments for {tool}: {error}; read its tool definition for exact fields"
            )
        })?;
        return Ok(Some(Command::MixerGraph { operation }));
    }
    let action = match tool {
        "inspect_project" => "inspect",
        "list_instruments"
        | "instrument_diagnostics"
        | "search_instruments"
        | "similar_instruments"
        | "inspect_audio"
        | "listen"
        | "read_notes"
        | "add_track"
        | "rename_track"
        | "remove_track"
        | "set_instrument"
        | "add_notes"
        | "set_tempo"
        | "set_loop"
        | "set_level"
        | "normalize_mix"
        | "set_track_state"
        | "add_clip"
        | "add_note"
        | "remove_notes"
        | "replace_notes"
        | "edit_harmony"
        | "list_voices"
        | "inspect_singer"
        | "set_singer_voice"
        | "write_lyrics"
        | "sing" => tool,
        _ => return Ok(None),
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
    fn every_graph_action_has_a_flat_tool_and_canonical_command() {
        let definitions = definitions();
        for (action, tool) in auris_session::mixer_graph::commands::GRAPH_TOOLS {
            let definition = definitions
                .iter()
                .find(|definition| definition.name == *tool)
                .unwrap();
            assert!(definition.parameters["properties"].get("action").is_none());
            assert!(definition.parameters["properties"].get("project").is_none());
            assert!(!definition.description.is_empty());
            let args = match *action {
                "read" => serde_json::json!({}),
                "connect" | "disconnect" => {
                    serde_json::json!({"source":"track:1","target":"track:2","kind":"send"})
                }
                "add_effect" => {
                    serde_json::json!({"input":"track:1","effect":"auris.fx.compressor"})
                }
                "remove_effect" => serde_json::json!({"node":"effect:3"}),
                "move_effect" => serde_json::json!({"node":"effect:3","position":1}),
                "enable_effect" => serde_json::json!({"node":"effect:3","enabled":false}),
                "set_send" => serde_json::json!({"send_id":4,"level_db":-12}),
                "parameters" => serde_json::json!({"node":"effect:3"}),
                "set_parameter" => {
                    serde_json::json!({"node":"effect:3","param":"threshold_db","value":-18})
                }
                "set_strip" => serde_json::json!({"node":"track:1","gain_db":-12,"pan":0}),
                _ => panic!("unhandled graph action"),
            };
            let Some(Command::MixerGraph { operation }) = command(tool, &args).unwrap() else {
                panic!("graph command was not dispatched");
            };
            assert_eq!(operation.tool_name(), *tool);
            let mut wire = serde_json::to_value(&operation).unwrap();
            wire.as_object_mut().unwrap().remove("action");
            let Some(Command::MixerGraph {
                operation: restored,
            }) = command(tool, &wire).unwrap()
            else {
                panic!("graph command did not round-trip");
            };
            assert_eq!(
                serde_json::to_value(&operation).unwrap(),
                serde_json::to_value(restored).unwrap()
            );
            assert_eq!(definition.parameters["additionalProperties"], false);
            assert!(!definition.parameters.to_string().contains("\"$ref\""));
            let canonical = serde_json::json!({"command":Command::MixerGraph { operation }});
            assert!(
                auris_session::agent_policy::Operation::parse("edit_project", &canonical).is_ok()
            );
        }
        assert!(command("connect_mixer", &serde_json::json!({"action":"connect","source":"track:1","target":"track:2","kind":"send"})).is_err());
        assert!(
            command(
                "connect_mixer",
                &serde_json::json!({"source":"track:1","target":"track:2","kind":"chain"})
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
    fn nested_note_errors_identify_the_index_and_expected_type() {
        let error = command("add_notes", &serde_json::json!({"clip":1, "notes":[{"pitch":[60], "start_beat":0, "duration_beats":1, "velocity":0.8}]})).unwrap_err();
        assert!(
            error.contains("notes[0].pitch") && error.contains("integer"),
            "{error}"
        );
    }

    #[test]
    fn live_schemas_publish_runtime_limits() {
        let tools = definitions();
        let schema = |name: &str| {
            &tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap()
                .parameters
        };
        assert_eq!(schema("set_tempo")["properties"]["bpm"]["minimum"], 20.0);
        assert_eq!(schema("set_tempo")["properties"]["bpm"]["maximum"], 300.0);
        assert_eq!(schema("add_notes")["properties"]["notes"]["minItems"], 1);
        assert_eq!(schema("add_notes")["properties"]["notes"]["maxItems"], 256);
        assert_eq!(
            schema("add_notes")["properties"]["notes"]["items"]["properties"]["pitch"]["anyOf"][0]
                ["maximum"],
            127
        );
        assert_eq!(schema("inspect_audio")["properties"]["bars"]["maximum"], 8);
        for tool in ["add_clip", "set_loop"] {
            assert_eq!(schema(tool)["properties"]["start_bar"]["minimum"], 1);
            assert_eq!(schema(tool)["properties"]["bars"]["minimum"], 1);
            assert_eq!(schema(tool)["properties"]["bars"]["maximum"], 1024);
        }
        for (field, min, max) in [("gain_db", -60.0, 12.0), ("pan", -1.0, 1.0)] {
            assert_eq!(schema("set_level")["properties"][field]["minimum"], min);
            assert_eq!(schema("set_level")["properties"][field]["maximum"], max);
        }
        for (note, start, duration) in [
            (&schema("add_note")["properties"], "start", "beats"),
            (
                &schema("add_notes")["properties"]["notes"]["items"]["properties"],
                "start_beat",
                "duration_beats",
            ),
        ] {
            assert_eq!(note["pitch"]["anyOf"][0]["maximum"], 127);
            assert_eq!(note[start]["minimum"], 0.0);
            assert_eq!(note[duration]["exclusiveMinimum"], 0);
            assert_eq!(note["velocity"]["minimum"], 0.0);
            assert_eq!(note["velocity"]["maximum"], 1.0);
        }
    }

    #[test]
    fn every_live_operation_has_a_round_tripping_flat_contract() {
        let fixtures = [
            (
                "edit_harmony",
                serde_json::json!({"start_bar":1,"bars":4,"key":null,"chords":"| I | IV | V | I |","section":"Chorus","clear_chords":false}),
            ),
            ("list_voices", serde_json::json!({"offset":0})),
            ("inspect_singer", serde_json::json!({"track":1,"offset":0})),
            (
                "set_singer_voice",
                serde_json::json!({"track":1,"voice":"/voice/test.voicevox.json","speaker":null}),
            ),
            (
                "write_lyrics",
                serde_json::json!({"clip":1,"text":"あ","from":null}),
            ),
            ("sing", serde_json::json!({"track":1,"seed":null})),
            (
                "search_instruments",
                serde_json::json!({"query":"piano","limit":10,"offset":0,"refresh":false,"source":null,"library":null}),
            ),
            (
                "similar_instruments",
                serde_json::json!({"id":"s:test:1","limit":10,"source":null,"library":null}),
            ),
            (
                "instrument_diagnostics",
                serde_json::json!({"offset":0,"limit":10}),
            ),
            ("inspect_project", serde_json::json!({})),
            (
                "inspect_audio",
                serde_json::json!({"start_bar":1,"bars":1,"track":null}),
            ),
            (
                "listen",
                serde_json::json!({"start_bar":1,"bars":1,"focus":null,"compare_to":null}),
            ),
            (
                "list_instruments",
                serde_json::json!({"query":null,"offset":0,"refresh":false}),
            ),
            ("read_notes", serde_json::json!({"clip":1,"offset":0})),
            (
                "add_track",
                serde_json::json!({"name":"Drums","kind":"drum"}),
            ),
            ("rename_track", serde_json::json!({"track":1,"name":"Bass"})),
            ("remove_track", serde_json::json!({"track":1})),
            (
                "set_instrument",
                serde_json::json!({"track":1,"sound_id":"auris.synth.drumkit"}),
            ),
            (
                "set_level",
                serde_json::json!({"track":1,"gain_db":-6.0,"pan":0.0}),
            ),
            (
                "normalize_mix",
                serde_json::json!({"target_lufs":-23.0,"ceiling_db":-1.0}),
            ),
            (
                "set_track_state",
                serde_json::json!({"track":1,"mute":false,"solo":false}),
            ),
            (
                "add_clip",
                serde_json::json!({"track":1,"name":"Phrase","start_bar":1,"bars":4}),
            ),
            (
                "add_note",
                serde_json::json!({"clip":1,"pitch":60,"start":0.0,"beats":1.0,"velocity":0.5}),
            ),
            (
                "add_notes",
                serde_json::json!({"clip":1,"notes":[{"pitch":60,"start_beat":0.0,"duration_beats":1.0,"velocity":0.5}]}),
            ),
            ("set_tempo", serde_json::json!({"bpm":120.0})),
            ("set_loop", serde_json::json!({"start_bar":1,"bars":4})),
            ("remove_notes", serde_json::json!({"clip":1,"indices":[0]})),
            ("replace_notes", serde_json::json!({"clip":1,"notes":[]})),
        ];
        let tools = definitions();
        assert_eq!(
            tools.len(),
            fixtures.len() + 3 + auris_session::mixer_graph::commands::GRAPH_TOOLS.len()
        );
        for (name, args) in fixtures {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            let properties = tool.parameters["properties"].as_object().unwrap();
            assert_eq!(
                properties.keys().collect::<Vec<_>>(),
                args.as_object().unwrap().keys().collect::<Vec<_>>(),
                "{name}"
            );
            assert_eq!(tool.parameters["additionalProperties"], false, "{name}");
            assert!(!tool.parameters.to_string().contains("\"$ref\""), "{name}");
            let parsed = command(name, &args).unwrap().unwrap();
            let mut wire = serde_json::to_value(parsed).unwrap();
            wire.as_object_mut().unwrap().remove("action");
            assert_eq!(wire, args, "{name}");
        }
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
                .is_some()
        );
    }
    #[test]
    fn flat_catalog_covers_commands_without_a_tagged_union_or_file_destinations() {
        let tools = definitions();
        assert_eq!(
            tools.len(),
            31 + auris_session::mixer_graph::commands::GRAPH_TOOLS.len()
        );
        assert!(
            command("compose_song", &serde_json::json!({}))
                .unwrap()
                .is_none()
        );
        for tool in tools {
            assert_eq!(tool.parameters["type"], "object");
            assert!(tool.parameters.get("oneOf").is_none());
            assert!(tool.parameters["properties"].get("action").is_none());
            assert!(tool.parameters["properties"].get("command").is_none());
            assert!(tool.parameters["properties"].get("spec").is_none());
            if tool.name != transcription_import::NAME {
                assert!(tool.parameters["properties"].get("path").is_none());
            }
        }
    }
}
