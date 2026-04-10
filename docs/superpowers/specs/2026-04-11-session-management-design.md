# Session Management: /new, /switch, /list + Autocomplete Fix

## Problem

1. **Single session per chat+thread.** Users cannot have multiple concurrent conversations with an agent. `/reset` destroys the session — no way to go back.
2. **Slash commands don't autocomplete** in Telegram (mobile or desktop). The CC Telegram plugin uses the same bot token and overwrites our commands via `set_my_commands` with default scope.

## Solution

Three new commands (`/new`, `/switch`, `/list`) replace `/reset`. Per-chat command scope fixes autocomplete.

## Schema

Replace `telegram_sessions` with a new `sessions` table supporting multiple sessions per `(chat_id, thread_id)`.

```sql
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id INTEGER NOT NULL,
    thread_id INTEGER NOT NULL DEFAULT 0,
    root_session_id TEXT NOT NULL,      -- CC session UUID
    label TEXT,                          -- first message (truncated 60 chars) or explicit /new <name>
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    last_used_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- At most one active session per (chat_id, thread_id)
CREATE UNIQUE INDEX idx_sessions_active
    ON sessions(chat_id, thread_id) WHERE is_active = 1;
```

### Migration

Single migration: `DROP TABLE IF EXISTS telegram_sessions` then create `sessions`. Old sessions are lost — CC retains conversation history independently, so sessions resume naturally on next message.

## Commands

| Command | Behavior |
|---------|----------|
| `/new` | Deactivate current session, kill worker. Next message creates a new session with label = message text (truncated 60 chars). Replies with info about previous session UUID (in `<pre>` for tap-to-copy). |
| `/new <name>` | Deactivate current session, kill worker. Create new session immediately with label = `<name>`. Reply with new session UUID. |
| `/list` | Show all sessions for `(chat_id, thread_id)` ordered by `last_used_at DESC`. Active session marked with bullet. Each entry: label, relative time, UUID in `<pre>`. |
| `/switch <uuid>` | Deactivate current session, activate target (partial match via `LIKE '%<input>%'`), kill worker. Next message resumes target session via `--resume <uuid>`. |
| `/start` | Greeting only, no session changes (unchanged). |
| `/mcp`, `/doctor` | Unchanged. |

### `/reset` removal

`/reset` is removed. `/new` replaces it — old session is deactivated, not deleted.

### Registered commands (for autocomplete)

`/start`, `/new`, `/list`, `/switch`, `/mcp`, `/doctor` — six commands total.

## UX Details

### `/list` format

```
Sessions:
● crypto research — 5m ago
  `550e8400-e29b-41d4-a716-446655440000`
  test cron setup — 2h ago
  `7a3f1b22-c9d8-4e5f-b123-abcdef012345`
```

Active session marked with `●`, inactive with two spaces.

### `/switch` partial match

Search `root_session_id LIKE '%<input>%'` scoped to `(chat_id, thread_id)`.

- 0 matches: "No session matching `<input>`. Use /list to see available sessions."
- 1 match: activate it.
- 2+ matches: show all matches and ask user to be more specific.

### `/switch` on already-active session

No-op. Reply: "Already active."

### First message (no sessions exist)

Create new session automatically. Label = message text (truncated 60 chars). Same as current behavior, just in new table.

### `/new` response (no name)

```
Session cleared.
Previous session:
`550e8400-e29b-41d4-a716-446655440000`
Tap to copy, then /switch to return.
```

Send a message to start a new conversation.

### `/new <name>` response

```
New session: crypto research
Previous session:
`550e8400-e29b-41d4-a716-446655440000`
Tap to copy, then /switch to return.
```

## Autocomplete Fix

### Problem

CC Telegram plugin calls `set_my_commands` with default scope on the same bot token, overwriting our commands.

### Solution

Register commands with `BotCommandScopeChat` for each `chat_id` from the agent's allowlist in `agent.yaml`. Per-chat scope has higher priority than default scope in Telegram's resolution order.

```rust
for chat_id in allowed_chat_ids {
    bot.set_my_commands(commands)
       .scope(BotCommandScopeChat { chat_id })
       .await;
}
```

- Empty allowlist (secure default): no commands registered anywhere.
- Allowlist changes trigger bot restart via config watcher — new chat_ids get commands on next startup.
- `delete_my_commands` also scoped per chat_id before setting new ones.

## Handler Changes

### `/new` handler

1. `UPDATE sessions SET is_active = 0 WHERE chat_id = ? AND thread_id = ? AND is_active = 1`
2. If `/new <name>`: insert new row with label, generate UUID, set `is_active = 1`
3. Drop sender from DashMap (kills worker + CC subprocess via `kill_on_drop`)
4. Reply with confirmation + previous session UUID

### `/switch` handler

1. Partial-match query on `root_session_id`
2. If exactly 1 match: deactivate current, `UPDATE sessions SET is_active = 1 WHERE id = ?`
3. Drop sender from DashMap
4. Reply with confirmation

### `/list` handler

1. `SELECT * FROM sessions WHERE chat_id = ? AND thread_id = ? ORDER BY last_used_at DESC`
2. Format and reply

## Worker Changes

Minimal changes to `worker.rs`:

- Replace `session::get_session` / `session::create_session` with new functions operating on `sessions` table
- `get_active_session(conn, chat_id, thread_id) -> Option<(root_session_id, id)>`
- `create_session(conn, chat_id, thread_id, uuid, label) -> id`
- `touch_session(conn, id)` — updates `last_used_at` by row `id`
- On first message in new session: pass message text (truncated 60 chars) as label to `create_session`

DashMap key remains `(chat_id, thread_id)`. One worker per chat+thread; the underlying CC session UUID changes via `/new` and `/switch`.

## Edge Cases

- **Worker kill during CC response:** CC subprocess interrupted via `kill_on_drop`. User receives `/new` or `/switch` confirmation, so interruption is expected.
- **Concurrent messages during `/switch`:** Handler holds DashMap lock briefly. Existing stale-sender detection logic handles the race — respawns worker if send fails.
- **Session limit:** None enforced. Future cleanup of sessions with `last_used_at > 30d` is out of scope.
- **Groups (future):** Schema supports it — `thread_id` separates sessions by topic. `BotCommandScopeChat` works for groups. No group-specific work needed now.

## Cron Impact

None. Crons are stateless — no session table interaction.

## Files to Modify

| File | Change |
|------|--------|
| `crates/rightclaw/src/memory/migrations.rs` | Add migration: drop `telegram_sessions`, create `sessions` |
| `crates/bot/src/telegram/session.rs` | Rewrite: new CRUD for `sessions` table |
| `crates/bot/src/telegram/handler.rs` | Add `/new`, `/list`, `/switch` handlers; remove `/reset` |
| `crates/bot/src/telegram/dispatch.rs` | Update `BotCommand` enum; per-chat `set_my_commands` |
| `crates/bot/src/telegram/worker.rs` | Use new session functions |
