//! Bounded, lossless operation evidence and project-local script snapshots.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const PROMPT_BYTES: usize = 256 * 1024;
const SCRIPT_BYTES: usize = 64 * 1024;
const HISTORY_BYTES: usize = 48 * 1024;

pub(super) struct Evidence {
    pub prompt: String,
    scripts: Vec<(std::path::PathBuf, std::path::PathBuf, String)>,
}

impl Evidence {
    pub fn unchanged(&self) -> Result<(), String> {
        for (path, resolved, hash) in &self.scripts {
            if path.canonicalize().map_err(|e| e.to_string())? != *resolved {
                return Err(format!("Reviewed script path changed: {}", path.display()));
            }
            let (bytes, _) = read_script(path)?;
            if fingerprint(&bytes) != *hash {
                return Err(format!("Reviewed script changed: {}", path.display()));
            }
        }
        Ok(())
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_script(path: &Path) -> Result<(Vec<u8>, String), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Not a regular script file".into());
    }
    let mut bytes = Vec::new();
    file.take(SCRIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > SCRIPT_BYTES {
        return Err("Script exceeds the 64 KiB evidence limit".into());
    }
    let text = String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?;
    Ok((bytes, text))
}

pub(super) fn prepare(
    requests: Vec<String>,
    operation: String,
    arguments: Value,
    folder: Option<&Path>,
    history: Vec<Value>,
) -> Result<Evidence, String> {
    let mut recent = Vec::new();
    let mut bytes = 0;
    let mut omitted = 0;
    for item in history.into_iter().rev().take(8) {
        let size = item.to_string().len();
        if bytes + size <= HISTORY_BYTES {
            bytes += size;
            recent.push(item);
        } else {
            omitted += 1;
        }
    }
    recent.reverse();
    let mut evidence = Evidence {
        prompt: String::new(),
        scripts: Vec::new(),
    };
    let mut scripts = Vec::new();
    if matches!(operation.as_str(), "cli_command" | "cli_file_change")
        && let Some(folder) = folder
    {
        snapshot_scripts(folder, &mut scripts, &mut evidence.scripts);
    }
    let value = json!({"user_requests":requests,"operation":operation,"arguments":arguments,
        "project_folder":folder,"recent_completed_operations":recent,"omitted_recent_operations":omitted,
        "project_scripts":scripts,
        "script_scope":"Current regular UTF-8 scripts within two directory levels of the saved project; hidden and external paths excluded. Missing or oversized evidence is not approval. Snapshots do not prove future contents or authorize effects."});
    evidence.prompt = encode(&value);
    if evidence.prompt.len() > PROMPT_BYTES {
        return Err(
            "Complete operation evidence exceeds the 256 KiB automatic review limit".into(),
        );
    }
    Ok(evidence)
}

fn snapshot_scripts(
    folder: &Path,
    records: &mut Vec<Value>,
    hashes: &mut Vec<(std::path::PathBuf, std::path::PathBuf, String)>,
) {
    let Ok(root) = folder.canonicalize() else {
        return;
    };
    let mut pending = vec![(root.clone(), 0)];
    let mut visited = 0;
    let mut remaining = SCRIPT_BYTES * 2;
    while let Some((directory, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.take(257).filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            visited += 1;
            if visited > 256 || records.len() >= 16 {
                records.push(
                    json!({"complete":false,"error":"Project script inventory limit reached"}),
                );
                return;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            if !resolved.starts_with(&root) {
                continue;
            }
            if kind.is_dir() && depth < 2 {
                if !matches!(
                    entry.file_name().to_str(),
                    Some("Audio" | "node_modules" | "target" | "__pycache__")
                ) {
                    pending.push((resolved, depth + 1));
                }
            } else if kind.is_file()
                && matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("py" | "ps1" | "sh" | "js" | "mjs" | "cjs" | "bat" | "cmd")
                )
            {
                match read_script(&resolved) {
                    Ok((bytes, text)) if bytes.len() <= remaining => {
                        remaining -= bytes.len();
                        let hash = fingerprint(&bytes);
                        records.push(json!({"path":resolved,"sha256":hash,"complete":true,"content":text}));
                        // Retain the unresolved path too: replacement by a link must be noticed.
                        hashes.push((path, resolved, hash));
                    }
                    Ok(_) => records.push(json!({"path":path,"complete":false,"error":"Total script evidence budget exceeded"})),
                    Err(error) => records.push(json!({"path":path,"complete":false,"error":error})),
                }
            }
        }
    }
}

fn encode(value: &Value) -> String {
    let plain = value.to_string();
    if plain.len() <= 64 * 1024 {
        return plain;
    }
    let mut texts = Vec::new();
    let tree = shared_text(value, &mut texts);
    json!({"encoding":"shared_text_v1","texts":texts,"evidence":tree}).to_string()
}

fn shared_text(value: &Value, texts: &mut Vec<String>) -> Value {
    match value {
        Value::String(text) if text.len() >= 256 => {
            let index = texts.iter().position(|s| s == text).unwrap_or_else(|| {
                texts.push(text.clone());
                texts.len() - 1
            });
            json!({"text":index})
        }
        Value::Object(fields) => json!({"object": fields.iter().map(|(key, value)|
            (key.clone(), shared_text(value, texts))).collect::<serde_json::Map<_, _>>()}),
        Value::Array(items) => Value::Array(items.iter().map(|v| shared_text(v, texts)).collect()),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(value: &Value, texts: &Value) -> Value {
        match value {
            Value::Object(fields) if fields.contains_key("object") => Value::Object(
                fields["object"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), decode(v, texts)))
                    .collect(),
            ),
            Value::Object(fields) => texts[fields["text"].as_u64().unwrap() as usize].clone(),
            Value::Array(values) => Value::Array(values.iter().map(|v| decode(v, texts)).collect()),
            _ => value.clone(),
        }
    }

    #[test]
    fn shared_text_encoding_is_lossless_even_for_literal_reference_objects() {
        let text = "print('こんにちは \\\"')\n".repeat(1800);
        let original = json!({"command":text,"request":{"command":text},"item":{"command":text},"literal":{"text":0,"object":{"text":1}}});
        let encoded = encode(&original);
        assert!(encoded.len() < original.to_string().len() / 2);
        let packed: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decode(&packed["evidence"], &packed["texts"]), original);
    }

    #[test]
    fn scripts_are_complete_current_scoped_and_rechecked_after_review() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("analysis.py"), "print('current version')").unwrap();
        fs::write(root.path().join(".secret.py"), "secret").unwrap();
        fs::write(root.path().join("credentials.json"), "secret").unwrap();
        let history = vec![
            json!({"operation":"cli_command","arguments":{"command":"hash analysis.py"},"result":"previous hash"}),
        ];
        let evidence = prepare(
            vec!["Make a cover".into()],
            "cli_command".into(),
            json!({"command":"python analysis.py"}),
            Some(root.path()),
            history,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&evidence.prompt).unwrap();
        assert_eq!(value["project_scripts"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["project_scripts"][0]["content"],
            "print('current version')"
        );
        assert_eq!(
            value["recent_completed_operations"][0]["result"],
            "previous hash"
        );
        evidence.unchanged().unwrap();
        fs::write(root.path().join("analysis.py"), "print('changed')").unwrap();
        assert!(evidence.unchanged().is_err());
    }

    #[test]
    fn oversized_evidence_is_explicit_and_operations_are_never_truncated() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("huge.py"), vec![b'x'; SCRIPT_BYTES + 1]).unwrap();
        let evidence = prepare(
            vec![],
            "cli_command".into(),
            json!({"command":"python huge.py"}),
            Some(root.path()),
            vec![],
        )
        .unwrap();
        let value: Value = serde_json::from_str(&evidence.prompt).unwrap();
        assert_eq!(value["project_scripts"][0]["complete"], false);
        assert_eq!(value["arguments"]["command"], "python huge.py");
        assert!(
            prepare(
                vec![],
                "cli_command".into(),
                json!({"command":"x".repeat(PROMPT_BYTES)}),
                None,
                vec![]
            )
            .is_err()
        );
    }
}
