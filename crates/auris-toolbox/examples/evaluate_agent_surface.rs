//! Print the actual saved-project and live-agent tool surfaces for workflow audits.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let saved = auris_toolbox::tool_catalog();
    // Optional configured host tools (such as internet search) are not counted here.
    let live = auris_toolbox::live_agent::definitions();
    let compact_bytes = |schemas: Vec<serde_json::Value>| {
        let schemas: Vec<_> = schemas
            .into_iter()
            .map(|mut schema| {
                auris_toolbox::compact_parameters(&mut schema);
                schema
            })
            .collect();
        serde_json::to_vec(&schemas).unwrap().len()
    };
    let harmony = auris_toolbox::live_agent::command(
        "edit_harmony",
        &serde_json::json!({"start_bar":1,"bars":4,"chords":"| I | IV | V | I |"}),
    )?;
    let mut session = auris_session::Session::new(auris_session::SessionOptions::headless())?;
    let singer = auris_toolbox::live_agent::command(
        "add_track",
        &serde_json::json!({"name":"Vocal workflow probe","kind":"singer"}),
    )?
    .ok_or("add_track did not resolve to a live command")?;
    let singer_reply = session.agent_command(singer)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "saved_tools": saved.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>(),
            "live_tools": live.iter().map(|tool| &tool.name).collect::<Vec<_>>(),
            "saved_parameter_bytes": compact_bytes(saved.iter().map(|tool| tool.parameters.clone()).collect()),
            "live_parameter_bytes": compact_bytes(live.iter().map(|tool| tool.parameters.clone()).collect()),
            "live_edit_harmony_resolves_to_command": harmony.is_some(),
            "live_singer_creation": singer_reply,
            "live_vocal_operations_present": live.iter().filter(|tool|
                ["write_lyrics", "sing", "set_singer_voice", "list_voices", "inspect_singer", "edit_vocal_notes"]
                    .contains(&tool.name.as_str())
            ).map(|tool| &tool.name).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
