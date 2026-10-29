//! Rotating examples and debounced predictions for the unsent workshop draft.

use super::*;
use auris_agent::input_suggestion::{self, SuggestionJob};
use auris_session::AgentPreferences;
use std::time::{Duration, Instant};

const EXAMPLES: &[Key] = &[
    Key::WorkshopBriefHint,
    Key::WorkshopBriefHintGame,
    Key::WorkshopBriefHintVocal,
    Key::WorkshopBriefHintAmbient,
    Key::WorkshopBriefHintDance,
    Key::WorkshopBriefHintLoop,
];
const EXAMPLE_INTERVAL: Duration = Duration::from_secs(8);
const DEBOUNCE: Duration = Duration::from_millis(1000);
const REQUEST_INTERVAL: Duration = Duration::from_secs(4);

pub(super) struct InputAssist {
    example: usize,
    example_since: Instant,
    draft: String,
    preferences: Option<AgentPreferences>,
    changed: Instant,
    last_request: Option<Instant>,
    attempted: bool,
    available: bool,
    eligible: bool,
    job: Option<SuggestionJob>,
    suffix: Option<String>,
}

impl InputAssist {
    pub(super) fn new(opening: u64, now: Instant) -> Self {
        Self {
            example: opening as usize % EXAMPLES.len(),
            example_since: now,
            draft: String::new(),
            preferences: None,
            changed: now,
            last_request: None,
            attempted: false,
            available: true,
            eligible: false,
            job: None,
            suffix: None,
        }
    }

    pub(super) fn placeholder(&self) -> Key {
        EXAMPLES[self.example]
    }

    pub(super) fn edited(&mut self) {
        self.job = None;
        self.suffix = None;
        self.attempted = false;
        self.changed = Instant::now();
    }

    pub(super) fn dismiss(&mut self) {
        self.job = None;
        self.suffix = None;
        self.attempted = true;
    }

    pub(super) fn pause(&mut self) {
        if self.job.take().is_some() {
            self.attempted = false;
        }
    }

    pub(super) fn suffix<'a>(
        &'a self,
        field: &TextField,
        prefs: &AgentPreferences,
    ) -> Option<&'a str> {
        (field.content() == self.draft
            && self.preferences.as_ref() == Some(prefs)
            && field.marked().is_none()
            && field.selection() == (field.content().len()..field.content().len()))
            .then_some(self.suffix.as_deref())
            .flatten()
    }

    /// Polling never waits for a provider. Unsupported or failing routes leave ordinary input intact.
    pub(super) fn tick(
        &mut self,
        field: &TextField,
        prefs: &AgentPreferences,
        focused_idle: bool,
        language: &str,
        now: Instant,
    ) -> bool {
        let mut changed = false;
        if !field.content().is_empty() || field.marked().is_some() {
            self.example_since = now;
        } else if now.duration_since(self.example_since) >= EXAMPLE_INTERVAL {
            self.example = (self.example + 1) % EXAMPLES.len();
            self.example_since = now;
            changed = true;
        }
        let eligible = focused_idle
            && field.marked().is_none()
            && field.selection() == (field.content().len()..field.content().len())
            && (4..=2048).contains(&field.content().chars().count())
            && !field.content().trim().is_empty()
            && !field.content().trim_start().starts_with('/')
            && input_suggestion::supported(prefs);
        if self.preferences.as_ref() != Some(prefs) {
            self.preferences = Some(prefs.clone());
            self.available = true;
            self.edited();
            self.changed = now;
        }
        if self.draft != field.content() {
            changed |= self.suffix.is_some();
            self.draft = field.content().to_owned();
            self.edited();
            self.changed = now;
        }
        if self.eligible != eligible {
            self.pause();
            self.changed = now;
        }
        self.eligible = eligible;
        if !eligible {
            self.job = None;
            return changed;
        }
        if let Some(result) = self.job.as_ref().and_then(SuggestionJob::poll) {
            self.job = None;
            match result {
                Ok(suffix) => self.suffix = suffix,
                Err(_) => self.available = false,
            }
            return true;
        }
        if self.available
            && !self.attempted
            && now.duration_since(self.changed) >= DEBOUNCE
            && self
                .last_request
                .is_none_or(|last| now.duration_since(last) >= REQUEST_INTERVAL)
        {
            self.attempted = true;
            self.last_request = Some(now);
            match SuggestionJob::start(prefs.clone(), self.draft.clone(), language) {
                Ok(job) => self.job = Some(job),
                Err(_) => self.available = false,
            }
        }
        changed
    }

    #[cfg(test)]
    pub(super) fn offer(&mut self, draft: &str, prefs: &AgentPreferences, suffix: &str) {
        self.draft = draft.into();
        self.preferences = Some(prefs.clone());
        self.suffix = Some(suffix.into());
        self.attempted = true;
    }
}

impl AurisApp {
    pub(super) fn accept_workshop_suggestion(&mut self) {
        if let Some(s) = &mut self.song_workshop
            && !s.busy()
            && s.active_field == Some(0)
            && let Some(suffix) = s
                .input_assist
                .suffix(&s.brief, &s.preferences)
                .map(str::to_owned)
        {
            s.brief.insert(&suffix);
            s.input_assist.edited();
            s.completions.reset();
        }
    }

    pub(super) fn poll_workshop_input_assist(&mut self, cx: &mut Context<Self>) {
        let language = if self.language() == auris_i18n::Language::Japanese {
            "ja"
        } else {
            "en"
        };
        if let Some(s) = &mut self.song_workshop {
            let focused_idle = s.input_focused
                && s.active_field == Some(0)
                && !s.busy()
                && self.menu.is_none()
                && self.attachment_preview.is_none();
            if s.input_assist.tick(
                &s.brief,
                &s.preferences,
                focused_idle,
                language,
                Instant::now(),
            ) {
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_change_between_openings_and_rotate_only_while_empty() {
        let now = Instant::now();
        let mut assist = InputAssist::new(0, now);
        assert_ne!(assist.placeholder(), InputAssist::new(1, now).placeholder());
        let prefs = AgentPreferences::default();
        assert!(assist.tick(
            &TextField::default(),
            &prefs,
            false,
            "en",
            now + EXAMPLE_INTERVAL
        ));
        assert_eq!(assist.placeholder(), Key::WorkshopBriefHintGame);
        let filled = TextField::new("A piano song");
        assist.tick(&filled, &prefs, false, "en", now + EXAMPLE_INTERVAL * 3);
        assert_eq!(assist.placeholder(), Key::WorkshopBriefHintGame);
    }

    #[test]
    fn predictions_disappear_after_edits_model_changes_caret_moves_and_ime() {
        let prefs = AgentPreferences::default();
        let mut assist = InputAssist::new(0, Instant::now());
        let mut field = TextField::new("A calm song");
        field.caret_to_end();
        assist.offer(field.content(), &prefs, " with piano");
        assert_eq!(assist.suffix(&field, &prefs), Some(" with piano"));
        let mut other = prefs.clone();
        other.model = "different".into();
        assert!(assist.suffix(&field, &other).is_none());
        field.place_caret(0, false);
        assert!(assist.suffix(&field, &prefs).is_none());
        field.caret_to_end();
        field.insert(" please");
        assert!(assist.suffix(&field, &prefs).is_none());
        field = TextField::default();
        field.replace_and_mark(0..0, "A calm song", None);
        assert!(assist.suffix(&field, &prefs).is_none());
    }
}
