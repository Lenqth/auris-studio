//! The card that says what an unlabelled control is, and which key also reaches it.
//!
//! A transport bar is a row of glyphs. A triangle is play everywhere and a circle is record
//! everywhere, but the punch, the click and the cycle are three rectangles with marks in them,
//! and nothing on screen said which was which — the only way to find out was to press one and
//! listen. The status bar's panel switches have the same shape of problem: five small marks, and
//! the panel each one opens is a thing you learn by clicking all five.
//!
//! The keystroke is on the card as well, and that is the half a tooltip in this application is
//! for. Every key here is the user's to move, so a printed sheet or a line in a manual would be
//! wrong for anybody who has been in the settings window; asking the keymap at the moment of
//! hovering is the only telling that stays true.
//!
use gpui::{AnyView, App, SharedString, Window};

use crate::theme::Theme;

impl crate::app::AurisApp {
    /// A tooltip naming a control, carrying whatever key currently runs it.
    ///
    /// `command` is an id from [`crate::actions::BINDABLE`], or `""` for a control that is not a
    /// bindable command — the stop button, a panel switch's neighbour, anything reached only by
    /// pressing it. An unknown id draws no chip rather than complaining, which is the same
    /// answer a command with no default key gets and is right for both.
    pub(crate) fn tip(
        &self,
        label: auris_i18n::Key,
        command: &str,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        keyed_tip(self.t(label), self.keystroke_for(command), &self.theme)
    }
}

/// The callback gpui's `tooltip` wants.
///
/// `keystroke` arrives already written the way this platform writes it — the caller has the
/// keymap and this does not. Empty means the command has no key, which is the ordinary case and
/// draws no chip.
pub fn keyed_tip(
    label: impl Into<SharedString>,
    keystroke: impl Into<SharedString>,
    _theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    let keystroke = keystroke.into();
    move |window, cx| {
        atelier::Tooltip::new(label.clone())
            .shortcut(keystroke.clone())
            .build(window, cx)
    }
}
