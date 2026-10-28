//! Library timbre exploration with a background session job and explicit adoption.

use crate::{app::AurisApp, theme::Theme};
use atelier::plot::Plot3d;
use atelier::{
    Button, Card, ControlSize, EmptyState, IconName, Label, ListItem, ProgressBar, SectionHeader,
    Segment, SegmentedControl, StatusBar, StatusDot, TextColor, TextSize,
};
use auris_i18n::Key;
use auris_session::{TimbreMap, TimbreMapControl, prelude::*};
use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*};
use std::sync::Arc;
#[path = "timbre_plot.rs"]
mod plot;
use plot::{TimbreCamera, plot_data};

#[derive(Default)]
pub(crate) struct TimbreMapState {
    pub(crate) open: bool,
    camera: TimbreCamera,
    hovered: Option<usize>,
    list_nearest: bool,
    pub(crate) map: Option<Arc<TimbreMap>>,
    pub(crate) control: Option<TimbreMapControl>,
    generation: u64,
    total: usize,
    selected: Option<usize>,
    preview_track: Option<TrackId>,
    error: Option<String>,
}

impl TimbreMapState {
    pub(crate) fn cancel(&mut self) {
        if let Some(control) = self.control.take() {
            control.cancel();
        }
        self.generation = self.generation.wrapping_add(1);
    }
}

fn cluster_color(cluster: usize, theme: &Theme) -> gpui::Hsla {
    theme.group_color(cluster as f32 * 0.618_034)
}

impl AurisApp {
    pub(crate) fn close_timbre_map(&mut self) {
        self.timbre_map.cancel();
        self.timbre_map.open = false;
        if let Some(track) = self.timbre_map.preview_track.take() {
            self.session.stop_singer_preview(track);
        }
    }

    pub(crate) fn scan_timbre_map(&mut self, cx: &mut Context<Self>) {
        self.timbre_map.cancel();
        self.timbre_map.selected = None;
        self.timbre_map.hovered = None;
        self.timbre_map.error = None;
        if let Some(track) = self.timbre_map.preview_track.take() {
            self.session.stop_singer_preview(track);
        }
        self.timbre_map.map = None;
        let job = match self.session.timbre_map_job() {
            Ok(job) => job,
            Err(e) => {
                self.timbre_map.error = Some(e.to_string());
                return;
            }
        };
        self.timbre_map.total = job.sound_count();
        let generation = self.timbre_map.generation;
        let control = TimbreMapControl::default();
        self.timbre_map.control = Some(control.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { job.run(&control) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.timbre_map.generation != generation {
                    return;
                }
                this.timbre_map.control = None;
                match result {
                    Ok(map) => this.timbre_map.map = Some(Arc::new(map)),
                    Err(e) => this.timbre_map.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        let active = this.timbre_map.generation == generation
                            && this.timbre_map.control.is_some();
                        if active {
                            cx.notify();
                        }
                        active
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }

    fn select_timbre(&mut self, index: usize) {
        let Some(map) = self.timbre_map.map.clone() else {
            return;
        };
        if index >= map.sounds.len() {
            return;
        }
        self.timbre_map.selected = Some(index);
        self.timbre_map.error = None;
        if let Some(track) = self.timbre_map.preview_track.take() {
            self.session.stop_singer_preview(track);
        }
        if let Some(track) = self.session.audition_track(self.selected_track) {
            match self.session.preview_timbre(&map, index, track) {
                Ok(()) => self.timbre_map.preview_track = Some(track),
                Err(e) => self.timbre_map.error = Some(e.to_string()),
            }
        } else {
            self.timbre_map.error = Some(self.t(Key::LibraryNeedsInstrumentTrack).into());
        }
    }

    fn adopt_timbre(&mut self) {
        let Some(sound) = self
            .timbre_map
            .map
            .as_ref()
            .and_then(|m| self.timbre_map.selected.and_then(|i| m.sounds.get(i)))
            .cloned()
        else {
            return;
        };
        let Some(track) = self.selected_track else {
            self.timbre_map.error = Some(self.t(Key::LibraryNeedsInstrumentTrack).into());
            return;
        };
        match self.session.use_timbre_sound(track, &sound) {
            Ok(()) => self.timbre_map.error = None,
            Err(e) => self.timbre_map.error = Some(e.to_string()),
        }
    }

    pub(crate) fn render_timbre_map(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.timbre_map.open {
            return None;
        }
        let theme = &self.theme.atelier;
        let spacing = theme.spacing;
        let busy = self.timbre_map.control.is_some();
        let mut content = div().flex().flex_col().flex_1().min_h_0().gap(spacing.md);
        if let Some(control) = &self.timbre_map.control {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing.sm)
                    .child(
                        SectionHeader::new(self.t(Key::AnalysisRunning))
                            .description(format!(
                                "{}/{}",
                                control.completed(),
                                self.timbre_map.total
                            ))
                            .action(Button::new("timbre-cancel", self.t(Key::Cancel)).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.timbre_map.cancel();
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(ProgressBar::new(
                        control.completed() as f32 / self.timbre_map.total.max(1) as f32,
                    )),
            );
        }
        if let Some(error) = &self.timbre_map.error {
            content = content.child(
                Label::new(format!("! {error}"))
                    .color(TextColor::Danger)
                    .size(TextSize::Small),
            );
        }
        if let Some(map) = &self.timbre_map.map {
            if map.sounds.is_empty() {
                content = content.child(EmptyState::new(
                    IconName::Music,
                    self.t(Key::TimbreMapEmpty),
                ));
            } else {
                let (positions, extent) = plot_data(map.positions());
                let neighbors = self
                    .timbre_map
                    .selected
                    .map(|i| map.nearest(i, 8))
                    .unwrap_or_default();
                let mut plot = Plot3d::new()
                    .labels("PC1", "PC2", "PC3")
                    .x_range(-extent, extent)
                    .y_range(-extent, extent)
                    .z_range(-extent, extent)
                    .view(
                        self.timbre_map.camera.azimuth,
                        self.timbre_map.camera.elevation,
                    );
                for cluster in 0..=map.clusters().iter().copied().max().unwrap_or(0) {
                    let points = positions
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| map.clusters()[*i] == cluster)
                        .map(|(_, p)| *p)
                        .collect();
                    plot = plot.scatter(points, Some(cluster_color(cluster, &self.theme)));
                }
                // Small three-axis crosses identify the selected source without relying on hue.
                if let Some(index) = self.timbre_map.selected {
                    for axis in 0..3 {
                        let mut a = positions[index];
                        let mut b = a;
                        a[axis] -= extent * 0.035;
                        b[axis] += extent * 0.035;
                        plot = plot.curve(vec![a, b], Some(theme.colors.text));
                    }
                    for (i, _) in &neighbors {
                        plot = plot.scatter(vec![positions[*i]], Some(theme.colors.accent));
                    }
                }
                let bounds = self.timbre_map.camera.bounds.clone();
                let picking = positions.clone();
                let hovering = positions;
                let mut well = div()
                    .id("timbre-plot")
                    .debug_selector(|| "timbre-plot".into())
                    .relative()
                    .flex_1()
                    .min_h(gpui::rems(16.))
                    .w_full()
                    .rounded(theme.shape.radius_sm)
                    .bg(theme.colors.sunken)
                    .overflow_hidden()
                    .cursor_grab()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.timbre_map.camera.press(event.position);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                            if this
                                .timbre_map
                                .camera
                                .drag(event.position, event.dragging())
                            {
                                this.timbre_map.hovered = None;
                                cx.notify();
                            } else {
                                let hovered =
                                    this.timbre_map
                                        .camera
                                        .pick(&hovering, extent, event.position);
                                if this.timbre_map.hovered != hovered {
                                    this.timbre_map.hovered = hovered;
                                    cx.notify();
                                }
                            }
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseUpEvent, _, cx| {
                            if this.timbre_map.camera.release()
                                && let Some(i) =
                                    this.timbre_map
                                        .camera
                                        .pick(&picking, extent, event.position)
                            {
                                this.select_timbre(i);
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.timbre_map.camera.release();
                        }),
                    )
                    .child(plot)
                    .child(
                        gpui::canvas(
                            move |rect, _, _| {
                                bounds.set(rect);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    );
                if let Some(i) = self.timbre_map.hovered.and_then(|i| map.sounds.get(i)) {
                    well = well.tooltip(crate::ui::tooltip::keyed_tip(
                        i.name.clone(),
                        "",
                        &self.theme,
                    ));
                }
                let camera = &self.timbre_map.camera;
                let views = SegmentedControl::new(
                    "timbre-views",
                    [
                        Key::TimbreMapView3d,
                        Key::TimbreMapViewTop,
                        Key::TimbreMapViewFront,
                        Key::TimbreMapViewSide,
                    ]
                    .map(|key| Segment::label(self.t(key))),
                    camera.preset.unwrap_or(usize::MAX),
                )
                .size(ControlSize::Small)
                .on_select(cx.listener(|this, index: &usize, _, cx| {
                    this.timbre_map.camera.set_view(*index);
                    cx.notify();
                }));
                let mut turns = div().flex().items_center().gap(spacing.xs);
                for (id, icon, label, azimuth, elevation) in [
                    (
                        "timbre-left",
                        IconName::ChevronLeft,
                        Key::TimbreMapTurnLeft,
                        -15.,
                        0.,
                    ),
                    (
                        "timbre-right",
                        IconName::ChevronRight,
                        Key::TimbreMapTurnRight,
                        15.,
                        0.,
                    ),
                    (
                        "timbre-up",
                        IconName::ChevronUp,
                        Key::TimbreMapTiltUp,
                        0.,
                        15.,
                    ),
                    (
                        "timbre-down",
                        IconName::ChevronDown,
                        Key::TimbreMapTiltDown,
                        0.,
                        -15.,
                    ),
                ] {
                    turns = turns.child(
                        Button::icon_only(id, icon)
                            .ghost()
                            .size(ControlSize::Small)
                            .tooltip(self.t(label))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.timbre_map.camera.turn(azimuth, elevation);
                                cx.notify();
                            })),
                    );
                }
                let mut legend = div().flex().flex_wrap().gap(spacing.sm);
                for cluster in 0..=map.clusters().iter().copied().max().unwrap_or(0) {
                    legend = legend.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing.xs)
                            .child(StatusDot::new(cluster_color(cluster, &self.theme)))
                            .child(
                                Label::new(format!(
                                    "{} {}",
                                    self.t(Key::TimbreMapGroup),
                                    cluster + 1
                                ))
                                .size(TextSize::XSmall)
                                .color(TextColor::Muted),
                            ),
                    );
                }
                let figure = Card::new()
                    .flex_1()
                    .min_w_0()
                    .min_h(gpui::rems(24.))
                    .p(spacing.lg)
                    .gap(spacing.md)
                    .child(
                        SectionHeader::new(self.t(Key::TimbreMap))
                            .description(self.t(Key::TimbreMapHint)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(spacing.sm)
                            .child(views)
                            .child(turns)
                            .child(
                                Label::new(format!(
                                    "{:.0}° / {:.0}°",
                                    camera.azimuth, camera.elevation
                                ))
                                .mono()
                                .size(TextSize::XSmall)
                                .color(TextColor::Muted),
                            ),
                    )
                    .child(well)
                    .child(legend)
                    .child(
                        Label::new(self.t(Key::TimbreMapGeometry))
                            .size(TextSize::XSmall)
                            .color(TextColor::Muted),
                    );
                let selected = self.timbre_map.selected.and_then(|i| map.sounds.get(i));
                let target = self
                    .selected_track
                    .filter(|id| self.session.audition_track(Some(*id)) == Some(*id));
                let list_nearest = self.timbre_map.list_nearest;
                let entries: Vec<_> = if list_nearest {
                    neighbors
                        .iter()
                        .map(|(i, distance)| (*i, Some(*distance)))
                        .collect()
                } else {
                    (0..map.sounds.len()).map(|i| (i, None)).collect()
                };
                let list_id = if list_nearest {
                    "timbre-neighbors"
                } else {
                    "timbre-all"
                };
                let narrow = window.viewport_size().width < window.rem_size() * 48.;
                let inspector = div()
                    .flex()
                    .flex_col()
                    .gap(spacing.md)
                    .w(gpui::rems(20.))
                    .when(narrow, |column| {
                        column.w_full().h(gpui::rems(24.)).flex_shrink_0()
                    })
                    .max_w_full()
                    .min_w_0()
                    .min_h(gpui::rems(18.))
                    .child(SectionHeader::new(selected.map_or_else(
                        || self.t(Key::TimbreMapSelect).to_owned(),
                        |s| s.name.clone(),
                    )))
                    .children(
                        self.session
                            .audition_track(self.selected_track)
                            .and_then(|id| self.project().track(id))
                            .map(|track| {
                                Label::new(format!(
                                    "{}: {}",
                                    self.t(Key::TimbreMapAudition),
                                    track.name
                                ))
                                .size(TextSize::Small)
                                .color(TextColor::Muted)
                            }),
                    )
                    .child(
                        Button::new("timbre-use", self.t(Key::TimbreMapUse))
                            .primary()
                            .disabled(selected.is_none() || target.is_none())
                            .full_width()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.adopt_timbre();
                                cx.notify();
                            })),
                    )
                    .child(
                        SegmentedControl::new(
                            "timbre-lists",
                            [
                                Segment::label(self.t(Key::TimbreMapAll)),
                                Segment::label(self.t(Key::TimbreMapNearest)),
                            ],
                            usize::from(list_nearest),
                        )
                        .size(ControlSize::Small)
                        .on_select(cx.listener(
                            |this, i: &usize, _, cx| {
                                this.timbre_map.list_nearest = *i == 1;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        div()
                            .id(list_id)
                            .flex_1()
                            .min_h(gpui::rems(10.))
                            .p(spacing.xs)
                            .overflow_y_scroll()
                            .children(entries.into_iter().map(|(i, distance)| {
                                ListItem::new(
                                    SharedString::from(format!("{list_id}-{i}")),
                                    map.sounds[i].name.clone(),
                                )
                                .selected(self.timbre_map.selected == Some(i))
                                .icon(IconName::Music)
                                .icon_color(cluster_color(map.clusters()[i], &self.theme))
                                .trailing(
                                    Label::new(distance.map_or_else(
                                        || {
                                            format!(
                                                "{} {}",
                                                self.t(Key::TimbreMapGroup),
                                                map.clusters()[i] + 1
                                            )
                                        },
                                        |d| format!("{d:.2}"),
                                    ))
                                    .mono()
                                    .size(TextSize::XSmall)
                                    .color(TextColor::Muted),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.select_timbre(i);
                                        cx.notify();
                                    },
                                ))
                            })),
                    );
                content = content.child(
                    div()
                        .id("timbre-map-body")
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .gap(spacing.lg)
                        .when(narrow, |row| row.flex_col().overflow_y_scroll())
                        .child(figure)
                        .child(inspector),
                );
            }
        } else if !busy {
            content = content.child(EmptyState::new(
                IconName::Music,
                self.t(Key::TimbreMapReady),
            ));
        }
        let summary = self
            .timbre_map
            .map
            .as_ref()
            .map(|map| {
                format!(
                    "{}: {:.0}% · {}: {}",
                    self.t(Key::TimbreMapVariance),
                    map.explained_variance() * 100.,
                    self.t(Key::TimbreMapSkipped),
                    map.skipped.len()
                )
            })
            .unwrap_or_default();
        Some(
            div()
                .id("timbre-map")
                .key_context("AurisVisualizationControls")
                .on_action(|_: &super::widgets::ClaimVisualizationActivation, _, _| {})
                .size_full()
                .flex()
                .flex_col()
                .bg(theme.window_fill())
                .text_color(theme.colors.text)
                .font(theme.typography.ui.clone())
                .occlude()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(spacing.sm)
                        .p(spacing.md)
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(theme.colors.separator)
                        .child(
                            Button::new("timbre-rescan", self.t(Key::TimbreMapRescan))
                                .size(ControlSize::Small)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.scan_timbre_map(cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("timbre-close", self.t(Key::Close))
                                .ghost()
                                .size(ControlSize::Small)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_timbre_map();
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .p(spacing.lg)
                        .child(content),
                )
                .child(StatusBar::new().message(summary).busy(busy))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{click, open, paint};
    use crate::theme::{SCHEMES, Theme};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    fn map_window(app: &Entity<AurisApp>, cx: &mut VisualTestContext) -> VisualTestContext {
        let handle = app.read_with(cx, |app, _| {
            app.auxiliary_windows[&crate::auxiliary_window::Surface::TimbreMap]
        });
        VisualTestContext::from_window(handle.into(), cx)
    }

    #[test]
    fn cluster_colours_follow_every_theme_group_palette() {
        for scheme in SCHEMES {
            let theme = Theme::from_scheme(scheme);
            for cluster in 0..8 {
                assert_eq!(
                    cluster_color(cluster, &theme),
                    theme.group_color(cluster as f32 * 0.618_034),
                    "{} cluster {cluster}",
                    scheme.name
                );
            }
        }
    }

    #[gpui::test]
    fn browsing_is_read_only_and_adoption_is_undoable(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let (before, index) = app.update(cx, |this, _| {
            let track = this
                .session
                .add_default_instrument_track("Map target")
                .unwrap();
            this.select_track(track);
            let map = this
                .session
                .timbre_map_job()
                .unwrap()
                .run(&TimbreMapControl::default())
                .unwrap();
            // Use the first alternate sound: the visible rows remain clickable as the built-in
            // catalogue grows, without relying on FM2 staying above the scroll fold.
            let index = map
                .sounds
                .iter()
                .position(|s| s.instrument_id != auris_session::DEFAULT_INSTRUMENT)
                .unwrap();
            this.timbre_map.map = Some(Arc::new(map));
            this.timbre_map.open = true;
            (this.project().clone(), index)
        });
        paint(&app, cx);
        // GPUI's debug selector API requires a static string, including for dynamic rows.
        let mut utility = map_window(&app, cx);
        // Picking rotates the display only; it must never author instrument changes.
        let (picked, at) = app.read_with(&utility, |this, _| {
            let map = this.timbre_map.map.as_ref().unwrap();
            let (positions, extent) = plot_data(map.positions());
            let camera = &this.timbre_map.camera;
            camera
                .projected(&positions, extent)
                .iter()
                .enumerate()
                .find_map(|(i, (at, _))| {
                    (camera.pick(&positions, extent, *at) == Some(i)).then_some((i, *at))
                })
                .unwrap()
        });
        utility.simulate_click(at, gpui::Modifiers::none());
        app.read_with(&utility, |this, _| {
            assert_eq!(this.timbre_map.selected, Some(picked));
            assert_eq!(this.project(), &before);
        });
        let center = utility.debug_bounds("timbre-plot").unwrap().center();
        crate::harness::drag(
            &mut utility,
            center,
            center + gpui::point(gpui::px(50.), gpui::px(20.)),
        );
        app.read_with(&utility, |this, _| {
            assert_eq!(this.timbre_map.selected, Some(picked));
            assert_eq!(this.timbre_map.camera.preset, None);
            assert_eq!(this.project(), &before);
        });
        // View presets and turn buttons remain reachable without pointer gestures.
        paint(&app, &mut utility);
        click("timbre-views-1", &mut utility);
        app.read_with(&utility, |this, _| {
            assert_eq!(this.timbre_map.camera.elevation, 90.)
        });
        click("timbre-right", &mut utility);
        let azimuth = app.read_with(&utility, |this, _| this.timbre_map.camera.azimuth);
        utility.update(|window, cx| window.focus(&app.read(cx).focus));
        // Rescan, Close, four standard views, then left and right rotation.
        utility.simulate_keystrokes("tab tab tab tab tab tab tab tab");
        utility.simulate_keystrokes("space");
        utility.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("space").unwrap(),
        });
        app.read_with(&utility, |this, _| {
            assert_eq!(this.timbre_map.camera.azimuth, azimuth + 15.)
        });
        paint(&app, &mut utility);
        click(
            Box::leak(format!("timbre-all-{index}").into_boxed_str()),
            &mut utility,
        );
        app.read_with(cx, |this, _| {
            assert_eq!(this.project(), &before);
            assert_eq!(this.timbre_map.selected, Some(index));
        });
        for direction in [
            atelier::Direction::Studio,
            atelier::Direction::Glass,
            atelier::Direction::Mono,
        ] {
            for appearance in [atelier::Appearance::Light, atelier::Appearance::Dark] {
                app.update(&mut utility, |this, cx| {
                    this.theme.atelier = atelier::Theme::new(direction, appearance);
                    this.theme.install(cx);
                    cx.notify();
                });
                crate::harness::resize(
                    &app,
                    &mut utility,
                    gpui::size(gpui::px(900.), gpui::px(700.)),
                );
                assert!(utility.debug_bounds("timbre-plot").is_some());
                crate::harness::resize(
                    &app,
                    &mut utility,
                    gpui::size(gpui::px(640.), gpui::px(540.)),
                );
                assert!(utility.debug_bounds("timbre-plot").is_some());
            }
        }
        crate::harness::resize(
            &app,
            &mut utility,
            gpui::size(gpui::px(900.), gpui::px(700.)),
        );
        paint(&app, cx);
        click("timbre-use", &mut utility);
        app.update(cx, |this, _| {
            assert_ne!(this.project(), &before);
            this.session.undo();
            assert_eq!(this.project(), &before);
        });
        click("timbre-close", &mut utility);
        app.read_with(cx, |this, _| assert!(!this.timbre_map.open));
    }

    #[gpui::test]
    fn closing_and_replacing_the_document_invalidate_pending_results(cx: &mut TestAppContext) {
        let (app, cx) = open(cx);
        let generation = app.update(cx, |this, _| {
            this.timbre_map.open = true;
            this.timbre_map.control = Some(TimbreMapControl::default());
            this.timbre_map.generation
        });
        paint(&app, cx);
        let mut utility = map_window(&app, cx);
        click("timbre-close", &mut utility);
        app.update(cx, |this, _| {
            assert_ne!(this.timbre_map.generation, generation);
            assert!(this.timbre_map.control.is_none());
            this.timbre_map.open = true;
            this.timbre_map.control = Some(TimbreMapControl::default());
            let generation = this.timbre_map.generation;
            this.reset_view();
            assert!(!this.timbre_map.open);
            assert!(this.timbre_map.control.is_none());
            assert_ne!(this.timbre_map.generation, generation);
        });
    }
}
