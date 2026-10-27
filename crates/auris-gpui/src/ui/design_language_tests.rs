//! Behavioral layout checks for the Atelier shell composition.
//!
//! These tests intentionally inspect relationships between live controls rather than pixels or
//! colors. They catch the regressions most likely when the design language is applied: a fixed
//! side panel escaping its pane, a composer falling below the window, or a chooser losing its
//! primary actions at the minimum supported width.

#[cfg(test)]
mod tests {
    use gpui::{px, size};

    use crate::dock::Panel;
    use crate::harness::{open, paint, resize};

    #[gpui::test]
    fn agent_panel_keeps_transcript_and_composer_inside_the_panel_when_narrow(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = open(cx);
        app.update(cx, |this, _| this.toggle_panel(Panel::Agent));
        paint(&app, cx);

        resize(&app, cx, size(px(720.0), px(720.0)));
        let panel = cx
            .debug_bounds("agent-panel")
            .expect("the agent panel remains reachable at the narrow width");
        let composer = cx
            .debug_bounds("agent-composer")
            .expect("the composer remains visible while the transcript scrolls");
        assert!(composer.left() >= panel.left());
        assert!(composer.right() <= panel.right());
        assert!(composer.bottom() <= panel.bottom());
        assert!(cx.debug_bounds("agent-input").is_some());
    }

    #[gpui::test]
    fn project_chooser_keeps_primary_actions_and_recent_region_at_narrow_width(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = open(cx);
        cx.update(|window, cx| {
            app.update(cx, |this, cx| this.show_project_chooser(window, cx));
        });
        resize(&app, cx, size(px(720.0), px(600.0)));

        let chooser = cx.debug_bounds("project-chooser").unwrap();
        assert!(cx.debug_bounds("chooser-content").is_some());
        for action in [
            "chooser-new",
            "chooser-recents",
            "chooser-confirm",
            "chooser-close",
        ] {
            let bounds = cx
                .debug_bounds(action)
                .expect("the chooser action is rendered");
            assert!(bounds.left() >= chooser.left(), "{action}");
            assert!(bounds.right() <= chooser.right(), "{action}");
            assert!(bounds.top() >= chooser.top(), "{action}");
            assert!(bounds.bottom() <= chooser.bottom(), "{action}");
        }
    }
}
