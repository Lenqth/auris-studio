//! A graph presentation of the session's mixer, with document-authoritative connections.

use std::collections::{HashMap, HashSet, VecDeque};

use auris_i18n::Key;
use auris_session::mixer_graph::{MixerEdge, MixerEdgeKind, MixerGraph, MixerNode};
use auris_session::prelude::*;
use gpui::{AnyElement, Axis, Entity, IntoElement, MouseButton, SharedString, div, prelude::*, px};
use gpui_flow::{
    Connection, EdgeStroke, FlowEdge, FlowGraph, FlowNode, FlowPoint, FlowState, HandleDef,
    HandlePosition,
};

use crate::app::AurisApp;
use crate::theme::{Metrics, Theme};
use crate::ui::context_menu::{ContextMenu, MenuCommand};
use crate::ui::plugin_window::PluginSubject;
use crate::ui::widgets::{ButtonStyle, button, db_to_meter_position, level_meter};

const NODE_WIDTH: f32 = 176.0;
const COLUMN: f32 = 360.0;
const ROW: f32 = 280.0;
// Curve control points are separated enough to leave a label-height gap at their midpoints.
const WIRE_LANE_SPACING: f32 = 48.0;

#[cfg(test)]
#[path = "mixer_graph_wires_tests.rs"]
mod wire_tests;

/// Presentation state kept across paints and panel moves.
pub(crate) struct MixerCanvas {
    state: Entity<FlowState>,
    view: Entity<FlowGraph>,
    document: MixerGraph,
    by_id: HashMap<SharedString, MixerNode>,
    send: bool,
    palette: [u32; 4],
}

fn wire_style(kind: MixerEdgeKind, theme: &Theme) -> (gpui::Hsla, EdgeStroke) {
    match kind {
        MixerEdgeKind::Output => (theme.mixer_output_color(), EdgeStroke::Solid),
        MixerEdgeKind::Send { .. } => (theme.mixer_send_color(), EdgeStroke::Dashed),
        MixerEdgeKind::Sidechain => (theme.mixer_sidechain_color(), EdgeStroke::Dotted),
        MixerEdgeKind::Chain => (theme.text_faint, EdgeStroke::Solid),
    }
}

fn wire_palette(theme: &Theme) -> [u32; 4] {
    [
        theme.mixer_output_color(),
        theme.mixer_send_color(),
        theme.mixer_sidechain_color(),
        theme.accent,
    ]
    .map(|color| u32::from(color.to_rgb()) >> 8)
}

fn wire_lanes(graph: &MixerGraph) -> HashMap<SharedString, f32> {
    let mut pairs: HashMap<_, Vec<_>> = HashMap::new();
    for edge in &graph.edges {
        if !matches!(edge.kind, MixerEdgeKind::Chain) {
            pairs
                .entry((edge.source, edge.target))
                .or_default()
                .push(edge);
        }
    }
    pairs
        .into_values()
        .flat_map(|mut edges| {
            edges.sort_by_key(|edge| match edge.kind {
                MixerEdgeKind::Output => (0, 0),
                MixerEdgeKind::Send { send, .. } => (1, send.0),
                MixerEdgeKind::Sidechain => (2, 0),
                MixerEdgeKind::Chain => (3, 0),
            });
            let center = (edges.len() - 1) as f32 / 2.0;
            edges.into_iter().enumerate().map(move |(index, edge)| {
                (edge_id(edge), (index as f32 - center) * WIRE_LANE_SPACING)
            })
        })
        .collect()
}

fn wire_legend(label: &'static str, color: gpui::Hsla, stroke: EdgeStroke) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, (), window, _| {
                    let y = f32::from(bounds.center().y);
                    let path = gpui_flow::edges::EdgePath {
                        points: vec![
                            (f32::from(bounds.left()), y),
                            (f32::from(bounds.right()), y),
                        ],
                        label: (0.0, 0.0),
                    };
                    gpui_flow::edges::paint_edge_path(&path, stroke, 2.0, color.into(), window);
                },
            )
            .w_6()
            .h_2(),
        )
        .child(label)
        .into_any_element()
}

fn node_id(node: MixerNode) -> SharedString {
    match node {
        MixerNode::Strip(owner) => format!("mix-track-{}", owner.map_or(0, |id| id.0)),

        MixerNode::Effect(_, slot) => format!("mix-effect-{}", slot.0),
    }
    .into()
}

fn edge_id(edge: &MixerEdge) -> SharedString {
    let owner = match edge.source {
        MixerNode::Strip(owner) | MixerNode::Effect(owner, _) => owner,
    };
    let source = MixerNode::Strip(owner).key();
    match edge.kind {
        MixerEdgeKind::Send { send, .. } => format!("mix-send-{}", send.0),
        MixerEdgeKind::Output => format!("mix-output-{source}-{}", edge.target.key()),
        MixerEdgeKind::Sidechain => format!("mix-key-{source}-{}", edge.target.key()),
        MixerEdgeKind::Chain => format!("mix-chain-{}-{}", edge.source.key(), edge.target.key()),
    }
    .into()
}

fn gesture(graph: &MixerGraph, connection: &Connection) -> Option<(TrackId, MixerNode)> {
    let source = graph.nodes.iter().find_map(|node| match node {
        MixerNode::Strip(Some(track)) | MixerNode::Effect(Some(track), _)
            if node_id(*node) == connection.source =>
        {
            Some(*track)
        }
        _ => None,
    })?;
    let target = graph
        .nodes
        .iter()
        .find(|node| node_id(**node) == connection.target)?;
    Some((source, *target))
}

/// Longest-path columns make every dependency point forward, including sidechains.
fn positions(graph: &MixerGraph) -> HashMap<MixerNode, FlowPoint> {
    let indices: HashMap<_, _> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, i))
        .collect();
    let mut incoming = vec![0; graph.nodes.len()];
    let mut outgoing = vec![Vec::new(); graph.nodes.len()];
    for edge in &graph.edges {
        if let (Some(&from), Some(&to)) = (indices.get(&edge.source), indices.get(&edge.target)) {
            outgoing[from].push(to);
            incoming[to] += 1;
        }
    }
    let mut columns = vec![0; graph.nodes.len()];
    let mut ready: VecDeque<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    while let Some(from) = ready.pop_front() {
        for &to in &outgoing[from] {
            columns[to] = columns[to].max(columns[from] + 1);
            incoming[to] -= 1;
            if incoming[to] == 0 {
                ready.push_back(to);
            }
        }
    }
    let mut rows = HashMap::new();
    let mut y = 24.0;
    for node in &graph.nodes {
        if let MixerNode::Strip(owner) = node {
            rows.insert(*owner, y);
            let sends = graph.edges.iter().filter(|edge| matches!(edge.kind, MixerEdgeKind::Send { track, .. } if Some(track) == *owner)).count();
            y += ROW + sends as f32 * 64.0;
        }
    }
    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let owner = match node {
                MixerNode::Strip(t) | MixerNode::Effect(t, _) => t,
            };
            (
                *node,
                FlowPoint::new(24.0 + columns[i] as f32 * COLUMN, rows[owner]),
            )
        })
        .collect()
}

impl AurisApp {
    /// The graph is rebuilt from the document, preserving the positions of surviving identities.
    pub(crate) fn render_mixer_graph(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let document = self.session.mixer_graph();
        if self.mixer_canvas.is_none() {
            let state = cx.new(|_| {
                let mut state = FlowState::new(Vec::new(), Vec::new());
                state.externally_managed = true;
                // Released GPUI cannot scale arbitrary child controls. Keep the canvas at
                // native control size and use panning, so faders remain readable and operable.
                state.min_zoom = 1.0;
                state.max_zoom = 1.0;
                state
            });
            cx.observe(&state, |this, state, cx| {
                this.apply_mixer_connections(&state, cx);
                cx.notify();
            })
            .detach();
            let app = cx.entity().downgrade();
            let view = cx.new(|cx| {
                FlowGraph::new(state.clone(), cx)
                    .no_node_chrome()
                    .default_renderer(move |node, _, cx| {
                        app.update(cx, |this, cx| this.render_mixer_node(&node.id, cx))
                            .unwrap_or_else(|_| div().into_any_element())
                    })
            });
            self.mixer_canvas = Some(MixerCanvas {
                state,
                view,
                document: MixerGraph {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                },
                by_id: HashMap::new(),
                send: !self.mixer_connect_send,
                palette: [0; 4],
            });
        }
        self.sync_mixer_graph(&document, cx);
        let theme = self.theme.clone();
        let view = self.mixer_canvas.as_ref().map(|canvas| canvas.view.clone());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(theme.surface_sunken)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .min_h(Metrics::PANEL_HEADER_HEIGHT)
                    .py_1()
                    .bg(theme.surface_raised)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text)
                            .child(self.t(Key::MixerGraph)),
                    )
                    .child(button(
                        "mixer-connect-output",
                        self.t(Key::OutputShort),
                        ButtonStyle::Ghost,
                        !self.mixer_connect_send,
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.mixer_connect_send = false;
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "mixer-connect-send",
                        self.t(Key::Sends),
                        ButtonStyle::Ghost,
                        self.mixer_connect_send,
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.mixer_connect_send = true;
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "mixer-add-bus",
                        self.t(Key::MenuNewBusTrack),
                        ButtonStyle::Ghost,
                        false,
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.add_bus_track();
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "mixer-layout",
                        self.t(Key::MixerLayout),
                        ButtonStyle::Ghost,
                        false,
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.arrange_mixer_nodes(cx);
                            cx.notify();
                        }),
                    ))
                    .child(div().flex_1())
                    .child(button(
                        "mixer-show-console",
                        self.t(Key::MixerConsole),
                        ButtonStyle::Ghost,
                        false,
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.mixer_graph_visible = false;
                            cx.notify();
                        }),
                    )),
            )
            .child(div().flex_1().min_h_0().min_w_0().children(view))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(wire_legend(
                                self.t(Key::OutputShort),
                                theme.mixer_output_color(),
                                EdgeStroke::Solid,
                            ))
                            .child(wire_legend(
                                self.t(Key::Sends),
                                theme.mixer_send_color(),
                                EdgeStroke::Dashed,
                            ))
                            .child(wire_legend(
                                self.t(Key::MenuSidechain),
                                theme.mixer_sidechain_color(),
                                EdgeStroke::Dotted,
                            )),
                    )
                    .child(self.t(Key::MixerGraphHelp)),
            )
            .into_any_element()
    }

    fn apply_mixer_connections(&mut self, state: &Entity<FlowState>, cx: &mut gpui::Context<Self>) {
        let (pending, menu) = state.update(cx, |state, _| {
            (
                std::mem::take(&mut state.pending_connections),
                state.pending_edge_menu.take(),
            )
        });
        if let Some((_, at)) = menu {
            let selected: HashSet<_> = state
                .read(cx)
                .edges
                .iter()
                .filter(|edge| edge.selected && edge.deletable)
                .map(|edge| edge.id.clone())
                .collect();
            let edges: Vec<_> = self
                .session
                .mixer_graph()
                .edges
                .into_iter()
                .filter(|edge| selected.contains(&edge_id(edge)))
                .collect();
            if !edges.is_empty() {
                self.open_menu(ContextMenu::new(at, self.t(Key::MixerGraph)).item(
                    self.t(Key::MixerDeleteConnections),
                    MenuCommand::DeleteMixerConnections(edges),
                ));
            }
        }
        for connection in pending {
            let document = self.session.mixer_graph();
            let result = gesture(&document, &connection).map(|(from, to)| {
                self.session
                    .connect_mixer(from, to, self.mixer_connect_send)
            });
            match result {
                Some(Ok(true)) => {}
                Some(Err(error)) => self.set_failed_status(self.failure(Key::MixerGraph, &error)),
                _ => self.set_failed_status(self.t(Key::MixerConnectionRejected).to_string()),
            }
        }
    }

    pub(crate) fn delete_mixer_connections(&mut self, edges: &[MixerEdge]) {
        if let Err(error) = self.session.disconnect_mixer_edges(edges) {
            self.set_failed_status(self.failure(Key::MixerGraph, &error));
        }
    }

    pub(crate) fn delete_focused_mixer_connections(
        &mut self,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(canvas) = self
            .mixer_canvas
            .as_ref()
            .filter(|canvas| canvas.view.read(cx).is_focused(window))
        else {
            return false;
        };
        let selected: HashSet<_> = canvas
            .state
            .read(cx)
            .edges
            .iter()
            .filter(|edge| edge.selected && edge.deletable)
            .map(|edge| edge.id.clone())
            .collect();
        let edges: Vec<_> = self
            .session
            .mixer_graph()
            .edges
            .into_iter()
            .filter(|edge| selected.contains(&edge_id(edge)))
            .collect();
        self.delete_mixer_connections(&edges);
        true
    }

    fn sync_mixer_graph(&mut self, document: &MixerGraph, cx: &mut gpui::Context<Self>) {
        let palette = wire_palette(&self.theme);
        let changed = self.mixer_canvas.as_ref().is_some_and(|canvas| {
            canvas.document != *document
                || canvas.send != self.mixer_connect_send
                || canvas.palette != palette
        });
        let routing_changed = self.mixer_canvas.as_ref().is_some_and(|canvas| {
            canvas.send != self.mixer_connect_send
                || canvas.document.nodes != document.nodes
                || canvas.document.edges.len() != document.edges.len()
                || canvas
                    .document
                    .edges
                    .iter()
                    .zip(&document.edges)
                    .any(|(a, b)| {
                        a.source != b.source
                            || a.target != b.target
                            || std::mem::discriminant(&a.kind) != std::mem::discriminant(&b.kind)
                    })
        });
        let mut allowed = HashSet::new();
        if routing_changed {
            for node in &document.nodes {
                let source = match node {
                    MixerNode::Strip(Some(id)) | MixerNode::Effect(Some(id), _) => Some(*id),
                    _ => None,
                };
                if let Some(source) = source
                    && self.session.mixer_graph_output(Some(source)) == *node
                {
                    for target in &document.nodes {
                        if self
                            .session
                            .can_connect_mixer(source, *target, self.mixer_connect_send)
                        {
                            allowed.insert((node_id(*node), node_id(*target)));
                        }
                    }
                }
            }
        }
        let theme = self.theme.clone();
        let Some(mut canvas) = self.mixer_canvas.take() else {
            return;
        };
        canvas.view.update(cx, |view, cx| {
            view.set_colors(
                theme.surface_sunken,
                theme.border_subtle,
                theme.accent,
                theme.text_muted,
            );
            cx.notify();
        });
        if !changed {
            self.mixer_canvas = Some(canvas);
            return;
        }
        let layout = positions(document);
        let lanes = wire_lanes(document);
        let old_owners: HashSet<_> = canvas.document.nodes.iter().copied().collect();
        let mut sidechain_slots = HashSet::new();
        for node in &document.nodes {
            if let MixerNode::Effect(track, slot) = node
                && self.session.effect_wants_sidechain(*track, *slot)
            {
                sidechain_slots.insert(*slot);
            }
        }
        canvas.state.update(cx, |state, _| {
            let nodes = document
                .nodes
                .iter()
                .map(|node| {
                    let id = node_id(*node);
                    let existing = state.get_node(&id).filter(|_| old_owners.contains(node));
                    let position = existing.map_or(layout[node], |n| n.position);
                    let handles = match node {
                        MixerNode::Strip(owner) => {
                            let mut handles = Vec::new();
                            if owner.is_some() && self.session.mixer_graph_output(*owner) == *node {
                                handles.push(HandleDef::source(HandlePosition::Right));
                            }
                            if owner.is_none_or(|id| {
                                self.project()
                                    .track(id)
                                    .is_some_and(|track| track.kind.is_bus())
                            }) {
                                handles.push(HandleDef::target(HandlePosition::Left));
                            }
                            handles
                        }
                        MixerNode::Effect(owner, slot) => {
                            let mut handles = Vec::new();
                            if owner.is_some() && self.session.mixer_graph_output(*owner) == *node {
                                handles.push(HandleDef::source(HandlePosition::Right));
                            }
                            if sidechain_slots.contains(slot) {
                                handles
                                    .push(HandleDef::target(HandlePosition::Top).id("sidechain"));
                            }
                            handles
                        }
                    };
                    let mut flow = FlowNode::new(id, position.x, position.y)
                        .size(NODE_WIDTH, 136.0)
                        .handles(handles);
                    flow.deletable = false;
                    if let Some(existing) = existing {
                        flow.measured_width = existing.measured_width;
                        flow.measured_height = existing.measured_height;
                        flow.selected = existing.selected;
                        flow.dragging = existing.dragging;
                    }
                    flow
                })
                .collect();
            let edges = document
                .edges
                .iter()
                .map(|edge| {
                    let id = edge_id(edge);
                    let (color, stroke) = wire_style(edge.kind, &theme);
                    let mut flow =
                        FlowEdge::new(id.clone(), node_id(edge.source), node_id(edge.target))
                            .color(u32::from(color.to_rgb()) >> 8);
                    flow.stroke = stroke;
                    flow.lane_offset = lanes.get(&id).copied().unwrap_or_default();
                    flow.selected = state
                        .edges
                        .iter()
                        .any(|existing| existing.id == id && existing.selected);
                    flow.deletable = !matches!(edge.kind, MixerEdgeKind::Chain);
                    flow.selectable = flow.deletable;
                    if !flow.deletable {
                        flow.stroke_width = Some(1.0);
                    }
                    match edge.kind {
                        MixerEdgeKind::Send {
                            send,
                            level_db,
                            pre_fader,
                            ..
                        } => {
                            flow = flow.label(format!(
                                "{} #{} {level_db:.1} dB{}",
                                self.t(Key::Sends),
                                send.0,
                                if pre_fader {
                                    format!(" · {}", self.t(Key::SendPreFaderMark))
                                } else {
                                    String::new()
                                }
                            ));
                        }
                        MixerEdgeKind::Sidechain => {
                            flow = flow
                                .label(self.t(Key::MenuSidechain))
                                .target_handle("sidechain");
                        }
                        MixerEdgeKind::Output => flow = flow.label(self.t(Key::OutputShort)),
                        _ => {}
                    }
                    flow
                })
                .collect();
            state.set_nodes(nodes);
            state.set_edges(edges);
            state.edge_selection_color = Some(u32::from(theme.accent.to_rgb()) >> 8);
            if routing_changed {
                state.allowed_connections = Some(allowed);
            }
        });
        canvas.document = document.clone();
        canvas.by_id = document
            .nodes
            .iter()
            .map(|node| (node_id(*node), *node))
            .collect();
        canvas.send = self.mixer_connect_send;
        canvas.palette = palette;
        self.mixer_canvas = Some(canvas);
    }

    fn arrange_mixer_nodes(&mut self, cx: &mut gpui::Context<Self>) {
        let layout = positions(&self.session.mixer_graph());
        if let Some(canvas) = &self.mixer_canvas {
            canvas.state.update(cx, |state, _| {
                for (node, position) in layout {
                    if let Some(node) = state.get_node_mut(&node_id(node)) {
                        node.position = position;
                    }
                }
                state.viewport = Default::default();
            });
            canvas.view.update(cx, |_, cx| cx.notify());
        }
    }

    fn render_mixer_node(&mut self, id: &SharedString, cx: &mut gpui::Context<Self>) -> AnyElement {
        let Some(node) = self
            .mixer_canvas
            .as_ref()
            .and_then(|canvas| canvas.by_id.get(id))
            .copied()
        else {
            return div().into_any_element();
        };
        let owner = match node {
            MixerNode::Strip(t) | MixerNode::Effect(t, _) => t,
        };
        let theme = self.theme.clone();
        let name = owner.and_then(|id| self.project().track(id)).map_or_else(
            || self.t(Key::Master).to_string(),
            |track| track.name.clone(),
        );
        let mut card = div()
            .w(px(NODE_WIDTH))
            .min_h(px(96.0))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .bg(theme.surface)
            .border_1()
            .border_color(if self.selected_track == owner {
                theme.accent
            } else {
                theme.border
            })
            .rounded(Metrics::RADIUS_MD)
            .text_xs()
            .text_color(theme.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if let Some(id) = owner {
                        this.select_track(id);
                    }
                    cx.notify();
                }),
            )
            .child(div().truncate().text_color(theme.text_muted).child(name));
        match node {
            MixerNode::Effect(_, slot) => {
                let effect = owner
                    .and_then(|id| self.project().track(id))
                    .map_or(&self.project().master, |t| &t.mixer)
                    .effects
                    .iter()
                    .find(|effect| effect.id == slot);
                if let Some(effect) = effect {
                    let label = self.effect_label(slot, &effect.effect_id.clone());
                    let enabled = effect.enabled;
                    let menu_label = label.clone();
                    card = card
                        .child(button(
                            SharedString::from(format!("graph-effect-open-{id}")),
                            label,
                            ButtonStyle::Ghost,
                            false,
                            theme.accent_soft,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                this.open_plugin_window(PluginSubject::Insert {
                                    track: owner,
                                    slot,
                                });
                                cx.notify();
                            }),
                        ))
                        .child(button(
                            SharedString::from(format!("graph-effect-enabled-{id}")),
                            self.t(Key::MenuEnabled),
                            ButtonStyle::Ghost,
                            enabled,
                            theme.accent_soft,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                this.session.set_effect_enabled(owner, slot, !enabled);
                                cx.notify();
                            }),
                        ))
                        .child(button(
                            SharedString::from(format!("graph-effect-menu-{id}")),
                            self.t(Key::Effect),
                            ButtonStyle::Ghost,
                            false,
                            theme.accent_soft,
                            &theme,
                            Self::opens_menu(cx, move |this, at| {
                                this.effect_menu(at, owner, slot, menu_label.clone())
                            }),
                        ));
                }
            }
            MixerNode::Strip(_) => {
                let strip = owner
                    .and_then(|id| self.project().track(id))
                    .map_or(&self.project().master, |t| &t.mixer);
                let (gain, pan, muted, soloed) = (strip.gain_db, strip.pan, strip.mute, strip.solo);
                card = card
                    .child(button(
                        SharedString::from(format!("graph-add-fx-{id}")),
                        self.t(Key::MenuAddEffect),
                        ButtonStyle::Ghost,
                        false,
                        theme.accent_soft,
                        &theme,
                        Self::opens_menu(cx, move |this, at| this.effect_picker_menu(at, owner)),
                    ))
                    .child(self.fader(
                        SharedString::from(format!("graph-gain-{id}")),
                        self.t(Key::Volume),
                        owner.map_or(ParamTarget::MasterGain, ParamTarget::TrackGain),
                        gain,
                        cx,
                    ))
                    .child(self.fader(
                        SharedString::from(format!("graph-pan-{id}")),
                        self.t(Key::Pan),
                        owner.map_or(ParamTarget::MasterPan, ParamTarget::TrackPan),
                        pan,
                        cx,
                    ));
                if let Some(track) = owner {
                    let index = self.project().track_index(track).unwrap_or(0);
                    let sends: Vec<_> = self
                        .project()
                        .track(track)
                        .into_iter()
                        .flat_map(|track| &track.sends)
                        .cloned()
                        .collect();
                    let level_db = gain_to_db(self.track_level(index));
                    card = card
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(
                                    button(
                                        SharedString::from(format!("graph-mute-{id}")),
                                        self.t(Key::MuteInitial),
                                        ButtonStyle::Normal,
                                        muted,
                                        theme.mute,
                                        &theme,
                                        cx.listener(move |this, _, _, cx| {
                                            this.toggle_mute(track);
                                            cx.notify();
                                        }),
                                    )
                                    .tooltip(self.tip(Key::CmdToggleTrackMute, "track.mute")),
                                )
                                .child(
                                    button(
                                        SharedString::from(format!("graph-solo-{id}")),
                                        self.t(Key::SoloInitial),
                                        ButtonStyle::Normal,
                                        soloed,
                                        theme.solo,
                                        &theme,
                                        cx.listener(move |this, _, _, cx| {
                                            this.toggle_solo(track);
                                            cx.notify();
                                        }),
                                    )
                                    .tooltip(self.tip(Key::CmdToggleTrackSolo, "track.solo")),
                                ),
                        )
                        .child(div().h(px(6.0)).child(level_meter(
                            db_to_meter_position(level_db),
                            db_to_meter_position(level_db),
                            self.session.meters().track_clipped(index),
                            Axis::Horizontal,
                            theme.meter_color(level_db),
                            &theme,
                        )))
                        .child(button(
                            SharedString::from(format!("graph-route-menu-{id}")),
                            self.t(Key::OutputShort),
                            ButtonStyle::Ghost,
                            false,
                            theme.accent_soft,
                            &theme,
                            Self::opens_menu(cx, move |this, at| this.output_menu(at, track)),
                        ));
                    if self.session.buses().next().is_some() {
                        card = card.child(button(
                            SharedString::from(format!("graph-send-menu-{id}")),
                            self.t(Key::Sends),
                            ButtonStyle::Ghost,
                            false,
                            theme.accent_soft,
                            &theme,
                            Self::opens_menu(cx, move |this, at| this.send_picker_menu(at, track)),
                        ));
                    }
                    for (position, send) in sends.iter().enumerate() {
                        card = card.child(self.send_row(
                            index,
                            position,
                            track,
                            send.id,
                            send.target,
                            send.level_db,
                            send.pre_fader,
                            cx,
                        ));
                    }
                } else {
                    let level_db = gain_to_db(self.master_level());
                    card = card.child(div().h(px(6.0)).child(level_meter(
                        db_to_meter_position(level_db),
                        db_to_meter_position(level_db),
                        self.session.meters().master_clipped(),
                        Axis::Horizontal,
                        theme.meter_color(level_db),
                        &theme,
                    )));
                }
            }
        }
        card.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::{Dock, Panel};
    use crate::harness::{click, drag, open, paint};
    use gpui::{TestAppContext, VisualTestContext};

    fn selector(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    fn expose_pair(
        app: &Entity<AurisApp>,
        cx: &mut VisualTestContext,
        from: MixerNode,
        to: MixerNode,
    ) {
        app.update(cx, |this, cx| {
            let canvas = this.mixer_canvas.as_ref().unwrap();
            canvas.state.update(cx, |state, _| {
                for node in &mut state.nodes {
                    node.position = if node.id == node_id(from) {
                        FlowPoint::new(24.0, 24.0)
                    } else if node.id == node_id(to) {
                        FlowPoint::new(380.0, 24.0)
                    } else {
                        FlowPoint::new(3000.0, 3000.0)
                    };
                }
            });
            canvas.view.update(cx, |_, cx| cx.notify());
        });
        paint(app, cx);
    }

    fn connect(cx: &mut VisualTestContext, from: MixerNode, to: MixerNode) {
        let source = cx
            .debug_bounds(selector(format!("flow-handle-{}-0", node_id(from))))
            .unwrap();
        let target = cx
            .debug_bounds(selector(format!("flow-handle-{}-1", node_id(to))))
            .unwrap();
        let canvas = cx.debug_bounds("flow-graph").unwrap();
        assert!(
            canvas.contains(&source.center()) && canvas.contains(&target.center()),
            "the gesture must land within the mixer"
        );
        drag(cx, source.center(), target.center());
    }

    fn show(this: &mut AurisApp) {
        this.panels.set_detached(Panel::Mixer, false);
        this.panels.move_to(Panel::Mixer, Dock::Bottom);
        this.panels.set_size(Dock::Bottom, px(520.0));
        this.panels.show(Panel::Mixer);
    }

    #[gpui::test]
    fn pointer_connections_change_the_document_and_follow_undo(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let (a, b) = app.update(cx, |this, _| {
            show(this);
            (
                this.session.add_bus_track("A"),
                this.session.add_bus_track("B"),
            )
        });
        paint(&app, cx);
        expose_pair(
            &app,
            cx,
            MixerNode::Strip(Some(a)),
            MixerNode::Strip(Some(b)),
        );
        connect(cx, MixerNode::Strip(Some(a)), MixerNode::Strip(Some(b)));
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master, Output::Bus(b)]
            )
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            );
            assert_eq!(
                this.mixer_canvas.as_ref().unwrap().document,
                this.session.mixer_graph()
            );
        });
        click("mixer-show-console", cx);
        paint(&app, cx);
        assert!(cx.debug_bounds("mixer-show-graph").is_some());
        click("mixer-show-graph", cx);
        paint(&app, cx);
        assert!(cx.debug_bounds("flow-graph").is_some());
    }

    #[gpui::test]
    fn track_and_fader_controls_share_one_graph_node(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let track = app.update(cx, |this, _| {
            show(this);
            this.session.add_bus_track("Bus")
        });
        paint(&app, cx);
        assert!(
            cx.debug_bounds(selector(format!(
                "graph-gain-{}",
                node_id(MixerNode::Strip(Some(track)))
            )))
            .is_some()
        );
        app.read_with(cx, |this, _| {
            let nodes = &this.mixer_canvas.as_ref().unwrap().document.nodes;
            assert_eq!(nodes.len(), this.project().tracks.len() + 1);
            assert_eq!(
                nodes
                    .iter()
                    .filter(|node| **node == MixerNode::Strip(Some(track)))
                    .count(),
                1,
                "a track and its fader have one shared node"
            );
            assert!(nodes.iter().all(|node| matches!(node, MixerNode::Strip(_))));
        });
    }

    #[gpui::test]
    fn selecting_a_wire_deletes_only_that_output_and_supports_undo_and_context_menu(
        cx: &mut TestAppContext,
    ) {
        let (app, cx) = open(cx);
        let (a, b) = app.update(cx, |this, _| {
            show(this);
            let a = this.session.add_bus_track("A");
            let b = this.session.add_bus_track("B");
            this.session.add_track_output(a, Output::Bus(b)).unwrap();
            (a, b)
        });
        paint(&app, cx);
        expose_pair(
            &app,
            cx,
            MixerNode::Strip(Some(a)),
            MixerNode::Strip(Some(b)),
        );
        let source = cx
            .debug_bounds(selector(format!(
                "flow-handle-{}-0",
                node_id(MixerNode::Strip(Some(a)))
            )))
            .unwrap()
            .center();
        let target = cx
            .debug_bounds(selector(format!(
                "flow-handle-{}-1",
                node_id(MixerNode::Strip(Some(b)))
            )))
            .unwrap()
            .center();
        let wire = gpui::point((source.x + target.x) / 2., (source.y + target.y) / 2.);
        cx.simulate_click(wire, gpui::Modifiers::none());
        paint(&app, cx);
        app.read_with(cx, |this, cx| {
            let state = this.mixer_canvas.as_ref().unwrap().state.read(cx);
            assert_eq!(state.edges.iter().filter(|edge| edge.selected).count(), 1);
        });
        cx.simulate_keystrokes("delete");
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            )
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master, Output::Bus(b)]
            )
        });
        cx.dispatch_action(crate::actions::Redo);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            )
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        crate::harness::right_press(cx, wire);
        paint(&app, cx);
        assert!(cx.debug_bounds("context-menu-panel").is_some());
        click("menu-item-0", cx);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            )
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        cx.simulate_click(wire, gpui::Modifiers::none());
        cx.simulate_keystrokes("backspace");
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            )
        });
    }

    #[gpui::test]
    fn pointer_feedback_is_rejected_without_a_speculative_edge(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let (a, b) = app.update(cx, |this, _| {
            show(this);
            let a = this.session.add_bus_track("A");
            let b = this.session.add_bus_track("B");
            this.session.set_track_output(a, Output::Bus(b)).unwrap();
            (a, b)
        });
        paint(&app, cx);
        let before = app.read_with(cx, |this, _| this.session.mixer_graph());
        expose_pair(
            &app,
            cx,
            MixerNode::Strip(Some(b)),
            MixerNode::Strip(Some(a)),
        );
        connect(cx, MixerNode::Strip(Some(b)), MixerNode::Strip(Some(a)));
        paint(&app, cx);
        app.read_with(cx, |this, cx| {
            assert_eq!(this.session.mixer_graph(), before);
            let canvas = this.mixer_canvas.as_ref().unwrap();
            assert_eq!(canvas.state.read(cx).edges.len(), before.edges.len());
        });
    }

    #[gpui::test]
    fn modifier_selection_deletes_two_outputs_in_one_undo_step(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let (a, b, c) = app.update(cx, |this, _| {
            show(this);
            let a = this.session.add_bus_track("A");
            let b = this.session.add_bus_track("B");
            let c = this.session.add_bus_track("C");
            this.session.add_track_output(a, Output::Bus(b)).unwrap();
            this.session.add_track_output(a, Output::Bus(c)).unwrap();
            (a, b, c)
        });
        paint(&app, cx);
        expose_pair(
            &app,
            cx,
            MixerNode::Strip(Some(a)),
            MixerNode::Strip(Some(b)),
        );
        app.update(cx, |this, cx| {
            let canvas = this.mixer_canvas.as_ref().unwrap();
            canvas.state.update(cx, |state, _| {
                state
                    .get_node_mut(&node_id(MixerNode::Strip(Some(c))))
                    .unwrap()
                    .position = FlowPoint::new(380., 240.);
            });
            canvas.view.update(cx, |_, cx| cx.notify());
        });
        paint(&app, cx);
        let from = cx
            .debug_bounds(selector(format!(
                "flow-handle-{}-0",
                node_id(MixerNode::Strip(Some(a)))
            )))
            .unwrap()
            .center();
        for (index, target) in [b, c].into_iter().enumerate() {
            let to = cx
                .debug_bounds(selector(format!(
                    "flow-handle-{}-1",
                    node_id(MixerNode::Strip(Some(target)))
                )))
                .unwrap()
                .center();
            cx.simulate_click(
                gpui::point((from.x + to.x) / 2., (from.y + to.y) / 2.),
                if index == 0 {
                    gpui::Modifiers::none()
                } else {
                    gpui::Modifiers::secondary_key()
                },
            );
            paint(&app, cx);
        }
        app.read_with(cx, |this, cx| {
            assert_eq!(
                this.mixer_canvas
                    .as_ref()
                    .unwrap()
                    .state
                    .read(cx)
                    .edges
                    .iter()
                    .filter(|edge| edge.selected)
                    .count(),
                2
            )
        });
        cx.simulate_keystrokes("delete");
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master]
            )
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.project().track(a).unwrap().outputs,
                vec![Output::Master, Output::Bus(b), Output::Bus(c)]
            )
        });
    }

    #[gpui::test]
    fn send_mode_adds_a_branch_and_effect_nodes_control_the_real_insert(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let (a, b, slot) = app.update(cx, |this, _| {
            show(this);
            let a = this.session.add_bus_track("A");
            let b = this.session.add_bus_track("B");
            let slot = this.session.add_effect(Some(a), "auris.fx.reverb").unwrap();
            (a, b, slot)
        });
        paint(&app, cx);
        click("mixer-connect-send", cx);
        paint(&app, cx);
        expose_pair(
            &app,
            cx,
            MixerNode::Effect(Some(a), slot),
            MixerNode::Strip(Some(b)),
        );
        connect(
            cx,
            MixerNode::Effect(Some(a), slot),
            MixerNode::Strip(Some(b)),
        );
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            let track = this.project().track(a).unwrap();
            assert_eq!(track.outputs, vec![Output::Master]);
            assert_eq!(track.sends[0].target, b);
        });
        expose_pair(
            &app,
            cx,
            MixerNode::Effect(Some(a), slot),
            MixerNode::Strip(Some(b)),
        );
        click(
            selector(format!(
                "graph-effect-enabled-{}",
                node_id(MixerNode::Effect(Some(a), slot))
            )),
            cx,
        );
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(this.session.effect_enabled(Some(a), slot), Some(false))
        });
    }

    #[test]
    fn automatic_layout_points_every_dependency_forward() {
        let mut session =
            auris_session::Session::new(auris_session::SessionOptions::headless()).unwrap();
        let a = session.add_bus_track("A");
        let b = session.add_bus_track("B");
        let slot = session.add_effect(Some(b), "auris.fx.compressor").unwrap();
        session
            .set_effect_sidechain(Some(b), slot, Some(a))
            .unwrap();
        let graph = session.mixer_graph();
        let layout = positions(&graph);
        assert!(
            graph
                .edges
                .iter()
                .all(|edge| layout[&edge.source].x < layout[&edge.target].x)
        );
    }
}
