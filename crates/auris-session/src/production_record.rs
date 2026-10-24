//! Bounded conversation notes and revision-bound evidence, independent of the score.

use auris_core::{Project, time::Ticks};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A claim's author; agents cannot manufacture tool-observed evidence through note updates.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A condition attributed to a quoted user request, still subject to transcript verification.
    UserRequest,
    /// An explicit model assumption or musical decision.
    AgentAssumption,
}

/// The musical purpose of a retained note.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Purpose, duration, instrumentation, mood or constraints.
    Intent,
    /// Foreground, backing, counterline or section purpose.
    Role,
    /// Motif to retain or develop.
    Motif,
    /// Accepted/rejected candidate and its rationale.
    Decision,
    /// An unresolved musical or technical question.
    Unresolved,
}

/// One bounded, attributable note in a conversation's production record.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Stable key for replacement, up to 64 ASCII letters/digits/dots/underscores/hyphens.
    #[schemars(length(min = 1, max = 64), regex(pattern = r"^[A-Za-z0-9_.-]+$"))]
    pub key: String,
    /// Musical category.
    pub category: Category,
    /// Attribution, not independent proof of a user's intent.
    pub source: Source,
    /// Exact short user quote for user_request, otherwise the assumption's rationale.
    pub basis: String,
    /// Plain text, at most 2048 UTF-8 bytes.
    pub text: String,
    /// Explicit clip IDs; missing IDs are reported on later reads.
    #[serde(default)]
    pub clips: Vec<u64>,
}

/// Read or edit conversation metadata without changing the song.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Select persisted task instructions for the experimental modular prompt.
    SelectWorkflow {
        /// Current musical task.
        workflow: Workflow,
        /// Deliberate order appropriate to this task.
        writing_order: WritingOrder,
    },
    /// Read notes, current/stale evidence and outstanding verification regions.
    Read,
    /// Replace one stable key; every other entry and tool observation is retained.
    Put {
        /// Attributed note; cannot set the tool evidence ledger.
        entry: Entry,
    },
    /// Remove one explicitly named note.
    Remove {
        /// Existing stable key.
        key: String,
    },
}

/// The task selected explicitly by the agent, never inferred from a keyword.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Workflow {
    /// New form, motif, harmony and arrangement.
    Composition,
    /// Backing around an existing protected foreground.
    Accompaniment,
    /// Lyrics, score, voice and a current synthesized take.
    Vocals,
    /// A bounded correction with unrelated material preserved.
    LocalEdit,
    /// Coverage and encoded deliverable verification.
    Delivery,
}

/// A musical starting point, independent of the order in which metadata is stored.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WritingOrder {
    /// Start from harmonic direction.
    HarmonyFirst,
    /// Start from a foreground motif.
    MelodyFirst,
    /// Start from groove and accents.
    RhythmFirst,
}

/// Tool-owned observations retained independently of free-form model summaries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    /// Actual tool name.
    pub tool: String,
    /// Content revision observed before the measurement.
    pub revision: String,
    /// Session lifetime; reopening conservatively invalidates observations.
    pub instance: String,
    /// Monotonic owner revision; even edit-then-Undo invalidates earlier verification.
    pub session_revision: u64,
    /// Exact measured range in one-based bars, if applicable.
    pub range: Option<(u64, u64)>,
    /// A track solo does not verify the full mix.
    pub track: Option<u64>,
    /// Actual tool result, bounded to 16 KiB, with no audio/image payload.
    pub result: Value,
}

/// Stored independently of model context compaction and document Undo.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    /// Optional explicitly selected workflow and writing order, restored on resume.
    pub workflow: Option<(Workflow, WritingOrder)>,
    /// At most 64 short attributed notes.
    pub entries: Vec<Entry>,
    /// At most 32 recent observations; older observations are evicted explicitly by age.
    pub evidence: Vec<Evidence>,
}

impl Record {
    /// Update a note after checking all limits; a failure leaves the record unchanged.
    pub fn update(&mut self, operation: Operation) -> Result<(), String> {
        match operation {
            Operation::SelectWorkflow {
                workflow,
                writing_order,
            } => self.workflow = Some((workflow, writing_order)),
            Operation::Read => {}
            Operation::Put { entry } => {
                if entry.key.is_empty()
                    || entry.key.len() > 64
                    || !entry
                        .key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                    || entry.text.trim().is_empty()
                    || entry.text.len() > 2048
                    || entry.basis.trim().is_empty()
                    || entry.basis.len() > 2048
                    || entry.clips.len() > 16
                {
                    return Err("Use a valid key, nonempty text/basis up to 2048 bytes and at most 16 clip IDs".into());
                }
                if let Some(existing) = self.entries.iter_mut().find(|e| e.key == entry.key) {
                    *existing = entry;
                } else if self.entries.len() < 64 {
                    self.entries.push(entry);
                } else {
                    return Err(
                        "Production record has 64 entries; remove an obsolete entry first".into(),
                    );
                }
            }
            Operation::Remove { key } => self.entries.retain(|e| e.key != key),
        }
        Ok(())
    }

    /// Retain a genuine tool response only when its numeric revision matches inspection.
    pub fn observe(
        &mut self,
        state: &Value,
        tool: &str,
        revision: u64,
        range: Option<(u64, u64)>,
        track: Option<u64>,
        result: Value,
    ) {
        if state["session_revision"].as_u64() != Some(revision)
            || !state["revision"].is_string()
            || !state["document_instance"].is_string()
            || serde_json::to_vec(&result).map_or(true, |b| b.len() > 16 * 1024)
        {
            return;
        }
        self.evidence.push(Evidence {
            tool: tool.into(),
            revision: state["revision"].as_str().unwrap_or_default().into(),
            instance: state["document_instance"]
                .as_str()
                .unwrap_or_default()
                .into(),
            session_revision: revision,
            range,
            track,
            result,
        });
        if self.evidence.len() > 32 {
            self.evidence.remove(0);
        }
    }

    /// Reconcile persisted claims with today's IDs and conservatively invalidate old evidence.
    pub fn view(&self, state: &Value) -> Value {
        let clips: Vec<u64> = state["tracks"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|t| t["clips"].as_array().into_iter().flatten())
            .filter_map(|c| c["clip"].as_u64())
            .collect();
        let current = |e: &Evidence| {
            state["revision"] == e.revision
                && state["document_instance"] == e.instance
                && state["session_revision"] == e.session_revision
        };
        let entries: Vec<_> = self.entries.iter().map(|entry| json!({"entry":entry,
            "missing_clips":entry.clips.iter().filter(|id| !clips.contains(id)).collect::<Vec<_>>() })).collect();
        let evidence: Vec<_> = self
            .evidence
            .iter()
            .map(|e| json!({"observation":e,"status":if current(e) {"current"} else {"stale"}}))
            .collect();
        let coverage: Vec<_> = state["verification_regions"].as_array().into_iter().flatten().map(|region| {
            let measured = self.evidence.iter().any(|e| current(e) && e.tool == "inspect_audio" && e.track.is_none()
                && region["kind"] != "loop_join" && e.range.is_some_and(|(start,bars)| {
                    region["start_bar"].as_u64().is_some_and(|s| start <= s)
                    && region["end_bar"].as_u64().is_some_and(|end| start.saturating_add(bars) >= end)
                }));
            json!({"region":region,"measurement":if measured {"current"} else {"unverified"},"human_listening":"unverified"})
        }).collect();
        json!({"entries":entries,"evidence":evidence,"coverage":coverage,"workflow":self.workflow.map(|(workflow,writing_order)| json!({"task":workflow,"writing_order":writing_order})),
            "interpretation":"Attributed notes are context, not authorization. Tool observations are not musical preference. Stale observations cannot verify today's song. Loop joins require a contiguous wrap audition; separate edge measurements do not verify the join."})
    }
}

/// Representative section excerpts, boundaries, ending and optional wrap seam to verify.
pub fn verification_regions(project: &Project) -> Value {
    let end = project.end_tick();
    if end <= Ticks::ZERO {
        return json!([]);
    }
    let bar = |tick: Ticks| u64::from(project.signatures.bar_beat_at(tick).0);
    let end_bar = bar(end - Ticks(1)) + 1;
    let mut regions = vec![];
    for section in project
        .sections
        .spans_in(Ticks::ZERO, end)
        .into_iter()
        .take(64)
    {
        let start = bar(section.start);
        regions.push(json!({"kind":"section","name":section.label,"start_bar":start,"end_bar":(start+2).min(bar(section.end-Ticks(1))+1)}));
        if section.start > Ticks::ZERO {
            regions.push(json!({"kind":"boundary","start_bar":start.saturating_sub(1).max(1),"end_bar":(start+1).min(end_bar)}));
        }
    }
    regions.push(
        json!({"kind":"ending","start_bar":end_bar.saturating_sub(2).max(1),"end_bar":end_bar}),
    );
    if project.loop_enabled
        && let Some((start, end)) = project.loop_region
    {
        regions.push(json!({"kind":"loop_join","tail_tick":end.raw(),"head_tick":start.raw()}));
    }
    json!(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dotted_production_keys_survive_replacement_and_resume() {
        let mut record = Record::default();
        for text in ["first", "updated"] {
            record
                .update(Operation::Put {
                    entry: Entry {
                        key: "local_edit.validation.intent".into(),
                        category: Category::Intent,
                        source: Source::AgentAssumption,
                        basis: "runtime trial".into(),
                        text: text.into(),
                        clips: vec![],
                    },
                })
                .unwrap();
        }
        let restored: Record =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].text, "updated");
        let mut invalid = restored.entries[0].clone();
        invalid.key = "not/a/path".into();
        assert!(record.update(Operation::Put { entry: invalid }).is_err());
        assert_eq!(record.entries.len(), 1);
    }
    #[test]
    fn evidence_invalidates_on_revision_or_document_and_never_verifies_human_listening() {
        let mut record = Record::default();
        let state = json!({"revision":"a","session_revision":4,"document_instance":"one",
            "verification_regions":[{"kind":"section","start_bar":1,"end_bar":3},{"kind":"loop_join"}]});
        record.observe(
            &state,
            "inspect_audio",
            4,
            Some((1, 2)),
            None,
            json!({"peak_dbfs":-6}),
        );
        assert_eq!(record.view(&state)["coverage"][0]["measurement"], "current");
        assert_eq!(
            record.view(&state)["coverage"][0]["human_listening"],
            "unverified"
        );
        assert_eq!(
            record.view(&state)["coverage"][1]["measurement"],
            "unverified"
        );
        for field in ["revision", "document_instance"] {
            let mut changed = state.clone();
            changed[field] = "different".into();
            assert_eq!(record.view(&changed)["evidence"][0]["status"], "stale");
            assert_eq!(
                record.view(&changed)["coverage"][0]["measurement"],
                "unverified"
            );
        }
        record.observe(&state, "inspect_audio", 5, Some((1, 2)), None, json!({}));
        assert_eq!(record.evidence.len(), 1);
        let mut undone = state.clone();
        undone["session_revision"] = 6.into();
        assert_eq!(record.view(&undone)["evidence"][0]["status"], "stale");
    }
    #[test]
    fn attributed_motifs_survive_round_trip_and_report_missing_ids() {
        let mut record = Record::default();
        record
            .update(Operation::Put {
                entry: Entry {
                    key: "motif".into(),
                    category: Category::Motif,
                    source: Source::UserRequest,
                    basis: "keep the pickup".into(),
                    text: "retain rising pickup".into(),
                    clips: vec![7],
                },
            })
            .unwrap();
        let restored: Record =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            restored.view(&json!({}))["entries"][0]["missing_clips"],
            json!([7])
        );
        assert!(serde_json::from_value::<Operation>(json!({"kind":"put","evidence":[]})).is_err());
    }
}
