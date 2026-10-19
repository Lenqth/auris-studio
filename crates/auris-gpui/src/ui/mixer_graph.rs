//! A graph presentation of the session's mixer, with document-authoritative connections.

use std::collections::{HashMap, HashSet, VecDeque};

use auris_i18n::Key;
use auris_session::mixer_graph::{MixerEdgeKind, MixerGraph, MixerNode};
use auris_session::prelude::*;
use gpui::{AnyElement, Axis, Entity, IntoElement, MouseButton, SharedString, div, prelude::*, px};
use gpui_flow::{
    Connection, FlowEdge, FlowGraph, FlowNode, FlowPoint, FlowState, HandleDef, HandlePosition,
};

use crate::app::AurisApp;
use crate::theme::Metrics;
use crate::ui::plugin_window::PluginSubject;
use crate::ui::widgets::{ButtonStyle, button, db_to_meter_position, level_meter};

const NODE_WIDTH: f32 = 176.0;
const COLUMN: f32 = 240.0;
const ROW: f32 = 240.0;

/// Presentation state kept across paints and panel moves.
pub(crate) struct MixerCanvas {
    state: Entity<FlowState>,
    view: Entity<FlowGraph>,
    document: MixerGraph,
    by_id: HashMap<SharedString, MixerNode>,
    send: bool,
}

fn node_id(node: MixerNode) -> SharedString {
    match node {
        MixerNode::Input(owner) => format!("mix-input-{}", owner.map_or(0, |id| id.0)),
        MixerNode::Fader(owner) => format!("mix-fader-{}", owner.map_or(0, |id| id.0)),
        MixerNode::Effect(_, slot) => format!("mix-effect-{}", slot.0),
    }
    .into()
}

fn gesture(graph: &MixerGraph, connection: &Connection) -> Option<(TrackId, MixerNode)> {
    let source = graph.nodes.iter().find_map(|node| match node {
        MixerNode::Fader(Some(track)) if node_id(*node) == connection.source => Some(*track),
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
        if let MixerNode::Input(owner) = node {
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
                MixerNode::Input(t) | MixerNode::Effect(t, _) | MixerNode::Fader(t) => t,
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
                    .child(self.t(Key::MixerGraphHelp)),
            )
            .into_any_element()
    }

    fn apply_mixer_connections(&mut self, state: &Entity<FlowState>, cx: &mut gpui::Context<Self>) {
        let pending = state.update(cx, |state, _| {
            std::mem::take(&mut state.pending_connections)
        });
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

    fn sync_mixer_graph(&mut self, document: &MixerGraph, cx: &mut gpui::Context<Self>) {
        let changed = self.mixer_canvas.as_ref().is_some_and(|canvas| {
            canvas.document != *document || canvas.send != self.mixer_connect_send
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
                if let MixerNode::Fader(Some(source)) = node {
                    for target in &document.nodes {
                        if self
                            .session
                            .can_connect_mixer(*source, *target, self.mixer_connect_send)
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
                        MixerNode::Input(owner)
                            if owner.is_none_or(|t| {
                                self.session
                                    .project()
                                    .track(t)
                                    .is_some_and(|t| t.kind.is_bus())
                            }) =>
                        {
                            vec![HandleDef::target(HandlePosition::Left)]
                        }
                        MixerNode::Fader(Some(_)) => vec![HandleDef::source(HandlePosition::Right)],
                        MixerNode::Effect(_, slot) if sidechain_slots.contains(slot) => {
                            vec![HandleDef::target(HandlePosition::Top).id("sidechain")]
                        }
                        _ => Vec::new(),
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
                .enumerate()
                .map(|(i, edge)| {
                    let mut flow = FlowEdge::new(
                        format!("mix-edge-{i}"),
                        node_id(edge.source),
                        node_id(edge.target),
                    )
                    .color(u32::from(theme.text_muted.to_rgb()) >> 8);
                    flow.deletable = false;
                    match edge.kind {
                        MixerEdgeKind::Send {
                            level_db,
                            pre_fader,
                            ..
                        } => {
                            flow = flow.label(format!(
                                "{} {level_db:.1} dB{}",
                                self.t(Key::Sends),
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
                        _ => {}
                    }
                    flow
                })
                .collect();
            state.set_nodes(nodes);
            state.set_edges(edges);
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
            MixerNode::Input(t) | MixerNode::Effect(t, _) | MixerNode::Fader(t) => t,
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
            MixerNode::Input(_) => {
                card = card.child(self.t(Key::MixerInput)).child(button(
                    SharedString::from(format!("graph-add-fx-{id}")),
                    self.t(Key::MenuAddEffect),
                    ButtonStyle::Ghost,
                    false,
                    theme.accent_soft,
                    &theme,
                    Self::opens_menu(cx, move |this, at| this.effect_picker_menu(at, owner)),
                ));
            }
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
            MixerNode::Fader(_) => {
                let strip = owner
                    .and_then(|id| self.project().track(id))
                    .map_or(&self.project().master, |t| &t.mixer);
                let (gain, pan, muted, soloed) = (strip.gain_db, strip.pan, strip.mute, strip.solo);
                card = card
                    .child(self.t(Key::MixerFader))
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
            .debug_bounds(selector(format!("flow-handle-{}-0", node_id(to))))
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
            MixerNode::Fader(Some(a)),
            MixerNode::Input(Some(b)),
        );
        connect(cx, MixerNode::Fader(Some(a)), MixerNode::Input(Some(b)));
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(this.project().track(a).unwrap().output, Output::Bus(b))
        });
        cx.dispatch_action(crate::actions::Undo);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert_eq!(this.project().track(a).unwrap().output, Output::Master);
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
            MixerNode::Fader(Some(b)),
            MixerNode::Input(Some(a)),
        );
        connect(cx, MixerNode::Fader(Some(b)), MixerNode::Input(Some(a)));
        paint(&app, cx);
        app.read_with(cx, |this, cx| {
            assert_eq!(this.session.mixer_graph(), before);
            let canvas = this.mixer_canvas.as_ref().unwrap();
            assert_eq!(canvas.state.read(cx).edges.len(), before.edges.len());
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
            MixerNode::Fader(Some(a)),
            MixerNode::Input(Some(b)),
        );
        connect(cx, MixerNode::Fader(Some(a)), MixerNode::Input(Some(b)));
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            let track = this.project().track(a).unwrap();
            assert_eq!(track.output, Output::Master);
            assert_eq!(track.sends[0].target, b);
        });
        expose_pair(
            &app,
            cx,
            MixerNode::Effect(Some(a), slot),
            MixerNode::Input(Some(b)),
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
