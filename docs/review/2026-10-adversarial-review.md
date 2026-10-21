# Adversarial review, October 2026

A whole-workspace review of commit `2c4bcfd1` (2026-10-21), looking for what a hostile file, a
hostile model, another platform or an ordinary user doing something slightly unusual would break.
Findings are numbered `AR-nn`; cite the number from the commit that fixes one, and delete its
entry once the fix lands. Line numbers are as of `2c4bcfd1` and will drift.

The previous review (`F-nnn`, September 2026) was fixed and removed in `805da784`. Almost all of
the code below was written after it.

## How it was done

Read by hand, risk first: everything that runs a hostile input (project files, MIDI, SoundFonts,
plugin binaries, model output, conversation history), the realtime paths, the agent permission
gate, and the platform seams. Then targeted greps over the non-test code for panics, open
transactions and byte-index string slicing. Two findings were confirmed by running a probe test
(the probes are not part of this change). `cargo test --workspace` was run as well; see the end.

Nothing here was checked on a Mac. The macOS findings (AR-03, AR-04, AR-09) are reasoned from
the code, Apple's documented behaviour and `std::path` semantics.

## Summary

| ID | Severity | Area | Finding |
|----|----------|------|---------|
| AR-03 | High | Cross-platform | A Windows-saved project opens on macOS with its SoundFont tracks silent |
| AR-04 | High | macOS release | `Auris Studio.app` has no microphone usage description, so recording fails |
| AR-05 | Medium-High | Recording | Takes are not compensated for round-trip latency and land late |
| AR-06 | Medium | Denial of service | A MIDI file of meter changes takes quadratic time to import (verified) |
| AR-07 | Medium | DSP | Ping-pong delay does nothing on a centred source (verified) |
| AR-08 | Medium | Security | Native CLI file reads are approved automatically, in every mode, anywhere on disk |
| AR-09 | Medium | i18n, macOS | The app launched from Finder ignores the system language |
| AR-10 | Medium | VST3 | Every class in a multi-class bundle gets the first class's kind |
| AR-11 | Low-Medium | Security | `OPENAI_API_KEY` is sent to whatever OpenAI-compatible URL is configured |
| AR-12 | Low | VST3 | An editor change during playback overrides automation for one block |
| AR-13 | Low | Performance | Every tempo conversion walks the whole tempo map, including on the audio thread |
| AR-14 | Low | DSP | `loudness_quantile` panics on a NaN block; one infinite block reads as silence |
| AR-15 | Low | Agent | The Codex path check is case-sensitive about `.exe` |
| AR-16 | Low | Docs, platform | Stale CLAUDE.md layout; "double-click a `.auris`" is not wired up on macOS |
| AR-17 | Low | Latent | Two transactions can be left open by an early `?` |

## Findings

### AR-03 — A Windows-saved project opens on macOS with its SoundFont tracks silent

**Where.** `AssetPath::file_name` is `Path::file_name` of the stored string
(`crates/auris-core/src/asset.rs:106-108`). Two places match by it:
`install_library_reference` (`session/files.rs:1536-1546`), which re-attaches the shipped fonts,
and `locate`/`find_named` (`session/assets.rs:250-253`, `412-424`), which find moved assets.

**What happens.** On Windows, an `External` path is stored with backslashes,
`C:\Users\…\GeneralUser GS v2.0.2.sf2`. On macOS and Linux a backslash is an ordinary character,
so `file_name()` returns the whole string. The shipped font then matches nothing. It is inserted
again under a fresh ID while the tracks keep the old one, the old ID has no samples, and every
SoundFont track is silent. Every composed project names the shipped General MIDI font as
`External`, so this hits the common case. The comment at `files.rs:706-711` describes precisely
this case as the one that "always has an answer". `Inside` paths already split on both separators
(`asset.rs:129-134`); `External` file names do not. macOS to Windows works, because Windows
accepts `/`.

**Direction.** Take the last component after splitting on both `/` and `\` (the rule `resolve`
already applies), and add a test with a literal `C:\…` path. As things stand, a test on Windows
passes for the wrong reason.

### AR-04 — `Auris Studio.app` has no microphone usage description, so recording fails

**Where.** `crates/auris-gpui/macos/Info.plist`. The bundle is assembled from it in
`.github/workflows/release.yml:110-117`.

**What happens.** The plist declares no `NSMicrophoneUsageDescription`. On macOS 10.14 and later,
a process that opens an input device without that key is either terminated by TCC or given
silence, depending on the OS version and hardening. Either way, recording and input monitoring do
not work from the released `.app`. The developer never sees this: `cargo run` from Terminal runs
under Terminal's own microphone grant.

**Direction.** Add `NSMicrophoneUsageDescription` (and `CFBundleDocumentTypes`; see AR-16), and
make one manual check of a release bundle launched from Finder part of the release checklist.

### AR-05 — Takes are not compensated for round-trip latency and land late

**Where.** The first input block stamps the take with the *output* engine's playhead
(`crates/auris-engine/src/capture.rs:611-621`). That playhead is the next frame to be rendered,
stored after each block (`device.rs:934-935`). `land_take` places the clip at exactly that stamp
(`session/record.rs:953-956`).

**What happens.** What the player hears lags the playhead by the output buffer, the device's
output latency and the graph's plugin delay compensation. What the input delivers lags the
performance by the input latency and buffer. Neither is subtracted anywhere, and there is no
recording-offset setting (`settings.rs:166` says as much about the buffer figure). Every overdub
therefore lands late by the round trip: typically 10–40 ms on WASAPI shared mode, more with a
look-ahead limiter on the master. The module documentation covers start position and clock drift
carefully but not latency.

**Direction.** Subtract the output and input latency cpal reports (`StreamInstant` callback and
capture/playback timestamps) plus the graph's latency, and offer a manual offset for devices that
report nothing.

### AR-06 — A MIDI file of meter changes takes quadratic time to import (verified)

**Where.** `read_smf_with_limits` calls `SignatureMap::set_point` once per time-signature event
(`crates/auris-io/src/midi.rs:387-392`). Each call runs `align_to_bars`, which rebuilds the whole
point list (`crates/auris-core/src/time.rs:495-504`, `657-694`). The budget allows a million
events per track and four million per file (`midi.rs:59-63`), and parsing has no cancellation
point.

**Measured.** A probe importing alternating 4/4 and 3/4 bars at one tick per quarter took 92 ms
for 5 000 meter changes, 0.52 s for 10 000 and 1.48 s for 20 000 (test profile). Extrapolated,
one million changes, an 8 MB file, take about an hour. A map that size also stays slow
afterwards: `bar_of`, `spans` and `bar_start` are linear and allocate on every call
(`time.rs:548-594`), and the UI calls them while painting.

**Direction.** Collect the meter events and build the map once with `from_points` (one sort and
one alignment pass). Also bound the number of timeline points far below the note budget; no real
file has more than a few thousand.

### AR-07 — Ping-pong delay does nothing on a centred source (verified)

**Where.** `crates/auris-dsp/src/delay.rs:258-290`.

**What happens.** In ping-pong mode each channel's input is written into its own line and only the
feedback crosses. A source with identical left and right channels, which is any centred
instrument, therefore produces identical repeats on both sides, and the toggle is inaudible. A
probe with an impulse on both channels measured a left–right difference of exactly 0, with taps of
1.000/1.000, 0.700/0.700 and 0.490/0.490. The existing test (`delay.rs:415-433`) feeds the left
channel only, which is the one case where the current wiring alternates.

**Direction.** Feed the mono sum (or the left channel alone) into one line, then cross the
feedback, so the first repeat comes from one side and each later repeat alternates. Add a
centred-source test.

### AR-08 — Native CLI file reads are approved automatically, in every mode, anywhere on disk

**Where.** `Operation::parse` gives `cli_file_read` `mutating: false, confirm: false`
(`crates/auris-session/src/agent_policy.rs:178-183`), so `Policy::decide` allows it in every mode,
Read-only and Plan included (`:212-219`). The Claude backend routes `Read`/`Glob`/`Grep` to that
gate on purpose, by listing them under `ask` (`crates/auris-agent/src/backends/claude.rs:21-38`,
`108-111`). The Codex backend runs with `sandbox: "read-only", approvalPolicy: "untrusted"`
(`backends/codex.rs:452`), where reads never reach the gate at all.

**What happens.** The CLI's working directory is the project folder, but its reads are not confined
to it: `~/.ssh`, `.env` files and other projects are all readable without a prompt. Claude Code's
own default asks before reading outside its working directory, so forcing `ask` and then
answering it automatically is weaker than not routing reads through Auris at all. Combined with AR-02, a
planted instruction can collect secrets. They then need a way out, such as a track name in a
project that gets shared, or an approved command.

**Direction.** Approve reads automatically only under the project folder (canonicalised, with
symlinks resolved), and ask for anything else.

### AR-09 — The app launched from Finder ignores the system language

**Where.** `crates/auris-i18n/src/lib.rs:96-134`.

**What happens.** `from_environment` reads `LC_ALL`, `LC_MESSAGES` and `LANG`, then falls back to
`from_system_locale`, which answers only on Windows. A macOS app launched from Finder, the Dock or
Launchpad has none of those variables (launchd sets none), so a Japanese Mac gets English unless
the app is started from a terminal. This is the bug the Windows branch was written to fix.

**Direction.** Ask `NSLocale.preferredLanguages` (or `CFLocaleCopyPreferredLanguages`) on macOS.
The `objc2-foundation` crate is already in the dependency graph.

### AR-10 — Every class in a multi-class bundle gets the first class's kind

**Where.** `crates/auris-vst3/src/lib.rs:184-224`: inside the per-class map, `kind_of` and
`category_of` read `detailed.info`, the bundle's first audio class, rather than the class being
listed (`:191-192`, `:207`). The vendored `ClassInfo` does not carry the per-class subcategories
needed to do better (`vendor/vst3-host/src/discovery.rs:206-217`).

**What happens.** Shell bundles that expose many classes (WaveShell is the familiar one) list all of
their instruments as effects, or all of their effects as instruments, according to whichever class
comes first. A misfiled instrument cannot be placed on an instrument track.

**Direction.** Read `PClassInfo2::subCategories` per class in the vendored discovery, and classify
from that.

### AR-11 — `OPENAI_API_KEY` is sent to whatever OpenAI-compatible URL is configured

**Where.** `crates/auris-agent/src/lib.rs:199-214` resolves the key. It is then used for every
completion through `OpenAIConfig::new(key)…with_base_url(url)` (`:687-689`) and for listing
models (`:838`).

**What happens.** With the OpenAI provider and no `api_key_env`, the key is read from
`OPENAI_API_KEY` whatever the base URL is. A user who points Auris at a LAN or third-party
OpenAI-compatible server sends their real OpenAI key there.

**Direction.** Use the implicit `OPENAI_API_KEY` only when the URL is unset or is `api.openai.com`,
and require an explicit `api_key_env` otherwise.

### AR-12 — An editor change during playback overrides automation for one block

**Where.** `crates/auris-vst3/src/lib.rs:999-1015`, called at `:1154`, which runs after the engine
has written automation through `set_param` (`crates/auris-engine/src/graph/automation.rs`).

**What happens.** When the control instance publishes any parameter change, a turned knob or a
preset, `sync_control_parameters` copies the whole snapshot over the renderer's values. That
includes parameters automation set a moment earlier, so automated parameters jump to the editor's
stale value for one block.

**Direction.** Copy only the indices whose snapshot value changed since the renderer last saw it,
or let automated parameters win.

### AR-13 — Every tempo conversion walks the whole tempo map, including on the audio thread

**Where.** `TempoMap::ticks_to_seconds`/`seconds_to_ticks` are linear in the number of points
(`crates/auris-core/src/time.rs:878-923`). They are called per note when scheduling
(`crates/auris-engine/src/graph/schedule.rs:204-215`), twice per rendered segment on the audio
thread (`graph/mod.rs:807-850`), and per click by the metronome (`metronome.rs:115-133`).

**What happens.** A MIDI import with a dense tempo map (a rubato performance, or AR-06's budget
used for tempo events) slows every graph rebuild and eats into the realtime budget.

**Direction.** Cache cumulative seconds per point and binary-search, which makes both directions
logarithmic.

### AR-14 — `loudness_quantile` panics on a NaN block; one infinite block reads as silence

**Where.** `crates/auris-dsp/src/loudness.rs:191-199`, and `block_powers`/`gated_mean`
(`:114-176`).

**What happens.** `block_powers` does not sanitise samples, so `loudness_quantile` panics on the
`expect("a power is never NaN")` comparison. It has no callers today, which makes this a trap
rather than a crash. In `integrated_lufs`, one `+inf` block makes the relative gate infinite, so
every block is gated out and a loud mix measures as "silent".

**Direction.** Skip non-finite samples in `block_powers`.

### AR-15 — The Codex path check is case-sensitive about `.exe`

`crates/auris-agent/src/backends/codex.rs:37` compares the extension with `!=`, so `CODEX.EXE`
is refused. The Claude path accepts it with `eq_ignore_ascii_case` (`claude.rs:51-54`).

### AR-16 — Stale CLAUDE.md layout; "double-click a `.auris`" is not wired up on macOS

* `CLAUDE.md` describes `auris-agent` as a binary frontend, but it is now a library that the
  desktop links (`crates/auris-gpui/Cargo.toml`). The layout also omits `auris-analysis` and
  `auris-vst3`, which `docs/development.md` already lists.
* `crates/auris-gpui/src/main.rs:128-136` says the shell hands over a double-clicked `.auris` as an
  argument. On macOS the plist declares no `CFBundleDocumentTypes`, and Finder delivers documents
  as an open-URLs event, which nothing handles (no `on_open_urls`). The Windows zip registers no
  association either.

### AR-17 — Two transactions can be left open by an early `?`

`set_drum_lane_note` (`crates/auris-session/src/session/drum_assignments.rs:233-234`) and the
send branch of `apply_mixer_graph` (`crates/auris-session/src/mixer_graph/commands.rs:481-487`)
return through `?` between `begin_transaction` and `end_transaction`. Both errors are unreachable
today because of validation further up, but if either is ever reached, the open transaction
disables Undo (`session/mod.rs:1220`), autosave and checkpoints until the next gesture. That
gesture's undo step would also absorb everything in between. The closure-and-revert form used by
`setup_tracks` and `recognition` avoids this.

## Covered without findings

These were read with the same questions and held up: project load validation and atomic save;
asset path containment for `Inside`; bounded reads for project, MIDI, configuration and memory
files; SoundFont download verification; the realtime callback (allocation, locking, retirement off
the audio thread); sampler panic containment; WAV/FLAC/MP3 quantisation; the live MCP endpoint
(loopback, bearer, `Origin` refused); permit and revision binding for agent edits; the VOICEVOX
client (loopback only for automatic connections, bounded bodies); plugin and drum probe worker
processes; clipboard and clip commands.

The desktop crate (114k lines) was sampled, not read: keymap portability, agent chat permissions
and conversation restore.

## Test suite

`cargo test --workspace --no-fail-fast` on Windows 11 at `2c4bcfd1`: 4 253 passed, 0 failed,
19 ignored, across 50 test binaries and the doctests. The count includes the two temporary probes
behind AR-06 and AR-07. Nothing above was caught by the existing suite, and every finding marked
"verified" was reproduced by a probe written for this review. The suite passing is evidence that
these are gaps in what is tested rather than regressions.
