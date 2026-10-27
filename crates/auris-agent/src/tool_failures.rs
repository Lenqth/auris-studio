//! Per-turn failure accounting for tool loops and native CLI recovery.

use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ToolFailures(BTreeMap<(String, String), usize>);

pub(crate) fn signature(name: &str, args: &str) -> (String, String) {
    let args = serde_json::from_str::<serde_json::Value>(args)
        .map(|mut value| {
            // Dispatch adds correlation evidence after the guard observes the original call.
            // It must not make identical failed arguments look different on every retry.
            if let Some(object) = value.as_object_mut() {
                object.remove("__auris_call_origin");
            }
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
    fn dispatch_identity_does_not_reset_the_original_argument_failure_budget() {
        let mut failures = ToolFailures::default();
        for id in ["first", "second", "third"] {
            let args = serde_json::json!({"track":99,"__auris_call_origin":{"kind":"tool_call","call_id":id,"tool":"read_notes"}});
            failures.record("read_notes", &args.to_string(), true);
        }
        assert!(failures.exhausted("read_notes", r#"{"track":99}"#, 3));
        assert!(!failures.exhausted("read_notes", r#"{"track":100}"#, 3));
        failures.record("read_notes", r#"{"track":99}"#, false);
        assert!(!failures.exhausted("read_notes", r#"{"track":99}"#, 1));
    }

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
