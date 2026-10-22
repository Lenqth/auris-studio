//! Saved execution of the canonical session contracts; no legacy argument translation.

use crate::{ToolDefinition, live_agent};

#[cfg(test)]
const REMOVED_TOOLS: &[&str] = &[
    "add_note",
    "add_notes",
    "remove_notes",
    "replace_notes",
    "replace_notes_batch",
    "mixer",
    "effects",
    "set_effect",
    "routing",
    "set_level",
    "set_track_state",
    "set_instrument_param",
    "add_part",
    "describe",
    "notes",
    "similar_instruments",
    "list_instruments",
    "regenerate_clips",
    "set_tempo",
    "create_piano_cover",
    "transcribe_mixture",
];

pub(crate) fn definitions() -> Vec<ToolDefinition> {
    live_agent::definitions().into_iter().filter(|tool| {
        !matches!(tool.name.as_str(), "capabilities" | "discover_tools" | "tool_help" | "search_documentation" | "export_audio_attachment" | "read_report" | "transcribe_audio")
    }).map(|tool| {
        let name = tool.name;
        let mut parameters = tool.parameters;
        parameters["properties"].as_object_mut().expect("session object schema").insert("project".into(), serde_json::json!({"type":"string","description":"Project path or project_id from open_project/create_project."}));
        if parameters.get("required").is_none() { parameters["required"] = serde_json::json!([]); }
        parameters["required"].as_array_mut().expect("required session fields").push("project".into());
        let action = name.clone();
        ToolDefinition {
            name, description: tool.description, parameters,
            call: Box::new(move |mut args| {
                let object = args.as_object_mut().ok_or("Expected argument object")?;
                let project = object.remove("project").and_then(|value| value.as_str().map(str::to_owned)).ok_or("Invalid arguments: project is required")?;
                let command = if action == "edit_notes" {
                    serde_json::from_value::<crate::score_input::Args>(args).map_err(|e|format!("Invalid arguments: {e}"))?.command()?
                } else if action == "import_transcription" {
                    serde_json::from_value::<live_agent::transcription_import::Args>(args).map_err(|e|format!("Invalid arguments: {e}"))?.command(&auris_session::AnalysisControl::cancelled_by(crate::cancellation::current().flag_handle()))?
                } else { live_agent::command(&action, &args)?.ok_or("Unknown session tool")? };
                let mut session = crate::opened(&project)?;
                if let auris_session::live_agent::Command::InspectAudio {start_bar,bars,track}=command {
                    let report=session.audio_inspection_job(start_bar,bars,track)?.run(crate::cancellation::current().flag())?;
                    return serde_json::to_string(&report).map_err(|e|e.to_string());
                }
                if let auris_session::live_agent::Command::Listen {start_bar,bars,focus,compare_to}=command {
                    return crate::saved_listening::run(&mut session,start_bar,bars,focus.as_deref(),compare_to.as_deref());
                }
                let before = command.is_mutating().then(||session.project().clone());
                let text = if command.needs_worker() {
                    let cancellation=crate::cancellation::current();
                    let mut job=session.begin_agent_job(command,auris_session::Settings::load())?;
                    loop {
                        let result=job.run(cancellation.flag())?;
                        match session.continue_agent_job(result,cancellation.flag())? {
                            auris_session::live_agent::AgentStep::Done(text)=>break text,
                            auris_session::live_agent::AgentStep::Work(next) => job = next,
                        }
                    }
                } else {
                    session.agent_command_with_plugin_paths(command,&crate::sound_search::configured_plugin_paths())?
                };
                if before.as_ref().is_some_and(|before|session.project() != before) { crate::save_checkpointed(&mut session)?; }
                if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text)
                    && let Some(object) = value.as_object_mut()
                    && object.contains_key("unsaved") {
                    object.insert("unsaved".into(),session.is_dirty().into());
                    return Ok(value.to_string());
                }
                Ok(text)
            }),
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use auris_session::prelude::*;
    use serde_json::{Value, json};

    #[test]
    fn saved_creation_ids_are_direct_inputs_to_the_shared_editors() {
        let directory = tempfile::tempdir().unwrap();
        let call = |name, args| {
            serde_json::from_str::<Value>(
                &crate::runtime::call_saved(&crate::ToolGroups::default(), name, args).unwrap(),
            )
            .unwrap()
        };
        let created = call(
            "create_project",
            json!({"output":directory.path().join("Manual.auris"),"tempo":10,"meter":"6/8"}),
        );
        assert_eq!(created["tempo"].as_f64(), Some(10.0));
        assert!(created["tracks"][0]["track"].is_u64());
        let clip = call(
            "add_clip",
            json!({"project":created["project_id"],"track":created["tracks"][0]["track"],"name":"Phrase","start_bar":1,"bars":1}),
        );
        let read = call(
            "read_notes",
            json!({"project":created["project_id"],"clip":clip["clip"]}),
        );
        assert_eq!(read["total"], 0);
        assert_eq!(read["ticks_per_quarter"], 960);
    }

    #[test]
    fn saved_inspection_uses_the_common_range_and_never_saves() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Inspection.auris");
        let mut session = crate::headless().unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let clip = session
            .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks(3840))
            .unwrap();
        session
            .replace_notes(clip, vec![Note::new(60, Ticks::ZERO, Ticks(997))])
            .unwrap();
        session.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let result = crate::runtime::call_saved(
            &crate::ToolGroups::default(),
            "inspect_audio",
            json!({"project":path,"start_bar":1,"bars":1}),
        )
        .unwrap();
        let report: auris_session::audio_inspection::Inspection =
            serde_json::from_str(&result).unwrap();
        let visual = crate::audio_inspection::present(&report).unwrap();
        assert!(!visual.png.is_empty());
        assert!(report.columns > 0);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn saved_and_live_contracts_match_and_removed_names_have_no_dispatch() {
        let saved = crate::tool_catalog();
        for live in live_agent::definitions() {
            if let Some(definition) = definitions()
                .into_iter()
                .find(|tool| tool.name == live.name)
            {
                let mut schema = definition.parameters;
                schema["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("project");
                schema["required"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|value| value != "project");
                let mut expected = live.parameters;
                if expected.get("required").is_none() {
                    expected["required"] = json!([]);
                }
                assert_eq!(schema, expected, "{}", live.name);
            }
        }
        for name in REMOVED_TOOLS {
            assert!(!saved.iter().any(|tool| tool.name == *name), "{name}");
            assert!(
                crate::runtime::call_saved(&crate::ToolGroups::default(), name, json!({}))
                    .unwrap_err()
                    .contains("Unknown tool")
            );
            assert!(live_agent::command(name, &json!({})).unwrap().is_none());
        }
    }

    #[test]
    fn exact_score_and_performance_round_trip_through_saved_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Song.auris");
        let mut session = crate::headless().unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let clip = session
            .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks(3840))
            .unwrap();
        session.save(&path).unwrap();
        let call = |name, args| {
            crate::runtime::call_saved(&crate::ToolGroups::default(), name, args).unwrap()
        };
        let read = || {
            serde_json::from_str::<Value>(&call(
                "read_notes",
                json!({"project":path,"clip":clip.0}),
            ))
            .unwrap()
        };
        let initial = read();
        let edits = json!([{"kind":"replace","clip":clip.0,"notes":[{"pitch":60,"start_tick":13,"length_ticks":97,"velocity":0.73125}]}]);
        call(
            "edit_notes",
            json!({"project":path,"expected_revision":initial["revision"],"edits":edits}),
        );
        let current = read();
        assert_eq!(current["notes"][0]["start_tick"], 13);
        assert_eq!(current["notes"][0]["length_ticks"], 97);
        let bytes = std::fs::read(&path).unwrap();
        call(
            "edit_notes",
            json!({"project":path,"expected_revision":current["revision"],"edits":edits}),
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        call(
            "set_performance",
            json!({"project":path,"clip":clip.0,"settings":{"gate":0.5}}),
        );
        let performance: Value = serde_json::from_str(&call(
            "inspect_performance",
            json!({"project":path,"clip":clip.0}),
        ))
        .unwrap();
        assert_eq!(performance["settings"]["gate"], 0.5);
        assert_eq!(read()["notes"], current["notes"]);
    }
}
