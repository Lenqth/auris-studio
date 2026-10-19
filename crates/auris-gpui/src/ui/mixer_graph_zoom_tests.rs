use super::*;
use crate::dock::{Dock, Panel};
use crate::harness::{click, drag, open, paint};
use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, point};

#[test]
fn zoom_keeps_the_pointer_anchor_and_cached_node_bounds_consistent_at_both_limits() {
    let mut node = FlowNode::new("a", 24., 30.);
    node.measured_width = Some(px(176.));
    node.measured_height = Some(px(240.));
    let mut state = FlowState::new(vec![node], vec![]);
    state.min_zoom = MIN_ZOOM;
    state.max_zoom = MAX_ZOOM;
    state.viewport.x = -42.;
    state.viewport.y = 17.;
    let anchor = state.viewport.screen_to_flow(351., 197.);
    for requested in [1.4, 10., 0.01, 1.] {
        state.zoom_at(requested, 351., 197.);
        let actual = state.viewport.screen_to_flow(351., 197.);
        assert!((actual.x - anchor.x).abs() < 1e-4);
        assert!((actual.y - anchor.y).abs() < 1e-4);
        assert_eq!(state.viewport.zoom, requested.clamp(MIN_ZOOM, MAX_ZOOM));
        assert!(
            (f32::from(state.nodes[0].measured_width.unwrap()) - 176. * state.viewport.zoom).abs()
                < 1e-4
        );
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        state.zoom_at(invalid, 351., 197.);
        assert_eq!(state.viewport.zoom, 1.);
    }
}

#[gpui::test]
fn control_wheel_scales_nodes_handles_and_controls_and_pans_without_editing(
    cx: &mut TestAppContext,
) {
    let (app, cx) = open(cx);
    let track = app.update(cx, |this, _| {
        this.panels.set_detached(Panel::Mixer, false);
        this.panels.move_to(Panel::Mixer, Dock::Bottom);
        this.panels.set_size(Dock::Bottom, px(600.));
        this.panels.show(Panel::Mixer);
        this.session.add_bus_track("Zoom")
    });
    paint(&app, cx);
    let id = node_id(MixerNode::Strip(Some(track)));
    let node_selector = Box::leak(id.to_string().into_boxed_str());
    let gain_selector = Box::leak(format!("graph-gain-{id}").into_boxed_str());
    let handle_selector = Box::leak(format!("flow-handle-{id}-0").into_boxed_str());
    app.update(cx, |this, cx| {
        let canvas = this.mixer_canvas.as_ref().unwrap();
        canvas.state.update(cx, |state, _| {
            state.viewport.x = 0.;
            state.viewport.y = 0.;
            for node in &mut state.nodes {
                node.position = if node.id == id {
                    FlowPoint::new(180., 70.)
                } else {
                    FlowPoint::new(3000., 3000.)
                };
            }
        });
        canvas.view.update(cx, |_, cx| cx.notify());
    });
    paint(&app, cx);
    let node_before = cx.debug_bounds(node_selector).unwrap();
    let gain_before = cx.debug_bounds(gain_selector).unwrap();
    let handle_before = cx.debug_bounds(handle_selector).unwrap();
    let document = app.read_with(cx, |this, _| serde_json::to_value(this.project()).unwrap());
    let at = node_before.center();
    let before = app.read_with(cx, |this, cx| {
        let state = this.mixer_canvas.as_ref().unwrap().state.read(cx);
        state.viewport.screen_to_flow(
            f32::from(at.x) - state.origin.0,
            f32::from(at.y) - state.origin.1,
        )
    });
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(30.))),
        modifiers: Modifiers {
            control: true,
            ..Modifiers::none()
        },
        ..Default::default()
    });
    paint(&app, cx);
    let zoom = app.read_with(cx, |this, cx| {
        let state = this.mixer_canvas.as_ref().unwrap().state.read(cx);
        let after = state.viewport.screen_to_flow(
            f32::from(at.x) - state.origin.0,
            f32::from(at.y) - state.origin.1,
        );
        assert!((after.x - before.x).abs() < 1e-3 && (after.y - before.y).abs() < 1e-3);
        state.viewport.zoom
    });
    assert!(zoom > 1.);
    let node_after = cx.debug_bounds(node_selector).unwrap();
    let gain_after = cx.debug_bounds(gain_selector).unwrap();
    let handle_after = cx.debug_bounds(handle_selector).unwrap();
    // These are control hit regions, not raster assertions: interactions must grow with the card.
    assert!((node_after.size.width / node_before.size.width - zoom).abs() < 0.02);
    assert!((gain_after.size.height / gain_before.size.height - zoom).abs() < 0.02);
    assert!((handle_after.size.width / handle_before.size.width - zoom).abs() < 0.02);
    assert_eq!(
        app.read_with(cx, |this, _| serde_json::to_value(this.project()).unwrap()),
        document
    );
    // Ordinary wheel input pans, including over a fader, without changing the gain or zoom.
    cx.simulate_event(ScrollWheelEvent {
        position: gain_after.center(),
        delta: ScrollDelta::Pixels(point(px(12.), px(-24.))),
        ..Default::default()
    });
    paint(&app, cx);
    assert_eq!(
        app.read_with(cx, |this, cx| this
            .mixer_canvas
            .as_ref()
            .unwrap()
            .state
            .read(cx)
            .viewport
            .zoom),
        zoom
    );
    assert_eq!(
        app.read_with(cx, |this, _| serde_json::to_value(this.project()).unwrap()),
        document
    );
    // A scaled fader still edits and resets through the real pointer handlers.
    let gain = cx.debug_bounds(gain_selector).unwrap();
    drag(cx, gain.center(), gain.center() + point(px(30.), px(0.)));
    paint(&app, cx);
    assert_ne!(
        app.read_with(cx, |this, _| this
            .project()
            .track(track)
            .unwrap()
            .mixer
            .gain_db),
        0.
    );
    click("mixer-zoom-reset", cx);
    paint(&app, cx);
    assert_eq!(
        app.read_with(cx, |this, cx| this
            .mixer_canvas
            .as_ref()
            .unwrap()
            .state
            .read(cx)
            .viewport
            .zoom),
        1.
    );
    click("mixer-zoom-out", cx);
    paint(&app, cx);
    assert!(
        app.read_with(cx, |this, cx| this
            .mixer_canvas
            .as_ref()
            .unwrap()
            .state
            .read(cx)
            .viewport
            .zoom)
            < 1.
    );
    click("mixer-zoom-in", cx);
    paint(&app, cx);
    assert!(
        (app.read_with(cx, |this, cx| this
            .mixer_canvas
            .as_ref()
            .unwrap()
            .state
            .read(cx)
            .viewport
            .zoom)
            - 1.)
            .abs()
            < 1e-6
    );
}
