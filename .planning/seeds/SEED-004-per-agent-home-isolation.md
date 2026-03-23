---
id: SEED-004
status: dormant
planted: 2026-03-23
planted_during: v1.0 / manual testing
trigger_when: next milestone or agent isolation phase
scope: Medium
---

# SEED-004: Set $HOME per agent to isolate from host Claude Code settings

## Problem

When running in `--no-sandbox` mode, each agent's Claude Code session inherits the host user's `$HOME`. This means:

- All agents share `~/.claude/settings.json` (host user's plugins, hooks, env vars, permissions)
- All agents share `~/.claude.json` (OAuth tokens, project trust, cached feature flags)
- Agent-specific `.claude/settings.json` in the agent dir adds to but doesn't replace host settings
- Host settings like `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` broke channels for ALL agents
- Hooks from GSD/other tools fire inside agent sessions (we saw "SessionStart:startup hook error")
- MCP servers from the host user's config load into agent sessions (we saw "1 MCP server needs auth" for unrelated servers like Canva, Gmail, Google Calendar)

Agents should be isolated from each other AND from the host user's Claude Code configuration.

## Proposed solution

Set `HOME` environment variable per agent in the shell wrapper to the agent's directory:

```bash
export HOME="/home/wb/.rightclaw/agents/right"
exec claude ...
```

This makes Claude Code look for `~/.claude/` inside the agent directory instead of the host home. Each agent gets its own:
- `~/.claude/settings.json` (already created by `rightclaw init`)
- `~/.claude.json` (agent-specific trust, tokens, feature flags)
- `~/.claude/plugins/` (per-agent plugin cache)

## Benefits

- No more host hook interference ("SessionStart:startup hook error" gone)
- No more unrelated MCP servers loading (Canva, Gmail, etc.)
- No more host env vars leaking (`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`)
- True per-agent isolation even without OpenShell
- Each agent can have different plugins, permissions, settings

## Risks

- OAuth tokens in `~/.claude.json` won't exist in the agent's HOME — need to copy or symlink
- Plugin cache would be per-agent (more disk, slower first start)
- Some Claude Code features might expect the real HOME

## Breadcrumbs

- `templates/agent-wrapper.sh.j2` — where to add `export HOME=...`
- `crates/rightclaw/src/codegen/shell_wrapper.rs` — template context already has `working_dir`
- `crates/rightclaw/src/init.rs` — already creates `.claude/settings.json` in agent dir
- Debug log showing host MCP servers loading: `~/.rightclaw/run/right-debug.log`

## Scope estimate

Medium — simple env var change in wrapper, but needs testing for OAuth token handling and plugin cache behavior.
