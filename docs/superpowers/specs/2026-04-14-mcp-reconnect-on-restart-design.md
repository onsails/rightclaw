# MCP Reconnect on Restart

## Problem

When the Aggregator restarts (full process-compose restart), all external MCP servers show as "unreachable (0 tools)" even though their OAuth tokens are persisted in SQLite. Two root causes:

1. **No `connect()` on startup.** `ProxyBackend::new()` initializes with `status = Unreachable` and `cached_tools = vec![]`. The startup loop in `main.rs:433-476` creates backends from SQLite data but never calls `connect()`. Only `handle_mcp_add` and `handle_set_token` (runtime paths) call `connect()`.

2. **Refresh scheduler lives in the wrong process.** `run_refresh_scheduler` runs inside the bot process (`crates/bot/src/lib.rs:236`), but `ProxyBackend` instances live in the Aggregator process. The scheduler's `token_handles` map starts empty on restart (no `NewEntry` messages arrive), so even when a refresh timer fires, the in-memory `Arc<RwLock<Option<String>>>` in the Aggregator's `ProxyBackend` is never updated. The scheduler only writes to SQLite — a different process can't observe that change.

## Design

### 1. Startup reconnect (Aggregator, `main.rs`)

After creating all `ProxyBackend` instances in the startup loop (before `run_aggregator_http`), spawn background tasks to connect each server:

- **Non-OAuth** (bearer, header, query_string, no auth): call `connect()` immediately.
- **OAuth with `refresh_token` + `expires_at` still in the future**: call `do_refresh()` to get a fresh token, update the in-memory Arc + SQLite, then `connect()`.
- **OAuth with expired `expires_at` or no `refresh_token`**: set status to `NeedsAuth`, skip `connect()`.

Load `expires_at`, `refresh_token`, `token_endpoint`, `client_id`, `client_secret` from the `mcp_servers` SQLite table alongside the existing server data.

### 2. Move refresh scheduler to Aggregator

Transfer `run_refresh_scheduler` from the bot process to `run_aggregator_http`:

- Create `(refresh_tx, refresh_rx)` channel in `run_aggregator_http`.
- Spawn `run_refresh_scheduler` with `agents_dir` (or per-agent dirs).
- For each OAuth server loaded on startup that has a valid `refresh_token`, send `RefreshMessage::NewEntry` with the `token` Arc from the corresponding `ProxyBackend`.
- Pass `refresh_tx` to `internal_router` so `handle_set_token` can send `NewEntry` to the scheduler after delivering a new OAuth token.

The scheduler now has direct access to the same `Arc<RwLock<Option<String>>>` that `ProxyBackend` uses, so token refreshes update in-memory state correctly.

**Notify channel**: refresh errors are logged via `tracing::warn!` only. No Telegram notification — users see server status via `/mcp list`. Telegram notify for refresh errors can be added later as a separate feature (requires a reverse channel from Aggregator to bot that doesn't exist yet).

### 3. Multi-agent refresh

The current refresh scheduler is single-agent (one `agent_dir`). The Aggregator serves multiple agents. Two options:

- **(a)** One refresh scheduler per agent, each with its own channel and `agent_dir`.
- **(b)** Single scheduler that handles all agents, keyed by `(agent_name, server_name)`.

Use **(a)** — matches the existing per-agent `BackendRegistry` structure, avoids refactoring `RefreshMessage` to carry agent context. Each agent's scheduler is independent.

### 4. Cleanup bot

Remove from `crates/bot/`:

- `lib.rs`: `refresh_tx`, `refresh_rx`, `notify_refresh_tx`, `notify_refresh_rx` channels, scheduler spawn, notify forward task.
- `telegram/handler.rs`: `RefreshTx` newtype.
- `telegram/dispatch.rs`: `refresh_tx` parameter, `refresh_tx_arc` wrapping, DI injection.
- `run_telegram` signature: remove `refresh_tx` parameter.

The `RefreshMessage` type and `run_refresh_scheduler` function stay in `crates/rightclaw/src/mcp/refresh.rs` (core crate) — only the call site moves.

### 5. `handle_set_token` integration

Currently `handle_set_token` in `internal_api.rs`:
1. Updates in-memory token Arc.
2. Persists to SQLite.
3. Spawns `connect()` in background.

After the change, add step 4: send `RefreshMessage::NewEntry` to the agent's refresh scheduler so it schedules future refreshes.

This requires `internal_router` to receive a map of `agent_name -> refresh_tx` channels.

## Files changed

| File | Change |
|------|--------|
| `crates/rightclaw-cli/src/main.rs` | Startup reconnect loop, create per-agent refresh channels |
| `crates/rightclaw-cli/src/aggregator.rs` | Accept refresh channels, spawn schedulers, pass to internal router |
| `crates/rightclaw-cli/src/internal_api.rs` | Accept refresh_tx map, send `NewEntry` in `handle_set_token` |
| `crates/rightclaw/src/mcp/credentials.rs` | No changes needed — `McpServerEntry` already includes all OAuth fields |
| `crates/bot/src/lib.rs` | Remove refresh scheduler spawn and channels |
| `crates/bot/src/telegram/handler.rs` | Remove `RefreshTx` |
| `crates/bot/src/telegram/dispatch.rs` | Remove `refresh_tx` parameter and DI |

## Not in scope

- Telegram notifications for refresh errors (requires Aggregator→bot reverse channel).
- Retry logic for `connect()` failures on startup (server temporarily down). Can be added later.
- Periodic reconnect attempts for `Unreachable` servers. Currently requires manual `/mcp add` or bot restart.
