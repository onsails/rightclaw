---
id: SEED-003
status: dormant
planted: 2026-03-23
planted_during: v1.0 / manual testing (sandbox mode)
trigger_when: next milestone or sandboxing/security phase
scope: Large
---

# SEED-003: Use Claude Code native sandboxing instead of or alongside OpenShell

## Problem

OpenShell sandboxes are fully isolated K3s containers with no host file access. This breaks Claude Code's OAuth authentication flow:

- Claude Code stores OAuth tokens in `~/.claude.json` on the host
- Inside an OpenShell sandbox, the home directory is `/home/sandbox/` — host files don't exist
- OpenShell's provider system only supports `ANTHROPIC_API_KEY` / `CLAUDE_API_KEY` env vars
- No OAuth support: no file-based credential discovery, no token refresh mechanism
- Result: Claude Max / OAuth users CANNOT use OpenShell sandbox mode without also having an API key

This is a fundamental architectural mismatch — OpenShell was designed for API key injection via L7 proxy rewriting, not for OAuth session management.

## Proposed solution

Claude Code has **built-in sandbox support** that may handle this natively:
- Docs: https://code.claude.com/docs/en/sandboxing
- Claude-native sandboxing preserves the auth context because it's managed by Claude Code itself
- Could potentially run alongside OpenShell policies for network/filesystem restrictions
- Or replace OpenShell entirely for the sandboxing layer

## Why it matters

- **Security model broken for OAuth users**: The core value prop of RightClaw ("every agent sandboxed") only works for API key users
- **Claude Max is the primary audience**: Most RightClaw users will have Claude Max (OAuth), not API keys
- **OpenShell is alpha**: Betting entirely on OpenShell is risky. Having Claude-native sandboxing as a fallback or primary option reduces dependency on external alpha software

## What we learned during testing

1. OpenShell base image has Claude Code v2.1.80 installed at `/usr/local/bin/claude`
2. `--auto-providers` and `--from-existing` only check env vars, not `~/.claude.json`
3. `--no-auto-providers` skips provider creation but Claude still fails without auth
4. Policy `read_write: ~/.claude` expands to the sandbox user's home, NOT the host
5. `--upload` copies files but is one-time, not live mount
6. No `openshell sandbox mount` or bind-mount capability exists
7. OpenShell provider system uses L7 proxy secret rewriting — elegant but only for API keys

## Breadcrumbs

- `templates/agent-wrapper.sh.j2` — current sandbox invocation pattern
- `templates/right/policy.yaml` — OpenShell policy with anthropic_api endpoints
- `templates/right/policy-telegram.yaml` — Telegram-enabled variant
- OpenShell source: `/tmp/openshell-analysis/` (cloned during investigation)
- OpenShell provider discovery: `crates/openshell-providers/src/providers/claude.rs`
- Claude Code sandboxing docs: https://code.claude.com/docs/en/sandboxing
- NVIDIA/OpenShell GitHub: https://github.com/NVIDIA/OpenShell
- NVIDIA/OpenShell-Community sandboxes: https://github.com/NVIDIA/OpenShell-Community/tree/main/sandboxes

## Options to evaluate

1. **Claude-native sandboxing as primary** — Replace OpenShell with Claude Code's built-in sandbox. Simpler, auth works natively. May lack OpenShell's kernel-level enforcement (Landlock, Seccomp).

2. **Dual-layer**: Claude-native sandbox for auth/process isolation + OpenShell for network policy enforcement only (L7 proxy without credential injection).

3. **API key requirement for sandbox mode** — Accept that sandbox mode requires `ANTHROPIC_API_KEY`. Document as requirement. `--no-sandbox` for OAuth users.

4. **Custom provider plugin for OpenShell** — Write an OpenShell provider that reads `~/.claude.json` OAuth tokens from the host and injects them. Would require OpenShell to support file-based credential sources (currently env-only).

## Scope estimate

Large — requires researching Claude Code's sandbox system, potentially redesigning the agent launch architecture, and updating all sandbox-related code.
