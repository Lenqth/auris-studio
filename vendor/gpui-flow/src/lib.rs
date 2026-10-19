pub mod controls;
pub mod edges;
pub mod graph;
pub mod minimap;
pub mod store;
pub mod types;

pub use controls::Controls;
pub use graph::FlowGraph;
pub use minimap::Minimap;
pub use store::FlowState;
pub use types::*;

// Compatibility with released GPUI, whose Pixels converts through From.
trait PixelsExt {
    fn as_f32(self) -> f32;
}
impl PixelsExt for gpui::Pixels {
    fn as_f32(self) -> f32 {
        self.into()
    }
}
