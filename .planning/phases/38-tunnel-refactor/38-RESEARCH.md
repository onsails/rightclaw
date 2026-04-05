# Phase 38: Tunnel Refactor — Research

**Researched:** 2026-04-05
**Domain:** cloudflared named tunnel management (credentials-file vs --tunnel-token)
**Confidence:** HIGH

## Summary

Phase 38 replaces the current `--tunnel-token` (remote-managed) approach with the proper named tunnel flow (`cloudflared tunnel create` + credentials-file). The current system works but has a fundamental flaw: when cloudflared runs with `--token`, it fetches ingress rules from the Cloudflare dashboard and ignores the local `cloudflared-config.yml`. The codebase already partially solved this in `cmd_up` by detecting an existing credentials file in `~/.cloudflared/` and embedding `tunnel: + credentials-file:` in the generated config — but this is a fragile best-effort workaround. Phase 38 makes credentials-file the primary setup path.

The user already has a valid credentials file at `~/.cloudflared/e765cc71-d0c2-42a3-864b-81566f8817fd.json` (verified via `ls`), which means the migration path is: teach `rightclaw init` to consume that file (by copying it to `~/.rightclaw/tunnel/`) rather than accepting a raw JWT token. The `--tunnel-token` CLI arg and `TunnelConfig.token` field are replaced by `--tunnel-credentials-file` and `TunnelConfig.credentials_file`.

**Primary recommendation:** Replace `--tunnel-token`/`token` with `--tunnel-credentials-file`/`credentials_file: PathBuf` in `TunnelConfig`. Read TunnelID directly from the JSON file (no JWT decode needed). The DNS routing wrapper script and cloudflared config generation already work correctly once a credentials file path is in hand — the planner's job is mainly to swap the input pathway.

## Project Constraints (from CLAUDE.md)

- Rust edition 2024
- Error handling: always propagate with `?`, never swallow. Use `{:#}` for error chain display.
- `thiserror` for library crates, `anyhow`/`miette` for CLI/binary.
- `config.rs` uses manual YAML write (serde-saphyr is deserialize-only) — continue pattern for new fields.
- Workspace architecture — changes span `crates/rightclaw/` (config, codegen, doctor) and `crates/rightclaw-cli/` (main.rs).

## Standard Stack

No new crates needed. All required capabilities are already in the workspace:

| Library | Current Use | Use in Phase 38 |
|---------|-------------|-----------------|
| `serde_json` | Credential file parsing in `detect_cloudflared_credentials` | Parse credentials JSON in `TunnelConfig::from_credentials_file()` |
| `std::fs` | File I/O throughout | Copy credentials file to `~/.rightclaw/tunnel/<uuid>.json` |
| `miette` | Error reporting in config.rs | Same — parse errors with `.map_err(|e| miette::miette!(...))` |
| `dirs` | Resolve `~/.cloudflared/` in `detect_cloudflared_credentials` | Resolve `~/.rightclaw/` |

**No new dependencies required.** [VERIFIED: codebase grep]

## Architecture Patterns

### Current TunnelConfig Structure
```rust
// crates/rightclaw/src/config.rs
pub struct TunnelConfig {
    pub token: String,
    pub hostname: String,
}
```

### Target TunnelConfig Structure
```rust
pub struct TunnelConfig {
    pub tunnel_uuid: String,           // TunnelID from credentials JSON
    pub credentials_file: PathBuf,     // absolute path to <uuid>.json
    pub hostname: String,              // public hostname (same as before)
}
```

### config.yaml Format Change

**Before:**
```yaml
tunnel:
  token: "eyJhIjoiN..."
  hostname: "right.example.com"
```

**After:**
```yaml
tunnel:
  tunnel_uuid: "e765cc71-d0c2-42a3-864b-81566f8817fd"
  credentials_file: "/home/wb/.rightclaw/tunnel/e765cc71-d0c2-42a3-864b-81566f8817fd.json"
  hostname: "right.example.com"
```

### Credentials File JSON Format

The file produced by `cloudflared tunnel create` (and stored in `~/.cloudflared/`) has 4 fields:
```json
{
  "AccountTag": "...",
  "TunnelSecret": "...",
  "TunnelID": "e765cc71-d0c2-42a3-864b-81566f8817fd",
  "Endpoint": ""
}
```

Reading `TunnelID` directly from the JSON replaces the JWT base64-decode logic (`tunnel_uuid()` method). [VERIFIED: local `~/.cloudflared/e765cc71-d0c2-42a3-864b-81566f8817fd.json` field names confirmed]

### cmd_init Flow Change

**Before:** `rightclaw init --tunnel-token <JWT> --tunnel-hostname <domain>`

**After:** `rightclaw init --tunnel-credentials-file <path> --tunnel-hostname <domain>`

`cmd_init` logic:
1. Accept `--tunnel-credentials-file <path>` (absolute or relative — resolve to absolute)
2. Read the file, parse JSON, extract `TunnelID`
3. Copy file to `~/.rightclaw/tunnel/<TunnelID>.json` (creates dir)
4. Store `TunnelConfig { tunnel_uuid, credentials_file: ~/.rightclaw/tunnel/<uuid>.json, hostname }` in `config.yaml`
5. Print: `"Tunnel UUID: <uuid> (hostname: <hostname>)"`

**Why copy instead of storing the original path:** The original path may be `~/.cloudflared/` which the user might later delete/rotate. Storing a copy in `~/.rightclaw/` ensures `rightclaw up` always has the file.

### cmd_up Changes

The current `cmd_up` cloudflared block already has two code paths:
1. No credentials file detected → warn and fall back to `--token`
2. Credentials file found → embed in config, omit `--token`

With Phase 38, path 1 is eliminated. The credentials file is now always available (stored in `~/.rightclaw/tunnel/`). The wrapper script becomes:
```sh
#!/bin/sh
set -e
cloudflared tunnel route dns <UUID> <HOSTNAME>
exec cloudflared tunnel --config <config_path> run
```

The `--token` branch in the wrapper script generation is removed. The `detect_cloudflared_credentials` function is removed. `TunnelConfig` provides `tunnel_uuid` and `credentials_file` directly.

### CloudflaredCredentials Struct

The `CloudflaredCredentials` struct in `codegen/cloudflared.rs` can be simplified or removed — `TunnelConfig` now directly carries `tunnel_uuid` and `credentials_file`. Callers can pass them directly.

### Doctor Check Changes

- `check_tunnel_token`: Checks `TunnelConfig.token` and decodes JWT. **Remove this check.**
- `check_tunnel_config`: Currently checks for `tunnel:` presence. Update fix hint to reference `--tunnel-credentials-file`.
- **New check** `check_tunnel_credentials_file`: Verifies the stored credentials file path actually exists on disk.

```
tunnel-config         warn  no tunnel configured — MCP OAuth callbacks will not work
                            fix: rightclaw init --tunnel-credentials-file PATH --tunnel-hostname HOSTNAME
tunnel-credentials    warn  credentials file not found at /path/to/<uuid>.json
                            fix: re-run rightclaw init to restore the credentials file
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead |
|---------|-------------|-------------|
| Parsing credentials JSON | Custom parser | `serde_json::from_str::<serde_json::Value>(&content)` — already used in `detect_cloudflared_credentials` |
| Copying files atomically | tmp+rename | `std::fs::copy` is fine here — credentials file is not written concurrently |
| UUID validation | Regex | Trust the file: if `TunnelID` parses as string, it is the UUID |

## Runtime State Inventory

This is a refactor phase — existing users who ran `rightclaw init --tunnel-token` have:

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | `~/.rightclaw/config.yaml` with `token:` + `hostname:` fields | Migration: old config still parses (add `#[serde(default)]` to new fields, detect missing `credentials_file` to guide re-init) |
| Live service config | cloudflared process running via process-compose | No migration — `rightclaw down` + `rightclaw up` after re-init |
| OS-registered state | None | None |
| Secrets/env vars | `RIGHTCLAW_HOME` — unchanged | None |
| Build artifacts | None | None |

**Migration strategy (critical):** The `RawTunnelConfig` deserializer must handle old configs with `token:` gracefully. Two options:

1. **Fail with clear message:** If `credentials_file` is absent/empty, return error: "Tunnel config is outdated — re-run `rightclaw init --tunnel-credentials-file PATH --tunnel-hostname HOSTNAME`"
2. **Silent skip:** Treat missing `credentials_file` as no tunnel configured (same as Phase 37 hostname migration)

Option 1 is safer — old config with `token:` but no `credentials_file:` means the user's tunnel won't work anyway (local ingress not enforced). Explicit error beats silent broken state.

The `token` field in `RawTunnelConfig` should be kept as `#[serde(default)]` to avoid parse errors on old configs. The `tunnel_uuid()` method (JWT decode) can be removed entirely once the `token` field is gone.

## Common Pitfalls

### Pitfall 1: cloudflared ignores local ingress config when --token is used
**What goes wrong:** Running `cloudflared tunnel run --token <JWT>` causes cloudflared to fetch ingress rules from the Cloudflare dashboard. Local `cloudflared-config.yml` with `ingress:` rules is ignored entirely. OAuth callback routing to Unix sockets never works.
**Why it happens:** The `--token` flag activates "remote management" mode. Local config file is only respected in "local management" mode (credentials-file).
**How to avoid:** Always use `cloudflared tunnel --config <path> run` (no `--token`) when credentials-file is embedded in config. Phase 38 makes this the only code path.
**Warning signs:** OAuth callback never arrives; cloudflared logs show "fetching remote config" instead of loading local ingress rules. [VERIFIED: cloudflared `--help` output confirms `--token` takes precedence over credentials]

### Pitfall 2: `cloudflared tunnel run` requires tunnel identifier
**What goes wrong:** `cloudflared tunnel --config <path> run` without a trailing name/UUID argument works — cloudflared reads `tunnel:` field from the config file. This is the correct invocation. But `cloudflared tunnel run` (no `--config`) fails if `~/.cloudflared/config.yml` doesn't exist.
**How to avoid:** Always pass `--config <path>` and embed `tunnel: <UUID>` + `credentials-file: <path>` in the config. [VERIFIED: cloudflared tunnel run --help]

### Pitfall 3: cert.pem required for `cloudflared tunnel create`, not for `tunnel run`
**What goes wrong:** Calling `cloudflared tunnel create` requires `cert.pem` from `cloudflared tunnel login`. But `cloudflared tunnel run` with a credentials file does NOT need `cert.pem`. If rightclaw tries to run `tunnel create` on behalf of the user, it needs `cert.pem` — which may not exist.
**How to avoid:** Phase 38 does NOT run `cloudflared tunnel create` automatically. The user creates the tunnel manually (or it already exists) and passes the credentials file to `rightclaw init`. This avoids the `cert.pem` dependency entirely. [VERIFIED: cloudflared tunnel run --help: "does not need access to cert.pem if you identify the tunnel by UUID"]

### Pitfall 4: Credentials file permissions
**What goes wrong:** `~/.cloudflared/<uuid>.json` is mode `0400` (`r--------`). `std::fs::copy` preserves the source mode on some platforms. The copied file at `~/.rightclaw/tunnel/<uuid>.json` needs to be readable by the process running cloudflared.
**How to avoid:** After `std::fs::copy`, explicitly set permissions to `0600` (user read+write) so the file is accessible. [VERIFIED: `ls -la ~/.cloudflared/` shows `r--------`]

### Pitfall 5: Old config.yaml backward compatibility
**What goes wrong:** Existing users have `token: "eyJ..."` in their config.yaml. Phase 38 changes the struct — if `RawTunnelConfig` removes the `token` field, serde will fail to parse old configs.
**How to avoid:** Keep `#[serde(default)] token: String` in `RawTunnelConfig` for now (or use `#[serde(rename = "token")]` with optional). Detect absence of `credentials_file` and emit helpful migration error.

### Pitfall 6: DNS routing `cloudflared tunnel route dns` is idempotent but requires cert.pem
**What goes wrong:** The DNS routing wrapper script calls `cloudflared tunnel route dns <UUID> <HOSTNAME>`. This command needs `cert.pem` (from `cloudflared tunnel login`). If `cert.pem` is missing or expired, the script fails — and with `set -e`, cloudflared never starts.
**Why it happens:** Route management (`route dns`) is an account-level operation, different from tunnel running.
**How to avoid:** The DNS route only needs to be set once. Options: (a) keep `set -e` and let process-compose restart handle cert expiry; (b) make `route dns` non-fatal (`|| true`) since the DNS record persists across restarts. Option (b) is safer for long-running deployments. [ASSUMED — behavior of route dns with expired cert not confirmed]

## Code Examples

### Reading TunnelID from Credentials File
```rust
// Source: current detect_cloudflared_credentials in crates/rightclaw-cli/src/main.rs
pub fn tunnel_uuid_from_credentials_file(path: &Path) -> miette::Result<String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("read credentials file: {e:#}"))?;
    let v: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| miette::miette!("parse credentials file: {e:#}"))?;
    v["TunnelID"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| miette::miette!("credentials file missing 'TunnelID' field"))
}
```

### Copying Credentials File with Correct Permissions
```rust
// After std::fs::copy, set 0600
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| miette::miette!("chmod credentials file: {e:#}"))?;
}
```

### Updated wrapper script (no --token)
```sh
#!/bin/sh
set -e
cloudflared tunnel route dns <UUID> <HOSTNAME>
exec cloudflared tunnel --config <config_path> run
```

### Updated cloudflared-config.yml.j2 (no change needed if template already has tunnel_uuid/credentials_file conditionals)
The template already renders `tunnel:` and `credentials-file:` conditionally on `tunnel_uuid`. With Phase 38, these are always non-empty when tunnel is configured — the conditional can be simplified or kept as-is.

## State of the Art

| Old Approach | Current Approach | Why Changed |
|--------------|-----------------|-------------|
| `--tunnel-token JWT` | `credentials-file + tunnel UUID` | JWT token mode uses remote config (dashboard); credentials-file mode uses local config (local ingress) |
| JWT base64 decode to get UUID | Read `TunnelID` from JSON | Simpler, no JWT parsing, works for all token formats |
| Detect `~/.cloudflared/` at runtime | Store copy in `~/.rightclaw/tunnel/` | Deterministic path, not dependent on cloudflared's default directory |

## Open Questions

1. **Should `route dns` failure be fatal in the wrapper script?**
   - What we know: `set -e` makes it fatal; DNS record persists once set and doesn't need re-routing every restart
   - What's unclear: behavior when cert.pem expires — does `route dns` fail silently or with an error?
   - Recommendation: Make `route dns` non-fatal (`|| true`) in the wrapper script. The DNS entry persists. [ASSUMED — verify by testing or checking cloudflared source]

2. **Should `rightclaw init` accept a tunnel name instead of credentials file path?**
   - What we know: `cloudflared tunnel token --cred-file <path> <NAME>` can fetch and write a credentials file for an existing tunnel
   - What's unclear: whether this is simpler UX for users who know their tunnel name
   - Recommendation: Out of scope for Phase 38 — accept `--tunnel-credentials-file <path>` only. Name-based lookup can be Phase 39+ if needed.

3. **Should the old `--tunnel-token` flag be removed or kept as deprecated?**
   - What we know: removing it breaks existing users with scripted `rightclaw init` calls
   - Recommendation: Keep `--tunnel-token` as a hidden deprecated arg that prints a clear migration message and exits 1, OR remove it entirely since this is pre-1.0 software. Decision for planner.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cloudflared | Tunnel management | Yes | 2026.3.0 | — |
| cert.pem | `cloudflared tunnel create` (user step) | Yes (at `~/.cloudflared/cert.pem`) | — | User must run `cloudflared tunnel login` |
| credentials JSON | `rightclaw init --tunnel-credentials-file` | Yes (`~/.cloudflared/e765cc71-d0c2-42a3-864b-81566f8817fd.json`) | — | User must run `cloudflared tunnel create` |

[VERIFIED: `ls /home/wb/.cloudflared/` confirms both cert.pem and credentials JSON exist]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `cloudflared tunnel route dns` with expired cert.pem fails with an error (not silently succeeds) | Pitfall 6, Open Question 1 | If it silently succeeds, `set -e` in wrapper script is unnecessary concern; if it fails with error, tunnel won't start |
| A2 | `std::fs::copy` preserves source file permissions on Linux | Pitfall 4 | If copy doesn't preserve, chmod step is still correct but the "pitfall" framing is wrong |

## Sources

### Primary (HIGH confidence)
- `~/.cloudflared/e765cc71-d0c2-42a3-864b-81566f8817fd.json` — credentials JSON field names verified locally
- `cloudflared tunnel run --help` output (v2026.3.0) — confirmed `--token` takes precedence over credentials, `--config` flag position
- `cloudflared tunnel create --help` output — confirmed what command produces credentials file
- `crates/rightclaw/src/config.rs` — current TunnelConfig, tunnel_uuid(), write_global_config
- `crates/rightclaw-cli/src/main.rs` (lines 566-636) — full cmd_up cloudflared block including detect_cloudflared_credentials
- `crates/rightclaw/src/codegen/cloudflared.rs` — generate_cloudflared_config, CloudflaredCredentials struct
- `crates/rightclaw/src/doctor.rs` (lines 114-684) — check_cloudflared_binary, check_tunnel_config, check_tunnel_token
- `templates/cloudflared-config.yml.j2` — cloudflared config template with tunnel_uuid/credentials_file conditionals
- `templates/process-compose.yaml.j2` — cloudflared process entry uses cloudflared_script_path

### Secondary (MEDIUM confidence)
- [Cloudflare docs: config file](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/configure-tunnels/local-management/configuration-file/) — confirmed `tunnel:` and `credentials-file:` fields in config.yml
- [Cloudflare docs: create local tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/get-started/create-local-tunnel/) — confirmed tunnel create flow, cert.pem requirement
- WebSearch: cloudflared credentials JSON format — confirmed AccountTag/TunnelSecret/TunnelID/Endpoint field names

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new crates, all changes in existing files
- Architecture: HIGH — credentials file format verified locally, cloudflared CLI help verified
- Pitfalls: MEDIUM — pitfall 6 (cert.pem + route dns) has one assumed claim

**Research date:** 2026-04-05
**Valid until:** 2026-05-05 (cloudflared API is stable)
