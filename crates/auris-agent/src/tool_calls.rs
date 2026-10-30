//! Per-request Auris tool allowance, independent of transport and failed-call recovery.

const PREFIX: &str = "Auris tool call limit reached (";

/// Recover the limit from the stable diagnostic stored in transcripts and candidates.
/// Unknown errors are left to their existing presentation path.
pub fn limit_from_error(error: &str) -> Option<usize> {
    error.strip_prefix(PREFIX)?.strip_suffix(").")?.parse().ok()
}

pub(super) fn limit_error(limit: usize) -> String {
    format!("{PREFIX}{limit}).")
}

pub(super) struct Budget {
    limit: usize,
    calls: usize,
    exhausted: bool,
}

impl Budget {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            calls: 0,
            exhausted: false,
        }
    }

    pub(super) fn reset(&mut self) {
        self.calls = 0;
        self.exhausted = false;
    }

    pub(super) fn reserve(&mut self) -> Result<(), String> {
        if self.calls >= self.limit {
            self.exhausted = true;
            return Err(limit_error(self.limit));
        }
        self.calls += 1;
        Ok(())
    }

    pub(super) fn error(&self) -> Option<String> {
        self.exhausted.then(|| limit_error(self.limit))
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(auris_session::AgentPreferences::default().tool_call_limit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_the_exact_allowance_and_resets_only_for_a_new_request() {
        let mut budget = Budget::new(2);
        budget.reserve().unwrap();
        budget.reserve().unwrap();
        assert_eq!(limit_from_error(&budget.reserve().unwrap_err()), Some(2));
        assert!(budget.reserve().is_err());
        budget.reset();
        budget.reserve().unwrap();
    }
}
