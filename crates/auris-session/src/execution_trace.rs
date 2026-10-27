//! Opt-in, bounded transition evidence. Credentials, prompts and file contents are never logged.

use crate::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

/// Explicit collection controls, separate from ordinary conversation memory.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TraceOperation {
    /// Enable collection after the host has obtained user consent.
    Start,
    /// Read bounded events after a monotonic cursor; persist them outside Auris as needed.
    Read {
        /// Last sequence already copied; omit to read the retained window.
        after: Option<u64>,
    },
    /// Disable collection and retain already recorded events until this session closes.
    Stop,
}

/// The exact invocation that requested authorization, or an explicitly independent operation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PermissionOrigin {
    /// A permission check belonging to an original tool invocation.
    ToolCall {
        /// Original invocation ID, unchanged by command normalization.
        call_id: String,
        /// Original tool name, unchanged by command normalization.
        tool: String,
    },
    /// An operation for which the transport supplied no tool invocation identity.
    Independent {
        /// Transport or operation that requested permission.
        source: String,
    },
}

#[derive(Default)]
pub(crate) struct Recorder {
    enabled: bool,
    agent_active: bool,
    sequence: u64,
    dropped: u64,
    events: VecDeque<Value>,
    pending: Option<(String, u64, String)>,
    calls: std::collections::BTreeMap<String, (String, u64, String)>,
    permissions: std::collections::BTreeMap<u64, (PermissionOrigin, String)>,
    completed: VecDeque<String>,
}

impl Recorder {
    fn completed(&mut self, id: String) {
        if self.completed.len() == 1024 {
            self.completed.pop_front();
        }
        self.completed.push_back(id);
    }
    fn push(&mut self, mut event: Value) {
        self.sequence += 1;
        event["sequence"] = self.sequence.into();
        event["timestamp_unix_ms"] = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64)
            .into();
        if self.events.len() == 1024 {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
    }
    fn flush(&mut self, hash: &str, revision: u64) {
        if let Some((before, from, label)) = self.pending.take()
            && (before != hash || from != revision)
        {
            self.push(json!({"kind":"document_transition","origin":"human_or_frontend","operation":label,
                    "before_state":before,"after_state":hash,"before_revision":from,"after_revision":revision,
                    "work_unit_boundary":true}));
        }
    }
}

impl Session {
    /// Close outstanding host calls at a cancellation boundary, retaining their original IDs.
    pub fn cancel_execution_calls(&self) {
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            for (id, (origin, operation)) in std::mem::take(&mut recorder.permissions) {
                recorder.push(json!({"kind":"permission_outcome","permission_id":id,"origin":origin,
                    "call_id":permission_call_id(&origin),"tool":permission_tool(&origin),"operation":operation,"outcome":"cancelled",
                    "state":self.agent_document_revision().ok(),"revision":self.revision()}));
            }
            for (id, (state, revision, tool)) in std::mem::take(&mut recorder.calls) {
                recorder.push(json!({"kind":"host_outcome","call_id":id,"tool":tool,"outcome":"cancelled","before_state":state,
                    "before_revision":revision,"state":self.agent_document_revision().ok(),"revision":self.revision()}));
                recorder.completed(id);
            }
        }
    }
    /// Record the host's exact call ID before permission or execution, without its arguments.
    pub fn record_execution_call(&self, call_id: &str, tool: &str) -> Result<(), String> {
        if call_id.is_empty() || call_id.len() > 256 || tool.is_empty() || tool.len() > 128 {
            return Err("Invalid bounded call identity".into());
        }
        self.trace_edit_boundary("external_edit");
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            let state = self.agent_document_revision()?;
            recorder.completed.retain(|id| id != call_id);
            if recorder.calls.len() >= 128 {
                return Err("Too many unresolved trace calls".into());
            }
            recorder
                .calls
                .entry(call_id.to_owned())
                .or_insert_with(|| (state.clone(), self.revision(), tool.to_owned()));
            recorder.push(json!({"kind":"host_call","call_id":call_id,"tool":tool,"before_state":state,"before_revision":self.revision()}));
        }
        Ok(())
    }

    /// Record a permission request separately from the original call's lifecycle.
    pub fn record_permission_request(
        &self,
        id: u64,
        origin: PermissionOrigin,
        operation: &str,
    ) -> Result<(), String> {
        let bounded = |value: &str| !value.is_empty() && value.len() <= 256;
        if !bounded(operation)
            || match &origin {
                PermissionOrigin::ToolCall { call_id, tool } => !bounded(call_id) || !bounded(tool),
                PermissionOrigin::Independent { source } => !bounded(source),
            }
        {
            return Err("Invalid bounded permission identity".into());
        }
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            if recorder.permissions.len() >= 128 || recorder.permissions.contains_key(&id) {
                return Err("Duplicate or excessive pending permission identities".into());
            }
            recorder
                .permissions
                .insert(id, (origin.clone(), operation.into()));
            recorder.push(json!({"kind":"permission_request","permission_id":id,"call_id":permission_call_id(&origin),
                "tool":permission_tool(&origin),"origin":origin,"operation":operation,"state":self.agent_document_revision()?,"revision":self.revision()}));
        }
        Ok(())
    }

    /// Finish one permission check without completing its associated tool invocation.
    pub fn record_permission_outcome(&self, id: u64, outcome: &str) -> Result<(), String> {
        if !matches!(outcome, "allowed" | "denied" | "cancelled") {
            return Err("Invalid permission outcome".into());
        }
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            let (origin, operation) = recorder.permissions.remove(&id).unwrap_or_else(|| {
                (
                    PermissionOrigin::Independent {
                        source: "unobserved_permission_request".into(),
                    },
                    "unknown".into(),
                )
            });
            recorder.push(json!({"kind":"permission_outcome","permission_id":id,"call_id":permission_call_id(&origin),
                "tool":permission_tool(&origin),"origin":origin,"operation":operation,"outcome":outcome,
                "state":self.agent_document_revision()?,"revision":self.revision()}));
        }
        Ok(())
    }

    pub(crate) fn trace_published_files(&self, files: &[Value]) {
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            let files: Vec<_> = files
                .iter()
                .map(|file| json!({"path":file["path"],"sha256":file["encoded"]["sha256"]}))
                .collect();
            recorder.push(json!({"kind":"published_files","files":files,"state":self.agent_document_revision().ok(),"revision":self.revision()}));
        }
    }
    /// Record a document boundary. Disabled recording performs no serialization or I/O.
    pub(crate) fn trace_edit_boundary(&self, label: &str) {
        let mut recorder = self.execution_trace.borrow_mut();
        if !recorder.enabled || recorder.agent_active {
            return;
        }
        if let Ok(hash) = self.agent_document_revision() {
            recorder.flush(&hash, self.revision());
            recorder.pending = Some((hash, self.revision(), label.to_owned()));
        }
    }

    /// Opt in/out or read transition evidence. The caller owns permission and durable storage.
    pub fn execution_trace(&self, operation: TraceOperation) -> Result<String, String> {
        self.trace_edit_boundary("external_edit");
        let mut recorder = self.execution_trace.borrow_mut();
        match operation {
            TraceOperation::Start=>{
                recorder.enabled=true;
                recorder.pending=Some((self.agent_document_revision()?,self.revision(),"external_edit".into()));
            },
            TraceOperation::Stop=>{recorder.enabled=false;recorder.calls.clear();recorder.permissions.clear();recorder.completed.clear();},
            TraceOperation::Read{after}=>return Ok(json!({"schema_version":2,"enabled":recorder.enabled,
                "events":recorder.events.iter().filter(|e|after.is_none_or(|s|e["sequence"].as_u64().is_some_and(|n|n>s))).collect::<Vec<_>>(),
                "last_sequence":recorder.sequence,"dropped_events":recorder.dropped,"document_instance":self.document_instance(),
                "content_policy":"no_arguments_results_prompts_credentials_or_file_contents"}).to_string()),
        }
        Ok(json!({"enabled":recorder.enabled,"schema_version":2}).to_string())
    }

    pub(crate) fn trace_call_begin(&self) -> Option<(String, u64)> {
        self.trace_edit_boundary("external_edit");
        let mut recorder = self.execution_trace.borrow_mut();
        if !recorder.enabled {
            return None;
        }
        let state = self.agent_document_revision().ok()?;
        recorder.pending = None;
        recorder.agent_active = true;
        Some((state, self.revision()))
    }

    pub(crate) fn trace_call_end(&self, before: Option<(String, u64)>, tool: &str, ok: bool) {
        let Some((state, revision)) = before else {
            return;
        };
        let mut recorder = self.execution_trace.borrow_mut();
        recorder.agent_active = false;
        if let Ok(hash) = self.agent_document_revision() {
            recorder.push(json!({"kind":"tool_call","origin":"agent","tool":tool,"outcome":if ok{"success"}else{"failure"},
                "permission":"host_authorized","before_state":state,"after_state":hash,"before_revision":revision,"after_revision":self.revision()}));
            recorder.pending = Some((hash, self.revision(), "external_edit".into()));
        }
    }

    /// Let a frontend record a terminal result using its exact call ID.
    /// No arguments, response bodies, credentials or untrusted error text are retained.
    pub fn record_execution_outcome(
        &self,
        call_id: &str,
        tool: &str,
        outcome: &str,
    ) -> Result<(), String> {
        if call_id.len() > 256
            || tool.len() > 128
            || !matches!(outcome, "cancelled" | "failed" | "published" | "success")
        {
            return Err("Invalid bounded execution outcome".into());
        }
        let mut recorder = self.execution_trace.borrow_mut();
        if recorder.enabled {
            if !recorder.calls.contains_key(call_id)
                && recorder.completed.iter().any(|id| id == call_id)
            {
                return Ok(());
            }
            let before = recorder.calls.remove(call_id);
            recorder.push(
                json!({"kind":"host_outcome","call_id":call_id,"tool":tool,"outcome":outcome,
                "before_state":before.as_ref().map(|(state,_,_)|state),"before_revision":before.map(|(_,revision,_)|revision),
                "state":self.agent_document_revision()?,"revision":self.revision()}),
            );
            recorder.completed(call_id.to_owned());
        }
        Ok(())
    }
}

fn permission_call_id(origin: &PermissionOrigin) -> Option<&str> {
    match origin {
        PermissionOrigin::ToolCall { call_id, .. } => Some(call_id),
        _ => None,
    }
}

fn permission_tool(origin: &PermissionOrigin) -> Option<&str> {
    match origin {
        PermissionOrigin::ToolCall { tool, .. } => Some(tool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionOptions, live_agent::Command};

    #[test]
    fn authorized_deterministic_payloads_replay_to_the_same_score_and_failures_do_not_edit() {
        let payloads = [
            json!({"action":"add_track","name":"Lead","kind":"instrument"}),
            json!({"action":"edit_harmony","start_bar":1,"bars":2,"key":"C major","chords":"| I | V |","clear_chords":false}),
            json!({"action":"generate_phrase","track":1,"part":"lead","start_bar":1,"bars":2,"seed":42}),
            json!({"action":"set_loop","start_bar":1,"bars":2}),
        ];
        let mut first = Session::new(SessionOptions::headless()).unwrap();
        first.execution_trace(TraceOperation::Start).unwrap();
        for (id, payload) in payloads.iter().enumerate() {
            first
                .record_execution_call(&format!("call-{id}"), payload["action"].as_str().unwrap())
                .unwrap();
            first
                .agent_command(serde_json::from_value(payload.clone()).unwrap())
                .unwrap();
            first
                .record_execution_outcome(
                    &format!("call-{id}"),
                    payload["action"].as_str().unwrap(),
                    "success",
                )
                .unwrap();
        }
        let before = first.project().clone();
        let invalid = serde_json::from_value(
            json!({"action":"generate_phrase","track":9999,"part":"lead","start_bar":1,"bars":1}),
        )
        .unwrap();
        assert!(first.agent_command(invalid).is_err());
        assert_eq!(first.project(), &before);
        let mut replay = Session::new(SessionOptions::headless()).unwrap();
        for payload in payloads {
            replay
                .agent_command(serde_json::from_value(payload).unwrap())
                .unwrap();
        }
        assert_eq!(first.project(), replay.project());
        let events: Value = serde_json::from_str(
            &first
                .execution_trace(TraceOperation::Read { after: None })
                .unwrap(),
        )
        .unwrap();
        assert!(
            events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["outcome"] == "failure")
        );
        assert!(
            events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["call_id"] == "call-1" && event["before_state"].is_string())
        );
    }

    #[test]
    fn tracing_requires_opt_in_and_separates_human_edits_failed_calls_and_agent_edits() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        session.add_bus_track("Private name before opt in");
        let read = |s: &Session| {
            serde_json::from_str::<Value>(
                &s.execution_trace(TraceOperation::Read { after: None })
                    .unwrap(),
            )
            .unwrap()
        };
        assert!(read(&session)["events"].as_array().unwrap().is_empty());
        session.execution_trace(TraceOperation::Start).unwrap();
        session.add_bus_track("secret-user-label");
        session.agent_command(Command::Inspect {}).unwrap();
        let invalid: Command =
            serde_json::from_value(json!({"action":"remove_track","track":99999})).unwrap();
        let _ = session.agent_command(invalid);
        session
            .record_permission_request(
                42,
                PermissionOrigin::ToolCall {
                    call_id: "call-42".into(),
                    tool: "render".into(),
                },
                "edit_project",
            )
            .unwrap();
        session.record_permission_outcome(42, "denied").unwrap();
        let events = read(&session);
        assert!(
            events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["origin"] == "human_or_frontend")
        );
        assert!(
            events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "tool_call")
        );
        assert!(
            events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["call_id"] == "call-42")
        );
        assert!(!events.to_string().contains("secret-user-label"));
        let sequence = events["last_sequence"].as_u64().unwrap();
        let empty: Value = serde_json::from_str(
            &session
                .execution_trace(TraceOperation::Read {
                    after: Some(sequence),
                })
                .unwrap(),
        )
        .unwrap();
        assert!(empty["events"].as_array().unwrap().is_empty());
    }

    #[test]
    fn cursor_reports_eviction_and_stopping_disables_collection() {
        let session = Session::new(SessionOptions::headless()).unwrap();
        session.execution_trace(TraceOperation::Start).unwrap();
        for id in 0..1030 {
            session
                .record_execution_outcome(&format!("call-{id}"), "render", "cancelled")
                .unwrap();
        }
        let read: Value = serde_json::from_str(
            &session
                .execution_trace(TraceOperation::Read { after: None })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read["events"].as_array().unwrap().len(), 1024);
        assert_eq!(read["dropped_events"], 6);
        session.execution_trace(TraceOperation::Stop).unwrap();
        session
            .record_execution_outcome("ignored", "render", "cancelled")
            .unwrap();
        let stopped: Value = serde_json::from_str(
            &session
                .execution_trace(TraceOperation::Read { after: Some(1030) })
                .unwrap(),
        )
        .unwrap();
        assert!(stopped["events"].as_array().unwrap().is_empty());
    }
}
