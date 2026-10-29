//! Immutable, portable copies of files explicitly attached by the user.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Copy authorized files into a conversation, preserving names and bounding disk use per send.
/// Call on a storage worker, never while restoring untrusted transcript metadata.
pub fn capture(root: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if paths.len() > 8 {
        return Err("Attach at most 8 files per message".into());
    }
    let directory = root.join("attachments");
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    paths
        .iter()
        .map(|path| {
            let name = path.file_name().ok_or("Attachment has no filename")?;
            let mut source =
                std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            if !source.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Attachments must be regular files".into());
            }
            let mut temporary =
                tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
            let mut hash = Sha256::new();
            let mut size = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                size += count as u64;
                total += count as u64;
                if size > 512 * 1024 * 1024 || total > 1024 * 1024 * 1024 {
                    return Err(
                        "Attachments exceed the 512 MiB per-file or 1 GiB combined limit".into(),
                    );
                }
                hash.update(&buffer[..count]);
                temporary
                    .write_all(&buffer[..count])
                    .map_err(|e| e.to_string())?;
            }
            let digest: String = hash
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let folder = directory.join(digest);
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let target = folder.join(name);
            match temporary.persist_noclobber(&target) {
                Ok(_) => {}
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
            Ok(target)
        })
        .collect()
}

/// Store only private archived paths relatively, leaving legacy display references untouched.
pub(crate) fn portable(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .ok()
        .filter(|p| archived(p))
        .map_or_else(
            || path.to_path_buf(),
            |p| PathBuf::from(p.to_string_lossy().replace('\\', "/")),
        )
}

fn archived(path: &Path) -> bool {
    let parts: Vec<_> = path.components().collect();
    matches!(parts.as_slice(), [Component::Normal(prefix), Component::Normal(hash), Component::Normal(_)]
        if *prefix == "attachments" && hash.to_str().is_some_and(|h| h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit())))
}

/// Resolve only the exact archive format; arbitrary relative transcript paths are not trusted.
pub(crate) fn resolve(root: &Path, path: &Path) -> PathBuf {
    if archived(path) {
        root.join(path)
    } else {
        path.to_path_buf()
    }
}

/// Whether a file is inside this archive, including after following filesystem links.
pub(crate) fn owns(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(archived)
        && root
            .join("attachments")
            .canonicalize()
            .ok()
            .is_some_and(|root| path.canonicalize().is_ok_and(|path| path.starts_with(root)))
}

/// Copy archived assets alongside a conversation during Save As.
pub(crate) fn copy_to(source: &Path, destination: &Path) -> Result<(), String> {
    let source = source.join("attachments");
    if !source.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
            let file = file.map_err(|e| e.to_string())?;
            if !file.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let relative = PathBuf::from("attachments")
                .join(entry.file_name())
                .join(file.file_name());
            if !archived(&relative) {
                continue;
            }
            let target = destination.join(relative);
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::copy(file.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_survives_source_changes_and_relocation() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("歌詞.txt");
        std::fs::write(&original, "First lyrics").unwrap();
        let root = temp.path().join("chat");
        let paths = capture(&root, std::slice::from_ref(&original)).unwrap();
        std::fs::write(&original, "New lyrics").unwrap();
        let changed = capture(&root, &[original]).unwrap();
        assert_ne!(paths, changed);
        let relative = portable(&root, &paths[0]);
        assert!(relative.is_relative());
        let destination = temp.path().join("moved");
        copy_to(&root, &destination).unwrap();
        assert_eq!(
            std::fs::read_to_string(resolve(&destination, &relative)).unwrap(),
            "First lyrics"
        );
        assert_eq!(
            resolve(&destination, Path::new("attachments/../../secret")),
            PathBuf::from("attachments/../../secret")
        );
    }
}
