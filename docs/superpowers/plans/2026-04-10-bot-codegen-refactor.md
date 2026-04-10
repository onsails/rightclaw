# Bot Codegen Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move per-agent codegen from `rightclaw up` to `rightclaw bot` startup, so config changes via `rightclaw agent config` take effect on bot restart without re-running `up`.

**Architecture:** Extract per-agent codegen into `run_single_agent_codegen()`. Bot calls it at startup before sandbox setup. `up` keeps only orchestrator concerns: agent-tokens, process-compose.yaml, cloudflared, runtime state.

**Tech Stack:** Rust, rightclaw core crate, rightclaw-bot crate

---

### Task 1: Add `discover_single_agent()` to discovery module

**Files:**
- Modify: `crates/rightclaw/src/agent/discovery.rs:66-69`

The bot needs to build an `AgentDef` from a known agent directory. Currently only `discover_agents()` exists (scans parent dir). Add a single-agent variant.

- [ ] **Step 1: Write failing test**

Add to `crates/rightclaw/src/agent/discovery_tests.rs`:

```rust
#[test]
fn discover_single_agent_from_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let agent_dir = dir.path().join("agents").join("test");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(agent_dir.join("agent.yaml"), "restart: never\n").unwrap();
    std::fs::write(agent_dir.join("IDENTITY.md"), "# Test Agent").unwrap();

    let agent = super::discover_single_agent(&agent_dir).unwrap();
    assert_eq!(agent.name, "test");
    assert_eq!(agent.path, agent_dir);
    assert!(agent.identity_path.ends_with("IDENTITY.md"));
    assert!(agent.config.is_some());
}

#[test]
fn discover_single_agent_without_agent_yaml_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let agent_dir = dir.path().join("agents").join("test");
    std::fs::create_dir_all(&agent_dir).unwrap();

    let result = super::discover_single_agent(&agent_dir);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rightclaw discover_single_agent -- --nocapture`
Expected: FAIL — `discover_single_agent` not found

- [ ] **Step 3: Implement `discover_single_agent()`**

Add to `crates/rightclaw/src/agent/discovery.rs` after `optional_file()`:

```rust
/// Build an `AgentDef` from a single known agent directory.
///
/// Unlike `discover_agents()` which scans a parent directory, this takes
/// the agent directory directly. Used by the bot at startup.
pub fn discover_single_agent(agent_dir: &Path) -> miette::Result<AgentDef> {
    let name = agent_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| miette::miette!("agent directory has no valid name: {}", agent_dir.display()))?
        .to_string();

    validate_agent_name(&name)?;

    if !agent_dir.join("agent.yaml").exists() {
        return Err(miette::miette!(
            "agent.yaml not found in {}",
            agent_dir.display()
        ));
    }

    let config = parse_agent_config(agent_dir)?;

    Ok(AgentDef {
        name,
        identity_path: agent_dir.join("IDENTITY.md"),
        config,
        soul_path: optional_file(agent_dir, "SOUL.md"),
        user_path: optional_file(agent_dir, "USER.md"),
        agents_path: optional_file(agent_dir, "AGENTS.md"),
        tools_path: optional_file(agent_dir, "TOOLS.md"),
        bootstrap_path: optional_file(agent_dir, "BOOTSTRAP.md"),
        heartbeat_path: optional_file(agent_dir, "HEARTBEAT.md"),
        path: agent_dir.to_path_buf(),
    })
}
```

Export from `crates/rightclaw/src/agent/mod.rs`:

```rust
pub use discovery::discover_single_agent;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rightclaw discover_single_agent -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw/src/agent/discovery.rs crates/rightclaw/src/agent/discovery_tests.rs crates/rightclaw/src/agent/mod.rs
git commit -m "feat: add discover_single_agent() for bot startup codegen"
```

---

### Task 2: Extract `run_single_agent_codegen()` from pipeline

**Files:**
- Modify: `crates/rightclaw/src/codegen/pipeline.rs:14-274`
- Modify: `crates/rightclaw/src/codegen/mod.rs`

Extract the per-agent loop body (lines 34-273) into a standalone public function. Add policy.yaml generation. The existing `run_agent_codegen()` calls the new function in a loop.

- [ ] **Step 1: Write failing test**

Add to `crates/rightclaw/src/codegen/pipeline.rs` tests:

```rust
#[test]
fn run_single_agent_codegen_generates_all_files() {
    let dir = tempfile::TempDir::new().unwrap();
    let home = dir.path();
    let agent_dir = home.join("agents").join("test");
    std::fs::create_dir_all(agent_dir.join(".claude")).unwrap();
    std::fs::write(agent_dir.join("IDENTITY.md"), "# Test").unwrap();
    std::fs::write(
        agent_dir.join("agent.yaml"),
        "restart: never\nnetwork_policy: permissive\n",
    )
    .unwrap();

    let agent = crate::agent::discover_single_agent(&agent_dir).unwrap();
    let self_exe = std::path::PathBuf::from("/usr/bin/rightclaw");

    run_single_agent_codegen(home, &agent, &self_exe, false).unwrap();

    // Core files must exist
    assert!(agent_dir.join(".claude/settings.json").exists());
    assert!(agent_dir.join(".claude/agents/test.md").exists());
    assert!(agent_dir.join(".claude/agents/test-bootstrap.md").exists());
    assert!(agent_dir.join(".claude/system-prompt.md").exists());
    assert!(agent_dir.join(".claude/reply-schema.json").exists());
    assert!(agent_dir.join(".claude/bootstrap-schema.json").exists());
    assert!(agent_dir.join("TOOLS.md").exists());
    assert!(agent_dir.join("mcp.json").exists());
    assert!(agent_dir.join("memory.db").exists());
    // Policy must be generated
    assert!(agent_dir.join("policy.yaml").exists());
    let policy = std::fs::read_to_string(agent_dir.join("policy.yaml")).unwrap();
    assert!(policy.contains(r#"host: "**.*""#), "permissive policy must allow all HTTPS");
}

#[test]
fn run_single_agent_codegen_restrictive_policy() {
    let dir = tempfile::TempDir::new().unwrap();
    let home = dir.path();
    let agent_dir = home.join("agents").join("test");
    std::fs::create_dir_all(agent_dir.join(".claude")).unwrap();
    std::fs::write(agent_dir.join("IDENTITY.md"), "# Test").unwrap();
    std::fs::write(
        agent_dir.join("agent.yaml"),
        "restart: never\nnetwork_policy: restrictive\n",
    )
    .unwrap();

    let agent = crate::agent::discover_single_agent(&agent_dir).unwrap();
    let self_exe = std::path::PathBuf::from("/usr/bin/rightclaw");

    run_single_agent_codegen(home, &agent, &self_exe, false).unwrap();

    let policy = std::fs::read_to_string(agent_dir.join("policy.yaml")).unwrap();
    assert!(policy.contains(r#"host: "*.anthropic.com""#));
    assert!(!policy.contains(r#"host: "**.*""#));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rightclaw run_single_agent_codegen -- --nocapture`
Expected: FAIL — function not found

- [ ] **Step 3: Extract `run_single_agent_codegen()`**

In `crates/rightclaw/src/codegen/pipeline.rs`, create a new public function containing the per-agent loop body (current lines 34-273) plus policy.yaml generation. The function signature:

```rust
/// Run codegen for a single agent.
///
/// Generates all per-agent artifacts: settings, agent definitions, schemas,
/// .claude.json, mcp.json, TOOLS.md, skills, memory.db, policy.yaml.
/// Called by the bot at startup and by `run_agent_codegen()` during `rightclaw up`.
///
/// Returns the agent secret (existing or newly generated).
pub fn run_single_agent_codegen(
    home: &Path,
    agent: &AgentDef,
    self_exe: &Path,
    debug: bool,
) -> miette::Result<String> {
```

The function body is the current for-loop body (lines 36-248 of pipeline.rs), with these additions at the end (before the mcp.json generation):

```rust
    // Generate policy.yaml from network_policy setting.
    let network_policy = agent
        .config
        .as_ref()
        .map(|c| &c.network_policy)
        .cloned()
        .unwrap_or_default();
    let mcp_port = crate::runtime::MCP_HTTP_PORT;
    let policy_content = crate::codegen::policy::generate_policy(mcp_port, &network_policy);
    std::fs::write(agent.path.join("policy.yaml"), &policy_content).map_err(|e| {
        miette::miette!(
            "failed to write policy.yaml for '{}': {e:#}",
            agent.name
        )
    })?;
    tracing::debug!(agent = %agent.name, %network_policy, "wrote policy.yaml");
```

The function returns the agent_secret string (needed by `run_agent_codegen` for agent-tokens.json).

Then refactor `run_agent_codegen()` to call it in a loop:

```rust
pub fn run_agent_codegen(
    home: &Path,
    agents: &[AgentDef],
    all_agents: &[AgentDef],
    self_exe: &Path,
    debug: bool,
) -> miette::Result<()> {
    let run_dir = home.join("run");
    std::fs::create_dir_all(&run_dir)
        .map_err(|e| miette::miette!("failed to create run directory: {e:#}"))?;

    let global_cfg = crate::config::read_global_config(home)?;

    // Per-agent codegen.
    let mut generated_secrets: HashMap<String, String> = HashMap::new();
    for agent in agents {
        let secret = run_single_agent_codegen(home, agent, self_exe, debug)?;
        generated_secrets.insert(agent.name.clone(), secret);
    }

    // Cross-agent: agent-tokens.json
    // ... (existing lines 276-297, using generated_secrets)

    // Validate policy files for all agents
    // ... (existing lines 299-304)

    // Cloudflared config
    // ... (existing lines 306-372)

    // process-compose.yaml
    // ... (existing lines 374-388)

    // runtime state
    // ... (existing lines 390-413)

    Ok(())
}
```

Export from `crates/rightclaw/src/codegen/mod.rs`:

```rust
pub use pipeline::run_single_agent_codegen;
```

- [ ] **Step 4: Run tests to verify everything passes**

Run: `cargo test -p rightclaw -- --nocapture`
Expected: ALL PASS (both new tests and existing `run_agent_codegen_*` tests)

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw/src/codegen/pipeline.rs crates/rightclaw/src/codegen/mod.rs
git commit -m "refactor: extract run_single_agent_codegen() from pipeline"
```

---

### Task 3: Call codegen from bot startup

**Files:**
- Modify: `crates/bot/src/lib.rs:33-107`

Insert `run_single_agent_codegen()` call in the bot's `run_async()` function, after parsing agent.yaml and before sandbox setup.

- [ ] **Step 1: Add codegen call to bot startup**

In `crates/bot/src/lib.rs`, after the config parsing block (line 89) and before `let is_sandboxed` (line 91), add:

```rust
    // Per-agent codegen: regenerate all derived files from agent.yaml + identity files.
    // This ensures policy.yaml, settings.json, mcp.json, etc. reflect the current config
    // even after a config change triggered restart.
    let self_exe = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("rightclaw"));
    let agent_def = rightclaw::agent::discover_single_agent(&agent_dir)?;
    rightclaw::codegen::run_single_agent_codegen(&home, &agent_def, &self_exe, args.debug)?;
    tracing::info!(agent = %args.agent, "per-agent codegen complete");

    // Re-parse config after codegen (secret may have been generated).
    let config = parse_agent_config(&agent_dir)?.unwrap_or_else(|| {
        rightclaw::agent::types::AgentConfig {
            allowed_chat_ids: vec![],
            telegram_token: None,
            restart: Default::default(),
            max_restarts: 3,
            backoff_seconds: 5,
            model: None,
            sandbox: None,
            env: Default::default(),
            secret: None,
            attachments: Default::default(),
            network_policy: Default::default(),
            max_turns: 30,
            max_budget_usd: 1.0,
            show_thinking: true,
        }
    });
```

- [ ] **Step 2: Build workspace**

Run: `cargo build --workspace`
Expected: SUCCESS

- [ ] **Step 3: Commit**

```bash
git add crates/bot/src/lib.rs
git commit -m "feat: run per-agent codegen at bot startup"
```

---

### Task 4: Remove per-agent codegen from `up`

**Files:**
- Modify: `crates/rightclaw/src/codegen/pipeline.rs`

Now that both `up` and bot call codegen, remove the per-agent codegen from `run_agent_codegen()`. Keep only cross-agent concerns.

- [ ] **Step 1: Slim down `run_agent_codegen()`**

Replace the per-agent loop in `run_agent_codegen()` with only secret resolution (needed for agent-tokens.json). The function becomes:

```rust
pub fn run_agent_codegen(
    home: &Path,
    _agents: &[AgentDef],
    all_agents: &[AgentDef],
    self_exe: &Path,
    debug: bool,
) -> miette::Result<()> {
    let _ = (self_exe, debug); // No longer used for per-agent codegen

    let run_dir = home.join("run");
    std::fs::create_dir_all(&run_dir)
        .map_err(|e| miette::miette!("failed to create run directory: {e:#}"))?;

    let global_cfg = crate::config::read_global_config(home)?;

    // Build agent token map from existing secrets.
    // Secrets are generated by bot startup (run_single_agent_codegen),
    // but on first `up` they may not exist yet — generate if missing.
    let mut token_map_entries = serde_json::Map::new();
    for agent in all_agents {
        let secret = match agent.config.as_ref().and_then(|c| c.secret.clone()) {
            Some(s) => s,
            None => {
                // Generate secret now so agent-tokens.json is complete.
                // Bot will also generate on startup (idempotent).
                let new_secret = crate::mcp::generate_agent_secret();
                let yaml_path = agent.path.join("agent.yaml");
                let yaml_content = std::fs::read_to_string(&yaml_path).map_err(|e| {
                    miette::miette!("failed to read agent.yaml for '{}': {e:#}", agent.name)
                })?;
                let mut doc: serde_json::Map<String, serde_json::Value> =
                    serde_saphyr::from_str(&yaml_content).map_err(|e| {
                        miette::miette!("failed to parse agent.yaml for '{}': {e:#}", agent.name)
                    })?;
                doc.insert("secret".to_owned(), serde_json::Value::String(new_secret.clone()));
                let updated_yaml = serde_saphyr::to_string(&doc).map_err(|e| {
                    miette::miette!("failed to serialize agent.yaml for '{}': {e:#}", agent.name)
                })?;
                std::fs::write(&yaml_path, &updated_yaml).map_err(|e| {
                    miette::miette!("failed to write agent secret for '{}': {e:#}", agent.name)
                })?;
                tracing::info!(agent = %agent.name, "generated agent secret for token map");
                new_secret
            }
        };
        let token = crate::mcp::derive_token(&secret, "right-mcp")?;
        token_map_entries.insert(agent.name.clone(), serde_json::Value::String(token));
    }
    let token_map_path = run_dir.join("agent-tokens.json");
    std::fs::write(
        &token_map_path,
        serde_json::to_string_pretty(&serde_json::Value::Object(token_map_entries))
            .map_err(|e| miette::miette!("failed to serialize token map: {e:#}"))?,
    )
    .map_err(|e| miette::miette!("failed to write agent-tokens.json: {e:#}"))?;
    tracing::debug!("wrote agent-tokens.json");

    // --- Everything below is unchanged: cloudflared, process-compose, runtime state ---
    // (existing lines 306-413)
```

- [ ] **Step 2: Update existing tests**

The `run_agent_codegen_writes_bootstrap_definition` test expects per-agent files to be written by `run_agent_codegen()`. This test should now use `run_single_agent_codegen()` instead. Update:

```rust
#[test]
fn run_agent_codegen_writes_bootstrap_definition() {
    // ... (same setup) ...

    // Use run_single_agent_codegen instead
    let _secret = run_single_agent_codegen(home, &agent, &self_exe, false).unwrap();

    // ... (same assertions) ...
}
```

The `run_agent_codegen_with_empty_agents` test stays — it verifies cross-agent artifacts are generated even with no agents.

- [ ] **Step 3: Run all tests**

Run: `cargo test -p rightclaw -- --nocapture`
Expected: ALL PASS

- [ ] **Step 4: Build full workspace**

Run: `cargo build --workspace`
Expected: SUCCESS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No new warnings (pre-existing `too_many_arguments` on `cmd_init` is known)

- [ ] **Step 6: Commit**

```bash
git add crates/rightclaw/src/codegen/pipeline.rs
git commit -m "refactor: remove per-agent codegen from rightclaw up

Bot now handles all per-agent codegen at startup. up only generates
cross-agent artifacts: agent-tokens.json, process-compose.yaml,
cloudflared config, runtime state."
```

---

### Task 5: Verify end-to-end config change flow

**Files:** None (manual verification)

- [ ] **Step 1: Build release binary**

Run: `cargo build --workspace`

- [ ] **Step 2: Verify rightclaw up still works**

Run: `rightclaw up --detach` (if agents configured)
Check: process-compose starts, bot processes come up, logs show "per-agent codegen complete"

- [ ] **Step 3: Verify config change triggers policy regeneration**

Run: `rightclaw agent config` → change network_policy
Check logs: bot exits with code 2 → restarts → "per-agent codegen complete" → "policy applied" → "wrote policy.yaml"
Check: `~/.rightclaw/agents/right/policy.yaml` reflects new network_policy

- [ ] **Step 4: Commit any fixes**

If any issues found, fix and commit.

---

### Task 6: Update ARCHITECTURE.md

**Files:**
- Modify: `ARCHITECTURE.md`

- [ ] **Step 1: Update the Data Flow section**

Update the `rightclaw up` section to reflect that per-agent codegen is no longer done there. Update the bot startup section to show the new codegen step. Key changes:

In the `rightclaw up` flow:
```
rightclaw up [--agents x,y] [--detach]
  ├─ Discover agents from agents/ directory
  ├─ Generate agent-tokens.json (secrets + bearer tokens)
  ├─ Generate process-compose.yaml (minijinja)
  ├─ Generate cloudflared config (if tunnel)
  └─ Launch process-compose (TUI or detached)
```

In the bot startup flow, add before sandbox setup:
```
rightclaw bot --agent <name>  (spawned by process-compose)
  ├─ Resolve token, open memory.db
  ├─ Per-agent codegen (NEW):
  │   ├─ settings.json, agent defs, system-prompt, schemas
  │   ├─ .claude.json, credentials symlink, mcp.json
  │   ├─ TOOLS.md, skills install, policy.yaml
  │   └─ memory.db init, git init, secret generation
  ├─ Sandbox lifecycle: ...
```

- [ ] **Step 2: Commit**

```bash
git add ARCHITECTURE.md
git commit -m "docs: update architecture for bot-owned codegen"
```
