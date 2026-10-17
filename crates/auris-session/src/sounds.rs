//! Reusable instrument patches and insert chains, independent of a song's track identities.

use std::path::{Path, PathBuf};

use auris_core::PluginState;
use serde::{Deserialize, Serialize};

mod factory;

pub use factory::{SoundFamily, factory_sound_families, factory_sounds};

use crate::SessionError;
use crate::settings::{config_dir, read_config_text, write_config_bytes};

/// A reusable sound: an instrument and its ordered insert chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundPreset {
    /// User-visible name, unique within the user library.
    pub name: String,
    /// Stable registry or hosted instrument identifier.
    pub instrument_id: String,
    /// Complete instrument state, including opaque hosted-plugin data.
    pub state: PluginState,
    /// Absolute hosted-plugin path, if needed.
    pub file: Option<PathBuf>,
    /// External SoundFont reference, independent of project-local font identifiers.
    pub soundfont: Option<SoundPresetFont>,
    /// Inserts in signal order, without project-local slot or sidechain identities.
    pub effects: Vec<SoundPresetEffect>,
}

/// The sample library and patch a reusable sampler sound needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundPresetFont {
    /// Absolute path to the SoundFont.
    pub path: PathBuf,
    /// MIDI bank.
    pub bank: i32,
    /// MIDI program.
    pub patch: i32,
}

/// An insert saved with a reusable sound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundPresetEffect {
    /// Stable effect identifier.
    pub effect_id: String,
    /// Whether the insert processes audio.
    pub enabled: bool,
    /// Complete parameter and opaque state.
    pub state: PluginState,
    /// Absolute hosted-plugin path, if needed.
    pub file: Option<PathBuf>,
}

/// Installation-wide user sounds. Projects retain their own copies of applied settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SoundBook(Vec<SoundPreset>);

impl SoundBook {
    /// The user library file.
    pub fn path() -> PathBuf {
        config_dir().join("sounds.json")
    }

    /// Reads a library; a missing file is an empty library, other failures are reported.
    pub fn read(path: &Path) -> Result<Self, SessionError> {
        let text = match read_config_text(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(SessionError::SettingsWrite {
                    path: path.into(),
                    source,
                });
            }
        };
        serde_json::from_str(&text).map_err(|error| auris_io::IoError::from(error).into())
    }

    /// Saved sounds in name order.
    pub fn entries(&self) -> &[SoundPreset] {
        &self.0
    }

    /// Adds a named sound, refusing duplicates so existing work is never overwritten implicitly.
    pub fn insert(&mut self, mut sound: SoundPreset) -> Result<(), SessionError> {
        if std::iter::once(&sound.state)
            .chain(sound.effects.iter().map(|effect| &effect.state))
            .any(|state| state.params.values().any(|value| !value.is_finite()))
        {
            return Err(SessionError::SoundPreset(
                "sound parameters must be finite".into(),
            ));
        }
        sound.name = sound.name.trim().to_string();
        if sound.name.is_empty() || self.0.iter().any(|entry| entry.name == sound.name) {
            return Err(SessionError::SoundNameUnavailable);
        }
        self.0.push(sound);
        self.0.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    /// Reloads and adds a sound under a cross-process lock, then writes atomically.
    pub fn save_sound(path: &Path, sound: SoundPreset) -> Result<Self, SessionError> {
        Self::update(path, |book| book.insert(sound))
    }

    /// Replaces a saved sound's contents, retaining its name and refusing a stale selection.
    pub fn overwrite_sound(
        path: &Path,
        expected: &SoundPreset,
        mut replacement: SoundPreset,
    ) -> Result<Self, SessionError> {
        replacement.name.clone_from(&expected.name);
        Self::update(path, |book| {
            let index = book.expected_index(expected)?;
            book.0.remove(index);
            book.insert(replacement)
        })
    }

    /// Renames a saved sound, refusing duplicate names and a stale selection.
    pub fn rename_sound(
        path: &Path,
        expected: &SoundPreset,
        name: &str,
    ) -> Result<Self, SessionError> {
        let mut replacement = expected.clone();
        replacement.name = name.into();
        Self::update(path, |book| {
            let index = book.expected_index(expected)?;
            book.0.remove(index);
            book.insert(replacement)
        })
    }

    /// Copies a saved sound under a new name, refusing duplicates and a stale source.
    pub fn duplicate_sound(
        path: &Path,
        expected: &SoundPreset,
        name: &str,
    ) -> Result<Self, SessionError> {
        Self::update(path, |book| {
            let index = book.expected_index(expected)?;
            let mut copy = book.0[index].clone();
            copy.name = name.into();
            book.insert(copy)
        })
    }

    /// Deletes only the selected library entry, refusing a stale selection.
    pub fn delete_sound(path: &Path, expected: &SoundPreset) -> Result<Self, SessionError> {
        Self::update(path, |book| {
            let index = book.expected_index(expected)?;
            book.0.remove(index);
            Ok(())
        })
    }

    fn expected_index(&self, expected: &SoundPreset) -> Result<usize, SessionError> {
        self.0
            .iter()
            .position(|entry| entry == expected)
            .ok_or(SessionError::SoundLibraryChanged)
    }

    fn update(
        path: &Path,
        edit: impl FnOnce(&mut Self) -> Result<(), SessionError>,
    ) -> Result<Self, SessionError> {
        let failed = |source| SessionError::SettingsWrite {
            path: path.into(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .map_err(failed)?;
        lock.lock().map_err(failed)?;
        let mut book = Self::read(path)?;
        edit(&mut book)?;
        let bytes = serde_json::to_vec_pretty(&book).map_err(auris_io::IoError::from)?;
        write_config_bytes(path, &bytes).map_err(failed)?;
        Ok(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicating_a_sound_copies_all_settings_without_linking_the_entries() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let mut original = factory_sounds().remove(0);
        original.soundfont = Some(SoundPresetFont {
            path: scratch.path().join("library.sf2"),
            bank: 4,
            patch: 17,
        });
        original.state.extra = serde_json::json!({"settings": [1, 3, 5]});
        original.effects.push(SoundPresetEffect {
            effect_id: "hosted.effect".into(),
            enabled: false,
            state: PluginState {
                params: [("mix".into(), 0.25)].into(),
                extra: serde_json::json!({"hosted": "opaque state"}),
            },
            file: Some(scratch.path().join("effect.clap")),
        });
        SoundBook::save_sound(&path, original.clone()).unwrap();
        let book = SoundBook::duplicate_sound(&path, &original, "  Independent copy  ").unwrap();
        let mut copy = original.clone();
        copy.name = "Independent copy".into();
        assert_eq!(book.entries().len(), 2);
        assert!(book.entries().contains(&original));
        assert!(book.entries().contains(&copy));
        assert_eq!(SoundBook::read(&path).unwrap(), book);
        let book = SoundBook::overwrite_sound(&path, &copy, factory_sounds().remove(2)).unwrap();
        assert!(book.entries().contains(&original));
        assert!(!book.entries().contains(&copy));
    }

    #[test]
    fn saved_sound_management_round_trips_contents_names_and_order() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let mut sounds = factory_sounds();
        let original = sounds.remove(0);
        let other = sounds.remove(0);
        SoundBook::save_sound(&path, original.clone()).unwrap();
        SoundBook::save_sound(&path, other.clone()).unwrap();
        let mut replacement = sounds.remove(0);
        replacement.effects.push(SoundPresetEffect {
            effect_id: "auris.fx.reverb".into(),
            enabled: false,
            state: PluginState::default(),
            file: None,
        });
        let book = SoundBook::overwrite_sound(&path, &original, replacement.clone()).unwrap();
        replacement.name.clone_from(&original.name);
        assert!(book.entries().contains(&replacement));
        assert!(book.entries().contains(&other));
        let book = SoundBook::rename_sound(&path, &replacement, "  A new sound  ").unwrap();
        replacement.name = "A new sound".into();
        assert_eq!(book.entries(), &[replacement.clone(), other.clone()]);
        assert_eq!(SoundBook::read(&path).unwrap(), book);
        let book = SoundBook::delete_sound(&path, &replacement).unwrap();
        assert_eq!(book.entries(), &[other]);
        assert_eq!(SoundBook::read(&path).unwrap(), book);
    }

    #[test]
    fn invalid_and_stale_library_edits_preserve_the_file() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let sounds = factory_sounds();
        SoundBook::save_sound(&path, sounds[0].clone()).unwrap();
        SoundBook::save_sound(&path, sounds[1].clone()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        for name in ["  ", &sounds[1].name] {
            assert!(SoundBook::rename_sound(&path, &sounds[0], name).is_err());
            assert!(SoundBook::duplicate_sound(&path, &sounds[0], name).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let mut invalid = sounds[2].clone();
        invalid.state.params.insert("level".into(), f32::INFINITY);
        assert!(SoundBook::overwrite_sound(&path, &sounds[0], invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        SoundBook::overwrite_sound(&path, &sounds[0], sounds[2].clone()).unwrap();
        let changed = std::fs::read(&path).unwrap();
        assert!(SoundBook::overwrite_sound(&path, &sounds[0], sounds[1].clone()).is_err());
        assert!(SoundBook::rename_sound(&path, &sounds[0], "Stale").is_err());
        assert!(SoundBook::duplicate_sound(&path, &sounds[0], "Stale copy").is_err());
        assert!(SoundBook::delete_sound(&path, &sounds[0]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), changed);
        assert!(SoundBook::delete_sound(&path, &sounds[3]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), changed);
    }

    #[test]
    fn damaged_library_is_preserved_by_every_management_operation() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let bytes = b"damaged library";
        std::fs::write(&path, bytes).unwrap();
        let sounds = factory_sounds();
        assert!(SoundBook::overwrite_sound(&path, &sounds[0], sounds[1].clone()).is_err());
        assert!(SoundBook::rename_sound(&path, &sounds[0], "Renamed").is_err());
        assert!(SoundBook::duplicate_sound(&path, &sounds[0], "Copy").is_err());
        assert!(SoundBook::delete_sound(&path, &sounds[0]).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn concurrent_library_edits_preserve_unrelated_changes() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let sounds = factory_sounds();
        for sound in &sounds[..3] {
            SoundBook::save_sound(&path, sound.clone()).unwrap();
        }
        std::thread::scope(|scope| {
            let rename = scope.spawn(|| SoundBook::rename_sound(&path, &sounds[0], "Renamed"));
            let replace =
                scope.spawn(|| SoundBook::overwrite_sound(&path, &sounds[1], sounds[3].clone()));
            let delete = scope.spawn(|| SoundBook::delete_sound(&path, &sounds[2]));
            let insert = scope.spawn(|| SoundBook::save_sound(&path, sounds[4].clone()));
            rename.join().unwrap().unwrap();
            replace.join().unwrap().unwrap();
            delete.join().unwrap().unwrap();
            insert.join().unwrap().unwrap();
        });
        let mut renamed = sounds[0].clone();
        renamed.name = "Renamed".into();
        let mut replaced = sounds[3].clone();
        replaced.name.clone_from(&sounds[1].name);
        let book = SoundBook::read(&path).unwrap();
        assert_eq!(book.entries().len(), 3);
        for sound in [&renamed, &replaced, &sounds[4]] {
            assert!(book.entries().contains(sound));
        }
    }

    #[test]
    fn unreadable_library_is_preserved_when_adding_a_sound() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let bytes = b"a damaged but recoverable library";
        std::fs::write(&path, bytes).unwrap();
        assert!(SoundBook::save_sound(&path, factory_sounds().remove(0)).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn nonfinite_parameters_cannot_corrupt_the_saved_library() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let original = SoundBook::save_sound(&path, factory_sounds().remove(0)).unwrap();
        let mut invalid = factory_sounds().remove(1);
        invalid.state.params.insert("level".into(), f32::NAN);
        assert!(SoundBook::save_sound(&path, invalid).is_err());
        assert_eq!(SoundBook::read(&path).unwrap(), original);
    }

    #[test]
    fn concurrent_writers_preserve_both_sounds() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("sounds.json");
        let threads: Vec<_> = factory_sounds()
            .into_iter()
            .take(4)
            .map(|sound| {
                let path = path.clone();
                std::thread::spawn(move || SoundBook::save_sound(&path, sound).unwrap())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(SoundBook::read(&path).unwrap().entries().len(), 4);
    }
}
