# /rightmcp Skill Design

## Problem

Agents receive MCP management instructions in the system prompt but don't follow them
reliably — they search for wrong URL types or guess from training data instead of
performing the prescribed search. Skills are followed more strictly than background
system prompt instructions.

## Solution

Create a `/rightmcp` built-in skill (same infrastructure as rightcron/rightskills) that
activates when the user asks to connect an MCP server. The skill contains the full
search algorithm and architecture knowledge the agent needs.

## Skill Content

### Metadata

```
name: rightmcp
description: Find and add MCP servers via Telegram commands
version: 0.1.0
```

### Architecture Context (what the agent must know)

- The agent has NO direct MCP management access. All management goes through the
  user's Telegram commands.
- The RightClaw MCP Aggregator proxies all MCP traffic, stores OAuth tokens,
  refreshes them automatically. The agent never sees or handles credentials.
- `/mcp add <name> <url>` auto-detects auth type:
  1. OAuth AS discovery on bare URL → if found, registers and tells user to `/mcp auth`
  2. Query string detection (key embedded in URL) → registers as-is
  3. Haiku-based auth type detection → determines bearer/header
  4. Falls back to bearer → asks user for token in Telegram
- `/mcp auth <name>` — starts OAuth flow (browser-based)
- `/mcp remove <name>` — unregisters (`right` is protected)
- `/mcp list` — shows all servers with status (also available as `mcp_list()` MCP tool)

### Activation

When the user asks to add, connect, or set up an MCP server/integration.

### Algorithm

1. **Check current servers.** Call `mcp_list()` to see what's already registered.
   If the requested service is already connected, tell the user and stop.

2. **Search for OAuth endpoint first.** The first search query MUST target Claude Code
   or Codex integration docs — these describe OAuth-capable MCP endpoints.
   Search: `"<service> MCP Claude Code"` or `"<service> MCP server Codex"`.
   Look for streamable HTTP or SSE URLs (e.g. `mcp.service.dev/sse`,
   `service.com/mcp`).

3. **If OAuth URL found** — give the user the Telegram command:
   ```
   /mcp add <name> <url>
   ```
   The bot will detect OAuth automatically and prompt for `/mcp auth`.

4. **If no OAuth endpoint found** — search for any MCP endpoint (API-key, public).
   Look for URLs that may include tokens/keys in query parameters or require
   API key headers. Give the user:
   ```
   /mcp add <name> <url>
   ```
   The bot will determine the auth method and ask for credentials if needed.

5. **If no MCP endpoint found at all** — tell the user the service may not have
   MCP support yet. Suggest checking the service's docs or integrations page.

### Constraints

- NEVER ask the user for API keys or tokens — the bot handles credential collection
- NEVER guess or fabricate URLs from training data — only use URLs from search results
- NEVER attempt to call internal MCP management APIs — they don't exist as agent tools
- ALWAYS search the web first — do not rely on prior knowledge of MCP endpoints

## Changes to OPERATING_INSTRUCTIONS.md

Replace the current MCP Management section (everything between `## MCP Management`
and `## Communication`) with:

```markdown
## MCP Management

You CANNOT add, remove, or authenticate MCP servers yourself.
The user manages them via Telegram commands:

- `/mcp add <name> <url>` — register a server (auto-detects auth type)
- `/mcp remove <name>` — unregister a server (`right` is protected)
- `/mcp auth <name>` — start OAuth flow
- `/mcp list` — show all servers with status

When the user asks to connect an MCP server, ALWAYS use the `/rightmcp` skill.
NEVER attempt to find MCP URLs without it.
```

## Infrastructure Changes

1. **New file:** `skills/rightmcp/SKILL.md`
2. **codegen/skills.rs:** Add `SKILL_RIGHTMCP` constant + install to `.claude/skills/rightmcp/`
3. **sync.rs:** Add `"rightmcp"` to the builtin skills upload loop
4. **openshell.rs:** Add `"rightmcp"` to staging dir copy loop
5. **OPERATING_INSTRUCTIONS.md:** Replace MCP Management section

## Testing

1. Build binary, restart bot
2. Ask agent "добавь mcp composio"
3. Verify: agent activates /rightmcp, calls mcp_list(), searches "Composio MCP Claude Code",
   finds OAuth URL, gives `/mcp add composio <url>` command
