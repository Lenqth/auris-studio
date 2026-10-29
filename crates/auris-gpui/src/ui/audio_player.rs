//! Shared controls for finite audio previews. The callback owns the playback clock.

use super::icons::{Icon, icon};
use super::tooltip::keyed_tip;
use super::widgets::{
    AdjustSlider, ButtonState, ButtonStyle, button_with_content, slider_fraction_after_key,
};
use crate::app::AurisApp;
use auris_i18n::Key;
use auris_session::OutputPreviewStatus;
use gpui::{
    AnyElement, Bounds, Context, IntoElement, MouseButton, MouseMoveEvent, Pixels, SharedString,
    canvas, div, prelude::*, relative, rems,
};
use std::{cell::Cell, rc::Rc};

pub(crate) struct PlayerState {
    status: Option<OutputPreviewStatus>,
    position: f64,
    volume: f32,
    muted: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: None,
            position: 0.0,
            volume: 1.0,
            muted: false,
        }
    }
}

impl Drop for PlayerState {
    fn drop(&mut self) {
        self.stop();
    }
}

impl PlayerState {
    pub(crate) fn reset_audio(&mut self) {
        self.stop();
        self.position = 0.0;
    }

    pub(crate) fn stop(&mut self) {
        self.position = self.position();
        if let Some(status) = self.status.take() {
            status.stop();
        }
    }

    pub(crate) fn start(&mut self, status: OutputPreviewStatus) {
        self.stop();
        self.status = Some(status);
    }

    pub(crate) fn options(&self, duration: f64) -> auris_session::OutputPreviewOptions {
        let position = self.position();
        auris_session::OutputPreviewOptions {
            position_seconds: if position >= duration { 0.0 } else { position },
            volume: if self.muted { 0.0 } else { self.volume },
        }
    }

    pub(crate) fn toggle_pause(&self) -> bool {
        if let Some(status) = &self.status
            && status.is_active()
            && status.position_seconds() < status.duration_seconds()
        {
            status.set_paused(status.is_playing());
            true
        } else {
            false
        }
    }

    fn position(&self) -> f64 {
        self.status
            .as_ref()
            .map_or(self.position, OutputPreviewStatus::position_seconds)
    }

    /// Whether the current audition reports active playback.
    pub(super) fn playing(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(OutputPreviewStatus::is_playing)
    }

    fn seek(&mut self, seconds: f64) {
        self.position = seconds;
        if let Some(status) = &self.status {
            status.seek_seconds(seconds);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.muted = false;
        self.apply_volume();
    }

    fn apply_volume(&self) {
        if let Some(status) = &self.status {
            status.set_volume(if self.muted { 0.0 } else { self.volume });
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PlayerTarget {
    Workshop(usize),
    Attachment,
}

#[derive(Clone, Copy)]
pub(crate) struct PlayerDrag {
    target: PlayerTarget,
    volume: bool,
    bounds: Bounds<Pixels>,
    duration: f64,
}

fn time_label(seconds: f64) -> String {
    let seconds = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn note_rect(
    note: &auris_session::AuditionNote,
    low: u8,
    high: u8,
    duration: f64,
) -> (f32, f32, f32, f32) {
    let rows = f32::from(high - low + 1);
    (
        (note.start_seconds / duration) as f32,
        f32::from(high.saturating_sub(note.pitch)) / rows,
        ((note.end_seconds - note.start_seconds) / duration) as f32,
        1.0 / rows,
    )
}

// Keep authored colors when they identify a track, then use themed categorical fallbacks.
fn audition_track_colors(
    timeline: &auris_session::AuditionTimeline,
    theme: &crate::theme::Theme,
) -> Vec<gpui::Hsla> {
    let mut colors = Vec::with_capacity(timeline.tracks.len());
    let palette = auris_session::prelude::Color::PALETTE;
    for (index, track) in timeline.tracks.iter().enumerate() {
        let authored = theme.track_color(track.color);
        let color = if colors.contains(&authored) {
            palette
                .iter()
                .cycle()
                .skip(index % palette.len())
                .take(palette.len())
                .map(|color| theme.track_color(color.0))
                .chain(
                    (1..=index + 1)
                        .map(|slot| theme.group_color((slot as f32 * 0.618_034).fract())),
                )
                .find(|color| !colors.contains(color))
                .unwrap_or(authored)
        } else {
            authored
        };
        colors.push(color);
    }
    colors
}

impl AurisApp {
    fn player(&self, target: PlayerTarget) -> Option<&PlayerState> {
        match target {
            PlayerTarget::Workshop(index) => self.song_workshop.as_ref().map(|s| &s.players[index]),
            PlayerTarget::Attachment => self.attachment_preview.as_ref().map(|s| &s.player),
        }
    }

    fn player_mut(&mut self, target: PlayerTarget) -> Option<&mut PlayerState> {
        match target {
            PlayerTarget::Workshop(index) => {
                self.song_workshop.as_mut().map(|s| &mut s.players[index])
            }
            PlayerTarget::Attachment => self.attachment_preview.as_mut().map(|s| &mut s.player),
        }
    }

    fn adjust_player(&mut self, target: PlayerTarget, volume: bool, fraction: f32, duration: f64) {
        if let Some(player) = self.player_mut(target) {
            if volume {
                player.set_volume(fraction);
            } else {
                player.seek(f64::from(fraction) * duration);
            }
        }
    }

    pub(crate) fn move_audio_player(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.audio_player_drag else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.audio_player_drag = None;
            return false;
        }
        let fraction =
            ((event.position.x - drag.bounds.left()) / drag.bounds.size.width).clamp(0.0, 1.0);
        self.adjust_player(drag.target, drag.volume, fraction, drag.duration);
        cx.notify();
        true
    }

    pub(crate) fn render_audio_player(
        &self,
        target: PlayerTarget,
        duration: f64,
        enabled: bool,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(player) = self.player(target) else {
            return div().into_any_element();
        };
        let theme = &self.theme;
        let prefix = match target {
            PlayerTarget::Workshop(i) => format!("song-preview-{i}"),
            PlayerTarget::Attachment => "attachment-player".into(),
        };
        let play_id: SharedString = format!("{prefix}-play").into();
        let playing = player.playing();
        let timeline = match target {
            PlayerTarget::Workshop(index) => self
                .song_workshop
                .as_ref()
                .and_then(|s| s.audition_timeline(index)),
            _ => None,
        };
        // Draft auditions can update while the captured buffer keeps playing.
        let duration = timeline.as_ref().map_or_else(
            || {
                player
                    .status
                    .as_ref()
                    .filter(|s| s.is_active())
                    .map_or(duration, OutputPreviewStatus::duration_seconds)
            },
            |t| t.duration_seconds,
        );
        let position = player.position().min(duration);
        let fraction = if duration > 0.0 {
            (position / duration) as f32
        } else {
            0.0
        };
        let play_label = self.t(if loading {
            Key::WorkshopPreparing
        } else if playing {
            Key::PreviewPause
        } else {
            Key::PreviewPlay
        });
        let muted = player.muted || player.volume == 0.0;
        let mute_label = self.t(if muted {
            Key::PreviewUnmute
        } else {
            Key::PreviewMute
        });
        let controls = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                button_with_content(
                    play_id,
                    icon(
                        if playing { Icon::Pause } else { Icon::Play },
                        crate::theme::Metrics::CONTROL_HEIGHT * 0.6,
                        theme.text,
                    ),
                    ButtonStyle::Ghost,
                    ButtonState::available(false, enabled && !loading),
                    theme.accent,
                    theme,
                    cx.listener(move |this, _, _, cx| {
                        if !this.player(target).is_some_and(PlayerState::toggle_pause) {
                            match target {
                                PlayerTarget::Workshop(index) => {
                                    this.preview_song_candidate(index, cx)
                                }
                                PlayerTarget::Attachment => this.play_attachment_preview(cx),
                            }
                        }
                        if let PlayerTarget::Workshop(_) = target
                            && let Some(workshop) = &mut this.song_workshop
                        {
                            workshop.sync_idle_auditions();
                        }
                        cx.notify();
                    }),
                )
                .size_8()
                .tooltip(keyed_tip(play_label, "", theme)),
            )
            .when(timeline.is_none(), |row| {
                row.child(self.player_slider(
                    format!("{prefix}-seek"),
                    target,
                    false,
                    fraction,
                    duration,
                    enabled && duration > 0.0,
                    cx,
                ))
            })
            .when(timeline.is_some(), |row| row.child(div().flex_1()))
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .child(time_label(position))
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(format!(" / {}", time_label(duration))),
                    )
                    .flex()
                    .items_center(),
            )
            .child(
                button_with_content(
                    SharedString::from(format!("{prefix}-mute")),
                    icon(
                        if muted {
                            Icon::VolumeMuted
                        } else {
                            Icon::Volume
                        },
                        crate::theme::Metrics::CONTROL_HEIGHT * 0.6,
                        theme.text,
                    ),
                    ButtonStyle::Ghost,
                    ButtonState::available(false, enabled),
                    theme.accent,
                    theme,
                    cx.listener(move |this, _, _, cx| {
                        if let Some(player) = this.player_mut(target) {
                            if player.volume == 0.0 {
                                player.volume = 1.0;
                                player.muted = false;
                            } else {
                                player.muted = !player.muted;
                            }
                            player.apply_volume();
                        }
                        cx.notify();
                    }),
                )
                .size_8()
                .tooltip(keyed_tip(mute_label, "", theme)),
            )
            .child(self.player_slider(
                format!("{prefix}-volume"),
                target,
                true,
                if muted { 0.0 } else { player.volume },
                duration,
                enabled,
                cx,
            ));
        div()
            .id(SharedString::from(prefix.clone()))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .bg(theme.surface_raised)
            .when_some(timeline, |player, timeline| {
                player
                    .child(self.player_timeline(&prefix, target, &timeline, position, enabled, cx))
            })
            .child(controls)
            .into_any_element()
    }

    fn player_timeline(
        &self,
        prefix: &str,
        target: PlayerTarget,
        timeline: &auris_session::AuditionTimeline,
        position: f64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let duration = timeline.duration_seconds;
        let mut sections = div()
            .relative()
            .h_8()
            .w_full()
            .border_1()
            .border_color(gpui::transparent_black());
        for (index, section) in timeline.sections.iter().enumerate() {
            let start = section.start_seconds;
            let end = section.end_seconds;
            let current = position >= start && position < end;
            let repeated = timeline
                .sections
                .iter()
                .filter(|s| s.label == section.label)
                .count()
                > 1;
            let label = if repeated {
                format!("{} {}", section.label, section.instance)
            } else {
                section.label.clone()
            };
            let detail = format!("{} · {}–{}", label, time_label(start), time_label(end));
            sections = sections.child(
                div()
                    .absolute()
                    .left(relative((start / duration) as f32))
                    .w(relative(((end - start) / duration) as f32))
                    .h_full()
                    .pr_1()
                    .child(
                        button_with_content(
                            SharedString::from(format!("{prefix}-section-{index}")),
                            div().min_w_0().truncate().child(label),
                            ButtonStyle::Ghost,
                            ButtonState::available(current, enabled),
                            theme.selection,
                            theme,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(player) = this.player_mut(target) {
                                    player.seek(start);
                                }
                                cx.notify();
                            }),
                        )
                        .w_full()
                        .min_w_0()
                        .rounded_sm()
                        .px_0()
                        .tooltip(keyed_tip(detail, "", theme)),
                    ),
            );
        }
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .when(!timeline.sections.is_empty(), |view| view.child(sections))
            .child(self.player_slider(
                format!("{prefix}-seek"),
                target,
                false,
                if duration > 0.0 {
                    (position / duration) as f32
                } else {
                    0.0
                },
                duration,
                enabled && duration > 0.0,
                cx,
            ))
            .child(
                div()
                    .id(SharedString::from(format!("{prefix}-tracks")))
                    .debug_selector({
                        let label = format!("{prefix}-tracks");
                        move || label.clone()
                    })
                    .flex()
                    .min_w_0()
                    .w_full()
                    .gap_2()
                    .overflow_x_scroll()
                    .children(
                        timeline
                            .tracks
                            .iter()
                            .zip(audition_track_colors(timeline, theme))
                            .map(|(track, color)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .child(div().size_2().rounded_sm().bg(color))
                                    .child(track.name.clone())
                            }),
                    ),
            )
            .into_any_element()
    }

    // The two sliders share interaction policy while their geometry and availability differ.
    #[expect(clippy::too_many_arguments)]
    fn player_slider(
        &self,
        id: String,
        target: PlayerTarget,
        volume: bool,
        fraction: f32,
        duration: f64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let painted = bounds.clone();
        let selector = id.clone();
        let label = self.t(if volume {
            Key::PreviewVolume
        } else {
            Key::PreviewSeek
        });
        let timeline = match target {
            PlayerTarget::Workshop(index) if !volume => self
                .song_workshop
                .as_ref()
                .and_then(|s| s.audition_timeline(index)),
            _ => None,
        };
        let has_waveform = timeline.is_some();
        let track = if let Some(timeline) = timeline {
            let mut waveform = div()
                .relative()
                .h_16()
                .w_full()
                .rounded_sm()
                .overflow_hidden()
                .bg(theme.surface_sunken)
                .child(
                    div()
                        .size_full()
                        .opacity(if timeline.tracks.is_empty() {
                            1.0
                        } else {
                            0.45
                        })
                        .child(
                            atelier::viz::PeakBars::new(timeline.peaks.clone()).progress(fraction),
                        ),
                )
                .child(
                    canvas(move |bounds, _, _| painted.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                );
            if !timeline.tracks.is_empty() && duration > 0.0 {
                let score = std::sync::Arc::clone(&timeline);
                let colors = audition_track_colors(&timeline, theme);
                let overlay_id = format!("{selector}-notes");
                waveform = waveform.child(
                    div()
                        .absolute()
                        .inset_0()
                        .debug_selector(move || overlay_id.clone())
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let low = score
                                        .tracks
                                        .iter()
                                        .flat_map(|t| &t.notes)
                                        .map(|n| n.pitch)
                                        .min()
                                        .unwrap_or(60)
                                        .saturating_sub(2);
                                    let high = score
                                        .tracks
                                        .iter()
                                        .flat_map(|t| &t.notes)
                                        .map(|n| n.pitch)
                                        .max()
                                        .unwrap_or(72)
                                        .saturating_add(2)
                                        .min(127);
                                    for (track, color) in score.tracks.iter().zip(&colors) {
                                        let color = color.opacity(0.72);
                                        for note in &track.notes {
                                            let (x, y, width, height) =
                                                note_rect(note, low, high, duration);
                                            // Keep short notes visible at the overlay's raster resolution.
                                            let rect = gpui::Bounds::new(
                                                bounds.origin
                                                    + gpui::point(
                                                        bounds.size.width * x,
                                                        bounds.size.height * y,
                                                    ),
                                                gpui::size(
                                                    (bounds.size.width * width).max(gpui::px(1.0)),
                                                    (bounds.size.height * height)
                                                        .max(gpui::px(1.0)),
                                                ),
                                            );
                                            window.paint_quad(gpui::fill(rect, color));
                                        }
                                    }
                                },
                            )
                            .size_full(),
                        ),
                );
            }
            for section in &timeline.sections {
                waveform = waveform.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative((section.start_seconds / duration) as f32))
                        .border_l_1()
                        .border_color(theme.border),
                );
            }
            waveform
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative(fraction))
                        .border_l_1()
                        .border_color(theme.text),
                )
                .into_any_element()
        } else {
            div()
                .relative()
                .h_1()
                .w_full()
                .rounded_full()
                .bg(theme.surface_sunken)
                .child(
                    canvas(move |bounds, _, _| painted.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .h_full()
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(theme.text_muted),
                )
                .child(
                    div()
                        .absolute()
                        .left(relative(fraction))
                        .top(rems(-0.25))
                        .ml(rems(-0.375))
                        .size_3()
                        .rounded_full()
                        .bg(if enabled {
                            theme.text
                        } else {
                            theme.text_faint
                        }),
                )
                .into_any_element()
        };
        let slider = div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .relative()
            .when(has_waveform, |s| s.h_16().w_full().flex_shrink_0())
            .when(!has_waveform, |s| s.h_8())
            .min_w_0()
            .rounded_sm()
            .border_1()
            .border_color(gpui::transparent_black())
            .when(volume, |s| s.w_12().flex_shrink_0())
            .when(!volume && !has_waveform, |s| s.flex_1())
            .flex()
            .items_center()
            .when(!has_waveform, |s| s.px_1())
            .child(track)
            .when(enabled, |s| {
                s.tab_index(0)
                    .key_context("AurisSlider")
                    .focus(|s| s.border_color(theme.selection))
                    .hover(|s| s.bg(theme.surface_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            let bounds = bounds.get();
                            if bounds.size.width <= gpui::px(0.0) {
                                return;
                            }
                            let fraction = ((event.position.x - bounds.left()) / bounds.size.width)
                                .clamp(0.0, 1.0);
                            this.audio_player_drag = Some(PlayerDrag {
                                target,
                                volume,
                                bounds,
                                duration,
                            });
                            this.adjust_player(target, volume, fraction, duration);
                            // Let GPUI's built-in mouse-down listener focus this Tab stop.
                            cx.notify();
                        }),
                    )
                    .on_action(cx.listener(move |this, event: &AdjustSlider, _, cx| {
                        if let Some(next) = slider_fraction_after_key(fraction, event.key) {
                            this.adjust_player(target, volume, next, duration);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }))
            })
            .when(!enabled, |s| s.opacity(0.5))
            .tooltip(keyed_tip(label, "← → · Home · End", theme));
        slider.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_instrument_colors_remain_distinguishable_in_the_overlay_and_legend() {
        let theme = crate::theme::Theme::default();
        let color = auris_session::prelude::Color::INSTRUMENT.0;
        let timeline = auris_session::AuditionTimeline {
            tracks: (0..10)
                .map(|id| auris_session::AuditionTrack {
                    id,
                    name: format!("Track {id}"),
                    color,
                    notes: vec![],
                })
                .collect(),
            ..Default::default()
        };
        let colors = super::audition_track_colors(&timeline, &theme);
        assert_eq!(colors[0], theme.track_color(color));
        assert_eq!(colors.len(), 10);
        for (index, color) in colors.iter().enumerate() {
            assert!(!colors[..index].contains(color));
        }
        assert!(timeline.tracks.iter().all(|track| track.color == color));
    }
    #[test]
    fn overlaid_notes_share_one_time_and_pitch_scale() {
        let note = auris_session::AuditionNote {
            pitch: 72,
            start_seconds: 2.0,
            end_seconds: 3.0,
        };
        assert_eq!(
            super::note_rect(&note, 36, 83, 8.0),
            (0.25, 11.0 / 48.0, 0.125, 1.0 / 48.0)
        );
        let bass = auris_session::AuditionNote { pitch: 36, ..note };
        let (_, y, _, _) = super::note_rect(&bass, 36, 83, 8.0);
        assert_eq!(y, 47.0 / 48.0);
    }
}
