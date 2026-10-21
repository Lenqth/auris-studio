//! Saved-file adapters for bounded, atomic parameter and authored-note edits.
use super::*;

/// Sets static instrument controls across several tracks in one save.
pub mod set_instrument_params {
    use super::*;
    /// Wire name.
    pub const NAME: &str = "set_instrument_params";
    /// Model-facing contract.
    pub const DESCRIPTION: &str = "Set 1..256 instrument parameters across tracks atomically, in native units. Discover exact keys/ranges with automation read. All targets are validated before editing; duplicate targets are refused. Returns previous/current values and automation overrides. Saves once with one checkpoint; identical retries do not save or add history.";
    /// One instrument control in native units.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Edit {
        /// Exact track name or stable id:N selector.
        pub track: String,
        /// Exact key from automation read.
        pub param: String,
        /// Value in descriptor units, within range and on a valid discrete step.
        pub value: f32,
    }
    /// One atomic parameter request.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Saved project path or project_id.
        pub project: String,
        /// Up to 256 unique controls, possibly on different tracks.
        #[schemars(length(min = 1, max = 256))]
        pub edits: Vec<Edit>,
    }
    /// Applies one session command and saves only if the document changed.
    pub fn run(args: &Args) -> Result<String, String> {
        if !(1..=256).contains(&args.edits.len()) {
            return Err("edits must contain 1..256 instrument parameters".into());
        }
        let mut session = opened(&args.project)?;
        let edits = args
            .edits
            .iter()
            .map(|edit| {
                Ok(auris_session::InstrumentParamEdit {
                    track: track_by_name(session.project(), &edit.track)?.id,
                    param: edit.param.clone(),
                    value: edit.value,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let changes = session.set_instrument_params(&edits)?;
        let changed = changes.iter().any(|change| change.previous != change.value);
        if changed {
            save_checkpointed(&mut session)?;
        }
        let changes = changes.into_iter().map(|change|serde_json::json!({
            "track":format!("id:{}",change.track.0), "param":change.param, "name":change.name,
            "previous":change.previous,"value":change.value,"unit":change.unit,"automated":change.automated,
            "warning":change.automated.then_some("Automation overrides this static value during playback.")
        })).collect::<Vec<_>>();
        reports::publish(serde_json::json!({"changed":changed,"changes":changes}))
    }
}

/// Replaces authored notes in several clips in one save.
pub mod replace_notes_batch {
    use super::*;
    /// Wire name.
    pub const NAME: &str = "replace_notes_batch";
    /// Model-facing contract.
    pub const DESCRIPTION: &str = "Replace authored notes in 1..16 clips atomically, at most 65536 notes total. Each edit supplies track, 1-based clip, and exactly one of notes (up to 256 inline) or source (absolute UTF-8 JSON array path, at most 16 MiB). Uses replace_notes pitch/bar/beat/duration units; [] clears a clip. Preserves recipes, curves, transforms and lengths. All input is validated before editing. Saves once with one checkpoint; identical retries do not save or add history.";
    /// One complete clip replacement.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Replacement {
        /// Exact track name or stable id:N selector.
        pub track: String,
        /// One-based clip number from describe.
        #[schemars(range(min = 1))]
        pub clip: usize,
        /// Complete authored note array; omit when using source.
        #[serde(
            default,
            deserialize_with = "replace_notes::deserialize_optional_notes"
        )]
        #[schemars(length(max = 256))]
        pub notes: Option<Vec<edit_notes::NoteSpec>>,
        /// Absolute server path to a UTF-8 JSON array; omit when using notes.
        pub source: Option<String>,
    }
    /// Several clips in one saved project.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Saved project path or project_id.
        pub project: String,
        /// Unique target clips; each includes its full replacement score.
        #[schemars(length(min = 1, max = 16))]
        pub edits: Vec<Replacement>,
    }
    /// Prepares every score before invoking the transactional session command.
    pub fn run(args: &Args) -> Result<String, String> {
        if !(1..=16).contains(&args.edits.len()) {
            return Err("edits must contain 1..16 clips".into());
        }
        let mut session = opened(&args.project)?;
        let mut replacements = Vec::new();
        let mut reports = Vec::new();
        let mut total = 0;
        for (index, edit) in args.edits.iter().enumerate() {
            let loaded;
            let notes = match (&edit.notes, &edit.source) {
                (Some(notes), None) if notes.len() <= 256 => notes.as_slice(),
                (None, Some(source)) => {
                    loaded = replace_notes::read_notes(source)?;
                    &loaded
                }
                _ => {
                    return Err(format!(
                        "edits[{index}]: supply exactly one of notes (up to 256) or source"
                    ));
                }
            };
            total += notes.len();
            if total > 65_536 {
                return Err("batch exceeds 65536 total notes".into());
            }
            let track = track_by_name(session.project(), &edit.track)?.id;
            let (id, clip) = clip_by_number(session.project(), track, edit.clip)?;
            let placed = edit_notes::prepare(&session, clip, notes)
                .map_err(|error| format!("edits[{index}]: {error}"))?;
            reports.push(serde_json::json!({"track":format!("id:{}",track.0),"clip":edit.clip,"clip_id":id.0,"notes":placed.len()}));
            replacements.push((id, placed));
        }
        let changed = session.replace_notes_batch(replacements)?;
        if changed > 0 {
            save_checkpointed(&mut session)?;
        }
        reports::publish(serde_json::json!({"changed_clips":changed,"clips":reports}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn parameter_batch_persists_eighteen_controls_with_one_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let mut session = headless().unwrap();
        let mut edits = Vec::new();
        for index in 0..18 {
            let name = format!("Lead {index}");
            let track = session.add_default_instrument_track(&name).unwrap();
            let descriptors = session.instrument_descriptors(track);
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.min < descriptor.max)
                .unwrap();
            let target = ParamTarget::Instrument {
                track,
                param: descriptor.id,
            };
            let value = if session.param_value(target, descriptor) != descriptor.min {
                descriptor.min
            } else {
                descriptor.max
            };
            edits.push(set_instrument_params::Edit {
                track: name,
                param: descriptor.key.to_string(),
                value,
            });
        }
        let path = root.path().join("Params.auris");
        session.save(&path).unwrap();
        let mut args = set_instrument_params::Args {
            project: path.to_string_lossy().into_owned(),
            edits,
        };
        let key = std::mem::replace(&mut args.edits[17].param, "missing".repeat(10_000));
        let before = std::fs::read(&path).unwrap();
        let error = set_instrument_params::run(&args).unwrap_err();
        assert!(error.len() < 256);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        args.edits[17].param = key;
        let report: Value =
            serde_json::from_str(&set_instrument_params::run(&args).unwrap()).unwrap();
        assert_eq!(report["changes"].as_array().unwrap().len(), 18);
        let mut reopened = opened(path.to_str().unwrap()).unwrap();
        assert_eq!(reopened.checkpoints().unwrap().len(), 1);
        for edit in &args.edits {
            let track = track_by_name(reopened.project(), &edit.track).unwrap().id;
            let descriptors = reopened.instrument_descriptors(track);
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.key == edit.param)
                .unwrap();
            assert_eq!(
                reopened.param_value(
                    ParamTarget::Instrument {
                        track,
                        param: descriptor.id
                    },
                    descriptor
                ),
                edit.value
            );
        }
        let bytes = std::fs::read(&path).unwrap();
        let report: Value =
            serde_json::from_str(&set_instrument_params::run(&args).unwrap()).unwrap();
        assert_eq!(report["changed"], false);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            opened(path.to_str().unwrap())
                .unwrap()
                .checkpoints()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn note_batch_saves_once_and_invalid_or_repeated_requests_do_not_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let mut session = headless().unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        for _ in 0..12 {
            session
                .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks::QUARTER)
                .unwrap();
        }
        let path = root.path().join("Batch.auris");
        session.save(&path).unwrap();
        let edits = (1..=12).map(|clip|json!({"track":"Lead","clip":clip,"notes":[{"pitch":60,"bar":1,"beat":1,"beats":1}]})).collect::<Vec<_>>();
        let args: replace_notes_batch::Args =
            serde_json::from_value(json!({"project":path,"edits":edits})).unwrap();
        let response: Value =
            serde_json::from_str(&replace_notes_batch::run(&args).unwrap()).unwrap();
        assert_eq!(response["changed_clips"], 12);
        let saved = opened(path.to_str().unwrap()).unwrap();
        let checkpoints = saved.checkpoints().unwrap().len();
        assert_eq!(checkpoints, 1);
        assert!(
            saved.project().tracks[0]
                .kind
                .as_instrument()
                .unwrap()
                .clips
                .iter()
                .all(|clip| clip.notes.len() == 1)
        );
        let bytes = std::fs::read(&path).unwrap();
        replace_notes_batch::run(&args).unwrap();
        let mut invalid = args;
        invalid.edits[11].clip = 999;
        assert!(replace_notes_batch::run(&invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            opened(path.to_str().unwrap())
                .unwrap()
                .checkpoints()
                .unwrap()
                .len(),
            checkpoints
        );
    }
}
