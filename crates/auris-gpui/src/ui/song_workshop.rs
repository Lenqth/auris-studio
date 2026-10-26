//! A persistent conversation with inline, independently produced song alternatives.

use super::audio_player::{PlayerState, PlayerTarget};
use super::text_field::{KeyEffect, TextField};
use super::widgets::{ButtonState, ButtonStyle, Latch, activity_indicator, button, button_enabled};
use crate::app::{AurisApp, Pane};
use auris_agent::workshop::{Command, Snapshot, Workshop};
use auris_i18n::Key;
use auris_session::SongCandidate;
use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window, div, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};

#[path = "song_workshop/chat_input.rs"]
mod input;
#[path = "song_workshop/preview.rs"]
mod preview;

pub(crate) struct SongWorkshop {
    identity: u64,
    pub(crate) brief: TextField,
    foci: [FocusHandle; 1],
    active_field: Option<usize>,
    _focus_subscriptions: Vec<gpui::Subscription>,
    preferences: auris_session::AgentPreferences,
    models: Vec<super::agent_chat::ModelOption>,
    model_receiver: Option<mpsc::Receiver<Result<String, String>>>,
    model_error: Option<String>,
    runtime: Option<Workshop>,
    snapshot: Option<Snapshot>,
    view_round: Option<usize>,
    candidates: [Option<Arc<SongCandidate>>; 3],
    pub(crate) players: [PlayerState; 3],
    preview_serial: u64,
    preparing: Option<usize>,
    error: Option<String>,
    submitted: bool,
    show_memory: bool,
    scroll: gpui::ScrollHandle,
}

impl SongWorkshop {
    pub(crate) fn busy(&self) -> bool {
        self.submitted
            || self
                .snapshot
                .as_ref()
                .is_none_or(|s| s.planning || !s.progress.is_empty())
    }
    fn send(&self, command: Command) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(|runtime| runtime.send(command))
    }
    fn stop(&mut self) {
        self.send(Command::Stop(None));
        for player in &mut self.players {
            player.stop();
        }
        self.preview_serial += 1;
        self.preparing = None;
    }
    fn view(&mut self, index: usize) {
        for player in &mut self.players {
            player.reset_audio();
        }
        self.preview_serial += 1;
        self.preparing = None;
        self.view_round = Some(index);
        self.refresh_candidates();
        self.send(Command::View(index));
    }
    fn refresh_candidates(&mut self) {
        self.candidates = std::array::from_fn(|slot| {
            self.snapshot.as_ref().and_then(|s| {
                let candidate = s
                    .document
                    .rounds
                    .get(self.view_round?)?
                    .candidates
                    .get(slot)?;
                s.candidates.get(&candidate.id).cloned()
            })
        });
    }
    fn text(&self, _: usize) -> &TextField {
        &self.brief
    }
    fn text_mut(&mut self, _: usize) -> &mut TextField {
        &mut self.brief
    }
    pub(crate) fn field(&self) -> Option<&TextField> {
        self.active_field.map(|_| &self.brief)
    }
    pub(crate) fn field_mut(&mut self) -> Option<&mut TextField> {
        self.active_field.map(|_| &mut self.brief)
    }
}

impl Drop for SongWorkshop {
    fn drop(&mut self) {
        self.send(Command::Draft(self.brief.content().to_owned()));
    }
}

impl AurisApp {
    pub(crate) fn open_song_workshop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let focus = cx.focus_handle().tab_stop(true);
        let subscription = cx.on_focus(&focus, window, |this, _, cx| {
            if let Some(s) = &mut this.song_workshop {
                s.active_field = Some(0);
            }
            cx.notify();
        });
        let runtime = self
            .session
            .project_folder()
            .ok_or_else(|| "No project workspace".to_string())
            .and_then(|folder| {
                Workshop::start(
                    folder.into(),
                    self.session.path().is_none(),
                    self.settings.clone(),
                )
            });
        let (runtime, error) = match runtime {
            Ok(runtime) => (Some(runtime), None),
            Err(e) => (None, Some(e)),
        };
        self.song_workshop = Some(SongWorkshop {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            brief: TextField::default(),
            foci: [focus.clone()],
            active_field: Some(0),
            _focus_subscriptions: vec![subscription],
            preferences: self.settings.agent.clone(),
            models: Vec::new(),
            model_receiver: Some(auris_agent::discover_models_background(
                self.settings.agent.clone(),
            )),
            model_error: None,
            runtime,
            snapshot: None,
            view_round: None,
            candidates: std::array::from_fn(|_| None),
            players: std::array::from_fn(|_| PlayerState::default()),
            preview_serial: 0,
            preparing: None,
            error,
            submitted: false,
            show_memory: false,
            scroll: gpui::ScrollHandle::new(),
        });
        focus.focus(window);
        cx.notify();
    }

    fn start_song_workshop(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if s.busy() || s.brief.content().trim().is_empty() {
            return;
        }
        if !s.preferences.is_configured() {
            return;
        }
        if !s.send(Command::Send(
            s.brief.content().to_owned(),
            Box::new(s.preferences.clone()),
        )) {
            s.error =
                Some("The song conversation is unavailable. Close and reopen it to retry.".into());
            cx.notify();
            return;
        }
        s.brief = TextField::default();
        s.submitted = true;
        s.error = None;
        cx.notify();
    }

    pub(crate) fn poll_song_workshop(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if let Some(answer) = s.model_receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            s.model_receiver = None;
            if self.menu.as_ref().is_some_and(|menu| menu.entries.iter().any(|entry| matches!(
                entry,
                super::context_menu::MenuEntry::Item(item)
                    if matches!(item.command, super::context_menu::MenuCommand::WorkshopModel(_) | super::context_menu::MenuCommand::WorkshopRefreshModels)
            ))) {
                self.menu = None;
            }
            match answer.and_then(|text| super::agent_chat::parse_model_list(&text)) {
                Ok(models) => s.models = models,
                Err(e) => s.model_error = Some(e),
            }
            cx.notify();
        }
        if let Some(snapshot) = s.runtime.as_ref().and_then(Workshop::poll) {
            let follow_tail =
                f32::from(s.scroll.max_offset().height) + f32::from(s.scroll.offset().y) < 64.0;
            let initial = s.snapshot.is_none();
            let added = s
                .snapshot
                .as_ref()
                .map_or(0, |old| old.document.rounds.len())
                < snapshot.document.rounds.len();
            if initial && s.brief.content().is_empty() {
                s.brief = TextField::new(snapshot.document.draft.clone());
            }
            s.error = snapshot.error.clone();
            if snapshot.document.messages.len()
                > s.snapshot
                    .as_ref()
                    .map_or(0, |old| old.document.messages.len())
                || snapshot.error.is_some()
            {
                s.submitted = false;
            }
            s.snapshot = Some(snapshot);
            if follow_tail {
                s.scroll.scroll_to_bottom();
            }
            if initial || added {
                if let Some(index) = s
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.document.rounds.len().checked_sub(1))
                {
                    s.view(index);
                }
            } else {
                s.refresh_candidates();
            }
            cx.notify();
        }
    }

    fn adopt_song_candidate(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        let selection_request = self.t(Key::WorkshopSelectionRequest).to_string();
        if let Some(s) = &mut self.song_workshop
            && let Some(snapshot) = &s.snapshot
            && let Some(round) = s.view_round.and_then(|i| snapshot.document.rounds.get(i))
            && let Some(candidate) = round.candidates.get(index)
        {
            let message = selection_request.replace("{song}", &candidate.direction.title);
            if s.send(Command::Select(
                candidate.id,
                Box::new(s.preferences.clone()),
                message,
            )) {
                s.submitted = true;
                s.scroll.scroll_to_bottom();
            }
        }
        cx.notify();
    }

    fn open_workshop_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if s.busy() || self.agent_chat.busy {
            return;
        }
        let Some(snapshot) = &s.snapshot else {
            return;
        };
        let Some(candidate) = snapshot
            .document
            .selected
            .and_then(|id| snapshot.candidates.get(&id))
            .cloned()
        else {
            return;
        };
        let Some(folder) = self
            .session
            .project_folder()
            .map(std::path::Path::to_path_buf)
        else {
            return;
        };
        let unsaved = self.session.path().is_none();
        let revision = self.session.revision();
        let selected = snapshot.document.selected;
        let mut document = snapshot.document.clone();
        document.draft = s.brief.content().to_owned();
        let identity = s.identity;
        s.submitted = true;
        let view = cx.entity().downgrade();
        window
            .spawn(cx, async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { auris_agent::workshop::handoff(folder, unsaved, document) })
                    .await;
                let _ = view.update_in(cx, |this, window, cx| {
                    let Some(s) = &mut this.song_workshop else {
                        return;
                    };
                    if s.identity != identity {
                        return;
                    }
                    s.submitted = false;
                    if this.session.revision() != revision
                        || s.snapshot.as_ref().and_then(|s| s.document.selected) != selected
                    {
                        s.error = Some(
                            "The selected song changed while opening the editor. Try again.".into(),
                        );
                        cx.notify();
                        return;
                    }
                    match result {
                        Err(e) => s.error = Some(e),
                        Ok((store, id)) => {
                            if let Err(e) = this.session.adopt_song_candidate(&candidate) {
                                s.error = Some(e.to_string());
                                cx.notify();
                                return;
                            }
                            this.song_workshop = None;
                            this.project_chooser = None;
                            this.open_workshop_conversation(store, id);
                            this.resync_selection();
                            this.reset_view();
                            this.focus_pane(Pane::Arrangement, window);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        cx.notify();
    }

    pub(crate) fn render_song_workshop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.render_workshop_input(cx);
        let menu = self
            .render_context_menu(window, cx)
            .map(IntoElement::into_any_element);
        let theme = self.theme.clone();
        let Some(s) = &self.song_workshop else {
            return div().into_any_element();
        };
        let enabled = |yes| {
            if yes {
                ButtonState::Enabled(Latch::Off)
            } else {
                ButtonState::Disabled
            }
        };
        let mut body = div()
            .id("song-workshop-body")
            .w_full()
            .max_w(gpui::px(1000.0))
            .mx_auto()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&s.scroll)
            .p_4()
            .flex()
            .flex_col()
            .gap_4();
        if let Some(snapshot) = &s.snapshot {
            if snapshot.document.messages.is_empty() {
                body = body.child(self.t(Key::WorkshopChatHint));
            }
            for (i, message) in snapshot.document.messages.iter().enumerate() {
                body =
                    body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_color(theme.text_muted).child(self.t(
                                if message.user {
                                    Key::WorkshopYou
                                } else {
                                    Key::WorkshopAssistant
                                },
                            )))
                            .child(message.text.clone()),
                    );
                for (round_index, round) in snapshot
                    .document
                    .rounds
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.after_message == i + 1)
                {
                    if s.view_round != Some(round_index) {
                        body = body.child(button(
                            ("song-round", round_index),
                            round
                                .candidates
                                .iter()
                                .map(|c| c.direction.title.as_str())
                                .collect::<Vec<_>>()
                                .join(" · "),
                            ButtonStyle::Normal,
                            false,
                            theme.accent,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(s) = &mut this.song_workshop {
                                    s.view(round_index);
                                }
                                cx.notify();
                            }),
                        ));
                        continue;
                    }
                    let mut cards = div()
                        .id(("song-candidates", round_index))
                        .w_full()
                        .min_w_0()
                        .flex_shrink_0()
                        .overflow_x_scroll()
                        .flex()
                        .gap_3();
                    for (index, candidate) in round.candidates.iter().enumerate() {
                        let id = candidate.id;
                        let ready = s.candidates[index].is_some();
                        let running = snapshot.progress.contains_key(&id);
                        let selected = snapshot.document.selected == Some(id);
                        let mut card = div()
                            .id(("song-candidate", index))
                            .debug_selector(move || format!("song-candidate-{index}"))
                            .flex_1()
                            .min_w(gpui::px(300.0))
                            .p_3()
                            .border_1()
                            .border_color(if selected {
                                theme.selection
                            } else {
                                theme.border
                            })
                            .rounded_md()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_base().child(candidate.direction.title.clone()))
                            .child(candidate.direction.description.clone());
                        if selected {
                            card = card.child(self.t(Key::WorkshopSelected));
                        }
                        if let Some(stage) = snapshot.progress.get(&id) {
                            let label = self.t(match stage.as_str() {
                                "harmony" => Key::WorkshopHarmonyProgress,
                                "melody" => Key::WorkshopMelodyProgress,
                                "compose" => Key::WorkshopComposing,
                                "sing" => Key::WorkshopSinging,
                                "render" => Key::WorkshopRendering,
                                "render_wait" => Key::WorkshopQueued,
                                "stopping" => Key::WorkshopStopping,
                                "planning" => Key::WorkshopPlanning,
                                "recovering" => Key::WorkshopRecovering,
                                _ => Key::WorkshopArrangeProgress,
                            });
                            card = card.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(activity_indicator(
                                        gpui::SharedString::from(format!(
                                            "song-candidate-activity-{index}"
                                        )),
                                        theme.accent,
                                    ))
                                    .child(label),
                            );
                        }
                        if let Some(error) = &candidate.error {
                            card = card.child(div().text_color(theme.danger).child(error.clone()));
                        }
                        if candidate.recoveries > 0 {
                            card = card.child(format!(
                                "{} {}/{}",
                                self.t(Key::WorkshopRecoveryCount),
                                candidate.recoveries,
                                auris_agent::workshop::AUTO_RECOVERY_LIMIT
                            ));
                        }
                        if !running
                            && !candidate.ready
                            && candidate.recoveries >= auris_agent::workshop::AUTO_RECOVERY_LIMIT
                        {
                            card = card.child(self.t(Key::WorkshopRecoveryLimit));
                        }
                        card = card.child(
                            self.render_audio_player(
                                PlayerTarget::Workshop(index),
                                s.candidates[index]
                                    .as_ref()
                                    .map_or(0.0, |c| c.audio.duration_seconds()),
                                ready && !running && s.preparing.is_none(),
                                s.preparing == Some(index),
                                cx,
                            ),
                        );
                        card = card.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(button_enabled(
                                    ("song-adopt", index),
                                    self.t(Key::WorkshopContinue),
                                    ButtonStyle::Normal,
                                    enabled(ready && !running),
                                    theme.accent,
                                    &theme,
                                    cx.listener(move |this, _, window, cx| {
                                        this.adopt_song_candidate(index, window, cx)
                                    }),
                                ))
                                .child(button_enabled(
                                    ("song-retry", index),
                                    self.t(if running {
                                        Key::WorkshopCancel
                                    } else if !candidate.ready {
                                        Key::WorkshopRecover
                                    } else {
                                        Key::WorkshopRetry
                                    }),
                                    ButtonStyle::Normal,
                                    enabled(
                                        running
                                            || (!snapshot.planning && snapshot.progress.len() < 3),
                                    ),
                                    theme.accent,
                                    &theme,
                                    cx.listener({
                                        let unfinished = !candidate.ready;
                                        move |this, _, _, cx| {
                                            if let Some(s) = &this.song_workshop {
                                                s.send(if running {
                                                    Command::Stop(Some(id))
                                                } else if unfinished {
                                                    Command::Recover(
                                                        id,
                                                        Box::new(s.preferences.clone()),
                                                    )
                                                } else {
                                                    Command::Retry(id)
                                                });
                                            }
                                            cx.notify();
                                        }
                                    }),
                                )),
                        );
                        cards = cards.child(card);
                    }
                    body = body.child(cards);
                }
            }
            if snapshot.planning {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(activity_indicator("song-planning-activity", theme.accent))
                        .child(self.t(Key::WorkshopPlanning)),
                );
            }
            if s.show_memory {
                for (index, selection) in snapshot.preferences.selections.iter().enumerate() {
                    if let Some(chosen) = selection.alternatives.get(selection.chosen) {
                        let conversation = selection.conversation.clone();
                        let candidate = selection.candidate;
                        body = body.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .text_color(theme.text_muted)
                                .child(format!("{} — {}", chosen.title, chosen.description))
                                .child(selection.context.clone())
                                .child(button(
                                    ("song-forget-choice", index),
                                    self.t(Key::WorkshopForgetChoice),
                                    ButtonStyle::Normal,
                                    false,
                                    theme.accent,
                                    &theme,
                                    cx.listener(move |this, _, _, cx| {
                                        if let Some(s) = &this.song_workshop {
                                            s.send(Command::ForgetChoice(
                                                conversation.clone(),
                                                candidate,
                                            ));
                                        }
                                        cx.notify();
                                    }),
                                )),
                        );
                    }
                }
                body = body.child(button(
                    "song-forget",
                    self.t(Key::WorkshopForget),
                    ButtonStyle::Normal,
                    false,
                    theme.accent,
                    &theme,
                    cx.listener(|this, _, _, cx| {
                        if let Some(s) = &this.song_workshop {
                            s.send(Command::Forget);
                        }
                        cx.notify();
                    }),
                ));
            }
        } else {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(activity_indicator("song-loading-activity", theme.accent))
                    .child(self.t(Key::WorkshopLoadingModels)),
            );
        }
        if let Some(error) = &s.error {
            body = body.child(div().text_color(theme.danger).child(error.clone()));
        }
        let selected = s
            .snapshot
            .as_ref()
            .is_some_and(|s| s.document.selected.is_some());
        let memory = s.snapshot.as_ref().is_some_and(|s| s.preferences.enabled);
        let footer = div()
            .w_full()
            .max_w(gpui::px(1000.0))
            .mx_auto()
            .flex_shrink_0()
            .p_4()
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_2()
            .child(input)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(button(
                        "song-back",
                        self.t(Key::WorkshopBack),
                        ButtonStyle::Normal,
                        false,
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, window, cx| {
                            this.song_workshop = None;
                            this.session.stop_output_preview();
                            this.focus.focus(window);
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "song-memory",
                        self.t(Key::WorkshopMemory),
                        ButtonStyle::Normal,
                        memory,
                        theme.accent,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &this.song_workshop {
                                s.send(Command::Memory(!memory));
                            }
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "song-memory-view",
                        self.t(Key::WorkshopMemoryView),
                        ButtonStyle::Normal,
                        s.show_memory,
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop {
                                s.show_memory = !s.show_memory;
                            }
                            cx.notify();
                        }),
                    ))
                    .child(div().flex_1())
                    .child(button_enabled(
                        "song-editor",
                        self.t(Key::WorkshopOpenEditor),
                        ButtonStyle::Normal,
                        enabled(selected && !s.busy()),
                        theme.accent,
                        &theme,
                        cx.listener(Self::open_workshop_editor_click),
                    ))
                    .child(button_enabled(
                        "song-cancel",
                        self.t(Key::WorkshopCancel),
                        ButtonStyle::Normal,
                        enabled(s.busy()),
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop {
                                s.stop();
                            }
                            cx.notify();
                        }),
                    ))
                    .child(button_enabled(
                        "song-generate",
                        self.t(Key::WorkshopSend),
                        ButtonStyle::Primary,
                        enabled(
                            !s.busy()
                                && !s.brief.content().trim().is_empty()
                                && s.preferences.is_configured(),
                        ),
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, _, cx| this.start_song_workshop(cx)),
                    )),
            );
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font(theme.font.clone())
            .text_sm()
            .track_focus(&self.focus)
            .key_context(crate::actions::KEY_CONTEXT)
            .on_key_down(cx.listener(Self::workshop_key))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                this.move_audio_player(event, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.audio_player_drag = None),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.audio_player_drag = None),
            )
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusNextPane, window, _| window.focus_next()),
            )
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusPreviousPane, window, _| {
                    window.focus_prev()
                }),
            )
            .child(
                crate::titlebar::titlebar(window, &theme)
                    .child(
                        crate::titlebar::drag_region("song-workshop-title")
                            .flex_1()
                            .justify_center()
                            .child(self.t(Key::WorkshopTitle)),
                    )
                    .child(crate::titlebar::controls(
                        window,
                        &theme,
                        cx.listener(|_, _, window, _| window.remove_window()),
                    )),
            )
            .child(body)
            .child(footer)
            .children(menu)
            .children(crate::titlebar::resize_handles(window))
            .into_any_element()
    }

    fn open_workshop_editor_click(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workshop_editor(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ready(app: &gpui::Entity<AurisApp>, cx: &mut gpui::VisualTestContext) {
        app.update(cx, |this, _| {
            let s = this.song_workshop.as_mut().unwrap();
            s.runtime = None;
            s.model_receiver = None;
            s.snapshot = Some(Snapshot {
                document: Default::default(),
                candidates: Default::default(),
                progress: Default::default(),
                planning: false,
                preferences: Default::default(),
                error: None,
            });
        });
    }

    #[gpui::test]
    fn candidates_stay_horizontal_and_identify_active_workers(cx: &mut gpui::TestAppContext) {
        use auris_session::workshop::{Candidate, Direction, Message, Round};

        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |this, cx| {
                this.show_project_chooser(window, cx);
                this.open_song_workshop(window, cx);
            })
        });
        make_ready(&app, cx);
        app.update(cx, |this, _| {
            let s = this.song_workshop.as_mut().unwrap();
            s.view_round = Some(0);
            let snapshot = s.snapshot.as_mut().unwrap();
            snapshot.document.messages.push(Message {
                user: false,
                text: "Compare these songs".into(),
            });
            snapshot.document.rounds.push(Round {
                after_message: 1,
                candidates: (0..3)
                    .map(|id| Candidate {
                        id,
                        parent: None,
                        ready: false,
                        error: None,
                        recoveries: 0,
                        direction: Direction {
                            title: format!("Candidate {id}"),
                            description: "Piano and strings".into(),
                            instructions: String::new(),
                            protected_tracks: Vec::new(),
                        },
                    })
                    .collect(),
            });
            snapshot.progress.insert(0, "melody".into());
            snapshot.planning = true;
        });
        for width in [1200.0, 800.0] {
            crate::harness::resize(&app, cx, gpui::size(gpui::px(width), gpui::px(800.0)));
            let a = cx.debug_bounds("song-candidate-0").unwrap();
            let b = cx.debug_bounds("song-candidate-1").unwrap();
            let c = cx.debug_bounds("song-candidate-2").unwrap();
            assert_eq!(a.origin.y, b.origin.y);
            assert_eq!(b.origin.y, c.origin.y);
            assert!(a.right() < b.left() && b.right() < c.left());
            assert!(a.size.width >= gpui::px(300.0));
            assert!(cx.debug_bounds("song-candidate-activity-0").is_some());
            assert!(cx.debug_bounds("song-candidate-activity-1").is_none());
            assert!(cx.debug_bounds("song-planning-activity").is_some());
        }
    }

    #[gpui::test]
    fn single_composer_accepts_japanese_and_shift_enter_without_changing_score(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::harness::open(cx);
        let before = app.read_with(cx, |this, _| this.session.project().clone());
        cx.update(|window, cx| {
            app.update(cx, |this, cx| {
                this.show_project_chooser(window, cx);
                this.open_song_workshop(window, cx);
            })
        });
        make_ready(&app, cx);
        crate::harness::paint(&app, cx);
        crate::harness::click("song-brief", cx);
        cx.simulate_input("静かなピアノ曲。歌詞は");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("さくらさいた");
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.song_workshop.as_ref().unwrap().brief.content(),
                "静かなピアノ曲。歌詞は\nさくらさいた"
            );
            assert_eq!(this.session.project(), &before);
        });
    }

    #[gpui::test]
    fn changing_comparison_invalidates_pending_audition(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| app.update(cx, |this, cx| this.open_song_workshop(window, cx)));
        make_ready(&app, cx);
        app.update(cx, |this, _| {
            let s = this.song_workshop.as_mut().unwrap();
            s.preparing = Some(0);
            let serial = s.preview_serial;
            s.view(1);
            assert!(s.preparing.is_none());
            assert_ne!(serial, s.preview_serial);
        });
    }

    #[gpui::test]
    fn model_selection_does_not_change_global_settings(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        let before = app.read_with(cx, |this, _| this.settings.agent.clone());
        cx.update(|window, cx| app.update(cx, |this, cx| this.open_song_workshop(window, cx)));
        make_ready(&app, cx);
        app.update(cx, |this, _| {
            this.song_workshop.as_mut().unwrap().models =
                vec![super::super::agent_chat::ModelOption {
                    name: "test-model".into(),
                    ..Default::default()
                }];
            this.choose_workshop_model(0);
            assert_eq!(
                this.song_workshop.as_ref().unwrap().preferences.model,
                "test-model"
            );
            assert_eq!(this.settings.agent, before);
        });
    }
}
