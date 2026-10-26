//! Conversational song director and bounded, independently cancellable composers.

use auris_session::workshop::{
    Candidate, Direction, Document, Message, Preferences, Round, Selection, Store,
};
use auris_session::{AgentPreferences, Session, SessionOptions, Settings, SongCandidate, SongIdea};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Commands accepted by the background workshop owner.
pub enum Command {
    /// Submit one natural-language message with model settings captured at submission.
    Send(String, Box<AgentPreferences>),
    /// Choose an exact completed candidate without closing the conversation.
    Select(u64),
    /// Retry only this candidate in a new artifact directory.
    Retry(u64),
    /// Load auditions for an older comparison without retaining every historical WAV.
    View(usize),
    /// Stop one candidate, or all work when absent.
    Stop(Option<u64>),
    /// Persist an unsent draft before closing.
    Draft(String),
    /// Enable or disable cross-project preference evidence.
    Memory(bool),
    /// Forget all recorded preferences.
    Forget,
    /// Remove one incorrect or no-longer-relevant preference observation.
    ForgetChoice(String, u64),
}

/// Full UI snapshot, containing immutable shared audition buffers.
pub struct Snapshot {
    /// Durable conversation and comparison history.
    pub document: Document,
    /// Completed candidates currently available for preview or adoption.
    pub candidates: BTreeMap<u64, Arc<SongCandidate>>,
    /// Real current stages; absence means the job is not running.
    pub progress: BTreeMap<u64, String>,
    /// The director is responding.
    pub planning: bool,
    /// Preference evidence the user may inspect or clear.
    pub preferences: Preferences,
    /// Storage/provider/runtime failure, never silently swallowed.
    pub error: Option<String>,
}

/// One background owner; dropping it cancels all child workers.
pub struct Workshop {
    sender: mpsc::Sender<Command>,
    receiver: mpsc::Receiver<Snapshot>,
    stopped: Arc<AtomicBool>,
}

impl Workshop {
    /// Start or restore a project-scoped conversation off the UI thread.
    pub fn start(folder: PathBuf, unsaved: bool, settings: Settings) -> Result<Self, String> {
        let (sender, commands) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let cancelled = stopped.clone();
        std::thread::Builder::new()
            .name("auris-workshop".into())
            .spawn(move || {
                let result = (|| {
                    let conversations = if unsaved {
                        auris_session::conversations::ConversationStore::for_unsaved_document(
                            &folder,
                        )?
                    } else {
                        auris_session::conversations::ConversationStore::for_project(&folder)
                    };
                    let store = Store::new(conversations.root().into());
                    let _lease = store.lease()?;
                    // Do not overwrite an unreadable history with a fresh default.
                    store.load()?;
                    let mut owner = Owner::new(store, settings, events.clone());
                    owner.run(commands, cancelled);
                    Ok::<(), String>(())
                })();
                if let Err(error) = result {
                    let _ = events.send(Snapshot {
                        document: Document::default(),
                        candidates: BTreeMap::new(),
                        progress: BTreeMap::new(),
                        planning: false,
                        preferences: Preferences::default(),
                        error: Some(error),
                    });
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender,
            receiver,
            stopped,
        })
    }
    /// Queue a command without blocking the window.
    pub fn send(&self, command: Command) -> bool {
        self.sender.send(command).is_ok()
    }
    /// Drain available snapshots; callers should keep the newest.
    pub fn poll(&self) -> Option<Snapshot> {
        self.receiver.try_iter().last()
    }
}

impl Drop for Workshop {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

/// Create an editor conversation with the workshop's readable history and bounded model memory.
/// Run on a storage worker; the returned store and ID are ready for normal conversation navigation.
pub fn handoff(
    folder: PathBuf,
    unsaved: bool,
    document: Document,
) -> Result<(auris_session::conversations::ConversationStore, String), String> {
    use auris_session::conversations::{
        ConversationContent, ConversationMessage, ConversationStore,
    };
    let store = if unsaved {
        ConversationStore::for_unsaved_document(&folder)?
    } else {
        ConversationStore::for_project(&folder)
    };
    let mut conversation = store.create()?;
    let mut memory = crate::memory::Memory::default();
    let mut pending = String::new();
    for (index, message) in document.messages.iter().enumerate() {
        conversation.transcript.messages.push(ConversationMessage {
            id: index as u64,
            model: String::new(),
            content: if message.user {
                if !pending.is_empty() {
                    pending.push('\n');
                }
                pending.push_str(&message.text);
                ConversationContent::User {
                    text: message.text.clone(),
                    attachments: vec![],
                }
            } else {
                memory.push(&pending, &message.text);
                pending.clear();
                ConversationContent::Assistant {
                    text: message.text.clone(),
                }
            },
        });
    }
    if !pending.is_empty() {
        memory.push(&pending, "");
    }
    if let Some(selected) = document
        .rounds
        .iter()
        .flat_map(|r| &r.candidates)
        .find(|c| Some(c.id) == document.selected)
    {
        memory.summary = format!(
            "The user selected this exact song and opened it in the editor. Continue against the current score; do not recreate it. Selection: {}. {}. Assignment: {}",
            selected.direction.title,
            selected.direction.description,
            selected.direction.instructions
        );
    }
    conversation.transcript.draft = document.draft;
    store.save(&conversation.info.id, &conversation.transcript)?;
    memory.save(&store.memory_path(&conversation.info.id)?)?;
    Ok((store, conversation.info.id))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    reply: String,
    directions: Vec<Direction>,
}

fn decision(text: &str) -> Result<Decision, String> {
    if text.len() > 100_000 {
        return Err("Director response is too large".into());
    }
    let text = text.trim();
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(text)
        .trim();
    let value: Decision =
        serde_json::from_str(text).map_err(|e| format!("Invalid director response: {e}"))?;
    if value.reply.trim().is_empty()
        || value.reply.chars().count() > 4000
        || !matches!(value.directions.len(), 0 | 3)
        || value.directions.iter().any(|d| {
            d.title.trim().is_empty()
                || d.title.chars().count() > 80
                || d.description.chars().count() > 400
                || d.instructions.trim().is_empty()
                || d.instructions.chars().count() > 16000
        })
    {
        return Err("Invalid or oversized composition assignments".into());
    }
    Ok(value)
}

enum JobEvent {
    Progress(u64, String),
    Done(u64, Result<Arc<SongCandidate>, String>),
}
struct Owner {
    store: Store,
    settings: Settings,
    snapshot: Snapshot,
    events: mpsc::Sender<Snapshot>,
    planner: Option<crate::song_ideas::SongPlanner>,
    planned_parent: Option<u64>,
    prefs: AgentPreferences,
    jobs: BTreeMap<u64, Arc<AtomicBool>>,
    job_sender: mpsc::Sender<JobEvent>,
    job_receiver: mpsc::Receiver<JobEvent>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl Owner {
    fn new(store: Store, settings: Settings, events: mpsc::Sender<Snapshot>) -> Self {
        let (job_sender, job_receiver) = mpsc::channel();
        let mut error = None;
        let mut document = store.load().unwrap_or_else(|e| {
            error = Some(e);
            Document::default()
        });
        for candidate in document.rounds.iter_mut().flat_map(|r| &mut r.candidates) {
            if !candidate.ready && candidate.error.is_none() {
                candidate.error = Some("Production was interrupted; retry this candidate".into());
            }
        }
        let preferences = auris_session::workshop::load_preferences().unwrap_or_else(|e| {
            error = Some(e);
            Preferences::default()
        });
        let mut candidates = BTreeMap::new();
        for candidate in document
            .rounds
            .iter()
            .flat_map(|r| &r.candidates)
            .filter(|c| {
                c.ready
                    && (Some(c.id) == document.selected
                        || document
                            .rounds
                            .last()
                            .is_some_and(|r| r.candidates.iter().any(|last| last.id == c.id)))
            })
        {
            match SongCandidate::load_workshop(
                &store.candidate_path(candidate.id),
                idea(&candidate.direction),
            ) {
                Ok(song) => {
                    candidates.insert(candidate.id, Arc::new(song));
                }
                Err(e) => error = Some(e),
            }
        }
        Self {
            prefs: settings.agent.clone(),
            store,
            settings,
            events,
            planner: None,
            planned_parent: None,
            jobs: BTreeMap::new(),
            job_sender,
            job_receiver,
            handles: Vec::new(),
            snapshot: Snapshot {
                document,
                candidates,
                progress: BTreeMap::new(),
                planning: false,
                preferences,
                error,
            },
        }
    }
    fn publish(&mut self, save: bool) {
        if save && let Err(e) = self.store.save(&self.snapshot.document) {
            self.snapshot.error = Some(e);
        }
        let _ = self.events.send(Snapshot {
            document: self.snapshot.document.clone(),
            candidates: self.snapshot.candidates.clone(),
            progress: self.snapshot.progress.clone(),
            planning: self.planner.is_some(),
            preferences: self.snapshot.preferences.clone(),
            error: self.snapshot.error.clone(),
        });
    }
    fn run(&mut self, commands: mpsc::Receiver<Command>, stopped: Arc<AtomicBool>) {
        self.publish(false);
        while !stopped.load(Ordering::Acquire) {
            match commands.recv_timeout(Duration::from_millis(25)) {
                Ok(command) => {
                    self.command(command);
                    self.publish(true);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(answer) = self.planner.as_mut().and_then(|p| p.poll_text()) {
                self.planner = None;
                match answer.and_then(|text| decision(&text)) {
                    Err(e) => self.snapshot.error = Some(e),
                    Ok(answer) => {
                        self.snapshot.document.messages.push(Message {
                            user: false,
                            text: answer.reply,
                        });
                        if !answer.directions.is_empty() {
                            let mut candidates = Vec::new();
                            for direction in answer.directions {
                                self.snapshot.document.next_id += 1;
                                candidates.push(Candidate {
                                    id: self.snapshot.document.next_id,
                                    parent: self.planned_parent,
                                    direction,
                                    ready: false,
                                    error: None,
                                });
                            }
                            self.snapshot.document.rounds.push(Round {
                                after_message: self.snapshot.document.messages.len(),
                                candidates: candidates.clone(),
                            });
                            for candidate in candidates {
                                self.launch(candidate);
                            }
                        }
                    }
                }
                self.publish(true);
            }
            while let Ok(event) = self.job_receiver.try_recv() {
                match event {
                    JobEvent::Progress(id, stage) if self.jobs.contains_key(&id) => {
                        self.snapshot.progress.insert(id, stage);
                        self.publish(false);
                    }
                    JobEvent::Done(id, result) => {
                        let cancelled = self
                            .jobs
                            .remove(&id)
                            .is_none_or(|c| c.load(Ordering::Acquire));
                        self.snapshot.progress.remove(&id);
                        {
                            let result = if cancelled {
                                Err("Composition stopped".into())
                            } else {
                                result
                            };
                            let entry = self
                                .snapshot
                                .document
                                .rounds
                                .iter_mut()
                                .flat_map(|r| &mut r.candidates)
                                .find(|c| c.id == id);
                            if let Some(entry) = entry {
                                match result {
                                    Ok(candidate) => {
                                        entry.ready = true;
                                        self.snapshot.candidates.insert(id, candidate);
                                    }
                                    Err(e) => entry.error = Some(e),
                                }
                            }
                        }
                        self.publish(true);
                    }
                    _ => {}
                }
            }
        }
        for cancel in self.jobs.values() {
            cancel.store(true, Ordering::Release);
        }
        self.planner = None;
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
        // Draft commands sent immediately before Drop must still be durable.
        for command in commands.try_iter() {
            if let Command::Draft(draft) = command {
                self.snapshot.document.draft = draft;
            }
        }
        self.publish(true);
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Send(text, prefs) if self.planner.is_none() && self.jobs.is_empty() => {
                if text.trim().is_empty() || text.chars().count() > 16000 {
                    self.snapshot.error = Some("Enter 1..16000 characters".into());
                    return;
                }
                self.prefs = *prefs;
                self.planned_parent = self.snapshot.document.selected;
                self.snapshot.error = None;
                match auris_session::workshop::load_preferences() {
                    Ok(preferences) => self.snapshot.preferences = preferences,
                    Err(e) => {
                        self.snapshot.error = Some(e);
                        return;
                    }
                }
                self.snapshot.document.draft.clear();
                self.snapshot
                    .document
                    .messages
                    .push(Message { user: true, text });
                let selected = self
                    .snapshot
                    .document
                    .rounds
                    .iter()
                    .flat_map(|r| &r.candidates)
                    .find(|c| Some(c.id) == self.snapshot.document.selected);
                let voices = auris_session::library::voices_with_settings(&self.settings);
                let prompt = json!({"conversation": self.snapshot.document.messages, "selected": selected, "selected_score":self.snapshot.document.selected.and_then(|id| self.snapshot.candidates.get(&id)).map(|c| c.workshop_outline()),
                    "preference_evidence": self.snapshot.preferences.context(), "installed_voices": voices}).to_string();
                match crate::song_ideas::SongPlanner::conversation(self.prefs.clone(), prompt) {
                    Ok(planner) => self.planner = Some(planner),
                    Err(e) => self.snapshot.error = Some(e),
                }
            }
            Command::Select(id) if self.snapshot.candidates.contains_key(&id) => {
                self.snapshot.document.selected = Some(id);
                if let Some(round) = self
                    .snapshot
                    .document
                    .rounds
                    .iter()
                    .find(|r| r.candidates.len() > 1 && r.candidates.iter().any(|c| c.id == id))
                {
                    let chosen = round
                        .candidates
                        .iter()
                        .position(|c| c.id == id)
                        .unwrap_or(0);
                    let selection = Selection {
                        conversation: self.store.identity(),
                        candidate: id,
                        round: round.candidates.first().map_or(id, |c| c.id),
                        context: self
                            .snapshot
                            .document
                            .messages
                            .iter()
                            .take(round.after_message)
                            .filter(|m| m.user)
                            .map(|m| m.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                            .chars()
                            .take(2000)
                            .collect(),
                        alternatives: round
                            .candidates
                            .iter()
                            .map(|c| c.direction.clone())
                            .collect(),
                        chosen,
                    };
                    match auris_session::workshop::update_preferences(|p| p.record(selection)) {
                        Ok(p) => self.snapshot.preferences = p,
                        Err(e) => self.snapshot.error = Some(e),
                    }
                }
            }
            Command::Retry(id)
                if self.jobs.len() < 3
                    && !self.jobs.contains_key(&id)
                    && self.planner.is_none() =>
            {
                let candidate = self
                    .snapshot
                    .document
                    .rounds
                    .iter()
                    .flat_map(|r| &r.candidates)
                    .find(|c| c.id == id)
                    .cloned();
                if let Some(mut candidate) = candidate {
                    self.snapshot.document.next_id += 1;
                    candidate.id = self.snapshot.document.next_id;
                    candidate.ready = false;
                    candidate.error = None;
                    self.snapshot.document.rounds.push(Round {
                        after_message: self.snapshot.document.messages.len(),
                        candidates: vec![candidate.clone()],
                    });
                    self.launch(candidate);
                }
            }
            Command::Stop(id) => {
                if id.is_none() {
                    self.planner = None;
                }
                for (&job, cancel) in &self.jobs {
                    if id.is_none_or(|id| job == id) {
                        cancel.store(true, Ordering::Release);
                        self.snapshot.progress.insert(job, "stopping".into());
                    }
                }
            }
            Command::Draft(draft) => self.snapshot.document.draft = draft,
            Command::View(index) => {
                if let Some(round) = self.snapshot.document.rounds.get(index) {
                    let ids: Vec<_> = round
                        .candidates
                        .iter()
                        .filter(|c| c.ready)
                        .map(|c| c.id)
                        .collect();
                    self.snapshot.candidates.retain(|id, _| {
                        ids.contains(id) || Some(*id) == self.snapshot.document.selected
                    });
                    for candidate in &round.candidates {
                        if candidate.ready && !self.snapshot.candidates.contains_key(&candidate.id)
                        {
                            match SongCandidate::load_workshop(
                                &self.store.candidate_path(candidate.id),
                                idea(&candidate.direction),
                            ) {
                                Ok(song) => {
                                    self.snapshot
                                        .candidates
                                        .insert(candidate.id, Arc::new(song));
                                }
                                Err(e) => self.snapshot.error = Some(e),
                            }
                        }
                    }
                }
            }
            Command::Memory(enabled) => {
                match auris_session::workshop::update_preferences(|p| p.enabled = enabled) {
                    Ok(p) => self.snapshot.preferences = p,
                    Err(e) => self.snapshot.error = Some(e),
                }
            }
            Command::Forget => {
                match auris_session::workshop::update_preferences(|p| p.selections.clear()) {
                    Ok(p) => self.snapshot.preferences = p,
                    Err(e) => self.snapshot.error = Some(e),
                }
            }
            Command::ForgetChoice(conversation, candidate) => {
                match auris_session::workshop::update_preferences(|p| {
                    p.selections
                        .retain(|s| s.conversation != conversation || s.candidate != candidate)
                }) {
                    Ok(p) => self.snapshot.preferences = p,
                    Err(e) => self.snapshot.error = Some(e),
                }
            }
            _ => {}
        }
    }
    fn launch(&mut self, mut candidate: Candidate) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.insert(candidate.id, cancel.clone());
        self.snapshot
            .progress
            .insert(candidate.id, "planning".into());
        let settings = self.settings.clone();
        let prefs = self.prefs.clone();
        let sender = self.job_sender.clone();
        let folder = self.store.candidate_path(candidate.id);
        let parent = candidate
            .parent
            .map(|id| self.store.candidate_path(id).join("song/song.auris"));
        let id = candidate.id;
        candidate.direction.instructions = json!({"assignment":candidate.direction.instructions,"conversation":self.snapshot.document.messages}).to_string();
        match std::thread::Builder::new()
            .name(format!("auris-composer-{id}"))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    compose(
                        &candidate, prefs, settings, folder, parent, &cancel, &sender,
                    )
                }))
                .unwrap_or_else(|_| Err("Candidate worker failed".into()))
                .map(Arc::new);
                let _ = sender.send(JobEvent::Done(id, result));
            }) {
            Ok(handle) => self.handles.push(handle),
            Err(e) => {
                self.jobs.remove(&id);
                self.snapshot.progress.remove(&id);
                self.snapshot.error = Some(e.to_string());
            }
        }
    }
}

fn idea(direction: &Direction) -> SongIdea {
    SongIdea {
        title: direction.title.clone(),
        description: direction.description.clone(),
        preset: "pop-band".into(),
        tempo: 120.0,
        key: "C major".into(),
        density: 0.5,
        lead: "piano".into(),
    }
}

// Render and voice inference share a conservative process-wide gate; model turns remain parallel.
static AUDIO_WORK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn compose(
    candidate: &Candidate,
    mut prefs: AgentPreferences,
    settings: Settings,
    folder: PathBuf,
    parent: Option<PathBuf>,
    cancel: &AtomicBool,
    sender: &mpsc::Sender<JobEvent>,
) -> Result<SongCandidate, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Cancelled".into());
    }
    let mut session = Session::new(
        SessionOptions::headless()
            .with_balance(false)
            .with_plugin_paths(settings.plugin_paths.clone()),
    )
    .map_err(|e| e.to_string())?;
    if let Some(dictionary) = &settings.japanese_dictionary {
        session
            .set_japanese_dictionary(Some(dictionary))
            .map_err(|e| e.to_string())?;
    }
    if let Some(parent) = parent {
        let missing = session.open(&parent).map_err(|e| e.to_string())?;
        if !missing.is_empty() {
            return Err("Parent candidate has missing assets".into());
        }
    }
    prefs.policy = auris_session::agent_policy::Policy {
        mode: auris_session::agent_policy::Mode::Edit,
        allow: vec![],
        deny: vec!["search_internet".into(), "edit_project.listen".into()],
    };
    prefs.system_prompt = auris_toolbox::workshop::COMPOSER.into();
    let worker = crate::Worker::spawn(prefs.clone(), None, true, None)?;
    let before = session.project().clone();
    if candidate.direction.protected_tracks.len() > 128
        || candidate
            .direction
            .protected_tracks
            .iter()
            .any(|id| !before.tracks.iter().any(|t| t.id.0 == *id))
    {
        return Err("Protected track is not present in the parent candidate".into());
    }
    session.begin_agent_request();
    worker.send(
        &json!({"say": candidate.direction.instructions,"policy":prefs.policy}).to_string(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(900);
    let mut edits = 0;
    let mut last_failure = String::new();
    let completion = loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        if Instant::now() > deadline {
            return Err("Candidate production timed out".into());
        }
        let event = match worker.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(_) => return Err("Candidate worker stopped".into()),
        };
        match event["event"].as_str().unwrap_or_default() {
            "permission" => {
                let allowed = auris_session::agent_policy::Operation::parse(
                    event["tool"].as_str().unwrap_or_default(),
                    &event["args"],
                )
                .is_ok_and(|op| {
                    op.name.starts_with("edit_project.")
                        || matches!(
                            op.name.as_str(),
                            "search_instruments" | "instrument_diagnostics" | "production_record"
                        )
                        || auris_toolbox::runtime::live_references()
                            .iter()
                            .any(|t| t.name == op.name)
                });
                worker.send(&json!({"event":"permission_result","id":event["id"],"ok":allowed,"reason":"Isolated candidate music operations only"}).to_string())?;
            }
            "edit" => {
                edits += 1;
                if edits > 160 {
                    return Err("Candidate exceeded its tool budget".into());
                }
                let action = event["command"]["action"].as_str().unwrap_or_default();
                let stage = match action {
                    "edit_harmony" | "suggest_harmony" => "harmony",
                    "edit_notes" | "generate_phrase" => "melody",
                    "sing" | "set_singer_voice" => "sing",
                    "compose" | "add_track" => "compose",
                    _ => "arrange",
                };
                let _ = sender.send(JobEvent::Progress(candidate.id, stage.into()));
                let result = execute(&mut session, event["command"].clone(), &settings, cancel);
                let reply = match result {
                    Ok(text) if action == "inspect_audio" => {
                        json!({"event":"edit_result","ok":true,"inspection":serde_json::from_str::<serde_json::Value>(&text).map_err(|e|e.to_string())?})
                    }
                    result => {
                        json!({"event":"edit_result","ok":result.is_ok(),"text":result.unwrap_or_else(|e|e)})
                    }
                };
                worker.send(&reply.to_string())?;
            }
            "result" if event["ok"] == false => {
                last_failure = event["text"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(1000)
                    .collect();
            }
            "answer" => {
                break event["text"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(2000)
                    .collect::<String>();
            }
            "error" => {
                return Err(event["message"]
                    .as_str()
                    .unwrap_or("Candidate failed")
                    .into());
            }
            "ended" => return Err("Candidate ended without completing".into()),
            _ => {}
        }
    };
    session.end_agent_request();
    for id in &candidate.direction.protected_tracks {
        let original = before
            .tracks
            .iter()
            .find(|t| t.id.0 == *id)
            .and_then(|t| t.kind.note_clips());
        let current = session
            .project()
            .tracks
            .iter()
            .find(|t| t.id.0 == *id)
            .and_then(|t| t.kind.note_clips());
        if original != current {
            return Err(format!(
                "The composer changed protected notes on track {id}"
            ));
        }
    }
    if session.project() == &before {
        return Err(format!(
            "The composer did not make the requested music changes. {completion} {last_failure}"
        ));
    }
    let _ = sender.send(JobEvent::Progress(candidate.id, "render_wait".into()));
    let _gate = audio_gate(cancel)?;
    let _ = sender.send(JobEvent::Progress(candidate.id, "render".into()));
    session.finish_workshop_candidate(&folder, idea(&candidate.direction), cancel)
}

fn audio_gate(cancel: &AtomicBool) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        match AUDIO_WORK.try_lock() {
            Ok(gate) => return Ok(gate),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err("Audio worker failed".into()),
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

fn execute(
    session: &mut Session,
    command: serde_json::Value,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let action = command["action"].as_str().unwrap_or_default();
    // File exports/imports, playback, nested jobs and network tools are not child capabilities.
    if matches!(
        action,
        "render"
            | "listen"
            | "import_transcription"
            | "preview"
            | "export_midi"
            | "import_audio"
            | "import_midi"
            | "audio_assets"
            | "checkpoints"
            | "verify_render"
            | "candidates"
            | "accompaniment"
            | "arrangement"
    ) {
        return Err("This operation is not available inside candidate production".into());
    }
    let vocal = matches!(action, "list_voices" | "set_singer_voice" | "sing");
    let command = serde_json::from_value(command).map_err(|e| e.to_string())?;
    if let auris_session::live_agent::Command::InspectAudio {
        start_bar,
        bars,
        track,
    } = command
    {
        let _gate = audio_gate(cancel)?;
        let report = session
            .audio_inspection_job(start_bar, bars, track)?
            .run(cancel)?;
        return serde_json::to_string(&report).map_err(|e| e.to_string());
    }
    if vocal {
        let _gate = audio_gate(cancel)?;
        let mut job = session.begin_agent_vocal(command, settings.clone())?;
        loop {
            let result = job.run(cancel)?;
            match session.continue_agent_vocal(result)? {
                auris_session::live_agent::VocalStep::Work(next) => job = *next,
                auris_session::live_agent::VocalStep::Done(text) => return Ok(text),
            }
        }
    }
    session.agent_command_with_plugin_paths(command, &settings.plugin_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn director_does_not_leak_its_read_only_role_into_composer_assignments() {
        let answer =
            json!({"role":"assistant","content":"{\"reply\":\"Which mood?\",\"directions\":[]}"})
                .to_string();
        let (url, seen) =
            crate::tests::mock_server(vec![crate::tests::completion(&answer, "stop")]);
        let prefs = AgentPreferences {
            provider: "openai".into(),
            model: "mock".into(),
            url,
            ..Default::default()
        };
        let mut director =
            crate::song_ideas::SongPlanner::conversation(prefs, "Make three short songs".into())
                .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(result) = director.poll_text() {
                assert!(decision(&result.unwrap()).unwrap().directions.is_empty());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].contains("Current Auris permission mode"));
    }

    #[test]
    fn editor_handoff_keeps_readable_turns_and_provider_memory() {
        let dir = tempfile::tempdir().unwrap();
        let document = Document {
            messages: vec![
                Message {
                    user: true,
                    text: "Keep the melody".into(),
                },
                Message {
                    user: false,
                    text: "I will change only the accompaniment".into(),
                },
            ],
            draft: "Make the chorus brighter".into(),
            ..Default::default()
        };
        let (store, id) = handoff(dir.path().into(), false, document).unwrap();
        let conversation = store.load(&id).unwrap();
        assert_eq!(conversation.transcript.messages.len(), 2);
        assert_eq!(conversation.transcript.draft, "Make the chorus brighter");
        let memory = crate::memory::Memory::load(&store.memory_path(&id).unwrap()).unwrap();
        assert_eq!(memory.turns[0].user, "Keep the melody");
        assert_eq!(
            memory.turns[0].answer,
            "I will change only the accompaniment"
        );
    }
    fn assignment() -> Candidate {
        Candidate {
            id: 1,
            parent: None,
            ready: false,
            error: None,
            direction: Direction {
                title: "Test".into(),
                description: "A short instrumental".into(),
                instructions: "Create a short chiptune instrumental".into(),
                protected_tracks: vec![],
            },
        }
    }
    #[test]
    fn composer_creates_persistent_audition_through_real_session_tools() {
        let call = |id: &str, name: &str, arguments: serde_json::Value| {
            json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments.to_string()}}]}).to_string()
        };
        let record = call(
            "record-1",
            "production_record",
            json!({"operation":{"kind":"select_workflow","workflow":"composition","writing_order":"rhythm_first"}}),
        );
        let setup = call("setup-1", "add_track", json!({"name":"Kit","kind":"drum"}));
        let phrase = call(
            "phrase-1",
            "generate_phrase",
            json!({"track":1,"part":"drums","start_bar":1,"bars":2,"seed":42}),
        );
        let inspect = call(
            "inspect-1",
            "inspect_audio",
            json!({"start_bar":1,"bars":2}),
        );
        let answer = json!({"role":"assistant","content":"Finished"}).to_string();
        let (url, seen) = crate::tests::mock_server(vec![
            crate::tests::completion(&record, "tool_calls"),
            crate::tests::completion(&setup, "tool_calls"),
            crate::tests::completion(&phrase, "tool_calls"),
            crate::tests::completion(&inspect, "tool_calls"),
            crate::tests::completion(&answer, "stop"),
        ]);
        let prefs = AgentPreferences {
            provider: "openai".into(),
            model: "mock".into(),
            url,
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let (sender, receiver) = mpsc::channel();
        let result = compose(
            &assignment(),
            prefs,
            Settings::default(),
            dir.path().join("1"),
            None,
            &AtomicBool::new(false),
            &sender,
        )
        .unwrap();
        assert!(result.audio.peak() > 1e-8);
        assert!(dir.path().join("1/song/song.auris").is_file());
        assert_eq!(seen.lock().unwrap().len(), 5);
        assert!(seen.lock().unwrap()[1].contains("worker_lifetime"));
        assert!(
            receiver
                .try_iter()
                .any(|e| matches!(e, JobEvent::Progress(_, stage) if stage == "compose"))
        );
    }
    #[test]
    fn cancellation_prevents_model_work_and_output() {
        let dir = tempfile::tempdir().unwrap();
        let (sender, _) = mpsc::channel();
        let result = compose(
            &assignment(),
            AgentPreferences::default(),
            Settings::default(),
            dir.path().join("1"),
            None,
            &AtomicBool::new(true),
            &sender,
        );
        assert!(result.is_err());
        assert!(!dir.path().join("1").exists());
    }
    #[test]
    fn child_cannot_export_to_an_arbitrary_path() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let error = execute(
            &mut session,
            json!({"action":"render","path":"untrusted.wav"}),
            &Settings::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("not available"));
    }
    #[test]
    fn clarification_does_not_dispatch_composers() {
        assert!(
            decision(r#"{"reply":"歌ものですか？","directions":[]}"#)
                .unwrap()
                .directions
                .is_empty()
        );
    }
    #[test]
    fn incomplete_comparison_is_rejected() {
        assert!(decision(r#"{"reply":"作ります","directions":[{"title":"A","description":"Piano","instructions":"Compose"}]}"#).is_err());
    }
}
