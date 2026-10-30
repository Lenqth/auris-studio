//! Compact window chrome; editing and playback live in the control bar below.

use auris_i18n::Key;
use gpui::{Context, IntoElement, Window, div, prelude::*};

use crate::app::AurisApp;
use crate::titlebar;
use crate::ui::icons::Icon;
use crate::ui::widgets::{icon_button, icon_button_enabled};

impl AurisApp {
    /// Draws project identity, the application menu and native window controls.
    pub(crate) fn render_title_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = &self.theme;
        let controls = titlebar::controls(
            window,
            theme,
            cx.listener(|this, _, window, cx| {
                this.close_project_window(window, cx);
                cx.notify();
            }),
        );

        titlebar::bar(
            window,
            theme,
            titlebar::Parts::new("project-title", self.document_title()).leading(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.render_application_menu_button(cx))
                    .child(
                        atelier::Button::icon_only("editor-start", atelier::IconName::ChevronLeft)
                            .ghost()
                            .size(atelier::ControlSize::Small)
                            .tooltip(self.t(Key::BackToStart))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_editor_start(window, cx)
                            })),
                    ),
            ),
            controls,
        )
    }

    /// A single entry point to all application commands on Windows and Linux.
    pub(crate) fn render_application_menu_button(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div().when(Self::wants_menu_bar(), |slot| {
            slot.child(
                icon_button(
                    "application-menu",
                    Icon::Menu,
                    self.menu_bar.is_some(),
                    self.theme.accent,
                    &self.theme,
                    cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::OpenMenuBar), cx);
                    }),
                )
                .tooltip(self.tip(Key::CmdOpenMenuBar, "view.menu_bar")),
            )
        })
    }
}

impl AurisApp {
    /// Undo and Redo beside the timeline editing controls.
    pub(crate) fn render_history_buttons(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = &self.theme;
        let step = |id: &'static str, glyph: Icon, enabled: bool, label, command| {
            icon_button_enabled(
                id,
                glyph,
                false,
                enabled,
                theme.accent,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if glyph == Icon::Undo {
                        this.undo();
                    } else {
                        this.redo();
                    }
                    cx.notify();
                }),
            )
            .size(theme.atelier.density.control_sm)
            .tooltip(self.tip(label, command))
        };
        div()
            .debug_selector(|| "editing-history".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(theme.atelier.spacing.xxs)
            .child(step(
                "editing-undo",
                Icon::Undo,
                self.session.can_undo(),
                Key::CmdUndo,
                "edit.undo",
            ))
            .child(step(
                "editing-redo",
                Icon::Redo,
                self.session.can_redo(),
                Key::CmdRedo,
                "edit.redo",
            ))
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, px, size};

    use crate::dock::Panel;
    use crate::harness::{click, open, paint, resize, with_a_clip};

    #[gpui::test]
    fn start_button_keeps_unsaved_work_and_escape_resumes_it(cx: &mut TestAppContext) {
        let (app, cx, _, clip) = with_a_clip(cx);
        let before = app.read_with(cx, |this, _| this.project().clone());
        click("editor-start", cx);
        paint(&app, cx);
        assert!(cx.debug_bounds("project-chooser").is_some());
        app.read_with(cx, |this, _| {
            assert_eq!(this.project(), &before);
            assert!(this.project().midi_clip(clip).is_some());
            assert!(this.session.is_dirty());
        });
        cx.simulate_keystrokes("escape");
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            assert!(this.project_chooser.is_none());
            assert_eq!(this.project(), &before);
        });
        click("editor-start", cx);
        paint(&app, cx);
        click("chooser-close", cx);
        app.read_with(cx, |this, _| assert!(this.project_chooser.is_none()));
    }

    #[gpui::test]
    fn creating_from_the_returned_start_screen_asks_before_replacing_unsaved_work(
        cx: &mut TestAppContext,
    ) {
        let (app, cx, _, _) = with_a_clip(cx);
        let before = app.read_with(cx, |this, _| this.project().clone());
        click("editor-start", cx);
        paint(&app, cx);
        click("chooser-confirm", cx);
        paint(&app, cx);
        assert!(cx.debug_bounds("prompt-cancel").is_some());
        click("prompt-cancel", cx);
        app.read_with(cx, |this, _| {
            assert!(this.project_chooser.is_none());
            assert_eq!(this.project(), &before);
        });
    }

    #[gpui::test]
    fn playback_and_panel_switches_stay_outside_the_titlebar(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        app.read_with(cx, |this, _| {
            assert_eq!(this.document_title(), "Untitled");
            assert!(this.window_title().contains("Auris Studio"));
        });
        for width in [1360.0, 640.0, 1100.0] {
            resize(&app, cx, size(px(width), px(700.0)));
            let title = cx.debug_bounds("project-title").unwrap();
            // Native title bars keep the title centred until side controls need the room; once
            // they do, Atelier shifts it away rather than letting it overlap a control.
            assert!(title.left() >= px(0.0) && title.right() <= px(width));
            let transport = cx.debug_bounds("transport-buttons").unwrap();
            assert!(transport.top() >= crate::titlebar::HEIGHT);
            let panels = cx.debug_bounds("panel-switch-5").unwrap();
            assert!(panels.top() > transport.bottom());
            assert!(panels.right() <= px(width));
            let looping = app.read_with(cx, |this, _| this.project().loop_enabled);
            click("loop", cx);
            app.read_with(cx, |this, _| {
                assert_eq!(this.project().loop_enabled, !looping)
            });

            let open = app.read_with(cx, |this, _| this.panels.is_open(Panel::Mixer));
            click("panel-switch-2", cx);
            app.read_with(cx, |this, _| {
                assert_eq!(this.panels.is_open(Panel::Mixer), !open)
            });
            paint(&app, cx);
        }
    }

    #[gpui::test]
    fn closing_from_the_titlebar_keeps_unsaved_work_until_answered(cx: &mut TestAppContext) {
        if cfg!(target_os = "macos") {
            return; // macOS draws its own traffic lights outside the headless view tree.
        }
        let (app, cx, _, clip) = with_a_clip(cx);
        click("window-close", cx);
        app.read_with(cx, |this, _| {
            assert!(this.prompt.is_some());
            assert!(this.session.midi_clip(clip).is_some());
            assert!(this.session.is_dirty());
        });
        click("prompt-cancel", cx);
        app.read_with(cx, |this, _| {
            assert!(this.prompt.is_none());
            assert!(this.session.midi_clip(clip).is_some());
        });
    }
}
