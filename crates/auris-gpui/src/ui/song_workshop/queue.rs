//! Messages written while the director or a composer is still at work.
//!
//! The workshop owner accepts a message only when it is idle, so the composer used to go dark for
//! as long as a round took. Now Enter puts the message in line and it goes out by itself once the
//! workshop is free; "Send now" (or the secondary modifier with Enter) stops the work in progress
//! and puts the message at the front. The same two gestures the Agent panel has.
use super::*;

/// One message waiting for the workshop to be idle again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QueuedBrief {
    pub(crate) text: String,
    pub(crate) attachments: Vec<std::path::PathBuf>,
}

impl AurisApp {
    /// Moves the composer's message into the queue, at the back or, to interrupt, the front.
    ///
    /// `false` when there is nothing to queue or it cannot be sent as it stands, which leaves the
    /// composer exactly as it was.
    pub(super) fn enqueue_workshop_brief(&mut self, front: bool) -> bool {
        let language = self.language;
        let Some(s) = &mut self.song_workshop else {
            return false;
        };
        let text = s.brief.content().trim();
        if text.is_empty()
            || s.brief.marked().is_some()
            || s.switching
            || s.references.midi_loading
            || !s.preferences.is_configured()
        {
            return false;
        }
        if !s
            .references
            .attachments
            .iter()
            .all(|path| s.references.attached_here.contains(path))
        {
            s.references.attachment_error = Some(Key::AgentQueueReattach.get(language).into());
            return false;
        }
        let message = QueuedBrief {
            text: text.to_owned(),
            attachments: std::mem::take(&mut s.references.attachments),
        };
        if s.queued.is_empty() {
            s.queue_paused = false;
        }
        if front {
            s.queued.push_front(message);
        } else {
            s.queued.push_back(message);
        }
        s.brief = TextField::default();
        s.references.attachment_error = None;
        s.command_feedback = None;
        s.completions.reset();
        s.input_assist.edited();
        true
    }

    /// Hands the oldest queued message to the workshop once it is idle. `true` when one went.
    pub(super) fn dispatch_queued_workshop_brief(&mut self) -> bool {
        let Some(s) = &mut self.song_workshop else {
            return false;
        };
        if s.busy() || s.queue_paused || s.in_flight.is_some() {
            return false;
        }
        let Some(message) = s.queued.pop_front() else {
            return false;
        };
        if !s.send(Command::Send(
            message.text.clone(),
            Box::new(s.preferences.clone()),
            message.attachments.clone(),
        )) {
            s.queued.push_front(message);
            s.queue_paused = true;
            s.error =
                Some("The song conversation is unavailable. Close and reopen it to retry.".into());
            return true;
        }
        s.submitted = true;
        s.error = None;
        s.in_flight = Some(message);
        s.scroll.scroll_to_bottom();
        true
    }

    /// Puts the queued message at `index` first and stops whatever is running so it goes next.
    pub(super) fn prioritize_workshop_brief(&mut self, index: usize) {
        let Some(s) = &mut self.song_workshop else {
            return;
        };
        let Some(message) = s.queued.remove(index) else {
            return;
        };
        s.queued.push_front(message);
        s.queue_paused = false;
        if s.busy() && !s.switching {
            s.stop();
        }
        self.dispatch_queued_workshop_brief();
    }

    /// The composer's message, sent ahead of everything: queued at the front, then prioritised.
    pub(super) fn submit_workshop_brief_now(&mut self) {
        if self.enqueue_workshop_brief(true) {
            self.prioritize_workshop_brief(0);
        }
    }

    /// The waiting messages, drawn over the composer while there are any.
    pub(super) fn workshop_queue_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.song_workshop.as_ref()?;
        if s.queued.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let paused = s.queue_paused;
        let title = self
            .t(if paused {
                Key::AgentQueuePaused
            } else {
                Key::AgentQueued
            })
            .replace("{count}", &s.queued.len().to_string());
        let rows = s.queued.iter().enumerate().map(|(index, message)| {
            let preview = message
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let preview = if message.attachments.is_empty() {
                preview
            } else {
                format!(
                    "{preview} · {}",
                    self.t(Key::AgentQueuedAttachments)
                        .replace("{count}", &message.attachments.len().to_string())
                )
            };
            div()
                .id(("workshop-queued-message", index))
                .debug_selector(move || format!("workshop-queued-message-{index}"))
                .flex()
                .items_center()
                .min_w_0()
                .gap(theme.atelier.spacing.xs)
                .child(div().flex_1().min_w_0().text_sm().truncate().child(preview))
                .child(
                    Button::new(("workshop-queued-now", index), self.t(Key::AgentSendNow))
                        .ghost()
                        .size(atelier::ControlSize::Small)
                        .disabled(s.switching)
                        .tooltip(self.t(Key::AgentSendNowHint))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.prioritize_workshop_brief(index);
                            cx.notify();
                        })),
                )
                .child(
                    Button::icon_only(("workshop-queued-remove", index), IconName::Close)
                        .ghost()
                        .size(atelier::ControlSize::Small)
                        .tooltip(self.t(Key::AgentQueueRemove))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(s) = &mut this.song_workshop {
                                s.queued.remove(index);
                            }
                            cx.notify();
                        })),
                )
        });
        Some(
            div()
                .id("workshop-queue")
                .debug_selector(|| "workshop-queue".into())
                .flex()
                .flex_col()
                .min_w_0()
                .gap(theme.atelier.spacing.xs)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.atelier.spacing.xs)
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(title),
                        )
                        .child(
                            Button::new(
                                "workshop-queue-toggle",
                                self.t(if paused {
                                    Key::AgentQueueResume
                                } else {
                                    Key::AgentQueuePause
                                }),
                            )
                            .ghost()
                            .size(atelier::ControlSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(s) = &mut this.song_workshop {
                                    s.queue_paused = !s.queue_paused;
                                }
                                this.dispatch_queued_workshop_brief();
                                cx.notify();
                            })),
                        ),
                )
                .children(rows)
                .into_any_element(),
        )
    }
}
