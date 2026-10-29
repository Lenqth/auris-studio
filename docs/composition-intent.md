# Composing from an idea

[日本語](composition-intent.ja.md)

Song specifications accept mood, tonality and pace. The style supplies instruments and form.

| Sound | Scale | Automatic mood example |
| --- | --- | --- |
| Bright | Major | Bright |
| Subdued | Natural minor | Dark |
| Dark with warmth | Dorian | Epic |
| Floating | Lydian | Dreamy |
| Bright and mellow | Mixolydian | Funky |
| Tense and dark | Phrygian | Tense |

These descriptions are starting points: rhythm, register and instruments also shape the mood.
The request can be written in a `.asong` file through the CLI or composition tools:

```toml
style = "pop-band"
mood = "dark"
tonality = "minor"
pace = "slow"
```

`tonality` accepts `auto`, `major`, or `minor`. `pace` accepts `auto`, `slow`, `moderate`, or
`fast` (80, 120, or 160 BPM). Omitting them when naming a mood uses `auto`.
`sound` accepts `auto`, `major`, `minor`, `dorian`, `lydian`, `mixolydian`, or `phrygian`.
For example, `mood = "dark"` with `sound = "dorian"` keeps dark performance settings and uses
Dorian harmony. See `examples/dorian-song.asong` for a complete request.
An explicit `sound` wins over `tonality`, which wins over mood-based scale selection.
An explicit `key` or `scale` wins over both; an explicit `tempo` wins over `pace`.
`chords` supplies an explicit progression if desired.

## Auditioning chords

For a headless WAV using the same renderer:

```sh
cargo run -p auris-session --example chord_preview -- examples/dorian-song.asong verse target/chords.wav
```

## How harmony is generated

The generator in `auris-compose/src/progression.rs` has three steps:

1. Split a section into balanced phrases of at most four bars.
2. Choose a complete phrase of scale degrees and build chords by stacking the selected scale's
   thirds. Major and minor have tonal phrases; the four modal sounds share six phrase shapes
   with just two degree choices per mode. Each modal phrase anchors the tonic and visits a
   characteristic chord: IV in Dorian, II in Lydian and Phrygian, VII in Mixolydian.
   Repeat the opening phrase every other phrase, varying the phrases between repetitions.
3. In busier tonal phrases, optionally prepare V with ii in major or iv in minor within the bar.

The frame planner adds chord colour and decides whether a section needs an arrival.
Modal harmony keeps extensions within the selected scale and approaches the tonic with its
characteristic chord, retaining the mode instead of inserting a tonal dominant. An explicit
dominant lead-in for a key change is still honoured. Tonal minor can use a major dominant.
All instruments read that completed plan. Short sections retain each phrase's opening and
destination, with modal phrases retaining the characteristic chord even at two bars. Odd lengths
remain fully covered. The seed chooses repeatable takes within a build.
