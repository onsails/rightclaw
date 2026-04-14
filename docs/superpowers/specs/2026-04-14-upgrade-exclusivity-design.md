# Upgrade Exclusivity Design

## Problem

The `claude upgrade` background task (`upgrade.rs`) is disabled because it conflicts with running CC sessions — `claude install`/`claude upgrade` kills a running `claude auth login` via shared lockfiles/daemon. We need to re-enable it with guarantees:

1. Upgrade runs only when no CC sessions are active (no cron jobs, no user message handlers, no cron deliveries)
2. While upgrade runs, new CC invocations block until it completes (user messages buffer silently)
3. If upgrade cannot acquire exclusivity, it skips until the next 8-hour cycle

## Design

### Primitive

```rust
type UpgradeLock = Arc<tokio::sync::RwLock<()>>;
```

Single shared `RwLock<()>` — upgrade takes write (exclusive), all CC invocations take read (shared, concurrent).

### Integration Points

#### 1. `upgrade.rs` — write lock (exclusive)

Re-enable `run_upgrade_loop()`: remove the disabled stub, restore the 8-hour interval loop.

On each periodic tick:
- `try_write()` on the lock
- If acquired: hold guard for duration of `run_upgrade()` (install + upgrade)
- If not acquired: log "skipping upgrade — active sessions", wait for next 8h tick

Startup upgrade is handled separately (see Startup Order below).

#### 2. `worker.rs` — read lock (shared)

In `spawn_worker()`, immediately before `invoke_cc()` (~line 365):
```rust
let _guard = upgrade_lock.read().await;
let reply_result = invoke_cc(&input, first_text, chat_id, eff_thread_id, &ctx).await;
// guard held through CC invocation + bootstrap reverse sync
// dropped after reply processing
```

Guard covers `invoke_cc()` through bootstrap reverse sync (if applicable). Debounce and attachment download happen outside the lock — only the CC subprocess is protected.

Messages buffer naturally: the mpsc channel (capacity 32) and debounce window absorb incoming messages while `read().await` blocks.

#### 3. `cron.rs` — read lock (shared)

In `execute_job()`, after per-job lock check but before subprocess spawn:
```rust
let _guard = upgrade_lock.read().await;
// ... spawn claude -p subprocess
```

Guard held for duration of CC subprocess. Cron jobs remain concurrent with each other (shared read). Per-job file locks unchanged.

#### 4. `cron_delivery.rs` — read lock (shared)

In `deliver_through_session()`, before CC invocation:
```rust
let _guard = upgrade_lock.read().await;
// ... invoke claude -p for delivery
```

### Lock Propagation

`UpgradeLock` passes through existing plumbing, same pattern as `idle_timestamp` and `shutdown`:

| Source | Target | Mechanism |
|--------|--------|-----------|
| `lib.rs` | `spawn_upgrade_task()` | New parameter |
| `lib.rs` | `run_cron_task()` | New parameter |
| `lib.rs` | `run_delivery_loop()` | New parameter |
| `lib.rs` | `run_telegram()` → handler | New field in `WorkerContext` |

### Startup Order

Current order in `lib.rs`:
1. `initial_sync()` — blocking
2. Spawn cron task
3. Spawn cron delivery
4. Spawn upgrade task
5. `run_telegram()`

New order:
1. `initial_sync()` — blocking
2. **`run_upgrade()` — blocking, no lock needed** (no cron/telegram running yet)
3. Spawn cron task (with `UpgradeLock`)
4. Spawn cron delivery (with `UpgradeLock`)
5. Spawn upgrade task (with `UpgradeLock`, first tick after 8h, not immediate)
6. `run_telegram()` (with `UpgradeLock` in handler context)

Startup upgrade runs as a blocking call before any concurrent tasks exist — no lock contention possible. The background upgrade task starts its first tick after 8 hours (not immediately), avoiding a redundant immediate check.

### What Does NOT Change

- `sync.rs` — not part of upgrade coordination
- `idle_timestamp` — unchanged, cron delivery continues its idle gating
- Cron per-job file locks — unchanged
- Worker debounce/attachment pipeline — not under lock
- `run_upgrade()` function itself — unchanged, only its callers change

## Failure Modes

| Scenario | Behavior |
|----------|----------|
| Upgrade cannot acquire lock (active sessions) | Logs warning, skips to next 8h tick |
| Upgrade fails (SSH error, install error) | Logged, task continues — retries in 8h |
| Upgrade holds lock, user sends message | Message buffered in mpsc channel, worker blocks on `read().await`, processes after upgrade completes |
| Upgrade holds lock, cron fires | Cron's `execute_job()` blocks on `read().await`, executes after upgrade |
| Bot shutdown during upgrade | `CancellationToken` cancels the upgrade loop; `tokio::select!` in upgrade loop picks up cancellation |
| Startup upgrade fails | Logged, bot continues — periodic task retries in 8h |
