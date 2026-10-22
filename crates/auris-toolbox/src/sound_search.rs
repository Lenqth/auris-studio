//! Saved-file bindings for the shared preset discovery and acoustic index.
use super::*;

pub(crate) fn configured_plugin_paths() -> Vec<PathBuf> {
    Settings::load().plugin_paths
}
