---
phase: 35-token-refresh
plan: "02"
subsystem: mcp-refresh
tags: [mcp, oauth, token-refresh, scheduler, tokio]
dependency_graph:
  requires: [35-01]
  provides: [mcp::refresh, run_refresh_scheduler, refresh_token_for_server, post_refresh_grant, deadline_from_unix]
  affects: [35-03-integration]
tech_stack:
  added: []
  patterns: [tokio::spawn per-token task, sleep_until deadline scheduling, mock HTTP server tests]
key_files:
  created:
    - crates/rightclaw/src/mcp/refresh.rs
  modified:
    - crates/rightclaw/src/mcp/mod.rs
key_decisions:
  - "deadline_from_unix returns None for expires_at=0 (REFRESH-04), underflow guard (expires_at <= buffer), and already-within-buffer — all mean refresh immediately or never"
  - "refresh_token fallback: if provider returns new refresh_token use it, else keep old — handles rotating and non-rotating providers uniformly"
  - "scheduler only spawns tasks for servers with stored credential, refresh_token present, and expires_at != 0"
metrics:
  duration: "10m"
  completed: "2026-04-03"
  tasks_completed: 1
  files_modified: 2
---

# Phase 35 Plan 02: mcp::refresh module with scheduler and refresh grant Summary

**One-liner:** Implemented `mcp::refresh` module with `run_refresh_scheduler` spawning per-server tokio tasks, `run_token_refresh_loop` with 3-retry backoff, `post_refresh_grant` form POST, and `deadline_from_unix` with REFRESH-04 non-expiring guard and underflow protection.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Implement mcp::refresh module with scheduler and refresh grant | cf79ce4 | refresh.rs (new, 482 lines), mod.rs |

## Decisions Made

- `deadline_from_unix(0, buffer)` returns None — REFRESH-04 guard: non-expiring tokens are never scheduled.
- `deadline_from_unix(expires_at, buffer)` where `expires_at <= buffer` returns None — underflow guard prevents saturating subtraction from producing garbage deadlines.
- Scheduler checks `credential.refresh_token.is_none()` before spawning — no task for servers that can't be refreshed.
- Old refresh_token is preserved if provider doesn't return a new one (`token_resp.refresh_token.or(Some(old_refresh_token))`).
- `run_token_refresh_loop` re-reads credential on each loop iteration — picks up tokens updated by the OAuth flow.

## Verification

- `cargo test -p rightclaw mcp::refresh` — 8 tests pass
- `cargo build --workspace` — zero errors
- `rg "if token.expires_at == 0" crates/rightclaw/src/mcp/refresh.rs` — REFRESH-04 guard found
- `rg "pub mod refresh" crates/rightclaw/src/mcp/mod.rs` — module declared

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan references McpStatus but actual struct is ServerStatus**
- **Found during:** Task 1 (reading detect.rs before implementation)
- **Issue:** Plan interface section shows `McpStatus` but `detect.rs` exports `ServerStatus` (renamed in Phase 33). The plan was written against the old name.
- **Fix:** Used `ServerStatus` throughout refresh.rs; updated `status.state` checks to match actual field names.
- **Files modified:** `crates/rightclaw/src/mcp/refresh.rs`
- **Commit:** cf79ce4 (bundled with task)

## Known Stubs

None — all functions are fully implemented with real HTTP calls and credential I/O.

## Self-Check: PASSED
