# Optional local evaluation and bounded revision

Use this path when the user asks for automatic composition refinement and a local
SongEval environment is available. Keep the musical brief and an audible baseline
before looking at model scores. Do not make a model installation a prerequisite
for an ordinary composition request.

In an Auris source checkout, `tools/eval/composition_refine.py` connects the real
agent-panel trial runner to synthesis, rendering and `tools/eval/songeval.py`.
Read `docs/composition-refinement.md` for its recipe and runtime arguments. This is
development tooling, not a built-in `listen` backend; discover actual available
tools instead of inventing a SongEval tool call in an installed app.

Compose and evaluate the baseline, then decide whether one local repair is justified.
Choose the repair after inspecting the actual notes, lyrics, brief and all five
SongEval axes. Explain the concrete musical issue, expected benefit and tradeoff.
SongEval has whole-recording predictions, not timestamped advice; a low score alone
does not identify a repair. Keep the baseline unchanged when no repair is justified.
Limit a repair to one foreground clip within the user's allowed fields and note budget.

Use all five axes as uncertain evidence, with identical model/preprocessing and
audio identity checks. Do not sum them, chase a threshold, lengthen silence or
change the mix to obtain a higher score. Naturalness predicts vocal phrasing and
breathing, not general melody quality; Memorability is not a delayed-recall test.
Instrumental and synthetic-vocal results require their own listening calibration.

The harness creates at most one candidate. No-change decisions skip repeat synthesis
and evaluation. Usually render the full mix only; request foreground/backing auditions
when useful. Machine/model paths live in a shared configuration, while each song needs
only a brief, focus track and optional revision scope. Preserve unrelated notes, lyrics,
backing, sounds and gain. Keep original and candidate audio, editable scores and reasons.
A failed revision leaves the baseline available and records the failure without retrying.
The baseline remains the default even when predicted scores rise. Compare rhythm, motif,
chorus contrast, naturalness and breathing separately when listening afterwards.
