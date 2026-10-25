//! New-song directions, isolated generation, audition and exact adoption.

use super::text_field::{KeyEffect, TextField};
use super::widgets::{ButtonState, ButtonStyle, Latch, button, button_enabled};
use crate::app::{AurisApp, Pane};
use auris_i18n::Key;
use auris_session::{SongCandidate, SongIdea};
use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window, div, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

pub(crate) struct SongWorkshop {
    identity: u64,
    pub(crate) brief: TextField,
    brief_focus: FocusHandle,
    genre: usize,
    mood: usize,
    seconds: u32,
    planner: Option<auris_agent::song_ideas::SongPlanner>,
    receiver: Option<mpsc::Receiver<(usize, Result<SongCandidate, String>)>>,
    cancel: Arc<AtomicBool>,
    ideas: Option<[SongIdea; 3]>,
    candidates: [Option<SongCandidate>; 3],
    errors: [Option<String>; 3],
    pending: [bool; 3],
    error: Option<String>,
    captured_brief: String,
    preview_serial: u64,
    preview: Option<(usize, auris_session::OutputPreviewStatus)>,
    preparing: Option<usize>,
}

impl Drop for SongWorkshop {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some((_, status)) = &self.preview {
            status.stop();
        }
    }
}

impl SongWorkshop {
    pub(crate) fn busy(&self) -> bool {
        self.planner.is_some() || self.receiver.is_some()
    }
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.planner = None;
        self.receiver = None;
        self.pending = [false; 3];
        self.preview_serial = self.preview_serial.wrapping_add(1);
        self.preparing = None;
        if let Some((_, status)) = self.preview.take() {
            status.stop();
        }
    }
    fn generate(&mut self, indices: Vec<usize>) {
        let Some(ideas) = self.ideas.clone() else {
            return;
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let seconds = self.seconds;
        let brief = self.captured_brief.clone();
        let (sender, receiver) = mpsc::channel();
        for &index in &indices {
            self.pending[index] = true;
            self.errors[index] = None;
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        match std::thread::Builder::new()
            .name("auris-song-candidates".into())
            .spawn(move || {
                for index in indices {
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                    let result = SongCandidate::generate(
                        ideas[index].clone(),
                        &brief,
                        seconds,
                        seed.wrapping_add(index as u64),
                        &cancel,
                    );
                    if cancel.load(Ordering::Acquire) || sender.send((index, result)).is_err() {
                        break;
                    }
                }
            }) {
            Ok(_) => self.receiver = Some(receiver),
            Err(error) => {
                self.pending = [false; 3];
                self.error = Some(error.to_string());
            }
        }
    }
}

const GENRES: &[&str] = &["Pop", "Rock", "Electronic", "Orchestral", "Jazz", "Ambient"];
const GENRE_LABELS: &[Key] = &[
    Key::WorkshopPop,
    Key::WorkshopRock,
    Key::WorkshopElectronic,
    Key::WorkshopOrchestral,
    Key::WorkshopJazz,
    Key::WorkshopAmbient,
];
const MOODS: &[Key] = &[
    Key::WorkshopBright,
    Key::WorkshopMelancholy,
    Key::WorkshopCalm,
    Key::WorkshopEnergetic,
];

impl AurisApp {
    pub(crate) fn open_song_workshop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let brief_focus = cx.focus_handle().tab_stop(true);
        self.focus.focus(window);
        self.song_workshop = Some(SongWorkshop {
            identity: {
                static NEXT: AtomicU64 = AtomicU64::new(1);
                NEXT.fetch_add(1, Ordering::Relaxed)
            },
            brief: TextField::default(),
            brief_focus,
            genre: 0,
            mood: 0,
            seconds: 60,
            planner: None,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            ideas: None,
            candidates: std::array::from_fn(|_| None),
            errors: std::array::from_fn(|_| None),
            pending: [false; 3],
            error: None,
            captured_brief: String::new(),
            preview_serial: 0,
            preview: None,
            preparing: None,
        });
        cx.notify();
    }

    fn start_song_workshop(&mut self, cx: &mut Context<Self>) {
        let language = self.language();
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        if state.busy() {
            return;
        }
        state.stop();
        let brief = format!(
            "Genre: {}\nMood: {}\n{}",
            GENRES[state.genre],
            MOODS[state.mood].get(language),
            state.brief.content()
        );
        match auris_agent::song_ideas::SongPlanner::start(
            self.settings.agent.clone(),
            &brief,
            state.seconds,
        ) {
            Ok(planner) => {
                state.planner = Some(planner);
                state.captured_brief = brief;
                state.candidates = std::array::from_fn(|_| None);
                state.errors = std::array::from_fn(|_| None);
                state.ideas = None;
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn poll_song_workshop(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        if let Some(result) = state.planner.as_mut().and_then(|p| p.poll()) {
            state.planner = None;
            match result {
                Ok(ideas) => {
                    state.ideas = Some(ideas);
                    state.generate(vec![0, 1, 2]);
                }
                Err(error) => state.error = Some(error),
            }
            cx.notify();
        }
        let mut disconnected = false;
        if let Some(receiver) = &state.receiver {
            loop {
                match receiver.try_recv() {
                    Ok((index, result)) => {
                        state.pending[index] = false;
                        match result {
                            Ok(candidate) => state.candidates[index] = Some(candidate),
                            Err(error) => state.errors[index] = Some(error),
                        }
                        cx.notify();
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            state.receiver = None;
            for index in 0..3 {
                if state.pending[index] {
                    state.errors[index] = Some("Candidate worker stopped".into());
                    state.pending[index] = false;
                }
            }
            cx.notify();
        }
    }

    fn preview_song_candidate(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.acquire_audio(cx) {
            return;
        }
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        let Some(candidate) = &state.candidates[index] else {
            return;
        };
        let audio = candidate.audio.clone();
        if let Some((_, status)) = state.preview.take() {
            status.stop();
        }
        state.preview_serial = state.preview_serial.wrapping_add(1);
        let serial = state.preview_serial;
        let identity = state.identity;
        state.preparing = Some(index);
        let rate = self.session.sample_rate();
        cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { auris_session::prepare_output_preview(&audio, rate) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(state) = &mut this.song_workshop else {
                    return;
                };
                if state.identity != identity || state.preview_serial != serial {
                    return;
                }
                state.preparing = None;
                if this.session.audio_suspended() {
                    return;
                }
                match prepared.and_then(|audio| this.session.play_output_preview(audio)) {
                    Ok(status) => state.preview = Some((index, status)),
                    Err(error) => state.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn adopt_song_candidate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        let Some(candidate) = &state.candidates[index] else {
            return;
        };
        if let Err(error) = self.session.adopt_song_candidate(candidate) {
            state.error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.song_workshop = None;
        self.project_chooser = None;
        self.agent_reset_conversation();
        self.resync_selection();
        self.reset_view();
        self.focus_pane(Pane::Arrangement, window);
        cx.notify();
    }

    pub(crate) fn render_song_workshop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.clone();
        let state = self.song_workshop.as_ref().expect("workshop visible");
        let busy = state.busy();
        let enabled = |yes| {
            if yes {
                ButtonState::Enabled(Latch::Off)
            } else {
                ButtonState::Disabled
            }
        };
        let mut genres = div().flex().flex_wrap().gap_2();
        for (index, label) in GENRE_LABELS.iter().enumerate() {
            genres = genres.child(button_enabled(
                ("song-genre", index),
                self.t(*label),
                ButtonStyle::Normal,
                if busy {
                    ButtonState::Disabled
                } else {
                    ButtonState::Enabled(if state.genre == index {
                        Latch::On
                    } else {
                        Latch::Off
                    })
                },
                theme.accent_soft,
                &theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && !s.busy()
                    {
                        s.genre = index;
                    }
                    cx.notify();
                }),
            ));
        }
        let mut moods = div().flex().flex_wrap().gap_2();
        for (index, key) in MOODS.iter().enumerate() {
            moods = moods.child(button_enabled(
                ("song-mood", index),
                self.t(*key),
                ButtonStyle::Normal,
                if busy {
                    ButtonState::Disabled
                } else {
                    ButtonState::Enabled(if state.mood == index {
                        Latch::On
                    } else {
                        Latch::Off
                    })
                },
                theme.accent_soft,
                &theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && !s.busy()
                    {
                        s.mood = index;
                    }
                    cx.notify();
                }),
            ));
        }
        let mut lengths = div().flex().gap_2();
        for seconds in [30, 60, 90] {
            lengths = lengths.child(button_enabled(
                ("song-length", seconds as usize),
                format!("{seconds} s"),
                ButtonStyle::Normal,
                if busy {
                    ButtonState::Disabled
                } else {
                    ButtonState::Enabled(if state.seconds == seconds {
                        Latch::On
                    } else {
                        Latch::Off
                    })
                },
                theme.accent_soft,
                &theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && !s.busy()
                    {
                        s.seconds = seconds;
                    }
                    cx.notify();
                }),
            ));
        }
        let mut body = div()
            .id("song-workshop-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_lg().child(self.t(Key::WorkshopTitle)))
            .child(
                div()
                    .text_color(theme.text_muted)
                    .child(self.t(Key::WorkshopHint)),
            )
            .child(div().child(self.t(Key::WorkshopGenre)))
            .child(genres)
            .child(div().child(self.t(Key::WorkshopMood)))
            .child(moods)
            .child(div().child(self.t(Key::WorkshopLength)))
            .child(lengths)
            .child(div().child(self.t(Key::WorkshopBrief)))
            .child(
                div()
                    .text_color(theme.text_muted)
                    .child(self.t(Key::WorkshopBriefHint)),
            )
            .child(
                div()
                    .id("song-brief")
                    .debug_selector(|| "song-brief".into())
                    .h_10()
                    .flex_shrink_0()
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .bg(theme.surface)
                    .track_focus(&state.brief_focus)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event, window, cx| {
                            if let Some(state) = &mut this.song_workshop
                                && !state.busy()
                            {
                                super::prompt::click_field(
                                    &mut state.brief,
                                    &state.brief_focus,
                                    true,
                                    window,
                                    event,
                                );
                                state.brief_focus.focus(window);
                                cx.notify();
                            }
                        }),
                    )
                    .child(super::prompt::editable_text(
                        state.brief.content().to_owned().into(),
                        state.brief.selection(),
                        state.brief.marked(),
                        state.brief_focus.clone(),
                        cx.entity(),
                        theme.clone(),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(format!(
                        "AI: {} / {}",
                        self.settings.agent.backend, self.settings.agent.model
                    ))
                    .child(button(
                        "song-ai-settings",
                        self.t(Key::AgentConfigure),
                        ButtonStyle::Normal,
                        false,
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, _, cx| {
                            this.open_settings_tab(crate::settings_window::SettingsTab::Agent, cx)
                        }),
                    )),
            );
        if let Some(error) = &state.error {
            body = body.child(div().text_color(theme.danger).child(error.clone()));
        }
        if state.planner.is_some() {
            body = body.child(div().child(self.t(Key::WorkshopPlanning)));
        }
        if let Some(ideas) = &state.ideas {
            let mut cards = div().flex().flex_wrap().gap_3();
            for (index, idea) in ideas.iter().enumerate() {
                let ready = state.candidates[index].is_some();
                let playing = state
                    .preview
                    .as_ref()
                    .is_some_and(|(i, s)| *i == index && s.is_active());
                let mut card = div()
                    .flex_1()
                    .min_w(gpui::rems(16.0))
                    .p_4()
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_base()
                            .child(format!("{} · {}", index + 1, idea.title)),
                    )
                    .child(idea.description.clone())
                    .child(div().text_color(theme.text_muted).child(format!(
                        "{} · {} BPM · {}",
                        idea.preset, idea.tempo, idea.key
                    )));
                if state.pending[index] {
                    card = card.child(self.t(Key::WorkshopRendering));
                }
                if state.preparing == Some(index) {
                    card = card.child(self.t(Key::WorkshopPreparing));
                }
                if let Some(error) = &state.errors[index] {
                    card = card.child(div().text_color(theme.danger).child(error.clone()));
                }
                card = card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(button_enabled(
                            ("song-preview", index),
                            self.t(if playing {
                                Key::WorkshopStop
                            } else {
                                Key::WorkshopListen
                            }),
                            ButtonStyle::Normal,
                            enabled(ready && state.preparing.is_none()),
                            theme.accent,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                if playing {
                                    if let Some(s) = &mut this.song_workshop
                                        && let Some((_, p)) = s.preview.take()
                                    {
                                        p.stop();
                                    }
                                } else {
                                    this.preview_song_candidate(index, cx);
                                }
                                cx.notify();
                            }),
                        ))
                        .child(button_enabled(
                            ("song-retry", index),
                            self.t(Key::WorkshopRetry),
                            ButtonStyle::Normal,
                            enabled(!busy),
                            theme.accent,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(s) = &mut this.song_workshop {
                                    s.stop();
                                    s.generate(vec![index]);
                                }
                                cx.notify();
                            }),
                        ))
                        .child(button_enabled(
                            ("song-adopt", index),
                            self.t(Key::WorkshopAdopt),
                            ButtonStyle::Primary,
                            enabled(ready && !state.pending[index]),
                            theme.accent,
                            &theme,
                            cx.listener(move |this, _, window, cx| {
                                this.adopt_song_candidate(index, window, cx)
                            }),
                        )),
                );
                cards = cards.child(card);
            }
            body = body.child(cards);
        }
        let footer = div()
            .p_4()
            .border_t_1()
            .border_color(theme.border)
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
            .child(div().flex_1())
            .child(button_enabled(
                "song-cancel",
                self.t(Key::WorkshopCancel),
                ButtonStyle::Normal,
                enabled(busy),
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
                self.t(Key::WorkshopGenerate),
                ButtonStyle::Primary,
                enabled(!busy && self.settings.agent.is_configured()),
                theme.accent,
                &theme,
                cx.listener(|this, _, _, cx| this.start_song_workshop(cx)),
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font(theme.font.clone())
            .text_sm()
            .track_focus(&self.focus)
            .key_context(crate::actions::KEY_CONTEXT)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(state) = &mut this.song_workshop
                    && !state.busy()
                    && state.brief_focus.is_focused(window)
                {
                    if state.brief.marked().is_some() {
                        return;
                    }
                    if state.brief.apply_key_with_clipboard(
                        &event.keystroke.key,
                        event.keystroke.modifiers.shift,
                        event.keystroke.modifiers.secondary(),
                        false,
                        cx,
                    ) != KeyEffect::Ignored
                    {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                }
                if event.keystroke.key == "escape" {
                    this.song_workshop = None;
                    this.session.stop_output_preview();
                    this.focus.focus(window);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusNextPane, window, _| window.focus_next()),
            )
            .on_action(
                cx.listener(|_, _: &crate::actions::FocusPreviousPane, window, _| {
                    window.focus_prev()
                }),
            )
            .on_action(cx.listener(|this, _: &crate::actions::Quit, window, cx| {
                this.quit_asking(window, cx)
            }))
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
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn chooser_workshop_back_keeps_the_project_and_cancels_work(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| app.update(cx, |app, cx| app.show_project_chooser(window, cx)));
        let before = app.read_with(cx, |app, _| app.session.project().clone());
        crate::harness::paint(&app, cx);
        crate::harness::click("chooser-ai", cx);
        crate::harness::paint(&app, cx);
        crate::harness::click("song-brief", cx);
        cx.simulate_input("夕暮れの海辺のインスト曲");
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.song_workshop.as_ref().unwrap().brief.content(),
                "夕暮れの海辺のインスト曲"
            )
        });
        let cancel = app.read_with(cx, |app, _| {
            app.song_workshop.as_ref().unwrap().cancel.clone()
        });
        crate::harness::click("song-back", cx);
        app.read_with(cx, |app, _| {
            assert!(app.song_workshop.is_none());
            assert!(app.project_chooser.is_some());
            assert_eq!(app.session.project(), &before);
        });
        assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
    }
}
