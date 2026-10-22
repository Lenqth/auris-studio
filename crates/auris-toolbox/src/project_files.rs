//! New documents and media interchange through the session's file commands.

use super::*;

fn absolute_path<'a>(text: &'a str, field: &str) -> Result<&'a Path, String> {
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(format!("{field} must be an absolute path"));
    }
    Ok(path)
}

fn extension(path: &Path, allowed: &[&str], field: &str) -> Result<(), String> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            allowed
                .iter()
                .any(|allowed| value.eq_ignore_ascii_case(allowed))
        })
    {
        return Ok(());
    }
    Err(format!(
        "{field} requires a .{} extension",
        allowed.join(" or .")
    ))
}

fn project_destination(output: &str) -> Result<&Path, String> {
    let path = absolute_path(output, "output")?;
    extension(path, &["auris"], "output")?;
    if path.exists() {
        return Err("output already exists; choose a new .auris path".into());
    }
    Ok(path)
}

fn save_new(session: &mut Session, output: &Path) -> Result<PathBuf, String> {
    cancellation::begin_commit()?;
    match session.save_as(output) {
        Ok(saved) => Ok(saved.document),
        Err(SessionError::WouldReplace(path)) => Err(format!(
            "a project already exists at {}; choose a new output path",
            path.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// Creates a document for manual note entry or audio import.
pub mod create_project {
    use super::*;

    /// The tool's wire name.
    pub const NAME: &str = "create_project";
    /// The model-facing description.
    pub const DESCRIPTION: &str = "Creates an empty project with one default instrument track and no clips. Optional tempo and meter set its clock. Output must be a new absolute .auris path; choosing Song.auris writes Song/Song.auris. Returns the actual project path to use in later calls. Use add_clip and edit_notes for manual notes, import_audio for recordings, or add_track for more parts.";

    /// The new document's destination and optional clock.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// New absolute .auris path. Existing projects are never replaced.
        pub output: String,
        /// Initial tempo in native BPM bounds, 10-999. Defaults to the application's new-project tempo.
        pub tempo: Option<f64>,
        /// Meter such as 4/4, 3/4 or 6/8. Defaults to 4/4.
        pub meter: Option<String>,
    }

    /// Creates and saves an empty document without invoking the composer.
    pub fn run(args: &Args) -> Result<String, String> {
        let output = project_destination(&args.output)?;
        if let Some(tempo) = args.tempo
            && !(TempoMap::MIN_BPM..=TempoMap::MAX_BPM).contains(&tempo)
        {
            return Err(format!(
                "tempo must be within {}..{} BPM",
                TempoMap::MIN_BPM,
                TempoMap::MAX_BPM
            ));
        }
        let meter = args
            .meter
            .as_deref()
            .map(str::parse::<TimeSignature>)
            .transpose()
            .map_err(|e| e.to_string())?;
        let mut session = headless()?;
        session.new_project();
        if let Some(tempo) = args.tempo {
            session.set_tempo_at(Ticks::ZERO, tempo);
        }
        if let Some(meter) = meter {
            session.set_signature_at(Ticks::ZERO, meter);
        }
        let written = save_new(&mut session, output)?;
        let tracks: Vec<_> = session
            .project()
            .tracks
            .iter()
            .map(|track| serde_json::json!({"track":track.id.0,"name":track.name}))
            .collect();
        Ok(
            serde_json::json!({"project_id":project_handles::register(&written)?,"project":written,"tempo":session.project().bpm(),
            "meter":session.signature_at(Ticks::ZERO).to_string(),"tracks":tracks,"clips":0})
            .to_string(),
        )
    }
}
