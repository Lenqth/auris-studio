//! Startup project selection, before the editor claims realtime audio.

use auris_i18n::Key;
use gpui::{AnyElement, Context, IntoElement, KeyDownEvent, Window, div, prelude::*};

use super::icons::{Icon, icon};
use super::widgets::{ButtonState, ButtonStyle, Latch, button, button_enabled};
use crate::app::{AurisApp, Pane};

#[derive(Default)]
pub(crate) struct ProjectChooser {
    recent: bool,
    selected: usize,
    ai: bool,
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
        } else if chooser.ai {
            self.open_song_workshop(window, cx);
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
        let ai_selected = chooser.ai;
        let animate = self.appearance.animate_welcome;
        let busy = self.opening_project.is_some();
        let can_choose = !busy && (!recent || selected < self.settings.recent.len());
        let mut content = div()
            .id("chooser-content")
            .debug_selector(|| "chooser-content".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(theme.atelier.spacing.xl)
            .flex()
            .flex_col()
            .gap(theme.atelier.spacing.lg);
        let content_header = div()
            .flex()
            .flex_col()
            .gap_1()
            .pb_2()
            .child(div().text_xl().child(self.t(if recent {
                Key::ChooseRecent
            } else {
                Key::ChooseNew
            })))
            .when(!recent || self.settings.recent.is_empty(), |header| {
                header.child(div().text_sm().text_color(theme.text_muted).child(self.t(
                    if recent {
                        Key::ChooseRecentEmpty
                    } else {
                        Key::ChooseStartHelp
                    },
                )))
            });
        content = content.child(content_header);
        if recent {
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
                    .rounded(theme.atelier.shape.radius_md)
                    .bg(if selected == index {
                        theme.accent_soft
                    } else {
                        theme.surface
                    })
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
            let mut tiles = div().flex().flex_wrap().gap(theme.atelier.spacing.lg);
            for (id, key, ai) in [
                ("chooser-empty", Key::ChooseEmpty, false),
                ("chooser-ai", Key::WorkshopTitle, true),
            ] {
                tiles = tiles.child(
                    super::widgets::button_with_content(
                        id,
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .items_start()
                            .gap(theme.atelier.spacing.md)
                            .child(
                                div()
                                    .size(theme.atelier.spacing.xxl + theme.atelier.spacing.lg)
                                    .flex_shrink_0()
                                    .rounded(theme.atelier.shape.radius_lg)
                                    .bg(if ai {
                                        theme.atelier.colors.agent_wash
                                    } else {
                                        theme.accent_soft
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(
                                        if ai { Icon::Sparkles } else { Icon::Notes },
                                        theme.atelier.spacing.xl,
                                        if ai {
                                            theme.atelier.colors.agent
                                        } else {
                                            theme.accent
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(theme.atelier.typography.size_lg)
                                    .font_weight(theme.atelier.typography.heading_weight)
                                    .child(self.t(key)),
                            )
                            .child(
                                div()
                                    .text_size(theme.atelier.typography.size_sm)
                                    .text_color(theme.text_muted)
                                    .child(self.t(if ai {
                                        Key::ChooseAiHelp
                                    } else {
                                        Key::ChooseEmptyHelp
                                    })),
                            ),
                        ButtonStyle::Normal,
                        ButtonState::available(ai == ai_selected, !busy),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if let Some(chooser) = &mut this.project_chooser {
                                chooser.ai = ai;
                            }
                            if event.click_count() == 2 {
                                this.choose_project(window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .w(theme.atelier.spacing.xxl * 10.0)
                    .max_w_full()
                    .h_auto()
                    .min_h(theme.atelier.spacing.xxl * 5.0)
                    .p(theme.atelier.spacing.lg)
                    .flex_shrink_0()
                    .rounded(theme.atelier.shape.radius_lg)
                    .bg(theme.surface_raised)
                    .border_color(if ai == ai_selected {
                        theme.accent
                    } else {
                        theme.border_subtle
                    })
                    .text_base()
                    .flex_col()
                    .gap_4(),
                );
            }
            content = content.child(tiles);
        }

        let sidebar = div()
            .w(if window.viewport_size().width < gpui::px(900.0) {
                gpui::px(176.0)
            } else {
                gpui::px(232.0)
            })
            .flex_shrink_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .border_r_1()
            .border_color(theme.border_subtle)
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
            )
            .child(div().flex_1())
            .child(
                button_enabled(
                    "chooser-setup",
                    self.t(Key::SetupGuide),
                    ButtonStyle::Ghost,
                    ButtonState::available(false, !busy),
                    theme.accent,
                    &theme,
                    cx.listener(|this, _, _, cx| this.open_setup_guide(cx)),
                )
                .w_full()
                .h_auto()
                .min_h(theme.atelier.density.control_md)
                .whitespace_normal(),
            );
        let footer = div()
            .p_4()
            .border_t_1()
            .border_color(theme.border_subtle)
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
                .px_4()
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
                .px_4()
                .text_sm(),
            )
            .child(
                button_enabled(
                    "chooser-confirm",
                    self.t(if recent {
                        Key::ChooseOpen
                    } else if ai_selected {
                        Key::ChooseStartAi
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
                .px_4()
                .text_sm(),
            );
        div()
            .id("project-chooser")
            .debug_selector(|| "project-chooser".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.atelier.window_fill())
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
                    .child(button(
                        "chooser-motion",
                        self.t(if animate {
                            Key::ChoosePauseBackground
                        } else {
                            Key::ChooseAnimateBackground
                        }),
                        ButtonStyle::Ghost,
                        false,
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.appearance.animate_welcome = !this.appearance.animate_welcome;
                            if let Err(error) = this.appearance.save() {
                                log::warn!("could not save welcome motion preference: {error}");
                            }
                            cx.notify();
                        }),
                    ))
                    .child(crate::titlebar::controls(
                        window,
                        &theme,
                        cx.listener(|_, _, window, _| window.remove_window()),
                    )),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .p(theme.atelier.spacing.xl)
                    // Ambient owns its GPU cache and animation lifetime. No backdrop is mounted
                    // in the editor or workshop; inactive welcome windows hold a still frame.
                    .when(!cfg!(test), |view| {
                        view.child(
                            div().absolute().inset_0().child(
                                atelier::AmbientBackdrop::new(
                                    "chooser-aurora",
                                    atelier::Ambience::Aurora,
                                )
                                .when(!animate || !window.is_window_active(), |backdrop| {
                                    backdrop.phase(0.12)
                                }),
                            ),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .flex()
                            .size_full()
                            .max_w(gpui::px(1040.0))
                            .max_h(gpui::px(560.0))
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded(theme.atelier.shape.radius_xl)
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .shadow(theme.atelier.elevation.high.clone())
                            .child(sidebar)
                            .child(
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
                                                .when(self.status_failed, |view| {
                                                    view.text_color(theme.danger)
                                                })
                                                .child(self.status.clone()),
                                        )
                                    })
                                    .child(footer),
                            ),
                    ),
            )
            .children(crate::titlebar::resize_handles(window))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn choosing_a_start_card_waits_for_the_explicit_commit(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |app, cx| app.show_project_chooser(window, cx));
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("chooser-ai", cx);
        app.read_with(cx, |app, _| {
            assert!(app.project_chooser.as_ref().unwrap().ai);
            assert!(app.song_workshop.is_none());
        });
        crate::harness::click("chooser-empty", cx);
        crate::harness::paint(&app, cx);
        cx.simulate_keystrokes("enter");
        app.read_with(cx, |app, _| {
            assert!(app.project_chooser.is_none());
            assert!(app.song_workshop.is_none());
        });
    }

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
