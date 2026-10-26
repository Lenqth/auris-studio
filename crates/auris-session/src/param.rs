//! Addressing a parameter wherever it lives.
//!
//! The types moved down to [`auris_core::param`] when the document gained automation: a lane is a
//! [`ParamTarget`] and a shape, and the document model may not name a crate above it. This module
//! re-exports them so `auris_session::param::ParamTarget` still resolves — the same arrangement
//! `auris-compose` has with `auris_core::theory`, and for the same reason.

pub use auris_core::param::{
    ParamDescriptor, ParamId, ParamTarget, ParamUnit, ParamValueCurve, db_to_gain, gain_to_db,
};

impl crate::Session {
    /// Shared model-facing control metadata for instruments and mixer inserts.
    pub(crate) fn agent_parameter_report(
        &self,
        target: ParamTarget,
        descriptor: &ParamDescriptor,
    ) -> serde_json::Value {
        let normalized = descriptor.key.starts_with("vst3.");
        serde_json::json!({
            "key": descriptor.key,
            "name": descriptor.name,
            "value": self.param_value(target, descriptor),
            "min": descriptor.min,
            "max": descriptor.max,
            "default": descriptor.default,
            "unit": if normalized { ParamUnit::Plain } else { descriptor.unit },
            "display_unit": descriptor.unit,
            "value_domain": if normalized { "normalized" } else { "native" },
            "steps": descriptor.steps,
            "read_only": descriptor.read_only,
            "choices": descriptor.choices.iter().take(16).collect::<Vec<_>>(),
            "choice_count": descriptor.choices.len(),
            "automated": self.is_automated(target),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_reports_normalized_vst3_values_separately_from_display_units() {
        let session = crate::Session::new(crate::SessionOptions::headless()).unwrap();
        let descriptor = ParamDescriptor::new(0u32, "vst3.42", "Frequency", 0.0, 1.0, 0.5)
            .with_unit(ParamUnit::Hertz)
            .with_steps(5);
        let report = session.agent_parameter_report(ParamTarget::MasterPan, &descriptor);
        assert_eq!(report["value_domain"], "normalized");
        assert_eq!(report["unit"], "Plain");
        assert_eq!(report["display_unit"], "Hertz");
        assert_eq!(report["steps"], 5);
        assert_eq!(report["read_only"], false);
    }
}
