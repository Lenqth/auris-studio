//! Per-user-turn repair budget shared by every live transport on a bridge.
use auris_session::lyric_harness::RepairRequest;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct Budget {
    rounds: Vec<Round>,
}

struct Round {
    target: Value,
    revision: String,
    contract: Value,
    candidates: Vec<Value>,
    applications: Vec<Value>,
}

impl Budget {
    pub(crate) fn reset(&mut self) {
        self.rounds.clear();
    }

    pub(crate) fn reserve(&mut self, request: &RepairRequest) -> Result<(), String> {
        if request.candidates.is_empty() || request.candidates.len() > 3 {
            return Err("A lyric repair round accepts 1..3 alternatives".into());
        }
        let mut target = request.target.clone();
        target.notes.sort_unstable();
        let target = json!(target);
        let mut reference = request.reference.clone();
        if let Some(reference) = &mut reference {
            reference.notes.sort_unstable();
        }
        let mut keywords = request.required_keywords.clone();
        keywords.sort();
        let contract = json!({"reference":reference,"required_keywords":keywords});
        let index = self
            .rounds
            .iter()
            .position(|r| r.target == target && r.revision == request.expected_revision);
        let index = match index {
            Some(index) => index,
            None => {
                if self.rounds.len() >= 128 {
                    return Err("Lyric repair round limit reached for this request".into());
                }
                self.rounds.push(Round {
                    target,
                    revision: request.expected_revision.clone(),
                    contract: contract.clone(),
                    candidates: Vec::new(),
                    applications: Vec::new(),
                });
                self.rounds.len() - 1
            }
        };
        let round = &mut self.rounds[index];
        if round.contract != contract {
            return Err("Keep the reference and required keywords fixed during this repair round; report the needed contract change to the user".into());
        }
        let mut proposed = round.candidates.clone();
        for candidate in &request.candidates {
            let candidate = json!(candidate);
            if !proposed.contains(&candidate) {
                proposed.push(candidate);
            }
        }
        if proposed.len() > 3 {
            return Err("Three lyric alternatives have already been checked. Stop this repair round and report unresolved diagnostics; do not alter the melody to bypass the limit".into());
        }
        if !request.apply && proposed.len() == round.candidates.len() {
            return Err("These lyric alternatives were already checked; read their diagnostics instead of repeating the preview".into());
        }
        let applications: Vec<_> = request.candidates.iter().map(|c| json!(c)).collect();
        if request.apply && applications.iter().all(|c| round.applications.contains(c)) {
            return Err("These lyric alternatives were already submitted for application; do not repeat an unchanged repair".into());
        }
        round.candidates = proposed;
        if request.apply {
            for candidate in applications {
                if !round.applications.contains(&candidate) {
                    round.applications.push(candidate);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_apply_share_three_alternatives_and_a_fixed_contract() {
        let mut budget = Budget::default();
        let mut request: RepairRequest = serde_json::from_value(json!({
            "target":{"clip":1,"notes":[0,1]},"expected_revision":"r1",
            "candidates":[{"text":"あい"}],"required_keywords":["あ"]
        }))
        .unwrap();
        for text in ["あい", "あう", "あえ"] {
            request.candidates[0].text = text.into();
            budget.reserve(&request).unwrap();
        }
        request.apply = true;
        budget.reserve(&request).unwrap();
        assert!(budget.reserve(&request).unwrap_err().contains("unchanged"));
        request.candidates[0].text = "あお".into();
        assert!(budget.reserve(&request).unwrap_err().contains("Three"));
        request.candidates[0].text = "あえ".into();
        request.required_keywords.clear();
        assert!(budget.reserve(&request).unwrap_err().contains("fixed"));
        budget.reset();
        budget.reserve(&request).unwrap();
    }
}
