# Agent backends

The Agent panel discovers the signed-in Codex CLI, local Ollama, and the API
connection configured in Settings > Agent. An `OPENAI_API_KEY` also enables
discovery at OpenAI's default endpoint. Providers are queried concurrently;
an unavailable provider does not hide models returned by another.

Choose a model in the command row at the bottom of the message composer. Each option names its connection,
for example `GPT-6.1 Sol (Codex)`. Custom API connections also show their host.
Choosing an option switches its backend automatically and preserves the configured
API connection for future discovery. Refresh in the model menu repeats discovery
(`Ctrl+R` / `Command+R` while that selector has focus). Reasoning effort sits next
to the model; Codex's advertised reasoning levels determine its choices. Permission
rules open from the approval-mode menu on the left. The circular send control
becomes Stop during a turn.

Consecutive tool calls share a collapsed summary showing their count and any
running or failed calls. Open the group to inspect each call's arguments and
result; user messages and assistant replies keep separate groups apart.

## Reference attachments

Drop files onto the Agent panel to attach them to the next message. Dropping audio
there attaches a reference; dropping audio onto the arrangement imports it into
the song. Attached filenames remain visible and each file can be removed before
sending. A message accepts at most eight files and 25 MiB in total.

PNG, JPEG, GIF and WebP are sent as image parts. UTF-8 text, Markdown, lyrics,
CSV, JSON and source files are sent as delimited reference data, up to 256 KiB
per text file. Instructions inside an attachment remain reference content.
The selected model must support the supplied media. Audio files use an
OpenAI-compatible model implementing `input_audio`; the panel explains this
requirement before sending and keeps the draft available when it is not met.

## Codex setup

Install [Codex CLI](https://learn.chatgpt.com/docs/cli) and run `codex login` with
your ChatGPT account. Auris starts `codex app-server --listen stdio://`; Codex
manages its authentication and inference. Auris does not read credential files or
use subscription tokens through rig. API-key authentication is refused by this
backend. Use rig for an API endpoint.

Leave the executable path empty to search PATH. On Windows, Auris also recognizes
the native executable inside an npm installation; a custom path must name
`codex.exe`, not a PowerShell or batch launcher. The integration is tested with
Codex CLI 0.163.0. The app-server protocol is experimental, so incompatible CLI
versions may report startup errors.

Open the Agent Panel on a new, unsaved song and ask it to add an instrument track.
The track appears immediately, the song remains unsaved, and Undo removes the
track through the normal document history. The same permission modes and deny
rules apply to both backends. Saving is still an explicit user action.

## Runtime boundary

`auris-agent::Worker` owns the background runtime and cancellation. Its backend
owns the conversation loop: rig runs it locally, while Codex drives an app-server
child process. Provider protocol types stay inside the backend. `ToolHost`
normalizes flat toolbox calls, serializes authorization and execution, and sends
live commands to the window. The window owns the session and rechecks permissions
immediately before an edit.

Codex connects to an embedded Streamable HTTP MCP server on an ephemeral loopback
port with a random per-conversation bearer token. This endpoint exposes the live
tool catalog, not the saved-file `auris-mcp` catalog. It supports unsaved documents
and never opens a second copy of the song. Codex's tool approvals are delegated
to the common ToolHost; the application's live permission checks still authorize
every call. Its listener and child process are revoked when the worker ends.
Stop requests interrupt the Codex turn before the
process is terminated, while the window immediately stops accepting old edits.

The child uses a private working directory. Auris disables inherited MCP servers,
plugins, apps, hooks, shell execution and subagents for this invocation, and checks
the effective MCP catalog before sending a prompt. User configuration files are
not edited. Unexpected server-side requests are refused. Process arguments never
include the live MCP bearer token.

Completed exchanges use the existing Auris transcript storage. Codex threads are
ephemeral; a replacement worker receives the stored text as historical context
once, then Codex owns the active conversation. The transcript preview and reset
generation checks remain shared with rig.

## Validation

`cargo test -p auris-agent` includes a scripted app-server peer that calls the real
HTTP MCP endpoint, passes the common permission gate, edits a real unsaved
headless session and verifies Undo. The existing desktop harness verifies live
edits and permissions in the window. The default tests require no model account.

With a ChatGPT-authenticated Codex installation, this optional transport check
starts app-server and verifies its live MCP catalog without running inference:

```sh
cargo test -p auris-agent installed_codex_connects_to_the_live_mcp_catalog -- --ignored
```

To verify a real model turn, `cargo test -p auris-agent
installed_codex_edits_an_unsaved_song -- --ignored` uses the signed-in ChatGPT
subscription to add one track to an in-memory session and verify Undo. It does
not open or modify a saved song.
