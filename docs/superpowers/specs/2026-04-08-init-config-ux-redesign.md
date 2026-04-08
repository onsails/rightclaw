# Init & Config UX Redesign

## Problem

`rightclaw init` has several UX issues:

1. **Unclear tunnel error messaging.** "Tunnel 'rightclaw' already exists" doesn't say *where* — user doesn't know if it's in `~/.rightclaw`, Cloudflare cloud, or local CF config. Messaging must be explicit.
2. **Dead-end on refusal.** Saying "no" to reusing an existing tunnel quits the wizard entirely. No option to rename, recreate, or skip.
3. **No reconfiguration.** After init, there's no CLI command to change tunnel, Telegram, or agent settings. Users must manually edit YAML files.
4. **Phantom `--force` flag.** Error message says "Use --force to reinitialize" but the flag doesn't exist.
5. **MCP auth doesn't check tunnel state.** `mcp_auth` tool discovers OAuth endpoints without verifying the tunnel can receive callbacks.

## Approach

**Unified Config Layer (Approach B):** Extract all setting-mutation logic into shared functions. `init` creates the directory skeleton then delegates to the same config flows that `rightclaw config` uses. No duplication — adding a new setting means writing it once.

**Interactive prompts:** Replace raw stdin with `inquire` (0.9) for select menus, confirmations, text input, and password prompts.

## Command Structure

### Global config

```
rightclaw config                              # interactive menu: pick setting, see current value, edit
rightclaw config <key>                        # print current value
rightclaw config <key> <value>                # set value directly
```

Global keys: `tunnel.name`, `tunnel.hostname`, `tunnel.credentials-file`

### Agent config

```
rightclaw agent config                        # menu: pick agent -> pick setting -> edit
rightclaw agent config <name>                 # pick setting -> edit (skip agent selection)
rightclaw agent config <name> <key>           # print current value
rightclaw agent config <name> <key> <value>   # set value directly
```

Agent keys: `telegram-token`, `allowed-chat-ids`, `model`, `restart-policy`, `env.*`

### Init (revised)

```
rightclaw init [--telegram-token <T>] [--telegram-allowed-chat-ids <IDs>]
               [--tunnel-name <N>] [--tunnel-hostname <H>] [-y]
```

Same flags. `--force` removed. If already initialized, error says: "Already initialized. Use `rightclaw config` to change settings."

## Init Flow (Revised)

1. Create directory skeleton + write templates (unchanged)
2. Call shared `tunnel_setup()` with inquire prompts
3. Call shared `telegram_setup()` if token provided or interactive
4. Write `config.yaml` + `agent.yaml`

## Tunnel "Already Exists" Flow

When `cloudflared tunnel list` finds a tunnel with the requested name:

```
Found tunnel 'rightclaw' in your Cloudflare account (UUID: e765cc71...).

  > Reuse this tunnel
    Use a different tunnel name
    Delete 'rightclaw' and create a new one
    Skip tunnel setup (warning: MCP OAuth will be unavailable)
```

- **Reuse:** continue with existing UUID
- **Different name:** prompt for new name, create it
- **Delete + recreate:** `cloudflared tunnel cleanup <name>` (close active connections), then `cloudflared tunnel delete <name>`, then `cloudflared tunnel create <name>`
- **Skip:** confirm with warning about implications, then proceed without tunnel

The "skip" menu item shows the warning inline. Selecting it triggers a confirmation:

```
Without a tunnel, MCP server OAuth authentication will not work.
Agents can still run, but external OAuth callbacks can't reach them.

Skip tunnel setup? (y/N)
```

## Tunnel Health Check

### States

| State | Detection | Used by |
|-------|-----------|---------|
| Not configured | `config.yaml` has no `tunnel` section | `mcp_auth`, `doctor` |
| Not running | process-compose API: cloudflared process status != Running | `mcp_auth`, `doctor` |
| Not healthy | HTTP probe to `https://<hostname>` fails (expect 404 from catch-all ingress) | `mcp_auth`, `doctor` |
| Healthy | HTTP probe returns 404 (catch-all) or 200 (routed) | `mcp_auth`, `doctor` |

### Type

```rust
pub enum TunnelState {
    NotConfigured,
    NotRunning,
    Unhealthy { reason: String },
    Healthy,
}

pub async fn check_tunnel(home: &Path, pc_port: u16) -> TunnelState
```

### MCP Auth Guard

`mcp_auth` calls `check_tunnel()` before OAuth discovery. Error messages per state:

- **NotConfigured:** "Tunnel not configured. Run `rightclaw config` to set up. Without a tunnel, OAuth callbacks can't reach this agent."
- **NotRunning:** "Tunnel is configured but cloudflared is not running. Is `rightclaw up` running?"
- **Unhealthy:** "Tunnel is configured and cloudflared is running, but `<hostname>` is not reachable. Check DNS and Cloudflare dashboard."

### Doctor Reuse

`rightclaw doctor` replaces its current tunnel checks with `check_tunnel()` — single source of truth.

## Shared Config Module

### Core library (`crates/rightclaw/`)

Data types and read/write — no UI:

- `config/config.rs`: add `save_global_config(home, &GlobalConfig) -> Result<()>` (currently config.yaml is written inline in `cmd_init`)
- `tunnel/mod.rs`: new module root
- `tunnel/health.rs`: `TunnelState`, `check_tunnel()`

### CLI crate (`crates/rightclaw-cli/`)

Interactive flows using inquire:

- `wizard.rs`: new module containing:
  - `tunnel_setup(home, existing: Option<&TunnelConfig>, interactive: bool) -> Result<Option<TunnelConfig>>`
  - `tunnel_menu_existing(name: &str) -> Result<TunnelAction>` — the 4-option inquire Select menu
  - `telegram_setup(agent_dir, existing: Option<&str>, interactive: bool) -> Result<Option<String>>`
  - `global_setting_menu(home) -> Result<()>` — inquire Select of global settings with current values
  - `agent_setting_menu(agent_dir) -> Result<()>` — inquire Select of agent settings with current values

### Config read-modify-write

Both `config.yaml` and `agent.yaml` use read-modify-write: read current values, present as defaults in inquire prompts, write back only what changed. No full-file regeneration.

## Storage

No schema changes. Same files:

- `~/.rightclaw/config.yaml` — global (tunnel)
- `~/.rightclaw/agents/<name>/agent.yaml` — per-agent (telegram, model, restart, env)

## Dependencies

```toml
# crates/rightclaw-cli/Cargo.toml
inquire = "0.9"
```

Only in CLI crate. Core library stays UI-free.

## File Changes Summary

| File | Change |
|------|--------|
| `crates/rightclaw-cli/src/wizard.rs` | **New** — interactive config flows using inquire |
| `crates/rightclaw-cli/src/main.rs` | Add `Config` and `Agent Config` subcommands, replace stdin prompts with inquire calls to wizard, remove `--force` reference from init error |
| `crates/rightclaw/src/config/config.rs` | Add `save_global_config()` |
| `crates/rightclaw/src/tunnel/mod.rs` | **New** — module root |
| `crates/rightclaw/src/tunnel/health.rs` | **New** — `TunnelState` enum, `check_tunnel()` |
| `crates/rightclaw-cli/src/memory_server.rs` | `mcp_auth` calls `check_tunnel()` before OAuth discovery |
| `crates/rightclaw/src/doctor.rs` | Reuse `check_tunnel()` instead of bespoke tunnel checks |
| `crates/rightclaw/src/init.rs` | Slim down — skeleton + template writing only, remove inline config logic, fix "already initialized" error message |

## Removals

- `--force` flag reference in init error message
- Inline `prompt_yes_no` for tunnel reuse in `main.rs`
- Manual YAML string building for `config.yaml` in `cmd_init`
- Duplicate tunnel checks in `doctor.rs`
