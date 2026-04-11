# Cron Telegram Commands & Trigger — Design Spec

## Goal

Add read-only `/cron` Telegram commands for monitoring cron jobs, and an MCP `cron_trigger` tool for manual job execution through the existing cron engine pipeline.

## Scope

1. **Telegram `/cron`** — list all jobs with human-readable schedules and last run status
2. **Telegram `/cron <job-name>`** — job details + recent run history
3. **MCP `cron_trigger`** — queue a job for immediate execution via `triggered_at` column
4. **Migration V7** — add `triggered_at` column to `cron_specs`
5. **Cron engine update** — check `triggered_at` alongside schedule in `reconcile_jobs`
6. **SKILL.md update** — document `cron_trigger`
7. **Comprehensive tests** for all components

## Architecture

### `/cron` Telegram Command

Follows the existing `/mcp` pattern in `handler.rs`:
- `BotCommand::Cron(String)` variant in `dispatch.rs`
- `handle_cron(bot, msg, args, agent_dir)` dispatcher
- Subcommand routing via `args.split_whitespace()`

**`/cron` (no args or "list")** — overview of all jobs:
```
Cron Jobs:

- health-check — Every 5 minutes — last: 2m ago ✅
- github-tracker — At minute 0 and 30 — last: 28m ago ✅
- weekly-report — At 09:00 AM, Monday — never run
```

**`/cron <job-name>`** — details + last 5 runs:
```
health-check
Schedule: Every 5 minutes (*/5 * * * *)
Budget: $0.05
Lock TTL: 30m

Recent runs:
  1. 2m ago — ✅ success (12s)
  2. 7m ago — ✅ success (8s)
  3. 12m ago — ❌ failed (exit 1)
```

Human-readable schedule descriptions via `cron-descriptor` crate (0.1.1). Falls back to raw cron expression if parsing fails.

### MCP `cron_trigger` Tool

**Parameters:**
```rust
pub struct CronTriggerParams {
    pub job_name: String,
}
```

**Behavior:**
1. Validate job exists in `cron_specs`
2. `UPDATE cron_specs SET triggered_at = datetime('now') WHERE job_name = ?`
3. Return success message: `"Triggered job '{name}'. Will execute on next engine tick (≤60s)."`
4. If job not found: return error

**Shared helper** in `cron_spec.rs` (`trigger_spec(conn, job_name)`), called from both MCP servers (stdio `memory_server.rs` + HTTP `memory_server_http.rs`).

### Migration V7

```sql
ALTER TABLE cron_specs ADD COLUMN triggered_at TEXT;
```

Single nullable column. No index needed — the engine loads all specs on each tick anyway.

### Cron Engine Integration

Unified check in `reconcile_jobs` (single code path for both schedule and trigger):

```rust
let should_run = schedule_matches || spec.triggered_at.is_some();
if should_run && !locked {
    if spec.triggered_at.is_some() {
        clear_triggered_at(conn, &job_name);
    }
    execute_job(...)
}
```

- Lock check applies equally — if locked, trigger waits (not cleared until job actually spawns)
- `execute_job()` unchanged — doesn't know/care about trigger source
- Max 60s delay between trigger and execution (engine tick interval)

### SKILL.md Update

Add `cron_trigger` to the `skills/rightcron/SKILL.md` tool documentation with usage examples.

## Files Changed

| File | Change |
|------|--------|
| `crates/rightclaw/src/memory/sql/v7_cron_trigger.sql` | New migration |
| `crates/rightclaw/src/memory/migrations.rs` | Wire V7 |
| `crates/rightclaw/src/cron_spec.rs` | Add `trigger_spec()`, `clear_triggered_at()`, `get_spec_detail()`, `get_recent_runs()` helpers |
| `crates/rightclaw/Cargo.toml` | Add `cron-descriptor` dependency |
| `crates/bot/src/telegram/dispatch.rs` | Add `BotCommand::Cron(String)` |
| `crates/bot/src/telegram/handler.rs` | Add `handle_cron`, `handle_cron_list`, `handle_cron_detail` |
| `crates/bot/src/cron.rs` | Check `triggered_at` in reconcile loop, add `CronSpec.triggered_at` field |
| `crates/rightclaw-cli/src/memory_server.rs` | Add `cron_trigger` tool + update `with_instructions()` |
| `crates/rightclaw-cli/src/memory_server_http.rs` | Add `cron_trigger` tool + update `with_instructions()` |
| `skills/rightcron/SKILL.md` | Document `cron_trigger` |

## Tests

### `cron_spec.rs` tests
- `trigger_spec` — sets `triggered_at`, verify column is non-null
- `trigger_nonexistent_job` — returns error
- `trigger_clears_on_read` — verify `triggered_at` included in loaded specs
- `get_spec_detail` — returns full spec with schedule description
- `get_recent_runs` — returns last N runs for a job, ordered by recency
- `get_recent_runs_empty` — no runs returns empty list

### `cron.rs` tests
- `triggered_job_executes` — job with `triggered_at` set gets picked up
- `triggered_and_locked_waits` — triggered job with active lock doesn't run, `triggered_at` preserved
- `trigger_cleared_after_spawn` — `triggered_at` is null after job starts
- `schedule_and_trigger_simultaneous` — both true at same time = single execution, not double

### `handler.rs` tests
- `/cron` list formatting — correct output with multiple jobs
- `/cron <job>` detail formatting — correct output with run history
- `/cron unknown-job` — error message

### Migration test
- V7 migration applies cleanly, `triggered_at` column exists and is nullable

## Non-Goals

- CRUD via Telegram (already covered by MCP tools)
- Synchronous trigger execution (trigger is fire-and-forget, delivery loop handles results)
- Trigger queue (multiple triggers of same job collapse — latest `triggered_at` wins)
