//! Worker/owner handoffs for live voice selection and synthesis.
use super::*;
use crate::{
    SingPlan, SingerLandingJob, SingerLandingResult, SingerVoiceLoadJob, SingerVoiceLoadResult,
};
#[cfg(test)]
mod tests;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

/// Owned vocal work safe to move off the document/UI thread.
pub struct VocalJob {
    work: Work,
}

enum Work {
    Discover {
        settings: crate::Settings,
        offset: usize,
    },
    Load {
        job: SingerVoiceLoadJob,
        track: TrackId,
        speaker: Option<String>,
        select: bool,
        seed: Option<u64>,
    },
    Render {
        voice: SingerVoiceLoadResult,
        plan: SingPlan,
        landing: SingerLandingJob,
    },
}

/// A worker result that only the owning session may continue or publish.
pub struct VocalResult {
    outcome: Outcome,
}

enum Outcome {
    Discovery(String),
    Loaded {
        voice: SingerVoiceLoadResult,
        track: TrackId,
        speaker: Option<String>,
        select: bool,
        seed: Option<u64>,
    },
    Rendered {
        track: TrackId,
        result: SingerLandingResult,
    },
}

/// Either the next worker stage or a completed, model-facing response.
pub enum VocalStep {
    /// Execute off the UI thread, then return the result to the session.
    Work(Box<VocalJob>),
    /// The operation completed; the response describes the actual state.
    Done(String),
}

impl VocalJob {
    /// Runs discovery, loading, inference and staging with cooperative cancellation.
    /// No live document is accessed or changed here.
    pub fn run(self, cancelled: &AtomicBool) -> Result<VocalResult, String> {
        let check = || {
            if cancelled.load(Ordering::Relaxed) {
                Err("Vocal operation cancelled".to_string())
            } else {
                Ok(())
            }
        };
        check()?;
        let outcome = match self.work {
            Work::Discover { settings, offset } => {
                let voices = crate::library::voices_with_settings(&settings);
                if offset > voices.len() {
                    return Err("offset exceeds voice count".into());
                }
                let page: Vec<_> = voices.iter().skip(offset).take(16).map(|(name,path)|
                    serde_json::json!({"name":name,"voice":path,"validated":false})
                ).collect();
                let next = offset + page.len();
                Outcome::Discovery(serde_json::json!({"voices":page,"total":voices.len(),"next_offset":(next < voices.len()).then_some(next),"usage":"Choose a returned voice path with set_singer_voice; inspect_singer returns speakers after validation."}).to_string())
            }
            Work::Load {
                job,
                track,
                speaker,
                select,
                seed,
            } => {
                let voice = job
                    .run(cancelled)
                    .map_err(|e| e.to_string())?
                    .ok_or("Vocal operation cancelled")?;
                Outcome::Loaded {
                    voice,
                    track,
                    speaker,
                    select,
                    seed,
                }
            }
            Work::Render {
                voice,
                plan,
                landing,
            } => {
                let model = voice.model();
                let render = model
                    .lock()
                    .map_err(|_| "Singer worker stopped unexpectedly")?
                    .sing_render_with(
                        &plan.frames,
                        &plan.score,
                        plan.speaker,
                        plan.seed,
                        |_, _| !cancelled.load(Ordering::Relaxed),
                    )
                    .map_err(|e| e.to_string())?;
                check()?;
                let result = landing
                    .run(render, cancelled)
                    .map_err(|e| e.to_string())?
                    .ok_or("Vocal operation cancelled")?;
                Outcome::Rendered {
                    track: plan.track,
                    result,
                }
            }
        };
        check()?;
        Ok(VocalResult { outcome })
    }
}

impl Session {
    /// Captures a slow live vocal operation after permission approval, without loading a model.
    /// Pass the window's current settings so unsaved voice-path preferences are respected.
    pub fn begin_agent_vocal(
        &mut self,
        command: Command,
        settings: crate::Settings,
    ) -> Result<VocalJob, String> {
        let work = match command {
            Command::ListVoices { offset } => Work::Discover { settings, offset },
            Command::SetSingerVoice {
                track,
                voice,
                speaker,
            } => {
                let track = TrackId(track);
                if !self
                    .project()
                    .track(track)
                    .is_some_and(|track| track.kind.is_singer())
                {
                    return Err("set_singer_voice requires a singer track ID".into());
                }
                if voice.len() > 4096 || !Path::new(&voice).is_absolute() {
                    return Err(
                        "voice must be an absolute local entry path, within 4096 bytes".into(),
                    );
                }
                Work::Load {
                    job: self.begin_local_singer_voice_load(Path::new(&voice)),
                    track,
                    speaker,
                    select: true,
                    seed: None,
                }
            }
            Command::Sing { track, seed } => {
                let track = TrackId(track);
                self.preflight_singer_plan(track)
                    .map_err(|e| e.to_string())?;
                Work::Load {
                    job: self
                        .begin_singer_voice_load_for_automatic_access(track)
                        .map_err(|e| e.to_string())?,
                    track,
                    speaker: None,
                    select: false,
                    seed,
                }
            }
            _ => return Err("Expected list_voices, set_singer_voice or sing".into()),
        };
        Ok(VocalJob { work })
    }

    /// Continues a worker result on its document thread. Stale model loads and takes are refused.
    /// Discovery and cache handoffs are read-only; a selection or take is one ordinary Undo edit.
    pub fn continue_agent_vocal(&mut self, result: VocalResult) -> Result<VocalStep, String> {
        let done = match result.outcome {
            Outcome::Discovery(text) => text,
            Outcome::Loaded {
                voice,
                track,
                speaker,
                select,
                seed,
            } => {
                let voice = self
                    .continue_singer_voice_load(voice)
                    .ok_or("The document or voice changed during loading; retry the operation")?;
                if select {
                    self.set_singer_voice_selection_from_load(track, &voice, speaker.as_deref())
                        .map_err(|e| e.to_string())?;
                    self.agent_inspect_singer(track, 0)?
                } else {
                    let plan = self
                        .sing_plan_from_voice_load(track, seed, &voice)
                        .map_err(|e| e.to_string())?;
                    let landing = self
                        .begin_singer_landing(&plan)
                        .map_err(|e| e.to_string())?;
                    return Ok(VocalStep::Work(Box::new(VocalJob {
                        work: Work::Render {
                            voice,
                            plan,
                            landing,
                        },
                    })));
                }
            }
            Outcome::Rendered { track, result } => {
                let seconds = self
                    .continue_singer_landing(result)
                    .map_err(|e| e.to_string())?
                    .ok_or("The document changed during singing; the take was not applied")?;
                let seed = self
                    .project()
                    .track(track)
                    .and_then(|track| track.kind.as_singer())
                    .and_then(|singer| singer.take.as_ref())
                    .map(|take| take.seed);
                serde_json::json!({"track":track.0,"seconds":seconds,"seed":seed,"take_state":"current","unsaved":true}).to_string()
            }
        };
        Ok(VocalStep::Done(done))
    }

    pub(super) fn agent_inspect_singer(
        &self,
        track: TrackId,
        offset: usize,
    ) -> Result<String, String> {
        let info = self.singer_voice_info(track).map_err(|e| e.to_string())?;
        let total = info.as_ref().map_or(0, |info| info.speakers.len());
        if offset > total {
            return Err("offset exceeds speaker count".into());
        }
        let next = (offset + 128).min(total);
        let voice = info.map(|info| serde_json::json!({
            "name":info.name,"voice":info.path,"backend":format!("{:?}",info.backend),
            "speaker":info.speaker,"speakers":info.speakers.iter().skip(offset).take(128).collect::<Vec<_>>(),
            "speakers_total":info.speakers.len()
        }));
        let seed = self
            .project()
            .track(track)
            .and_then(|track| track.kind.as_singer())
            .and_then(|singer| singer.take.as_ref())
            .map(|take| take.seed);
        Ok(serde_json::json!({"track":track.0,"voice":voice,"take_seed":seed,"next_offset":(next < total).then_some(next),
            "take_state":match self.singer_take_state(track).map_err(|e|e.to_string())? {
                crate::SingerTakeState::Absent => "missing",
                crate::SingerTakeState::Current => "current",
                crate::SingerTakeState::Behind => "stale",
            },"unsaved":self.is_dirty()})
        .to_string())
    }
}
