//! Durable new-song conversations and evidence-based preference retrieval.
//! All methods that touch storage must run away from the UI thread.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One readable turn, kept independently from provider memory.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Archived user references, resolved for display and explicit reuse by the workshop.
    #[serde(default)]
    pub attachments: Vec<PathBuf>,
    /// Whether the user authored this message.
    pub user: bool,
    /// Original text, including authored lyric line breaks.
    pub text: String,
    /// Exact symbolic references captured on submission, independent of later file edits.
    #[serde(default)]
    pub references: Vec<Reference>,
}

/// One readable attachment and its bounded model-facing snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reference {
    /// Explicitly attached local file, retained for display and editor handoff.
    pub path: PathBuf,
    /// Decoded note, chord or instrument data; never an audio-listening claim.
    pub text: String,
}

/// One independent candidate assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Direction {
    /// Short, user-facing name.
    pub title: String,
    /// Audible distinction from the other alternatives.
    pub description: String,
    /// Complete instructions for the isolated composer.
    pub instructions: String,
    /// Exact parent track IDs whose note clips must remain unchanged.
    #[serde(default)]
    pub protected_tracks: Vec<u64>,
}

/// Persisted candidate identity; IDs also name private artifact directories.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    /// Stable local ID.
    pub id: u64,
    /// Exact source candidate, when continuing a selected song.
    pub parent: Option<u64>,
    /// Composition assignment.
    pub direction: Direction,
    /// True only after document, assets and audition have been saved.
    pub ready: bool,
    /// Failure retained across reopening.
    pub error: Option<String>,
    /// Automatic recoveries consumed since the last explicit recovery request.
    #[serde(default)]
    pub recoveries: u32,
}

/// A comparison is retained even after another round is generated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Round {
    /// Transcript position immediately after the director's response.
    pub after_message: usize,
    /// Alternatives in stable display order.
    pub candidates: Vec<Candidate>,
}

/// Provider-independent workshop state, separate from score undo history.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Document {
    /// Monotonic identity allocator.
    pub next_id: u64,
    /// Full readable conversation.
    pub messages: Vec<Message>,
    /// Comparison history and parent links.
    pub rounds: Vec<Round>,
    /// Current musical baseline.
    pub selected: Option<u64>,
    /// Unsent composer contents.
    pub draft: String,
    /// Unsent local references, restored beside the draft message.
    pub draft_attachments: Vec<PathBuf>,
}

/// A choice is evidence of a relative preference, never a dislike of every other song.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Conversation identity, preventing collisions between projects.
    pub conversation: String,
    /// Candidate identity within the conversation.
    pub candidate: u64,
    /// Stable comparison identity; different rounds may reuse the same titles.
    #[serde(default)]
    pub round: u64,
    /// User's request and context, not an inferred personality trait.
    pub context: String,
    /// All alternatives actually offered.
    pub alternatives: Vec<Direction>,
    /// Selected alternative's position.
    pub chosen: usize,
}

/// Local preference evidence. Explicit current instructions always take precedence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Whether to record and retrieve choices.
    pub enabled: bool,
    /// Bounded recent evidence, with original context.
    pub selections: Vec<Selection>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: true,
            selections: Vec::new(),
        }
    }
}

impl Preferences {
    /// Correct or remove an observation, rejecting stale edits from another window.
    /// Candidate identities and the offered alternatives remain unchanged. A correction
    /// becomes the newest evidence so it is included in the next bounded retrieval.
    pub fn revise_selection(
        &mut self,
        original: &Selection,
        correction: Option<(String, usize)>,
    ) -> Result<(), String> {
        let index = self
            .selections
            .iter()
            .position(|s| s == original)
            .ok_or_else(|| {
                "This preference changed. Refresh the history and try again.".to_owned()
            })?;
        if let Some((context, chosen)) = correction {
            if context.trim().is_empty() || chosen >= original.alternatives.len() {
                return Err("Enter a request and choose one of the offered alternatives.".into());
            }
            let mut revised = self.selections.remove(index);
            revised.context = context;
            revised.chosen = chosen;
            self.selections.push(revised);
        } else {
            self.selections.remove(index);
        }
        Ok(())
    }

    /// Record a choice once; selecting another alternative replaces that comparison's vote.
    pub fn record(&mut self, selection: Selection) {
        if !self.enabled {
            return;
        }
        self.selections.retain(|old| {
            !(old.conversation == selection.conversation && old.round == selection.round)
        });
        self.selections.push(selection);
        if self.selections.len() > 100 {
            self.selections.remove(0);
        }
    }

    /// Bounded attributed evidence for a director, rather than an ungrounded preference summary.
    pub fn context(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        serde_json::to_string(&self.selections.iter().rev().take(8).map(|s| serde_json::json!({
            "context":s.context, "chosen":s.chosen,
            "alternatives":s.alternatives.iter().map(|d| serde_json::json!({"title":d.title,"description":d.description})).collect::<Vec<_>>()
        })).collect::<Vec<_>>())
            .unwrap_or_default()
    }
}

/// Storage under the existing conversation root, so Save As copies it with conversations.
#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Hold one process-wide workshop owner until its worker exits.
    pub fn lease(&self) -> Result<std::fs::File, String> {
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("workshop.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock()
            .map_err(|_| "This song conversation is already open in another window".to_string())?;
        Ok(file)
    }

    /// Copy closed workshop history and exact artifacts during Save As.
    pub fn copy_to(&self, destination: &Self) -> Result<(), String> {
        if self.root == destination.root || !self.root.exists() {
            return Ok(());
        }
        let _lease = self.lease()?;
        if destination.root.exists() {
            if let (Ok(source), Ok(target)) = (
                std::fs::read(self.root.join("conversation.json")),
                std::fs::read(destination.root.join("conversation.json")),
            ) && source == target
            {
                return Ok(());
            }
            return Err("Destination contains a different workshop conversation".into());
        }
        let parent = destination.root.parent().ok_or("Missing workshop parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let staging = tempfile::tempdir_in(parent).map_err(|e| e.to_string())?;
        copy_tree(&self.root, staging.path())?;
        std::fs::rename(staging.path(), &destination.root).map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Use a host-resolved conversation directory, never a model-provided path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("workshop"),
        }
    }
    /// Stable identity for cross-project choice evidence.
    pub fn identity(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }
    /// Artifact path allocated by the host.
    pub fn candidate_path(&self, id: u64) -> PathBuf {
        self.root.join(id.to_string())
    }
    /// Load the full conversation or start a new one.
    pub fn load(&self) -> Result<Document, String> {
        let mut document: Document = read_or_default(&self.root.join("conversation.json"))?;
        for message in &mut document.messages {
            for path in &mut message.attachments {
                *path = crate::conversation_attachments::resolve(&self.root, path);
            }
            for reference in &mut message.references {
                reference.path =
                    crate::conversation_attachments::resolve(&self.root, &reference.path);
            }
        }
        Ok(document)
    }
    /// Atomically save a conversation.
    pub fn save(&self, document: &Document) -> Result<(), String> {
        let mut document = document.clone();
        for message in &mut document.messages {
            for path in &mut message.attachments {
                *path = crate::conversation_attachments::portable(&self.root, path);
            }
            for reference in &mut message.references {
                reference.path =
                    crate::conversation_attachments::portable(&self.root, &reference.path);
            }
        }
        write(&self.root.join("conversation.json"), &document)
    }
    /// Preserve explicitly attached files before passing them to the director.
    pub fn capture_attachments(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
        crate::conversation_attachments::capture(&self.root, paths)
    }
    /// Return only references owned by this workshop; restored metadata cannot grant file access.
    pub fn owned_attachments(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|path| crate::conversation_attachments::owns(&self.root, path))
            .cloned()
            .collect()
    }
}

/// Read local cross-project preference evidence.
pub fn load_preferences() -> Result<Preferences, String> {
    read_or_default(&crate::config_dir().join("song-preferences.json"))
}

/// Update preferences under a process-safe lock to avoid losing another window's choice.
pub fn update_preferences(update: impl FnOnce(&mut Preferences)) -> Result<Preferences, String> {
    try_update_preferences(|preferences| {
        update(preferences);
        Ok(())
    })
}

/// Atomically correct or remove an existing preference without overwriting concurrent changes.
pub fn revise_preference(
    original: &Selection,
    correction: Option<(String, usize)>,
) -> Result<Preferences, String> {
    try_update_preferences(|preferences| preferences.revise_selection(original, correction))
}

fn try_update_preferences(
    update: impl FnOnce(&mut Preferences) -> Result<(), String>,
) -> Result<Preferences, String> {
    let root = crate::config_dir();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("song-preferences.lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let mut preferences = load_preferences()?;
    update(&mut preferences)?;
    write(&root.join("song-preferences.json"), &preferences)?;
    Ok(preferences)
}

fn read_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.to_string()),
        Ok(meta) if meta.len() > 8 * 1024 * 1024 => Err("Workshop history is too large".into()),
        Ok(_) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string()),
    }
}

fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Workshop history is too large".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::settings::write_config_bytes(path, &bytes).map_err(|e| e.to_string())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if entry.file_name() == "workshop.lock" {
            continue;
        }
        if kind.is_symlink() {
            return Err("Workshop artifacts may not contain symbolic links".into());
        }
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &destination)?;
        } else {
            std::fs::copy(entry.path(), destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attached_files_follow_the_workshop_without_trusting_external_history_paths() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("lyrics.txt");
        std::fs::write(&source, "夜の雨").unwrap();
        let store = Store::new(folder.path().join("first"));
        let attachments = store
            .capture_attachments(std::slice::from_ref(&source))
            .unwrap();
        let mut document = Document::default();
        document.messages.push(Message {
            user: true,
            text: "Use these lyrics".into(),
            references: vec![Reference {
                path: attachments[0].clone(),
                text: "夜の雨".into(),
            }],
            attachments,
        });
        store.save(&document).unwrap();
        let destination = Store::new(folder.path().join("second"));
        store.copy_to(&destination).unwrap();
        let restored = destination.load().unwrap();
        let files = destination.owned_attachments(&restored.messages[0].attachments);
        assert_eq!(files.len(), 1);
        assert_eq!(restored.messages[0].references[0].path, files[0]);
        assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), "夜の雨");
        assert!(destination.owned_attachments(&[source]).is_empty());
        let legacy: Message =
            serde_json::from_str(r#"{"user":true,"text":"Older message"}"#).unwrap();
        assert!(legacy.attachments.is_empty());
    }
    fn selection(round: u64, candidate: u64) -> Selection {
        Selection {
            conversation: "test".into(),
            round,
            candidate,
            context: "A calm theme".into(),
            alternatives: vec![Direction {
                title: "Piano".into(),
                description: "Sparse".into(),
                instructions: "Private production instructions".into(),
                protected_tracks: vec![],
            }],
            chosen: 0,
        }
    }
    #[test]
    fn corrections_change_retrieved_evidence_and_preserve_provenance() {
        let mut original = selection(1, 1);
        let mut alternative = original.alternatives[0].clone();
        alternative.title = "Strings".into();
        original.alternatives.push(alternative);
        let mut preferences = Preferences::default();
        preferences.record(original.clone());
        preferences.record(selection(2, 2));
        preferences
            .revise_selection(&original, Some(("A warm theme\nWith strings".into(), 1)))
            .unwrap();
        let corrected = &preferences.selections[1];
        assert_eq!(corrected.conversation, original.conversation);
        assert_eq!(corrected.candidate, original.candidate);
        assert_eq!(corrected.round, original.round);
        assert_eq!(corrected.alternatives, original.alternatives);
        let evidence: serde_json::Value = serde_json::from_str(&preferences.context()).unwrap();
        assert_eq!(evidence[0]["chosen"], 1);
        assert_eq!(evidence[0]["context"], "A warm theme\nWith strings");
        assert_eq!(preferences.selections[0], selection(2, 2));
    }

    #[test]
    fn correcting_older_history_brings_it_into_the_next_retrieval() {
        let original = selection(1, 1);
        let mut preferences = Preferences::default();
        for id in 1..=12 {
            preferences.record(selection(id, id));
        }
        preferences
            .revise_selection(&original, Some(("Corrected older request".into(), 0)))
            .unwrap();
        let evidence: serde_json::Value = serde_json::from_str(&preferences.context()).unwrap();
        assert_eq!(evidence[0]["context"], "Corrected older request");
        assert_eq!(preferences.selections.len(), 12);
    }

    #[test]
    fn stale_corrections_cannot_restore_reset_or_replaced_choices() {
        let original = selection(1, 1);
        let mut preferences = Preferences::default();
        preferences.record(original.clone());
        preferences.record(selection(1, 2));
        assert!(preferences.revise_selection(&original, None).is_err());
        assert_eq!(preferences.selections[0].candidate, 2);
        preferences.selections.clear();
        assert!(
            preferences
                .revise_selection(&original, Some(("Stale".into(), 0)))
                .is_err()
        );
        assert!(preferences.selections.is_empty());
    }

    #[test]
    fn invalid_correction_preserves_history_and_disabled_history_remains_editable() {
        let original = selection(1, 1);
        let mut preferences = Preferences::default();
        preferences.record(original.clone());
        for correction in [(" ".into(), 0), ("Valid".into(), 99)] {
            assert!(
                preferences
                    .revise_selection(&original, Some(correction))
                    .is_err()
            );
            assert_eq!(preferences.selections[0], original);
        }
        preferences.enabled = false;
        preferences.revise_selection(&original, None).unwrap();
        assert!(preferences.selections.is_empty());
        assert!(!preferences.enabled);
    }
    #[test]
    fn reselecting_replaces_one_vote_but_same_titles_in_new_round_count() {
        let mut preferences = Preferences::default();
        preferences.record(selection(1, 1));
        preferences.record(selection(1, 2));
        preferences.record(selection(4, 4));
        assert_eq!(
            preferences
                .selections
                .iter()
                .map(|s| s.candidate)
                .collect::<Vec<_>>(),
            [2, 4]
        );
        assert!(
            !preferences
                .context()
                .contains("Private production instructions")
        );
    }
    #[test]
    fn save_as_copies_relative_artifacts_and_leaves_source_usable() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let original = Store::new(source.path().into());
        original
            .save(&Document {
                selected: Some(3),
                ..Default::default()
            })
            .unwrap();
        std::fs::create_dir_all(original.candidate_path(3)).unwrap();
        std::fs::write(original.candidate_path(3).join("asset.wav"), b"retained").unwrap();
        let target = Store::new(destination.path().into());
        original.copy_to(&target).unwrap();
        assert_eq!(target.load().unwrap().selected, Some(3));
        assert_eq!(
            std::fs::read(target.candidate_path(3).join("asset.wav")).unwrap(),
            b"retained"
        );
        assert!(original.candidate_path(3).join("asset.wav").exists());
    }
    #[test]
    fn unreadable_history_is_not_replaced_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        std::fs::create_dir_all(&store.root).unwrap();
        let path = store.root.join("conversation.json");
        std::fs::write(&path, b"broken").unwrap();
        assert!(store.load().is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"broken");
    }
    #[test]
    fn transcript_and_parent_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        let mut document = Document::default();
        document.messages.push(Message {
            references: vec![],
            attachments: Vec::new(),
            user: true,
            text: "歌詞\n二行目".into(),
        });
        document.selected = Some(7);
        store.save(&document).unwrap();
        let restored = store.load().unwrap();
        assert_eq!(restored.messages[0].text, "歌詞\n二行目");
        assert_eq!(restored.selected, Some(7));
    }
    #[test]
    fn disabled_memory_does_not_record_or_disclose_choices() {
        let mut preferences = Preferences {
            enabled: false,
            ..Default::default()
        };
        preferences.record(selection(1, 1));
        assert!(preferences.selections.is_empty());
        assert!(preferences.context().is_empty());
    }
}
