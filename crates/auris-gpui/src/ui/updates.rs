//! Background update flow and its desktop feedback.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use auris_i18n::Key;
use gpui::{Context, Window};

use crate::app::AurisApp;
use crate::ui::prompt::{PendingAction, Prompt, PromptBody, Question};
use crate::update_check::{
    ReadyUpdate, ReleaseAsset, UpdateOutcome, UpdateStatus, check_latest_release, download_update,
    installed_target, take_update_result,
};

/// One release operation at a time, with a verified package retained until quit.
#[derive(Debug)]
pub(crate) enum UpdatePhase {
    Idle,
    Checking {
        manual: bool,
    },
    Downloading {
        version: String,
        total: u64,
        received: Arc<AtomicU64>,
        cancelled: Arc<AtomicBool>,
    },
    Ready(ReadyUpdate),
}

impl AurisApp {
    /// Changes the background update preference and discards pending work when it is disabled.
    pub(crate) fn apply_auto_update(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let was_enabled = self.settings.auto_update;
        self.settings.auto_update = enabled;
        if !enabled {
            let was_updating = !matches!(self.update_phase, UpdatePhase::Idle);
            if let UpdatePhase::Downloading { cancelled, .. } = &self.update_phase {
                cancelled.store(true, Ordering::Relaxed);
            }
            self.update_generation = self.update_generation.wrapping_add(1);
            self.update_phase = UpdatePhase::Idle;
            self.restart_after_update = false;
            if matches!(
                self.prompt.as_ref().map(|prompt| &prompt.body),
                Some(PromptBody::Ask(Question::RestartForUpdate { .. }))
            ) {
                self.cancel_prompt();
            }
            if was_updating {
                self.set_status("");
            }
        }
        if let Err(error) = self.settings.save() {
            log::warn!("could not save settings: {error}");
        }
        if enabled && !was_enabled && installed_target().is_some() {
            self.check_for_updates_mode(false, cx);
        }
        cx.notify();
    }

    /// Starts one quiet check shortly after launch and repeats while the app remains open.
    pub(crate) fn start_auto_updates(&mut self, cx: &mut Context<Self>) {
        if let Some(UpdateOutcome::Failure(detail)) = take_update_result() {
            log::warn!("previous update failed: {detail}");
            let message = self.t(Key::UpdateInstallFailed).to_owned();
            self.set_failed_status(&message);
            if self.prompt.is_none() {
                self.open_prompt(Prompt::notice(
                    self.t(Key::CmdCheckForUpdates),
                    [message.into()],
                ));
            }
            cx.notify();
        }
        if installed_target().is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let mut delay = Duration::from_secs(10);
            loop {
                cx.background_executor().timer(delay).await;
                if this
                    .update(cx, |this, cx| {
                        if this.settings.auto_update {
                            this.check_for_updates_mode(false, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                delay = Duration::from_secs(6 * 60 * 60);
            }
        })
        .detach();
    }

    pub(crate) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        self.check_for_updates_mode(true, cx);
    }

    fn check_for_updates_mode(&mut self, manual: bool, cx: &mut Context<Self>) {
        if let UpdatePhase::Ready(update) = &self.update_phase {
            if manual {
                let version = update.latest.clone();
                self.show_ready_prompt(version);
                cx.notify();
            }
            return;
        }
        if let UpdatePhase::Checking { manual: was_manual } = &mut self.update_phase {
            if manual && !*was_manual {
                *was_manual = true;
                self.set_status(self.t(Key::UpdateChecking));
                cx.notify();
            }
            return;
        }
        if matches!(self.update_phase, UpdatePhase::Downloading { .. }) {
            return;
        }
        self.update_phase = UpdatePhase::Checking { manual };
        self.update_generation = self.update_generation.wrapping_add(1);
        let generation = self.update_generation;
        if manual {
            self.set_status(self.t(Key::UpdateChecking));
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { check_latest_release() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_update_check(result, generation, cx)
            });
        })
        .detach();
    }

    fn finish_update_check(
        &mut self,
        result: Result<UpdateStatus, crate::update_check::CheckError>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if self.update_generation != generation {
            return;
        }
        let UpdatePhase::Checking { manual } = &self.update_phase else {
            return;
        };
        let manual = *manual;
        self.update_phase = UpdatePhase::Idle;
        let title = self.t(Key::CmdCheckForUpdates).to_owned();
        match result {
            Ok(UpdateStatus::Available {
                current,
                latest,
                asset,
            }) => {
                if let (Some(asset), Some(target)) = (asset, installed_target()) {
                    self.start_update_download(asset, latest, target, manual, cx);
                    return;
                }
                let message = self
                    .t(Key::UpdateAvailable)
                    .replace("{current}", &current)
                    .replace("{version}", &latest);
                self.set_status(&message);
                if manual && self.prompt.is_none() {
                    self.open_prompt(Prompt::ask(
                        title,
                        Question::OpenUpdateRelease { current, latest },
                    ));
                }
            }
            Ok(UpdateStatus::Current(version)) if manual => {
                let message = self.t(Key::UpdateCurrent).replace("{version}", &version);
                self.set_status(&message);
                if self.prompt.is_none() {
                    self.open_prompt(Prompt::notice(title, [message.into()]));
                }
            }
            Ok(UpdateStatus::NoRelease) if manual => {
                let message = self.t(Key::UpdateNoRelease).to_owned();
                self.set_status(&message);
                if self.prompt.is_none() {
                    self.open_prompt(Prompt::notice(title, [message.into()]));
                }
            }
            Ok(_) => {}
            Err(error) => {
                log::warn!("update check failed: {error}");
                if manual {
                    let message = self.t(Key::UpdateFailed).to_owned();
                    self.set_failed_status(&message);
                    if self.prompt.is_none() {
                        self.open_prompt(Prompt::notice(title, [message.into()]));
                    }
                }
            }
        }
        cx.notify();
    }

    fn start_update_download(
        &mut self,
        asset: ReleaseAsset,
        latest: String,
        target: crate::update_check::InstallTarget,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_generation = self.update_generation.wrapping_add(1);
        let generation = self.update_generation;
        let received = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        self.update_phase = UpdatePhase::Downloading {
            version: latest.clone(),
            total: asset.size,
            received: Arc::clone(&received),
            cancelled: Arc::clone(&cancelled),
        };
        self.set_status(
            self.t(Key::UpdateDownloading)
                .replace("{version}", &latest)
                .replace("{percent}", "0"),
        );
        cx.notify();
        let worker_received = Arc::clone(&received);
        let worker_cancelled = Arc::clone(&cancelled);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    download_update(&asset, &latest, target, &worker_received, &worker_cancelled)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.update_generation != generation
                    || !matches!(this.update_phase, UpdatePhase::Downloading { .. })
                {
                    return;
                }
                match result {
                    Ok(ready) => {
                        let version = ready.latest.clone();
                        this.update_phase = UpdatePhase::Ready(ready);
                        this.set_status(this.t(Key::UpdateReady).replace("{version}", &version));
                        this.show_ready_prompt(version);
                    }
                    Err(error) => {
                        log::warn!("update download failed: {error}");
                        this.update_phase = UpdatePhase::Idle;
                        if manual {
                            this.set_failed_status(this.t(Key::UpdateDownloadFailed));
                            if this.prompt.is_none() {
                                this.open_prompt(Prompt::notice(
                                    this.t(Key::CmdCheckForUpdates),
                                    [this.t(Key::UpdateDownloadFailed).into()],
                                ));
                            }
                        } else {
                            this.set_status("");
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let keep_polling = this
                    .update(cx, |this, cx| {
                        if this.update_generation != generation {
                            return false;
                        }
                        let UpdatePhase::Downloading {
                            version,
                            total,
                            received,
                            ..
                        } = &this.update_phase
                        else {
                            return false;
                        };
                        let percent = received.load(Ordering::Relaxed).saturating_mul(100) / *total;
                        let message = this
                            .t(Key::UpdateDownloading)
                            .replace("{version}", version)
                            .replace("{percent}", &percent.to_string());
                        if this.status != message {
                            this.set_status(message);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn show_ready_prompt(&mut self, version: String) {
        if self.prompt.is_none() {
            self.open_prompt(Prompt::ask(
                self.t(Key::CmdCheckForUpdates),
                Question::RestartForUpdate { version },
            ));
        }
    }

    /// Use the same save guard as Quit before requesting an update restart.
    pub(crate) fn restart_for_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.update_phase, UpdatePhase::Ready(_)) {
            return;
        }
        crate::termination::activate(window, cx);
        if self.compose_progress.is_some() {
            return;
        }
        if self.confirm_discard(PendingAction::RestartForUpdate) {
            self.complete_update_restart(cx);
        }
        cx.notify();
    }

    pub(crate) fn complete_update_restart(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update_phase, UpdatePhase::Ready(_)) {
            self.restart_after_update = true;
            self.save_window_placement();
            crate::termination::quit(cx);
        }
    }
}
