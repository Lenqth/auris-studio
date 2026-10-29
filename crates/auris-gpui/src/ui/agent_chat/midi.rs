//! Inline score drafts; only immutable, explicitly attached snapshots reach a worker.

use super::*;
use auris_session::midi_reference::{MAX_REFERENCE_NOTES, MidiReference};
use auris_session::prelude::{Note, TICKS_PER_QUARTER, Ticks, midi_name};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};

const STEP: i64 = TICKS_PER_QUARTER / 2;
const PAGE: i64 = 8 * TICKS_PER_QUARTER;

pub(crate) struct MidiDraft {
    score: MidiReference,
    replacing: Option<PathBuf>,
    track: usize,
    selected: Option<usize>,
    page: i64,
    low_pitch: u8,
    pub(crate) lyric: TextField,
    scroll: gpui::ScrollHandle,
}

impl MidiDraft {
    fn new(score: MidiReference, replacing: Option<PathBuf>) -> Self {
        let mut draft = Self {
            score,
            replacing,
            track: 0,
            selected: None,
            page: 0,
            low_pitch: 54,
            lyric: TextField::default(),
            scroll: gpui::ScrollHandle::new(),
        };
        draft.select(draft.notes().first().map(|_| 0));
        draft
    }

    fn notes(&self) -> &[Note] {
        &self.score.tracks[self.track].notes
    }

    fn flush_lyric(&mut self) {
        if let Some(note) = self
            .selected
            .and_then(|i| self.score.tracks[self.track].notes.get_mut(i))
            && note.lyric != self.lyric.content()
        {
            note.lyric = self.lyric.content().to_owned();
            note.phonemes.clear();
            note.phoneme_seconds.clear();
        }
    }

    fn select(&mut self, selected: Option<usize>) {
        self.flush_lyric();
        self.selected = selected;
        self.lyric = TextField::new(
            selected
                .and_then(|i| self.notes().get(i))
                .map_or("", |n| n.lyric.as_str()),
        );
        self.reveal_selection();
    }

    fn reveal_selection(&mut self) {
        if let Some(note) = self.selected.and_then(|i| self.notes().get(i)) {
            let (pitch, start) = (note.pitch, note.start.0);
            if start < self.page || start >= self.page + PAGE {
                self.page = start / PAGE * PAGE;
            }
            if pitch < self.low_pitch || pitch > self.low_pitch + 12 {
                self.low_pitch = pitch.saturating_sub(6).min(115);
            }
        }
    }

    fn add(&mut self, pitch: u8, at: i64) {
        if self
            .score
            .tracks
            .iter()
            .map(|t| t.notes.len())
            .sum::<usize>()
            >= MAX_REFERENCE_NOTES
        {
            return;
        }
        self.flush_lyric();
        self.score.tracks[self.track]
            .notes
            .push(Note::new(pitch, Ticks(at), Ticks(STEP)));
        self.selected = Some(self.notes().len() - 1);
        self.lyric = TextField::default();
        self.reveal_selection();
    }
}

impl AurisApp {
    pub(crate) fn cancel_midi_for_attachment(&mut self, path: &Path) {
        if self
            .music_references()
            .midi_draft
            .as_ref()
            .is_some_and(|d| d.replacing.as_deref() == Some(path))
        {
            self.music_references_mut().midi_draft = None;
            if let Some(s) = &mut self.song_workshop {
                s.active_field = Some(0);
            } else if self.agent_chat.focused == Some(AgentField::MidiLyric) {
                self.agent_chat.focused = Some(AgentField::Chat);
            }
        }
    }
    pub(crate) fn start_midi_draft(&mut self, from_selection: bool) {
        if self.music_references().midi_draft.is_some()
            || self.music_composer_locked()
            || self.music_references().midi_loading
        {
            return;
        }
        let project = self.session.project();
        let score = if from_selection {
            let Some(clip) = self.selected_clip.and_then(|id| self.session.midi_clip(id)) else {
                return;
            };
            MidiReference::from_clip(clip, project.tempo_map.clone(), project.signatures.clone())
        } else {
            Ok(MidiReference::new(
                self.t(Key::AgentMidiMelody).to_owned(),
                project.tempo_map.clone(),
                project.signatures.clone(),
            ))
        };
        match score {
            Ok(score) => self.music_references_mut().midi_draft = Some(MidiDraft::new(score, None)),
            Err(error) => self.music_references_mut().attachment_error = Some(error),
        }
    }

    pub(crate) fn edit_midi_attachment(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        if self.music_references().midi_draft.is_some()
            || self.music_references().midi_loading
            || self.music_composer_locked()
        {
            return;
        }
        if !path.is_absolute() {
            self.music_references_mut().attachment_error = Some(
                self.t(Key::AgentAttachmentUnavailable)
                    .replace("{path}", &path.to_string_lossy()),
            );
            cx.notify();
            return;
        }
        let from_draft = self.music_references().attachments.contains(&path);
        self.music_references_mut().midi_loading = true;
        let owner = self.music_owner();
        cx.spawn(async move |this, cx| {
            let source = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { MidiReference::read(&source) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.music_owner() != owner {
                    if let super::music::MusicOwner::Agent(project, conversation) = &owner
                        && this.session.path() == project.as_deref()
                        && this.agent_chat.conversations.active_id() == conversation.as_deref()
                    {
                        this.agent_chat.references.midi_loading = false;
                    }
                    cx.notify();
                    return;
                }
                this.music_references_mut().midi_loading = false;
                if this.music_composer_locked()
                    || this.music_references().midi_draft.is_some()
                    || (from_draft && !this.music_references().attachments.contains(&path))
                {
                    return;
                }
                match result {
                    Ok(score) => {
                        this.music_references_mut().midi_draft =
                            Some(MidiDraft::new(score, from_draft.then_some(path)))
                    }
                    Err(error) => this.music_references_mut().attachment_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn choose_midi_attachment(&mut self, cx: &mut gpui::Context<Self>) {
        let owner = self.music_owner();
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("MIDI", &["mid", "midi"])
                .pick_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |this, cx| {
                if this.music_owner() != owner {
                    return;
                }
                this.attach_music_files(std::slice::from_ref(&path), cx);
                this.edit_midi_attachment(path, cx);
            });
        })
        .detach();
    }

    /// Commit the visible surface's independent draft before submitting or leaving it.
    pub(crate) fn commit_midi_draft(&mut self) -> bool {
        let limit_error = self.t(Key::AgentAttachmentLimit).to_owned();
        let refs = self.music_references_mut();
        let Some(draft) = refs.midi_draft.as_mut() else {
            return true;
        };
        if draft.lyric.marked().is_some() {
            return false;
        }
        draft.flush_lyric();
        let replacing = draft
            .replacing
            .as_ref()
            .and_then(|path| refs.attachments.iter().position(|p| p == path));
        if replacing.is_none() && refs.attachments.len() >= auris_agent::ATTACHMENT_COUNT_LIMIT {
            refs.attachment_error = Some(limit_error);
            return false;
        }
        let directory = auris_session::settings::config_dir().join("conversation-midi");
        match draft.score.save_snapshot(&directory) {
            Ok(path) => {
                refs.attached_here.insert(path.clone());
                if let Some(index) = replacing {
                    refs.attachments[index] = path;
                } else {
                    refs.attachments.push(path);
                }
                refs.midi_draft = None;
                refs.attachment_error = None;
                if let Some(s) = &mut self.song_workshop {
                    s.active_field = Some(0);
                } else if self.agent_chat.focused == Some(AgentField::MidiLyric) {
                    self.agent_chat.focused = Some(AgentField::Chat);
                }
                true
            }
            Err(error) => {
                refs.attachment_error = Some(error);
                false
            }
        }
    }

    pub(super) fn commit_agent_midi_draft(&mut self) -> bool {
        self.song_workshop.is_some() || self.commit_midi_draft()
    }

    pub(crate) fn midi_composer_controls(
        &mut self,
        locked: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let menu_open = self.menu.as_ref().is_some_and(|menu| {
            menu.entries.iter().any(|entry| {
                matches!(entry,
                super::super::context_menu::MenuEntry::Item(item)
                    if matches!(item.command, MenuCommand::ComposerMelody(_)))
            })
        });
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(
                atelier::Button::new("music-reference-menu", self.t(Key::AgentMusicReferences))
                    .ghost()
                    .size(atelier::ControlSize::Small)
                    .icon(atelier::IconName::More)
                    .icon_after(atelier::IconName::ChevronDown)
                    .selected(menu_open)
                    .disabled(locked)
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        if this.music_composer_locked() {
                            return;
                        }
                        let available = this.can_attach_agent_music();
                        let midi_available = available
                            && !this.music_references().midi_loading
                            && this.music_references().midi_draft.is_none();
                        let has_clip = this
                            .selected_clip
                            .and_then(|id| this.session.midi_clip(id))
                            .is_some();
                        let mut menu =
                            ContextMenu::new(event.position(), this.t(Key::AgentMusicReferences))
                                .item_greyed_unless(
                                    midi_available,
                                    this.t(Key::AgentMidiNew),
                                    MenuCommand::ComposerMelody(false),
                                )
                                .item_greyed_unless(
                                    midi_available && has_clip,
                                    this.t(Key::AgentMidiClip),
                                    MenuCommand::ComposerMelody(true),
                                )
                                .item_greyed_unless(
                                    midi_available,
                                    this.t(Key::AgentMidiFile),
                                    MenuCommand::ComposerMidi,
                                )
                                .item_greyed_unless(
                                    available,
                                    this.t(Key::AgentChords),
                                    MenuCommand::ComposerChords,
                                )
                                .item_greyed_unless(
                                    available,
                                    this.t(Key::AgentSound),
                                    MenuCommand::ComposerSound,
                                );
                        menu.step(1);
                        this.open_menu(menu);
                        cx.notify();
                    })),
            )
            .when(self.music_references().midi_loading, |row| {
                row.child(self.t(Key::AgentMidiLoading))
            })
            .into_any_element()
    }

    pub(crate) fn midi_editor(
        &mut self,
        max_height: gpui::Pixels,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(draft) = &self.music_references().midi_draft else {
            return div().into_any_element();
        };
        let theme = self.theme.clone();
        let score_name = draft.score.name.clone();
        let track_name = draft.score.tracks[draft.track].name.clone();
        let track = draft.track;
        let track_count = draft.score.tracks.len();
        let selected = draft.selected;
        let notes = draft.notes().to_vec();
        let page = draft.page;
        let low = draft.low_pitch;
        let lyric_empty = draft.lyric.content().is_empty();
        let scroll = draft.scroll.clone();
        let focus = self
            .music_references_mut()
            .midi_lyric_focus
            .get_or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        let mut grid = div()
            .id("agent-midi-grid")
            .debug_selector(|| "agent-midi-grid".into())
            .relative()
            .w_full()
            .h(gpui::rems(13.0))
            .flex_shrink_0()
            .bg(theme.surface_sunken)
            .overflow_hidden();
        // Pitch and time are data coordinates; all screen geometry scales with the text size.
        for row in 0..13u8 {
            let pitch = low + 12 - row;
            grid = grid.child(
                div()
                    .absolute()
                    .top(relative(row as f32 / 13.0))
                    .w_full()
                    .h(relative(1.0 / 13.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .when(matches!(pitch % 12, 1 | 3 | 6 | 8 | 10), |row| {
                        row.bg(theme.surface)
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(midi_name(i32::from(pitch))),
                    ),
            );
        }
        // Grid cells provide pointer placement; Add note and the adjustment buttons provide
        // the equivalent keyboard path without hundreds of tab stops.
        for column in 0..16usize {
            for row in 0..13u8 {
                let pitch = low + 12 - row;
                let at = page + column as i64 * STEP;
                grid = grid.child(
                    div()
                        .id(("midi-cell", column * 13 + row as usize))
                        .absolute()
                        .left(relative(0.1 + column as f32 * 0.9 / 16.0))
                        .top(relative(row as f32 / 13.0))
                        .w(relative(0.9 / 16.0))
                        .h(relative(1.0 / 13.0))
                        .border_l_1()
                        .border_color(theme.border)
                        .hover(|cell| cell.bg(theme.surface_hover))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(draft) = &mut this.music_references_mut().midi_draft {
                                    draft.add(pitch, at);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
            }
        }
        for (index, note) in notes.iter().enumerate() {
            if note.pitch < low
                || note.pitch > low + 12
                || note.end().0 <= page
                || note.start.0 >= page + PAGE
            {
                continue;
            }
            let start = (note.start.0 - page).max(0) as f32 / PAGE as f32;
            let end = (note.end().0 - page).min(PAGE) as f32 / PAGE as f32;
            let label = if note.lyric.is_empty() {
                midi_name(i32::from(note.pitch))
            } else {
                note.lyric.clone()
            };
            grid = grid.child(
                bounded_button_enabled(
                    ("agent-midi-note", index),
                    label,
                    ButtonStyle::Normal,
                    ButtonState::available(selected == Some(index), true),
                    theme.accent,
                    &theme,
                    cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = &mut this.music_references_mut().midi_draft {
                            draft.select(Some(index));
                        }
                        cx.notify();
                    }),
                )
                .absolute()
                .left(relative(0.1 + start * 0.9))
                .top(relative((low + 12 - note.pitch) as f32 / 13.0))
                .w(relative((end - start) * 0.9))
                .h(relative(1.0 / 13.0))
                .min_w_0()
                .px_0()
                .py_0()
                .overflow_hidden(),
            );
        }
        let mut editor = div()
            .id("agent-midi-editor")
            .max_h(max_height)
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .debug_selector(|| "agent-midi-editor".into())
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .p_2()
            .border_1()
            .border_color(theme.border)
            .rounded(Metrics::RADIUS_SM)
            .child(div().text_sm().child(score_name))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .flex_wrap()
                    .min_w_0()
                    .gap_1()
                    .child(bounded_button_enabled(
                        "agent-midi-cancel",
                        self.t(Key::Cancel),
                        ButtonStyle::Ghost,
                        ButtonState::available(false, true),
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, window, cx| {
                            this.close_midi_editor(window);
                            cx.notify();
                        }),
                    ))
                    .child(bounded_button_enabled(
                        "agent-midi-apply",
                        self.t(Key::AgentMidiApply),
                        ButtonStyle::Normal,
                        ButtonState::available(false, true),
                        theme.accent,
                        &theme,
                        cx.listener(|this, _, window, cx| {
                            if this.commit_midi_draft() {
                                this.close_midi_editor(window);
                            }
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(self.t(Key::AgentMidiHint)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(
                        self.midi_action("agent-midi-track-prev", "◀", track > 0, cx, |d| {
                            d.select(None);
                            d.track -= 1;
                            d.select(d.notes().first().map(|_| 0));
                        }),
                    )
                    .child(div().text_xs().child(format!(
                        "{} / {} · {}",
                        track + 1,
                        track_count,
                        track_name
                    )))
                    .child(self.midi_action(
                        "agent-midi-track-next",
                        "▶",
                        track + 1 < track_count,
                        cx,
                        |d| {
                            d.select(None);
                            d.track += 1;
                            d.select(d.notes().first().map(|_| 0));
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(
                        self.midi_action("agent-midi-page-prev", "←", page > 0, cx, |d| {
                            d.page = (d.page - PAGE).max(0)
                        }),
                    )
                    .child(div().text_xs().child(format!(
                        "{} {}–{}",
                        self.t(Key::AgentMidiBeats),
                        page / TICKS_PER_QUARTER + 1,
                        page / TICKS_PER_QUARTER + 8
                    )))
                    .child(
                        self.midi_action("agent-midi-page-next", "→", true, cx, |d| {
                            d.page = (d.page + PAGE).min(TICKS_PER_QUARTER * 999_992)
                        }),
                    )
                    .child(
                        self.midi_action("agent-midi-octave-down", "−8va", low > 0, cx, |d| {
                            d.low_pitch = d.low_pitch.saturating_sub(12)
                        }),
                    )
                    .child(
                        self.midi_action("agent-midi-octave-up", "+8va", low < 115, cx, |d| {
                            d.low_pitch = (d.low_pitch + 12).min(115)
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(self.midi_action(
                        "agent-midi-add",
                        self.t(Key::AgentMidiAdd),
                        notes.len() < MAX_REFERENCE_NOTES,
                        cx,
                        |d| {
                            let at = d
                                .selected
                                .and_then(|i| d.notes().get(i))
                                .map_or(d.page, |n| n.end().0);
                            d.add(60, at);
                            d.page = at / PAGE * PAGE;
                        },
                    ))
                    .child(self.midi_action(
                        "agent-midi-remove",
                        self.t(Key::AgentMidiDelete),
                        selected.is_some(),
                        cx,
                        |d| {
                            if let Some(i) = d.selected.take() {
                                d.score.tracks[d.track].notes.remove(i);
                                d.lyric = TextField::default();
                            }
                        },
                    )),
            );
        if let Some(note) = selected.and_then(|i| notes.get(i)) {
            editor = editor
                .child(div().text_xs().child(format!(
                    "{} · {} {:.2} · {} {:.2}",
                    midi_name(i32::from(note.pitch)),
                    self.t(Key::AgentMidiBeats),
                    note.start.0 as f64 / TICKS_PER_QUARTER as f64 + 1.0,
                    self.t(Key::AgentMidiLength),
                    note.length.0 as f64 / TICKS_PER_QUARTER as f64
                )))
                .child(
                    div().flex().flex_wrap().gap_1().children(
                        [
                            ("agent-midi-pitch-down", Key::AgentMidiPitchDown, 0),
                            ("agent-midi-pitch-up", Key::AgentMidiPitchUp, 1),
                            ("agent-midi-earlier", Key::AgentMidiEarlier, 2),
                            ("agent-midi-later", Key::AgentMidiLater, 3),
                            ("agent-midi-shorter", Key::AgentMidiShorter, 4),
                            ("agent-midi-longer", Key::AgentMidiLonger, 5),
                        ]
                        .into_iter()
                        .map(|(id, key, action)| {
                            self.midi_action(id, self.t(key), true, cx, move |d| {
                                if let Some(note) = d
                                    .selected
                                    .and_then(|i| d.score.tracks[d.track].notes.get_mut(i))
                                {
                                    match action {
                                        0 => note.pitch = note.pitch.saturating_sub(1),
                                        1 => note.pitch = (note.pitch + 1).min(127),
                                        2 => note.start.0 = (note.start.0 - STEP).max(0),
                                        3 => note.start.0 += STEP,
                                        4 => note.length.0 = (note.length.0 - STEP).max(1),
                                        _ => note.length.0 += STEP,
                                    }
                                }
                                d.reveal_selection();
                            })
                        }),
                    ),
                )
                .child(div().text_xs().child(self.t(Key::AgentMidiLyric)))
                .child(self.panel_field(
                    "agent-midi-lyric",
                    AgentField::MidiLyric,
                    self.music_lyric_focused(),
                    lyric_empty,
                    self.t(Key::AgentMidiLyricPlaceholder).to_owned(),
                    focus,
                    true,
                    &theme,
                    cx,
                ));
        }
        let ruler = div()
            .relative()
            .w_full()
            .h_4()
            .flex_shrink_0()
            .children((0..8).map(|beat| {
                div()
                    .absolute()
                    .left(relative(0.1 + beat as f32 * 0.9 / 8.0))
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child((page / TICKS_PER_QUARTER + beat + 1).to_string())
            }));
        div()
            .relative()
            .min_w_0()
            .child(editor.child(ruler).child(grid))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::vertical(&scroll).scrollbar_show(ScrollbarShow::Always)),
            )
            .into_any_element()
    }

    fn midi_action(
        &self,
        id: &'static str,
        label: &str,
        enabled: bool,
        cx: &mut gpui::Context<Self>,
        edit: impl Fn(&mut MidiDraft) + 'static,
    ) -> AnyElement {
        bounded_button_enabled(
            id,
            label.to_owned(),
            ButtonStyle::Ghost,
            ButtonState::available(false, enabled),
            self.theme.accent,
            &self.theme,
            cx.listener(move |this, _, _, cx| {
                if let Some(draft) = &mut this.music_references_mut().midi_draft {
                    edit(draft);
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(crate) fn close_midi_editor(&mut self, window: &mut Window) {
        self.music_references_mut().midi_draft = None;
        if let Some(s) = &mut self.song_workshop {
            s.active_field = Some(0);
            window.focus(&s.foci[0]);
        } else {
            self.focus_agent_field(AgentField::Chat);
            if let Some(focus) = &self.agent_chat.input_focus {
                window.focus(focus);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn midi_editor_stays_inside_a_narrow_panel_at_large_text_sizes(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        app.update(cx, |this, _| {
            this.panels.show(crate::dock::Panel::Agent);
            this.panels
                .set_size(crate::dock::Dock::Right, crate::dock::PanelLayout::MIN_SIDE);
            this.agent_chat.models_loaded = true;
            this.start_midi_draft(false);
        });
        for (scheme, language, font_size) in [
            ("midnight", auris_i18n::Language::English, 16.0),
            ("daylight", auris_i18n::Language::Japanese, 24.0),
        ] {
            app.update(cx, |this, _| {
                this.theme = Theme::named(scheme);
                this.language = language;
            });
            cx.update(|window, _| window.set_rem_size(px(font_size)));
            crate::harness::resize(&app, cx, gpui::size(px(720.0), px(720.0)));
            let panel = cx.debug_bounds("agent-panel").unwrap();
            for selector in ["agent-midi-editor", "agent-midi-apply", "agent-input"] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.left() >= panel.left() && bounds.right() <= panel.right(),
                    "{selector}: {bounds:?} outside {panel:?}"
                );
                assert!(
                    bounds.top() >= panel.top() && bounds.bottom() <= panel.bottom(),
                    "{selector}: {bounds:?} outside {panel:?}"
                );
            }
        }
    }

    #[gpui::test]
    fn midi_queued_messages_hold_independent_score_snapshots(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        app.update(cx, |this, _| {
            this.settings.agent.model = "test-model".into();
            this.agent_chat.busy = true;
            for pitch in [60, 67] {
                this.start_midi_draft(false);
                this.agent_chat
                    .references
                    .midi_draft
                    .as_mut()
                    .unwrap()
                    .add(pitch, 0);
                this.agent_chat.input = TextField::new("Use this melody");
                assert!(this.enqueue_agent_message());
            }
            let first = &this.agent_chat.queued[0].attachments[0];
            let second = &this.agent_chat.queued[1].attachments[0];
            assert_ne!(first, second);
            assert_eq!(
                MidiReference::read(first).unwrap().tracks[0].notes[0].pitch,
                60
            );
            assert_eq!(
                MidiReference::read(second).unwrap().tracks[0].notes[0].pitch,
                67
            );
        });
    }

    #[gpui::test]
    fn midi_clip_copy_edits_and_lyrics_leave_the_song_unchanged(cx: &mut gpui::TestAppContext) {
        let (app, cx, _, clip) = crate::harness::with_a_clip(cx);
        app.update(cx, |this, _| {
            this.selected_clip = Some(clip);
            this.panels.show(crate::dock::Panel::Agent);
            this.agent_chat.models_loaded = true;
            this.start_midi_draft(true);
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("agent-midi-add", cx);
        crate::harness::paint(&app, cx);
        crate::harness::click("agent-midi-pitch-up", cx);
        crate::harness::click("agent-midi-lyric", cx);
        cx.simulate_input("きみ");
        app.read_with(cx, |this, _| {
            assert_eq!(
                this.agent_chat
                    .references
                    .midi_draft
                    .as_ref()
                    .unwrap()
                    .lyric
                    .content(),
                "きみ"
            );
            assert!(this.session.midi_clip(clip).unwrap().notes.is_empty());
        });
        crate::harness::click("agent-midi-apply", cx);
        app.read_with(cx, |this, _| {
            assert!(this.agent_chat.references.midi_draft.is_none());
            let score = MidiReference::read(&this.agent_chat.references.attachments[0]).unwrap();
            assert_eq!(score.tracks[0].notes[0].pitch, 61);
            assert_eq!(score.tracks[0].notes[0].lyric, "きみ");
            assert!(this.session.midi_clip(clip).unwrap().notes.is_empty());
        });
    }

    #[gpui::test]
    fn midi_lyrics_escape_cancels_and_restores_the_composer(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::harness::open(cx);
        app.update(cx, |this, _| {
            this.panels.show(crate::dock::Panel::Agent);
            this.agent_chat.models_loaded = true;
            this.start_midi_draft(false);
            this.agent_chat
                .references
                .midi_draft
                .as_mut()
                .unwrap()
                .add(60, 0);
        });
        crate::harness::paint(&app, cx);
        crate::harness::click("agent-midi-lyric", cx);
        cx.simulate_input("discard me");
        cx.simulate_keystrokes("escape");
        app.read_with(cx, |this, _| {
            assert!(this.agent_chat.references.midi_draft.is_none());
            assert!(this.agent_chat.references.attachments.is_empty());
            assert_eq!(this.agent_chat.focused, Some(AgentField::Chat));
        });
    }

    #[test]
    fn selecting_notes_flushes_only_the_previous_notes_lyric() {
        use auris_session::prelude::{SignatureMap, TempoMap};
        let score = MidiReference::new(
            "Melody".into(),
            TempoMap::constant(120.0),
            SignatureMap::default(),
        );
        let mut draft = MidiDraft::new(score, None);
        draft.add(60, 0);
        draft.lyric = TextField::new("あ");
        draft.add(64, STEP);
        draft.lyric = TextField::new("い");
        draft.select(Some(0));
        assert_eq!(draft.lyric.content(), "あ");
        assert_eq!(draft.notes()[1].lyric, "い");
    }
}
