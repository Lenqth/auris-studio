use super::*;
use crate::dock::{Dock, Panel};
use crate::harness::{click, drag_to, open, paint, press, release};
use gpui::{Modifiers, TestAppContext, point};

#[gpui::test]
fn node_drag_is_one_undo_step_and_noop_gestures_preserve_redo(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    let track = app.update(cx, |this, _| {
        this.panels.set_detached(Panel::Mixer, false);
        this.panels.move_to(Panel::Mixer, Dock::Bottom);
        this.panels.set_size(Dock::Bottom, px(600.));
        this.panels.show(Panel::Mixer);
        let track = this.session.add_bus_track("Move");
        this.session.forget_history();
        track
    });
    paint(&app, cx);
    let selector = Box::leak(
        node_id(MixerNode::Strip(Some(track)))
            .to_string()
            .into_boxed_str(),
    );
    let at = cx.debug_bounds(selector).unwrap().origin + point(px(12.), px(10.));
    let before = app.read_with(cx, |this, cx| {
        this.mixer_canvas.as_ref().unwrap().presentation(cx)
    });
    cx.simulate_click(at, Modifiers::none());
    paint(&app, cx);
    app.read_with(cx, |this, _| {
        assert!(!this.session.can_undo());
        assert!(!this.session.is_dirty());
    });
    press(cx, at);
    for delta in [(20., 10.), (40., 25.), (70., 40.)] {
        drag_to(cx, at + point(px(delta.0), px(delta.1)));
        paint(&app, cx);
        app.read_with(cx, |this, _| assert!(!this.session.can_undo()));
    }
    release(cx, at + point(px(70.), px(40.)));
    paint(&app, cx);
    let moved = app.read_with(cx, |this, cx| {
        assert!(this.session.can_undo());
        let view = this.mixer_canvas.as_ref().unwrap().presentation(cx);
        assert_eq!(this.project().mixer_graph_view.nodes, view.nodes);
        let key = MixerNode::Strip(Some(track)).key();
        assert_eq!(view.nodes[&key].x, before.nodes[&key].x + 70.);
        assert_eq!(view.nodes[&key].y, before.nodes[&key].y + 40.);
        view
    });
    cx.simulate_keystrokes("secondary-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        assert_eq!(
            this.mixer_canvas.as_ref().unwrap().presentation(cx).nodes,
            before.nodes
        );
        assert!(!this.session.can_undo());
        assert!(this.session.can_redo());
        assert!(!this.session.is_dirty());
    });
    // A click or movement that returns to its origin must not discard this redo branch.
    cx.simulate_click(at, Modifiers::none());
    press(cx, at);
    drag_to(cx, at + point(px(35.), px(15.)));
    drag_to(cx, at);
    release(cx, at);
    paint(&app, cx);
    app.read_with(cx, |this, _| {
        assert!(!this.session.can_undo());
        assert!(this.session.can_redo());
        assert!(!this.session.is_dirty());
    });
    click("mixer-zoom-out", cx);
    paint(&app, cx);
    cx.simulate_keystrokes("secondary-shift-z");
    paint(&app, cx);
    let zoom = app.read_with(cx, |this, cx| {
        let view = this.mixer_canvas.as_ref().unwrap().presentation(cx);
        assert_eq!(view.nodes, moved.nodes);
        assert!(view.zoom < 1.);
        view.zoom
    });
    cx.simulate_keystrokes("secondary-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        let view = this.mixer_canvas.as_ref().unwrap().presentation(cx);
        assert_eq!(view.nodes, before.nodes);
        assert_eq!(view.zoom, zoom);
        assert!(!this.session.can_undo());
    });
}

#[gpui::test]
fn arrange_button_restores_the_complete_layout_in_one_step_without_rewinding_the_camera(
    cx: &mut TestAppContext,
) {
    let (app, cx) = open(cx);
    app.update(cx, |this, _| {
        this.panels.set_detached(Panel::Mixer, false);
        this.panels.move_to(Panel::Mixer, Dock::Bottom);
        this.panels.show(Panel::Mixer);
        let track = this.session.add_bus_track("Arrange");
        this.session
            .add_effect(Some(track), "auris.fx.gain")
            .unwrap();
    });
    paint(&app, cx);
    app.update(cx, |this, cx| {
        let canvas = this.mixer_canvas.as_ref().unwrap();
        canvas.state.update(cx, |state, _| {
            for (index, node) in state.nodes.iter_mut().enumerate() {
                node.position =
                    FlowPoint::new(-300. + index as f32 * 600., 700. - index as f32 * 400.);
            }
            state.zoom_at(1.5, 0., 0.);
            state.viewport.x = -77.;
            state.viewport.y = 43.;
        });
        canvas.view.update(cx, |_, cx| cx.notify());
    });
    paint(&app, cx);
    let before = app.update(cx, |this, cx| {
        this.session.forget_history();
        this.mixer_canvas.as_ref().unwrap().presentation(cx)
    });
    click("mixer-layout", cx);
    paint(&app, cx);
    let arranged = app.read_with(cx, |this, cx| {
        let current = this.mixer_canvas.as_ref().unwrap().presentation(cx);
        assert_ne!(current.nodes, before.nodes);
        for (identity, position) in positions(&this.session.mixer_graph()) {
            assert_eq!(
                current.nodes[&identity.key()],
                MixerNodePosition {
                    x: position.x,
                    y: position.y
                }
            );
        }
        assert_eq!((current.zoom, current.pan_x, current.pan_y), (1., 0., 0.));
        current
    });
    cx.simulate_keystrokes("secondary-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        let current = this.mixer_canvas.as_ref().unwrap().presentation(cx);
        assert_eq!(current.nodes, before.nodes);
        assert_eq!((current.zoom, current.pan_x, current.pan_y), (1., 0., 0.));
        assert!(!this.session.can_undo());
    });
    cx.simulate_keystrokes("secondary-shift-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        assert_eq!(
            this.mixer_canvas.as_ref().unwrap().presentation(cx),
            arranged
        )
    });
    click("mixer-layout", cx);
    paint(&app, cx);
    cx.simulate_keystrokes("secondary-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        assert_eq!(
            this.mixer_canvas.as_ref().unwrap().presentation(cx).nodes,
            before.nodes
        );
        assert!(
            !this.session.can_undo(),
            "an already arranged layout adds no step"
        );
    });
}

#[gpui::test]
fn releasing_outside_the_canvas_finishes_the_node_drag(cx: &mut TestAppContext) {
    let (app, cx) = open(cx);
    app.update(cx, |this, _| {
        this.panels.set_detached(Panel::Mixer, false);
        this.panels.move_to(Panel::Mixer, Dock::Bottom);
        this.panels.set_size(Dock::Bottom, px(600.));
        this.panels.show(Panel::Mixer);
        this.session.forget_history();
    });
    paint(&app, cx);
    let selector = "mix-track-1";
    let at = cx.debug_bounds(selector).unwrap().origin + point(px(12.), px(10.));
    let before = app.read_with(cx, |this, cx| {
        this.mixer_canvas.as_ref().unwrap().presentation(cx)
    });
    press(cx, at);
    drag_to(cx, at + point(px(40.), px(30.)));
    release(cx, point(px(1.), px(1.)));
    paint(&app, cx);
    app.read_with(cx, |this, _| assert!(this.session.can_undo()));
    cx.simulate_keystrokes("secondary-z");
    paint(&app, cx);
    app.read_with(cx, |this, cx| {
        assert_eq!(
            this.mixer_canvas.as_ref().unwrap().presentation(cx).nodes,
            before.nodes
        );
        assert!(!this.session.can_undo());
    });
}
