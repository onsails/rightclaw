---
phase: 36-auto-derive-cfargotunnel-hostname-from-tunnel-token-jwt
plan: "01"
subsystem: config
tags: [jwt, cloudflared, config, cli, tdd]
dependency_graph:
  requires: []
  provides: [TunnelConfig::hostname(), token-only config]
  affects: [cmd_init, cmd_up, handle_mcp_auth]
tech_stack:
  added: []
  patterns: [JWT base64url decode via base64::URL_SAFE_NO_PAD + serde_json::from_slice]
key_files:
  created: []
  modified:
    - crates/rightclaw/src/config.rs
    - crates/rightclaw-cli/src/main.rs
    - crates/bot/src/telegram/handler.rs
    - crates/rightclaw/src/mcp/refresh.rs
decisions:
  - "D-04: TunnelConfig struct now has only token: String — hostname field removed"
  - "D-05: hostname() derives <uuid>.cfargotunnel.com on every call, no caching"
  - "D-06: write_global_config writes only token field — YAML shrinks from 3 to 2 lines"
  - "D-11: hostname()? called before write_global_config in cmd_init — fail fast on bad token"
  - "D-12: cmd_up calls tunnel_cfg.hostname()? — defensive re-validation on each launch"
metrics:
  duration: ~15min
  completed: 2026-04-04T20:49:43Z
  tasks_completed: 2
  files_modified: 4
---

# Phase 36 Plan 01: JWT Hostname Derivation Summary

JWT base64url decode of cloudflared tunnel token payload to derive `<uuid>.cfargotunnel.com` — `--tunnel-hostname` arg removed entirely.

## What Was Built

`TunnelConfig::hostname()` splits the JWT on `.`, base64url-decodes the payload segment with `URL_SAFE_NO_PAD`, parses JSON, extracts `"t"` field (tunnel UUID), returns `{uuid}.cfargotunnel.com`. All threat model cases handled via fail-fast `?` propagation.

`cmd_init` now accepts only `--tunnel-token`. Calls `hostname()?` before writing config (bad token never persisted). Prints `Tunnel hostname: <uuid>.cfargotunnel.com` to stdout. `cmd_up` calls `tunnel_cfg.hostname()?` instead of field access.

Old `config.yaml` files with `hostname:` field silently ignored — serde-saphyr drops unknown fields by default.

## Commits

| Hash | Message |
|------|---------|
| `9880a27` | test(36-01): add failing tests for JWT hostname derivation and token-only config shape |
| `13b30e4` | feat(36-01): implement TunnelConfig::hostname() and remove --tunnel-hostname arg |

## Test Results

- 7 new JWT tests in `config.rs`: all pass
- 447 workspace tests pass total
- Pre-existing failure: `test_status_no_running_instance` (documented in MEMORY.md, unrelated to this plan)
- Clippy: zero warnings

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] bot/handler.rs used `tunnel.hostname` as field (two call sites)**
- **Found during:** Task 2 — workspace build
- **Issue:** `crates/bot/src/telegram/handler.rs` lines 380 and 407 accessed `tunnel.hostname` as field; after struct change it became a method
- **Fix:** Line 380 — extracted `tunnel_hostname` via `match tunnel.hostname()` with user-facing error message; line 407 — reused the same variable. Updated stale init instruction text from `--tunnel-hostname` to token-only.
- **Files modified:** `crates/bot/src/telegram/handler.rs`
- **Commit:** `13b30e4`

**2. [Rule 1 - Bug] Pre-existing clippy warnings blocking `-D warnings`**
- **Found during:** Task 2 — clippy run
- **Issues:**
  - `refresh.rs:120` — `&PathBuf` instead of `&Path` (`ptr_arg`)
  - `handler.rs:180` — redundant `.trim()` before `split_whitespace()` (`trim_split_whitespace`)
  - `handler.rs:213-214` — `.map_err(|e| e)` identity map (`map_identity`)
  - `main.rs:596` — needless `&home` borrow (`needless_borrow`)
- **Fix:** Applied all clippy suggestions inline
- **Files modified:** `crates/rightclaw/src/mcp/refresh.rs`, `crates/bot/src/telegram/handler.rs`, `crates/rightclaw-cli/src/main.rs`
- **Commit:** `13b30e4`

## Known Stubs

None.

## Self-Check: PASSED

- `crates/rightclaw/src/config.rs` — FOUND, modified
- `crates/rightclaw-cli/src/main.rs` — FOUND, modified
- `crates/bot/src/telegram/handler.rs` — FOUND, modified
- Commit `9880a27` — FOUND
- Commit `13b30e4` — FOUND
- All config tests pass (41/41)
- Clippy: zero warnings
