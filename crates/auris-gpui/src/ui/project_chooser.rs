//! Startup project selection, before the editor claims realtime audio.

use auris_i18n::Key;
use gpui::{AnyElement, Context, IntoElement, KeyDownEvent, Window, div, prelude::*};

use super::widgets::{ButtonState, ButtonStyle, Latch, button, button_enabled};
use crate::app::{AurisApp, Pane};

#[derive(Default)]
pub(crate) struct ProjectChooser {
    recent: bool,
    selected: usize,
}

impl AurisApp {
    pub(crate) fn show_project_chooser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_chooser = Some(ProjectChooser::default());
        self.set_status("");
        self.focus.focus(window);
        cx.notify();
    }

    fn choose_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening_project.is_some() {
            return;
        }
        let Some(chooser) = &self.project_chooser else {
            return;
        };
        if chooser.recent {
            if let Some(path) = self.settings.recent.get(chooser.selected).cloned() {
                self.open_project_window(path, cx);
            }
        } else {
            self.project_chooser = None;
            self.focus_pane(Pane::Arrangement, window);
        }
        cx.notify();
    }

    fn chooser_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chooser) = self.project_chooser.as_mut() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "left" => chooser.recent = false,
            "right" => chooser.recent = true,
            "up" => chooser.selected = chooser.selected.saturating_sub(1),
            "down" => {
                chooser.selected =
                    (chooser.selected + 1).min(self.settings.recent.len().saturating_sub(1))
            }
            "enter" => self.choose_project(window, cx),
            "escape" if self.opening_project.is_none() => window.remove_window(),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn render_project_chooser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.song_workshop.is_some() {
            return self.render_song_workshop(window, cx);
        }
        let theme = self.theme.clone();
        let chooser = self.project_chooser.as_ref().expect("chooser is visible");
        let recent = chooser.recent;
        let selected = chooser.selected;
        let busy = self.opening_project.is_some();
        let can_choose = !busy && (!recent || selected < self.settings.recent.len());
        let mut content = div()
            .id("chooser-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_4();
        if recent {
            if self.settings.recent.is_empty() {
                content = content.child(
                    div()
                        .text_color(theme.text_muted)
                        .child(self.t(Key::ChooseRecentEmpty)),
                );
            }
            for (index, path) in self.settings.recent.iter().enumerate() {
                let title = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                content = content.child(
                    button(
                        ("chooser-recent", index),
                        title,
                        ButtonStyle::Normal,
                        selected == index,
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if let Some(chooser) = &mut this.project_chooser {
                                chooser.selected = index;
                            }
                            if event.click_count() == 2 {
                                this.choose_project(window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .h_auto()
                    .text_sm()
                    .w_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .py_3()
                    .items_start()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .w_full()
                            .overflow_hidden()
                            .text_color(theme.text_muted)
                            .child(path.display().to_string()),
                    ),
                );
            }
        } else {
            content = content
                .child(
                    button(
                        "chooser-ai",
                        self.t(Key::WorkshopTitle),
                        ButtonStyle::Normal,
                        false,
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, window, cx| this.open_song_workshop(window, cx)),
                    )
                    .h_12()
                    .text_base(),
                )
                .child(
                    button(
                        "chooser-empty",
                        self.t(Key::ChooseEmpty),
                        ButtonStyle::Normal,
                        false,
                        theme.accent,
                        &theme,
                        cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.choose_project(window, cx);
                            }
                        }),
                    )
                    .w_64()
                    .text_base()
                    .max_w_full()
                    .h_auto()
                    .py_6()
                    .flex_col()
                    .gap_4()
                    .border_color(theme.accent)
                    .child(
                        div()
                            .w_40()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().h_3().w_full().rounded_sm().bg(theme.accent_soft))
                            .child(div().h_3().w_24().rounded_sm().bg(theme.accent_soft))
                            .child(div().h_3().w_32().rounded_sm().bg(theme.accent_soft)),
                    ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.text_muted)
                        .child(self.t(Key::ChooseEmptyHelp)),
                );
        }
        let sidebar = div()
            .w_56()
            .flex_shrink_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .child(
                button(
                    "chooser-new",
                    self.t(Key::ChooseNew),
                    ButtonStyle::Ghost,
                    !recent,
                    theme.accent_soft,
                    &theme,
                    cx.listener(|this, _, _, cx| {
                        if let Some(chooser) = &mut this.project_chooser {
                            chooser.recent = false;
                        }
                        cx.notify();
                    }),
                )
                .h_10()
                .text_sm()
                .justify_start(),
            )
            .child(
                button(
                    "chooser-recents",
                    self.t(Key::ChooseRecent),
                    ButtonStyle::Ghost,
                    recent,
                    theme.accent_soft,
                    &theme,
                    cx.listener(|this, _, _, cx| {
                        if let Some(chooser) = &mut this.project_chooser {
                            chooser.recent = true;
                        }
                        cx.notify();
                    }),
                )
                .h_10()
                .text_sm()
                .justify_start(),
            );
        let footer = div()
            .p_4()
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                button_enabled(
                    "chooser-browse",
                    self.t(Key::ChooseExisting),
                    ButtonStyle::Normal,
                    if busy {
                        ButtonState::Disabled
                    } else {
                        ButtonState::Enabled(Latch::Off)
                    },
                    theme.accent,
                    &theme,
                    cx.listener(|this, _, _, cx| this.pick_and_open_project(cx)),
                )
                .h_9()
                .text_sm(),
            )
            .child(div().flex_1())
            .child(
                button_enabled(
                    "chooser-close",
                    self.t(Key::ChooseClose),
                    ButtonStyle::Normal,
                    if busy {
                        ButtonState::Disabled
                    } else {
                        ButtonState::Enabled(Latch::Off)
                    },
                    theme.accent,
                    &theme,
                    cx.listener(|_, _, window, _| window.remove_window()),
                )
                .h_9()
                .text_sm(),
            )
            .child(
                button_enabled(
                    "chooser-confirm",
                    self.t(if recent {
                        Key::ChooseOpen
                    } else {
                        Key::ChooseCreate
                    }),
                    ButtonStyle::Primary,
                    if can_choose {
                        ButtonState::Enabled(Latch::Off)
                    } else {
                        ButtonState::Disabled
                    },
                    theme.accent,
                    &theme,
                    cx.listener(|this, _, window, cx| this.choose_project(window, cx)),
                )
                .h_9()
                .text_sm(),
            );
        div()
            .id("project-chooser")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font(theme.font.clone())
            .text_sm()
            .track_focus(&self.focus)
            .key_context(crate::actions::KEY_CONTEXT)
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusNextPane, window, _| window.focus_next()),
            )
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusPreviousPane, window, _| {
                    window.focus_prev()
                }),
            )
            .on_key_down(cx.listener(Self::chooser_key))
            .on_action(cx.listener(|this, _: &crate::actions::OpenProject, _, cx| {
                if this.opening_project.is_none() {
                    this.pick_and_open_project(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::actions::Quit, window, cx| {
                this.quit_asking(window, cx)
            }))
            .child(
                crate::titlebar::titlebar(window, &theme)
                    .child(
                        crate::titlebar::drag_region("chooser-title")
                            .flex_1()
                            .justify_center()
                            .child(self.t(Key::ChooseProject)),
                    )
                    .child(crate::titlebar::controls(
                        window,
                        &theme,
                        cx.listener(|_, _, window, _| window.remove_window()),
                    )),
            )
            .child(
                div().flex().flex_1().min_h_0().child(sidebar).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(content)
                        .when(!self.status.is_empty(), |view| {
                            view.child(
                                div()
                                    .px_6()
                                    .py_3()
                                    .text_color(theme.text_muted)
                                    .when(self.status_failed, |view| view.text_color(theme.danger))
                                    .child(self.status.clone()),
                            )
                        })
                        .child(footer),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn chooser_creates_in_place_and_keeps_audio_suspended(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.session.set_audio_suspended(true).unwrap();
                app.show_project_chooser(window, cx);
            })
        });
        crate::harness::paint(&app, cx);
        cx.simulate_keystrokes("enter");
        app.read_with(cx, |app, _| {
            assert!(app.project_chooser.is_none());
            assert!(app.session.audio_suspended());
            assert!(!app.session.is_dirty());
        });
    }

    #[gpui::test]
    fn empty_recents_keep_the_chooser_open(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.settings.recent.clear();
                app.show_project_chooser(window, cx);
            })
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("chooser-recents", cx);
        crate::harness::paint(&app, cx);
        crate::harness::click("chooser-confirm", cx);
        app.read_with(cx, |app, _| assert!(app.project_chooser.is_some()));
    }
}
