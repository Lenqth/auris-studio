//! New-song directions, isolated generation, audition and exact adoption.

use super::audio_player::{PlayerState, PlayerTarget};
use super::text_field::{KeyEffect, TextField};
use super::widgets::{ButtonState, ButtonStyle, Latch, button, button_enabled};
use crate::app::{AurisApp, Pane};
use auris_i18n::{Key, messages};
use auris_session::prelude::{PartSpec, Role};
use auris_session::{
    SongCandidate, SongIdea, SongRequest, SongSinger, SongSinging, SongSourceChoice,
};
mod form;
use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window, div, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

enum CandidateEvent {
    Progress(usize, String),
    Complete(usize, Result<Box<SongCandidate>, String>),
}

pub(crate) struct SongWorkshop {
    identity: u64,
    pub(crate) brief: TextField,
    fields: [TextField; 4],
    foci: [FocusHandle; 5],
    active_field: Option<usize>,
    _focus_subscriptions: Vec<gpui::Subscription>,
    step: usize,
    automatic_length: bool,
    vocals: bool,
    lyric_mode: bool,
    singers: Vec<SongSinger>,
    singing: SongSinging,
    pub(crate) parts: Vec<PartSpec>,
    preferences: auris_session::AgentPreferences,
    models: Vec<super::agent_chat::ModelOption>,
    model_receiver: Option<mpsc::Receiver<Result<String, String>>>,
    model_error: Option<String>,
    lyric_planner: Option<auris_agent::song_ideas::SongPlanner>,
    captured_request: SongRequest,
    drag_length: Option<(gpui::Pixels, u32)>,
    seconds: u32,
    planner: Option<auris_agent::song_ideas::SongPlanner>,
    receiver: Option<mpsc::Receiver<CandidateEvent>>,
    progress: [String; 3],
    cancel: Arc<AtomicBool>,
    ideas: Option<[SongIdea; 3]>,
    /// Shared so a draft can be written as a project of its own off the window thread; its
    /// temporary audio lives exactly as long as the last holder.
    candidates: [Option<Arc<SongCandidate>>; 3],
    /// Where each draft, as it stands now, was last kept as a project.
    saved: [Option<std::path::PathBuf>; 3],
    errors: [Option<String>; 3],
    pending: [bool; 3],
    error: Option<String>,
    captured_brief: String,
    preview_serial: u64,
    pub(crate) players: [PlayerState; 3],
    preparing: Option<usize>,
}

impl Drop for SongWorkshop {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl SongWorkshop {
    pub(crate) fn busy(&self) -> bool {
        self.planner.is_some() || self.lyric_planner.is_some() || self.receiver.is_some()
    }
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.planner = None;
        self.lyric_planner = None;
        self.receiver = None;
        self.pending = [false; 3];
        self.preview_serial = self.preview_serial.wrapping_add(1);
        self.preparing = None;
        for player in &mut self.players {
            player.stop();
        }
    }
    fn generate(&mut self, indices: Vec<usize>) {
        let Some(ideas) = self.ideas.clone() else {
            return;
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let request = self.captured_request.clone();
        let brief = self.captured_brief.clone();
        let (sender, receiver) = mpsc::channel();
        for &index in &indices {
            self.players[index].reset_audio();
            self.pending[index] = true;
            self.progress[index] = String::new();
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
                    let result = SongCandidate::generate_with(
                        ideas[index].clone(),
                        &brief,
                        &request,
                        seed.wrapping_add(index as u64),
                        &cancel,
                        &mut |stage| {
                            let _ = sender.send(CandidateEvent::Progress(index, stage.to_owned()));
                        },
                    )
                    .map(Box::new);
                    if cancel.load(Ordering::Acquire)
                        || sender
                            .send(CandidateEvent::Complete(index, result))
                            .is_err()
                    {
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
        let foci: [FocusHandle; 5] = std::array::from_fn(|_| cx.focus_handle().tab_stop(true));
        let subscriptions = foci
            .iter()
            .enumerate()
            .map(|(index, focus)| {
                cx.on_focus(focus, window, move |this, _, cx| {
                    if let Some(state) = &mut this.song_workshop {
                        state.active_field = Some(index);
                    }
                    cx.notify();
                })
            })
            .collect();
        let preferences = self.settings.agent.clone();
        let model_receiver = Some(auris_agent::discover_models_background(preferences.clone()));
        self.focus.focus(window);
        self.song_workshop = Some(SongWorkshop {
            identity: {
                static NEXT: AtomicU64 = AtomicU64::new(1);
                NEXT.fetch_add(1, Ordering::Relaxed)
            },
            brief: TextField::default(),
            fields: [
                TextField::new(self.t(Key::WorkshopPop)),
                TextField::new(self.t(Key::WorkshopBright)),
                TextField::default(),
                TextField::default(),
            ],
            foci,
            active_field: None,
            _focus_subscriptions: subscriptions,
            step: 0,
            automatic_length: true,
            vocals: false,
            lyric_mode: false,
            singers: Vec::new(),
            singing: SongSinging::Auto,
            parts: [
                Role::Melody,
                Role::Chords,
                Role::Bass,
                Role::Pad,
                Role::Kick,
                Role::Snare,
                Role::Hat,
            ]
            .into_iter()
            .map(|role| {
                let mut part = PartSpec::of_role(role.name(), role);
                part.instrument.clear();
                part
            })
            .collect(),
            preferences,
            models: Vec::new(),
            model_receiver,
            model_error: None,
            lyric_planner: None,
            captured_request: SongRequest::default(),
            drag_length: None,
            seconds: 60,
            planner: None,
            receiver: None,
            progress: std::array::from_fn(|_| String::new()),
            cancel: Arc::new(AtomicBool::new(false)),
            ideas: None,
            candidates: std::array::from_fn(|_| None),
            saved: std::array::from_fn(|_| None),
            errors: std::array::from_fn(|_| None),
            pending: [false; 3],
            error: None,
            captured_brief: String::new(),
            preview_serial: 0,
            players: std::array::from_fn(|_| PlayerState::default()),
            preparing: None,
        });
        cx.notify();
    }

    fn start_song_workshop(&mut self, cx: &mut Context<Self>) {
        let missing_singer = self.t(Key::WorkshopNeedSinger).to_string();
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        if state.busy() {
            return;
        }
        let request = state.request(
            self.settings.plugin_paths.clone(),
            self.session
                .japanese_dictionary()
                .map(std::path::Path::to_path_buf),
        );
        if state.vocals && state.singers.is_empty() {
            state.step = 1;
            state.error = Some(missing_singer);
            cx.notify();
            return;
        }
        if let Err(error) = request.validate() {
            state.step = if state.vocals { 1 } else { 0 };
            state.error = Some(error);
            cx.notify();
            return;
        }
        state.stop();
        state.active_field = None;
        let brief = state.direction();
        state.captured_request = request;
        match auris_agent::song_ideas::SongPlanner::start(
            state.preferences.clone(),
            &brief,
            state.seconds,
        ) {
            Ok(planner) => {
                state.planner = Some(planner);
                state.step = 3;
                state.captured_brief = brief;
                state.candidates = std::array::from_fn(|_| None);
                state.saved = std::array::from_fn(|_| None);
                state.errors = std::array::from_fn(|_| None);
                state.ideas = None;
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn poll_song_workshop(&mut self, cx: &mut Context<Self>) {
        let model_menu_title = self.t(Key::AgentModelLabel);
        let Some(state) = &mut self.song_workshop else {
            return;
        };
        if let Some(receiver) = &state.model_receiver {
            match receiver.try_recv() {
                Ok(answer) => {
                    state.model_receiver = None;
                    match answer.and_then(|text| super::agent_chat::parse_model_list(&text)) {
                        Ok(models) => {
                            // Menu commands index the catalog that was visible when opened.
                            if self
                                .menu
                                .as_ref()
                                .is_some_and(|m| m.title == model_menu_title)
                            {
                                self.menu = None;
                            }
                            state.models = models;
                            state.model_error = None;
                        }
                        Err(error) => state.model_error = Some(error),
                    }
                    cx.notify();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    state.model_receiver = None;
                    state.model_error = Some("Model discovery stopped".into());
                    cx.notify();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(result) = state.lyric_planner.as_mut().and_then(|p| p.poll_lyrics()) {
            state.lyric_planner = None;
            match result {
                Ok(text) => state.fields[2] = TextField::new(text),
                Err(error) => state.error = Some(error),
            }
            cx.notify();
        }
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
                    Ok(CandidateEvent::Progress(index, stage)) => {
                        state.progress[index] = stage;
                        cx.notify();
                    }
                    Ok(CandidateEvent::Complete(index, result)) => {
                        state.pending[index] = false;
                        match result {
                            Ok(candidate) => {
                                state.candidates[index] = Some(Arc::from(candidate));
                                state.saved[index] = None;
                            }
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

    pub(crate) fn preview_song_candidate(&mut self, index: usize, cx: &mut Context<Self>) {
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
        for player in &mut state.players {
            player.stop();
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
                match prepared.and_then(|audio| {
                    let options = state.players[index].options(audio.duration_seconds());
                    this.session.play_output_preview_with(audio, options)
                }) {
                    Ok(status) => state.players[index].start(status),
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

    /// Asks where to keep one draft as a project of its own, without adopting it.
    fn save_song_candidate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &self.song_workshop else {
            return;
        };
        let Some(candidate) = state.candidates[index].clone() else {
            return;
        };
        let identity = state.identity;
        let language = self.language();
        let view = cx.entity().downgrade();
        window
            .spawn(cx, async move |cx| {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title(Key::DialogSaveProject.get(language))
                    .set_file_name(format!(
                        "{}.{}",
                        candidate.idea.title,
                        auris_session::PROJECT_EXTENSION
                    ))
                    .add_filter(
                        Key::FilterProject.get(language),
                        &[auris_session::PROJECT_EXTENSION],
                    )
                    .save_file()
                    .await;
                let Some(handle) = handle else { return };
                let path = handle.path().to_path_buf();
                let _ = view.update(cx, |this, cx| {
                    this.finish_save_song_candidate(identity, index, candidate, path, cx);
                });
            })
            .detach();
    }

    /// Writes a draft at the chosen path off the window thread and marks its card.
    pub(crate) fn finish_save_song_candidate(
        &mut self,
        identity: u64,
        index: usize,
        candidate: Arc<SongCandidate>,
        path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let kept = Arc::clone(&candidate);
            let saved = cx
                .background_executor()
                .spawn(async move { kept.save_as(&path) })
                .await;
            let _ = view.update(cx, |this, cx| {
                let failure = saved
                    .as_ref()
                    .err()
                    .map(|error| this.failure(Key::WorkshopSave, error));
                // A workshop closed, restarted or regenerated meanwhile is not the one asked.
                let Some(state) = this
                    .song_workshop
                    .as_mut()
                    .filter(|state| state.identity == identity)
                else {
                    return;
                };
                if !state.candidates[index]
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &candidate))
                {
                    return;
                }
                match saved {
                    Ok(report) => {
                        state.saved[index] = Some(report.document);
                        state.errors[index] = None;
                    }
                    Err(_) => state.errors[index] = failure,
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn render_song_workshop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let form = self.render_workshop_form(window, cx);
        let library = self
            .render_song_library_overlay(window, cx)
            .map(IntoElement::into_any_element);
        let menu = self
            .render_context_menu(window, cx)
            .map(IntoElement::into_any_element);
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
        let mut body = div()
            .id("song-workshop-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(form);
        if let Some(error) = &state.error {
            body = body.child(div().text_color(theme.danger).child(error.clone()));
        }
        if state.planner.is_some() {
            body = body.child(div().child(self.t(Key::WorkshopPlanning)));
        }
        if let Some(ideas) = &state.ideas
            && state.step == 3
        {
            let mut cards = div().flex().flex_wrap().gap_3();
            for (index, idea) in ideas.iter().enumerate() {
                let ready = state.candidates[index].is_some();
                let mut card = div()
                    .flex_1()
                    .min_w(gpui::rems(22.0))
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
                if let Some(candidate) = &state.candidates[index] {
                    card = card.child(candidate.part_names().join(" · "));
                }
                if state.pending[index] {
                    card = card.child(self.t(match state.progress[index].as_str() {
                        "sing" => Key::WorkshopSinging,
                        "compose" => Key::WorkshopComposing,
                        "" => Key::WorkshopQueued,
                        _ => Key::WorkshopRendering,
                    }));
                }
                if state.preparing == Some(index) {
                    card = card.child(self.t(Key::WorkshopPreparing));
                }
                if let Some(error) = &state.errors[index] {
                    card = card.child(div().text_color(theme.danger).child(error.clone()));
                }
                if let Some(saved) = &state.saved[index] {
                    card = card.child(div().text_color(theme.text_muted).child(messages::saved(
                        self.language(),
                        &saved.display().to_string(),
                    )));
                }
                card = card
                    .child(
                        self.render_audio_player(
                            PlayerTarget::Workshop(index),
                            state.candidates[index]
                                .as_ref()
                                .map_or(0.0, |c| c.audio.duration_seconds()),
                            ready && !state.pending[index] && state.preparing.is_none(),
                            state.preparing == Some(index),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
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
                                ("song-save", index),
                                self.t(Key::WorkshopSave),
                                ButtonStyle::Normal,
                                enabled(ready && !state.pending[index]),
                                theme.accent,
                                &theme,
                                cx.listener(move |this, _, window, cx| {
                                    this.save_song_candidate(index, window, cx)
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
                    if let Some(s) = &mut this.song_workshop
                        && s.step > 0
                        && !s.busy()
                    {
                        s.stop();
                        s.step = if s.step == 2 && !s.vocals {
                            0
                        } else {
                            s.step - 1
                        };
                        s.active_field = None;
                    } else {
                        this.song_workshop = None;
                    }
                    this.song_library = None;
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
                self.t(if state.step < 2 {
                    Key::WorkshopNext
                } else {
                    Key::WorkshopGenerate
                }),
                ButtonStyle::Primary,
                enabled(!busy && (state.step < 2 || state.preferences.is_configured())),
                theme.accent,
                &theme,
                cx.listener(|this, _, window, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && s.step < 2
                    {
                        s.step = if s.step == 0 && !s.vocals {
                            2
                        } else {
                            s.step + 1
                        };
                        s.active_field = None;
                        this.focus.focus(window);
                        cx.notify();
                    } else {
                        this.start_song_workshop(cx);
                    }
                }),
            ));
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
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.move_audio_player(event, cx) {
                    return;
                }
                if let Some(s) = &mut this.song_workshop
                    && let Some((x, seconds)) = s.drag_length
                {
                    if event.pressed_button != Some(gpui::MouseButton::Left) {
                        s.drag_length = None;
                        return;
                    }
                    s.seconds = ((seconds as f32 + f32::from(event.position.x - x)) / 10.0)
                        .round()
                        .clamp(3.0, 30.0) as u32
                        * 10;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.audio_player_drag = None;
                    if let Some(s) = &mut this.song_workshop {
                        s.drag_length = None;
                    }
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.audio_player_drag = None;
                    if let Some(s) = &mut this.song_workshop {
                        s.drag_length = None;
                    }
                }),
            )
            .on_key_down(cx.listener(Self::workshop_key))
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
            .children(library)
            .children(menu)
            .children(crate::titlebar::resize_handles(window))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn candidate_players_share_seek_volume_and_back_cancels_preparation(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::*;
        use crate::harness::{click, open, paint};
        let (app, cx) = open(cx);
        cx.update(|window, cx| {
            app.update(cx, |this, cx| {
                this.show_project_chooser(window, cx);
                this.open_song_workshop(window, cx);
            })
        });
        let idea = SongIdea {
            title: "Preview".into(),
            description: "Player controls".into(),
            preset: "chiptune".into(),
            tempo: 120.0,
            key: "C major".into(),
            density: 0.4,
            lead: "piano".into(),
        };
        let candidate =
            SongCandidate::generate(idea.clone(), "", 30, 42, &AtomicBool::new(false)).unwrap();
        let duration = candidate.audio.duration_seconds();
        app.update(cx, |this, _| {
            let state = this.song_workshop.as_mut().unwrap();
            state.step = 3;
            state.ideas = Some(std::array::from_fn(|_| idea.clone()));
            state.candidates[0] = Some(Arc::new(candidate));
            state.model_receiver = None;
        });
        paint(&app, cx);
        click("song-preview-0-seek", cx);
        cx.simulate_keystrokes("end");
        click("song-preview-0-volume", cx);
        cx.simulate_keystrokes("home");
        cx.simulate_keystrokes("pageup");
        app.read_with(cx, |this, _| {
            let state = this.song_workshop.as_ref().unwrap();
            assert_eq!(state.players[0].options(duration).position_seconds, 0.0);
            assert!((state.players[0].options(duration).volume - 0.1).abs() < 1e-6);
            assert_eq!(state.players[1].options(duration).volume, 1.0);
        });
        click("song-preview-0-play", cx);
        click("song-back", cx);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            let state = this.song_workshop.as_ref().unwrap();
            assert!(state.preparing.is_none());
            assert_eq!(state.step, 2);
        });
    }

    #[gpui::test]
    fn workshop_fields_modes_and_source_picker_preserve_the_draft(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.show_project_chooser(window, cx);
                app.open_song_workshop(window, cx);
            })
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("workshop-field-1", cx);
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("和風ジャズ");
        crate::harness::click("workshop-field-2", cx);
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("静かな夜");
        app.update(cx, |this, _| {
            let s = this.song_workshop.as_mut().unwrap();
            s.vocals = true;
            s.step = 1;
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("workshop-field-3", cx);
        cx.simulate_input("さくらさいた");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("はるがきた");
        app.read_with(cx, |this, _| {
            let s = this.song_workshop.as_ref().unwrap();
            assert_eq!(s.fields[0].content(), "和風ジャズ");
            assert_eq!(s.fields[1].content(), "静かな夜");
            assert_eq!(s.fields[2].content(), "さくらさいた\nはるがきた");
        });
        app.update(cx, |this, _| {
            this.song_workshop.as_mut().unwrap().step = 2;
            this.song_library = Some(crate::ui::library::SongLibrary::new("melody".into()));
        });
        crate::harness::paint(&app, cx);
        app.update(cx, |this, _| {
            this.choose_song_library_instrument("auris.physical.violin")
        });
        app.read_with(cx, |this, _| {
            let s = this.song_workshop.as_ref().unwrap();
            assert_eq!(s.parts[0].instrument, "auris.physical.violin");
            assert_eq!(s.fields[2].content(), "さくらさいた\nはるがきた");
            assert!(this.song_library.is_none());
        });
    }

    #[gpui::test]
    fn a_sheet_left_behind_the_chooser_does_not_take_the_workshop_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                // What the recovery offer at launch leaves: a question with no field, open
                // underneath a chooser that draws nothing of it.
                app.open_prompt(crate::ui::prompt::Prompt::ask(
                    "Remove",
                    crate::ui::prompt::Question::RemovePluginPath("plugins".into()),
                ));
                app.show_project_chooser(window, cx);
                app.open_song_workshop(window, cx);
            })
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("song-brief", cx);
        cx.simulate_input("夜の雨");
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.song_workshop.as_ref().unwrap().brief.content(),
                "夜の雨"
            );
            assert!(app.prompt.is_some());
        });
    }

    #[gpui::test]
    fn workshop_model_selection_is_local_and_duration_supports_keyboard(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::harness::open(cx);
        let before = app.read_with(cx, |this, _| this.settings.agent.clone());
        cx.update(|window, cx| {
            app.update(cx, |this, cx| {
                this.show_project_chooser(window, cx);
                this.open_song_workshop(window, cx);
            })
        });
        app.update(cx, |this, _| {
            let s = this.song_workshop.as_mut().unwrap();
            s.models = vec![super::super::agent_chat::ModelOption {
                name: "other-model".into(),
                ..Default::default()
            }];
            s.automatic_length = false;
            s.model_receiver = None;
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("workshop-model", cx);
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        crate::harness::paint(&app, cx);
        crate::harness::click("workshop-length", cx);
        cx.simulate_keystrokes("right");
        app.read_with(cx, |this, _| {
            assert_eq!(this.settings.agent, before);
            assert_eq!(
                this.song_workshop.as_ref().unwrap().preferences.model,
                "other-model"
            );
            assert!(this.song_workshop.as_ref().unwrap().seconds > 60);
        });
    }

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

    #[gpui::test]
    fn an_unchosen_draft_is_kept_as_a_project_without_touching_the_open_one(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = crate::harness::open(cx);
        cx.update(|window, cx| app.update(cx, |app, cx| app.open_song_workshop(window, cx)));
        let candidate = Arc::new(
            SongCandidate::generate(
                SongIdea {
                    title: "Second".into(),
                    description: "Bright keys".into(),
                    preset: "chiptune".into(),
                    tempo: 120.0,
                    key: "C major".into(),
                    density: 0.5,
                    lead: "piano".into(),
                },
                "Bright instrumental",
                30,
                42,
                &AtomicBool::new(false),
            )
            .unwrap(),
        );
        let root = tempfile::tempdir().unwrap();
        let chosen = root.path().join("Second.auris");
        let before = app.read_with(cx, |app, _| app.session.project().clone());
        let identity = app.update(cx, |app, _| {
            let state = app.song_workshop.as_mut().unwrap();
            state.candidates[1] = Some(Arc::clone(&candidate));
            state.identity
        });
        for _ in 0..2 {
            let (candidate, chosen) = (Arc::clone(&candidate), chosen.clone());
            app.update(cx, |app, cx| {
                app.finish_save_song_candidate(identity, 1, candidate, chosen, cx)
            });
            cx.run_until_parked();
        }
        let document = root.path().join("Second/Second.auris");
        assert!(document.is_file());
        app.read_with(cx, |app, _| {
            let state = app.song_workshop.as_ref().unwrap();
            assert_eq!(state.saved[1].as_deref(), Some(document.as_path()));
            // The second save named the same folder, and was refused rather than replacing it.
            assert!(state.errors[1].is_some());
            assert!(state.saved[0].is_none() && state.saved[2].is_none());
            assert_eq!(app.session.project(), &before);
        });
    }
}
