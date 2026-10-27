//! Shared agent-facing presentation primitives.
//!
//! The editor transcript and the song workshop are different workflows, but they speak with
//! the same two voices. Keeping those voices in one adapter makes the Atelier design language
//! visible in both places and prevents a new entry point from inventing a third chat treatment.

use atelier::agent::{AssistantMessage, ChatNotice, ThinkingIndicator, UserMessage};
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

/// Draws a person's message using the shared trailing bubble treatment.
pub(crate) fn user_message(id: impl Into<ElementId>, text: impl Into<SharedString>) -> AnyElement {
    let id = id.into();
    UserMessage::new(text).id(id).into_any_element()
}

/// Draws an agent answer with the shared identity line and optional real Markdown body.
pub(crate) fn assistant_message(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    body: Option<AnyElement>,
    streaming: bool,
) -> AnyElement {
    let mut message = AssistantMessage::new(text)
        .id(id)
        .name("Auris Agent")
        .streaming(streaming);
    if let Some(body) = body {
        message = message.body(body);
    }
    message.into_any_element()
}

/// Draws a workshop message through the same shared agent transcript primitives.
pub(crate) fn workshop_message(
    id: impl Into<ElementId>,
    user: bool,
    text: impl Into<SharedString>,
) -> AnyElement {
    let id = id.into();
    if user {
        user_message(id, text)
    } else {
        assistant_message(
            id,
            "",
            Some(div().child(text.into()).into_any_element()),
            false,
        )
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
        .gap(theme.atelier.spacing.sm)
        .rounded(theme.atelier.shape.radius_xl)
        .bg(theme.surface_raised)
        .border_1()
        .border_color(if focused {
            theme.accent
        } else if warning {
            theme.warning
        } else {
            theme.border_subtle
        })
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
    let (glyph, style, color) = match action {
        ComposerAction::Send => (
            atelier::IconName::ArrowUp,
            ButtonStyle::Agent,
            theme.atelier.colors.agent,
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
        button.w(theme.atelier.density.control_sm).px_0()
    })
    .child(
        atelier::Icon::new(glyph)
            .size(theme.atelier.typography.size_sm)
            .color(if enabled { color } else { theme.text_faint }),
    )
    .when(!icon_only, |button| button.child(label.clone()))
    .tooltip(super::tooltip::keyed_tip(label, "", theme))
}
