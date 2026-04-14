# Setup-Token Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace broken PTY/callback login with `CLAUDE_CODE_OAUTH_TOKEN` env var approach — user runs `claude setup-token` on their machine, sends token to Telegram bot, bot stores it and passes it to all `claude -p` invocations.

**Architecture:** New SQLite migration adds `auth_tokens` table. On auth error, bot requests token via Telegram. Token stored in memory.db, injected as env var into SSH/bash commands for `claude -p`.

**Tech Stack:** rusqlite (existing), rusqlite_migration (existing), tokio channels (existing `auth_code_tx` mechanism).

---

### Task 1: Add auth_tokens Migration

**Files:**
- Create: `crates/rightclaw/src/memory/sql/v11_auth_tokens.sql`
- Modify: `crates/rightclaw/src/memory/migrations.rs`

- [ ] **Step 1: Create migration SQL file**

Create `crates/rightclaw/src/memory/sql/v11_auth_tokens.sql`:

```sql
CREATE TABLE IF NOT EXISTS auth_tokens (
    token TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- [ ] **Step 2: Register migration**

In `crates/rightclaw/src/memory/migrations.rs`, add after `V10_SCHEMA`:

```rust
const V11_SCHEMA: &str = include_str!("sql/v11_auth_tokens.sql");
```

And add to the `Migrations::new` vec:

```rust
M::up(V11_SCHEMA),
```

- [ ] **Step 3: Run existing migration test**

Run: `devenv shell -- cargo test -p rightclaw --lib memory::migrations`
Expected: PASS — migrations apply cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/memory/sql/v11_auth_tokens.sql crates/rightclaw/src/memory/migrations.rs
git commit -m "feat(memory): add auth_tokens table migration (v11)"
```

---

### Task 2: Add auth_token DB helpers

**Files:**
- Modify: `crates/rightclaw/src/memory/store.rs`

**Context:** `store.rs` contains the `MemoryStore` struct with a `rusqlite::Connection`. Add two methods: `save_auth_token` (DELETE all + INSERT) and `get_auth_token` (SELECT LIMIT 1). These are standalone functions that take `&Connection` — same pattern as other DB helpers in the codebase.

- [ ] **Step 1: Write failing tests**

Add to the test module in `store.rs`:

```rust
#[test]
fn save_and_get_auth_token() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::memory::migrations::MIGRATIONS.to_latest(&mut conn).unwrap();
    save_auth_token(&conn, "test-token-123").unwrap();
    assert_eq!(get_auth_token(&conn).unwrap(), Some("test-token-123".to_string()));
}

#[test]
fn get_auth_token_empty() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::memory::migrations::MIGRATIONS.to_latest(&mut conn).unwrap();
    assert_eq!(get_auth_token(&conn).unwrap(), None);
}

#[test]
fn save_auth_token_replaces_existing() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::memory::migrations::MIGRATIONS.to_latest(&mut conn).unwrap();
    save_auth_token(&conn, "old-token").unwrap();
    save_auth_token(&conn, "new-token").unwrap();
    assert_eq!(get_auth_token(&conn).unwrap(), Some("new-token".to_string()));
    // Only one row
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM auth_tokens", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn delete_auth_token() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::memory::migrations::MIGRATIONS.to_latest(&mut conn).unwrap();
    save_auth_token(&conn, "token").unwrap();
    delete_auth_token(&conn).unwrap();
    assert_eq!(get_auth_token(&conn).unwrap(), None);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `devenv shell -- cargo test -p rightclaw --lib memory::store::tests::save_and_get_auth_token`
Expected: FAIL — functions don't exist.

- [ ] **Step 3: Implement functions**

Add to `store.rs` (as pub functions, not methods on MemoryStore):

```rust
/// Save an auth token, replacing any existing one.
pub fn save_auth_token(conn: &rusqlite::Connection, token: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM auth_tokens", [])?;
    conn.execute(
        "INSERT INTO auth_tokens (token) VALUES (?1)",
        rusqlite::params![token],
    )?;
    Ok(())
}

/// Get the stored auth token, if any.
pub fn get_auth_token(conn: &rusqlite::Connection) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT token FROM auth_tokens LIMIT 1")?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Delete the stored auth token.
pub fn delete_auth_token(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM auth_tokens", [])?;
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `devenv shell -- cargo test -p rightclaw --lib memory::store::tests`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw/src/memory/store.rs
git commit -m "feat(memory): add auth_token CRUD helpers"
```

---

### Task 3: Rewrite login.rs — Token Request Flow

**Files:**
- Rewrite: `crates/bot/src/login.rs`

**Context:** Replace entire file. The new flow is simple: send instruction message, wait for token via channel, save to DB. No SSH, no PTY, no curl, no port discovery.

- [ ] **Step 1: Rewrite login.rs**

Replace entire content of `crates/bot/src/login.rs` with:

```rust
//! Token-based Claude login flow.
//!
//! On auth error, instructs the user to run `claude setup-token` on their
//! machine and send the resulting token via Telegram. The token is stored
//! in memory.db and passed as `CLAUDE_CODE_OAUTH_TOKEN` env var to all
//! subsequent `claude -p` invocations.

use std::path::Path;

use tokio::sync::{mpsc, oneshot};

const TOKEN_INSTRUCTION: &str = "\
To authenticate this agent, run on your machine:
```
claude setup-token
```
Then send me the token it prints.";

/// Events emitted during the token request flow.
#[derive(Debug)]
pub enum LoginEvent {
    /// Login completed — token saved.
    Done,
    /// Login failed.
    Error(String),
}

/// Request an auth token from the user via Telegram and save it.
///
/// Communicates via channels:
/// - `event_tx`: sends `LoginEvent`s to the orchestrator
/// - `token_rx`: receives the token string from the Telegram handler
pub async fn request_token(
    db_path: &Path,
    agent_name: &str,
    event_tx: mpsc::Sender<LoginEvent>,
    token_rx: oneshot::Receiver<String>,
) {
    // Delete stale token if present
    match open_db_and_delete_stale(db_path) {
        Ok(()) => {}
        Err(e) => {
            let _ = event_tx.send(LoginEvent::Error(format!("DB error: {e}"))).await;
            return;
        }
    }

    // Wait for token from Telegram
    tracing::info!(agent = agent_name, "login: waiting for setup-token from user");
    let token = match token_rx.await {
        Ok(t) => t.trim().to_string(),
        Err(_) => {
            let _ = event_tx.send(LoginEvent::Error("token channel closed (timeout?)".into())).await;
            return;
        }
    };

    if token.is_empty() {
        let _ = event_tx.send(LoginEvent::Error("received empty token".into())).await;
        return;
    }

    tracing::info!(agent = agent_name, token_len = token.len(), "login: received token, saving");

    // Save token
    match save_token(db_path, &token) {
        Ok(()) => {
            let _ = event_tx.send(LoginEvent::Done).await;
        }
        Err(e) => {
            let _ = event_tx.send(LoginEvent::Error(format!("failed to save token: {e}"))).await;
        }
    }
}

/// Open DB and delete any existing auth token (it's stale if we're here).
fn open_db_and_delete_stale(db_path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("open db: {e}"))?;
    rightclaw::memory::store::delete_auth_token(&conn)
        .map_err(|e| format!("delete stale token: {e}"))?;
    Ok(())
}

/// Save token to DB.
fn save_token(db_path: &Path, token: &str) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("open db: {e}"))?;
    rightclaw::memory::store::save_auth_token(&conn, token)
        .map_err(|e| format!("save token: {e}"))?;
    Ok(())
}

/// Read the auth token from DB, if any.
pub fn load_auth_token(db_path: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open(db_path).ok()?;
    rightclaw::memory::store::get_auth_token(&conn).ok().flatten()
}

/// Instruction message sent to user when auth is needed.
pub fn auth_instruction_message() -> &'static str {
    TOKEN_INSTRUCTION
}
```

- [ ] **Step 2: Verify build**

Run: `devenv shell -- cargo check -p rightclaw-bot`
Expected: Errors in worker.rs (calls to old functions). That's expected — Task 4 fixes those.

- [ ] **Step 3: Commit**

```bash
git add crates/bot/src/login.rs
git commit -m "feat(login): rewrite as setup-token request flow"
```

---

### Task 4: Update worker.rs — Token-Based Auth

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs`

**Context:** Two changes: (1) replace `spawn_auth_watcher` with `spawn_token_request`, (2) inject `CLAUDE_CODE_OAUTH_TOKEN` env var into `invoke_cc`.

- [ ] **Step 1: Replace spawn_auth_watcher**

Replace the `spawn_auth_watcher` function (lines 635-740) with:

```rust
/// Spawn a background task that requests a setup-token from the user.
///
/// 1. Sends instruction to user via Telegram.
/// 2. Waits for token from Telegram message intercept.
/// 3. Saves token to memory.db.
fn spawn_token_request(
    ctx: &WorkerContext,
    tg_chat_id: teloxide::types::ChatId,
    eff_thread_id: i64,
) {
    let agent_name = ctx.agent_name.clone();
    let bot = ctx.bot.clone();
    let db_path = ctx.db_path.clone();
    let active_flag = Arc::clone(&ctx.auth_watcher_active);
    let auth_code_tx_slot = Arc::clone(&ctx.auth_code_tx);

    tokio::spawn(async move {
        // Send instruction to user
        if let Err(e) = send_tg(
            &bot, tg_chat_id, eff_thread_id,
            crate::login::auth_instruction_message(),
        ).await {
            tracing::warn!(agent = %agent_name, "token request: Telegram send failed: {e:#}");
            active_flag.store(false, Ordering::SeqCst);
            return;
        }

        // Create channel for token from Telegram
        let (token_tx, token_rx) = tokio::sync::oneshot::channel::<String>();
        auth_code_tx_slot.lock().await.replace(token_tx);

        // Create event channel
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<crate::login::LoginEvent>(4);

        // Spawn token request task
        let agent_for_login = agent_name.clone();
        tokio::spawn(async move {
            crate::login::request_token(&db_path, &agent_for_login, event_tx, token_rx).await;
        });

        // Process events with timeout
        let timeout = tokio::time::sleep(Duration::from_secs(300));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Some(crate::login::LoginEvent::Done) => {
                            if let Err(e) = send_tg(
                                &bot, tg_chat_id, eff_thread_id,
                                "Token saved. You can continue chatting.",
                            ).await {
                                tracing::warn!(agent = %agent_name, "token request: Telegram send failed: {e:#}");
                            }
                            break;
                        }
                        Some(crate::login::LoginEvent::Error(msg)) => {
                            tracing::error!(agent = %agent_name, "token request: {msg}");
                            if let Err(e) = send_tg(
                                &bot, tg_chat_id, eff_thread_id,
                                &format!("Token setup failed: {msg}"),
                            ).await {
                                tracing::warn!(agent = %agent_name, "token request: Telegram send failed: {e:#}");
                            }
                            break;
                        }
                        None => {
                            tracing::info!(agent = %agent_name, "token request: task exited");
                            break;
                        }
                    }
                }
                _ = &mut timeout => {
                    tracing::warn!(agent = %agent_name, "token request: timed out after 5 min");
                    if let Err(e) = send_tg(
                        &bot, tg_chat_id, eff_thread_id,
                        "Token request timed out after 5 minutes. Send another message to retry.",
                    ).await {
                        tracing::warn!(agent = %agent_name, "token request: Telegram send failed: {e:#}");
                    }
                    break;
                }
            }
        }

        // Cleanup
        auth_code_tx_slot.lock().await.take();
        active_flag.store(false, Ordering::SeqCst);
    });
}
```

- [ ] **Step 2: Update auth error handler call site**

In the `is_auth_error` block (around line 1219), replace:
```rust
spawn_auth_watcher(ctx, tg_chat_id, ctx.effective_thread_id);
```
with:
```rust
spawn_token_request(ctx, tg_chat_id, ctx.effective_thread_id);
```

And change the notification message (around line 1213) from:
```rust
"Claude needs to log in. A login link will be sent shortly...",
```
to:
```rust
"Claude needs authentication. Setup instructions incoming...",
```

- [ ] **Step 3: Inject CLAUDE_CODE_OAUTH_TOKEN into invoke_cc**

In `invoke_cc`, right after the `let mut cmd = if let Some(ref ssh_config) = ...` block that builds the command (around line 875-916), before `cmd.stdin(Stdio::piped())` (line 917), add:

```rust
    // Inject auth token if available
    if let Some(token) = crate::login::load_auth_token(&ctx.db_path) {
        if ctx.ssh_config_path.is_some() {
            // Sandbox: cannot use cmd.env() — SSH doesn't forward env vars.
            // Prepend env var export to the assembly script passed as SSH arg.
            // The assembly script is the last arg; we need to wrap it.
            // Actually, SSH concatenates all args after -- into one shell command,
            // so we prepend the env export.
            // Re-read the last arg and prepend.
        } else {
            cmd.env("CLAUDE_CODE_OAUTH_TOKEN", &token);
        }
    }
```

Wait — for sandbox mode, we need to inject the env var differently. The assembly script is passed as a single arg to SSH. The simplest approach: prepend `export CLAUDE_CODE_OAUTH_TOKEN='...' && ` to the assembly script string BEFORE passing it to SSH.

Update the sandbox branch (around line 879-893):

```rust
    let mut cmd = if let Some(ref ssh_config) = ctx.ssh_config_path {
        let ssh_host = rightclaw::openshell::ssh_host(&ctx.agent_name);
        let mut assembly_script = build_prompt_assembly_script(
            &base_prompt,
            bootstrap_mode,
            "/sandbox",
            "/tmp/rightclaw-system-prompt.md",
            "/sandbox",
            &claude_args,
            mcp_instructions.as_deref(),
        );
        // Inject auth token as env var in the remote shell
        if let Some(token) = crate::login::load_auth_token(&ctx.db_path) {
            let escaped_token = token.replace('\'', "'\\''");
            assembly_script = format!("export CLAUDE_CODE_OAUTH_TOKEN='{escaped_token}'\n{assembly_script}");
        }
        let mut c = tokio::process::Command::new("ssh");
        c.arg("-F").arg(ssh_config);
        c.arg(&ssh_host);
        c.arg("--");
        c.arg(assembly_script);
        c
    } else {
        // ... no-sandbox branch unchanged, but add env injection:
        let agent_dir_str = ctx.agent_dir.to_string_lossy();
        let prompt_path = ctx.agent_dir.join(".claude").join("composite-system-prompt.md");
        let prompt_path_str = prompt_path.to_string_lossy();
        let assembly_script = build_prompt_assembly_script(
            &base_prompt,
            bootstrap_mode,
            &agent_dir_str,
            &prompt_path_str,
            &agent_dir_str,
            &claude_args,
            mcp_instructions.as_deref(),
        );

        let mut c = tokio::process::Command::new("bash");
        c.arg("-c");
        c.arg(&assembly_script);
        c.env("HOME", &ctx.agent_dir);
        c.env("USE_BUILTIN_RIPGREP", "0");
        if let Some(token) = crate::login::load_auth_token(&ctx.db_path) {
            c.env("CLAUDE_CODE_OAUTH_TOKEN", &token);
        }
        c.current_dir(&ctx.agent_dir);
        c
    };
```

- [ ] **Step 4: Remove old LoginEvent variants**

Remove unused imports. The old `LoginEvent::Url` and `LoginEvent::WaitingForCode` are gone. Update any remaining references — the event loop in `spawn_token_request` only handles `Done` and `Error`.

- [ ] **Step 5: Verify build**

Run: `devenv shell -- cargo build --workspace`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat(worker): use setup-token flow, inject CLAUDE_CODE_OAUTH_TOKEN"
```

---

### Task 5: Clean Up — Remove Dead Code and Dependencies

**Files:**
- Modify: `crates/bot/Cargo.toml` (remove `urlencoding`)
- Modify: `crates/rightclaw/src/codegen/policy.rs` (remove `/dev/tty`, `/dev/pts`)
- Delete: `crates/bot/tests/claude_auth_login.rs` (tests old flow)

- [ ] **Step 1: Remove urlencoding from Cargo.toml**

Remove `urlencoding = { workspace = true }` from `crates/bot/Cargo.toml`.
Remove `urlencoding = "2.1"` from root `Cargo.toml` workspace dependencies (if no other crate uses it).

- [ ] **Step 2: Remove /dev/tty and /dev/pts from policy**

In `crates/rightclaw/src/codegen/policy.rs`, remove these lines from the `read_write` section:
```
    - /dev/tty
    - /dev/pts
```

Remove the `policy_allows_dev_tty_and_pts` test if it exists.

- [ ] **Step 3: Delete old integration test**

```bash
rm crates/bot/tests/claude_auth_login.rs
```

- [ ] **Step 4: Verify build and tests**

Run: `devenv shell -- cargo build --workspace && cargo test --workspace`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: remove PTY login dead code, urlencoding dep, /dev/tty policy"
```

---

### Task 6: Build and Smoke Test

**Files:** None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `devenv shell -- cargo build --workspace`

- [ ] **Step 2: Full test suite**

Run: `devenv shell -- cargo test --workspace`

- [ ] **Step 3: Verify no dead code**

Run: `rg "expectrl\|run_login_pty\|callback_port\|parse_listen_ports\|PTY_HELPER" crates/`
Expected: no matches in source code (only docs/specs).

- [ ] **Step 4: Commit any fixups**
