//! Chord and sound-map references use the same immutable attachment path as files.

use super::*;
use crate::ui::prompt::{Prompt, PromptTarget};
use auris_session::prelude::Ticks;

/// Independent draft references for each conversation surface.
#[derive(Default)]
pub(crate) struct MusicReferences {
    pub(crate) attachments: Vec<PathBuf>,
    // Restored paths are display metadata until explicitly attached in this window.
    pub(crate) attached_here: std::collections::HashSet<PathBuf>,
    pub(crate) attachment_error: Option<String>,
    pub(crate) midi_draft: Option<super::midi::MidiDraft>,
    pub(crate) midi_loading: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum MusicOwner {
    Workshop(u64),
    Agent(Option<PathBuf>, Option<String>),
}

impl AurisApp {
    pub(crate) fn attach_music_files(&mut self, paths: &[PathBuf], cx: &mut gpui::Context<Self>) {
        if self.song_workshop.is_none() {
            self.agent_attach_files(paths, cx);
            return;
        }
        if self.music_composer_locked() {
            return;
        }
        for path in paths {
            if auris_agent::attachment_kind(path).is_none() {
                self.music_references_mut().attachment_error = Some(
                    self.t(Key::AgentAttachmentUnsupported)
                        .replace("{name}", &path.to_string_lossy()),
                );
                continue;
            }
            if self.music_references().attachments.contains(path) {
                self.music_references_mut()
                    .attached_here
                    .insert(path.clone());
                self.music_references_mut().attachment_error = None;
                continue;
            }
            if !self.can_attach_agent_music() {
                self.music_references_mut().attachment_error =
                    Some(self.t(Key::AgentAttachmentLimit).into());
                break;
            }
            self.music_references_mut().attachments.push(path.clone());
            self.music_references_mut()
                .attached_here
                .insert(path.clone());
        }
        cx.notify();
    }

    pub(crate) fn music_owner(&self) -> MusicOwner {
        self.song_workshop.as_ref().map_or_else(
            || {
                MusicOwner::Agent(
                    self.session.path().map(Path::to_path_buf),
                    self.agent_chat.conversations.active_id().map(str::to_owned),
                )
            },
            |workshop| MusicOwner::Workshop(workshop.identity),
        )
    }

    pub(crate) fn music_references(&self) -> &MusicReferences {
        self.song_workshop
            .as_ref()
            .map_or(&self.agent_chat.references, |s| &s.references)
    }

    pub(crate) fn music_references_mut(&mut self) -> &mut MusicReferences {
        self.song_workshop
            .as_mut()
            .map_or(&mut self.agent_chat.references, |s| &mut s.references)
    }

    pub(crate) fn music_composer_locked(&self) -> bool {
        self.song_workshop.as_ref().map_or_else(
            || {
                self.agent_chat.pending_send.is_some()
                    || self.agent_chat.conversations.changing_conversation()
                    || self.agent_chat.conversations.previewing()
            },
            |s| s.busy(),
        )
    }

    pub(crate) fn can_attach_agent_music(&self) -> bool {
        !self.music_composer_locked()
            && self.music_references().attachments.len() < auris_agent::ATTACHMENT_COUNT_LIMIT
    }

    pub(crate) fn attach_agent_music_reference(
        &mut self,
        value: &serde_json::Value,
        label: &str,
    ) -> Result<(), String> {
        if !self.can_attach_agent_music() {
            return Err(self
                .t(
                    if self.music_references().attachments.len()
                        >= auris_agent::ATTACHMENT_COUNT_LIMIT
                    {
                        Key::AgentAttachmentLimit
                    } else {
                        Key::AgentMusicUnavailable
                    },
                )
                .to_owned());
        }
        let directory = auris_session::settings::config_dir().join("conversation-midi");
        let path = auris_session::music_reference::save_snapshot(value, &directory, label)?;
        self.music_references_mut()
            .attached_here
            .insert(path.clone());
        self.music_references_mut().attachments.push(path);
        self.music_references_mut().attachment_error = None;
        Ok(())
    }

    pub(crate) fn open_agent_chords(&mut self) {
        if !self.can_attach_agent_music() {
            return;
        }
        let key = self.session.harmony().key_at(Ticks::ZERO);
        let meter = self.project().signatures.signature_at(Ticks::ZERO);
        self.open_prompt(Prompt::new(
            format!(
                "{} — {} · {}/{}",
                self.t(Key::AgentChords),
                key.to_text(),
                meter.numerator,
                meter.denominator
            ),
            PromptTarget::AgentChordReference { key, meter },
            "",
        ));
    }

    /// Replace the references menu with the installed skills, listed off the UI thread.
    pub(crate) fn open_skill_menu(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.can_attach_agent_music() {
            return;
        }
        self.skill_menu_generation = self.skill_menu_generation.wrapping_add(1);
        let request = self.skill_menu_generation;
        let mut menu = self.skill_menu(at, None);
        menu.async_request = Some(request);
        self.open_menu(menu);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let skills = cx
                .background_executor()
                .spawn(async { auris_agent::skills::installed_skills() })
                .await;
            let _ = this.update(cx, |this, cx| this.finish_skill_menu(request, skills, cx));
        })
        .detach();
    }

    /// Fill the skill menu that asked, unless it has been closed or replaced since.
    pub(crate) fn finish_skill_menu(
        &mut self,
        request: u64,
        skills: Vec<auris_agent::skills::InstalledSkill>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(at) = self
            .menu
            .as_ref()
            .filter(|menu| {
                menu.async_request == Some(request) && request == self.skill_menu_generation
            })
            .map(|menu| menu.anchor)
        else {
            return;
        };
        let mut menu = self.skill_menu(at, Some(skills));
        menu.async_request = Some(request);
        self.open_menu(menu);
        cx.notify();
    }

    /// `None` while the folders are still being read.
    fn skill_menu(
        &self,
        at: gpui::Point<gpui::Pixels>,
        skills: Option<Vec<auris_agent::skills::InstalledSkill>>,
    ) -> ContextMenu {
        let mut menu = ContextMenu::new(at, self.t(Key::AgentSkillsTitle));
        menu = match skills {
            None => menu.item_greyed_unless(
                false,
                self.t(Key::AgentSkillsLoading),
                MenuCommand::ComposerSkills(at),
            ),
            Some(skills) if skills.is_empty() => menu.item_greyed_unless(
                false,
                self.t(Key::AgentSkillsNone),
                MenuCommand::ComposerSkills(at),
            ),
            Some(skills) => skills.into_iter().fold(menu, |menu, skill| {
                menu.item(
                    format!("{} — {}", skill.name, skill.source.label()),
                    MenuCommand::ComposerSkill(skill.path),
                )
            }),
        };
        let mut menu = menu
            .separator()
            .item(self.t(Key::AgentSkillFile), MenuCommand::ComposerSkillFile);
        menu.step(1);
        menu
    }

    /// Attach a frozen copy of a skill, so editing the installed one later cannot change what
    /// this conversation was told.
    pub(crate) fn attach_skill(&mut self, path: &Path, cx: &mut gpui::Context<Self>) {
        if !self.can_attach_agent_music() {
            return;
        }
        let directory = auris_session::settings::config_dir().join("conversation-skills");
        match auris_agent::skills::snapshot_skill(path, &directory) {
            Ok(copy) => self.attach_music_files(std::slice::from_ref(&copy), cx),
            Err(error) => {
                self.music_references_mut().attachment_error =
                    Some(self.t(Key::AgentSkillUnreadable).replace("{error}", &error));
                cx.notify();
            }
        }
    }

    pub(crate) fn choose_skill_file(&mut self, cx: &mut gpui::Context<Self>) {
        let owner = self.music_owner();
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("SKILL.md", &["md"])
                .pick_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |this, cx| {
                if this.music_owner() == owner {
                    this.attach_skill(&path, cx);
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use crate::harness::{click, open, paint};

    #[gpui::test]
    fn chord_prompt_attaches_timed_reference_without_editing_song(cx: &mut gpui::TestAppContext) {
        let (app, cx) = open(cx);
        app.update(cx, |this, _| {
            this.panels.show(crate::dock::Panel::Agent);
            this.agent_chat.models_loaded = true;
        });
        paint(&app, cx);
        let before = app.read_with(cx, |this, _| this.project().clone());
        assert!(cx.debug_bounds("agent-midi-new").is_none());
        assert!(cx.debug_bounds("agent-chords").is_none());
        click("music-reference-menu", cx);
        paint(&app, cx);
        app.read_with(cx, |this, _| {
            let menu = this.menu.as_ref().unwrap();
            assert_eq!(menu.entries.len(), 7);
            assert!(matches!(&menu.entries[1], crate::ui::context_menu::MenuEntry::Item(item) if !item.enabled));
        });
        cx.simulate_keystrokes("escape");
        app.read_with(cx, |this, _| assert!(this.menu.is_none()));
        click("music-reference-menu", cx);
        // Selected-clip attachment is disabled; Down skips it before reaching chords.
        cx.simulate_keystrokes("down down enter");
        paint(&app, cx);
        cx.simulate_input("| C/E G7 | vi | F |");
        cx.simulate_keystrokes("enter");
        app.read_with(cx, |this, _| {
            assert!(this.prompt.is_none());
            assert_eq!(this.project(), &before);
            let bytes = std::fs::read(&this.agent_chat.references.attachments[0]).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["chords"][0]["chord"], "C/E");
            assert_eq!(value["chords"][2]["start_ticks"], 3840);
        });
        paint(&app, cx);
        click("music-reference-menu", cx);
        paint(&app, cx);
        crate::harness::choose(
            &app,
            cx,
            &crate::ui::context_menu::MenuCommand::ComposerChords,
        );
        paint(&app, cx);
        cx.simulate_input("| nonsense |");
        cx.simulate_keystrokes("enter");
        app.read_with(cx, |this, _| {
            assert!(this.prompt.is_some());
            assert_eq!(this.agent_chat.references.attachments.len(), 1);
            assert_eq!(this.project(), &before);
        });
    }

    #[gpui::test]
    fn an_installed_skill_attaches_as_a_frozen_copy_named_after_it(cx: &mut gpui::TestAppContext) {
        use crate::ui::context_menu::MenuCommand;
        let installed = tempfile::tempdir().unwrap();
        let folder = installed.path().join("compose-kaiwai");
        std::fs::create_dir_all(&folder).unwrap();
        let skill = folder.join("SKILL.md");
        let text = "---
name: compose-kaiwai
description: Songs
---
Use four bars.";
        std::fs::write(&skill, text).unwrap();
        let (app, cx) = open(cx);
        app.update(cx, |this, _| {
            this.panels.show(crate::dock::Panel::Agent);
            this.agent_chat.models_loaded = true;
        });
        paint(&app, cx);
        click("music-reference-menu", cx);
        paint(&app, cx);
        let at = app.read_with(cx, |this, _| this.menu.as_ref().unwrap().anchor);
        crate::harness::choose(&app, cx, &MenuCommand::ComposerSkills(at));
        paint(&app, cx);
        app.update(cx, |this, cx| {
            let menu = this.menu.as_ref().unwrap();
            assert_eq!(
                menu.title.as_ref(),
                this.t(auris_i18n::Key::AgentSkillsTitle)
            );
            // The folders this machine really has are not the test's business: answer for them.
            let request = menu.async_request.unwrap();
            let skills = auris_agent::skills::skills_in(&[(
                installed.path().into(),
                auris_agent::skills::SkillSource::Codex,
            )]);
            this.finish_skill_menu(request, skills, cx);
        });
        paint(&app, cx);
        crate::harness::choose(&app, cx, &MenuCommand::ComposerSkill(skill.clone()));
        std::fs::write(
            &skill,
            "---
name: compose-kaiwai
---
Edited later.",
        )
        .unwrap();
        app.read_with(cx, |this, _| {
            let attachments = &this.agent_chat.references.attachments;
            assert_eq!(attachments.len(), 1);
            assert_eq!(
                attachments[0].file_name().unwrap().to_string_lossy(),
                "compose-kaiwai.skill.md"
            );
            assert_eq!(std::fs::read_to_string(&attachments[0]).unwrap(), text);
            assert_eq!(
                auris_agent::attachment_kind(&attachments[0]),
                Some(auris_agent::AttachmentKind::Skill)
            );
        });
    }

    #[gpui::test]
    fn sound_choice_snapshots_keep_exact_preset_identity(cx: &mut gpui::TestAppContext) {
        let (app, cx) = open(cx);
        app.update(cx, |this, _| {
            let before = this.project().clone();
            let sound = auris_session::TimbreSound {
                name: "Warm piano".into(),
                instrument_id: "sampler".into(),
                preset: Some(auris_session::prelude::PresetRef {
                    font: auris_session::prelude::SoundFontId(17),
                    bank: 3,
                    patch: 8,
                }),
            };
            let value = auris_session::music_reference::sound_reference(&this.session, &sound);
            this.attach_agent_music_reference(&value, "sound-piano")
                .unwrap();
            let bytes = std::fs::read(&this.agent_chat.references.attachments[0]).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["preset"]["bank"], 3);
            assert_eq!(value["preset"]["patch"], 8);
            assert_eq!(value["name"], "Warm piano");
            assert_eq!(this.project(), &before);
        });
    }
}
