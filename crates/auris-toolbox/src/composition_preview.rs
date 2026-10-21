//! Read-only composition layout at the saved-file model boundary.
use super::*;

/// Wire name.
pub const NAME: &str = "preview_composition";
/// Model-facing contract.
pub const DESCRIPTION: &str = "Preview a specification's actual bar count, added ending bars, part-to-track mapping, merged drum writers, vocal tracks and buses before compose. Uses the same score generation and lyric preparation as compose, but does not load instruments, render audio or write a project. Source availability is checked by compose. Read check_spec first for syntax validation.";
/// The same spec/preset/overrides fields as check_spec and compose.
pub use crate::SpecArgs as Args;
/// Generates an immutable layout report without changing any project.
pub fn run(args: &Args) -> Result<String, String> {
    let spec = resolve_spec(args)?;
    let composition = auris_session::prelude::compose(&spec);
    let report = headless()?
        .composition_preview(&composition)
        .map_err(|error| error.to_string())?;
    reports::publish(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_tool_reports_the_extra_bar_before_any_file_or_plugin_is_opened() {
        let answer = run(&Args {spec:Some("form='verse'\n[section.verse]\nbars=2\n[[part]]\nname='lead'\nsource={type='clap',path='Missing.clap',plugin_id='lead'}".into()),preset:None,overrides:None}).unwrap();
        let report: serde_json::Value = serde_json::from_str(&answer).unwrap();
        assert_eq!(report["total_bars"], 3);
        assert_eq!(report["ending_bars"], 1);
        assert_eq!(report["tracks"][0]["parts"][0], "lead");
        assert_eq!(report["source_availability_checked"], false);
    }
}
