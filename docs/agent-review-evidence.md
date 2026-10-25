# Automatic approval evidence

[日本語](agent-review-evidence.ja.md)

AutoReview still asks an independent, tool-free model to assess the exact pending
operation. Deny rules, one-time permissions, document revision checks and cancellation
remain in force. Uncertainty, malformed output, unsupported evidence and timeouts
leave the operation pending for a human decision.

The review request includes the human requests, operation arguments, project folder,
recent completed native calls and current project-local scripts. Prior output is
evidence of a completed call, not proof of what a script will do or permission to
perform another action. Assistant messages cannot grant consent.

Long commands can appear repeatedly in Codex's request and item representations.
For payloads over 64 KiB, a lossless `shared_text_v1` encoding stores strings of at
least 256 bytes once. `{text:N}` refers to `texts[N]`; `{object:fields}` wraps an
original object, arrays decode recursively, and primitive values remain unchanged.
The wrapper distinguishes real objects from encoding references. No command, diff
or human request is truncated. The final request is bounded at 256 KiB; an oversized
operation cannot obtain approval.

The desktop supplies up to eight recent successful native calls with nonempty
completed results. The review worker retains at most 48 KiB of these observations
and reports omitted items. It independently snapshots regular UTF-8 `.py`, `.ps1`,
`.sh`, `.js`, `.mjs`, `.cjs`, `.bat` and `.cmd` files in the project and two directory
levels below it. Hidden paths, external resolved paths, links and common generated
directories are excluded. Inventory is bounded to 256 entries and 16 script records;
contents are bounded to 64 KiB per file and 128 KiB total. Missing, invalid or oversized
contents are explicitly unavailable, never implicitly approved.

Collection runs off the UI thread. Snapshots carry the actual byte SHA-256 and
complete contents. After an approving review, the worker rechecks the resolved paths
and byte hashes before returning the recommendation. Changed scripts invalidate it.
This is a point-in-time check, not an OS-level guarantee against a separate process
changing files after authorization. Unknown dependencies can still require consent.

Regression coverage exercises repeated long scripts, lossless decoding, literal
reference-shaped objects, current source versus previous output, changed files,
oversized evidence, filtered call history and unchanged permission behavior:

```sh
cargo test -p auris-agent approval_review
cargo test -p auris-gpui --bins ui::agent_chat::controls::tests
```
