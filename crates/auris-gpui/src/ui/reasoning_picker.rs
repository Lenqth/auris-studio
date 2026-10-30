//! Shared composer popover for reasoning effort and native fast mode.

use auris_i18n::{Key, Language};
use auris_session::ReasoningEffort;
use gpui::{prelude::*, *};
use gpui_component::slider::{Slider, SliderEvent, SliderState};

use super::agent_chat::controls::effort_key;
use crate::theme::Theme;

/// Dismiss the topmost reasoning popover before editor-wide Escape bindings run.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = auris_reasoning, no_json)]
pub(crate) struct DismissReasoningPicker;

/// A preference change, applied by the owning conversation.
#[derive(Clone, Copy)]
pub(crate) enum Change {
    Opened,
    Effort(ReasoningEffort),
    Fast,
    Reset,
}

/// A picker and its owner subscription, retained across composer renders.
pub(crate) struct Handle {
    pub(crate) view: Entity<ReasoningPicker>,
    pub(crate) _subscription: Subscription,
}

/// Presentation values supplied by either conversation workflow.
pub(crate) struct Options {
    pub(crate) id: &'static str,
    pub(crate) theme: Theme,
    pub(crate) language: Language,
    pub(crate) model: String,
    pub(crate) choices: Vec<ReasoningEffort>,
    pub(crate) effort: ReasoningEffort,
    pub(crate) fast: bool,
    pub(crate) supports_fast: bool,
    pub(crate) enabled: bool,
}

/// Focus and slider state belong to the popover; preference ownership stays with its caller.
pub(crate) struct ReasoningPicker {
    pub(crate) options: Options,
    focus: FocusHandle,
    slider_focus: FocusHandle,
    open: bool,
    slider: Entity<SliderState>,
    slider_choices: Vec<ReasoningEffort>,
    _subscription: Subscription,
}

impl EventEmitter<Change> for ReasoningPicker {}

impl ReasoningPicker {
    pub(crate) fn new(options: Options, focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        let choices = normalized_choices(&options.choices);
        let (slider, subscription) = Self::slider(&choices, cx);
        Self {
            options,
            focus,
            slider_focus: cx.focus_handle().tab_stop(true),
            open: false,
            slider,
            slider_choices: choices,
            _subscription: subscription,
        }
    }

    fn slider(
        choices: &[ReasoningEffort],
        cx: &mut Context<Self>,
    ) -> (Entity<SliderState>, Subscription) {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max((choices.len().saturating_sub(1)).max(1) as f32)
                .step(1.)
        });
        let subscription = cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            let SliderEvent::Change(value) = event;
            if this.options.enabled
                && let Some(effort) = this.slider_choices.get(value.start().round() as usize)
            {
                cx.emit(Change::Effort(*effort));
                cx.notify();
            }
        });
        (slider, subscription)
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.focus.focus(window);
        cx.notify();
    }
}

/// Keep provider order while always offering a distinct provider-default position.
pub(crate) fn normalized_choices(choices: &[ReasoningEffort]) -> Vec<ReasoningEffort> {
    let mut result = vec![ReasoningEffort::Default];
    for effort in choices {
        if !result.contains(effort) {
            result.push(*effort);
        }
    }
    result
}

impl Render for ReasoningPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let choices = normalized_choices(&self.options.choices);
        if choices != self.slider_choices {
            (self.slider, self._subscription) = Self::slider(&choices, cx);
            self.slider_choices = choices;
        }
        let theme = self.options.theme.clone();
        let language = self.options.language;
        let label = effort_key(self.options.effort).get(language);
        let trigger_label = match self.options.effort {
            ReasoningEffort::Default => Key::AgentEffortDefaultShort,
            ReasoningEffort::Minimal => Key::AgentEffortMinimalShort,
            ReasoningEffort::Medium => Key::AgentEffortMediumShort,
            ReasoningEffort::Xhigh => Key::AgentEffortXhighShort,
            ReasoningEffort::Max => Key::AgentEffortMaxShort,
            effort => effort_key(effort),
        }
        .get(language);
        let enabled = self.options.enabled;
        if !enabled {
            self.open = false;
        }
        let index = self
            .slider_choices
            .iter()
            .position(|e| *e == self.options.effort);
        let value = index.unwrap_or(0) as f32;
        self.slider.update(cx, |slider, cx| {
            if slider.value().start() != value {
                slider.set_value(value, window, cx);
            }
        });
        let fraction = value / self.slider_choices.len().saturating_sub(1).max(1) as f32;
        let id = self.options.id;
        let was_open = self.open;
        let popup_id = format!("{id}-popup");
        let slider_id = format!("{id}-slider");
        div()
            .relative()
            .flex_shrink_0()
            .child(
                super::widgets::menu_picker(
                    id,
                    div()
                        .flex()
                        .items_center()
                        .h_full()
                        .min_w_0()
                        .overflow_hidden()
                        .child(trigger_label),
                    &theme,
                    cx.listener(move |this, _, window, cx| {
                        if !this.options.enabled {
                            return;
                        }
                        this.open = !was_open;
                        if this.open {
                            cx.emit(Change::Opened);
                            this.slider_focus.focus(window);
                        } else {
                            this.focus.focus(window);
                        }
                        cx.notify();
                    }),
                )
                .track_focus(&self.focus.clone().tab_stop(enabled))
                .w_16()
                .bg(if self.open {
                    theme.surface_hover
                } else {
                    gpui::transparent_black()
                })
                .border_color(gpui::transparent_black())
                .when(!enabled, |button| button.opacity(0.5))
                .tooltip(super::tooltip::keyed_tip(
                    Key::AgentEffort.get(language),
                    "",
                    &theme,
                )),
            )
            .when(self.open, |root| {
                root.child(gpui::deferred(
                    div().absolute().bottom_full().right_0().pb_2().child(
                        gpui::anchored()
                            .anchor(gpui::Corner::BottomRight)
                            .snap_to_window_with_margin(theme.atelier.spacing.sm)
                            .child(
                                div()
                                    .id(SharedString::from(popup_id.clone()))
                                    .debug_selector(move || popup_id.clone())
                                    .occlude()
                                    .tab_group()
                                    .key_context("AurisReasoningPicker")
                                    .w_64()
                                    .p_3()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .rounded(theme.atelier.shape.radius_xl)
                                    .bg(theme.surface_raised)
                                    .border_1()
                                    .border_color(theme.border)
                                    .shadow_md()
                                    .text_color(theme.text)
                                    .text_sm()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                        this.open = false;
                                        cx.notify();
                                    }))
                                    .on_action(cx.listener(
                                        |this, _: &DismissReasoningPicker, window, cx| {
                                            this.dismiss(window, cx);
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(div().w_8().when(
                                                self.options.supports_fast,
                                                |slot| {
                                                    slot.child(
                                                        atelier::Button::icon_only(
                                                            "reasoning-fast-mode",
                                                            crate::ui::icons::Icon::Bolt.source(),
                                                        )
                                                        .ghost()
                                                        .size(atelier::ControlSize::Small)
                                                        .selected(self.options.fast)
                                                        .tooltip(format!(
                                                            "{} — {}",
                                                            Key::AgentFastMode.get(language),
                                                            Key::AgentFastModeHelp.get(language)
                                                        ))
                                                        .on_click(cx.listener(|_, _, _, cx| {
                                                            cx.emit(Change::Fast)
                                                        })),
                                                    )
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(theme.accent_text)
                                                    .child(label),
                                            )
                                            .child(
                                                atelier::Button::icon_only(
                                                    "reasoning-reset",
                                                    atelier::IconName::Refresh,
                                                )
                                                .ghost()
                                                .size(atelier::ControlSize::Small)
                                                .tooltip(Key::ReasoningReset.get(language))
                                                .on_click(cx.listener(|_, _, _, cx| {
                                                    cx.emit(Change::Reset)
                                                })),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .text_center()
                                            .truncate()
                                            .child(self.options.model.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(slider_id.clone()))
                                            .debug_selector(move || slider_id.clone())
                                            .track_focus(&self.slider_focus)
                                            .tab_index(0)
                                            .key_context("AurisReasoningPicker AtelierControl")
                                            .relative()
                                            .px_3()
                                            .h_8()
                                            .rounded_full()
                                            .bg(theme.surface_sunken)
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .focus(|style| style.border_color(theme.accent))
                                            .on_key_down(cx.listener(
                                                |this, event: &KeyDownEvent, _, cx| {
                                                    let last =
                                                        this.slider_choices.len().saturating_sub(1);
                                                    let index = this
                                                        .slider_choices
                                                        .iter()
                                                        .position(|e| *e == this.options.effort)
                                                        .unwrap_or(0);
                                                    let next = match event.keystroke.key.as_str() {
                                                        "left" | "down" => index.saturating_sub(1),
                                                        "right" | "up" => (index + 1).min(last),
                                                        "home" => 0,
                                                        "end" => last,
                                                        _ => return,
                                                    };
                                                    cx.emit(Change::Effort(
                                                        this.slider_choices[next],
                                                    ));
                                                    cx.stop_propagation();
                                                },
                                            ))
                                            // Skin the standard slider; pointer and drag handling stay in GPUI Kit.
                                            .child(
                                                div()
                                                    .absolute()
                                                    .inset_0()
                                                    .rounded_full()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .h_full()
                                                            .w(relative(fraction))
                                                            .rounded_l_full()
                                                            .bg(theme.accent),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .relative()
                                                    .h_full()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .inset_0()
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .children(
                                                                self.slider_choices
                                                                    .iter()
                                                                    .enumerate()
                                                                    .map(|(i, _)| {
                                                                        div()
                                                                            .size_1()
                                                                            .rounded_full()
                                                                            .bg(
                                                                                if i <= index
                                                                                    .unwrap_or(0)
                                                                                {
                                                                                    theme
                                                                            .text_on_accent
                                                                            .opacity(0.5)
                                                                                } else {
                                                                                    theme.text_faint
                                                                                },
                                                                            )
                                                                    }),
                                                            ),
                                                    )
                                                    .when(index.is_some(), |rail| {
                                                        rail.child(
                                                            div()
                                                                .absolute()
                                                                .left(relative(fraction))
                                                                .ml(-theme.atelier.spacing.md)
                                                                .size_6()
                                                                .rounded_full()
                                                                .bg(theme.text),
                                                        )
                                                    })
                                                    .child(
                                                        Slider::new(&self.slider)
                                                            .bg(gpui::transparent_black())
                                                            .text_color(gpui::transparent_black())
                                                            .disabled(
                                                                self.slider_choices.len() <= 1,
                                                            ),
                                                    ),
                                            ),
                                    )
                                    .when(index.is_none(), |popup| {
                                        popup.child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.text_muted)
                                                .child(Key::ReasoningUnavailable.get(language)),
                                        )
                                    }),
                            ),
                    ),
                ))
            })
    }
}
