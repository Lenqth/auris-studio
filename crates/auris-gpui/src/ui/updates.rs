//! Feedback for the user-initiated release check.

use auris_i18n::Key;
use gpui::Context;

use crate::app::AurisApp;
use crate::ui::prompt::{Prompt, Question};
use crate::update_check::{UpdateStatus, check_latest_release};

impl AurisApp {
    pub(crate) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.checking_updates {
            return;
        }
        self.checking_updates = true;
        self.set_status(self.t(Key::UpdateChecking));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { check_latest_release() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.checking_updates = false;
                let title = this.t(Key::CmdCheckForUpdates).to_owned();
                match result {
                    Ok(UpdateStatus::Available { current, latest }) => {
                        let message = this
                            .t(Key::UpdateAvailable)
                            .replace("{current}", &current)
                            .replace("{version}", &latest);
                        this.set_status(message);
                        if this.prompt.is_none() {
                            this.open_prompt(Prompt::ask(
                                title,
                                Question::OpenUpdateRelease { current, latest },
                            ));
                        }
                    }
                    Ok(UpdateStatus::Current(version)) => {
                        let message = this.t(Key::UpdateCurrent).replace("{version}", &version);
                        this.set_status(&message);
                        if this.prompt.is_none() {
                            this.open_prompt(Prompt::notice(title, [message.into()]));
                        }
                    }
                    Ok(UpdateStatus::NoRelease) => {
                        let message = this.t(Key::UpdateNoRelease).to_owned();
                        this.set_status(&message);
                        if this.prompt.is_none() {
                            this.open_prompt(Prompt::notice(title, [message.into()]));
                        }
                    }
                    Err(error) => {
                        log::warn!("update check failed: {error}");
                        let message = this.t(Key::UpdateFailed).to_owned();
                        this.set_failed_status(&message);
                        if this.prompt.is_none() {
                            this.open_prompt(Prompt::notice(title, [message.into()]));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
