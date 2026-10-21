//! Shared machine-level search paths, edited without touching a project.
use super::*;

/// Wire name.
pub const NAME: &str = "configure_plugin_paths";
/// Model-facing contract.
pub const DESCRIPTION: &str = "Read or edit the shared extra CLAP/VST3 search paths used by MCP and desktop settings. Omit add/remove to list. Add absolute folders or plugin files; remove exact returned paths, up to 64 edits total. Saves machine settings only when changed; does not scan plugins or edit projects. Malformed settings are refused. After changing paths, run search_instruments with refresh:true and use the new sound IDs. A running desktop reloads these saved preferences on restart.";
/// Optional additions/removals; an empty request reads the current paths.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Absolute directories or plugin files to add; default empty.
    #[serde(default)]
    #[schemars(length(max = 64))]
    pub add: Vec<PathBuf>,
    /// Exact stored paths to remove; default empty.
    #[serde(default)]
    #[schemars(length(max = 64))]
    pub remove: Vec<PathBuf>,
}

/// Validates before saving, preserving every other preference.
pub fn run(args: &Args) -> Result<String, String> {
    let mut settings = Settings::load_for_update()?;
    let changed = settings.edit_plugin_paths(&args.add, &args.remove)?;
    if changed {
        settings.save().map_err(|error| error.to_string())?;
    }
    reports::publish(
        serde_json::json!({"paths":settings.plugin_paths,"changed":changed,
        "settings_file":Settings::path(),"refresh_required":changed,
        "next":changed.then_some("Search again with refresh:true; previous sound IDs belong to the old path configuration.")}),
    )
}
