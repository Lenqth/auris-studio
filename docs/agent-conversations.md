# Agent conversations

[日本語](agent-conversations.ja.md)

The Agent panel keeps separate conversations for each project. **New conversation**
preserves the existing history and draft. **Conversations** opens the list, ordered
by the most recent message. Search matches conversation titles. The first request
provides the initial title; the conversation menu offers rename, archive, restore
and delete. The **Archived** filter shows archived conversations.

Opening a conversation reads local history without starting a model worker.
Sending a message continues that conversation against the project's current state.
Opening history does not restore an older arrangement, and deleting a conversation
does not remove music or change Undo history.

While a turn is running, another conversation opens as a preview. The original
conversation keeps receiving its output and owns any approval requests. Return to
it to handle approvals, or use **Stop current work and resume this conversation**
to stop the original worker before continuing elsewhere. Only one conversation
can execute for a project at a time, including across windows. Idle conversations
release their execution lease so another window can continue.

## Persistence

Saved projects keep conversations under `.auris/agent/conversations/`, with one
directory per opaque conversation ID:

- `meta.json`: title, creation/update times, preview and archive state;
- `transcript.json`: readable messages, tool results, draft and scroll position;
- `memory.json`: the runtime's bounded context for the next request.

The transcript remains separate from context compaction. Streamed output updates
the same message identity. Snapshots are written on background threads, at most
twice a second during ordinary editing. A recovered unfinished turn is marked as
interrupted and never runs automatically. Metadata operations use a store lock;
transcript revisions reject stale writes from other windows. Deletion invalidates
runtime memory and retains a small tombstone so late writes cannot restore it.

Before the first project save, conversation storage lives in the application's
private recovery directory. A workspace identity follows project autosave recovery.
Save As copies the conversations into the new project after detaching its worker.
Conversation changes do not mark the score as modified.

The old `.auris-conversation.json` file is imported once as a conversation. Its
completed exchanges and summary are retained. After import, the old runtime
generation is invalidated and the old file is removed.
