//! Shared agent-facing presentation primitives.
//!
//! The editor transcript and the song workshop are different workflows, but they speak with
//! the same two voices. Keeping those voices in one adapter makes the Atelier design language
//! visible in both places and prevents a new entry point from inventing a third chat treatment.

use atelier::agent::{AssistantMessage, ChatNotice, ThinkingIndicator};
use gpui::prelude::*;
use gpui::{AnyElement, ElementId, IntoElement, ParentElement, SharedString, div};

use super::widgets::{ButtonState, ButtonStyle, button_enabled};
use crate::theme::Theme;

/// Introduces a new conversation without impersonating a generated assistant message.
pub(crate) fn conversation_intro(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.atelier.spacing.md)
        .py(theme.atelier.spacing.xl)
        .child(atelier::Avatar::agent())
        .child(
            div()
                .text_size(theme.atelier.typography.size_xl)
                .font_weight(theme.atelier.typography.heading_weight)
                .child(title.into()),
        )
        .child(
            div()
                .text_size(theme.atelier.typography.size_md)
                .text_color(theme.text_muted)
                .child(description.into()),
        )
        .into_any_element()
}

struct TextSelection {
    text: SharedString,
    anchor: usize,
    head: usize,
    dragging: bool,
    focus: gpui::FocusHandle,
}

impl TextSelection {
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

/// Read-only text using shaped glyph positions and the original UTF-8 source for selection.
/// Unlike a Markdown/HTML parser, this preserves blank lines, indentation and literal markup.
pub(crate) fn selectable_text(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> gpui::Div {
    let id = id.into();
    let text = text.into();
    let state = window.use_keyed_state((id.clone(), "selection"), cx, |_, cx| TextSelection {
        text: text.clone(),
        anchor: 0,
        head: 0,
        dragging: false,
        focus: cx.focus_handle(),
    });
    let (range, focus) = state.update(cx, |state, _| {
        if state.text != text {
            state.text = text.clone();
            state.anchor = 0;
            state.head = 0;
            state.dragging = false;
        }
        (state.range(), state.focus.clone())
    });
    use gpui_component::ActiveTheme;
    let selection_color = cx.theme().selection;
    let element = gpui::StyledText::new(text).with_highlights([(
        range,
        gpui::HighlightStyle {
            background_color: Some(selection_color),
            ..Default::default()
        },
    )]);
    let layout = element.layout().clone();
    let copy_state = state.clone();
    let selector = id.to_string();
    div()
        .debug_selector(move || selector.clone())
        .w_full()
        .min_w_0()
        .cursor_text()
        .child(
            div()
                .id(id)
                .w_full()
                .min_w_0()
                .relative()
                .track_focus(&focus)
                .key_context("TextView")
                .on_action(move |_: &gpui_component::input::Copy, _, cx| {
                    let state = copy_state.read(cx);
                    if !state.range().is_empty() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            state.text[state.range()].to_owned(),
                        ));
                    }
                })
                .child(element)
                .child(
                    gpui::canvas(
                        |bounds, window, _| {
                            window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                        },
                        move |bounds, hitbox: gpui::Hitbox, window, _| {
                            let parent = window.current_view();
                            let down_state = state.clone();
                            let down_layout = layout.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                    if !phase.bubble() || event.button != gpui::MouseButton::Left {
                                        return;
                                    }
                                    down_state.update(cx, |state, _| {
                                        if hitbox.is_hovered(window)
                                            && bounds.contains(&event.position)
                                        {
                                            let index = down_layout
                                                .index_for_position(event.position)
                                                .unwrap_or_else(|index| index)
                                                .min(state.text.len());
                                            if !event.modifiers.shift {
                                                state.anchor = index;
                                            }
                                            state.head = index;
                                            state.dragging = true;
                                        } else {
                                            state.anchor = 0;
                                            state.head = 0;
                                            state.dragging = false;
                                        }
                                    });
                                    cx.notify(parent);
                                },
                            );
                            let move_state = state.clone();
                            let move_layout = layout.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                    if !phase.bubble() || !move_state.read(cx).dragging {
                                        return;
                                    }
                                    move_state.update(cx, |state, _| {
                                        state.head = move_layout
                                            .index_for_position(event.position)
                                            .unwrap_or_else(|index| index)
                                            .min(state.text.len());
                                        state.dragging =
                                            event.pressed_button == Some(gpui::MouseButton::Left);
                                    });
                                    cx.notify(parent);
                                },
                            );
                            let up_state = state.clone();
                            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, cx| {
                                if phase.bubble() {
                                    up_state.update(cx, |state, _| state.dragging = false);
                                }
                            });
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    struct Fixture;
    impl gpui::Render for Fixture {
        fn render(
            &mut self,
            window: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div()
                .p(gpui::px(80.0))
                .w(gpui::px(560.0))
                .child(selectable_text(
                    "fixture-text",
                    "Plain <text> & **stars**\n\n  日本語",
                    window,
                    cx,
                ))
        }
    }
    #[gpui::test]
    fn plain_selection_preserves_whitespace_and_literal_markup(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let (view, cx) = cx.add_window_view(|_, _| Fixture);
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        let bounds = cx.debug_bounds("fixture-text").unwrap();
        crate::harness::press(
            cx,
            bounds.origin + gpui::point(gpui::px(0.5), gpui::px(0.5)),
        );
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        let end = bounds.bottom_right() - gpui::point(gpui::px(0.5), gpui::px(0.5));
        crate::harness::drag_to(cx, end);
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        crate::harness::release(cx, end);
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-c");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
                Some("Plain <text> & **stars**\n\n  日本語")
            )
        });
        crate::harness::press(
            cx,
            bounds.origin + gpui::point(gpui::px(20.0), gpui::px(5.0)),
        );
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        let end = bounds.origin + gpui::point(gpui::px(70.0), gpui::px(5.0));
        crate::harness::drag_to(cx, end);
        crate::harness::release(cx, end);
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-c");
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(!copied.is_empty());
            assert!(copied.len() < "Plain <text> & **stars**".len());
            assert!("Plain <text> & **stars**".contains(&copied));
        });
    }
}

/// Draws a person's selectable message with Atelier's trailing bubble geometry and tokens.
pub(crate) fn user_message(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    theme: &Theme,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> AnyElement {
    let id = id.into();
    let atelier = &theme.atelier;
    let body = selectable_text((id.clone(), "text"), text, window, cx);
    let message = div()
        .flex()
        .flex_col()
        .items_end()
        .pl(atelier.spacing.xxl)
        .child(
            div()
                .max_w_full()
                .px(atelier.spacing.md)
                .py(atelier.spacing.sm)
                .rounded(atelier.shape.radius_lg)
                .bg(atelier.colors.user_bubble)
                .text_color(atelier.colors.on_user_bubble)
                .text_size(atelier.typography.size_md)
                .child(body),
        );
    atelier::motion::appear(message, id, cx, |row, delta| row.opacity(delta))
}

/// Draws an agent answer: the prose alone, with no avatar or name above it.
///
/// Both conversations have exactly two voices, and the person's own sits in an accent bubble on
/// the trailing edge; whatever is not in a bubble is the agent, so the identity line is left out.
pub(crate) fn assistant_message(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    body: Option<AnyElement>,
    streaming: bool,
) -> AnyElement {
    AssistantMessage::new(text)
        .id(id)
        .anonymous()
        .streaming(streaming)
        .when_some(body, |message, body| message.body(body))
        .into_any_element()
}

/// Draws a workshop message through the same shared agent transcript primitives.
///
/// The agent's side is Markdown, rendered the way the editor's transcript renders it; the
/// person's side is shown as typed.
pub(crate) fn workshop_message(
    id: impl Into<ElementId>,
    user: bool,
    text: impl Into<SharedString>,
    theme: &Theme,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> AnyElement {
    let id = id.into();
    let text = text.into();
    if user {
        user_message(id, text, theme, window, cx)
    } else {
        let body = super::agent_markdown::render(
            SharedString::from(format!("workshop-markdown-{id}")),
            text.clone(),
            theme,
            window,
            cx,
        );
        assistant_message(id, text, Some(body), false)
    }
}

/// Draws runtime feedback with the same shape in both workflows.
pub(crate) fn error_notice(text: impl Into<SharedString>) -> AnyElement {
    ChatNotice::error(text).into_any_element()
}

/// Draws a quiet runtime milestone between transcript entries.
pub(crate) fn status_notice(text: impl Into<SharedString>) -> AnyElement {
    ChatNotice::status(text).into_any_element()
}

/// Draws an active agent phase with the shared avatar and reduced-motion-aware indicator.
pub(crate) fn thinking_indicator(id: &'static str, label: impl Into<SharedString>) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .min_w_0()
        .child(ThinkingIndicator::new(label))
        .into_any_element()
}

/// Starts the shared composer shell used by the editor and workshop.
///
/// Callers append their own editable field and command row so each workflow keeps its existing
/// keyboard and permission semantics while the containment, radius, inset and focus treatment
/// stay identical.
pub(crate) fn composer_frame(
    id: impl Into<ElementId>,
    theme: &Theme,
    focused: bool,
    warning: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .relative()
        .flex_shrink_0()
        .min_w_0()
        .m(theme.atelier.spacing.md)
        .p(theme.atelier.spacing.sm)
        .gap(theme.atelier.spacing.xs)
        // Atelier's `Composer`: a raised, hairlined well with the large radius, lit by the accent
        // while it has focus.
        .rounded(theme.atelier.shape.radius_lg)
        .bg(theme.atelier.colors.elevated)
        .border_1()
        .border_color(if focused {
            theme.accent.opacity(0.7)
        } else if warning {
            theme.warning
        } else {
            theme.atelier.colors.border_strong
        })
        .shadow(theme.atelier.elevation.low.clone())
}

/// The commands shared by both composers; each workflow owns their availability and effects.
#[derive(Clone, Copy)]
pub(crate) enum ComposerAction {
    Send,
    Stop,
}

/// A wrapping model/options/action lane inside the shared composer.
pub(crate) fn composer_controls(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .flex_wrap()
        .items_center()
        .min_w_0()
        .gap(theme.atelier.spacing.xs)
}

/// Shared keyboard-operable send/stop control, compact in a dock and labelled on a full page.
pub(crate) fn composer_action(
    id: &'static str,
    label: impl Into<SharedString>,
    action: ComposerAction,
    enabled: bool,
    icon_only: bool,
    theme: &Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    // As in Atelier's `Composer`: send is the box's one primary commit, stop an ordinary action.
    let (glyph, style, color) = match action {
        ComposerAction::Send => (
            atelier::IconName::ArrowUp,
            ButtonStyle::Primary,
            theme.text_on_accent,
        ),
        ComposerAction::Stop => (atelier::IconName::Stop, ButtonStyle::Normal, theme.text),
    };
    button_enabled(
        id,
        "",
        style,
        ButtonState::available(false, enabled),
        color,
        theme,
        on_click,
    )
    .gap(theme.atelier.spacing.xs)
    .when(icon_only, |button| {
        // The square follows the composer's corner, as Atelier's send button does.
        let size = theme.atelier.density.control_md;
        button.size(size).px_0().rounded(
            atelier::Theme::concentric(theme.atelier.shape.radius_lg, theme.atelier.spacing.sm)
                .min(size / 2.0),
        )
    })
    .child(
        atelier::Icon::new(glyph)
            .size(theme.atelier.typography.size_sm)
            .color(if enabled { color } else { theme.text_faint }),
    )
    .when(!icon_only, |button| button.child(label.clone()))
    .tooltip(super::tooltip::keyed_tip(label, "", theme))
}
