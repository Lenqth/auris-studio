//! The audible mixer as a directed acyclic graph, shared by every frontend.
//!
//! Nodes are stable document identities, not plugin instances. Internal edges describe the
//! insert order; editable edges use the same session commands as the console and the toolbox.

use auris_core::{EffectSlotId, Output, SendId, TrackId};

use crate::{Session, SessionError};

/// A point in the signal path. `None` identifies the master strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MixerNode {
    /// The source of a track, or the summing input of a bus or the master.
    Input(Option<TrackId>),
    /// An insert before its strip's fader.
    Effect(Option<TrackId>, EffectSlotId),
    /// Gain, pan, mute and solo, after the inserts.
    Fader(Option<TrackId>),
}

/// The operation represented by an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MixerEdgeKind {
    /// Consecutive points inside one strip.
    Chain,
    /// The strip's main output.
    Output,
    /// An auxiliary tap; its identity and level belong to the document.
    Send {
        /// Owner of the send.
        track: TrackId,
        /// Stable send identity.
        send: SendId,
        /// Tap gain in decibels.
        level_db: f32,
        /// Whether the tap precedes the fader.
        pre_fader: bool,
    },
    /// A key signal arriving at an effect's second input.
    Sidechain,
}

/// One directed signal connection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerEdge {
    /// Where the audio leaves.
    pub source: MixerNode,
    /// Where the audio arrives.
    pub target: MixerNode,
    /// The role of this connection.
    pub kind: MixerEdgeKind,
}

/// A document projection; reading it does not build plugins or touch the audio thread.
#[derive(Clone, Debug, PartialEq)]
pub struct MixerGraph {
    /// Track strips in document order, followed by the master strip.
    pub nodes: Vec<MixerNode>,
    /// All insert, output, send and sidechain connections.
    pub edges: Vec<MixerEdge>,
}

impl Session {
    /// Describes the complete mixer signal path, including bypassed inserts and silent sends.
    pub fn mixer_graph(&self) -> MixerGraph {
        let mut graph = MixerGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        for (owner, strip) in self
            .project()
            .tracks
            .iter()
            .map(|track| (Some(track.id), &track.mixer))
            .chain(std::iter::once((None, &self.project().master)))
        {
            let input = MixerNode::Input(owner);
            graph.nodes.push(input);
            let mut previous = input;
            for slot in &strip.effects {
                let node = MixerNode::Effect(owner, slot.id);
                graph.nodes.push(node);
                graph.edges.push(MixerEdge {
                    source: previous,
                    target: node,
                    kind: MixerEdgeKind::Chain,
                });
                if let Some(source) = slot.sidechain {
                    graph.edges.push(MixerEdge {
                        source: MixerNode::Fader(Some(source)),
                        target: node,
                        kind: MixerEdgeKind::Sidechain,
                    });
                }
                previous = node;
            }
            let fader = MixerNode::Fader(owner);
            graph.nodes.push(fader);
            graph.edges.push(MixerEdge {
                source: previous,
                target: fader,
                kind: MixerEdgeKind::Chain,
            });
            if let Some(track) = owner.and_then(|id| self.project().track(id)) {
                graph.edges.push(MixerEdge {
                    source: fader,
                    target: MixerNode::Input(track.output.bus()),
                    kind: MixerEdgeKind::Output,
                });
                for send in &track.sends {
                    graph.edges.push(MixerEdge {
                        source: if send.pre_fader { previous } else { fader },
                        target: MixerNode::Input(Some(send.target)),
                        kind: MixerEdgeKind::Send {
                            track: track.id,
                            send: send.id,
                            level_db: send.level_db,
                            pre_fader: send.pre_fader,
                        },
                    });
                }
            }
        }
        graph
    }

    /// Whether a graph gesture can connect a track's post-fader output to this node.
    ///
    /// An input receives an output, or a send when `send` is true. An effect receives a
    /// sidechain. Self connections, duplicates, stale identities and all cycles are rejected.
    pub fn can_connect_mixer(&mut self, source: TrackId, target: MixerNode, send: bool) -> bool {
        let Some(track) = self.project().track(source) else {
            return false;
        };
        match target {
            MixerNode::Input(owner) => {
                if let Some(bus) = owner
                    && (!self.project().track(bus).is_some_and(|t| t.kind.is_bus())
                        || self.project().routing_would_cycle(source, bus))
                {
                    return false;
                }
                if send {
                    owner.is_some_and(|bus| !track.sends.iter().any(|s| s.target == bus))
                } else {
                    track.output != owner.map_or(Output::Master, Output::Bus)
                }
            }
            MixerNode::Effect(owner, slot) => {
                self.effect_enabled(owner, slot).is_some()
                    && self.effect_sidechain(owner, slot) != Some(source)
                    && !self.project().sidechain_would_cycle(owner, source)
                    && self.effect_wants_sidechain(owner, slot)
            }
            MixerNode::Fader(_) => false,
        }
    }

    /// Applies a validated graph connection as one undoable session command.
    ///
    /// Returns `false` without changing the document or history when the gesture is invalid.
    /// Validation happens again here even when the frontend already validated a preview.
    pub fn connect_mixer(
        &mut self,
        source: TrackId,
        target: MixerNode,
        send: bool,
    ) -> Result<bool, SessionError> {
        if !self.can_connect_mixer(source, target, send) {
            return Ok(false);
        }
        match target {
            MixerNode::Input(Some(bus)) if send => {
                self.add_send(source, bus)?;
            }
            MixerNode::Input(owner) => {
                self.set_track_output(source, owner.map_or(Output::Master, Output::Bus))?
            }
            MixerNode::Effect(owner, slot) => {
                self.set_effect_sidechain(owner, slot, Some(source))?
            }
            MixerNode::Fader(_) => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionOptions;

    #[test]
    fn graph_connections_reject_feedback_through_outputs_sends_and_sidechains() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let a = session.add_bus_track("A");
        let b = session.add_bus_track("B");
        let c = session.add_bus_track("C");
        assert!(
            session
                .connect_mixer(a, MixerNode::Input(Some(b)), false)
                .unwrap()
        );
        assert!(
            session
                .connect_mixer(b, MixerNode::Input(Some(c)), true)
                .unwrap()
        );
        let before = session.mixer_graph();
        for send in [false, true] {
            assert!(
                !session
                    .connect_mixer(c, MixerNode::Input(Some(a)), send)
                    .unwrap()
            );
            assert!(
                !session
                    .connect_mixer(a, MixerNode::Input(Some(a)), send)
                    .unwrap()
            );
        }
        assert_eq!(session.mixer_graph(), before);
        session.undo();
        assert!(session.project().track(b).unwrap().sends.is_empty());
        session.redo();
        assert_eq!(session.mixer_graph(), before);
    }

    #[test]
    fn graph_places_pre_fader_taps_after_inserts_and_before_gain() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let track = session.add_bus_track("Source");
        let bus = session.add_bus_track("Return");
        let slot = session.add_effect(Some(track), "auris.fx.reverb").unwrap();
        let send = session.add_send(track, bus).unwrap();
        session.set_send_pre_fader(track, send, true).unwrap();
        let graph = session.mixer_graph();
        let edge = graph
            .edges
            .iter()
            .find(|e| matches!(e.kind, MixerEdgeKind::Send { .. }))
            .unwrap();
        assert_eq!(edge.source, MixerNode::Effect(Some(track), slot));
        assert_eq!(edge.target, MixerNode::Input(Some(bus)));
        session.set_send_pre_fader(track, send, false).unwrap();
        assert!(
            session
                .mixer_graph()
                .edges
                .iter()
                .any(|e| e.source == MixerNode::Fader(Some(track))
                    && matches!(e.kind, MixerEdgeKind::Send { .. }))
        );
    }

    #[test]
    fn graph_sidechains_participate_in_cycle_detection_and_undo() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let source = session.add_bus_track("Source");
        let listener = session.add_bus_track("Listener");
        let slot = session
            .add_effect(Some(listener), "auris.fx.compressor")
            .unwrap();
        let target = MixerNode::Effect(Some(listener), slot);
        let before = session.mixer_graph();
        assert!(session.connect_mixer(source, target, false).unwrap());
        assert!(
            !session
                .connect_mixer(listener, MixerNode::Input(Some(source)), false)
                .unwrap()
        );
        assert!(
            !session
                .connect_mixer(listener, MixerNode::Input(Some(source)), true)
                .unwrap()
        );
        assert!(!session.connect_mixer(listener, target, false).unwrap());
        assert!(
            session
                .mixer_graph()
                .edges
                .iter()
                .any(|edge| edge.source == MixerNode::Fader(Some(source))
                    && edge.target == target
                    && edge.kind == MixerEdgeKind::Sidechain)
        );
        session.undo();
        assert_eq!(session.mixer_graph(), before);
    }

    #[test]
    fn graph_refuses_stale_nodes_non_bus_inputs_and_duplicate_sends() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let track = session.add_audio_track("Audio");
        let bus = session.add_bus_track("Bus");
        assert!(
            !session
                .connect_mixer(bus, MixerNode::Input(Some(track)), false)
                .unwrap()
        );
        assert!(
            !session
                .connect_mixer(track, MixerNode::Input(Some(TrackId(u64::MAX))), false)
                .unwrap()
        );
        assert!(
            !session
                .connect_mixer(
                    track,
                    MixerNode::Effect(None, EffectSlotId(u64::MAX)),
                    false
                )
                .unwrap()
        );
        assert!(
            session
                .connect_mixer(track, MixerNode::Input(Some(bus)), true)
                .unwrap()
        );
        assert!(
            !session
                .connect_mixer(track, MixerNode::Input(Some(bus)), true)
                .unwrap()
        );
        assert_eq!(session.project().track(track).unwrap().sends.len(), 1);
    }

    #[test]
    fn graph_routing_and_insert_order_survive_save_and_open() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let source = session.add_audio_track("Source");
        let bus = session.add_bus_track("Return");
        let first = session
            .add_effect(Some(bus), "auris.fx.compressor")
            .unwrap();
        session.add_effect(Some(bus), "auris.fx.reverb").unwrap();
        session.move_effect(Some(bus), first, 1);
        session
            .connect_mixer(source, MixerNode::Input(Some(bus)), true)
            .unwrap();
        let master_slot = session.add_effect(None, "auris.fx.compressor").unwrap();
        session
            .set_effect_sidechain(None, master_slot, Some(source))
            .unwrap();
        let graph = session.mixer_graph();
        let saved = session
            .save_as(&directory.path().join("Graph.auris"))
            .unwrap();
        let mut reopened = Session::new(SessionOptions::headless()).unwrap();
        reopened.open(&saved.document).unwrap();
        assert_eq!(reopened.mixer_graph(), graph);
    }
}
