//! Draft controls; no field mutates the open project or agent conversation.

use super::*;
use crate::ui::context_menu::{ContextMenu, MenuCommand};
use crate::ui::widgets::{SliderFill, SliderKeyboardEvent, value_slider};

impl SongWorkshop {
    fn text(&self, index: usize) -> &TextField {
        if index == 0 {
            &self.brief
        } else {
            &self.fields[index - 1]
        }
    }

    fn text_mut(&mut self, index: usize) -> &mut TextField {
        if index == 0 {
            &mut self.brief
        } else {
            &mut self.fields[index - 1]
        }
    }

    pub(crate) fn field(&self) -> Option<&TextField> {
        if self.busy() {
            return None;
        }
        self.active_field.map(|index| self.text(index))
    }

    pub(crate) fn field_mut(&mut self) -> Option<&mut TextField> {
        if self.busy() {
            return None;
        }
        self.active_field.map(|index| self.text_mut(index))
    }

    pub(super) fn request(
        &self,
        plugin_paths: Vec<std::path::PathBuf>,
        japanese_dictionary: Option<std::path::PathBuf>,
    ) -> SongRequest {
        SongRequest {
            seconds: (!self.automatic_length).then_some(self.seconds),
            lyrics: if self.vocals {
                self.fields[2].content().to_owned()
            } else {
                String::new()
            },
            singers: if self.vocals {
                self.singers.clone()
            } else {
                Vec::new()
            },
            singing: self.singing,
            sources: self
                .parts
                .iter()
                .filter(|p| !p.instrument.is_empty() || p.source.is_some())
                .map(|p| SongSourceChoice {
                    role: p.role.name().into(),
                    instrument: p.instrument.clone(),
                    source: p.source.clone(),
                })
                .collect(),
            plugin_paths,
            japanese_dictionary,
        }
    }

    pub(super) fn direction(&self) -> String {
        format!(
            "Genre: {}\nMood: {}\nDirection: {}\nDuration: {}\nVocals: {}\nSingers: {}\nLyrics: {}\nInstruments: {}",
            self.fields[0].content(),
            self.fields[1].content(),
            self.brief.content(),
            if self.automatic_length {
                "automatic".into()
            } else {
                format!("{} seconds", self.seconds)
            },
            self.vocals,
            self.singers
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            if self.vocals {
                self.fields[2].content()
            } else {
                ""
            },
            self.parts
                .iter()
                .filter(|p| !p.instrument.is_empty() || p.source.is_some())
                .map(|p| format!("{}: {}", p.role.name(), p.instrument))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl AurisApp {
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

    pub(crate) fn add_workshop_singer(&mut self, singer: SongSinger) {
        if let Some(s) = &mut self.song_workshop
            && !s.busy()
        {
            s.singers.push(singer);
        }
    }

    pub(crate) fn choose_workshop_speaker(&mut self, index: usize, speaker: String) {
        if let Some(s) = &mut self.song_workshop
            && !s.busy()
            && let Some(voice) = s.singers.get_mut(index)
        {
            voice.speaker = Some(speaker);
        }
    }

    pub(crate) fn choose_workshop_voicevox(
        &mut self,
        index: usize,
        connection: &auris_session::VoicevoxConnection,
        choice: &auris_session::VoicevoxSpeakerChoice,
    ) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if s.busy() {
            return;
        }
        let Some(voice) = s.singers.get_mut(index) else {
            return;
        };
        match self.session.register_composed_voicevox_speaker(
            &voice.path,
            voice.speaker.as_deref(),
            connection,
            choice,
        ) {
            Ok(()) => voice.speaker = Some(choice.name.clone()),
            Err(error) => s.error = Some(error.to_string()),
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

    fn workshop_singer_menu(&mut self, at: gpui::Point<gpui::Pixels>) {
        let mut menu = ContextMenu::new(at, self.t(Key::SingerVoiceLabel));
        for (name, path) in auris_session::library::voices_with_settings(&self.settings) {
            menu = menu.item(
                name.clone(),
                MenuCommand::WorkshopSinger(SongSinger {
                    name,
                    path,
                    speaker: None,
                }),
            );
        }
        menu = menu.separator().item(
            self.t(Key::WorkshopVoiceSetup),
            MenuCommand::WorkshopVoiceSetup,
        );
        self.open_menu(menu);
    }

    fn workshop_speaker_menu(
        &mut self,
        index: usize,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(voice) = self
            .song_workshop
            .as_ref()
            .and_then(|s| s.singers.get(index))
            .cloned()
        else {
            return;
        };
        let mut menu = ContextMenu::new(at, self.t(Key::SingerSpeakerLabel));
        match self.session.voice_speakers_at(&voice.path) {
            Ok(speakers) => {
                for speaker in speakers {
                    menu = menu.toggle(
                        speaker.clone(),
                        MenuCommand::WorkshopSpeaker(index, speaker.clone()),
                        voice.speaker.as_ref() == Some(&speaker),
                    );
                }
            }
            Err(error) => {
                if let Some(s) = &mut self.song_workshop {
                    s.error = Some(error.to_string());
                }
            }
        }
        if auris_session::BackendKind::from_path(&voice.path)
            == Some(auris_session::BackendKind::Voicevox)
        {
            match self
                .session
                .voicevox_connection_at(&voice.path, voice.speaker.as_deref())
            {
                Ok(connection) => {
                    self.voicevox_menu_generation = self.voicevox_menu_generation.wrapping_add(1);
                    let serial = self.voicevox_menu_generation;
                    let identity = self.song_workshop.as_ref().map(|s| s.identity);
                    menu = menu.separator().item_greyed_unless(
                        false,
                        self.t(Key::VoiceSetupChecking),
                        MenuCommand::WorkshopVoiceSetup,
                    );
                    menu.async_request = Some(serial);
                    cx.spawn(async move |this, cx| {
                        let url = connection.url.clone();
                        let result = cx
                            .background_executor()
                            .spawn(async move { auris_session::fetch_voicevox_catalog(&url) })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.menu.as_ref().and_then(|m| m.async_request) != Some(serial)
                                || this.song_workshop.as_ref().map(|s| s.identity) != identity
                                || this
                                    .song_workshop
                                    .as_ref()
                                    .and_then(|s| s.singers.get(index))
                                    != Some(&voice)
                            {
                                return;
                            }
                            match result {
                                Ok(catalog) => {
                                    let mut menu =
                                        ContextMenu::new(at, this.t(Key::SingerSpeakerLabel));
                                    let connection = Arc::new(connection);
                                    for choice in catalog.speaker_choices(&connection) {
                                        let checked = voice.speaker.as_ref() == Some(&choice.name);
                                        menu = menu.toggle(
                                            choice.name.clone(),
                                            MenuCommand::WorkshopVoicevox {
                                                index,
                                                connection: connection.clone(),
                                                choice,
                                            },
                                            checked,
                                        );
                                    }
                                    this.open_menu(menu);
                                }
                                Err(error) => {
                                    if let Some(s) = &mut this.song_workshop {
                                        s.error = Some(error.to_string());
                                        this.close_menu();
                                    }
                                }
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                }
                Err(error) => {
                    if let Some(s) = &mut self.song_workshop {
                        s.error = Some(error.to_string());
                    }
                }
            }
        }
        self.open_menu(menu);
        cx.notify();
    }

    fn generate_workshop_lyrics(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        if s.busy() {
            return;
        }
        match auris_agent::song_ideas::SongPlanner::start_lyrics(
            s.preferences.clone(),
            s.fields[3].content(),
            &s.direction(),
            (!s.automatic_length).then_some(s.seconds),
        ) {
            Ok(planner) => {
                s.lyric_planner = Some(planner);
                s.active_field = None;
                s.error = None;
            }
            Err(error) => s.error = Some(error),
        }
        cx.notify();
    }

    fn workshop_field(
        &self,
        index: usize,
        label: Key,
        multiline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.song_workshop.as_ref().expect("visible workshop");
        let text = s.text(index);
        let focus = s.foci[index].clone();
        let theme = self.theme.clone();
        let active = s.active_field == Some(index);
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
            .flex_shrink_0()
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .bg(theme.surface)
            .when(s.busy(), |d| d.opacity(0.65))
            .focus(|d| d.border_color(theme.selection))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && !s.busy()
                    {
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
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.t(label))
            .child(field)
            .into_any_element()
    }

    pub(super) fn render_workshop_form(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.song_workshop.as_ref().expect("visible workshop");
        let busy = s.busy();
        let step = s.step;
        let theme = self.theme.clone();
        let state = |selected| {
            if busy {
                ButtonState::Disabled
            } else {
                ButtonState::Enabled(if selected { Latch::On } else { Latch::Off })
            }
        };
        let model = s
            .models
            .iter()
            .find(|m| m.matches(&s.preferences))
            .map(|m| m.label())
            .unwrap_or_else(|| format!("{} / {}", s.preferences.backend, s.preferences.model));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_lg().child(self.t(Key::WorkshopTitle)))
            .child(
                div()
                    .text_color(theme.text_muted)
                    .child(self.t(Key::WorkshopHint)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(button_enabled(
                        "workshop-model",
                        format!("{model} ▾"),
                        ButtonStyle::Normal,
                        state(
                            self.menu
                                .as_ref()
                                .is_some_and(|m| m.title == self.t(Key::AgentModelLabel)),
                        ),
                        theme.accent_soft,
                        &theme,
                        cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                            this.workshop_model_menu(event.position());
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "workshop-model-settings",
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
        if s.model_receiver.is_some() {
            body = body.child(self.t(Key::WorkshopLoadingModels));
        }
        if let Some(error) = &s.model_error {
            body = body.child(div().text_color(theme.danger).child(error.clone()));
        }
        let mut steps = div().flex().gap_2().flex_wrap();
        for (index, key) in [
            Key::WorkshopDirection,
            Key::WorkshopVocals,
            Key::WorkshopSources,
            Key::WorkshopCompare,
        ]
        .into_iter()
        .enumerate()
        {
            if index == 1 && !s.vocals {
                continue;
            }
            steps = steps.child(button_enabled(
                ("workshop-step", index),
                self.t(key),
                ButtonStyle::Normal,
                state(index == step),
                theme.accent_soft,
                &theme,
                cx.listener(move |this, _, window, cx| {
                    if let Some(s) = &mut this.song_workshop
                        && !s.busy()
                    {
                        s.step = index;
                        s.active_field = None;
                        this.focus.focus(window);
                    }
                    cx.notify();
                }),
            ));
        }
        body = body.child(steps);
        match step {
            0 => {
                body = body.child(self.workshop_field(1, Key::WorkshopGenre, false, cx));
                let mut genres = div().flex().gap_2().flex_wrap();
                for (index, key) in GENRE_LABELS.iter().copied().enumerate() {
                    let label = self.t(key).to_string();
                    genres = genres.child(button_enabled(
                        ("song-genre", index),
                        label.clone(),
                        ButtonStyle::Normal,
                        state(s.fields[0].content() == label),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.fields[0] = TextField::new(label.clone());
                            }
                            cx.notify();
                        }),
                    ));
                }
                body =
                    body.child(genres)
                        .child(self.workshop_field(2, Key::WorkshopMood, false, cx));
                let mut moods = div().flex().gap_2().flex_wrap();
                for (index, key) in MOODS.iter().copied().enumerate() {
                    let label = self.t(key).to_string();
                    moods = moods.child(button_enabled(
                        ("song-mood", index),
                        label.clone(),
                        ButtonStyle::Normal,
                        state(s.fields[1].content() == label),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.fields[1] = TextField::new(label.clone());
                            }
                            cx.notify();
                        }),
                    ));
                }
                body =
                    body.child(moods)
                        .child(self.workshop_field(0, Key::WorkshopBrief, true, cx));
                let mut length = div().flex().gap_2();
                for (automatic, key) in [
                    (true, Key::WorkshopAutomatic),
                    (false, Key::WorkshopSpecifyLength),
                ] {
                    length = length.child(button_enabled(
                        ("workshop-auto", usize::from(automatic)),
                        self.t(key),
                        ButtonStyle::Normal,
                        state(s.automatic_length == automatic),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.automatic_length = automatic;
                            }
                            cx.notify();
                        }),
                    ));
                }
                body = body
                    .child(div().child(self.t(Key::WorkshopLength)))
                    .child(length);
                if !s.automatic_length {
                    body = body.child(value_slider(
                        "workshop-length",
                        self.t(Key::WorkshopLength),
                        format!("{}:{:02}", s.seconds / 60, s.seconds % 60),
                        (s.seconds - 30) as f32 / 270.0,
                        theme.accent,
                        SliderFill::FromStart,
                        10.0 / 270.0,
                        &theme,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, _| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.drag_length = Some((event.position.x, s.seconds));
                            }
                        }),
                        cx.listener(|this, event: &SliderKeyboardEvent, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.seconds = 30 + (event.fraction * 27.0).round() as u32 * 10;
                            }
                            cx.notify();
                        }),
                    ));
                }
                let mut vocals = div().flex().gap_2();
                for (value, key) in [
                    (false, Key::WorkshopInstrumental),
                    (true, Key::WorkshopWithVocals),
                ] {
                    vocals = vocals.child(button_enabled(
                        ("workshop-vocals", usize::from(value)),
                        self.t(key),
                        ButtonStyle::Normal,
                        state(s.vocals == value),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.vocals = value;
                            }
                            cx.notify();
                        }),
                    ));
                }
                body = body.child(vocals);
            }
            1 => {
                for (index, voice) in s.singers.iter().enumerate() {
                    body = body.child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(format!("{} · {}", index + 1, voice.name))
                            .child(button_enabled(
                                ("workshop-speaker", index),
                                format!(
                                    "{} ▾",
                                    voice
                                        .speaker
                                        .as_deref()
                                        .unwrap_or(self.t(Key::WorkshopAutomatic))
                                ),
                                ButtonStyle::Normal,
                                state(false),
                                theme.accent_soft,
                                &theme,
                                cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                    this.workshop_speaker_menu(index, event.position(), cx)
                                }),
                            ))
                            .child(button_enabled(
                                ("workshop-remove-singer", index),
                                self.t(Key::WorkshopRemoveSinger),
                                ButtonStyle::Normal,
                                state(false),
                                theme.accent_soft,
                                &theme,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(s) = &mut this.song_workshop
                                        && !s.busy()
                                        && index < s.singers.len()
                                    {
                                        s.singers.remove(index);
                                    }
                                    cx.notify();
                                }),
                            )),
                    );
                }
                body = body.child(button_enabled(
                    "workshop-add-singer",
                    self.t(Key::WorkshopAddSinger),
                    ButtonStyle::Normal,
                    state(false),
                    theme.accent_soft,
                    &theme,
                    cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        this.workshop_singer_menu(event.position());
                        cx.notify();
                    }),
                ));
                if s.singers.len() > 1 {
                    let mut modes = div().flex().gap_2();
                    for (index, mode, key) in [
                        (0usize, SongSinging::Auto, Key::WorkshopAutomatic),
                        (1, SongSinging::Alternate, Key::WorkshopAlternate),
                        (2, SongSinging::Harmony, Key::WorkshopHarmony),
                    ] {
                        modes = modes.child(button_enabled(
                            ("workshop-singing", index),
                            self.t(key),
                            ButtonStyle::Normal,
                            state(s.singing == mode),
                            theme.accent_soft,
                            &theme,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(s) = &mut this.song_workshop
                                    && !s.busy()
                                {
                                    s.singing = mode;
                                }
                                cx.notify();
                            }),
                        ));
                    }
                    body = body.child(modes);
                }
                let mut modes = div().flex().gap_2();
                for (value, key) in [
                    (false, Key::WorkshopWriteLyrics),
                    (true, Key::WorkshopMotifLyrics),
                ] {
                    modes = modes.child(button_enabled(
                        ("workshop-lyric-mode", usize::from(value)),
                        self.t(key),
                        ButtonStyle::Normal,
                        state(s.lyric_mode == value),
                        theme.accent_soft,
                        &theme,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop
                                && !s.busy()
                            {
                                s.lyric_mode = value;
                            }
                            cx.notify();
                        }),
                    ));
                }
                body = body.child(modes);
                if s.lyric_mode {
                    body = body
                        .child(self.workshop_field(4, Key::WorkshopMotif, true, cx))
                        .child(button_enabled(
                            "workshop-generate-lyrics",
                            self.t(Key::WorkshopGenerateLyrics),
                            ButtonStyle::Normal,
                            state(false),
                            theme.accent,
                            &theme,
                            cx.listener(|this, _, _, cx| this.generate_workshop_lyrics(cx)),
                        ));
                }
                body = body.child(self.workshop_field(3, Key::WorkshopLyrics, true, cx));
                if s.lyric_planner.is_some() {
                    body = body.child(self.t(Key::WorkshopWritingLyrics));
                }
            }
            2 => {
                body = body.child(self.t(Key::WorkshopSourceHint));
                for (index, part) in s.parts.iter().enumerate() {
                    let name = if let Some(source) = &part.source {
                        self.song_library_source_label(source)
                    } else if part.instrument.is_empty() {
                        self.t(Key::WorkshopAutomatic).to_string()
                    } else {
                        self.plugin_label(&part.instrument)
                    };
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w_32().child(self.t(role_label(part.role))))
                            .child(button_enabled(
                                ("workshop-source", index),
                                format!("{name}…"),
                                ButtonStyle::Normal,
                                state(false),
                                theme.accent_soft,
                                &theme,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(s) = &mut this.song_workshop
                                        && !s.busy()
                                    {
                                        s.active_field = None;
                                        if let Some(part) = s.parts.get(index) {
                                            let mut browser = crate::ui::library::SongLibrary::new(
                                                part.name.clone(),
                                            );
                                            browser.focused = true;
                                            this.song_library = Some(browser);
                                        }
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(button_enabled(
                                ("workshop-source-auto", index),
                                self.t(Key::WorkshopAutomatic),
                                ButtonStyle::Normal,
                                state(false),
                                theme.accent_soft,
                                &theme,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(s) = &mut this.song_workshop
                                        && !s.busy()
                                        && let Some(p) = s.parts.get_mut(index)
                                    {
                                        p.instrument.clear();
                                        p.source = None;
                                    }
                                    cx.notify();
                                }),
                            )),
                    );
                }
            }
            _ => {
                body = body.child(self.t(Key::WorkshopCompareHint));
            }
        }
        body.into_any_element()
    }

    pub(super) fn workshop_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_key(event, window, cx) || self.song_library_key(event, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if let Some(s) = &mut self.song_workshop
            && !s.busy()
            && let Some(index) = s.active_field
            && s.foci[index].is_focused(window)
        {
            if s.text(index).marked().is_some() {
                return;
            }
            let multiline = matches!(index, 0 | 3 | 4);
            if event.keystroke.key == "enter" && multiline {
                s.text_mut(index).insert("\n");
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if event.keystroke.key == "escape" {
                s.active_field = None;
                self.focus.focus(window);
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if s.text_mut(index).apply_key_with_clipboard(
                &event.keystroke.key,
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.secondary(),
                multiline,
                cx,
            ) != KeyEffect::Ignored
            {
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        if event.keystroke.key == "escape" {
            if let Some(s) = &mut self.song_workshop
                && s.busy()
            {
                s.stop();
            } else {
                self.song_workshop = None;
            }
            self.session.stop_output_preview();
            self.focus.focus(window);
            cx.stop_propagation();
            cx.notify();
        }
    }
}

fn role_label(role: Role) -> Key {
    match role {
        Role::Melody => Key::WorkshopMelody,
        Role::Chords => Key::WorkshopChords,
        Role::Bass => Key::WorkshopBass,
        Role::Pad => Key::WorkshopPad,
        Role::Kick => Key::WorkshopKick,
        Role::Snare => Key::WorkshopSnare,
        _ => Key::WorkshopHat,
    }
}
