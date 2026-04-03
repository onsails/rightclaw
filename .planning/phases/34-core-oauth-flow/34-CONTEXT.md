# Phase 34: Core OAuth Flow + Bot MCP Commands — Context

**Gathered:** 2026-04-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 34 is a **merged phase** (formerly Phase 34 + Phase 36). It delivers:
1. The full OAuth 2.1 + PKCE engine (AS discovery, DCR, callback server, token write)
2. All Telegram bot commands for MCP management: `/mcp`, `/mcp list`, `/mcp auth`, `/mcp add`, `/mcp remove`, `/doctor`
3. cloudflared named tunnel integration (persistent process via process-compose)

**What's NOT in this phase:**
- `rightclaw mcp auth <server>` CLI command — **eliminated**. Bot is the only entrypoint.
- Token refresh (`/mcp refresh`, proactive `rightclaw up` refresh) — Phase 35
- Named tunnel TUNL-01 (stable URL requirement is met HERE via named tunnel — TUNL-01 is already in scope)

</domain>

<decisions>
## Implementation Decisions

### D-01: No CLI mcp auth — Bot is the only entrypoint
`rightclaw mcp auth <server>` CLI command does NOT exist. The only way to trigger an OAuth
flow is via Telegram: `/mcp auth <server>`. OAUTH-01 from REQUIREMENTS.md is superseded.

### D-02: Bot commands scope
All bot commands are in this phase:
- `/mcp` or `/mcp list` — list MCP servers with auth status per agent (replaces DETECT-01 output in Telegram)
- `/mcp auth <server>` — trigger full OAuth flow, reply with auth URL
- `/mcp add <config>` — add MCP server to agent's .mcp.json (BOT-03)
- `/mcp remove <server>` — remove MCP server from agent's .mcp.json (BOT-04)
- `/doctor` — run rightclaw doctor and reply with results (BOT-05)

### D-03: cloudflared — Named Tunnel (stable URL, NOT quick tunnel)
- Named Cloudflare Tunnel (not anonymous quick tunnel)
- Operator configures via `rightclaw init --tunnel-token <TOKEN> --tunnel-url <URL>`
- Stored in `~/.rightclaw/config.yaml`
- `rightclaw up` spawns cloudflared as a persistent process-compose entry using the token
- `rightclaw doctor` checks cloudflared binary and tunnel config presence (Warn severity)

### D-04: OAuth architecture — Option B (per-agent axum, cloudflared path routing)
**Each bot-process embeds its own axum callback server** on a Unix socket.
cloudflared routes by path prefix to the correct agent's socket.

```
redirect_uri = https://<tunnel-url>/oauth/<agent-name>/callback
cloudflared routes:
  /oauth/right/callback  → unix:/home/wb/.rightclaw/agents/right/oauth-callback.sock
  /oauth/scout/callback  → unix:/home/wb/.rightclaw/agents/scout/oauth-callback.sock
```

`rightclaw up` generates `~/.rightclaw/cloudflared-config.yml` with ingress rules for all agents.
cloudflared is launched with `--config ~/.rightclaw/cloudflared-config.yml --token <TOKEN>`.

**Why this beats central rightclaw-oauth (Option A):**
- Each bot owns its OAuth flow end-to-end — clean separation of concerns
- No shared in-process state across agents
- Bot already knows its own agent dir, credentials path, bot_token — no IPC needed

### D-05: Security model for public callback endpoint
Per-agent OAuth callback endpoint is public (via cloudflared tunnel). Security:
- `state` = 128-bit cryptographically random token generated at flow initiation
- Stored in-process in the bot (HashMap: state → PendingAuth)
- Constant-time comparison (`subtle` crate) to defeat timing attacks
- PendingAuth is consumed (removed) on first successful use (one-shot)
- PKCE `code_verifier` stored server-side only — authorization code is useless without it
- Fake/flooded callbacks → state lookup miss → immediate 400, no state mutation

### D-06: OAuth flow sequence (bot-initiated)
```
1. User → /mcp auth notion → right-bot
2. right-bot: AS discovery (RFC 9728 → RFC 8414 → OIDC well-known)
3. right-bot: DCR or static clientId fallback
4. right-bot: generate PKCE (code_verifier, code_challenge), state token
5. right-bot: store PendingAuth{server, pkce_verifier, state}
6. right-bot → Telegram: "Auth URL: https://api.notion.com/...?state=..."
7. User clicks → Notion → rightclaw-tunnel/oauth/right/callback?code=X&state=Y
8. right-bot axum: verify state, PKCE exchange, write credential
9. right-bot → Telegram: "notion authenticated — agent right restarting"
10. right-bot → PC REST API: restart_process("agent-right")
```

### D-07: AS discovery fallback behavior
- RFC 9728 (resource metadata): if 404 → try next; if 5xx → treat as failure (abort)
- RFC 8414 (AS metadata): if 404 → try OIDC; if 5xx → abort
- OIDC `.well-known/openid-configuration`: if not found → abort with clear error
- Discovery result visible in debug output (tracing::debug!)

### D-08: DCR fallback
If server has no `registration_endpoint` in AS metadata → use static `clientId` from `.mcp.json`
(OAUTH-03). If `.mcp.json` has no clientId either → abort with clear error.

### D-09: cloudflared config generation
`rightclaw up` generates `~/.rightclaw/cloudflared-config.yml` from agent list before launching.
cloudflared is added as a process-compose entry. Socket paths derived from agent dir.
Format: `unix:<agent_dir>/oauth-callback.sock`

### D-10: REQUIREMENTS.md updates needed
- OAUTH-01 superseded (no CLI — bot only)
- OAUTH-04/05 updated: cloudflared = named tunnel, not quick tunnel
- BOT-01 through BOT-05 merged into Phase 34 (Phase 36 removed from roadmap)
- TUNL-01 (named tunnel) is NOW in scope (not future) — moves from Future to Phase 34

### Claude's Discretion
- axum port/socket binding strategy within bot process (tokio select on bot + axum)
- PendingAuth timeout (how long to wait for user to complete OAuth before cleaning up)
- Exact response HTML/text returned to browser after successful callback
- `rightclaw-config.yaml` schema (tunnel_token, tunnel_url fields)
- cloudflared process name in process-compose YAML

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` — OAUTH-01..07, BOT-01..05, TUNL-01 (now in scope)

### Foundation (Phase 32–33)
- `crates/rightclaw/src/mcp/credentials.rs` — `write_credential`, `read_credential`, `CredentialToken`, `mcp_oauth_key`
- `crates/rightclaw/src/mcp/detect.rs` — `mcp_auth_status`, `AuthState`, `ServerStatus`

### Existing patterns to follow
- `crates/rightclaw/src/runtime/pc_client.rs` — `PcClient.restart_process()`, reqwest HTTP client pattern
- `crates/rightclaw/src/codegen/process_compose.rs` — process-compose YAML generation pattern (same approach for cloudflared config)
- `crates/rightclaw-cli/src/main.rs` — `McpCommands` enum (Phase 33 added `Status`; this phase adds `Auth`)
- `crates/rightclaw/src/codegen/mcp_config.rs` — `.mcp.json` read/write pattern (for `/mcp add`, `/mcp remove`)

### Telegram bot
- `crates/` — find existing teloxide bot implementation from Phase 23–26 (search for teloxide)

### External specs
- OAuth 2.1 + PKCE: RFC 7636 (PKCE), RFC 9728 (resource metadata), RFC 8414 (AS metadata), RFC 7591 (DCR)
- cloudflared config format: `~/.cloudflared/config.yml` YAML schema (ingress rules, unix socket service)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mcp::credentials::write_credential` — called after successful token exchange
- `mcp::credentials::read_credential` — for `/mcp list` to show auth status
- `mcp::detect::mcp_auth_status` — for `/mcp` command (show table per agent)
- `PcClient::restart_process(agent_name)` — agent restart after OAuth write
- `reqwest` workspace dep — already in rightclaw crate, use for AS discovery + token exchange
- `uuid` workspace dep (v4) — for PKCE state token generation, or use `rand`
- `sha2` workspace dep — for PKCE S256 code_challenge

### New deps needed
- `axum` — not yet in workspace; add for per-bot callback server
- `subtle` — constant-time comparison for state token validation
- cloudflared YAML config generation — minijinja already in workspace

### Established Patterns
- YAML generation: minijinja template → write file (see `codegen/process_compose.rs`)
- Telegram bot: teloxide long polling (find existing bot crate from Phase 23–26)
- Error handling: `miette::miette!()` for user-facing, `thiserror` for structured types

### Integration Points
- `rightclaw up` → add cloudflared config generation step + cloudflared process to PC config
- `rightclaw init` → add `--tunnel-token` / `--tunnel-url` flags
- `rightclaw doctor` → add cloudflared binary check + tunnel config presence check

</code_context>

<specifics>
## Specific Ideas

- Flow confirmation message in Telegram should include agent name: "notion authenticated — agent right restarting"
- Socket path convention: `<agent_dir>/oauth-callback.sock`
- cloudflared process name in PC: `"cloudflared"` (single entry, not per-agent)
- Tunnel URL in config is the user-managed subdomain (e.g., `rightclaw.example.com`) — rightclaw doesn't manage DNS

</specifics>

<deferred>
## Deferred Ideas

- `/mcp refresh` command via bot — Phase 35 (Token Refresh)
- Proactive `rightclaw up` token refresh — Phase 35
- `rightclaw doctor` MCP token warnings — Phase 35
- CLI `rightclaw mcp auth` entrypoint — eliminated, not deferred
- Anonymous quick tunnel (ephemeral URL) — eliminated in favor of named tunnel

</deferred>

---

*Phase: 34-core-oauth-flow*
*Context gathered: 2026-04-03*
