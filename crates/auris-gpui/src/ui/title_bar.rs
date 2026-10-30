//! Project identity and playback controls in the themed window title bar.

use auris_i18n::Key;
use gpui::{Context, IntoElement, Pixels, Window, div, prelude::*, px};

use crate::app::AurisApp;
use crate::dock::Dock;
use crate::titlebar;
use crate::ui::icons::Icon;
use crate::ui::widgets::icon_button_enabled;

/// Whether there is room beside playback and the native controls for the panel switches.
///
/// Smaller windows keep every switch in the status bar. The project name truncates first;
/// playback and window controls always retain their full hit targets.
pub(crate) fn panels_in_titlebar(width: Pixels) -> bool {
    width >= px(1000.0)
}

impl AurisApp {
    /// Draws the title bar, with playback above its readouts and panel switches to the right.
    pub(crate) fn render_title_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = &self.theme;
        let panels = panels_in_titlebar(window.viewport_size().width);
        let controls = titlebar::controls(
            window,
            theme,
            cx.listener(|this, _, window, cx| {
                this.close_project_window(window, cx);
                cx.notify();
            }),
        );

        let mut trailing = div()
            .flex()
            .items_center()
            .gap(theme.atelier.spacing.xs)
            .child(
                div()
                    .debug_selector(|| "titlebar-transport".into())
                    .flex()
                    .items_center()
                    .child(self.render_transport_buttons(cx)),
            );
        if panels {
            trailing = trailing.children(
                Dock::ALL
                    .into_iter()
                    .map(|dock| div().ml_1().child(self.dock_switches(dock, px(24.0), cx))),
            );
        }

        titlebar::bar(
            window,
            theme,
            titlebar::Parts::new("project-title", self.document_title())
                .leading(self.render_history_buttons(cx)),
            controls,
        )
        .trailing(trailing)
    }
}

impl AurisApp {
    /// Undo and Redo at the leading edge of the title bar, where Back sits on the other screens.
    ///
    /// They are the commands people reach for most often after playback, and the menu bar is a
    /// long way from the canvas. Each is disabled while there is nothing to step over, and its
    /// tooltip carries the key so the button teaches the shortcut.
    fn render_history_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            .debug_selector(|| "titlebar-history".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(theme.atelier.spacing.xxs)
            .pl(theme.atelier.spacing.sm)
            .child(step(
                "titlebar-undo",
                Icon::Undo,
                self.session.can_undo(),
                Key::CmdUndo,
                "edit.undo",
            ))
            .child(step(
                "titlebar-redo",
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
    fn titlebar_controls_still_change_the_document_and_relocate_panels(cx: &mut TestAppContext) {
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
            let transport = cx.debug_bounds("titlebar-transport").unwrap();
            assert!(transport.right() <= px(width));
            if let Some(minimize) = cx.debug_bounds("window-minimize") {
                assert!(transport.right() <= minimize.left());
            }
            if super::panels_in_titlebar(px(width)) {
                // The agent is the last switch in the default right dock.
                // Atelier's `TitleBar` keeps `spacing.sm` before caption buttons and `spacing.md`
                // from a bare window edge (macOS, where the traffic lights lead instead).
                let rightmost = cx.debug_bounds("panel-switch-5").unwrap().right();
                let spacing = app.read_with(cx, |this, _| this.theme.atelier.spacing);
                let edge = cx
                    .debug_bounds("window-minimize")
                    .map_or(px(width) - spacing.md, |bounds| bounds.left() - spacing.sm);
                assert_eq!(rightmost, edge);
            }
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
