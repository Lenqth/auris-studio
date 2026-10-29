//! One natural-language composer and provider controls.
use super::*;
use crate::ui::chat_commands::{self, CompletionKey, Scope};
use crate::ui::context_menu::{ContextMenu, MenuCommand};
impl AurisApp {
    pub(super) fn render_workshop_input(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
            && !s.brief.content().trim().is_empty()
            && (s.preferences.is_configured() || s.brief.content().trim().starts_with('/'));
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
            s.model_error.is_some(),
        )
        .debug_selector(|| "song-workshop-composer".to_owned())
        .m_0()
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(theme.atelier.spacing.xs)
                .children(s.attachments.iter().enumerate().map(|(index, path)| {
                    let path = path.clone();
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .max_w_full()
                        .child(
                            div()
                                .debug_selector(move || format!("song-draft-attachment-{index}"))
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .child(
                                    atelier::Button::new(("song-draft-attachment", index), name)
                                        .ghost()
                                        .icon(atelier::IconName::File)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_attachment_preview(path.clone(), true, cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("song-remove-attachment-{index}"))
                                .flex_shrink_0()
                                .child(
                                    atelier::Button::icon_only(
                                        ("song-remove-attachment", index),
                                        atelier::IconName::Close,
                                    )
                                    .ghost()
                                    .disabled(busy)
                                    .tooltip(
                                        self.t(Key::AgentRemoveAttachment).replace(
                                            "{name}",
                                            &s.attachments[index]
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy(),
                                        ),
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(s) = &mut this.song_workshop
                                                && !s.busy()
                                                && index < s.attachments.len()
                                            {
                                                s.attachments.remove(index);
                                            }
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                })),
        )
        .children(s.completions.render(
            if s.brief.marked().is_none() && s.active_field == Some(0) {
                s.completions
                    .matches(s.brief.content(), &s.models, Scope::Workshop)
            } else {
                Vec::new()
            },
            Scope::Workshop,
            &theme,
            self.language(),
            cx,
        ))
        .child(self.workshop_field(0, s.input_assist.placeholder(), true, cx))
        .children(
            (!busy)
                .then(|| s.input_assist.suffix(&s.brief, &s.preferences))
                .flatten()
                .map(|suffix| {
                    div()
                        .id("workshop-input-suggestion")
                        .debug_selector(|| "workshop-input-suggestion".into())
                        .flex()
                        .items_center()
                        .gap(theme.atelier.spacing.xs)
                        .child(
                            div()
                                .debug_selector(|| "workshop-accept-suggestion".into())
                                .min_w_0()
                                .overflow_hidden()
                                .child(
                                    atelier::Button::new(
                                        "workshop-accept-suggestion",
                                        suffix.to_owned(),
                                    )
                                    .ghost()
                                    .size(atelier::ControlSize::Small)
                                    .tooltip(suffix.to_owned())
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.focus_workshop_input(window);
                                            this.accept_workshop_suggestion();
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_faint)
                                .child(self.t(Key::WorkshopInputSuggestion)),
                        )
                        .child(
                            atelier::Button::icon_only(
                                "workshop-dismiss-suggestion",
                                atelier::IconName::Close,
                            )
                            .ghost()
                            .size(atelier::ControlSize::Small)
                            .tooltip(self.t(Key::WorkshopDismissSuggestion))
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    if let Some(s) = &mut this.song_workshop {
                                        s.input_assist.dismiss();
                                        s.foci[0].focus(window);
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                }),
        )
        .child(
            agent_components::composer_controls("workshop-composer-controls", &theme)
                .child(
                    atelier::Button::icon_only("song-attach-files", atelier::IconName::Paperclip)
                        .ghost()
                        .tooltip(self.t(Key::AttachFiles))
                        .disabled(busy)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.pick_conversation_files(true, cx)),
                        ),
                )
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
        if let Some(feedback) = &s.command_feedback {
            body = body.child(match feedback {
                Ok(text) => agent_components::status_notice(text.clone()),
                Err(text) => agent_components::error_notice(text.clone()),
            });
        }
        if let Some(error) = &s.model_error {
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
        if self.menu_key(event, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if let Some(s) = &mut self.song_workshop
            && s.foci[0].is_focused(window)
        {
            if s.brief.marked().is_some() {
                return;
            }
            let matches = s
                .completions
                .matches(s.brief.content(), &s.models, Scope::Workshop);
            match s.completions.key(s.brief.content(), &matches, event) {
                CompletionKey::Handled => {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                CompletionKey::Complete { fill, submit } => {
                    self.accept_workshop_completion(&fill);
                    if submit {
                        self.start_song_workshop(cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                CompletionKey::Ignored => {}
            }
            if s.input_assist.suffix(&s.brief, &s.preferences).is_some() {
                match event.keystroke.key.as_str() {
                    "tab" if !event.keystroke.modifiers.shift => {
                        self.accept_workshop_suggestion();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    "escape" => {
                        s.input_assist.dismiss();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Some(s) = &mut self.song_workshop
            && s.foci[0].is_focused(window)
        {
            if event.keystroke.key == "tab" {
                s.active_field = None;
                if event.keystroke.modifiers.shift {
                    window.focus_prev();
                } else {
                    window.focus_next();
                }
                cx.stop_propagation();
                cx.notify();
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
            let effect = s.brief.apply_key_with_clipboard(
                &event.keystroke.key,
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.secondary(),
                true,
                cx,
            );
            if effect == KeyEffect::Changed {
                s.completions.reset();
                s.input_assist.edited();
            }
            if effect != KeyEffect::Ignored {
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

    pub(crate) fn reset_workshop_completions(&mut self) {
        if let Some(s) = &mut self.song_workshop {
            s.completions.reset();
            s.input_assist.edited();
        }
    }

    pub(in crate::ui) fn accept_workshop_completion(&mut self, fill: &str) {
        if let Some(s) = &mut self.song_workshop {
            s.brief.replace(0..s.brief.content().len(), fill);
            s.completions.reset();
            s.active_field = Some(0);
        }
    }

    pub(in crate::ui) fn focus_workshop_input(&mut self, window: &mut Window) {
        if let Some(s) = &mut self.song_workshop {
            s.active_field = Some(0);
            s.foci[0].focus(window);
        }
    }

    /// Commands never become composition requests or consume draft attachments.
    pub(super) fn workshop_control_command(&mut self) -> bool {
        let language = self.language();
        let Some(s) = &mut self.song_workshop else {
            return false;
        };
        let text = s.brief.content().trim().to_owned();
        if !text.starts_with('/') {
            return false;
        }
        let (command, value) = text.split_once(' ').unwrap_or((&text, ""));
        let value = value.trim();
        let result = match command {
            "/help" => Ok(Some(chat_commands::help(Scope::Workshop, language))),
            "/model" | "/effort" if s.busy() => Err(Key::AgentSlashBusy.get(language).to_owned()),
            "/model" => {
                if s.model_receiver.is_some() {
                    Err(Key::AgentModelSearchLoading.get(language).to_owned())
                } else if value.is_empty() {
                    self.accept_workshop_completion("/model ");
                    return true;
                } else if let Some(&index) =
                    crate::ui::agent_chat::model_search::matches(value, &s.models).first()
                {
                    self.choose_workshop_model(index);
                    Ok(None)
                } else {
                    Err(Key::AgentModelSearchEmpty.get(language).to_owned())
                }
            }
            "/effort" => match auris_session::ReasoningEffort::named(value) {
                Some(effort)
                    if s.preferences.backend == "opencode" && effort != Default::default() =>
                {
                    Err(Key::AgentOpenCodeVariantHelp.get(language).to_owned())
                }
                Some(effort) => {
                    s.preferences.effort = effort;
                    s.preferences.thinking = None;
                    Ok(None)
                }
                None => Err(
                    "/effort default | none | minimal | low | medium | high | xhigh | max".into(),
                ),
            },
            _ => Err(if chat_commands::is_control_command(&text) {
                Key::AgentSlashWorkshopOnlyPanel
            } else {
                Key::AgentSlashUnknown
            }
            .get(language)
            .to_owned()),
        };
        if let Some(s) = &mut self.song_workshop {
            match result {
                Ok(feedback) => {
                    s.command_feedback = feedback.map(Ok);
                    s.brief = TextField::default();
                }
                Err(error) => s.command_feedback = Some(Err(error)),
            }
            s.completions.reset();
        }
        true
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
