# Exact composition candidates

For the live agent panel, set `AURIS_COMPOSITION_CANDIDATES=1` before starting
Auris to advertise the experimental candidate and phrase tools. Their additional
schemas need context headroom; use a 64k context for the full local-model
experiment. The ordinary 32k catalog remains available by default. Saved-project
MCP exposes these commands through its manual/composition tool groups.

`candidates` is shared by the live agent and saved-project MCP. It constructs
an immutable baseline plus one to three alternatives. Note proposals use the
same validation as `edit_notes`; accompaniment proposals use explicit protected
and foreground clip IDs. Instrument and drum note edits are supported.

1. Inspect the document and read the affected notes. State the musical purpose
   and what must remain fixed.
2. Create a round with that content revision and independent proposals. Each
   alternative starts from the baseline, never from another alternative.
3. Read changed clip summaries and paginated, exact before/after notes. New
   accompaniment clip IDs belong to their candidate snapshot.
4. Preview the same short range for all alternatives. The WAV files use one
   protective gain and report encoded measurements. They are not independently
   normalized or scored. Musical preference still requires listening.
5. Apply the selected candidate with the original revision, or cancel. Applying
   copies the evaluated snapshot; it does not rerun a writer. A live application
   makes one Undo edit. An identical retry does not create another edit.

A round expires after fifteen minutes and belongs to its original document.
Live document changes, cancellation and stale worker results prevent adoption.
Saved tools can reopen the same canonical project path while its contents remain
unchanged. At most three rounds per revision and three preview ranges per round
bound exploration. The process retains at most eight rounds and 256 MiB of score
snapshots, with 64 MiB per round. Preview paths live only while the round remains
cached; cancellation removes them. The tools never automatically choose a winner.
