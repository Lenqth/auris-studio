use crate::PixelsExt;
use std::collections::{HashMap, HashSet};

use gpui::{Bounds, Pixels, SharedString};

use crate::types::*;

/// Resolved handle position in screen coordinates, stored after layout.
#[derive(Debug, Clone)]
pub struct ResolvedHandle {
    pub node_id: NodeId,
    pub handle_id: Option<SharedString>,
    pub handle_type: HandleType,
    pub position: HandlePosition,
    pub bounds: Bounds<Pixels>,
}

/// Key for looking up a handle: (node_id, handle_id).
pub type HandleKey = (NodeId, Option<SharedString>);

/// A snapshot of the graph state for undo/redo.
#[derive(Clone)]
struct HistoryEntry {
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
}

/// The central state store for a flow graph. Wrapped in `Entity<FlowState>`.
pub struct FlowState {
    /// Let an owning application validate and apply edits to its document.
    pub externally_managed: bool,
    /// Completed gestures waiting for the document owner; never speculative visual edges.
    pub pending_connections: Vec<Connection>,
    /// An edge context-menu request, in window coordinates.
    pub pending_edge_menu: Option<(EdgeId, gpui::Point<Pixels>)>,
    /// Theme selection color supplied by the document owner.
    pub edge_selection_color: Option<u32>,
    /// Text-scale multiplier for minimum separation between parallel lanes.
    pub edge_lane_scale: f32,
    /// Optional owner-sampled signal animation; `None` keeps all wires static.
    pub edge_flow: Option<EdgeFlow>,
    /// Optional document-authoritative connection preview, keyed by source and target.
    pub allowed_connections: Option<HashSet<(NodeId, NodeId)>>,
    /// Canvas origin in window coordinates, for embedding in a dock panel.
    pub origin: (f32, f32),
    /// Measured canvas bounds, used to keep hover queries inside the visible panel.
    pub canvas_bounds: Option<Bounds<Pixels>>,
    /// Painted label bounds, in paint order, for exact wire hover ownership.
    pub edge_label_bounds: Vec<(EdgeId, Bounds<Pixels>)>,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    node_lookup: HashMap<NodeId, usize>,
    pub viewport: Viewport,
    pub handle_bounds: HashMap<HandleKey, ResolvedHandle>,
    // Interaction state
    pub drag_state: Option<DragState>,
    pub pan_drag: Option<PanDragState>,
    pub connecting: Option<ConnectionDraft>,
    pub selection_box: Option<SelectionBox>,
    // Undo/redo
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    // Config
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub snap_to_grid: bool,
    pub snap_grid: (f32, f32),
    pub connection_radius: f32,
}

impl FlowState {
    /// Select a specific editable wire, also used by labels so overlapping labels never pick another edge.
    pub fn select_edge(&mut self, id: &EdgeId, additive: bool) {
        if !self
            .edges
            .iter()
            .any(|edge| edge.id == *id && edge.selectable && !edge.hidden)
        {
            return;
        }
        if !additive {
            for node in &mut self.nodes {
                node.selected = false;
            }
            for edge in &mut self.edges {
                edge.selected = false;
            }
        }
        if let Some(edge) = self.edges.iter_mut().find(|edge| edge.id == *id) {
            edge.selected = true;
        }
    }

    pub fn new(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Self {
        let mut node_lookup = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            node_lookup.insert(node.id.clone(), i);
        }

        Self {
            externally_managed: false,
            pending_connections: Vec::new(),
            pending_edge_menu: None,
            edge_selection_color: None,
            edge_lane_scale: 1.0,
            edge_flow: None,
            allowed_connections: None,
            origin: (0.0, 0.0),
            canvas_bounds: None,
            edge_label_bounds: Vec::new(),
            nodes,
            edges,
            node_lookup,
            viewport: Viewport::default(),
            handle_bounds: HashMap::new(),
            drag_state: None,
            pan_drag: None,
            connecting: None,
            selection_box: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            min_zoom: 0.8,
            max_zoom: 2.5,
            snap_to_grid: false,
            snap_grid: (20.0, 20.0),
            connection_radius: 20.0,
        }
    }

    /// Rebuild the node lookup index.
    pub fn rebuild_lookup(&mut self) {
        self.node_lookup.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_lookup.insert(node.id.clone(), i);
        }
    }

    /// Push the current state onto the undo stack. Call before destructive operations.
    pub fn push_undo(&mut self) {
        self.undo_stack.push(HistoryEntry {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        });
        // Clear redo stack on new action
        self.redo_stack.clear();
        // Limit history size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last operation. Returns true if undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Redo the last undone operation. Returns true if redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&FlowNode> {
        self.node_lookup.get(id).map(|&i| &self.nodes[i])
    }

    /// Get a mutable node by ID.
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut FlowNode> {
        self.node_lookup.get(id).map(|&i| &mut self.nodes[i])
    }

    /// Set all nodes, rebuilding the index.
    pub fn set_nodes(&mut self, nodes: Vec<FlowNode>) {
        self.nodes = nodes;
        self.rebuild_lookup();
    }

    /// Set all edges.
    pub fn set_edges(&mut self, edges: Vec<FlowEdge>) {
        self.edges = edges;
    }

    /// Apply a batch of node changes.
    pub fn apply_node_changes(&mut self, changes: &[NodeChange]) {
        let mut needs_rebuild = false;

        for change in changes {
            match change {
                NodeChange::Position {
                    id,
                    position,
                    dragging,
                } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.position = *position;
                        node.dragging = *dragging;
                    }
                }
                NodeChange::Dimensions { id, width, height } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.measured_width = Some(*width);
                        node.measured_height = Some(*height);
                    }
                }
                NodeChange::Select { id, selected } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.selected = *selected;
                    }
                }
                NodeChange::Remove { id } => {
                    if let Some(&idx) = self.node_lookup.get(id) {
                        self.nodes.remove(idx);
                        needs_rebuild = true;
                    }
                }
            }
        }

        if needs_rebuild {
            self.rebuild_lookup();
        }
    }

    /// Apply a batch of edge changes.
    pub fn apply_edge_changes(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            match change {
                EdgeChange::Select { id, selected } => {
                    if let Some(edge) = self.edges.iter_mut().find(|e| e.id == *id) {
                        edge.selected = *selected;
                    }
                }
                EdgeChange::Remove { id } => {
                    self.edges.retain(|e| e.id != *id);
                }
            }
        }
    }

    /// Set the viewport, clamping zoom to min/max.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = Viewport {
            x: viewport.x,
            y: viewport.y,
            zoom: viewport.zoom.clamp(self.min_zoom, self.max_zoom),
        };
    }

    /// Register a resolved handle position (called during layout).
    pub fn set_handle_bounds(&mut self, key: HandleKey, resolved: ResolvedHandle) {
        self.handle_bounds.insert(key, resolved);
    }

    /// Find the screen-space center of a handle.
    ///
    /// Computes the position directly from the node's flow coordinates + viewport,
    /// using measured wrapper dimensions when available.
    pub fn find_handle_center(
        &self,
        node_id: &NodeId,
        _handle_id: &Option<SharedString>,
        handle_position: HandlePosition,
    ) -> Option<(f32, f32)> {
        let node = self.get_node(node_id)?;
        let (sx, sy) = self.viewport.flow_to_screen(node.position);

        // Use measured wrapper dimensions if available, otherwise estimate.
        // measured_width/height store the full wrapper size (set by the measurement canvas).
        let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(114.0);
        let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(54.0);

        let (cx, cy) = match handle_position {
            HandlePosition::Top => (sx + w / 2.0, sy),
            HandlePosition::Bottom => (sx + w / 2.0, sy + h),
            HandlePosition::Left => (sx, sy + h / 2.0),
            HandlePosition::Right => (sx + w, sy + h / 2.0),
        };
        Some((cx + self.origin.0, cy + self.origin.1))
    }

    /// Fit visible nodes, wire paths and measured labels with padding in screen pixels.
    /// Extends the minimum zoom when necessary so a large graph can fit in its entirety.
    pub fn fit_view(&mut self, padding: f32, container_width: f32, container_height: f32) {
        if !padding.is_finite()
            || !container_width.is_finite()
            || !container_height.is_finite()
            || container_width <= 0.0
            || container_height <= 0.0
        {
            return;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut include = |x: f32, y: f32| {
            if x.is_finite() && y.is_finite() {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        };
        for node in self.nodes.iter().filter(|node| !node.hidden) {
            let (x, y) = self.viewport.flow_to_screen(node.position);
            include(x, y);
            include(
                x + node
                    .measured_width
                    .map_or(150.0 * self.viewport.zoom, |p| p.as_f32()),
                y + node
                    .measured_height
                    .map_or(40.0 * self.viewport.zoom, |p| p.as_f32()),
            );
        }
        for edge in self.edges.iter().filter(|edge| !edge.hidden) {
            if let Some(path) = crate::edges::compute_edge_path(self, edge) {
                for (x, y) in path.points {
                    include(x - self.origin.0, y - self.origin.1);
                }
            }
        }
        for (id, bounds) in &self.edge_label_bounds {
            if self.edges.iter().any(|edge| edge.id == *id && !edge.hidden) {
                include(
                    bounds.left().as_f32() - self.origin.0,
                    bounds.top().as_f32() - self.origin.1,
                );
                include(
                    bounds.right().as_f32() - self.origin.0,
                    bounds.bottom().as_f32() - self.origin.1,
                );
            }
        }
        if !min_x.is_finite() || !min_y.is_finite() {
            return;
        }
        let padding = padding
            .max(0.0)
            .min((container_width.min(container_height) / 2.0 - 1.0).max(0.0));
        let width = (max_x - min_x).max(1.0);
        let height = (max_y - min_y).max(1.0);
        let factor = ((container_width - padding * 2.0) / width)
            .min((container_height - padding * 2.0) / height);
        let zoom = (self.viewport.zoom * factor).min(self.max_zoom);
        if !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        let centre = self
            .viewport
            .screen_to_flow(min_x + width / 2.0, min_y + height / 2.0);
        self.min_zoom = self.min_zoom.min(zoom);
        self.zoom_at(zoom, 0.0, 0.0);
        self.viewport.x = container_width / 2.0 - centre.x * zoom;
        self.viewport.y = container_height / 2.0 - centre.y * zoom;
    }

    /// Zoom in by a step, centered on the container center.
    pub fn zoom_in(&mut self, container_width: f32, container_height: f32) {
        let factor = 1.2;
        let cx = container_width / 2.0;
        let cy = container_height / 2.0;
        self.zoom_at(self.viewport.zoom * factor, cx, cy);
    }

    /// Zoom out by a step, centered on the container center.
    pub fn zoom_out(&mut self, container_width: f32, container_height: f32) {
        let factor = 1.0 / 1.2;
        let cx = container_width / 2.0;
        let cy = container_height / 2.0;
        self.zoom_at(self.viewport.zoom * factor, cx, cy);
    }

    /// Set a bounded zoom, keeping a canvas-local screen point fixed in flow coordinates.
    pub fn zoom_at(&mut self, zoom: f32, x: f32, y: f32) {
        if !zoom.is_finite() || !x.is_finite() || !y.is_finite() {
            return;
        }
        let zoom = zoom.clamp(self.min_zoom, self.max_zoom);
        let ratio = zoom / self.viewport.zoom;
        self.viewport.x = x - (x - self.viewport.x) * ratio;
        self.viewport.y = y - (y - self.viewport.y) * ratio;
        self.viewport.zoom = zoom;
        // Measurements are in screen pixels. Keep cached endpoints and culling consistent
        // until the node's next layout reports its exact new bounds.
        for node in &mut self.nodes {
            node.measured_width = node.measured_width.map(|width| width * ratio);
            node.measured_height = node.measured_height.map(|height| height * ratio);
        }
        self.edge_label_bounds.clear();
    }

    /// Center the viewport on a specific flow coordinate.
    pub fn set_center(
        &mut self,
        flow_x: f32,
        flow_y: f32,
        container_width: f32,
        container_height: f32,
    ) {
        self.viewport.x = container_width / 2.0 - flow_x * self.viewport.zoom;
        self.viewport.y = container_height / 2.0 - flow_y * self.viewport.zoom;
    }

    /// Select all nodes and edges.
    pub fn select_all(&mut self) {
        for node in &mut self.nodes {
            if node.selectable {
                node.selected = true;
            }
        }
        for edge in &mut self.edges {
            if edge.selectable {
                edge.selected = true;
            }
        }
    }

    /// Get nodes that have edges pointing TO the given node.
    pub fn get_incomers(&self, node_id: &NodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.target == *node_id)
            .filter_map(|e| self.get_node(&e.source))
            .collect()
    }

    /// Get nodes that the given node has edges pointing TO.
    pub fn get_outgoers(&self, node_id: &NodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id)
            .filter_map(|e| self.get_node(&e.target))
            .collect()
    }

    /// Get all edges connected to a node (as source or target).
    pub fn get_connected_edges(&self, node_id: &NodeId) -> Vec<&FlowEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id || e.target == *node_id)
            .collect()
    }

    /// Find the nearest valid handle to snap to during connection dragging.
    pub fn find_snap_target(
        &self,
        draft: &ConnectionDraft,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<SnapTarget> {
        let radius = self.connection_radius;
        let mut best: Option<(f32, SnapTarget)> = None;

        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            // Can't connect to the same node
            if node.id == draft.from_node {
                continue;
            }
            let pair = if draft.from_type == HandleType::Source {
                (draft.from_node.clone(), node.id.clone())
            } else {
                (node.id.clone(), draft.from_node.clone())
            };
            if self
                .allowed_connections
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(&pair))
            {
                continue;
            }

            for handle in &node.handles {
                // Source→Target or Target→Source only
                if handle.handle_type == draft.from_type {
                    continue;
                }
                // Check per-handle connectable flag
                if !handle.is_connectable {
                    continue;
                }

                if let Some((hx, hy)) =
                    self.find_handle_center(&node.id, &handle.id, handle.position)
                {
                    let dx = mouse_x - hx;
                    let dy = mouse_y - hy;
                    let dist = (dx * dx + dy * dy).sqrt();

                    if dist <= radius {
                        if best.is_none() || dist < best.as_ref().unwrap().0 {
                            best = Some((
                                dist,
                                SnapTarget {
                                    node_id: node.id.clone(),
                                    handle_id: handle.id.clone(),
                                    handle_type: handle.handle_type,
                                    position: handle.position,
                                    point: (hx, hy),
                                },
                            ));
                        }
                    }
                }
            }
        }

        best.map(|(_, target)| target)
    }

    /// Check if a connection is valid (default rules).
    pub fn is_valid_connection(&self, connection: &Connection) -> bool {
        if self.allowed_connections.as_ref().is_some_and(|allowed| {
            !allowed.contains(&(connection.source.clone(), connection.target.clone()))
        }) {
            return false;
        }
        // No self-connections
        if connection.source == connection.target {
            return false;
        }

        // No duplicate edges
        let duplicate = self.edges.iter().any(|e| {
            e.source == connection.source
                && e.target == connection.target
                && e.source_handle == connection.source_handle
                && e.target_handle == connection.target_handle
        });
        if duplicate {
            return false;
        }

        true
    }

    /// Finish a gesture without creating a second copy of an application's document.
    pub fn complete_connection(&mut self, connection: Connection) {
        if self.externally_managed {
            self.pending_connections.push(connection);
        } else if self.is_valid_connection(&connection) {
            self.push_undo();
            let id: SharedString = format!("e{}-{}", connection.source, connection.target).into();
            self.add_edge_from_connection(&connection, id);
        }
    }

    /// Add an edge from a completed connection.
    pub fn add_edge_from_connection(
        &mut self,
        connection: &Connection,
        edge_id: impl Into<SharedString>,
    ) {
        let mut edge = FlowEdge::new(
            edge_id,
            connection.source.clone(),
            connection.target.clone(),
        );
        if let Some(ref sh) = connection.source_handle {
            edge.source_handle = Some(sh.clone());
        }
        if let Some(ref th) = connection.target_handle {
            edge.target_handle = Some(th.clone());
        }
        self.edges.push(edge);
    }
}
