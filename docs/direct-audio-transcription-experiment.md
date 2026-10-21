# Direct audio-LLM transcription experiment

The 2026-10-21 local experiment did not support replacing Basic Pitch with direct
note predictions from the tested Gemma 4/Ollama connections. One configuration
invented notes on silence; the other returned empty drafts. These are results for
two specific local configurations, not a conclusion about every audio model or
an agent that can select and use separate analysis tools.

The [measurement record](../tools/eval/references/direct-audio-transcription-2026-10-21.json)
keeps the fixed prompt, model artifact digests, runtime version, original model
responses, token usage, elapsed times, input/reference hashes and every case's metrics.
No production transcription behavior changes as part of this experiment.

## Input and isolation

The inputs are the same 12 held-out composer recordings from the
[piano transcription evaluation](piano-transcription-evaluation.md): city-pop,
jazz-trio and orchestral, seeds 901/902, each rendered as piano and a native mixture.
Their frozen scores contain 924 reference events after coincident unison merging.
These are controlled synthetic recordings, not independent acoustic performances.

Each model receives a fresh single user message containing the original WAV bytes
and a fixed request for MIDI pitches and start/end seconds. Neither scores, filenames,
preset names, BPM, chord estimates nor Basic Pitch candidates are transmitted.
Inference never opens the annotations; a separate scoring command opens them afterward.
No transcription tool is available to the model. This isolates the recognition component;
it does not exercise Auris tool selection, live editing, or an iterative arranging agent.

Both locally installed models, `gemma4:e2b` and `gemma4:e4b`, advertise audio support.
The server is Ollama 0.40.5. Requests use `/v1/chat/completions`, `input_audio`,
temperature 0, `reasoning_effort: none`, JSON-object output and an 8,192-token budget.
The [pinned server adapter](https://github.com/ollama/ollama/blob/v0.40.5/openai/openai.go)
decodes `input_audio` and also supports the output/reasoning fields used here.
Audio remains on localhost; no new weights or paid API inference are used.

## Same-recording comparison

Matching uses exact MIDI pitch and a 50 ms onset tolerance, with maximum one-to-one
matching, as in the existing scorer. F1 combines precision and recall; it is not a
subjective musical-quality score. Baseline estimate and recording hashes were checked
against the previously published Basic Pitch measurement.

| Recognizer | Matched | Extra candidates (FP) | Missing (FN) | Onset F1 | Observed seconds/file |
| --- | ---: | ---: | ---: | ---: | ---: |
| Current Basic Pitch | 637 | 327 | 287 | 67.48% | 0.27 |
| Gemma 4 e2b, direct | 2 | 183 | 922 | 0.36% | 5.48 |
| Gemma 4 e4b, direct | 0 | 0 | 924 | 0.00% | 2.56 |

The direct rows score complete parseable raw candidates, including those extending
beyond the source duration. They are not silently clipped or repaired. Four e2b
drafts fail the duration check; rejecting each entire invalid draft leaves one match,
67 FP and 923 FN (F1 0.20%). All 12 e4b results are structurally valid empty arrays
claiming that no audio was attached. An empty array is not successful recognition.

Basic Pitch times are historical Rust measurements of the same recordings; direct
times include local HTTP, audio processing and generation. These are observed costs,
not interleaved speed benchmarks. The local runs incurred no paid inference calls;
they do not estimate cloud pricing or electricity cost.

## Controls and failure investigation

Three six-second controls contain silence, six isolated sine notes, and two three-note
chords. Their exact annotations and audio hashes are retained in the record. No example
pitch is supplied in the scoring prompt.

* In the primary audio-first JSON condition, e2b returned the same ten repeated C4 notes
  for all three controls, extending to ten seconds. All three fail the duration check;
  raw candidates score zero matches. E4b returned empty arrays for all three.
* Changing only part order to instruction-first, matching Auris's native attachment order,
  did not recover the controls. E2b invented six notes on silence; e4b also invented notes
  on silence. Both configurations score zero matches on the pitched controls. One e2b
  reply exhausts its token budget and is rejected as incomplete.
* Removing only the JSON-output constraint from the primary audio-first condition produces
  refusals/no-audio claims or fenced JSON, not accurate notes. Raw responses are retained;
  prose and fenced JSON are not interpreted as completed note drafts.
* A separate spoken positive control asks for the words in an offline synthesized sentence.
  E2b reproduces the sentence correctly with both part orders. E4b produces an empty answer
  or repeated syllables. Its input connection is therefore not validated for reliable
  semantic audio recognition, despite its advertised capability and successful HTTP calls.
* With the audio omitted entirely, e2b still invents a note sequence under the JSON request.
  E4b reports no audio. Structured output alone is insufficient evidence of recognition.
* A native `/api/chat` diagnostic combines instructions and WAV bytes in one message rather
  than using the compatibility adapter. With a 2,048-token limit, e2b still invents repeated
  C4 notes for the six-note control. E4b produces a truncated scale-like sequence, and its
  spoken request fails with HTTP 500. These are additional routing checks, not another
  matched-cohort accuracy comparison.

An initial A4 transport probe supplied an example note matching its stimulus; it is excluded
from all musical scoring. Diagnostic response times overlapped a workspace compilation and
are not included in the primary timing comparison. There was no score-driven prompt tuning
on the held-out recordings. Alternative input order/output format trials used controls only.

The experiment cannot isolate model training, quantization, prompting and local audio-runtime
limitations as the cause of every failure. In particular, e4b's failed speech control prevents
attributing its empty music results solely to musical ability. HTTP success, nonzero audio
tokens, and fluent claims about listening do not establish correct interpretation.

## Reproduce

Use a `uv` environment containing numpy/scipy, pytest and ruff, as for the existing scorer.
Start the configured local server with an already installed audio-capable model. Generate
the frozen corpus using the existing example; keep its manifest/audio unchanged.
Every output directory and report below must be new.

```sh
uv run --with numpy --with scipy tools/eval/direct_audio_transcription.py controls target/direct-controls
uv run --with numpy --with scipy tools/eval/direct_audio_transcription.py infer target/direct-controls/manifest.json target/direct-e2b-controls --model gemma4:e2b --split control
uv run --with numpy --with scipy tools/eval/direct_audio_transcription.py infer target/piano-transcription/corpus-v1/manifest.json target/direct-e2b-heldout --model gemma4:e2b
uv run --with numpy --with scipy tools/eval/direct_audio_transcription.py score target/piano-transcription/corpus-v1/manifest.json target/direct-e2b-heldout target/direct-e2b-score.json --baseline target/piano-transcription/candidate-confirmed
```

Repeat with `--model gemma4:e4b` and fresh output paths. For control-only diagnostics,
use `--text-first` or `--unconstrained` independently. Tests verify annotation isolation,
invalid pitch/time rejection, complete JSON requirements and separate raw/accepted metrics:

```sh
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_direct_audio_transcription.py tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/direct_audio_transcription.py tools/eval/test_direct_audio_transcription.py
```

A useful next experiment is a controller choosing analysis tools and turning their evidence
into a playable arrangement, with recognition, arranging and iterative correction evaluated
separately. The present results provide no evidence for removing the recognizer and trusting
unverified note JSON from these local connections.
