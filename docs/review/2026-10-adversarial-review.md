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
| AR-08 | Low | Security | Codex's native reads are not confined to the project folder |
| AR-10 | Medium | VST3 | Every class in a multi-class bundle gets the first class's kind |
| AR-11 | Low-Medium | Security | `OPENAI_API_KEY` is sent to whatever OpenAI-compatible URL is configured |
| AR-12 | Low | VST3 | An editor change during playback overrides automation for one block |
| AR-14 | Low | DSP | `loudness_quantile` panics on a NaN block; one infinite block reads as silence |
| AR-15 | Low | Agent | The Codex path check is case-sensitive about `.exe` |
| AR-17 | Low | Latent | Two transactions can be left open by an early `?` |

## Findings

### AR-08 — Codex's native reads are not confined to the project folder

The Claude Code half of this finding is fixed: its `Read`/`Glob`/`Grep` are approved without asking
only inside the folder the CLI works in, and ask everywhere else. The Codex backend runs with
`sandbox: "read-only", approvalPolicy: "untrusted"` (`crates/auris-agent/src/backends/codex.rs`),
where Codex runs commands it considers safe, reads included, without asking anybody, and its
read-only sandbox does not restrict *where* it reads. Nothing in Auris sees those reads.

**Direction.** Only Codex can confine them. Until it can, say so where the Codex backend is
chosen.

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
