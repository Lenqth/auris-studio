//! Bounded score input for the shared note editor.

use auris_session::live_agent::{Command, note_editing::NoteEdit};
use std::io::Read;

/// Exact score edits, inline or in a local JSON array file.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Content revision returned by read_notes or inspect_project.
    pub expected_revision: String,
    /// Up to 16 clip edits and 4096 inline note changes; omit when using source.
    pub edits: Option<Vec<NoteEdit>>,
    /// Absolute local JSON file containing an array of clip edits, at most 16 MiB and 65536 notes. Omit when using edits.
    pub source: Option<String>,
}

impl Args {
    /// Load and validate the bounded input before requesting a document edit.
    pub fn command(self) -> Result<Command, String> {
        let edits = match (self.edits, self.source) {
            (Some(edits), None) => {
                if note_count(&edits) > 4096 {
                    return Err(
                        "At most 4096 inline note changes; use source for larger scores".into(),
                    );
                }
                edits
            }
            (None, Some(source)) => {
                let path = std::path::Path::new(&source);
                if !path.is_absolute() || source.starts_with("\\\\") || source.starts_with("//") {
                    return Err("source must be an absolute local file path".into());
                }
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("source must be a regular file".into());
                }
                let mut bytes = Vec::new();
                file.take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err("source exceeds 16 MiB".into());
                }
                let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
                serde_json::from_slice(bytes).map_err(|e| format!("source JSON: {e}"))?
            }
            _ => return Err("Supply exactly one of edits or source".into()),
        };
        if edits.is_empty() || edits.len() > 16 || note_count(&edits) > 65_536 {
            return Err("Supply 1..16 clip edits and at most 65536 note changes".into());
        }
        Ok(Command::EditNotes {
            expected_revision: self.expected_revision,
            edits,
        })
    }
}

fn note_count(edits: &[NoteEdit]) -> usize {
    edits
        .iter()
        .map(|edit| match edit {
            NoteEdit::Replace { notes, .. } => notes.len(),
            NoteEdit::Patch {
                add,
                remove,
                update,
                ..
            } => add.len() + remove.len() + update.len(),
        })
        .sum()
}
