//! One natural-language composer and provider controls.
use super::*;
use crate::ui::context_menu::{ContextMenu, MenuCommand};
impl AurisApp {
    pub(crate) fn workshop_chord_prompt(&self) -> bool {
        self.song_workshop.is_some()
            && self.prompt.as_ref().is_some_and(|p| {
                matches!(
                    p.target(),
                    Some(crate::ui::prompt::PromptTarget::AgentChordReference { .. })
                )
            })
    }

    pub(super) fn render_workshop_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = &self.song_workshop else {
            return div().into_any_element();
        };
        let theme = self.theme.clone();
        let model = s
            .models
            .iter()
            .find(|m| m.matches(&s.preferences))
            .map(|m| m.label())
            .unwrap_or_else(|| {
                if s.preferences.model.is_empty() {
                    self.t(Key::AgentChooseModel).to_string()
                } else {
                    format!("{} / {}", s.preferences.backend, s.preferences.model)
                }
            });
        let effort = self.t(crate::ui::agent_chat::controls::effort_key(
            s.preferences.effort,
        ));
        let focused = s.active_field == Some(0);
        let busy = s.busy();
        let can_send = !busy
            && !s.references.midi_loading
            && !s.brief.content().trim().is_empty()
            && s.preferences.is_configured();
        let model_error = s.model_error.clone();
        let attachment_error = s.references.attachment_error.clone();
        let attachments = s.references.attachments.clone();
        let attached_here = s.references.attached_here.clone();
        let action = if busy {
            agent_components::composer_action(
                "song-cancel",
                self.t(Key::WorkshopCancel),
                agent_components::ComposerAction::Stop,
                true,
                true,
                &theme,
                cx.listener(|this, _, _, cx| {
                    if let Some(s) = &mut this.song_workshop {
                        s.stop();
                    }
                    cx.notify();
                }),
            )
        } else {
            agent_components::composer_action(
                "song-generate",
                self.t(Key::WorkshopSend),
                agent_components::ComposerAction::Send,
                can_send,
                true,
                &theme,
                cx.listener(|this, _, _, cx| this.start_song_workshop(cx)),
            )
        };
        let mut body = crate::ui::agent_components::composer_frame(
            "song-workshop-composer",
            &theme,
            focused,
            model_error.is_some(),
        )
        .debug_selector(|| "song-workshop-composer".to_owned())
        .m_0()
        .child(self.midi_composer_controls(busy, cx))
        .child(div().flex().flex_wrap().min_w_0().gap_1().children(
            attachments.into_iter().enumerate().map(|(index, path)| {
                let reattach = !attached_here.contains(&path);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                div()
                    .flex()
                    .min_w_0()
                    .max_w_full()
                    .child(
                        crate::ui::widgets::bounded_button_enabled(
                            ("workshop-reference", index),
                            if reattach {
                                self.t(Key::AgentReattachReference).replace("{name}", &name)
                            } else {
                                name
                            },
                            crate::ui::widgets::ButtonStyle::Ghost,
                            crate::ui::widgets::ButtonState::available(false, !busy),
                            theme.accent,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                if reattach {
                                    this.music_references_mut()
                                        .attached_here
                                        .insert(path.clone());
                                    this.music_references_mut().attachment_error = None;
                                }
                                if auris_agent::attachment_kind(&path)
                                    == Some(auris_agent::AttachmentKind::Midi)
                                {
                                    this.edit_midi_attachment(path.clone(), cx);
                                } else if !reattach {
                                    this.open_agent_attachment(path.clone(), cx);
                                }
                                cx.notify();
                            }),
                        )
                        .flex_1()
                        .flex_shrink(),
                    )
                    .child(crate::ui::widgets::button_enabled(
                        ("workshop-reference-remove", index),
                        self.t(Key::MenuRemove),
                        crate::ui::widgets::ButtonStyle::Ghost,
                        crate::ui::widgets::ButtonState::available(false, !busy),
                        theme.accent,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && index < s.references.attachments.len()
                            {
                                let removed = s.references.attachments.remove(index);
                                this.cancel_midi_for_attachment(&removed);
                            }
                            cx.notify();
                        }),
                    ))
            }),
        ))
        .when(self.music_references().midi_draft.is_some(), |body| {
            body.child(self.midi_editor(window.viewport_size().height * 0.3, cx))
        })
        .when_some(attachment_error, |body, error| {
            body.child(agent_components::error_notice(error))
        })
        .child(self.workshop_field(0, Key::WorkshopBriefHint, true, cx))
        .child(
            agent_components::composer_controls("workshop-composer-controls", &theme)
                .child(
                    div()
                        .debug_selector(|| "workshop-model".into())
                        .min_w_0()
                        .max_w_full()
                        .overflow_hidden()
                        .child(
                            atelier::Button::new("workshop-model", model)
                                .ghost()
                                .size(atelier::ControlSize::Small)
                                .icon_after(atelier::IconName::ChevronDown)
                                .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                                    this.workshop_model_menu(event.position());
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div().debug_selector(|| "workshop-effort".into()).child(
                        atelier::Button::new(
                            "workshop-effort",
                            format!("{}: {effort}", self.t(Key::AgentEffort)),
                        )
                        .ghost()
                        .size(atelier::ControlSize::Small)
                        .icon(atelier::IconName::Brain)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop {
                                let levels = s
                                    .models
                                    .iter()
                                    .find(|m| m.matches(&s.preferences))
                                    .and_then(|m| m.efforts.clone())
                                    .unwrap_or_else(|| {
                                        vec![auris_session::ReasoningEffort::Default]
                                    });
                                if !levels.is_empty() {
                                    let index = levels
                                        .iter()
                                        .position(|e| *e == s.preferences.effort)
                                        .map_or(0, |i| (i + 1) % levels.len());
                                    s.preferences.effort = levels[index];
                                }
                            }
                            cx.notify();
                        })),
                    ),
                )
                .child(
                    div()
                        .debug_selector(|| "workshop-model-settings".into())
                        .child(
                            atelier::Button::icon_only(
                                "workshop-model-settings",
                                atelier::IconName::Settings,
                            )
                            .size(atelier::ControlSize::Small)
                            .tooltip(self.t(Key::AgentConfigure))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_settings_tab(
                                    crate::settings_window::SettingsTab::Agent,
                                    cx,
                                )
                            })),
                        ),
                )
                .child(div().flex_1())
                .child(action),
        );
        if let Some(error) = &model_error {
            body = body.child(crate::ui::agent_components::error_notice(error.clone()));
        }
        body.into_any_element()
    }

    pub(super) fn workshop_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workshop_chord_prompt() {
            if self.prompt_key(event, window, cx) {
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if self.music_lyric_focused() {
            let composing = self
                .music_references()
                .midi_draft
                .as_ref()
                .is_some_and(|d| d.lyric.marked().is_some());
            if composing {
                return;
            }
            match event.keystroke.key.as_str() {
                "escape" => self.close_midi_editor(window),
                "enter" => {
                    if self.commit_midi_draft() {
                        self.close_midi_editor(window);
                    }
                }
                _ => {
                    if let Some(draft) = &mut self.music_references_mut().midi_draft {
                        if draft.lyric.apply_key_with_clipboard(
                            &event.keystroke.key,
                            event.keystroke.modifiers.shift,
                            event.keystroke.modifiers.secondary(),
                            false,
                            cx,
                        ) == KeyEffect::Ignored
                        {
                            return;
                        }
                    } else {
                        return;
                    }
                }
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.menu_key(event, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if let Some(s) = &mut self.song_workshop
            && s.foci[0].is_focused(window)
        {
            if s.submitted || s.brief.marked().is_some() {
                return;
            }
            if event.keystroke.key == "enter" {
                if event.keystroke.modifiers.shift {
                    s.brief.insert("\n");
                } else {
                    self.start_song_workshop(cx);
                }
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if s.brief.apply_key_with_clipboard(
                &event.keystroke.key,
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.secondary(),
                true,
                cx,
            ) != KeyEffect::Ignored
            {
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        if event.keystroke.key == "escape" {
            if let Some(s) = &mut self.song_workshop {
                s.active_field = None;
            }
            self.focus.focus(window);
            cx.stop_propagation();
            cx.notify();
        }
    }
    pub(crate) fn choose_workshop_model(&mut self, index: usize) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if s.busy() {
            return;
        }
        if let Some(model) = s.models.get(index) {
            if let Some(route) = &model.connection {
                route.apply_to(&mut s.preferences);
            }
            s.preferences.model.clone_from(&model.name);
            if model
                .efforts
                .as_ref()
                .is_some_and(|levels| !levels.contains(&s.preferences.effort))
            {
                s.preferences.effort = Default::default();
            }
        }
    }

    fn workshop_model_menu(&mut self, at: gpui::Point<gpui::Pixels>) {
        let Some(s) = &self.song_workshop else {
            return;
        };
        let mut menu = ContextMenu::new(at, self.t(Key::AgentModelLabel));
        for (index, model) in s.models.iter().enumerate() {
            menu = menu.toggle(
                model.label(),
                MenuCommand::WorkshopModel(index),
                model.matches(&s.preferences),
            );
        }
        menu = menu.separator().item(
            self.t(Key::WorkshopRefreshModels),
            MenuCommand::WorkshopRefreshModels,
        );
        self.open_menu(menu);
    }

    pub(crate) fn refresh_workshop_models(&mut self) {
        if let Some(s) = &mut self.song_workshop
            && !s.busy()
        {
            // A settings window may have configured a provider since the wizard opened.
            if !s.preferences.is_configured() {
                s.preferences = self.settings.agent.clone();
            }
            s.model_receiver = Some(auris_agent::discover_models_background(
                s.preferences.clone(),
            ));
            s.model_error = None;
        }
    }

    fn workshop_field(
        &self,
        index: usize,
        placeholder: Key,
        multiline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.song_workshop.as_ref().expect("visible workshop");
        let text = s.text(index);
        let focus = s.foci[index].clone();
        let theme = self.theme.clone();
        let active = s.active_field == Some(index);
        let theme_faint = theme.text_faint;
        let mut field = div()
            .id(("workshop-field", index))
            .debug_selector(move || {
                if index == 0 {
                    "song-brief".into()
                } else {
                    format!("workshop-field-{index}")
                }
            })
            .track_focus(&focus)
            .relative()
            .flex_shrink_0()
            .text_size(theme.atelier.typography.size_md)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if let Some(s) = &mut this.song_workshop {
                        let already = s.active_field == Some(index);
                        s.active_field = Some(index);
                        let focus = s.foci[index].clone();
                        if multiline {
                            if already
                                && let Some(offset) = crate::ui::text_area::area_offset_at(
                                    window,
                                    s.text(index).content(),
                                    event.position,
                                )
                            {
                                s.text_mut(index).place_caret(offset, event.modifiers.shift);
                            } else {
                                s.text_mut(index).caret_to_end();
                            }
                        } else {
                            super::super::prompt::click_field(
                                s.text_mut(index),
                                &focus,
                                already,
                                window,
                                event,
                            );
                        }
                        focus.focus(window);
                        cx.notify();
                    }
                }),
            );
        if multiline {
            let height = crate::ui::text_area::area_height(text.content(), 4, 12);
            field = field.h(height).overflow_hidden();
            // Only the active area owns multiline hit-test geometry and platform input.
            if active {
                field = field.child(crate::ui::text_area::editable_area(
                    text.content().to_owned().into(),
                    text.selection(),
                    text.marked(),
                    vec![],
                    focus,
                    cx.entity(),
                    theme.clone(),
                ));
            } else {
                field = field.child(
                    div()
                        .px_2()
                        .py(gpui::px(5.0))
                        .line_height(crate::ui::text_area::AREA_LINE_HEIGHT)
                        .child(text.content().to_owned()),
                );
            }
        } else if active {
            field = field.h_10().child(super::super::prompt::editable_text(
                text.content().to_owned().into(),
                text.selection(),
                text.marked(),
                focus,
                cx.entity(),
                theme,
            ));
        } else {
            field = field
                .h_10()
                .flex()
                .items_center()
                .px_2()
                .child(text.content().to_owned());
        }
        // Atelier's composer has no inner well and no caption: the prompt is the placeholder,
        // drawn where the first line will be until there is a first line.
        if text.content().is_empty() {
            field = field.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .px_2()
                    .py(gpui::px(5.0))
                    .line_height(crate::ui::text_area::AREA_LINE_HEIGHT)
                    .text_color(theme_faint)
                    .child(self.t(placeholder)),
            );
        }
        field.into_any_element()
    }
}
