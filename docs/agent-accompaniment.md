# Algorithmic backing for an authored melody

The live agent and saved-project MCP tools expose `accompaniment`. The agent can edit a
foreground melody with `edit_notes`, then ask Auris to write bass, chords, pads, arpeggios or
assigned drums. Requests contain roles, ranges and recipe controls; responses contain clip IDs,
note counts and recipe summaries rather than complete note arrays.

Inspect the project first. Identify the foreground by its musical role and copy its clip IDs
into `protected_clips`. Protection preserves the entire clips, including authored notes,
curves, lyrics, performance and recipes. It does not infer a new progression from the melody.
Choose harmony deliberately with `edit_harmony`; existing phrase-writing and backing-adjustment
algorithms perform the score. A seventh, suspension or diminished chord is a musical choice,
not a required proportion of the chart.

Create separate backing tracks and select their sounds using the normal track tools.
For drums, assign the track's sounds before generating. Copy track IDs and the current
content `revision` from `inspect_project` into the request. Example IDs below are placeholders.
Saved calls add a `project` path or project ID to the same arguments.

```json
{
  "expected_revision": "revision-from-inspect_project",
  "protected_clips": [10],
  "operation": {
    "kind": "generate",
    "parts": [
      {
        "track": 2,
        "name": "Verse bass",
        "part": "bass",
        "start_bar": 1,
        "bars": 4,
        "seed": 23,
        "settings": { "density": 0.65 }
      },
      {
        "track": 3,
        "name": "Verse pad",
        "part": "pad",
        "start_bar": 1,
        "bars": 4,
        "seed": 42
      }
    ]
  }
}
```

Keep the returned clip IDs. To change the progression and rewrite only the chosen backing,
read a current revision and pass those IDs with a harmony replacement:

```json
{
  "expected_revision": "current-revision",
  "protected_clips": [10],
  "operation": {
    "kind": "regenerate",
    "clips": [11],
    "harmony": {
      "start_bar": 1,
      "bars": 4,
      "key": "C major",
      "chords": "| Imaj7 | IVsus4 | viidim7 | V7 |"
    }
  }
}
```

Here clip 11 follows the new harmony, while clip 12 and the foreground retain their stored
notes. Harmony is project metadata shared by all tracks: unselected backing may therefore
still reflect the previous progression. Choose the intended targets explicitly. A separate
`edit_harmony` also preserves notes; omit `harmony` from a later `regenerate` request to use
the current project harmony. Changing recipe controls remains an explicit `edit_recipe` operation.

Regeneration preserves clip IDs, position, length, loop settings, recipe controls and seeds,
authored curves and performance stacks. A hand-edited generated score is refused rather than
overwritten, and a frozen clip has no recipe to regenerate. The protected foreground may be
authored manually or generated; neither is rewritten by this tool.

Each operation prepares an owned snapshot on a worker and publishes only if its originating
document is unchanged and the request has not been cancelled. Generation failure changes
neither the live document nor its Undo/Redo history. Harmony and all selected backing changes
form one Undo step; saved execution checkpoints the change. Retrying a completed generation
with its old revision is rejected, and occupied target ranges are refused.

Limits: 1–16 protected clips and 1–16 parts or distinct regeneration targets; at most 64 bars
and 256 quarter notes per target, 1024 quarter notes and 65,536 generated notes per operation.
Requests must produce nonempty clips. Split targets at internal meter/section changes, and
split clips that span beyond a requested harmony replacement. The snapshot is limited to
64 MiB of serialized project data. Recipe settings use the existing `edit_recipe` vocabulary;
use each generation part's `seed`, leaving `settings.seed` omitted.

Inspect the returned summaries and compare the same rendered bars with `inspect_audio` or a
preview. Preserved notes and correct regeneration are structural guarantees, not an assessment
of musical quality. Saved recipes retain their usual contract: a seed is reproducible within
a build, and freezing retains a take across changes to the composer.
