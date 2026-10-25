//! Per-turn failure accounting for tool loops and native CLI recovery.

use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ToolFailures(BTreeMap<(String, String), usize>);

pub(crate) fn signature(name: &str, args: &str) -> (String, String) {
    let args = serde_json::from_str::<serde_json::Value>(args)
        .map(|mut value| {
            value.sort_all_objects();
            value.to_string()
        })
        .unwrap_or_else(|_| args.to_owned());
    (name.to_owned(), args)
}

impl ToolFailures {
    pub(crate) fn record(&mut self, name: &str, args: &str, failed: bool) {
        let key = signature(name, args);
        if failed {
            let count = self.0.entry(key).or_default();
            *count = count.saturating_add(1);
        } else {
            self.0.remove(&key);
        }
    }

    pub(crate) fn exhausted(&self, name: &str, args: &str, limit: usize) -> bool {
        self.0.get(&signature(name, args)).copied().unwrap_or(0) >= limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_count_normalized_arguments_and_success_resets_only_that_call() {
        let mut failures = ToolFailures::default();
        for _ in 0..4 {
            failures.record("edit", r#"{"b":2,"a":1}"#, true);
        }
        assert!(!failures.exhausted("edit", r#"{"a":1,"b":2}"#, 5));
        failures.record("edit", r#"{ "a": 1, "b": 2 }"#, true);
        assert!(failures.exhausted("edit", r#"{"b":2,"a":1}"#, 5));
        assert!(!failures.exhausted("edit", r#"{"a":2,"b":2}"#, 5));
        failures.record("other", "{}", true);
        failures.record("edit", r#"{"b":2,"a":1}"#, false);
        assert!(!failures.exhausted("edit", r#"{"a":1,"b":2}"#, 1));
        assert!(failures.exhausted("other", "{}", 1));
    }
}
