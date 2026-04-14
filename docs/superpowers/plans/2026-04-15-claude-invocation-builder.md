# Claude Invocation Builder

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate duplicated `claude -p` argument assembly across worker, cron, and cron_delivery by extracting a shared `ClaudeInvocation` builder. Fix the bug where cron/delivery sessions lack `--mcp-config`.

**Architecture:** A `ClaudeInvocation` struct with required fields for invariant flags (`--mcp-config`, `--dangerously-skip-permissions`, `--verbose`) and optional fields for context-specific flags (`--model`, `--max-budget-usd`, `--resume`, `--disallowedTools`). All three callsites construct a `ClaudeInvocation` and call `.into_args()`. The prompt assembly function (`build_prompt_assembly_script`) already takes `&[String]` — no changes needed there.

**Tech Stack:** Rust, existing crate structure (rightclaw-bot)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/bot/src/telegram/invocation.rs` | Create | `ClaudeInvocation` struct + `into_args()` |
| `crates/bot/src/telegram/mod.rs` | Modify | Add `pub(crate) mod invocation;` |
| `crates/bot/src/telegram/worker.rs` | Modify | Replace raw `claude_args` with `ClaudeInvocation` |
| `crates/bot/src/cron.rs` | Modify | Replace raw `claude_args` with `ClaudeInvocation` (fixes missing `--mcp-config`) |
| `crates/bot/src/cron_delivery.rs` | Modify | Replace raw `claude_args` with `ClaudeInvocation` (fixes missing `--mcp-config`) |
| `ARCHITECTURE.md` | Modify | Add "Claude Invocation Contract" section |

## Invariant vs Optional Args (Reference)

Every `claude -p` invocation MUST have:
- `claude -p --dangerously-skip-permissions`
- `--mcp-config <path>` + `--strict-mcp-config`
- `--output-format stream-json` (with `--verbose`) OR `--output-format json` (without `--verbose`)
- `--json-schema <schema>`

**`--verbose` is coupled to `stream-json`:** it switches CC to NDJSON streaming.
With `--output-format json`, adding `--verbose` breaks single-blob stdout parsing (delivery).

Optional per-callsite:
- `--model` (cron: from spec, worker: from agent config, delivery: hardcoded haiku)
- `--max-budget-usd` (cron only)
- `--max-turns` (future use)
- `--resume` / `--session-id` (worker session management, delivery resume)
- `--disallowedTools` (worker: yes, cron: yes, delivery: no — relay task)

---

### Task 1: Create `ClaudeInvocation` struct with tests

**Files:**
- Create: `crates/bot/src/telegram/invocation.rs`

- [ ] **Step 1: Write failing tests for `ClaudeInvocation::into_args()`**

```rust
//! Claude CLI invocation builder.
//!
//! Every `claude -p` call MUST go through `ClaudeInvocation` to guarantee
//! invariant flags (--mcp-config, --dangerously-skip-permissions, etc.)
//! are never omitted. See ARCHITECTURE.md "Claude Invocation Contract".

/// Output format for CC invocation.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OutputFormat {
    /// `--output-format stream-json` — line-by-line NDJSON (worker, cron).
    StreamJson,
    /// `--output-format json` — single JSON blob (delivery).
    Json,
}

/// Builder for `claude -p` CLI arguments.
///
/// Required fields enforce the invocation contract at compile time.
/// All callsites (worker, cron, delivery) construct this instead of
/// raw `Vec<String>`.
pub(crate) struct ClaudeInvocation {
    /// Path to mcp.json — sandbox (`/sandbox/mcp.json`) or host.
    pub mcp_config_path: String,
    /// JSON schema string for structured output.
    pub json_schema: String,
    /// Output format.
    pub output_format: OutputFormat,
    /// Model override (e.g. `claude-haiku-4-5-20251001`).
    pub model: Option<String>,
    /// Budget cap for the session.
    pub max_budget_usd: Option<f64>,
    /// Max turns for the session.
    pub max_turns: Option<u32>,
    /// Resume an existing session.
    pub resume_session_id: Option<String>,
    /// Start a new named session.
    pub new_session_id: Option<String>,
    /// CC built-in tools to disable.
    pub disallowed_tools: Vec<String>,
    /// Extra args appended after all standard flags (before `--`).
    pub extra_args: Vec<String>,
    /// The prompt text (passed after `--`). None for stdin-piped invocations.
    pub prompt: Option<String>,
}

impl ClaudeInvocation {
    /// Convert to CLI argument list. First element is always `"claude"`.
    pub fn into_args(self) -> Vec<String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> ClaudeInvocation {
        ClaudeInvocation {
            mcp_config_path: "/sandbox/mcp.json".into(),
            json_schema: r#"{"type":"object"}"#.into(),
            output_format: OutputFormat::StreamJson,
            model: None,
            max_budget_usd: None,
            max_turns: None,
            resume_session_id: None,
            new_session_id: None,
            disallowed_tools: vec![],
            extra_args: vec![],
            prompt: Some("hello".into()),
        }
    }

    #[test]
    fn minimal_invocation_has_invariants() {
        let args = minimal().into_args();
        assert_eq!(args[0], "claude");
        assert_eq!(args[1], "-p");
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--mcp-config".to_string()));
        assert!(args.contains(&"/sandbox/mcp.json".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(args.contains(&"--verbose".to_string()), "stream-json implies --verbose");
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--json-schema".to_string()));
    }

    #[test]
    fn prompt_comes_after_double_dash() {
        let args = minimal().into_args();
        let dash_pos = args.iter().position(|a| a == "--").expect("must have --");
        assert_eq!(args[dash_pos + 1], "hello");
    }

    #[test]
    fn no_prompt_no_double_dash() {
        let mut inv = minimal();
        inv.prompt = None;
        let args = inv.into_args();
        assert!(!args.contains(&"--".to_string()));
    }

    #[test]
    fn optional_model() {
        let mut inv = minimal();
        inv.model = Some("claude-haiku-4-5-20251001".into());
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--model").expect("--model");
        assert_eq!(args[pos + 1], "claude-haiku-4-5-20251001");
    }

    #[test]
    fn optional_budget() {
        let mut inv = minimal();
        inv.max_budget_usd = Some(1.5);
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--max-budget-usd").expect("--max-budget-usd");
        assert_eq!(args[pos + 1], "1.50");
    }

    #[test]
    fn optional_max_turns() {
        let mut inv = minimal();
        inv.max_turns = Some(10);
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--max-turns").expect("--max-turns");
        assert_eq!(args[pos + 1], "10");
    }

    #[test]
    fn disallowed_tools_expanded() {
        let mut inv = minimal();
        inv.disallowed_tools = vec!["CronCreate".into(), "CronList".into()];
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--disallowedTools").expect("--disallowedTools");
        assert_eq!(args[pos + 1], "CronCreate");
        assert_eq!(args[pos + 2], "CronList");
    }

    #[test]
    fn resume_session() {
        let mut inv = minimal();
        inv.resume_session_id = Some("abc-123".into());
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--resume").expect("--resume");
        assert_eq!(args[pos + 1], "abc-123");
    }

    #[test]
    fn new_session() {
        let mut inv = minimal();
        inv.new_session_id = Some("def-456".into());
        let args = inv.into_args();
        let pos = args.iter().position(|a| a == "--session-id").expect("--session-id");
        assert_eq!(args[pos + 1], "def-456");
    }

    #[test]
    fn json_output_format() {
        let mut inv = minimal();
        inv.output_format = OutputFormat::Json;
        let args = inv.into_args();
        assert!(args.contains(&"json".to_string()));
        assert!(!args.contains(&"stream-json".to_string()));
        assert!(!args.contains(&"--verbose".to_string()), "json mode must NOT have --verbose");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `devenv shell -- cargo test -p rightclaw-bot invocation --no-run 2>&1 | tail -20`
Expected: compile error — `todo!()` in `into_args()`

- [ ] **Step 3: Implement `into_args()`**

Replace the `todo!()` body with:

```rust
pub fn into_args(self) -> Vec<String> {
    let mut args = vec![
        "claude".into(),
        "-p".into(),
        "--dangerously-skip-permissions".into(),
    ];

    // MCP config — invariant, never omit.
    args.push("--mcp-config".into());
    args.push(self.mcp_config_path);
    args.push("--strict-mcp-config".into());

    // Disallowed tools (before other optional flags).
    if !self.disallowed_tools.is_empty() {
        args.push("--disallowedTools".into());
        args.extend(self.disallowed_tools);
    }

    // Session management.
    if let Some(sid) = self.resume_session_id {
        args.push("--resume".into());
        args.push(sid);
    }
    if let Some(sid) = self.new_session_id {
        args.push("--session-id".into());
        args.push(sid);
    }

    // Optional flags.
    if let Some(model) = self.model {
        args.push("--model".into());
        args.push(model);
    }
    if let Some(budget) = self.max_budget_usd {
        args.push("--max-budget-usd".into());
        args.push(format!("{budget:.2}"));
    }
    if let Some(turns) = self.max_turns {
        args.push("--max-turns".into());
        args.push(turns.to_string());
    }

    // Extra args.
    args.extend(self.extra_args);

    // Output format. --verbose only with stream-json — it switches CC
    // to NDJSON streaming which breaks single-blob JSON consumers (delivery).
    args.push("--output-format".into());
    match self.output_format {
        OutputFormat::StreamJson => {
            args.push("--verbose".into());
            args.push("stream-json".into());
        }
        OutputFormat::Json => {
            args.push("json".into());
        }
    }

    // JSON schema (always present).
    args.push("--json-schema".into());
    args.push(self.json_schema);

    // Prompt after `--`.
    if let Some(prompt) = self.prompt {
        args.push("--".into());
        args.push(prompt);
    }

    args
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `devenv shell -- cargo test -p rightclaw-bot invocation -- --nocapture`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/bot/src/telegram/invocation.rs
git commit -m "feat(bot): add ClaudeInvocation builder for claude -p arg assembly"
```

---

### Task 2: Wire up module and resolve mcp_config_path helper

**Files:**
- Modify: `crates/bot/src/telegram/mod.rs` — add module declaration
- Modify: `crates/bot/src/telegram/invocation.rs` — add `mcp_config_path()` helper

- [ ] **Step 1: Add module declaration to `telegram/mod.rs`**

Add after existing module declarations:

```rust
pub(crate) mod invocation;
```

- [ ] **Step 2: Add `mcp_config_path()` helper to `invocation.rs`**

Add before the struct definition:

```rust
/// Resolve the mcp.json path based on execution mode.
///
/// Sandbox: `/sandbox/mcp.json` (constant from openshell module).
/// No-sandbox: `<agent_dir>/mcp.json` on host filesystem.
pub(crate) fn mcp_config_path(ssh_config_path: Option<&std::path::Path>, agent_dir: &std::path::Path) -> String {
    if ssh_config_path.is_some() {
        rightclaw::openshell::SANDBOX_MCP_JSON_PATH.to_string()
    } else {
        agent_dir.join("mcp.json").to_string_lossy().into_owned()
    }
}
```

- [ ] **Step 3: Add test for the helper**

```rust
#[test]
fn mcp_config_path_sandbox() {
    let path = mcp_config_path(Some(std::path::Path::new("/tmp/ssh")), std::path::Path::new("/home/agent"));
    assert_eq!(path, rightclaw::openshell::SANDBOX_MCP_JSON_PATH);
}

#[test]
fn mcp_config_path_no_sandbox() {
    let path = mcp_config_path(None, std::path::Path::new("/home/agent"));
    assert_eq!(path, "/home/agent/mcp.json");
}
```

- [ ] **Step 4: Run tests**

Run: `devenv shell -- cargo test -p rightclaw-bot invocation -- --nocapture`
Expected: all tests PASS

- [ ] **Step 5: Verify build**

Run: `devenv shell -- cargo check -p rightclaw-bot`
Expected: clean compile

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/mod.rs crates/bot/src/telegram/invocation.rs
git commit -m "feat(bot): add mcp_config_path helper, wire invocation module"
```

---

### Task 3: Migrate worker.rs to ClaudeInvocation

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs:688-747`

- [ ] **Step 1: Replace raw `claude_args` construction in worker.rs**

Replace lines 688–747 (from `let mut claude_args` through `claude_args.push(reply_schema)`) with:

```rust
    // Disallow CC built-in tools that conflict with MCP equivalents.
    let disallowed_tools: Vec<String> = [
        "CronCreate",
        "CronList",
        "CronDelete",
        "TaskCreate",
        "TaskUpdate",
        "TaskList",
        "TaskGet",
        "TaskOutput",
        "TaskStop",
        "EnterPlanMode",
        "ExitPlanMode",
        "RemoteTrigger",
    ].iter().map(|&s| s.into()).collect();

    let schema_filename = if bootstrap_mode {
        "bootstrap-schema.json"
    } else {
        "reply-schema.json"
    };
    let reply_schema_path = ctx.agent_dir.join(".claude").join(schema_filename);
    let reply_schema = std::fs::read_to_string(&reply_schema_path)
        .map_err(|e| format_error_reply(-1, &format!("{schema_filename} read failed: {:#}", e)))?;

    let mcp_path = super::invocation::mcp_config_path(
        ctx.ssh_config_path.as_deref(),
        &ctx.agent_dir,
    );

    let mut invocation = super::invocation::ClaudeInvocation {
        mcp_config_path: mcp_path,
        json_schema: reply_schema,
        output_format: super::invocation::OutputFormat::StreamJson,
        model: ctx.model.clone(),
        max_budget_usd: None,
        max_turns: None,
        resume_session_id: None,
        new_session_id: None,
        disallowed_tools,
        extra_args: vec![],
        prompt: None, // stdin-piped
    };

    // Session management (resume vs new).
    match &cmd_args[..] {
        [flag, sid] if flag == "--resume" => invocation.resume_session_id = Some(sid.clone()),
        [flag, sid] if flag == "--session-id" => invocation.new_session_id = Some(sid.clone()),
        _ => {}
    }

    let claude_args = invocation.into_args();
```

- [ ] **Step 2: Remove old `cmd_args` injection**

Remove lines that push `cmd_args` into `claude_args` (the old `for arg in &cmd_args` loop at line 725-727) — session args are now handled by the invocation builder.

- [ ] **Step 3: Run tests**

Run: `devenv shell -- cargo test -p rightclaw-bot -- --nocapture`
Expected: all tests PASS

- [ ] **Step 4: Run clippy**

Run: `devenv shell -- cargo clippy -p rightclaw-bot`
Expected: no warnings

- [ ] **Step 5: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "refactor(worker): use ClaudeInvocation builder for claude -p args"
```

---

### Task 4: Migrate cron.rs to ClaudeInvocation (fixes MCP bug)

**Files:**
- Modify: `crates/bot/src/cron.rs:170-188`

- [ ] **Step 1: Replace raw `claude_args` in `execute_job()`**

Replace lines 170–188 (from `let mut claude_args` through `claude_args.push(spec.prompt.clone())`) with:

```rust
    // Disallow CC built-in tools — cron jobs must not self-schedule or manage tasks.
    let disallowed_tools: Vec<String> = [
        "CronCreate",
        "CronList",
        "CronDelete",
        "TaskCreate",
        "TaskUpdate",
        "TaskList",
        "TaskGet",
        "TaskOutput",
        "TaskStop",
        "EnterPlanMode",
        "ExitPlanMode",
        "RemoteTrigger",
    ].iter().map(|&s| s.into()).collect();

    let mcp_path = crate::telegram::invocation::mcp_config_path(
        ssh_config_path.as_deref(),
        agent_dir,
    );

    let invocation = crate::telegram::invocation::ClaudeInvocation {
        mcp_config_path: mcp_path,
        json_schema: rightclaw::codegen::CRON_SCHEMA_JSON.into(),
        output_format: crate::telegram::invocation::OutputFormat::StreamJson,
        model: model.map(|s| s.to_owned()),
        max_budget_usd: Some(spec.max_budget_usd),
        max_turns: None,
        resume_session_id: None,
        new_session_id: None,
        disallowed_tools,
        extra_args: vec![],
        prompt: Some(spec.prompt.clone()),
    };

    let claude_args = invocation.into_args();
```

- [ ] **Step 2: Run tests**

Run: `devenv shell -- cargo test -p rightclaw-bot -- --nocapture`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/bot/src/cron.rs
git commit -m "fix(cron): add --mcp-config via ClaudeInvocation builder

Cron jobs were missing --mcp-config and --strict-mcp-config flags,
so claude -p sessions couldn't reach external MCP servers (composio, etc.)."
```

---

### Task 5: Migrate cron_delivery.rs to ClaudeInvocation (fixes MCP bug)

**Files:**
- Modify: `crates/bot/src/cron_delivery.rs:334-353`

- [ ] **Step 1: Replace raw `claude_args` in `deliver_through_session()`**

Replace lines 334–353 (from `let mut claude_args` through the json-schema push) with:

```rust
    let mcp_path = crate::telegram::invocation::mcp_config_path(
        ssh_config_path,
        agent_dir,
    );

    let reply_schema_path = agent_dir.join(".claude").join("reply-schema.json");
    let json_schema = std::fs::read_to_string(&reply_schema_path).unwrap_or_default();

    let invocation = crate::telegram::invocation::ClaudeInvocation {
        mcp_config_path: mcp_path,
        json_schema,
        output_format: crate::telegram::invocation::OutputFormat::Json,
        model: Some(DELIVERY_MODEL.into()),
        max_budget_usd: None,
        max_turns: None,
        resume_session_id: session_id,
        new_session_id: None,
        disallowed_tools: vec![], // delivery is a relay — no tools to disable
        extra_args: vec![],
        prompt: None, // stdin-piped
    };

    let claude_args = invocation.into_args();
```

Note: delivery uses `OutputFormat::Json` (not stream-json) and no `--verbose` is needed. Check if `into_args()` always adds `--verbose` — if so, this is fine for delivery too (verbose is harmless, just adds debug info to stderr).

- [ ] **Step 2: Run tests**

Run: `devenv shell -- cargo test -p rightclaw-bot -- --nocapture`
Expected: all tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/bot/src/cron_delivery.rs
git commit -m "fix(delivery): add --mcp-config via ClaudeInvocation builder

Delivery sessions were also missing MCP config flags."
```

---

### Task 6: Update ARCHITECTURE.md

**Files:**
- Modify: `ARCHITECTURE.md`

- [ ] **Step 1: Add "Claude Invocation Contract" section**

Add after the "Prompting Architecture" section:

```markdown
### Claude Invocation Contract

Every `claude -p` invocation MUST go through `ClaudeInvocation` (defined in
`crates/bot/src/telegram/invocation.rs`). Direct construction of `claude_args`
vectors is forbidden — the builder enforces invariant flags at compile time.

**Invariants** (always present, cannot be omitted):
- `claude -p --dangerously-skip-permissions`
- `--mcp-config <path>` + `--strict-mcp-config` — agents MUST have MCP access
- `--output-format <stream-json|json>` (`--verbose` auto-added for `stream-json` only)
- `--json-schema <schema>` — structured output

**Optional per-callsite:**
- `--model` — override default model
- `--max-budget-usd` — budget cap (cron jobs)
- `--max-turns` — turn limit
- `--resume` / `--session-id` — session management (worker, delivery)
- `--disallowedTools` — disable CC built-ins that conflict with MCP equivalents

**Adding a new `claude -p` callsite:** construct a `ClaudeInvocation`, set fields,
call `.into_args()`, pass result to `build_prompt_assembly_script()`. Never build
args manually.
```

- [ ] **Step 2: Commit**

```bash
git add ARCHITECTURE.md
git commit -m "docs: add Claude Invocation Contract to ARCHITECTURE.md"
```

---

### Task 7: Full build verification

- [ ] **Step 1: Full workspace build**

Run: `devenv shell -- cargo build --workspace`
Expected: clean compile

- [ ] **Step 2: Full test suite**

Run: `devenv shell -- cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 3: Clippy**

Run: `devenv shell -- cargo clippy --workspace`
Expected: no warnings
