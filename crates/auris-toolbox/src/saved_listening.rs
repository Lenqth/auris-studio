//! Saved calls retain opaque audio baselines while using the live session's listening command.

use crate::*;
use auris_session::audio_review::assessment::ComparisonAspect;
use auris_session::{audio_audition::ListeningContext, audio_review::AudioReviewOptions};
use std::sync::{Mutex, OnceLock};

// Two projects, each with the session's four bounded 30-second excerpts, stay below 128 MiB.
static CONTEXTS: OnceLock<Mutex<Vec<(PathBuf, ListeningContext)>>> = OnceLock::new();

pub(crate) fn run(
    session: &mut Session,
    start_bar: u32,
    bars: u32,
    focus: Option<&str>,
    compare_to: Option<&str>,
    aspect: Option<ComparisonAspect>,
) -> Result<String, String> {
    run_with_options(
        session,
        start_bar,
        bars,
        focus,
        compare_to,
        aspect,
        AudioReviewOptions::from_env()?,
    )
}

fn run_with_options(
    session: &mut Session,
    start_bar: u32,
    bars: u32,
    focus: Option<&str>,
    compare_to: Option<&str>,
    aspect: Option<ComparisonAspect>,
    options: AudioReviewOptions,
) -> Result<String, String> {
    let key = session
        .path()
        .ok_or("Missing saved project path")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let cache = CONTEXTS.get_or_init(Default::default);
    let mut context = {
        let mut contexts = cache.lock().map_err(|_| "Listening context lock failed")?;
        contexts
            .iter()
            .position(|(path, _)| path == &key)
            .map(|index| contexts.remove(index).1)
            .unwrap_or_default()
    };
    session.swap_listening_context(&mut context);
    let result = (|| {
        let job = session
            .begin_listening_with_options(start_bar, bars, focus, compare_to, aspect, options)?;
        let result = job.run(crate::cancellation::current().flag())?;
        session.continue_listening(result)
    })();
    session.swap_listening_context(&mut context);
    let mut contexts = cache.lock().map_err(|_| "Listening context lock failed")?;
    contexts.retain(|(path, _)| path != &key);
    if contexts.len() >= 2 {
        contexts.remove(0);
    }
    contexts.push((key, context));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde_json::{Value, json};
    use std::io::{BufRead, Read, Write};
    use std::time::{Duration, Instant};
    fn scripted_critic() -> (String, std::thread::JoinHandle<Vec<Value>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut captured = Vec::new();
            for reply in ["Scripted initial review.", "Scripted comparison review."] {
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "critic request did not arrive");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("critic accept failed: {error}"),
                    }
                };
                // Windows can inherit nonblocking mode from the listener.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut socket);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert!(first.starts_with("POST /v1/chat/completions "), "{first}");
                let mut length = None;
                let mut header_bytes = first.len();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    header_bytes += line.len();
                    assert!(header_bytes < 16_384, "unexpectedly large request headers");
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let length = length.expect("request has a Content-Length");
                assert!(
                    length <= 4 * 1024 * 1024,
                    "test excerpts should remain small"
                );
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                drop(reader);
                captured.push(serde_json::from_slice(&body).unwrap());
                let reply = json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":reply}}]}).to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
            }
            captured
        });
        (url, worker)
    }
    fn submitted_wavs(request: &Value) -> Vec<Vec<u8>> {
        assert!(request.get("tools").is_none());
        request["messages"][1]["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|part| part["type"] == "input_audio")
            .map(|part| {
                assert_eq!(part["input_audio"]["format"], "wav");
                base64::engine::general_purpose::STANDARD
                    .decode(part["input_audio"]["data"].as_str().unwrap())
                    .unwrap()
            })
            .collect()
    }
    #[test]
    fn fresh_saved_calls_compare_opaque_baselines_without_saving_or_replacing_audio() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Song.auris");
        let mut session = Session::new(SessionOptions::headless().with_balance(false)).unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        session
            .set_track_instrument(track, "auris.synth.fm2")
            .unwrap();
        let clip = session
            .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks(3840))
            .unwrap();
        session
            .replace_notes(clip, vec![Note::new(60, Ticks::ZERO, Ticks(997))])
            .unwrap();
        session.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let (url, worker) = scripted_critic();
        let options = AudioReviewOptions {
            url,
            model: "scripted-critic".into(),
            api_key: None,
            ollama_url: None,
        };
        let first: Value = serde_json::from_str(
            &run_with_options(&mut session, 1, 1, None, None, None, options.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(session);
        let mut session = crate::opened(path.to_str().unwrap()).unwrap();
        session.set_param(auris_session::ParamTarget::TrackGain(track), -12.0);
        session.save_with_checkpoint().unwrap();
        let edited = std::fs::read(&path).unwrap();
        let second: Value = serde_json::from_str(
            &run_with_options(
                &mut session,
                1,
                1,
                None,
                first["audition_id"].as_str(),
                Some(ComparisonAspect::Balance),
                options,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(second["before"]["audition_id"], first["audition_id"]);
        assert_eq!(std::fs::read(&path).unwrap(), edited);
        let requests = worker.join().unwrap();
        let original = submitted_wavs(&requests[0]);
        let compared = submitted_wavs(&requests[1]);
        assert_eq!(original.len(), 1);
        assert_eq!(compared.len(), 2);
        let baseline_index = usize::from(second["comparison"]["b"] == "baseline");
        assert_eq!(original[0], compared[baseline_index]);
        assert_ne!(compared[0], compared[1]);
    }
}
