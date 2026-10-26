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

Declare a small edit family before evaluation: for example phrase-ending lengths,
one motif answer, or a chorus-register alternative. Keep unrelated notes, lyrics,
backing, sound and gain fixed. Ask for a musical reason grounded in the score and
brief. SongEval has whole-recording predictions, not timestamped advice, and a low
score does not establish which notes should change. Keep a no-change outcome when
the proposed intervention is not justified.

Use all five axes as uncertain evidence, with identical model/preprocessing and
audio identity checks. Do not sum them, chase a threshold, lengthen silence or
change the mix to obtain a higher score. Naturalness predicts vocal phrasing and
breathing, not general melody quality; Memorability is not a delayed-recall test.
Instrumental and synthetic-vocal results require their own listening calibration.

Bound the run in advance. The current harness makes one to three independent
revisions of the baseline, preserves failed candidates, and stops on invalid
edits or rendering/evaluation failures. It keeps original and candidate audio,
editable scores, prompts, source/model hashes, per-axis deltas and edit reasons.
It retains the baseline as the default; a higher predicted score alone cannot
promote a candidate or establish an audible improvement. Compare rhythm, motif,
chorus contrast, naturalness and breathing separately when listening afterwards.
