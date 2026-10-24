//! Runtime persistence and trusted tool observations for a conversation's production record.
use super::*;
use auris_session::production_record::{Operation, Record};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct State {
    history: Option<HistoryGeneration>,
    record: Record,
    document: Value,
}

impl Bridge {
    pub(crate) fn bind_production_history(
        &self,
        history: Option<&HistoryGeneration>,
    ) -> Result<(), String> {
        self.0
            .production
            .lock()
            .map_err(|_| "Production record lock failed")?
            .history = history.cloned();
        Ok(())
    }

    fn with_production<T>(
        &self,
        operation: impl FnOnce(&mut State) -> Result<T, String>,
        write: bool,
    ) -> Result<T, String> {
        self.history(|| {
            let mut state = self
                .0
                .production
                .lock()
                .map_err(|_| "Production record lock failed")?;
            let binding = state.history.clone();
            let _file_lock = binding
                .as_ref()
                .map(|h| history_file_lock(&h.path))
                .transpose()?;
            if let Some(history) = &binding {
                if read_history_generation(&history.path)? != history.generation {
                    return Err("Conversation was reset; production record is stale".into());
                }
                let path = history.path.with_extension("production.json");
                match std::fs::File::open(path) {
                    Ok(file) => {
                        let mut bytes = Vec::new();
                        file.take(1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        if bytes.len() > 1024 * 1024 {
                            return Err("Production record exceeds 1 MiB".into());
                        }
                        state.record = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        state.record = Record::default()
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            let result = operation(&mut state)?;
            if write && let Some(history) = binding {
                let bytes = serde_json::to_vec(&state.record).map_err(|e| e.to_string())?;
                auris_session::settings::write_config_bytes(
                    &history.path.with_extension("production.json"),
                    &bytes,
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(result)
        })
    }

    pub(crate) fn production_record(&self, operation: Operation) -> Result<Value, String> {
        let write = !matches!(operation, Operation::Read);
        self.with_production(
            |state| {
                if !state.document["revision"].is_string() {
                    return Err(
                        "Call inspect_project before reading or updating production_record".into(),
                    );
                }
                state.record.update(operation)?;
                let mut view = state.record.view(&state.document);
                view["persistence"] = json!(if state.history.is_some() {
                    "conversation"
                } else {
                    "worker_lifetime"
                });
                Ok(view)
            },
            write,
        )
    }

    pub(crate) fn inspected_production_document(&self, document: &mut Value) -> Result<(), String> {
        let view = self.with_production(
            |state| {
                state.document = document.clone();
                Ok(state.record.view(&state.document))
            },
            false,
        )?;
        document["production_record"] = view;
        Ok(())
    }

    pub(crate) fn observe_production(
        &self,
        tool: &str,
        revision: u64,
        range: Option<(u64, u64)>,
        track: Option<u64>,
        result: Value,
    ) -> Result<(), String> {
        self.with_production(
            |state| {
                state
                    .record
                    .observe(&state.document, tool, revision, range, track, result);
                Ok(())
            },
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_resume_and_reset_rejects_a_late_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.json");
        let (_, generation) = HistoryGeneration::load(&path, false).unwrap();
        let (bridge, _, _) = test_bridge();
        bridge.bind_production_history(Some(&generation)).unwrap();
        bridge
            .inspected_production_document(&mut json!({"revision":"one"}))
            .unwrap();
        bridge.production_record(serde_json::from_value(json!({"kind":"put","entry":{
            "key":"motif","category":"motif","source":"agent_assumption","basis":"opening phrase","text":"keep the rising third"}})).unwrap()).unwrap();
        let (resumed, _, _) = test_bridge();
        resumed.bind_production_history(Some(&generation)).unwrap();
        resumed
            .inspected_production_document(&mut json!({"revision":"two"}))
            .unwrap();
        assert_eq!(
            resumed.production_record(Operation::Read).unwrap()["entries"][0]["entry"]["text"],
            "keep the rising third"
        );
        clear_history(&path).unwrap();
        assert!(
            bridge
                .production_record(Operation::Remove {
                    key: "motif".into()
                })
                .is_err()
        );
        assert!(!path.with_extension("production.json").exists());
    }
}
