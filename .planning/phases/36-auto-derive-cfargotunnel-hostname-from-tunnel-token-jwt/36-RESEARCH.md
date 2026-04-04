# Phase 36: auto-derive-cfargotunnel-hostname-from-tunnel-token-jwt - Research

**Researched:** 2026-04-04
**Domain:** Rust — JWT base64url decode, TunnelConfig refactor, CLI arg removal
**Confidence:** HIGH

## Summary

Phase 36 removes the `--tunnel-hostname` CLI arg and replaces the stored `TunnelConfig.hostname` field with a derived method `TunnelConfig::hostname() -> miette::Result<String>`. The derivation decodes the cloudflared JWT token payload (segment[1] after splitting by `.`), base64url-decodes it with `URL_SAFE_NO_PAD`, parses the JSON, extracts the `"t"` field (tunnel UUID), and formats `{uuid}.cfargotunnel.com`.

All dependencies are already in the workspace (`base64 = "0.22"`, `serde_json`). Both are direct dependencies of `crates/rightclaw/`. The change touches 4 files: `config.rs`, `main.rs`, `cloudflared.rs` (no logic change, call site update only), and `cloudflared_tests.rs` (no change needed — tests use literal hostnames).

**Primary recommendation:** Implement `hostname()` as a method on `TunnelConfig` in `config.rs`. Update `cmd_init` and `cmd_up` call sites in `main.rs`. Remove `hostname` field from `TunnelConfig`, `RawTunnelConfig`, and all tests/fixtures in `config.rs`.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Remove `--tunnel-hostname` CLI arg completely from `rightclaw init`. Hard remove, no deprecation path.
- **D-02:** Existing `config.yaml` files with `hostname` field — ignore silently. `RawTunnelConfig` serde struct drops `hostname` field; unknown YAML fields are silently ignored by serde-saphyr default behavior. No migration, no error.
- **D-03:** `cmd_init` current guard `"both --tunnel-token and --tunnel-hostname are required together"` — removed entirely. Only `--tunnel-token` required for tunnel setup.
- **D-04:** Remove `hostname: String` from `TunnelConfig`. Struct becomes `{ token: String }` only.
- **D-05:** Add `impl TunnelConfig { pub fn hostname(&self) -> miette::Result<String> }` — derives hostname on every call. No caching. Callers call `.hostname()?` when they need it.
- **D-06:** `write_global_config` writes only `token` field under `tunnel:` key. YAML shrinks from 3 lines to 2.
- **D-07:** No new deps. Use `base64 = "0.22"` (already in workspace) + `serde_json` (already in workspace).
- **D-08:** Token format: cloudflared named tunnel token is a JWT with 3 dot-separated segments (`header.payload.signature`). Split by `.`, take segment index 1 (payload), base64url-decode with `base64::engine::general_purpose::URL_SAFE_NO_PAD`, parse JSON, extract string field `"t"` (tunnel UUID).
- **D-09:** Derived hostname: `format!("{}.cfargotunnel.com", uuid)` — always the default cfargotunnel.com subdomain.
- **D-10:** Error messages: "tunnel token has wrong number of segments (expected 3, got N)" and "tunnel token payload missing 't' field" — clear, actionable.
- **D-11:** `cmd_init` calls `tunnel_config.hostname()?` immediately after constructing the config — fails fast before writing `config.yaml`. Prints derived hostname to stdout: `Tunnel hostname: <uuid>.cfargotunnel.com`.
- **D-12:** `cmd_up` (cloudflared config generation path) calls `tunnel_cfg.hostname()?` again when building the cloudflared ingress config. Defensive re-validation — catches corrupted config.yaml tokens.

### Claude's Discretion
- Exact error type/context wording beyond D-10
- Whether `hostname()` lives on `TunnelConfig` or as a free function in `config.rs`
- Test structure (unit tests for decode logic + integration test for init flow)

### Deferred Ideas (OUT OF SCOPE)
- Custom domain override via `--tunnel-hostname` — possible future phase if user configures a custom Cloudflare subdomain instead of `*.cfargotunnel.com`
</user_constraints>

---

## Standard Stack

### Core (no new deps)

| Library | Locked Version | Purpose | Location |
|---------|---------------|---------|----------|
| `base64` | 0.22.1 (workspace) | base64url decode JWT payload | `crates/rightclaw/Cargo.toml` — already `base64 = { workspace = true }` |
| `serde_json` | 1.0.x (workspace) | Parse JWT payload JSON, extract `"t"` field | `crates/rightclaw/Cargo.toml` — already `serde_json = { workspace = true }` |
| `miette` | workspace | Error type for `hostname()` return | Already in use throughout `config.rs` |

**No installation required.** Both deps are in `crates/rightclaw/Cargo.toml` already.

---

## Architecture Patterns

### base64 0.22 API — Verified

`base64` 0.22 uses an engine-based API. The correct call for JWT payload decoding:

```rust
// Source: base64 0.22 crate (verified via Cargo.lock: 0.22.1)
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

let decoded_bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(payload_segment)?;
```

Key points:
- `URL_SAFE_NO_PAD` is the correct engine — JWT uses base64url encoding without `=` padding
- The `Engine` trait must be in scope (the `_` import `use base64::Engine as _` brings the `decode` method into scope)
- `decode` returns `Result<Vec<u8>, base64::DecodeError>`
- `DecodeError` does NOT implement `std::error::Error` directly in all contexts — use `.map_err(|e| miette::miette!("base64 decode error: {e}"))` to convert

### JWT Decode Pattern for `hostname()`

```rust
// Source: decisions D-07, D-08 from CONTEXT.md + base64 0.22 API
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub fn hostname(&self) -> miette::Result<String> {
    let segments: Vec<&str> = self.token.split('.').collect();
    if segments.len() != 3 {
        return Err(miette::miette!(
            "tunnel token has wrong number of segments (expected 3, got {})",
            segments.len()
        ));
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(segments[1])
        .map_err(|e| miette::miette!("tunnel token base64 decode failed: {e}"))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| miette::miette!("tunnel token JSON parse failed: {e:#}"))?;
    let uuid = payload["t"]
        .as_str()
        .ok_or_else(|| miette::miette!("tunnel token payload missing 't' field"))?;
    Ok(format!("{uuid}.cfargotunnel.com"))
}
```

### Anti-Patterns to Avoid

- **Do not cache hostname** — D-05 explicitly requires derivation on every call. No `OnceCell`, no `Option<String>` field.
- **Do not use `base64::decode()` top-level function** — removed in 0.22. Must use engine API.
- **Do not use `URL_SAFE` (with padding)** — JWT payload has no `=` padding; `URL_SAFE_NO_PAD` is mandatory.
- **Do not use `serde_json::from_str`** — payload is bytes after base64 decode; use `from_slice`.

---

## Exact Call Sites to Change

All changes are surgical. Full inventory:

### `crates/rightclaw/src/config.rs`

| Location | Current Code | Change |
|----------|-------------|--------|
| `TunnelConfig` struct | `{ token: String, hostname: String }` | Remove `hostname` field → `{ token: String }` |
| `RawTunnelConfig` struct | `{ token: String, hostname: String }` | Remove `hostname` field → `{ token: String }` |
| `read_global_config` | `TunnelConfig { token: t.token, hostname: t.hostname }` | Remove `hostname: t.hostname` |
| `write_global_config` | writes `token` + `hostname` lines | Remove hostname line; remove `let hostname = ...` |
| `impl TunnelConfig` | does not exist | Add `pub fn hostname(&self) -> miette::Result<String>` |
| Test: `write_then_read_global_config_roundtrips_tunnel` | constructs `TunnelConfig { token, hostname }`, asserts `tunnel.hostname` | Remove hostname field construction + assertion |
| Test: `write_global_config_creates_valid_yaml` | constructs with hostname, asserts parsed hostname | Remove hostname field + assertion |
| Test: `read_global_config_parses_yaml_with_tunnel_fields` | YAML fixture has `hostname:` line, asserts `tunnel.hostname` | Remove hostname from fixture + assertion |
| New tests | none | Add unit tests for `hostname()`: valid JWT, wrong segment count, missing `t` field, malformed base64 |

### `crates/rightclaw-cli/src/main.rs`

| Location | Line ~ | Current Code | Change |
|----------|--------|-------------|--------|
| `Commands::Init` variant | ~104 | `tunnel_hostname: Option<String>` arg | Remove field |
| `Commands::Init` match arm | ~202 | `tunnel_hostname` in destructure + `cmd_init` call | Remove from destructure + call |
| `cmd_init` signature | ~250 | `tunnel_hostname: Option<&str>` param | Remove param |
| `cmd_init` validation guard | ~253 | `match (tunnel_token, tunnel_hostname)` — returns error if only one provided | Remove entire match block |
| `cmd_init` hostname validation | ~287 | `if let Some(h) = tunnel_hostname { ... }` — scheme/port/empty checks | Remove entire block |
| `cmd_init` config write | ~306 | `if let (Some(t_token), Some(t_hostname)) = (tunnel_token, tunnel_hostname)` | Change to `if let Some(t_token) = tunnel_token`; construct `TunnelConfig { token: t_token.to_string() }` |
| `cmd_init` post-write output | after write | currently no hostname print | Add: `let h = config.tunnel.as_ref().unwrap().hostname()?; println!("Tunnel hostname: {h}");` |
| `cmd_up` cloudflared path | ~636 | `generate_cloudflared_config(&agent_pairs, &tunnel_cfg.hostname)` | Change to `generate_cloudflared_config(&agent_pairs, &tunnel_cfg.hostname()?)` |

### `crates/rightclaw/src/codegen/cloudflared.rs`

No logic changes. Function signature stays `generate_cloudflared_config(agents: &[(String, PathBuf)], tunnel_hostname: &str)`. Only the call site in `main.rs` changes (field access → method call).

### `crates/rightclaw/src/codegen/cloudflared_tests.rs`

No changes required. Tests pass literal hostname strings directly to `generate_cloudflared_config` — they don't use `TunnelConfig` at all.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| base64url decode | custom base64 decoder | `base64::engine::general_purpose::URL_SAFE_NO_PAD` | Standard JWT encoding; edge cases around padding |
| JSON field extraction | manual string search in decoded bytes | `serde_json::Value` | Handles unicode, escaping, nested structures |

---

## Common Pitfalls

### Pitfall 1: Wrong base64 engine
**What goes wrong:** Using `STANDARD` or `URL_SAFE` (with padding) fails for JWT payloads — they use base64url without `=` padding.
**Why it happens:** `STANDARD` is the mental default.
**How to avoid:** Always use `URL_SAFE_NO_PAD` for JWT segments.
**Warning signs:** `DecodeError::InvalidPadding` at runtime.

### Pitfall 2: Forgetting `Engine` trait import
**What goes wrong:** `URL_SAFE_NO_PAD.decode(...)` fails to compile — `decode` method not found.
**Why it happens:** The `Engine` trait is not in scope.
**How to avoid:** Add `use base64::Engine as _;` — the `_` alias is idiomatic when you only need the methods, not the name.

### Pitfall 3: base64 padding on cloudflared tokens
**What goes wrong:** Some JWT implementations pad the base64url payload to a multiple of 4 bytes; others don't. `URL_SAFE_NO_PAD` handles unpadded tokens. If cloudflared tokens have inconsistent padding (unlikely but possible), decode fails.
**How to avoid:** `URL_SAFE_NO_PAD` is the correct engine per JWT spec (RFC 7515 §2) — cloudflared follows this. No padding stripping needed.

### Pitfall 4: `serde_json::from_str` vs `from_slice`
**What goes wrong:** After base64 decode you have `Vec<u8>`, not a `String`. `from_str` requires a `&str` — you'd need an extra UTF-8 conversion step.
**How to avoid:** Use `serde_json::from_slice(&payload_bytes)` directly.

### Pitfall 5: `cmd_init` writes config BEFORE calling `hostname()`
**What goes wrong:** A bad token gets persisted to `config.yaml` before failing.
**Why it happens:** Validation placed after write.
**How to avoid:** Per D-11, construct `TunnelConfig { token }`, call `.hostname()?`, then write. If hostname derivation fails, no file is written.

---

## Code Examples

### Complete `hostname()` implementation (verified pattern)

```rust
// File: crates/rightclaw/src/config.rs
// Source: D-07, D-08, D-09, D-10 from CONTEXT.md; base64 0.22.1 API

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

impl TunnelConfig {
    pub fn hostname(&self) -> miette::Result<String> {
        let segments: Vec<&str> = self.token.split('.').collect();
        if segments.len() != 3 {
            return Err(miette::miette!(
                "tunnel token has wrong number of segments (expected 3, got {})",
                segments.len()
            ));
        }
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(segments[1])
            .map_err(|e| miette::miette!("tunnel token base64 decode failed: {e}"))?;
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
            .map_err(|e| miette::miette!("tunnel token JSON parse failed: {e:#}"))?;
        let uuid = payload["t"]
            .as_str()
            .ok_or_else(|| miette::miette!("tunnel token payload missing 't' field"))?;
        Ok(format!("{uuid}.cfargotunnel.com"))
    }
}
```

### Updated `cmd_init` tunnel write block

```rust
// File: crates/rightclaw-cli/src/main.rs
// After removing --tunnel-hostname and associated validation

if let Some(t_token) = tunnel_token {
    let tunnel_config = rightclaw::config::TunnelConfig {
        token: t_token.to_string(),
    };
    let hostname = tunnel_config.hostname()?;  // fails fast before write
    let config = rightclaw::config::GlobalConfig {
        tunnel: Some(tunnel_config),
    };
    rightclaw::config::write_global_config(home, &config)?;
    println!("Tunnel config written to {}/config.yaml", home.display());
    println!("Tunnel hostname: {hostname}");
}
```

### Updated `cmd_up` call site

```rust
// File: crates/rightclaw-cli/src/main.rs (~line 635)
let cf_config =
    rightclaw::codegen::cloudflared::generate_cloudflared_config(
        &agent_pairs,
        &tunnel_cfg.hostname()?,  // was: &tunnel_cfg.hostname
    )?;
```

### Updated `write_global_config` (after removing hostname)

```rust
// File: crates/rightclaw/src/config.rs
if let Some(ref tunnel) = config.tunnel {
    content.push_str("tunnel:\n");
    let token = tunnel.token.replace('"', "\\\"");
    content.push_str(&format!("  token: \"{token}\"\n"));
}
```

### Unit tests for `hostname()`

```rust
// File: crates/rightclaw/src/config.rs — #[cfg(test)] mod tests

// Build a valid JWT with known payload: {"t":"test-uuid-1234"}
// base64url({"t":"test-uuid-1234"}) = eyJ0IjoidGVzdC11dWlkLTEyMzQifQ
// (no padding — URL_SAFE_NO_PAD)
fn make_fake_jwt(uuid: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let payload = serde_json::json!({"t": uuid});
    let encoded = URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("header.{encoded}.signature")
}

#[test]
fn hostname_derives_cfargotunnel_domain() {
    let cfg = TunnelConfig { token: make_fake_jwt("abc-123") };
    let h = cfg.hostname().unwrap();
    assert_eq!(h, "abc-123.cfargotunnel.com");
}

#[test]
fn hostname_errors_on_wrong_segment_count() {
    let cfg = TunnelConfig { token: "only.two".to_string() };
    let err = cfg.hostname().unwrap_err();
    assert!(err.to_string().contains("wrong number of segments"));
    assert!(err.to_string().contains("got 2"));
}

#[test]
fn hostname_errors_on_missing_t_field() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let payload = URL_SAFE_NO_PAD.encode(r#"{"other":"value"}"#);
    let cfg = TunnelConfig { token: format!("h.{payload}.sig") };
    let err = cfg.hostname().unwrap_err();
    assert!(err.to_string().contains("missing 't' field"));
}
```

---

## Open Questions

1. **Real cloudflared token structure**
   - What we know: cloudflared named tunnel tokens are JWTs; the `"t"` field contains the tunnel UUID per cloudflared source code.
   - What's unclear: Whether real tokens always have exactly 3 segments (standard JWT) — no cloudflared-specific deviation found.
   - Recommendation: The unit test with a synthetic token is sufficient. D-08 is confident based on JWT spec compliance.

2. **`serde_json::Value` index on missing key**
   - What we know: `payload["t"]` returns `Value::Null` when key absent; `.as_str()` on `Null` returns `None` — so `ok_or_else` fires correctly.
   - What's unclear: nothing — this is well-defined `serde_json` behavior.
   - Recommendation: No concern; `.as_str()` on `Null` is the idiomatic way.

---

## Environment Availability

Step 2.6: SKIPPED — phase is pure code changes, no external dependencies beyond what's already in the workspace.

---

## Sources

### Primary (HIGH confidence)
- `Cargo.lock` (project) — base64 locked at 0.22.1; `serde_json` in workspace
- `crates/rightclaw/Cargo.toml` — both deps already declared
- `crates/rightclaw/src/config.rs` — exact struct layout, test inventory
- `crates/rightclaw-cli/src/main.rs` — exact call sites (lines ~103, ~202, ~250, ~253, ~287, ~306, ~636)
- `crates/rightclaw/src/codegen/cloudflared.rs` — function signature confirmed unchanged
- `crates/rightclaw/src/codegen/cloudflared_tests.rs` — confirmed no TunnelConfig usage

### Secondary (MEDIUM confidence)
- base64 0.22 crate documentation — `Engine` trait API, `URL_SAFE_NO_PAD` constant
- RFC 7515 §2 — JWT uses base64url without padding

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — both deps verified in workspace at exact versions
- Call site inventory: HIGH — read all 4 target files directly
- JWT decode pattern: HIGH — base64 0.22 API is stable; RFC 7515 defines the format
- Test strategy: HIGH — synthetic JWT construction is deterministic

**Research date:** 2026-04-04
**Valid until:** 2026-05-04 (stable domain)
