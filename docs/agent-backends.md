# Agent backends

The Agent Panel supports rig (Ollama or OpenAI-compatible APIs) and the official
Codex CLI. Select a backend in Settings > Agent, apply it, then select a model in
the Agent Panel. Changing backends clears the selected model; the rig endpoint
and API-key environment-variable preference are retained.

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
