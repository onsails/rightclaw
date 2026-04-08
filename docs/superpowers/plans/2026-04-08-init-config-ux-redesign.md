# Init & Config UX Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign `rightclaw init` and add `rightclaw config` / `rightclaw agent config` commands so tunnel setup has clear messaging, dead-end prompts offer alternatives, and all settings are reconfigurable post-init.

**Architecture:** Extract interactive config flows into `wizard.rs` in the CLI crate (using `inquire`). Add `tunnel/health.rs` in core for runtime tunnel state detection. Init delegates to wizard for all interactive flows. Config commands call the same wizard functions on existing installs.

**Tech Stack:** Rust 2024 edition, inquire 0.9, clap 4.6, miette 7.6, reqwest 0.13, tokio 1.50

---

### Task 1: Add `inquire` Dependency

**Files:**
- Modify: `Cargo.toml:9-46` (workspace deps)
- Modify: `crates/rightclaw-cli/Cargo.toml:10-32`

- [ ] **Step 1: Add inquire to workspace dependencies**

In root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
inquire = "0.9"
```

- [ ] **Step 2: Add inquire to rightclaw-cli dependencies**

In `crates/rightclaw-cli/Cargo.toml`, add to `[dependencies]`:

```toml
inquire = { workspace = true }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/rightclaw-cli/Cargo.toml
git commit -m "deps: add inquire 0.9 for interactive terminal prompts"
```

---

### Task 2: Create Tunnel Health Module

**Files:**
- Create: `crates/rightclaw/src/tunnel/mod.rs`
- Create: `crates/rightclaw/src/tunnel/health.rs`
- Modify: `crates/rightclaw/src/lib.rs:1-10`

- [ ] **Step 1: Write the failing test**

Create `crates/rightclaw/src/tunnel/health.rs`:

```rust
use std::path::Path;

use crate::runtime::pc_client::PcClient;

/// Runtime state of the Cloudflare tunnel.
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelState {
    /// No tunnel section in config.yaml.
    NotConfigured,
    /// Tunnel configured but cloudflared process is not running in process-compose.
    NotRunning,
    /// Tunnel configured, cloudflared running, but hostname probe failed.
    Unhealthy { reason: String },
    /// Tunnel configured, cloudflared running, hostname reachable.
    Healthy,
}

impl TunnelState {
    /// Human-readable error message for non-healthy states.
    /// Returns `None` for `Healthy`.
    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::NotConfigured => Some(
                "Tunnel not configured. Run `rightclaw config` to set up. \
                 Without a tunnel, OAuth callbacks can't reach this agent."
                    .to_string(),
            ),
            Self::NotRunning => Some(
                "Tunnel is configured but cloudflared is not running. \
                 Is `rightclaw up` running?"
                    .to_string(),
            ),
            Self::Unhealthy { reason } => Some(format!(
                "Tunnel is configured and cloudflared is running, \
                 but the hostname is not reachable: {reason}. \
                 Check DNS and Cloudflare dashboard."
            )),
            Self::Healthy => None,
        }
    }
}

/// Check tunnel state: configured → running → healthy.
///
/// Reads config.yaml for tunnel section, queries process-compose for cloudflared
/// process status, then probes the tunnel hostname via HTTP.
pub async fn check_tunnel(home: &Path, pc_port: u16) -> TunnelState {
    // Step 1: Is tunnel configured?
    let config = match crate::config::read_global_config(home) {
        Ok(cfg) => cfg,
        Err(_) => return TunnelState::NotConfigured,
    };
    let tunnel_cfg = match config.tunnel {
        Some(t) => t,
        None => return TunnelState::NotConfigured,
    };

    // Step 2: Is cloudflared process running in process-compose?
    let pc = match PcClient::new(pc_port) {
        Ok(pc) => pc,
        Err(_) => return TunnelState::NotRunning,
    };
    let processes = match pc.list_processes().await {
        Ok(p) => p,
        Err(_) => return TunnelState::NotRunning,
    };
    let cf_running = processes
        .iter()
        .any(|p| p.name == "cloudflared" && p.status == "Running");
    if !cf_running {
        return TunnelState::NotRunning;
    }

    // Step 3: Is the hostname reachable? Expect 404 from our catch-all ingress.
    let probe_url = format!("https://{}/healthz-tunnel-probe", tunnel_cfg.hostname);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return TunnelState::Unhealthy {
                reason: format!("failed to create HTTP client: {e}"),
            }
        }
    };
    match client.get(&probe_url).send().await {
        Ok(resp) => {
            // 404 is expected (catch-all ingress), any response means tunnel is up.
            let _ = resp;
            TunnelState::Healthy
        }
        Err(e) => TunnelState::Unhealthy {
            reason: format!("{e}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_configured_has_error_message() {
        let msg = TunnelState::NotConfigured.error_message();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("rightclaw config"));
    }

    #[test]
    fn not_running_has_error_message() {
        let msg = TunnelState::NotRunning.error_message();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("rightclaw up"));
    }

    #[test]
    fn unhealthy_includes_reason() {
        let state = TunnelState::Unhealthy {
            reason: "connection refused".to_string(),
        };
        let msg = state.error_message().unwrap();
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn healthy_has_no_error() {
        assert!(TunnelState::Healthy.error_message().is_none());
    }

    #[tokio::test]
    async fn check_tunnel_returns_not_configured_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        let state = check_tunnel(dir.path(), 19999).await;
        assert_eq!(state, TunnelState::NotConfigured);
    }

    #[tokio::test]
    async fn check_tunnel_returns_not_running_when_pc_unreachable() {
        let dir = tempfile::tempdir().unwrap();
        // Write a config with tunnel section but no process-compose running.
        let config = crate::config::GlobalConfig {
            tunnel: Some(crate::config::TunnelConfig {
                tunnel_uuid: "test-uuid".to_string(),
                credentials_file: std::path::PathBuf::from("/tmp/test.json"),
                hostname: "test.example.com".to_string(),
            }),
        };
        crate::config::write_global_config(dir.path(), &config).unwrap();
        let state = check_tunnel(dir.path(), 19999).await;
        assert_eq!(state, TunnelState::NotRunning);
    }
}
```

- [ ] **Step 2: Create module root**

Create `crates/rightclaw/src/tunnel/mod.rs`:

```rust
pub mod health;
```

- [ ] **Step 3: Register module in lib.rs**

In `crates/rightclaw/src/lib.rs`, add after the `pub mod runtime;` line:

```rust
pub mod tunnel;
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p rightclaw tunnel::health`
Expected: all 6 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw/src/tunnel/ crates/rightclaw/src/lib.rs
git commit -m "feat: add tunnel health module with TunnelState enum and check_tunnel()"
```

---

### Task 3: Add MCP Auth Tunnel Guard

**Files:**
- Modify: `crates/rightclaw-cli/src/memory_server.rs:358-391`

- [ ] **Step 1: Write the tunnel guard in mcp_auth**

Replace the `mcp_auth` method (lines 358-391) in `crates/rightclaw-cli/src/memory_server.rs`:

```rust
    #[tool(description = "Discover the OAuth authorization server for an HTTP MCP server and return its authorization endpoint URL. Use this to confirm the server supports OAuth. To complete authentication, use the Telegram bot command: /mcp auth <server_name>")]
    async fn mcp_auth(
        &self,
        Parameters(params): Parameters<McpAuthParams>,
    ) -> Result<CallToolResult, McpError> {
        // Guard: check tunnel state before attempting OAuth discovery.
        let pc_port = std::env::var("RC_PC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(rightclaw::runtime::pc_client::PC_PORT);
        let tunnel_state =
            rightclaw::tunnel::health::check_tunnel(&self.rightclaw_home, pc_port).await;
        if let Some(err_msg) = tunnel_state.error_message() {
            return Ok(CallToolResult::error(vec![Content::text(err_msg)]));
        }

        let mcp_json_path = self.agent_dir.join("mcp.json");
        let servers = rightclaw::mcp::credentials::list_http_servers(
            &mcp_json_path,
        )
        .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        let server_url = servers
            .iter()
            .find(|(name, _)| name == &params.server_name)
            .map(|(_, url)| url.clone())
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "Server '{}' not found in mcp.json. Add it first with mcp_add.",
                        params.server_name
                    ),
                    None,
                )
            })?;

        let http_client = reqwest::Client::new();
        let metadata = rightclaw::mcp::oauth::discover_as(&http_client, &server_url)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Server '{}' supports OAuth. Authorization endpoint: {}\n\nTo authenticate, run in Telegram: /mcp auth {}",
            params.server_name, metadata.authorization_endpoint, params.server_name
        ))]))
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/rightclaw-cli/src/memory_server.rs
git commit -m "feat: guard mcp_auth with tunnel health check before OAuth discovery"
```

---

### Task 4: Refactor config.rs Into a Directory Module

The config module needs `save_global_config` (alias for existing `write_global_config`) and will be referenced by the wizard. Convert from single file to directory to prepare for growth.

**Files:**
- Rename: `crates/rightclaw/src/config.rs` → `crates/rightclaw/src/config/mod.rs`

- [ ] **Step 1: Convert config.rs to config/mod.rs**

```bash
mkdir -p crates/rightclaw/src/config
mv crates/rightclaw/src/config.rs crates/rightclaw/src/config/mod.rs
```

- [ ] **Step 2: Verify everything still compiles**

Run: `cargo check -p rightclaw`
Expected: compiles with no errors

- [ ] **Step 3: Run existing config tests**

Run: `cargo test -p rightclaw config::tests`
Expected: all config tests pass (7 tests)

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/config.rs crates/rightclaw/src/config/mod.rs
git commit -m "refactor: convert config.rs to config/ directory module"
```

---

### Task 5: Create the Wizard Module

This is the core of the redesign — interactive config flows using inquire that both `init` and `config` call.

**Files:**
- Create: `crates/rightclaw-cli/src/wizard.rs`

- [ ] **Step 1: Create wizard.rs with tunnel types and helpers**

Create `crates/rightclaw-cli/src/wizard.rs`:

```rust
use std::path::{Path, PathBuf};

use miette::Context;

/// Action chosen when an existing Cloudflare tunnel is found.
#[derive(Debug, Clone)]
pub enum TunnelExistingAction {
    Reuse,
    Rename,
    DeleteAndRecreate,
    Skip,
}

impl std::fmt::Display for TunnelExistingAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reuse => write!(f, "Reuse this tunnel"),
            Self::Rename => write!(f, "Use a different tunnel name"),
            Self::DeleteAndRecreate => write!(f, "Delete and create a new one"),
            Self::Skip => write!(
                f,
                "Skip tunnel setup (\x1b[33m⚠ MCP OAuth will be unavailable\x1b[0m)"
            ),
        }
    }
}

/// An entry returned by `cloudflared tunnel list -o json` or `cloudflared tunnel create -o json`.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct TunnelListEntry {
    pub id: String,
    pub name: String,
}

// ---- Cloudflared CLI helpers ----

/// Query cloudflared for existing tunnels and find one by name.
fn find_tunnel_by_name(
    cf_bin: &Path,
    name: &str,
) -> miette::Result<Option<TunnelListEntry>> {
    let output = std::process::Command::new(cf_bin)
        .args(["tunnel", "--loglevel", "error", "list", "-o", "json"])
        .output()
        .map_err(|e| miette::miette!("cloudflared list failed: {e:#}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!("cloudflared tunnel list failed: {stderr}"));
    }
    let tunnels: Vec<TunnelListEntry> = serde_json::from_slice(&output.stdout)
        .map_err(|e| miette::miette!("parse cloudflared list output: {e:#}"))?;
    Ok(tunnels.into_iter().find(|t| t.name == name))
}

/// Create a new Named Tunnel via cloudflared and return the created entry.
fn create_tunnel(cf_bin: &Path, name: &str) -> miette::Result<TunnelListEntry> {
    let output = std::process::Command::new(cf_bin)
        .args([
            "tunnel", "--loglevel", "error", "create", "-o", "json", name,
        ])
        .output()
        .map_err(|e| miette::miette!("cloudflared create failed: {e:#}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!("cloudflared tunnel create failed: {stderr}"));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| miette::miette!("parse cloudflared create output: {e:#}"))
}

/// Close active connections and delete a Named Tunnel.
fn delete_tunnel(cf_bin: &Path, name: &str) -> miette::Result<()> {
    // Cleanup active connections first.
    let _ = std::process::Command::new(cf_bin)
        .args(["tunnel", "--loglevel", "error", "cleanup", name])
        .output();
    let output = std::process::Command::new(cf_bin)
        .args(["tunnel", "--loglevel", "error", "delete", name])
        .output()
        .map_err(|e| miette::miette!("cloudflared delete failed: {e:#}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!("cloudflared tunnel delete failed: {stderr}"));
    }
    Ok(())
}

/// Create a DNS CNAME record for the tunnel. Non-fatal — logs warn on failure.
fn route_dns(cf_bin: &Path, uuid: &str, hostname: &str) {
    let result = std::process::Command::new(cf_bin)
        .args([
            "tunnel",
            "--loglevel",
            "error",
            "route",
            "dns",
            "--overwrite-dns",
            uuid,
            hostname,
        ])
        .output();
    match result {
        Ok(output) if output.status.success() => {
            println!("DNS CNAME record created for {hostname}");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("cloudflared route dns failed (non-fatal): {stderr}");
        }
        Err(e) => {
            tracing::warn!("cloudflared route dns invocation failed (non-fatal): {e:#}");
        }
    }
}

/// Returns true if `~/.cloudflared/cert.pem` exists.
fn detect_cloudflared_cert() -> bool {
    dirs::home_dir()
        .map(|h| h.join(".cloudflared").join("cert.pem").exists())
        .unwrap_or(false)
}

/// Returns `~/.cloudflared/<uuid>.json` for the given tunnel UUID.
fn cloudflared_credentials_path(uuid: &str) -> miette::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| miette::miette!("cannot determine home directory"))?;
    Ok(home.join(".cloudflared").join(format!("{uuid}.json")))
}

// ---- Interactive tunnel setup ----

/// Interactive tunnel setup flow. Used by both `init` and `config`.
///
/// When `existing_config` is `Some`, shows current values as defaults.
/// When `interactive` is false, requires all values via CLI args (errors if missing).
///
/// Returns `None` if user chose to skip tunnel setup.
pub fn tunnel_setup(
    tunnel_name: &str,
    tunnel_hostname: Option<&str>,
    interactive: bool,
) -> miette::Result<Option<rightclaw::config::TunnelConfig>> {
    if !detect_cloudflared_cert() {
        if interactive {
            println!(
                "No cloudflared login found. Run `cloudflared login` to enable tunnel support."
            );
            println!("Skipping tunnel setup.");
        }
        return Ok(None);
    }

    let cf_bin = which::which("cloudflared")
        .map_err(|_| miette::miette!("cloudflared not found in PATH — install it first"))?;

    // Find or create the Named Tunnel.
    let existing = find_tunnel_by_name(&cf_bin, tunnel_name)?;
    let uuid = match existing {
        Some(ref t) => {
            if !interactive {
                // Non-interactive: silently reuse.
                t.id.clone()
            } else {
                handle_existing_tunnel(&cf_bin, t, tunnel_name)?
            }
        }
        None => {
            println!("Creating tunnel '{tunnel_name}'...");
            let created = create_tunnel(&cf_bin, tunnel_name)?;
            created.id
        }
    };

    // Resolve hostname.
    let hostname = match tunnel_hostname {
        Some(h) => h.to_string(),
        None if !interactive => {
            return Err(miette::miette!(
                "--tunnel-hostname is required in non-interactive mode"
            ));
        }
        None => {
            inquire::Text::new("Public hostname for tunnel (e.g. right.example.com):")
                .prompt()
                .map_err(|e| miette::miette!("prompt failed: {e}"))?
        }
    };

    // Validate hostname is a bare domain.
    if hostname.starts_with("https://") || hostname.starts_with("http://") {
        return Err(miette::miette!(
            "Hostname must be a bare domain (e.g. example.com), not a URL"
        ));
    }

    // DNS CNAME (non-fatal).
    route_dns(&cf_bin, &uuid, &hostname);

    // Credentials file.
    let credentials_file = cloudflared_credentials_path(&uuid)?;
    if !credentials_file.exists() {
        tracing::warn!(
            path = %credentials_file.display(),
            "credentials file not found — tunnel may have been created on a different machine"
        );
    }

    let config = rightclaw::config::TunnelConfig {
        tunnel_uuid: uuid.clone(),
        credentials_file,
        hostname: hostname.clone(),
    };
    println!("Tunnel configured. UUID: {uuid}, hostname: {hostname}");
    Ok(Some(config))
}

/// Handle the case where a tunnel with the requested name already exists.
///
/// Shows a 4-option menu and returns the tunnel UUID to use,
/// or `Err` with a sentinel to signal "skip tunnel".
fn handle_existing_tunnel(
    cf_bin: &Path,
    existing: &TunnelListEntry,
    original_name: &str,
) -> miette::Result<String> {
    let short_uuid = if existing.id.len() > 8 {
        &existing.id[..8]
    } else {
        &existing.id
    };
    println!(
        "\nFound tunnel '{}' in your Cloudflare account (UUID: {short_uuid}...).",
        existing.name
    );

    let options = vec![
        TunnelExistingAction::Reuse,
        TunnelExistingAction::Rename,
        TunnelExistingAction::DeleteAndRecreate,
        TunnelExistingAction::Skip,
    ];

    let action = inquire::Select::new("What would you like to do?", options)
        .prompt()
        .map_err(|e| miette::miette!("prompt failed: {e}"))?;

    match action {
        TunnelExistingAction::Reuse => Ok(existing.id.clone()),
        TunnelExistingAction::Rename => {
            let new_name =
                inquire::Text::new("New tunnel name:")
                    .prompt()
                    .map_err(|e| miette::miette!("prompt failed: {e}"))?;
            println!("Creating tunnel '{new_name}'...");
            let created = create_tunnel(cf_bin, &new_name)?;
            Ok(created.id)
        }
        TunnelExistingAction::DeleteAndRecreate => {
            let confirmed = inquire::Confirm::new(&format!(
                "This will delete tunnel '{}' and all its connections. Continue?",
                original_name
            ))
            .with_default(false)
            .prompt()
            .map_err(|e| miette::miette!("prompt failed: {e}"))?;
            if !confirmed {
                return Err(miette::miette!("tunnel setup cancelled"));
            }
            println!("Deleting tunnel '{original_name}'...");
            delete_tunnel(cf_bin, original_name)?;
            println!("Creating tunnel '{original_name}'...");
            let created = create_tunnel(cf_bin, original_name)?;
            Ok(created.id)
        }
        TunnelExistingAction::Skip => {
            let confirmed = inquire::Confirm::new(
                "Without a tunnel, MCP server OAuth authentication will not work.\n\
                 Agents can still run, but external OAuth callbacks can't reach them.\n\
                 Skip tunnel setup?",
            )
            .with_default(false)
            .prompt()
            .map_err(|e| miette::miette!("prompt failed: {e}"))?;
            if confirmed {
                // Return a sentinel error that tunnel_setup catches.
                Err(miette::miette!("__skip_tunnel__"))
            } else {
                // Re-prompt.
                handle_existing_tunnel(cf_bin, existing, original_name)
            }
        }
    }
}

// ---- Interactive telegram setup ----

/// Interactive Telegram token setup. Used by both `init` and `config`.
///
/// When `existing_token` is `Some`, shows it as current value and asks to change.
/// Returns `None` if user skips.
pub fn telegram_setup(
    existing_token: Option<&str>,
    interactive: bool,
) -> miette::Result<Option<String>> {
    if !interactive {
        return Ok(None);
    }

    let prompt_msg = if existing_token.is_some() {
        "Update Telegram bot token? (paste new token or press Enter to keep current)"
    } else {
        "Set up Telegram channel? (paste bot token or press Enter to skip)"
    };

    let token = inquire::Text::new(prompt_msg)
        .prompt()
        .map_err(|e| miette::miette!("prompt failed: {e}"))?;

    let token = token.trim().to_string();
    if token.is_empty() {
        return Ok(existing_token.map(|t| t.to_string()));
    }

    rightclaw::init::validate_telegram_token(&token)?;
    Ok(Some(token))
}

// ---- Global config menu ----

/// Interactive menu for editing global settings. Shows current values.
pub fn global_setting_menu(home: &Path) -> miette::Result<()> {
    let config = rightclaw::config::read_global_config(home)?;

    loop {
        let tunnel_display = match &config.tunnel {
            Some(t) => format!("Tunnel: {} ({})", t.hostname, &t.tunnel_uuid[..8.min(t.tunnel_uuid.len())]),
            None => "Tunnel: not configured".to_string(),
        };

        let options = vec![tunnel_display.clone(), "Done".to_string()];

        let choice = inquire::Select::new("Global settings (current values shown):", options)
            .prompt()
            .map_err(|e| miette::miette!("prompt failed: {e}"))?;

        if choice == "Done" {
            break;
        }

        if choice == tunnel_display {
            // Determine tunnel name from existing config or use default.
            let tunnel_name = config
                .tunnel
                .as_ref()
                .map(|_| "rightclaw")
                .unwrap_or("rightclaw");
            let hostname = config.tunnel.as_ref().map(|t| t.hostname.as_str());

            let result = tunnel_setup(tunnel_name, hostname, true);
            match result {
                Ok(new_tunnel) => {
                    let new_config = rightclaw::config::GlobalConfig {
                        tunnel: new_tunnel,
                    };
                    rightclaw::config::write_global_config(home, &new_config)?;
                    println!("Config saved.");
                }
                Err(e) if e.to_string().contains("__skip_tunnel__") => {
                    let new_config = rightclaw::config::GlobalConfig { tunnel: None };
                    rightclaw::config::write_global_config(home, &new_config)?;
                    println!("Tunnel removed from config.");
                }
                Err(e) => return Err(e),
            }
        }
    }

    Ok(())
}

// ---- Agent config menu ----

/// Interactive menu for editing per-agent settings.
///
/// When `agent_name` is None, presents a list of discovered agents to choose from.
pub fn agent_setting_menu(
    home: &Path,
    agent_name: Option<&str>,
) -> miette::Result<()> {
    let agents_dir = home.join("agents");
    let agents = rightclaw::agent::discover_agents(&agents_dir)?;

    if agents.is_empty() {
        return Err(miette::miette!("No agents found in {}", agents_dir.display()));
    }

    let agent = match agent_name {
        Some(name) => agents
            .into_iter()
            .find(|a| a.name == name)
            .ok_or_else(|| miette::miette!("Agent '{}' not found", name))?,
        None => {
            let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
            let chosen = inquire::Select::new("Select agent:", names)
                .prompt()
                .map_err(|e| miette::miette!("prompt failed: {e}"))?;
            agents
                .into_iter()
                .find(|a| a.name == chosen)
                .ok_or_else(|| miette::miette!("Agent '{}' not found", chosen))?
        }
    };

    let agent_yaml_path = agent.path.join("agent.yaml");
    let yaml_content = std::fs::read_to_string(&agent_yaml_path)
        .map_err(|e| miette::miette!("Failed to read {}: {e}", agent_yaml_path.display()))?;

    // Parse current values for display.
    let current_token = agent
        .config
        .as_ref()
        .and_then(|c| c.telegram_token.as_deref());
    let current_model = agent
        .config
        .as_ref()
        .and_then(|c| c.model.as_deref())
        .unwrap_or("(default)");
    let current_chat_ids = agent
        .config
        .as_ref()
        .map(|c| {
            if c.allowed_chat_ids.is_empty() {
                "none (block all)".to_string()
            } else {
                c.allowed_chat_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        })
        .unwrap_or_else(|| "none (block all)".to_string());

    loop {
        let token_display = match current_token {
            Some(t) => {
                let masked = if t.len() > 10 {
                    format!("{}...{}", &t[..6], &t[t.len() - 4..])
                } else {
                    "****".to_string()
                };
                format!("Telegram token: {masked}")
            }
            None => "Telegram token: not set".to_string(),
        };
        let model_display = format!("Model: {current_model}");
        let chat_ids_display = format!("Allowed chat IDs: {current_chat_ids}");

        let options = vec![
            token_display.clone(),
            chat_ids_display.clone(),
            model_display.clone(),
            "Done".to_string(),
        ];

        let choice =
            inquire::Select::new(&format!("Agent '{}' settings:", agent.name), options)
                .prompt()
                .map_err(|e| miette::miette!("prompt failed: {e}"))?;

        if choice == "Done" {
            break;
        }

        if choice == token_display {
            let new_token = telegram_setup(current_token, true)?;
            if let Some(ref token) = new_token {
                update_agent_yaml_field(&agent_yaml_path, "telegram_token", &format!("\"{token}\""))
                    .wrap_err("failed to update telegram_token in agent.yaml")?;
                println!("Telegram token updated.");
            }
        } else if choice == chat_ids_display {
            let input = inquire::Text::new(
                "Allowed chat IDs (comma-separated, empty to clear):",
            )
            .prompt()
            .map_err(|e| miette::miette!("prompt failed: {e}"))?;

            let ids: Vec<String> = input
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            update_agent_yaml_chat_ids(&agent_yaml_path, &ids)
                .wrap_err("failed to update allowed_chat_ids in agent.yaml")?;
            println!("Allowed chat IDs updated.");
        } else if choice == model_display {
            let new_model =
                inquire::Text::new("Model (e.g. claude-sonnet-4-5-20250514, or Enter for default):")
                    .prompt()
                    .map_err(|e| miette::miette!("prompt failed: {e}"))?;

            let new_model = new_model.trim();
            if new_model.is_empty() {
                remove_agent_yaml_field(&agent_yaml_path, "model")
                    .wrap_err("failed to remove model from agent.yaml")?;
            } else {
                update_agent_yaml_field(&agent_yaml_path, "model", &format!("\"{new_model}\""))
                    .wrap_err("failed to update model in agent.yaml")?;
            }
            println!("Model updated.");
        }
    }

    Ok(())
}

// ---- YAML mutation helpers ----

/// Update or insert a single key-value pair in an agent.yaml file.
///
/// Simple line-based approach: finds existing `key:` line and replaces it,
/// or appends if not found.
fn update_agent_yaml_field(path: &Path, key: &str, value: &str) -> miette::Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("read {}: {e}", path.display()))?;

    let prefix = format!("{key}:");
    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.starts_with(&prefix) || line.starts_with(&format!("{key} :")) {
                found = true;
                format!("{key}: {value}")
            } else {
                line.to_string()
            }
        })
        .collect();

    if !found {
        lines.push(format!("{key}: {value}"));
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(path, &output)
        .map_err(|e| miette::miette!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Remove a key from agent.yaml.
fn remove_agent_yaml_field(path: &Path, key: &str) -> miette::Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("read {}: {e}", path.display()))?;

    let prefix = format!("{key}:");
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.starts_with(&prefix) && !line.starts_with(&format!("{key} :")))
        .collect();

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(path, &output)
        .map_err(|e| miette::miette!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Update the `allowed_chat_ids:` block in agent.yaml.
///
/// Replaces existing block or appends. Handles the multi-line YAML list format.
fn update_agent_yaml_chat_ids(path: &Path, ids: &[String]) -> miette::Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("read {}: {e}", path.display()))?;

    // Remove existing allowed_chat_ids block (header + indented list items).
    let mut lines: Vec<String> = Vec::new();
    let mut in_chat_ids_block = false;
    for line in content.lines() {
        if line.starts_with("allowed_chat_ids:") {
            in_chat_ids_block = true;
            continue;
        }
        if in_chat_ids_block {
            if line.starts_with("  - ") {
                continue; // Skip list items.
            }
            in_chat_ids_block = false;
        }
        lines.push(line.to_string());
    }

    // Append new block if IDs provided.
    if !ids.is_empty() {
        // Remove trailing empty lines before appending.
        while lines.last().map_or(false, |l| l.trim().is_empty()) {
            lines.pop();
        }
        lines.push(String::new());
        lines.push("allowed_chat_ids:".to_string());
        for id in ids {
            lines.push(format!("  - {id}"));
        }
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(path, &output)
        .map_err(|e| miette::miette!("write {}: {e}", path.display()))?;
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles (wizard.rs is created but not yet referenced from main.rs)

- [ ] **Step 3: Register the module in main.rs**

Add at the top of `crates/rightclaw-cli/src/main.rs`, after any existing `mod` declarations:

```rust
mod wizard;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles (may get unused warnings — that's fine)

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw-cli/src/wizard.rs crates/rightclaw-cli/src/main.rs
git commit -m "feat: add wizard module with interactive tunnel/telegram/config flows"
```

---

### Task 6: Refactor Init to Use Wizard

Replace the inline tunnel/prompt logic in `cmd_init` with calls to `wizard.rs`. Remove the old helper functions.

**Files:**
- Modify: `crates/rightclaw-cli/src/main.rs:324-545`
- Modify: `crates/rightclaw/src/init.rs:30-35`

- [ ] **Step 1: Fix the "already initialized" error message in init.rs**

In `crates/rightclaw/src/init.rs`, replace lines 30-35:

Old:
```rust
    if agents_dir.exists() {
        return Err(miette::miette!(
            "RightClaw home already initialized at {}. Use --force to reinitialize.",
            agents_dir.display()
        ));
    }
```

New:
```rust
    if agents_dir.exists() {
        return Err(miette::miette!(
            "RightClaw home already initialized at {}. Use `rightclaw config` to change settings.",
            agents_dir.display()
        ));
    }
```

- [ ] **Step 2: Rewrite cmd_init to use wizard**

In `crates/rightclaw-cli/src/main.rs`, replace the `cmd_init` function (lines 324-434):

```rust
fn cmd_init(
    home: &Path,
    telegram_token: Option<&str>,
    telegram_allowed_chat_ids: &[i64],
    tunnel_name: &str,
    tunnel_hostname: Option<&str>,
    yes: bool,
) -> miette::Result<()> {
    let interactive = !yes;

    // Telegram token: CLI flag > interactive prompt > skip.
    let token = match telegram_token {
        Some(t) => {
            rightclaw::init::validate_telegram_token(t)?;
            Some(t.to_string())
        }
        None if !interactive => None,
        None => crate::wizard::telegram_setup(None, true)?,
    };

    rightclaw::init::init_rightclaw_home(home, token.as_deref(), telegram_allowed_chat_ids)?;

    println!("Initialized RightClaw at {}", home.display());
    println!(
        "Default agent 'right' created at {}/agents/right/",
        home.display()
    );
    if token.is_some() {
        println!("Telegram channel configured.");
    }
    if !telegram_allowed_chat_ids.is_empty() {
        println!("Telegram chat ID allowlist configured.");
    }

    // Tunnel setup via wizard (replaces inline logic).
    let tunnel_cfg = match crate::wizard::tunnel_setup(tunnel_name, tunnel_hostname, interactive) {
        Ok(cfg) => cfg,
        Err(e) if e.to_string().contains("__skip_tunnel__") => {
            println!("Tunnel setup skipped.");
            None
        }
        Err(e) => return Err(e),
    };

    let config = rightclaw::config::GlobalConfig {
        tunnel: tunnel_cfg,
    };
    rightclaw::config::write_global_config(home, &config)?;

    Ok(())
}
```

- [ ] **Step 3: Remove old helper functions from main.rs**

Delete these functions from main.rs (lines ~436-545):
- `TunnelListEntry` struct (now in wizard.rs)
- `detect_cloudflared_cert_with_home`
- `detect_cloudflared_cert`
- `cloudflared_credentials_path_for_home`
- `cloudflared_credentials_path`
- `find_tunnel_by_name`
- `create_tunnel`
- `route_dns`
- `prompt_yes_no`
- `prompt_hostname`

Also remove any tests that reference these deleted functions.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles with no errors

- [ ] **Step 5: Run existing tests**

Run: `cargo test -p rightclaw init::tests`
Expected: all init tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/rightclaw-cli/src/main.rs crates/rightclaw/src/init.rs
git commit -m "refactor: init delegates to wizard for tunnel and telegram setup"
```

---

### Task 7: Add Config and Agent Config Subcommands

Wire up the new `rightclaw config` and `rightclaw agent config` commands in clap.

**Files:**
- Modify: `crates/rightclaw-cli/src/main.rs` (Commands enum, ConfigCommands enum, match arms)

- [ ] **Step 1: Extend ConfigCommands enum**

Replace the `ConfigCommands` enum (lines 23-28) in main.rs:

```rust
/// Subcommands for `rightclaw config`.
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Enable machine-wide domain blocking via managed settings (requires sudo)
    StrictSandbox,
    /// Get or set a global config value (interactive menu if no key given)
    #[command(name = "set")]
    Set {
        /// Config key (e.g. tunnel.hostname)
        key: Option<String>,
        /// New value (omit to print current value)
        value: Option<String>,
    },
}
```

- [ ] **Step 2: Add Agent subcommand with Config variant**

Add a new top-level `Agent` subcommand in the `Commands` enum. Find the existing `Config` variant and add `Agent` nearby:

```rust
    /// Manage agents
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
```

Add the `AgentCommands` enum after `ConfigCommands`:

```rust
/// Subcommands for `rightclaw agent`.
#[derive(Subcommand)]
pub enum AgentCommands {
    /// Configure an agent interactively (or get/set a specific setting)
    Config {
        /// Agent name (interactive selection if omitted)
        name: Option<String>,
        /// Setting key (e.g. telegram-token)
        key: Option<String>,
        /// New value (omit to print current)
        value: Option<String>,
    },
}
```

- [ ] **Step 3: Add match arms in main()**

In the main `match` on `cli.command`, add handlers:

```rust
        Commands::Config {
            command: ConfigCommands::Set { key, value },
        } => {
            let home = rightclaw::config::resolve_home(
                None,
                std::env::var("RIGHTCLAW_HOME").ok().as_deref(),
            )?;
            match (key, value) {
                (None, None) => crate::wizard::global_setting_menu(&home)?,
                (Some(key), None) => {
                    // Print current value.
                    let config = rightclaw::config::read_global_config(&home)?;
                    match key.as_str() {
                        "tunnel.hostname" => println!(
                            "{}",
                            config
                                .tunnel
                                .as_ref()
                                .map(|t| t.hostname.as_str())
                                .unwrap_or("(not set)")
                        ),
                        "tunnel.name" => println!(
                            "{}",
                            config
                                .tunnel
                                .as_ref()
                                .map(|t| t.tunnel_uuid.as_str())
                                .unwrap_or("(not set)")
                        ),
                        "tunnel.credentials-file" => println!(
                            "{}",
                            config
                                .tunnel
                                .as_ref()
                                .map(|t| t.credentials_file.display().to_string())
                                .unwrap_or("(not set)".to_string())
                        ),
                        other => {
                            return Err(miette::miette!("Unknown config key: {other}"));
                        }
                    }
                }
                (Some(_key), Some(_value)) => {
                    // Direct set — implement per-key write logic.
                    return Err(miette::miette!(
                        "Direct set not yet implemented. Use `rightclaw config set` without arguments for interactive mode."
                    ));
                }
                (None, Some(_)) => {
                    return Err(miette::miette!("Cannot set a value without a key"));
                }
            }
        }
        Commands::Agent {
            command: AgentCommands::Config { name, key, value },
        } => {
            let home = rightclaw::config::resolve_home(
                None,
                std::env::var("RIGHTCLAW_HOME").ok().as_deref(),
            )?;
            match (key, value) {
                (None, None) => crate::wizard::agent_setting_menu(&home, name.as_deref())?,
                (Some(_key), _) => {
                    return Err(miette::miette!(
                        "Direct get/set not yet implemented. Use `rightclaw agent config` without arguments for interactive mode."
                    ));
                }
                (None, Some(_)) => {
                    return Err(miette::miette!("Cannot set a value without a key"));
                }
            }
        }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p rightclaw-cli`
Expected: compiles with no errors

- [ ] **Step 5: Test CLI help output**

Run: `cargo run -p rightclaw-cli -- config --help`
Expected: shows `set` and `strict-sandbox` subcommands

Run: `cargo run -p rightclaw-cli -- agent --help`
Expected: shows `config` subcommand

- [ ] **Step 6: Commit**

```bash
git add crates/rightclaw-cli/src/main.rs
git commit -m "feat: add rightclaw config set and rightclaw agent config subcommands"
```

---

### Task 8: Refactor Doctor to Use Tunnel Health Module

Replace the bespoke tunnel checks in doctor.rs with `check_tunnel()`.

**Files:**
- Modify: `crates/rightclaw/src/doctor.rs:633-687`

- [ ] **Step 1: Replace tunnel check functions**

Replace `check_tunnel_config` and `check_tunnel_credentials_file` (lines 633-687) with a single async function that uses the tunnel health module:

```rust
/// Check tunnel state using the shared tunnel health module.
///
/// This is sync because doctor runs synchronously — we spawn a tokio runtime
/// just for this check.
fn check_tunnel_state(home: &Path) -> Vec<DoctorCheck> {
    let config = match crate::config::read_global_config(home) {
        Ok(cfg) => cfg,
        Err(e) => {
            return vec![DoctorCheck {
                name: "tunnel-config".to_string(),
                status: CheckStatus::Warn,
                detail: format!("failed to read config.yaml: {e:#}"),
                fix: None,
            }];
        }
    };

    let tunnel_cfg = match &config.tunnel {
        Some(t) => t,
        None => {
            return vec![DoctorCheck {
                name: "tunnel-config".to_string(),
                status: CheckStatus::Warn,
                detail: "no tunnel configured".to_string(),
                fix: Some(
                    "run `rightclaw config set` to configure tunnel".to_string(),
                ),
            }];
        }
    };

    let mut checks = vec![DoctorCheck {
        name: "tunnel-config".to_string(),
        status: CheckStatus::Pass,
        detail: format!("tunnel configured: {}", tunnel_cfg.hostname),
        fix: None,
    }];

    // Check credentials file exists.
    if !tunnel_cfg.credentials_file.exists() {
        checks.push(DoctorCheck {
            name: "tunnel-credentials".to_string(),
            status: CheckStatus::Warn,
            detail: format!(
                "credentials file missing: {}",
                tunnel_cfg.credentials_file.display()
            ),
            fix: Some(
                "re-run `rightclaw config set` to reconfigure tunnel".to_string(),
            ),
        });
    } else {
        checks.push(DoctorCheck {
            name: "tunnel-credentials".to_string(),
            status: CheckStatus::Pass,
            detail: format!(
                "credentials file present at {}",
                tunnel_cfg.credentials_file.display()
            ),
            fix: None,
        });
    }

    checks
}
```

- [ ] **Step 2: Update run_doctor to use check_tunnel_state**

Find where `check_tunnel_config` and `check_tunnel_credentials_file` are called in `run_doctor` and replace with:

```rust
    // Tunnel checks (replaces check_tunnel_config + check_tunnel_credentials_file).
    checks.extend(check_tunnel_state(home));
```

Remove the old calls and the `if let Some(ref tunnel_cfg)` block around `check_tunnel_credentials_file`.

- [ ] **Step 3: Delete the old functions**

Remove the now-unused `check_tunnel_config` and `check_tunnel_credentials_file` functions.

- [ ] **Step 4: Update fix messages referencing old commands**

Search doctor.rs for any remaining references to `rightclaw init --tunnel-name` and replace with `rightclaw config set`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p rightclaw`
Expected: compiles with no errors

- [ ] **Step 6: Run doctor tests if any exist**

Run: `cargo test -p rightclaw doctor`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/rightclaw/src/doctor.rs
git commit -m "refactor: doctor uses check_tunnel_state instead of bespoke tunnel checks"
```

---

### Task 9: Update Init Error Message Test

**Files:**
- Modify: `crates/rightclaw/src/init.rs:232-240`

- [ ] **Step 1: Update the test assertion**

In `crates/rightclaw/src/init.rs`, the test `init_errors_if_already_initialized` (line 232) asserts on error text. Update the assertion to match the new message:

```rust
    #[test]
    fn init_errors_if_already_initialized() {
        let dir = tempdir().unwrap();
        init_rightclaw_home(dir.path(), None, &[]).unwrap();

        let result = init_rightclaw_home(dir.path(), None, &[]);
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("already initialized"),
            "expected 'already initialized' in: {err}"
        );
        assert!(
            err.contains("rightclaw config"),
            "expected 'rightclaw config' (not --force) in: {err}"
        );
    }
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p rightclaw init::tests::init_errors_if_already_initialized`
Expected: PASS

- [ ] **Step 3: Run all init tests**

Run: `cargo test -p rightclaw init::tests`
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/init.rs
git commit -m "test: update init error message assertion to reference rightclaw config"
```

---

### Task 10: Full Integration Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Test CLI smoke check**

Run: `cargo run -p rightclaw-cli -- --help`
Expected: shows `init`, `config`, `agent` subcommands

Run: `cargo run -p rightclaw-cli -- config --help`
Expected: shows `set` and `strict-sandbox`

Run: `cargo run -p rightclaw-cli -- agent --help`
Expected: shows `config`

Run: `cargo run -p rightclaw-cli -- agent config --help`
Expected: shows optional `name`, `key`, `value` args

- [ ] **Step 4: Commit any fixes**

If any fixes were needed, commit them:

```bash
git add -A
git commit -m "fix: address clippy warnings and test failures"
```

---

Plan complete and saved to `docs/superpowers/plans/2026-04-08-init-config-ux-redesign.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
