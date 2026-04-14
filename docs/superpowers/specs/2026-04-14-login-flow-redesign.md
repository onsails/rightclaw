# Login Flow Redesign: PTY-free OAuth via Local Callback

## Problem

Claude Code CLI auto-updated its login TUI, breaking the PTY-driven login flow in `login.rs`. The old flow uses `expectrl` to navigate a multi-step interactive TUI (`claude -- /login`), which is fragile — any UI change breaks regex matching. The new CLI TUI changed its output format, causing step 7 (success detection) to fail silently.

## Key Discovery

`claude auth login` starts a local HTTP callback server on a random port inside the sandbox (e.g., `[::1]:36275`). The server accepts `GET /callback?code=CODE&state=STATE` and forwards the code to Anthropic's token exchange endpoint. This means we can complete the OAuth flow by curling localhost — no PTY interaction needed.

### Verified Behavior

| Step | Observation |
|------|-------------|
| `claude auth login` output | Plain text: URL on stdout, no TUI |
| Local callback server | Listens on `[::1]:RANDOM_PORT`, discoverable via `ss -tlnp` |
| `GET /callback?code=X&state=Y` | Returns 302 → success page. CLI exchanges code for token. |
| Fake code | CLI responds "Login failed: Request failed with status code 400" — mechanism works, just invalid code |
| Process lifecycle | CLI exits after callback received (success or failure) |

### Policy Prerequisite

No policy changes needed. `claude auth login` runs without `script`/PTY — it outputs the URL to stdout and starts a local callback server without requiring `/dev/tty` or `/dev/pts`.

## Design

### New Login Flow

```
Bot detects auth error (403/401 from claude -p)
  │
  ├─ Phase 1: Start auth session
  │   SSH exec: script -q -c "claude auth login" /dev/null
  │   (background, keep process alive)
  │   Parse URL from stdout
  │   Extract state= parameter from URL
  │
  ├─ Phase 2: Discover callback port
  │   SSH exec: ss -tlnp | grep claude
  │   Extract port number
  │
  ├─ Phase 3: User interaction (Telegram)
  │   Send OAuth URL to user
  │   Wait for user to send auth code
  │
  └─ Phase 4: Complete auth
      SSH exec: curl "http://[::1]:$PORT/callback?code=$CODE&state=$STATE"
      Monitor auth process exit
      Exit 0 → success, nonzero → report error to user
```

### Changes

#### `crates/bot/src/login.rs` — Full Rewrite

Replace expectrl-based PTY flow with async SSH exec:

1. **`start_auth_session()`**: Spawn `ssh ... -- 'script -q -c "claude auth login" /dev/null'` via `tokio::process::Command` with piped stdout. Read stdout until URL appears. Extract URL and `state` parameter. Return handle to keep process alive.

2. **`discover_callback_port()`**: `ssh ... -- 'ss -tlnp'` → parse port from line containing `claude`. Retry up to 5 times with 1s backoff (process may need time to bind).

3. **`submit_auth_code()`**: `ssh ... -- 'curl -s -o /dev/null -w "%{http_code}" "http://[::1]:$PORT/callback?code=$CODE&state=$STATE"'` → check HTTP status. 302 = code accepted (token exchange in progress). Wait for auth process to exit — exit 0 = success.

#### `crates/bot/src/telegram/worker.rs` — Adapt Auth Watcher

Replace `spawn_auth_watcher()` to use new login functions instead of `run_login_pty()`. Same Telegram interaction pattern: send URL, wait for code, submit code.

#### `crates/rightclaw/src/codegen/policy.rs` — Add /dev/tty and /dev/pts

Add to `read_write` section:
```yaml
read_write:
    - /dev/null
    - /dev/tty
    - /dev/pts
    - /tmp
    - /sandbox
    - /platform
```

#### Dependencies — Remove expectrl

If `expectrl` is not used elsewhere, remove from `Cargo.toml`.

### What Stays the Same

- Auth error detection in worker (`is_auth_error()`)
- Telegram flow: URL sent to user, code received from user
- `LoginEvent` enum (Url, WaitingForCode, Done, Error)
- OAuth callback server (`oauth_callback.rs`) — unrelated, handles MCP OAuth

### Edge Cases

| Case | Handling |
|------|----------|
| Port not found in 10s | Error: "Claude auth server didn't start" |
| Auth process exits before code sent | Error: "Auth session terminated prematurely" |
| Invalid code (400 from Anthropic) | CLI prints error, exits nonzero → relay to user |
| Timeout (user never sends code) | Kill auth process after configurable timeout (default 5min) |
| Multiple concurrent logins | Not supported (same as before) — one login per agent at a time |
| `curl` not in sandbox | It is — verified. Fallback: use `python3 urllib` |
| `ss` not in sandbox | Fallback: parse `/proc/net/tcp6` directly |

### Security

- Auth code passes through Telegram (same as before) — user-initiated, user-visible
- Code submitted via localhost curl inside sandbox — no network exposure
- State parameter validated by Claude CLI's callback server (CSRF protection)
- No credentials file manipulation — Claude CLI handles token storage
