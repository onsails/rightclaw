# /rightmcp Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a `/rightmcp` built-in skill that reliably guides agents through MCP server discovery and addition via Telegram commands.

**Architecture:** New SKILL.md file compiled into binary via `include_str!()`, installed by codegen, synced to sandbox — identical pipeline to rightcron/rightskills. OPERATING_INSTRUCTIONS.md MCP section trimmed to just commands + skill reference.

**Tech Stack:** Rust (include_str, codegen), Markdown (SKILL.md)

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `skills/rightmcp/SKILL.md` | Skill content: search algorithm, architecture context, constraints |
| Modify | `crates/rightclaw/src/codegen/skills.rs:1-31` | Add `SKILL_RIGHTMCP` constant + install entry |
| Modify | `crates/bot/src/sync.rs:123` | Add `"rightmcp"` to builtin skills upload loop |
| Modify | `crates/rightclaw/src/openshell.rs:490` | Add `"rightmcp"` to staging dir copy loop |
| Modify | `templates/right/prompt/OPERATING_INSTRUCTIONS.md:31-64` | Replace MCP Management section |

---

### Task 1: Create SKILL.md

**Files:**
- Create: `skills/rightmcp/SKILL.md`

- [ ] **Step 1: Create the skill file**

```markdown
---
name: rightmcp
description: >-
  Finds and adds MCP servers for this RightClaw agent. Searches for OAuth-capable
  endpoints first (Claude Code / Codex integration docs), falls back to API-key
  endpoints. All management goes through the user's Telegram commands — the agent
  never handles credentials directly. Use when the user asks to add, connect,
  or set up an MCP server or integration.
version: 0.1.0
---

# /rightmcp -- Add MCP Server

## When to Activate

Activate this skill when:
- The user asks to add, connect, or set up an MCP server or integration
- The user asks about connecting a third-party service via MCP
- The user names a specific service and wants it added (e.g. "add Composio", "connect Linear")

## Architecture

You have NO direct MCP management access. All management goes through the user's
Telegram commands. Here's what happens behind the scenes:

- The **RightClaw MCP Aggregator** proxies all MCP traffic, stores tokens, and
  refreshes OAuth automatically. You never see or handle credentials.
- `/mcp add <name> <url>` auto-detects authentication:
  1. Tries OAuth AS discovery on the URL — if found, registers and tells user to `/mcp auth`
  2. Detects query-string auth (key embedded in URL) — registers as-is
  3. For other URLs — detects auth type, asks user for token in Telegram if needed
- `/mcp auth <name>` — starts browser-based OAuth flow
- `/mcp remove <name>` — unregisters a server (`right` is protected)
- `/mcp list` — shows all servers with status

## Procedure

### Step 1: Check current servers

Call `mcp_list()` to see what's already registered. If the requested service is
already connected, tell the user and stop.

### Step 2: Search for OAuth endpoint FIRST

Your first search query MUST target Claude Code or Codex integration docs.
These describe OAuth-capable MCP endpoints that work with `/mcp auth`.

Use these search queries (in order, stop when you find a URL):
1. `"<service> MCP server Claude Code"`
2. `"<service> MCP Claude Desktop config"`
3. `"<service> MCP endpoint OAuth"`

Look for streamable HTTP or SSE URLs like:
- `https://mcp.service.dev/sse`
- `https://mcp.service.dev/mcp`
- `https://service.com/v1/mcp`

**DO NOT** use URLs from your training data. Only use URLs found in search results.

### Step 3: If OAuth URL found

Give the user the Telegram command:
```
/mcp add <name> <url>
```
The bot detects OAuth automatically and will prompt for `/mcp auth <name>`.

### Step 4: If no OAuth endpoint found

Search more broadly for any MCP endpoint:
1. `"<service> MCP server URL"`
2. Check the service's official docs for MCP/API integration pages

If you find an API-key URL (token in query string or requires header), give:
```
/mcp add <name> <url>
```
The bot will determine the auth method and ask the user for credentials if needed.

### Step 5: If no MCP endpoint found

Tell the user the service may not have MCP support yet. Suggest:
- Checking the service's docs or integrations page directly
- Looking for community MCP servers on GitHub

## Constraints

- **NEVER** ask the user for API keys or tokens — the bot handles credential collection
- **NEVER** guess or fabricate URLs from training data — only use URLs from search results
- **NEVER** attempt to call internal MCP management APIs — they don't exist as agent tools
- **ALWAYS** search the web before responding — do not rely on prior knowledge of MCP endpoints
- **ALWAYS** call `mcp_list()` first to check existing servers
```

- [ ] **Step 2: Verify file exists**

Run: `ls -la skills/rightmcp/SKILL.md`
Expected: file exists, non-empty

- [ ] **Step 3: Commit**

```bash
git add skills/rightmcp/SKILL.md
git commit -m "feat: add /rightmcp skill for MCP server discovery"
```

---

### Task 2: Wire into codegen

**Files:**
- Modify: `crates/rightclaw/src/codegen/skills.rs:1-15`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/rightclaw/src/codegen/skills.rs`:

```rust
#[test]
fn installs_rightmcp_skill() {
    let dir = tempdir().unwrap();
    install_builtin_skills(dir.path()).unwrap();
    assert!(
        dir.path().join(".claude/skills/rightmcp/SKILL.md").exists(),
        "rightmcp/SKILL.md should exist"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `devenv shell -- cargo test -p rightclaw installs_rightmcp_skill`
Expected: FAIL — `rightmcp/SKILL.md should exist`

- [ ] **Step 3: Add the constant and install entry**

In `crates/rightclaw/src/codegen/skills.rs`, add the third constant at line 4:

```rust
const SKILL_RIGHTMCP: &str = include_str!("../../../../skills/rightmcp/SKILL.md");
```

Add to the `built_in_skills` array (line 13-14), after the rightcron entry:

```rust
("rightmcp/SKILL.md", SKILL_RIGHTMCP),
```

Update the doc comment on `install_builtin_skills` (line 8) to mention rightmcp:

```rust
/// Writes `rightskills/SKILL.md`, `rightcron/SKILL.md`, `rightmcp/SKILL.md`, and `installed.json`.
```

- [ ] **Step 4: Run test to verify it passes**

Run: `devenv shell -- cargo test -p rightclaw installs_rightmcp_skill`
Expected: PASS

- [ ] **Step 5: Run all skills tests**

Run: `devenv shell -- cargo test -p rightclaw skills`
Expected: all 7 tests pass (6 existing + 1 new)

- [ ] **Step 6: Commit**

```bash
git add crates/rightclaw/src/codegen/skills.rs
git commit -m "feat: wire rightmcp skill into codegen"
```

---

### Task 3: Add to sandbox sync and staging

**Files:**
- Modify: `crates/bot/src/sync.rs:123`
- Modify: `crates/rightclaw/src/openshell.rs:490`

- [ ] **Step 1: Add "rightmcp" to sync.rs upload loop**

In `crates/bot/src/sync.rs`, line 123, change:

```rust
for skill_name in &["rightskills", "rightcron"] {
```

to:

```rust
for skill_name in &["rightskills", "rightcron", "rightmcp"] {
```

- [ ] **Step 2: Add "rightmcp" to openshell.rs staging loop**

In `crates/rightclaw/src/openshell.rs`, line 490, change:

```rust
for builtin in &["rightskills", "rightcron"] {
```

to:

```rust
for builtin in &["rightskills", "rightcron", "rightmcp"] {
```

- [ ] **Step 3: Verify it compiles**

Run: `devenv shell -- cargo check --workspace`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add crates/bot/src/sync.rs crates/rightclaw/src/openshell.rs
git commit -m "feat: add rightmcp to sandbox sync and staging"
```

---

### Task 4: Update OPERATING_INSTRUCTIONS.md

**Files:**
- Modify: `templates/right/prompt/OPERATING_INSTRUCTIONS.md:31-64`

- [ ] **Step 1: Replace the MCP Management section**

In `templates/right/prompt/OPERATING_INSTRUCTIONS.md`, replace everything from
`## MCP Management` (line 31) through the line before `## Communication` (line 65)
with:

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

- [ ] **Step 2: Verify the template compiles into the binary**

Run: `devenv shell -- cargo build --workspace`
Expected: clean build (include_str picks up the reduced template)

- [ ] **Step 3: Verify the new text is in the binary**

Run: `devenv shell -- rg "ALWAYS use the ./rightmcp" target/debug/rightclaw`
Expected: `binary file matches`

Run: `devenv shell -- rg "Find the OAuth endpoint first" target/debug/rightclaw`
Expected: no match (old text gone from OPERATING_INSTRUCTIONS — may still exist in rightmcp SKILL.md binary but NOT in the operating instructions section)

- [ ] **Step 4: Commit**

```bash
git add templates/right/prompt/OPERATING_INSTRUCTIONS.md
git commit -m "feat: slim MCP instructions, delegate to /rightmcp skill"
```

---

### Task 5: Full build and test

**Files:** (none — verification only)

- [ ] **Step 1: Run all workspace tests**

Run: `devenv shell -- cargo test --workspace`
Expected: all tests pass. Watch specifically for:
- `skills::` tests (should be 7, all pass)
- Any test that asserts on OPERATING_INSTRUCTIONS content

- [ ] **Step 2: Full build**

Run: `devenv shell -- cargo build --workspace`
Expected: clean build, no warnings related to our changes

- [ ] **Step 3: Verify rightmcp SKILL.md content in binary**

Run: `devenv shell -- rg "NEVER guess or fabricate URLs" target/debug/rightclaw`
Expected: `binary file matches` (skill content compiled in)

- [ ] **Step 4: Commit (if any fixups needed)**

Only if previous steps required fixes. Otherwise skip.
