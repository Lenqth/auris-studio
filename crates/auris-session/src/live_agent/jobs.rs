//! One worker/owner protocol for agent operations that must leave the UI thread.

use super::{
    Command, VocalJob, VocalResult, VocalStep,
    file_jobs::{FileJob, FileResult},
};
use crate::{Session, Settings};
use std::sync::atomic::AtomicBool;

/// Owned work that can run without accessing the live document.
pub enum AgentJob {
    /// Voice discovery, model loading or synthesis.
    Vocal(Box<VocalJob>),
    /// File encoding, audio export or encoded-file measurement.
    File(Box<FileJob>),
    /// Snapshot serialization, history inspection or prepared restoration.
    History(Box<crate::CheckpointJob>),
}

/// A worker result returned to its originating owner for validation and publication.
pub enum AgentResult {
    /// A voice worker's result.
    Vocal(VocalResult),
    /// A staged file operation's result.
    File(FileResult),
    /// Prepared history operation awaiting its owner.
    History(Box<crate::CheckpointResult>),
}

/// Another worker stage or the completed tool response.
pub enum AgentStep {
    /// Run the next detached stage.
    Work(AgentJob),
    /// The operation completed with verified readback.
    Done(String),
}

impl AgentJob {
    /// Perform detached work with cooperative cancellation.
    pub fn run(self, cancelled: &AtomicBool) -> Result<AgentResult, String> {
        match self {
            Self::History(job) => job
                .run(cancelled)
                .map(|result| AgentResult::History(Box::new(result))),
            Self::Vocal(job) => job.run(cancelled).map(AgentResult::Vocal),
            Self::File(job) => job.run(cancelled).map(AgentResult::File),
        }
    }
}

impl Session {
    /// Begin an expensive agent operation without doing its work on the caller's UI thread.
    pub fn begin_agent_job(
        &mut self,
        command: Command,
        settings: Settings,
    ) -> Result<AgentJob, String> {
        match command {
            Command::Checkpoints {
                operation,
                expected_revision,
            } => self
                .begin_agent_checkpoints(operation, expected_revision)
                .map(|job| AgentJob::History(Box::new(job))),
            command @ (Command::ListVoices { .. }
            | Command::SetSingerVoice { .. }
            | Command::Sing { .. }) => self
                .begin_agent_vocal(command, settings)
                .map(|job| AgentJob::Vocal(Box::new(job))),
            command => self
                .begin_agent_file_job(command)
                .map(|job| AgentJob::File(Box::new(job))),
        }
    }

    /// Validate and continue a worker result on the owning document thread.
    pub fn continue_agent_job(
        &mut self,
        result: AgentResult,
        cancelled: &AtomicBool,
    ) -> Result<AgentStep, String> {
        self.with_agent_history(|session| session.publish_agent_job(result, cancelled))
    }

    fn publish_agent_job(
        &mut self,
        result: AgentResult,
        cancelled: &AtomicBool,
    ) -> Result<AgentStep, String> {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("Agent operation cancelled before publication".into());
        }
        match result {
            AgentResult::History(result) => self
                .continue_agent_checkpoints(*result, cancelled)
                .map(AgentStep::Done),
            AgentResult::Vocal(result) => match self.continue_agent_vocal(result)? {
                VocalStep::Work(job) => Ok(AgentStep::Work(AgentJob::Vocal(job))),
                VocalStep::Done(text) => Ok(AgentStep::Done(text)),
            },
            AgentResult::File(result) => self
                .continue_agent_file_job(result, cancelled)
                .map(AgentStep::Done),
        }
    }
}
