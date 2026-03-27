---
id: SEED-010
status: dormant
planted: 2026-03-27
planted_during: v2.3 Memory System
trigger_when: hardening for production / production-ready security posture
scope: Medium
---

# SEED-010: denyRead `/` — switch sandbox from deny-list to allow-list for filesystem reads

## Why This Matters

Agents can currently read anything outside `~/.ssh`, `~/.aws`, `~/.gnupg`, and `~/`. That means `/etc/passwd`, `/proc/self/environ`, other users' home dirs, mounted secrets, and any system path are fair game. An agent with prompt injection or a rogue skill can exfiltrate secrets from the host filesystem.

The current model is **deny-list** (block known-bad paths). It should be **allow-list** (deny `/`, then explicitly permit only what the agent needs). This is the least-privilege principle applied to sandbox reads.

## When to Surface

**Trigger:** When hardening for production — moving from alpha to production-ready security posture.

This seed should be presented during `/gsd:new-milestone` when the milestone scope matches any of these conditions:
- Security hardening or production readiness milestone
- Sandbox model overhaul or tightening
- Compliance or audit preparation

## Scope Estimate

**Medium** — a phase or two. Needs careful enumeration of required read paths per platform (Linux vs macOS), testing that agents can still function (claude binary, shared libs, /tmp, /dev/null, etc.), and updating the `SandboxOverrides` schema if `deny_read` overrides are needed.

## Breadcrumbs

Related code and decisions found in the current codebase:

- `crates/rightclaw/src/codegen/settings.rs:57-65` — current `deny_read` construction (only blocks host HOME + secrets dirs)
- `crates/rightclaw/src/codegen/settings.rs:43-45` — `allow_read` defaults to agent path only
- `crates/rightclaw/src/codegen/settings_tests.rs:195-230` — existing denyRead/allowRead tests
- `crates/rightclaw/src/agent/types.rs:36` — `SandboxOverrides.allow_read` field
- `.planning/seeds/SEED-008-managed-settings-strict-sandbox.md` — related seed on strict sandbox settings

## Notes

Key implementation considerations:
- `denyRead: ["/"]` with `allowRead: [agent_path, "/usr/lib", "/usr/bin", "/tmp", ...]`
- Must allow paths needed by claude binary, Node.js, shared libraries, and `/dev/null`
- macOS and Linux have different required system paths (e.g. `/Library/` on macOS)
- CC's Write/Edit tools bypass bwrap — this only constrains Bash `cat`/`head`/etc. and the Read tool
- Need to test that process-compose, tracing output, and skill loading still work
- `allow_read` in `agent.yaml` already exists for user overrides — schema is ready
