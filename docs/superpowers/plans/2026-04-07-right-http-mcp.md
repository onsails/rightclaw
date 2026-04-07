# Right HTTP MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch the rightmemory MCP server to HTTP transport (renamed "right"), with per-agent secrets, HMAC-derived Bearer tokens, and a shared server process in process-compose.

**Architecture:** One HTTP MCP server process in process-compose serves all agents. Each agent has a persistent secret in `agent.yaml`. Bearer tokens are derived via `HMAC-SHA256(secret, "right-mcp")`. Sandbox agents reach the host server via `http://host.docker.internal:8100/mcp` through the OpenShell proxy. `--no-sandbox` agents use `http://127.0.0.1:8100/mcp`. All occurrences of "rightmemory" are renamed to "right".

**Tech Stack:** Rust (edition 2024), hmac + sha2 crates, base64, rand, serde_json, minijinja, axum, rmcp

**Spec:** `docs/superpowers/specs/2026-04-07-right-http-mcp-design.md`

---

### Task 1: Add `hmac` dependency and token derivation function

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/rightclaw/Cargo.toml`
- Modify: `crates/rightclaw/src/mcp/mod.rs`

- [ ] **Step 1: Add `hmac` to workspace dependencies**

In the root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
hmac = "0.12"
```

In `crates/rightclaw/Cargo.toml`, add to `[dependencies]`:

```toml
hmac = { workspace = true }
```

(`sha2` and `base64` are already workspace deps used by rightclaw.)

- [ ] **Step 2: Add `generate_agent_secret()` and `derive_token()` to `mcp/mod.rs`**

Replace the full content of `crates/rightclaw/src/mcp/mod.rs`:

```rust
pub mod credentials;
pub mod detect;
pub mod oauth;
pub mod refresh;

/// Name of the built-in MCP server that rightclaw manages.
/// Protected from `/mcp remove` — required for core functionality.
pub const PROTECTED_MCP_SERVER: &str = "right";

/// Generate a random 32-byte agent secret, base64url-encoded (no padding).
///
/// Stored persistently in `agent.yaml`. Used to derive Bearer tokens for
/// the HTTP MCP server and future services.
pub fn generate_agent_secret() -> String {
    use base64::Engine as _;
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Derive a Bearer token from an agent secret using HMAC-SHA256.
///
/// `secret_b64` is the base64url-encoded agent secret from `agent.yaml`.
/// `label` identifies the service (e.g., `"right-mcp"`).
///
/// Returns base64url-encoded HMAC digest (no padding), 43 characters.
pub fn derive_token(secret_b64: &str, label: &str) -> miette::Result<String> {
    use base64::Engine as _;
    use hmac::{Hmac, Mac as _};
    use sha2::Sha256;

    let secret_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(secret_b64)
        .map_err(|e| miette::miette!("invalid agent secret (bad base64url): {e:#}"))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes)
        .map_err(|e| miette::miette!("HMAC init failed: {e:#}"))?;
    mac.update(label.as_bytes());
    let result = mac.finalize().into_bytes();

    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_agent_secret_is_43_chars() {
        let secret = generate_agent_secret();
        assert_eq!(secret.len(), 43);
    }

    #[test]
    fn generate_agent_secret_unique() {
        let a = generate_agent_secret();
        let b = generate_agent_secret();
        assert_ne!(a, b);
    }

    #[test]
    fn derive_token_deterministic() {
        let secret = generate_agent_secret();
        let t1 = derive_token(&secret, "right-mcp").unwrap();
        let t2 = derive_token(&secret, "right-mcp").unwrap();
        assert_eq!(t1, t2);
    }

    #[test]
    fn derive_token_different_labels_differ() {
        let secret = generate_agent_secret();
        let t1 = derive_token(&secret, "right-mcp").unwrap();
        let t2 = derive_token(&secret, "right-cron").unwrap();
        assert_ne!(t1, t2);
    }

    #[test]
    fn derive_token_is_43_chars() {
        let secret = generate_agent_secret();
        let token = derive_token(&secret, "right-mcp").unwrap();
        assert_eq!(token.len(), 43);
    }

    #[test]
    fn derive_token_rejects_invalid_base64() {
        let result = derive_token("not!valid!base64", "right-mcp");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Build workspace**

Run: `cargo build --workspace`
Expected: clean build.

- [ ] **Step 4: Run tests**

Run: `cargo test -p rightclaw -- mcp::tests`
Expected: all 6 new tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/rightclaw/Cargo.toml crates/rightclaw/src/mcp/mod.rs
git commit -m "feat: agent secret generation + HMAC token derivation

Add generate_agent_secret() for persistent per-agent secrets and
derive_token() for HMAC-SHA256 Bearer token derivation.
Rename PROTECTED_MCP_SERVER from 'rightmemory' to 'right'."
```

---

### Task 2: Add `secret` field to `AgentConfig` and auto-generate on `rightclaw up`

**Files:**
- Modify: `crates/rightclaw/src/agent/types.rs`
- Modify: `templates/right/agent.yaml`
- Modify: `crates/rightclaw-cli/src/main.rs` (the `rightclaw up` command, ~lines 806-890)

- [ ] **Step 1: Add `secret` field to `AgentConfig`**

In `crates/rightclaw/src/agent/types.rs`, add to the `AgentConfig` struct after the `env` field (line 81):

```rust
    /// Persistent per-agent secret for deriving Bearer tokens.
    /// Base64url-encoded, 43 characters. Auto-generated if absent.
    #[serde(default)]
    pub secret: Option<String>,
```

- [ ] **Step 2: Add `secret` to the default agent.yaml template**

Append to `templates/right/agent.yaml`:

```yaml

# Per-agent secret for Bearer token derivation.
# Auto-generated by rightclaw. Do not edit.
# secret: <auto-generated on first rightclaw up>
```

(Commented out — `rightclaw up` will generate and write the actual value.)

- [ ] **Step 3: Add secret auto-generation to `rightclaw up`**

In `crates/rightclaw-cli/src/main.rs`, after step 10 (memory.db init, ~line 885) and before step 11 (mcp.json generation, ~line 887), add:

```rust
        // 11. Ensure agent has a persistent secret for token derivation.
        let agent_secret = if let Some(ref secret) = agent.config.as_ref().and_then(|c| c.secret.clone()) {
            secret.clone()
        } else {
            let new_secret = rightclaw::mcp::generate_agent_secret();
            // Append secret to agent.yaml
            let yaml_path = agent.path.join("agent.yaml");
            let mut yaml_content = std::fs::read_to_string(&yaml_path)
                .map_err(|e| miette::miette!("failed to read agent.yaml for '{}': {e:#}", agent.name))?;
            yaml_content.push_str(&format!("\nsecret: \"{new_secret}\"\n"));
            std::fs::write(&yaml_path, &yaml_content)
                .map_err(|e| miette::miette!("failed to write agent secret for '{}': {e:#}", agent.name))?;
            tracing::info!(agent = %agent.name, "generated new agent secret");
            new_secret
        };
```

Then change the existing step 11 (mcp.json generation, ~line 887-889) from:

```rust
        // 11. Generate mcp.json with rightmemory MCP server entry (Phase 17, SKILL-05).
        rightclaw::codegen::generate_mcp_config(&agent.path, &self_exe, &agent.name, home, chrome_cfg)?;
        tracing::debug!(agent = %agent.name, "wrote mcp.json with rightmemory entry");
```

To:

```rust
        // 12. Generate mcp.json with right HTTP MCP server entry.
        let bearer_token = rightclaw::mcp::derive_token(&agent_secret, "right-mcp")?;
        let right_mcp_url = if no_sandbox {
            "http://127.0.0.1:8100/mcp".to_string()
        } else {
            "http://host.docker.internal:8100/mcp".to_string()
        };
        rightclaw::codegen::generate_mcp_config_http(
            &agent.path,
            &agent.name,
            &right_mcp_url,
            &bearer_token,
            chrome_cfg,
        )?;
        tracing::debug!(agent = %agent.name, "wrote mcp.json with right HTTP MCP entry");
```

- [ ] **Step 4: Collect token map and write to `run/agent-tokens.json`**

After the per-agent loop ends (~line 890, after the closing `}`), before the policy generation block, add:

```rust
    // Write agent token map for the HTTP MCP server process.
    let mut token_map = serde_json::Map::new();
    for agent in &agents {
        let secret = agent.config.as_ref()
            .and_then(|c| c.secret.clone())
            // Re-read agent.yaml if secret was just generated (not in original config)
            .or_else(|| {
                let yaml_path = agent.path.join("agent.yaml");
                let content = std::fs::read_to_string(&yaml_path).ok()?;
                let config: crate::AgentConfig = serde_saphyr::from_str(&content).ok()?;
                config.secret
            })
            .ok_or_else(|| miette::miette!("agent '{}' has no secret after generation", agent.name))?;
        let token = rightclaw::mcp::derive_token(&secret, "right-mcp")?;
        token_map.insert(agent.name.clone(), serde_json::Value::String(token));
    }
    let token_map_path = run_dir.join("agent-tokens.json");
    std::fs::write(
        &token_map_path,
        serde_json::to_string_pretty(&serde_json::Value::Object(token_map))
            .map_err(|e| miette::miette!("failed to serialize token map: {e:#}"))?,
    )
    .map_err(|e| miette::miette!("failed to write agent-tokens.json: {e:#}"))?;
    tracing::debug!("wrote agent-tokens.json");
```

- [ ] **Step 5: Build workspace**

Run: `cargo build --workspace`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/rightclaw/src/agent/types.rs templates/right/agent.yaml crates/rightclaw-cli/src/main.rs
git commit -m "feat: per-agent secret in agent.yaml + HTTP mcp.json generation

Auto-generate secret on rightclaw up if absent. Derive Bearer tokens
via HMAC-SHA256(secret, 'right-mcp'). Write agent-tokens.json for
the HTTP MCP server. Switch from generate_mcp_config (stdio) to
generate_mcp_config_http (HTTP) for all agents."
```

---

### Task 3: Rename `rightmemory` → `right` in `mcp_config.rs`

**Files:**
- Modify: `crates/rightclaw/src/codegen/mcp_config.rs`

- [ ] **Step 1: Rename all occurrences**

In `crates/rightclaw/src/codegen/mcp_config.rs`, replace all `"rightmemory"` string literals with `"right"` (both in production code and tests). Also update doc comments from "rightmemory" to "right".

Key locations:
- Line 5: doc comment `rightmemory` → `right`
- Lines 8-9: doc comments
- Line 47: comment
- Line 49: `"rightmemory".to_string()` → `"right".to_string()`
- Line 86: doc comment
- Line 119: `"rightmemory".to_string()` → `"right".to_string()`
- All test assertions referencing `["mcpServers"]["rightmemory"]` → `["mcpServers"]["right"]`

Use find-and-replace: `"rightmemory"` → `"right"` for string literals, `rightmemory` → `right` in comments (but NOT in function names like `generate_mcp_config`).

- [ ] **Step 2: Also update `codegen/mod.rs` re-export**

In `crates/rightclaw/src/codegen/mod.rs`, add re-export for the HTTP variant (line 15):

```rust
pub use mcp_config::{generate_mcp_config, generate_mcp_config_http};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rightclaw -- codegen::mcp_config`
Expected: all tests pass with "right" instead of "rightmemory".

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/codegen/mcp_config.rs crates/rightclaw/src/codegen/mod.rs
git commit -m "refactor: rename rightmemory → right in mcp_config.rs

Update all JSON server keys, doc comments, and test assertions."
```

---

### Task 4: Update policy with `allowed_ips` and rename

**Files:**
- Modify: `crates/rightclaw/src/codegen/policy.rs`

- [ ] **Step 1: Update `generate_policy()` — rename + add `allowed_ips`**

Replace the `rightmemory:` policy section in the format string (~lines 63-70) with:

```yaml
  right:
    endpoints:
      - host: "host.docker.internal"
        port: {rightmemory_port}
        allowed_ips:
          - "172.16.0.0/12"
        protocol: rest
        access: full
    binaries:
      - path: "**"
```

Also update the doc comment on line 5 from `rightmemory_port` to `right_mcp_port`, and rename the function parameter from `rightmemory_port` to `right_mcp_port`.

- [ ] **Step 2: Update tests**

In the test `generates_policy_with_rightmemory_port` (~line 98):
- Rename test to `generates_policy_with_right_mcp_port`
- Change `assert!(policy.contains("host.docker.internal"))` — keep as-is
- Add `assert!(policy.contains("172.16.0.0/12"))` for `allowed_ips`
- Change `generate_policy(8100, &[])` — no change needed (just parameter name changed internally)

- [ ] **Step 3: Run tests**

Run: `cargo test -p rightclaw -- codegen::policy`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/codegen/policy.rs
git commit -m "fix: add allowed_ips to policy for host.docker.internal access

Rename rightmemory → right in policy section.
Add allowed_ips: 172.16.0.0/12 to bypass OpenShell SSRF protection.
Remove tls: terminate — plain HTTP works."
```

---

### Task 5: Add `right-mcp-server` process to process-compose

**Files:**
- Modify: `templates/process-compose.yaml.j2`
- Modify: `crates/rightclaw/src/codegen/process_compose.rs`
- Modify: `crates/rightclaw/src/codegen/process_compose_tests.rs`

- [ ] **Step 1: Add `right-mcp-server` entry to Jinja2 template**

In `templates/process-compose.yaml.j2`, add BEFORE the `{% for agent in agents %}` block (after line 4):

```yaml
{% if right_mcp_server %}
  right-mcp-server:
    command: "{{ right_mcp_server.exe_path }} memory-server-http --port {{ right_mcp_server.port }} --token-map {{ right_mcp_server.token_map_path }}"
    environment:
      - RC_RIGHTCLAW_HOME={{ right_mcp_server.home_dir }}
    availability:
      restart: "always"
      backoff_seconds: 3
      max_restarts: 10
    shutdown:
      signal: 15
      timeout_seconds: 10
{% endif %}
```

Then in the bot agent block, add a `depends_on` for `right-mcp-server` after `shutdown:` (inside the `{% for agent in agents %}` loop):

```yaml
{% if right_mcp_server %}
    depends_on:
      right-mcp-server:
        condition: process_started
{% endif %}
```

- [ ] **Step 2: Add template context struct and pass it**

In `crates/rightclaw/src/codegen/process_compose.rs`, add a new struct:

```rust
/// Template context for the shared right MCP HTTP server process.
#[derive(Debug, Serialize)]
struct RightMcpServer {
    exe_path: String,
    port: u16,
    token_map_path: String,
    home_dir: String,
}
```

Update the `generate_process_compose` signature to add `token_map_path: Option<&Path>`:

```rust
pub fn generate_process_compose(
    agents: &[AgentDef],
    exe_path: &Path,
    debug: bool,
    no_sandbox: bool,
    run_dir: &Path,
    home: &Path,
    cloudflared_script: Option<&Path>,
    token_map_path: Option<&Path>,
) -> miette::Result<String> {
```

Build the `right_mcp_server` context before the template render:

```rust
    let right_mcp_server: Option<RightMcpServer> = token_map_path.map(|p| RightMcpServer {
        exe_path: exe_path.display().to_string(),
        port: 8100,
        token_map_path: p.display().to_string(),
        home_dir: home.display().to_string(),
    });
```

Add it to the template render context:

```rust
    tmpl.render(context! {
        agents => bot_agents,
        cloudflared => cf_entry,
        pc_port => crate::runtime::pc_client::PC_PORT,
        right_mcp_server => right_mcp_server,
    })
```

- [ ] **Step 3: Update call site in `main.rs`**

In `crates/rightclaw-cli/src/main.rs` (~line 978), update the `generate_process_compose` call to pass the token map path:

```rust
    let pc_config = rightclaw::codegen::generate_process_compose(
        &agents,
        &self_exe,
        debug,
        no_sandbox,
        &run_dir,
        home,
        cloudflared_script_path.as_deref(),
        Some(&token_map_path),
    )?;
```

- [ ] **Step 4: Update tests in `process_compose_tests.rs`**

Update all `generate_process_compose` calls in tests to pass `None` as the last argument (existing tests don't use the MCP server). Add a new test:

```rust
    #[test]
    fn right_mcp_server_process_included_when_token_map_provided() {
        let dir = tempdir().unwrap();
        let token_map = dir.path().join("agent-tokens.json");
        std::fs::write(&token_map, "{}").unwrap();
        let agents = vec![make_agent(dir.path(), "test", Some("tok"))];
        let yaml = generate_process_compose(
            &agents,
            Path::new("/usr/bin/rightclaw"),
            false,
            false,
            dir.path(),
            dir.path(),
            None,
            Some(&token_map),
        )
        .unwrap();
        assert!(yaml.contains("right-mcp-server:"), "must have right-mcp-server process");
        assert!(yaml.contains("memory-server-http"), "must run memory-server-http command");
        assert!(yaml.contains("--port 8100"), "must specify port");
        assert!(yaml.contains("depends_on:"), "bot must depend on mcp server");
    }
```

- [ ] **Step 5: Build and test**

Run: `cargo build --workspace && cargo test -p rightclaw -- codegen::process_compose`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add templates/process-compose.yaml.j2 crates/rightclaw/src/codegen/process_compose.rs crates/rightclaw/src/codegen/process_compose_tests.rs crates/rightclaw-cli/src/main.rs
git commit -m "feat: add right-mcp-server process to process-compose

Shared HTTP MCP server runs as a separate process.
Bot processes depend on it via depends_on condition.
Token map path passed through to the Jinja2 template."
```

---

### Task 6: Add `memory-server-http` CLI subcommand

**Files:**
- Modify: `crates/rightclaw-cli/src/main.rs` (Commands enum + dispatch)
- Modify: `crates/rightclaw-cli/src/memory_server_http.rs` (bind address + token-map file loading)

- [ ] **Step 1: Add `MemoryServerHttp` command variant**

In `crates/rightclaw-cli/src/main.rs`, add to the `Commands` enum (after `MemoryServer`):

```rust
    /// Run HTTP MCP memory server (multi-agent, Bearer token auth)
    MemoryServerHttp {
        /// Port to listen on
        #[arg(long, default_value = "8100")]
        port: u16,
        /// Path to agent-tokens.json (agent name → Bearer token map)
        #[arg(long)]
        token_map: std::path::PathBuf,
    },
```

- [ ] **Step 2: Add dispatch before tracing init**

In `main()`, after the `MemoryServer` dispatch (~line 188-189), add:

```rust
    if let Commands::MemoryServerHttp { port, ref token_map } = cli.command {
        let home = rightclaw::config::resolve_rightclaw_home(cli.home.as_deref())?;
        let agents_dir = home.join("agents");

        // Load token map from file
        let token_map_content = std::fs::read_to_string(token_map)
            .map_err(|e| miette::miette!("failed to read token map {}: {e:#}", token_map.display()))?;
        let raw_map: std::collections::HashMap<String, String> = serde_json::from_str(&token_map_content)
            .map_err(|e| miette::miette!("failed to parse token map: {e:#}"))?;

        let mut agent_map = std::collections::HashMap::new();
        for (name, token) in raw_map {
            let dir = agents_dir.join(&name);
            agent_map.insert(token, memory_server_http::AgentInfo {
                name,
                dir,
            });
        }
        let token_map = std::sync::Arc::new(tokio::sync::RwLock::new(agent_map));

        return memory_server_http::run_memory_server_http(
            port,
            token_map,
            agents_dir,
            home,
        ).await;
    }
```

- [ ] **Step 3: Change bind address from `127.0.0.1` to `0.0.0.0`**

In `crates/rightclaw-cli/src/memory_server_http.rs`, line 481, change:

```rust
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
```

to:

```rust
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
```

And update the error message on line 483 accordingly.

- [ ] **Step 4: Build workspace**

Run: `cargo build --workspace`
Expected: clean build.

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw-cli/src/main.rs crates/rightclaw-cli/src/memory_server_http.rs
git commit -m "feat: add memory-server-http CLI subcommand

Loads agent-tokens.json, builds AgentTokenMap, launches HTTP MCP
server on 0.0.0.0:port. Bind 0.0.0.0 required for sandbox access
via host.docker.internal."
```

---

### Task 7: Rename `rightmemory` → `right` in remaining files

**Files:**
- Modify: `crates/rightclaw-cli/src/memory_server.rs`
- Modify: `crates/rightclaw-cli/src/memory_server_http.rs`
- Modify: `crates/rightclaw-cli/src/memory_server_mcp_tests.rs`
- Modify: `crates/rightclaw-cli/src/memory_server_http_tests.rs`
- Modify: `crates/rightclaw/src/mcp/detect.rs`
- Modify: `crates/rightclaw/src/mcp/credentials.rs`
- Modify: `crates/rightclaw/src/doctor.rs`
- Modify: `crates/rightclaw/src/doctor_tests.rs`
- Modify: `crates/bot/src/sync.rs`
- Modify: `crates/bot/src/lib.rs`
- Modify: `crates/bot/src/telegram/handler.rs`

- [ ] **Step 1: Bulk rename `"rightmemory"` → `"right"` across all files**

Use `rg` to find all remaining occurrences:

```bash
rg 'rightmemory' crates/ --type rust -l
```

For each file, replace:
- String literals `"rightmemory"` → `"right"`
- Comments mentioning `rightmemory` → `right` (where it refers to the MCP server name)
- Test function names containing `rightmemory` → `right` (e.g., `test_mcp_remove_rightmemory_rejected` → `test_mcp_remove_right_rejected`)
- Log messages: `"rightmemory HTTP server listening"` → `"right HTTP MCP server listening"`

Do NOT rename:
- File names (keep `memory_server.rs`, `memory_server_http.rs`)
- Function names like `generate_mcp_config` or `run_memory_server_http`
- The `memory-server` CLI subcommand name

- [ ] **Step 2: Build and test entire workspace**

Run: `cargo build --workspace && cargo test --workspace`
Expected: clean build, all tests pass.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: rename rightmemory → right across codebase

Update all string literals, comments, test names, and log messages
that referred to the MCP server as 'rightmemory'. Server key in
mcp.json is now 'right'. PROTECTED_MCP_SERVER constant is 'right'."
```

---

### Task 8: Update `main.rs` policy generation call site

**Files:**
- Modify: `crates/rightclaw-cli/src/main.rs` (~line 900)

- [ ] **Step 1: Update `generate_policy` call**

The parameter was renamed from `rightmemory_port` to `right_mcp_port` in Task 4. Update the call site in `main.rs` (~line 900) — no code change needed since it's a positional argument, but update the comment:

```rust
            let policy_yaml = rightclaw::codegen::policy::generate_policy(8100, &[]);
```

This is already correct (positional). Just verify it still compiles after Task 4's rename.

- [ ] **Step 2: Remove `self_exe` variable if no longer used**

After Task 2, `self_exe` is no longer passed to `generate_mcp_config()`. Check if it's still used elsewhere (e.g., in the process-compose generation). If only used for process-compose `exe_path`, keep it. Otherwise remove the dead code.

- [ ] **Step 3: Build workspace**

Run: `cargo build --workspace`
Expected: clean build, no warnings about unused variables.

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw-cli/src/main.rs
git commit -m "chore: clean up main.rs after right HTTP MCP migration"
```

---

### Task 9: Integration test on live sandbox

- [ ] **Step 1: Rebuild and restart**

```bash
rightclaw down 2>/dev/null
cargo build --workspace
rightclaw up --agents right
```

- [ ] **Step 2: Verify `agent-tokens.json` was generated**

```bash
cat ~/.rightclaw/run/agent-tokens.json
```

Expected: JSON with `"right": "<43-char-token>"`.

- [ ] **Step 3: Verify `mcp.json` has HTTP entry**

```bash
cat ~/.rightclaw/agents/right/mcp.json
```

Expected: `"right": { "type": "http", "url": "http://host.docker.internal:8100/mcp", ... }`

- [ ] **Step 4: Verify `right-mcp-server` is running in process-compose**

Check the process-compose TUI or:

```bash
curl -s http://127.0.0.1:8100/mcp -H "Authorization: Bearer $(jq -r '.right' ~/.rightclaw/run/agent-tokens.json)" -d '{"jsonrpc":"2.0","method":"tools/list","id":1}'
```

Expected: JSON response listing tools (store, recall, search, etc.).

- [ ] **Step 5: Verify `mcp.json` in sandbox**

```bash
ssh -F ~/.rightclaw/run/ssh/rightclaw-right.ssh-config openshell-rightclaw-right 'cat /sandbox/mcp.json'
```

Expected: `"right": { "type": "http", "url": "http://host.docker.internal:8100/mcp", ... }`

- [ ] **Step 6: Send test message via Telegram**

Send "hi" to the bot. Check logs for:
- No "MCP config file not found" errors
- `right` MCP server tools available (no "No matching deferred tools found")
- Response time under ~6s

- [ ] **Step 7: Verify right MCP tools work from sandbox**

Send a message asking the agent to store a memory. Verify the agent uses the `store` tool from the `right` MCP server (check session JSONL for tool_use with mcp prefix).

- [ ] **Step 8: Commit spec and plan**

```bash
git add docs/superpowers/specs/2026-04-07-right-http-mcp-design.md
git add docs/superpowers/plans/2026-04-07-right-http-mcp.md
git commit -m "docs: right HTTP MCP spec and implementation plan"
```
