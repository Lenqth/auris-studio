pub mod controls;
pub mod edges;
pub mod graph;
pub mod minimap;
mod rem_scope;
pub mod store;
pub mod types;

pub use controls::Controls;
pub use graph::FlowGraph;
pub use minimap::Minimap;
pub use store::FlowState;
pub use types::*;

/// gpui-pre asks the app for the host OS to resolve its secondary modifier. The host is known at
/// compile time, and asking would borrow `cx` inside closures that already hold it.
trait ModifiersExt {
    fn is_secondary(&self) -> bool;
}
impl ModifiersExt for gpui::Modifiers {
    fn is_secondary(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.platform
        } else {
            self.control
        }
    }
}
