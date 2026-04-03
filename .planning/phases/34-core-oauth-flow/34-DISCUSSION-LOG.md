# Phase 34: Core OAuth Flow — Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.

**Date:** 2026-04-03

---

## Q: Which areas to discuss?
**Selected:** Browser behavior, Callback page UX, cloudflared lifecycle, Auth when agents not running

---

## Q: Browser behavior — how to present auth URL?
**User answer:** "Ты все неправильно понял. У нас MCP-автовая аутентификация происходит через бот. Slash MCP-auth, slash MCP-list и так далее"
**Key insight:** OAuth is triggered via Telegram bot, not CLI. No browser-open needed — bot sends URL to user.

---

## Q: Phase 34 (CLI) vs Phase 36 (Bot) — how do they relate?
**Options:** Phase 34 = engine only | Merge into one bot-only | Both independent
**Selected:** Merge into one phase (bot only) — eliminate CLI `rightclaw mcp auth`

---

## Q: What bot commands are in scope?
**Selected:** /mcp + /mcp auth, /mcp add + /mcp remove, /doctor, /mcp list
**Note:** User added /mcp list explicitly

---

## Q: cloudflared — quick tunnel or named tunnel?
**Selected:** Quick tunnel (spawn inline) with auth
**User note:** "быстрый туннель но надо требовать токен туннеля"
**Clarification follow-up:** What does "require tunnel token" mean?
**Selected:** Named tunnel (stable URL)
**Implications:** Operator sets up named Cloudflare Tunnel, stores token + URL in ~/.rightclaw/config.yaml via rightclaw init

---

## Q: cloudflared process — per-auth spawn or persistent?
**Selected:** Persistent process via process-compose
**Implications:** rightclaw up adds cloudflared entry to PC config

---

## Q: axum callback server — inside bot process or separate?
**User question:** "у нас несколько агентов может раниться. они могут шарить один туннель? если да то у нас должен быть один axum который как-то месседжит ботам?"
**Discussion:** Explored Option A (central rightclaw-oauth) vs Option B (per-agent axum + cloudflared path routing)

**Option A analysis:**
- Central process, one stable redirect_uri
- state parameter routes callback to correct agent
- axum calls Telegram API directly (no IPC) — user initially confused about "messaging"
- Concern: one process mixes OAuth protocol + credential write + Telegram messaging

**Option B analysis:**
- axum embedded in each bot process, unix socket per agent
- cloudflared path routing: /oauth/{agent}/callback → agent socket
- rightclaw up generates cloudflared config.yml with ingress rules
- Clean separation of concerns — each bot owns its OAuth

**Security review of public callback URL (ultrathink):**
- state = 128-bit random CSRF token → forgery impossible
- PKCE = code useless without server-side verifier → code interception defeated
- Flood attacks → O(1) lookup miss → 400, no state corruption
- Agent enumeration via URL path → low value, not sensitive

**Final decision:** Option B — per-agent axum with cloudflared path routing

---

## Q: Phase 35 (Token Refresh) — merge into 34 or keep separate?
**Selected:** Phase 35 stays separate (/mcp refresh, proactive up refresh, doctor warnings)
