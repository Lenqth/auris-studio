# Physical instruments in the bass register

The 2026-10-21 correction addresses metallic bass attacks in Physical Strings and excessive
inharmonicity in Physical Piano. MIDI 24/36 denote C1/C2 here, with A4 = MIDI 69 = 440 Hz.
The original renderer is from `2c4bcfd1`.

## Bow scattering

The force previously jumped when the relative bow/string velocity crossed the static limit.
For example, an input change of 0.0000002 changed the outgoing force from 0.0207999 to
0.0058681 in the regression probe. Those repeated discontinuities excited the short
bow-to-bridge segment during a bass attack. Bypassing body radiation did not remove the
unstable startup.

The junction now uses a continuous, bounded scattering coefficient, following the
[bow-table formulation](https://www.dsprelated.com/freebooks/pasp/Bowed_Strings.html)
also used by [STK](https://github.com/thestk/stk/blob/master/include/BowTable.h).
Pressure controls the sticking range. Hardness blends cubic and quartic decay of the
sliding coefficient. The coefficient stays between zero and one, preserving passivity
relative to the moving bow. The implementation needs only arithmetic, with no new
allocation, I/O or locks in the callback.

The effective bridge phase compensation is 0.82 times the one-pole DC delay. This is an
empirical tuning calibration for the broadband bowed return. Full DC compensation with
the continuous junction sharpened the upper register by about ten cents. Apply the same
compensation at excitation, bends, glides and live loss changes. Factory sustain probes at
MIDI 69/85/88 stay within four cents at 44.1/48/96 kHz; the existing fundamental-level and
upper-midrange guards also remain in force.

## Piano dispersion

The piano's factory inharmonicity coefficient was fitted on C3–G4 with broad mel bands.
Using that coefficient unchanged on wound bass strings overstretched their overtones.
This is a separate cause from the bowed-string force discontinuity.

First-second partials 1–12 from the University of Iowa's mf Steinway recordings give
inharmonicity estimates of approximately 0.000256 at C1 and 0.000144 at C2. Each recording
retains its own fitted fundamental, so recording tuning is not mistaken for dispersion.
The coefficient fit uses the stiff-string relation normalized at the first partial.

The bass curve scales the public stiffness control by 0.35 at C1 and 0.20 at C2 and joins
the existing C3 calibration at unity. Cubic smoothstep joins avoid sudden register edges;
the multiplier remains bounded below C1. Zero stiffness still produces an unstretched
string. Notes from C3 upward retain the same samples as the baseline.

The three raw reference hashes, onset positions, partial frequencies and fit residuals
are stored in the [measurement report](../tools/eval/references/physical-low-register.json).
These few reference notes constrain bass dispersion, not whole-instrument acoustic fidelity.

## Measurements and listening

The paired study covers seven patches (grand, felt and bright piano; violin, viola, cello
and contrabass), MIDI 24/30/36/48/69/85/88, velocities 0.35/0.8 and 24/48/96 kHz:
294 conditions. Each note lasts three seconds, with release at 2.5 seconds. Rendering is
sequential to bound memory use. Unrelated physical models and piano notes from C3 upward
have sample-identical controls.

Factory values at 48 kHz and velocity 0.8:

| Measurement | Before | After |
| --- | ---: | ---: |
| Piano C1 eighth-partial stretch | 40.1 cents | 14.0 cents |
| Piano C2 eighth-partial stretch | 40.1 cents | 8.2 cents |
| Strings C1 attack one-period correlation | 0.792 | 0.975 |
| Strings C2 attack one-period correlation | 0.942 | 0.995 |
| Strings C1 attack upper/fundamental power | 13.5 dB | 4.9 dB |
| Strings C2 attack upper/fundamental power | 5.2 dB | 7.7 dB |

Attack measurements use 300–800 ms. Correlation removes DC and searches lags within 0.5%
of the played period. The upper band spans harmonics 5–40, capped at 5 kHz; it is a
diagnostic rather than an objective to minimize. The C2 upper-band increase is retained
in the table: the correction improves periodicity, not every brightness measure.
All 72 bowed conditions at MIDI 24/30/36 have attack correlation above 0.975 after correction.

The new friction curve also changes level: default C1/C2 sustain RMS increases by about
5.5 dB. The public level parameter keeps its meaning. Listening files include a common-gain
pair and an RMS-matched **before / gap / after** file for each default C1/C2 instrument.
Listening boundaries have 5 ms fades; no limiter or EQ is applied. Raw, unchanged-gain evaluation WAVs are in
the output's `before/` and `after/` directories.

The symbolic ruler is unchanged. Learned-ear evaluation was attempted before and after;
the initial run was interrupted by the workstation crash, and bounded offline retries
could not find all pinned model checkpoints. No TuneJury or MuQ score is claimed. The
waveform diagnostics establish the specific regressions, not a listening preference.

Validation completed: 207 library tests and eight example-worker tests with
`cargo test -p auris-synth --all-targets -j 1`; 16 Python measurement tests;
workspace Clippy with all targets; Rust formatting and Ruff. The full workspace test
run was stopped during the initial UI-dependency build and is not reported as passing.

```sh
cargo build -p auris-synth --example physical_fit_render --release -j 1
# Archive the original worker before changing DSP.
uv run tools/eval/physical_low_register.py before.exe after.exe target/low-register/check
# Add --reference-dir DIR when DIR contains Piano.mf.C1/C2/C3.aiff from Iowa.
uv run --with pytest --with numpy --with scipy --with soundfile pytest tools/eval/test_physical_low_register.py
uv run --with ruff ruff check tools/eval/physical_low_register.py tools/eval/test_physical_low_register.py
```
