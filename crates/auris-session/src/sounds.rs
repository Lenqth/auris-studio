//! Reusable instrument patches and insert chains, independent of a song's track identities.

use std::path::{Path, PathBuf};

use auris_core::PluginState;
use serde::{Deserialize, Serialize};

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
            return Err(SessionError::SoundPreset(
                "choose a new, nonempty sound name".into(),
            ));
        }
        self.0.push(sound);
        self.0.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    /// Reloads and adds a sound under a cross-process lock, then writes atomically.
    pub fn save_sound(path: &Path, sound: SoundPreset) -> Result<Self, SessionError> {
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
        book.insert(sound)?;
        let bytes = serde_json::to_vec_pretty(&book).map_err(auris_io::IoError::from)?;
        write_config_bytes(path, &bytes).map_err(failed)?;
        Ok(book)
    }
}

/// Editable starting patches for the bowed-string family, using the same format as user sounds.
pub fn factory_sounds() -> Vec<SoundPreset> {
    use auris_core::Parameterized;
    static DEFAULT: std::sync::OnceLock<PluginState> = std::sync::OnceLock::new();
    let defaults = DEFAULT.get_or_init(|| {
        let instrument = auris_synth::Physical::new(auris_synth::Model::Violin);
        let mut state = PluginState::empty();
        for param in instrument.parameters() {
            state.params.insert(param.key.to_string(), param.default);
        }
        state
    });
    [
        ("Violin", 0.65, 0.359_287, 0.336_820, 0.203_998, 0.12),
        ("Viola", 0.72, 0.42, 0.38, 0.24, 0.18),
        ("Cello", 0.85, 0.5, 0.42, 0.28, 0.3),
        ("Contrabass", 0.95, 0.62, 0.48, 0.32, 0.4),
    ]
    .into_iter()
    .map(|(name, body, damping, pressure, speed, release)| {
        let mut state = defaults.clone();
        if name != "Violin" {
            for (key, value) in [
                ("body", body),
                ("damping", damping),
                ("bow_pressure", pressure),
                ("bow_speed", speed),
                ("release", release),
            ] {
                state.params.insert(key.into(), value);
            }
        }
        SoundPreset {
            name: name.into(),
            instrument_id: auris_synth::Model::Violin.id().into(),
            state,
            file: None,
            soundfont: None,
            effects: Vec::new(),
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
