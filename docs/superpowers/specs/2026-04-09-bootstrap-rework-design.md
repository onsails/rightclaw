# Bootstrap Rework & Agent Definition Restructure

**Supersedes:** `2026-04-08-bootstrap-and-reverse-sync-design.md` (bootstrap injection section only; reverse sync design remains valid)

## Problem

The current bootstrap approach embeds BOOTSTRAP.md content directly into the agent definition alongside IDENTITY, SOUL, USER, and AGENTS. This fails because:

1. CC treats bootstrap instructions as "nice context" competing with identity files, not as a mandatory first action.
2. The agent definition is generated at codegen time — file content becomes stale when CC modifies files inside the sandbox.
3. Template IDENTITY/SOUL/USER files dilute the bootstrap prompt. The agent sees pre-filled templates and responds conversationally instead of running the onboarding ritual.

## Design

### Core Idea

Two distinct CC invocation modes controlled by BOOTSTRAP.md file presence:

- **Bootstrap mode**: `--agent` with a minimal definition containing ONLY `@./BOOTSTRAP.md`. No identity files — bootstrap is the sole context. Special structured output with `bootstrap_complete` signal.
- **Normal mode**: `--agent` with a definition referencing `@./AGENTS.md`, `@./SOUL.md`, `@./IDENTITY.md`, `@./USER.md`, `@./TOOLS.md`. Standard reply schema.

### Agent Definition Format

Both agent definitions use CC's native `@` import syntax (verified working in agent def bodies — file content is expanded at session start and prompt-cached).

**Normal mode** (`.claude/agents/<name>.md`):

```markdown
---
name: <agent_name>
model: <model>
description: "RightClaw agent: <agent_name>"
---

@./AGENTS.md

---

@./SOUL.md

---

@./IDENTITY.md

---

@./USER.md

---

@./TOOLS.md
```

**Bootstrap mode** (`.claude/agents/<name>-bootstrap.md`):

```markdown
---
name: <agent_name>
model: <model>
description: "RightClaw agent bootstrap: <agent_name>"
---

@./BOOTSTRAP.md
```

### Injection Order (Normal Mode) — Cache-Optimized

| Position | File | Stability | Cache rationale |
|----------|------|-----------|----------------|
| 1 | AGENTS.md | Static — shipped template, agent appends rarely | Stable prefix anchor |
| 2 | SOUL.md | Semi-stable — created during bootstrap, evolves slowly | Rarely invalidates |
| 3 | IDENTITY.md | Semi-stable — created during bootstrap, evolves slowly | Rarely invalidates |
| 4 | USER.md | Dynamic — grows as agent learns about user | Only invalidates itself |
| 5 | TOOLS.md | Dynamic — regenerated on `up`/`reload`, agent may update | Last position, nothing after it |

Anthropic prompt caching is prefix-based. Static content first maximizes cache-hit rate across sessions. When USER.md changes between sessions, only TOOLS.md (after it) loses its cache — and TOOLS.md is last so nothing else is affected.

**Comparison with IronClaw:**

| Position | IronClaw | RightClaw | Notes |
|----------|----------|-----------|-------|
| 1 | AGENTS.md | AGENTS.md | Both: static operational procedures first |
| 2 | SOUL.md | SOUL.md | Both: personality early |
| 3 | USER.md | IDENTITY.md | We put IDENTITY before USER (IDENTITY changes less) |
| 4 | IDENTITY.md | USER.md | USER is more dynamic |
| 5 | TOOLS.md | TOOLS.md | Both: environment guidance last |
| 6 | MEMORY.md | *(not injected)* | CC manages memory natively |
| 7 | daily logs | *(not used)* | We use SQLite records via MCP |

### File Lifecycle

| File | Created by | When | Injected into CC |
|------|-----------|------|-----------------|
| `BOOTSTRAP.md` | `rightclaw init` | Init time | Bootstrap agent def via `@` (deleted after bootstrap) |
| `AGENTS.md` | `rightclaw init` | Init time | Normal agent def via `@` |
| `agent.yaml` | `rightclaw init` | Init time | Not injected (config only) |
| `IDENTITY.md` | Bootstrap CC session | First conversation | Normal agent def via `@` |
| `SOUL.md` | Bootstrap CC session | First conversation | Normal agent def via `@` |
| `USER.md` | Bootstrap CC session | First conversation | Normal agent def via `@` |
| `TOOLS.md` | Codegen pipeline | `rightclaw up`/`reload` | Normal agent def via `@` |
| Normal agent def | Codegen pipeline | `rightclaw up`/`reload` | `claude -p --agent <name>` |
| Bootstrap agent def | Codegen pipeline | `rightclaw up`/`reload` | `claude -p --agent <name>-bootstrap` |

### Init Changes

`rightclaw init` and `rightclaw agent init` create only:
- `BOOTSTRAP.md` — onboarding script template
- `AGENTS.md` — operational procedures template
- `agent.yaml` — configuration

They NO LONGER create:
- `IDENTITY.md` — created by bootstrap CC session
- `SOUL.md` — created by bootstrap CC session
- `USER.md` — created by bootstrap CC session

### AGENTS.md Template

Shipped by `rightclaw init`. Contains operational procedures the agent follows — not personality. Includes:

- **Identity file maintenance**: instructions to update USER.md passively, never interview the user
- **Two-layer memory**: CC native memory for conversation context, `right` MCP tools (`store_record`, `query_records`, `search_records`, `delete_record`) for structured/tagged data
- **MCP management**: `mcp_add`, `mcp_remove`, `mcp_list`, `mcp_auth` tools
- **Communication**: Telegram medium, markdown constraints, outbox for attachments
- **Cron management**: `/rightcron` skill usage
- **Core skills**: list of available skills
- **Subagents / task routing**: placeholder sections for user customization

### TOOLS.md

Generated by codegen pipeline (`rightclaw up`/`reload`). Contains environment-specific information:

- Sandbox mode and constraints
- Available MCP servers
- File paths (inbox/outbox directories)
- Network policy summary

Codegen writes the full file on every `up`/`reload` — it's a generated artifact, not user-editable. If the agent needs environment notes beyond what codegen provides, those belong in AGENTS.md (which the agent can append to).

### Bootstrap Structured Output

Bootstrap mode uses a modified reply schema with `bootstrap_complete` field:

```json
{
  "type": "object",
  "properties": {
    "content": { "type": ["string", "null"] },
    "bootstrap_complete": { "type": "boolean" },
    "reply_to_message_id": { "type": ["integer", "null"] },
    "attachments": { ... }
  },
  "required": ["content", "bootstrap_complete"]
}
```

Normal mode schema is unchanged (no `bootstrap_complete` field).

### Worker Logic

```
before invoke_cc():
  bootstrap_exists = agent_dir/BOOTSTRAP.md exists on disk
  if bootstrap_exists:
    agent_name_arg = "<name>-bootstrap"
    schema = bootstrap_schema (with bootstrap_complete)
  else:
    agent_name_arg = "<name>"
    schema = normal_schema

  # Session management unchanged:
  #   first message → --session-id <uuid>
  #   subsequent → --resume <session_id>

after invoke_cc() (when bootstrap_exists was true):
  parse bootstrap_complete from reply JSON
  if bootstrap_complete == true:
    reverse_sync .md files from sandbox (if sandboxed)
    check = [IDENTITY.md, SOUL.md, USER.md]
    missing = check.filter(|f| !agent_dir/f.exists())
    if missing:
      send warning to Telegram: "Bootstrap complete but missing: {missing}"
    delete session from DB → next message starts fresh in normal mode
    log: "bootstrap complete for agent <name>"
```

### Session Reset on Bootstrap Completion

When `bootstrap_complete: true` is received:

1. Delete the session row from `telegram_sessions` for this `(chat_id, eff_thread_id)`.
2. The next message will trigger `get_session() → Ok(None)` → new session UUID → `is_first_call = true`.
3. `is_first_call = true` causes `--agent <name>` (normal mode) to be passed.
4. CC loads the normal agent definition with `@` references to the newly created IDENTITY/SOUL/USER files.

### Sync Fixes

Add `.claude/agents/` directory to `sync_cycle` upload list in `crates/bot/src/sync.rs`. This ensures:
- Agent definitions reach the sandbox after `rightclaw reload`
- Both normal and bootstrap agent defs stay current

### Doctor Checks

New per-agent checks in `rightclaw doctor`:

| Check | Severity | Message |
|-------|----------|---------|
| IDENTITY.md exists and non-empty | Warning | "IDENTITY.md missing — run bootstrap or create manually" |
| SOUL.md exists and non-empty | Warning | "SOUL.md missing — run bootstrap or create manually" |
| USER.md exists | Warning | "USER.md missing — run bootstrap or create manually" |
| AGENTS.md exists | Error | "AGENTS.md missing — run `rightclaw init`" |
| BOOTSTRAP.md absent | Info | "Onboarding complete" |
| BOOTSTRAP.md present | Warning | "Agent hasn't completed onboarding — send a message to start" |

### Codegen Changes (`agent_def.rs`)

Replace `generate_agent_definition()` which reads and embeds file content with a function that generates a small `.md` file with `@` references:

```rust
pub fn generate_agent_definition(agent: &AgentDef) -> String {
    let model = agent.config.as_ref()
        .and_then(|c| c.model.as_deref())
        .unwrap_or("inherit");

    format!(
        "---\nname: {name}\nmodel: {model}\n\
         description: \"RightClaw agent: {name}\"\n---\n\n\
         @./AGENTS.md\n\n---\n\n\
         @./SOUL.md\n\n---\n\n\
         @./IDENTITY.md\n\n---\n\n\
         @./USER.md\n\n---\n\n\
         @./TOOLS.md\n",
        name = agent.name, model = model
    )
}

pub fn generate_bootstrap_definition(agent: &AgentDef) -> String {
    let model = agent.config.as_ref()
        .and_then(|c| c.model.as_deref())
        .unwrap_or("inherit");

    format!(
        "---\nname: {name}\nmodel: {model}\n\
         description: \"RightClaw agent bootstrap: {name}\"\n---\n\n\
         @./BOOTSTRAP.md\n",
        name = agent.name, model = model
    )
}
```

Both files written to `.claude/agents/<name>.md` and `.claude/agents/<name>-bootstrap.md`.

### Attachment Format Docs

The current `ATTACHMENT_FORMAT_DOCS` constant (message input/output format, attachment schema) moves into AGENTS.md. It's operational documentation (how to parse input, how to format output, size limits) — belongs in the shipped template alongside other session procedures.

### What Is NOT Changed

- Reverse sync (`sync.rs:reverse_sync_md`) — already implemented and wired into worker. Design from `2026-04-08` spec remains valid.
- Session management — `get_session()` / `create_session()` logic unchanged.
- Reply parsing — `parse_reply_output()` handles both schemas (bootstrap has extra field).
- MCP tools — unchanged.
- Cron integration — unchanged (cron runs in normal mode only, agents must be bootstrapped first).

### Files Changed

| File | Change |
|------|--------|
| `crates/rightclaw/src/codegen/agent_def.rs` | Replace embed-based generation with `@` reference generation. Add `generate_bootstrap_definition()`. Remove `ATTACHMENT_FORMAT_DOCS` constant. |
| `crates/rightclaw/src/codegen/agent_def_tests.rs` | Rewrite tests for new `@`-based output format |
| `crates/rightclaw/src/codegen/pipeline.rs` | Write both `<name>.md` and `<name>-bootstrap.md` agent defs. Generate TOOLS.md. |
| `crates/rightclaw/src/init.rs` | Remove IDENTITY.md, SOUL.md, USER.md from init. Keep BOOTSTRAP.md, AGENTS.md. |
| `crates/bot/src/telegram/worker.rs` | Bootstrap mode detection, agent name switching, schema switching, session reset on completion. |
| `crates/bot/src/sync.rs` | Add `.claude/agents/` to `sync_cycle` uploads |
| `crates/rightclaw/src/doctor.rs` | Add identity file checks per agent |
| `templates/right/AGENTS.md` | Update template with identity file maintenance section, attachment format docs |
| `templates/right/BOOTSTRAP.md` | Review and update bootstrap prompt (ensure it instructs agent to create IDENTITY.md, SOUL.md, USER.md) |
| `templates/right/TOOLS.md` | New template (or generated by codegen) |

### Testing

- **agent_def**: verify `generate_agent_definition()` outputs `@` references in correct order
- **agent_def**: verify `generate_bootstrap_definition()` outputs only `@./BOOTSTRAP.md`
- **worker**: test bootstrap mode detection (BOOTSTRAP.md exists → bootstrap agent name)
- **worker**: test session reset after `bootstrap_complete: true`
- **worker**: test warning when bootstrap complete but IDENTITY.md missing
- **doctor**: test all new identity file checks
- **init**: verify IDENTITY.md, SOUL.md, USER.md are NOT created
- **pipeline**: verify both agent def files are written
