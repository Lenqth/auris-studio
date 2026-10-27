//! Exercise the actual saved tool dispatcher and live argument/command boundary.

use auris_session::{Session, live_agent::Command};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

enum Endpoint {
    Live(Box<Session>),
    Saved(PathBuf),
}

impl Endpoint {
    fn import(&mut self, mut args: Value) -> Result<Value, String> {
        let reply = match self {
            Self::Live(session) => {
                let command =
                    serde_json::from_value::<crate::live_agent::transcription_import::Args>(args)
                        .map_err(|e| e.to_string())?
                        .command(&Default::default())?;
                session.agent_command(command)
            }
            Self::Saved(path) => {
                args["project"] = path.to_string_lossy().as_ref().into();
                crate::runtime::call_saved(&Default::default(), "import_transcription", args)
            }
        }?;
        serde_json::from_str(&reply).map_err(|e| e.to_string())
    }

    fn snapshot(&self) -> Value {
        match self {
            Self::Live(session) => {
                json!({"project":session.project(),"undo":session.can_undo(),"redo":session.can_redo()})
            }
            Self::Saved(path) => {
                fn files(path: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
                    for entry in std::fs::read_dir(path).unwrap() {
                        let path = entry.unwrap().path();
                        if path.is_dir() {
                            files(&path, result);
                        } else {
                            result.insert(path.display().to_string(), std::fs::read(path).unwrap());
                        }
                    }
                }
                let mut content = BTreeMap::new();
                files(path.parent().unwrap(), &mut content);
                json!(content)
            }
        }
    }

    fn edit(&mut self) {
        match self {
            Self::Live(session) => {
                session.add_bus_track("Intervening edit");
            }
            Self::Saved(path) => {
                let mut session = crate::opened(path.to_str().unwrap()).unwrap();
                session.add_bus_track("Intervening edit");
                session.save(path).unwrap();
            }
        }
    }
}

fn draft() -> Value {
    json!({"schema_version":1,"source":{"audio_sha256":"a".repeat(64),"start_seconds":0.0,"end_seconds":2.0},"recognizer":{"name":"fixture"},"tracks":[{"name":"Piano","drums":false,"notes":[{"pitch":60,"start":0.0,"end":1.0,"strength":0.7}]}]})
}

#[test]
fn saved_and_live_imports_require_revision_and_exact_preview_without_history_side_effects() {
    for saved in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Song.auris");
        let mut session = crate::headless().unwrap();
        session.save(&path).unwrap();
        let mut endpoint = if saved {
            Endpoint::Saved(path)
        } else {
            Endpoint::Live(Box::new(session))
        };
        let args = json!({"multitrack":draft(),"name":"Draft","at_tick":0});
        let original = endpoint.snapshot();
        let mut preview_args = args.clone();
        preview_args["preview"] = true.into();
        let preview = endpoint.import(preview_args.clone()).unwrap();
        assert_eq!(endpoint.snapshot(), original);
        let mut apply = args.clone();
        apply["preview_digest"] = preview["preview_digest"].clone();
        assert!(
            endpoint
                .import(apply.clone())
                .unwrap_err()
                .contains("transcription_revision_required")
        );
        apply["expected_revision"] = preview["revision"].clone();
        let mut missing = apply.clone();
        missing.as_object_mut().unwrap().remove("preview_digest");
        assert!(
            endpoint
                .import(missing)
                .unwrap_err()
                .contains("transcription_preview_required")
        );
        for change in ["note", "sound", "at", "name"] {
            let mut changed = apply.clone();
            match change {
                "note" => changed["multitrack"]["tracks"][0]["notes"][0]["pitch"] = 61.into(),
                "sound" => changed["multitrack"]["tracks"][0]["sound_id"] = "another-sound".into(),
                "at" => changed["at_tick"] = 960.into(),
                _ => changed["name"] = "Changed".into(),
            }
            assert!(
                endpoint
                    .import(changed)
                    .unwrap_err()
                    .contains("transcription_preview_mismatch")
            );
            assert_eq!(endpoint.snapshot(), original);
        }
        let file = directory.path().join("draft.json");
        std::fs::write(&file, draft().to_string()).unwrap();
        let file_preview = endpoint
            .import(json!({"path":file,"preview":true}))
            .unwrap();
        let mut changed = draft();
        changed["tracks"][0]["notes"][0]["pitch"] = 62.into();
        std::fs::write(&file, changed.to_string()).unwrap();
        // Keep the input file outside the project snapshot when checking rejection side effects.
        let before_rejection = endpoint.snapshot();
        assert!(endpoint.import(json!({"path":file,"expected_revision":file_preview["revision"],"preview_digest":file_preview["preview_digest"]})).unwrap_err().contains("transcription_preview_mismatch"));
        assert_eq!(endpoint.snapshot(), before_rejection);
        endpoint.edit();
        let edited = endpoint.snapshot();
        assert!(
            endpoint
                .import(apply)
                .unwrap_err()
                .contains("transcription_revision_mismatch")
        );
        assert_eq!(endpoint.snapshot(), edited);
        let preview = endpoint.import(preview_args).unwrap();
        let mut accepted = args;
        accepted["expected_revision"] = preview["revision"].clone();
        accepted["preview_digest"] = preview["preview_digest"].clone();
        endpoint.import(accepted).unwrap();
        if let Endpoint::Live(session) = &mut endpoint {
            session.undo();
            assert_eq!(
                json!({"project":session.project(),"undo":session.can_undo(),"redo":false}),
                edited
            );
            session.redo();
            assert_eq!(session.project().tracks.len(), 2);
        } else if let Endpoint::Saved(path) = &endpoint {
            let reopened = crate::opened(path.to_str().unwrap()).unwrap();
            assert_eq!(reopened.project().tracks.len(), 2);
        }
    }
}

#[test]
fn direct_live_command_has_the_same_mandatory_multitrack_contract() {
    let mut session = crate::headless().unwrap();
    let command: Command = serde_json::from_value(json!({"action":"import_transcription","draft":{"kind":"multitrack","transcription":draft()},"name":"Draft","at_tick":0})).unwrap();
    assert!(
        session
            .agent_command(command)
            .unwrap_err()
            .contains("transcription_revision_required")
    );
    assert!(!session.can_undo());
}
