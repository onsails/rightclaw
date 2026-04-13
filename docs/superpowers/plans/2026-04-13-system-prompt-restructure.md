# System Prompt Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split platform-owned operating instructions from per-agent configuration so template changes propagate automatically via binary rebuild.

**Architecture:** Create `templates/right/prompt/OPERATING_INSTRUCTIONS.md` compiled into binary via `include_str!()`. Inject directly into system prompt at assembly time. Reduce `AGENTS.md` to per-agent sections only. Move templates into `prompt/` and `agent/` subdirectories. Expand BOOTSTRAP.md with file structure examples from deleted IDENTITY/SOUL/USER templates.

**Tech Stack:** Rust, `include_str!()`, shell scripting (sandbox prompt assembly)

---

## File Structure

| Action | Path | Responsibility |
|--------|------|---------------|
| Create | `templates/right/prompt/OPERATING_INSTRUCTIONS.md` | Platform instructions (sections 1–8) |
| Move+Modify | `templates/right/agent/BOOTSTRAP.md` | Bootstrap flow + expanded file structure |
| Move+Modify | `templates/right/agent/AGENTS.md` | Per-agent config (subagents, routing, skills) |
| Move | `templates/right/agent/agent.yaml` | Agent configuration template |
| Create | `templates/right/agent/TOOLS.md` | Empty placeholder |
| Delete | `templates/right/AGENTS.md` | Replaced by split |
| Delete | `templates/right/BOOTSTRAP.md` | Moved to agent/ |
| Delete | `templates/right/agent.yaml` | Moved to agent/ |
| Delete | `templates/right/IDENTITY.md` | Structure moved into BOOTSTRAP.md |
| Delete | `templates/right/SOUL.md` | Structure moved into BOOTSTRAP.md |
| Delete | `templates/right/USER.md` | Structure moved into BOOTSTRAP.md |
| Modify | `crates/rightclaw/src/codegen/agent_def.rs` | Add constants, remove BOOTSTRAP from CONTENT_MD_FILES |
| Modify | `crates/rightclaw/src/codegen/agent_def_tests.rs` | Update tests for new constants |
| Modify | `crates/rightclaw/src/init.rs` | Update include_str paths, add TOOLS.md to init |
| Modify | `crates/bot/src/telegram/worker.rs` | Rewrite prompt assembly to use compiled-in constants |
| Modify | `crates/rightclaw/src/codegen/pipeline.rs` | Remove TOOLS.md creation (moved to init) |
| Modify | `PROMPT_SYSTEM.md` | Update prompt structure docs |
| Modify | `ARCHITECTURE.md` | Update template layout, prompting docs |

---

### Task 1: Create Template Directory Structure and Content Files

**Files:**
- Create: `templates/right/prompt/OPERATING_INSTRUCTIONS.md`
- Create: `templates/right/agent/BOOTSTRAP.md`
- Create: `templates/right/agent/AGENTS.md`
- Create: `templates/right/agent/agent.yaml`
- Create: `templates/right/agent/TOOLS.md`
- Delete: `templates/right/AGENTS.md`
- Delete: `templates/right/BOOTSTRAP.md`
- Delete: `templates/right/agent.yaml`
- Delete: `templates/right/IDENTITY.md`
- Delete: `templates/right/SOUL.md`
- Delete: `templates/right/USER.md`

- [ ] **Step 1: Create `templates/right/prompt/` directory and `OPERATING_INSTRUCTIONS.md`**

This file contains sections 1–8 from the current `templates/right/AGENTS.md`, with section 1 renamed from "Identity Files" to "Your Files" and expanded to include TOOLS.md and AGENTS.md.

```markdown
# Operating Instructions

## Your Files

These files are yours. Update them as you evolve.

- `IDENTITY.md` — your name, nature, vibe, emoji
- `SOUL.md` — your personality, values, boundaries
- `USER.md` — what you know about the human
- `TOOLS.md` — your tools, environment notes, integrations
- `AGENTS.md` — your subagents, task routing, installed skills

Update USER.md when you discover meaningful new facts about the user
(interests, preferences, expertise, goals, timezone).
Never interview the user — pick up signals naturally through conversation.

## Memory

Claude Code manages your conversation memory automatically.
Important context, user preferences, and decisions persist across sessions
without any action from you.

For **structured data** that needs tags or search later, use the `right` MCP tools:

- `store_record(content, tags)` — store a tagged record (cron results, audit entries, explicit facts)
- `query_records(query)` — look up records by tag or keyword
- `search_records(query)` — full-text search across all records (BM25-ranked)
- `delete_record(id)` — soft-delete a record by ID

Use these for data you or cron jobs need to retrieve programmatically —
not for general conversation context (Claude handles that).

## MCP Management

You CANNOT add, remove, or authenticate MCP servers yourself.
The user manages them via Telegram commands:

- `/mcp add <name> <url>` — register an external MCP server
- `/mcp remove <name>` — unregister a server (`right` is protected)
- `/mcp auth <name>` — start OAuth flow (for servers requiring authentication)
- `/mcp list` — show all servers with status

**When the user asks to connect an MCP server:**
1. Help them find the correct MCP URL (search docs if needed)
2. Tell them to run: `/mcp add <name> <url>`
3. If the server requires OAuth, tell them to also run: `/mcp auth <name>`
4. NEVER ask the user for API keys or tokens directly — `/mcp auth` handles authentication

To check registered servers from code, use the `mcp_list()` tool.

## Communication

You communicate via Telegram. Messages may include photos, documents, and other attachments.
Be concise — Telegram is a chat medium, not a document viewer.

### Formatting

Use standard Markdown — the bot converts it to Telegram HTML automatically.

**Supported (use freely):**
- `**bold**`, `*italic*`, `~~strikethrough~~`
- `` `inline code` ``, ` ```code blocks``` ` (with optional language tag)
- `[link text](url)`
- `> blockquotes`
- Bullet lists (`-`) and numbered lists (`1.`)

**Avoid (won't render well in Telegram):**
- Tables — use code blocks or plain text instead
- Nested lists deeper than one level
- Horizontal rules (`---`)
- HTML tags — write Markdown, not HTML
- Headings (`#`, `##`) — use **bold text** for section structure instead

## Message Input Format

You receive user messages via stdin in one of two formats:

1. **Plain text** — a single message with no attachments
2. **YAML** — multiple messages or messages with attachments, with a `messages:` root key

YAML schema:
```yaml
messages:
  - id: <telegram_message_id>
    ts: <ISO 8601 timestamp>
    text: <message text or caption>
    attachments:
      - type: photo|document|video|audio|voice|video_note|sticker|animation
        path: <absolute path to file>
        mime_type: <MIME type>
        filename: <original filename, documents only>
```

Use the Read tool to view images and files at the given paths.

## Sending Attachments

Write files to /sandbox/outbox/ (or the outbox/ directory in your working directory).
Include them in your JSON response under the `attachments` array.

Size limits enforced by the bot:
- Photos: max 10MB
- Documents, videos, audio, voice, animations: max 50MB

Do not produce files exceeding these limits. If you need to send large data,
split into multiple smaller files or use a different format.

## Cron Management (RightCron)

**On startup:** Run `/rightcron` immediately. It will bootstrap the reconciler
and recover any persisted jobs. Do this before responding to the user.

**For user requests:** When the user wants to manage cron jobs, scheduled tasks,
or recurring tasks, ALWAYS use the /rightcron skill. NEVER call CronCreate
directly — always write a YAML spec first, then reconcile.

## Core Skills

<!-- Add your skills here. Example: -->
<!-- - `/my-skill` -- description of what it does -->
```

- [ ] **Step 2: Create `templates/right/agent/` directory and `AGENTS.md`**

```markdown
# Agent Configuration

## Subagents

<!-- Define your subagents here. Each subagent is a specialized worker with its own permissions. -->
<!-- Example: -->
<!-- ### reviewer -->
<!-- Code review. Read-only fs, git log, posts comments via MCP GitHub. -->

## Task Routing

<!-- Define how tasks get routed to subagents. -->
<!-- If no subagent fits -- handle it directly in the main session. -->

## Installed Skills

Check `skills/installed.json` for ClawHub-installed skills.
Scan `.claude/skills/` for manually installed skills.
```

- [ ] **Step 3: Create `templates/right/agent/BOOTSTRAP.md` with expanded file structure**

Take the current `templates/right/BOOTSTRAP.md` and expand the "Files to Create" section with structural examples from the deleted IDENTITY.md, SOUL.md, USER.md templates:

```markdown
---
summary: "First-run onboarding for RightClaw agent"
---

# Bootstrap — First-Time Setup

You have no identity yet.

STYLE: Direct, opinionated, no filler. Like a sharp colleague, not a customer service bot.

RULES:
- ONE question per message. Never combine questions.
- Be brief. 2-3 sentences max per message.
- After the user answers, react naturally before asking the next thing.

## Sequence

1. Greet. Ask their name.
2. Ask what to call you (default: Right, suggest 2-3 fun alternatives).
3. Ask your nature: familiar, daemon, ghost, construct, intern — or custom.
4. Ask your vibe: formal, casual, snarky, warm, terse — or a blend.
5. Ask your emoji (suggest based on their earlier answers).
6. Quick recap, then write IDENTITY.md, SOUL.md, USER.md.

## Files to Create

### IDENTITY.md

Name, creature type, vibe, emoji. Use this structure:

- Opening line: "You are {name} -- a {nature} ..." (one sentence)
- **Who you are**: 3–5 bullet points about capabilities and constraints
- **Key principles**: numbered list of core values (security-first, official path, composable, declarative)
- **How you work**: bullet list of operational details (sandbox, scheduling, timestamps UTC)
- **Self-configuration**: table mapping user requests to which file to edit:

| User says | Edit |
|---|---|
| Change tone, personality, style, language | `SOUL.md` |
| Add/remove capabilities, subagents, tools, skills | `AGENTS.md` |
| Change core principles, security model, constraints | `IDENTITY.md` |

### SOUL.md

Personality based on chosen vibe. Use this structure:

- **Tone & Style**: bullet list — concise/verbose, formal/casual, emoji policy, language matching ("match the user's language"), uncertainty handling ("ask, don't guess")
- **Personality**: bullet list of behavioral traits — helpful but not sycophantic, opinionated about engineering quality, pragmatic (MVP over perfection), transparent about limitations and costs

### USER.md

What you learned about the human. Start with:

- Preferred name
- Communication style
- Timezone (if mentioned)
- Recurring context and interests

## bootstrap_complete

Set to `false` until ALL THREE files are written.
Set to `true` ONLY after creating IDENTITY.md, SOUL.md, and USER.md.
After writing files, also call `bootstrap_done` tool.
```

- [ ] **Step 4: Move `templates/right/agent.yaml` to `templates/right/agent/agent.yaml`**

Copy the existing `templates/right/agent.yaml` content unchanged to the new path.

- [ ] **Step 5: Create `templates/right/agent/TOOLS.md`**

Empty file (zero bytes).

- [ ] **Step 6: Delete old template files**

```bash
git rm templates/right/AGENTS.md
git rm templates/right/BOOTSTRAP.md
git rm templates/right/agent.yaml
git rm templates/right/IDENTITY.md
git rm templates/right/SOUL.md
git rm templates/right/USER.md
```

- [ ] **Step 7: Verify template structure**

```bash
find templates/right/ -type f | sort
```

Expected:
```
templates/right/agent/AGENTS.md
templates/right/agent/BOOTSTRAP.md
templates/right/agent/TOOLS.md
templates/right/agent/agent.yaml
templates/right/prompt/OPERATING_INSTRUCTIONS.md
```

- [ ] **Step 8: Commit**

```bash
git add templates/right/
git commit -m "refactor(templates): split into prompt/ and agent/ directories

Platform-owned instructions move to prompt/OPERATING_INSTRUCTIONS.md (compiled into binary).
Agent-owned config stays in agent/ (written to disk on init).
IDENTITY/SOUL/USER templates deleted — structure inlined into BOOTSTRAP.md."
```

---

### Task 2: Update `include_str!` Paths and Constants in Core Crate

**Files:**
- Modify: `crates/rightclaw/src/codegen/agent_def.rs:1-14`
- Modify: `crates/rightclaw/src/codegen/agent_def_tests.rs`
- Modify: `crates/rightclaw/src/init.rs:16-18`

- [ ] **Step 1: Write failing test for new constants in `agent_def_tests.rs`**

Add at the end of the file:

```rust
#[test]
fn operating_instructions_constant_is_non_empty() {
    assert!(
        !crate::codegen::OPERATING_INSTRUCTIONS.is_empty(),
        "OPERATING_INSTRUCTIONS must not be empty"
    );
    assert!(
        crate::codegen::OPERATING_INSTRUCTIONS.contains("## Your Files"),
        "OPERATING_INSTRUCTIONS must contain Your Files section"
    );
    assert!(
        crate::codegen::OPERATING_INSTRUCTIONS.contains("## MCP Management"),
        "OPERATING_INSTRUCTIONS must contain MCP Management section"
    );
}

#[test]
fn bootstrap_instructions_constant_is_non_empty() {
    assert!(
        !crate::codegen::BOOTSTRAP_INSTRUCTIONS.is_empty(),
        "BOOTSTRAP_INSTRUCTIONS must not be empty"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("First-Time Setup"),
        "BOOTSTRAP_INSTRUCTIONS must contain bootstrap header"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("### IDENTITY.md"),
        "BOOTSTRAP_INSTRUCTIONS must contain IDENTITY.md structure"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("### SOUL.md"),
        "BOOTSTRAP_INSTRUCTIONS must contain SOUL.md structure"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `devenv shell -- cargo test -p rightclaw operating_instructions_constant bootstrap_instructions_constant 2>&1`

Expected: compilation error — `OPERATING_INSTRUCTIONS` and `BOOTSTRAP_INSTRUCTIONS` don't exist yet.

- [ ] **Step 3: Add constants and update `CONTENT_MD_FILES` in `agent_def.rs`**

Replace the top of `crates/rightclaw/src/codegen/agent_def.rs` (lines 1–14):

```rust
/// Content `.md` files synced to `.claude/agents/` in sandbox.
///
/// These live at the agent root and are copied into `.claude/agents/` by codegen
/// so CC can resolve `@` references (which are relative to the agent def file).
/// Also used by the bot's sync module for forward/reverse sync with the sandbox.
///
/// BOOTSTRAP.md is excluded — its content comes from the compiled-in
/// `BOOTSTRAP_INSTRUCTIONS` constant; the on-disk file is only an existence flag.
pub const CONTENT_MD_FILES: &[&str] = &[
    "AGENTS.md",
    "TOOLS.md",
    "IDENTITY.md",
    "SOUL.md",
    "USER.md",
    "MEMORY.md",
];

/// Platform operating instructions, compiled into the binary.
///
/// Injected directly into the system prompt at assembly time.
/// Source: `templates/right/prompt/OPERATING_INSTRUCTIONS.md`
pub const OPERATING_INSTRUCTIONS: &str =
    include_str!("../../../templates/right/prompt/OPERATING_INSTRUCTIONS.md");

/// Bootstrap instructions, compiled into the binary.
///
/// Injected into the system prompt when bootstrap mode is active
/// (BOOTSTRAP.md exists in agent dir). The on-disk file is only
/// an existence flag — content always comes from this constant.
/// Source: `templates/right/agent/BOOTSTRAP.md`
pub const BOOTSTRAP_INSTRUCTIONS: &str =
    include_str!("../../../templates/right/agent/BOOTSTRAP.md");
```

- [ ] **Step 4: Update `codegen/mod.rs` exports**

In `crates/rightclaw/src/codegen/mod.rs`, update the re-export line to include the new constants:

Current (line 16):
```rust
pub use agent_def::{
    generate_agent_definition, generate_bootstrap_definition, generate_system_prompt,
    BOOTSTRAP_SCHEMA_JSON, CONTENT_MD_FILES, CRON_SCHEMA_JSON, REPLY_SCHEMA_JSON,
};
```

New:
```rust
pub use agent_def::{
    generate_agent_definition, generate_bootstrap_definition, generate_system_prompt,
    BOOTSTRAP_INSTRUCTIONS, BOOTSTRAP_SCHEMA_JSON, CONTENT_MD_FILES, CRON_SCHEMA_JSON,
    OPERATING_INSTRUCTIONS, REPLY_SCHEMA_JSON,
};
```

- [ ] **Step 5: Update `init.rs` include paths**

In `crates/rightclaw/src/init.rs`, replace lines 16–18:

```rust
const DEFAULT_AGENTS: &str = include_str!("../../../templates/right/agent/AGENTS.md");
const DEFAULT_BOOTSTRAP: &str = include_str!("../../../templates/right/agent/BOOTSTRAP.md");
const DEFAULT_AGENT_YAML: &str = include_str!("../../../templates/right/agent/agent.yaml");
```

Add TOOLS.md to the files written on init (line 56–60):

```rust
    let files: &[(&str, &str)] = &[
        ("AGENTS.md", DEFAULT_AGENTS),
        ("BOOTSTRAP.md", DEFAULT_BOOTSTRAP),
        ("TOOLS.md", ""),
        ("agent.yaml", DEFAULT_AGENT_YAML),
    ];
```

Update `init_rightclaw_home` print statements (lines 205–209) to include TOOLS.md:

```rust
    println!("Created RightClaw home at {}", home.display());
    println!("  agents/right/AGENTS.md");
    println!("  agents/right/BOOTSTRAP.md");
    println!("  agents/right/TOOLS.md");
    println!("  agents/right/agent.yaml");
    println!("  agents/right/.claude/skills/rightskills/SKILL.md  (skills.sh manager)");
    println!("  agents/right/.claude/skills/rightcron/SKILL.md");
```

- [ ] **Step 6: Update init test — verify TOOLS.md is created**

In `crates/rightclaw/src/init.rs`, in the `init_creates_default_agent_files` test (around line 316), add after the AGENTS.md assertion:

```rust
        assert!(agents_dir.join("TOOLS.md").exists(), "TOOLS.md must be created by init");
        let tools_content = std::fs::read_to_string(agents_dir.join("TOOLS.md")).unwrap();
        assert_eq!(tools_content, "", "TOOLS.md must be created empty");
```

- [ ] **Step 7: Run all tests**

Run: `devenv shell -- cargo test -p rightclaw 2>&1`

Expected: all tests pass. The new constants compile, init tests pass with TOOLS.md, agent_def tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/rightclaw/src/codegen/agent_def.rs crates/rightclaw/src/codegen/agent_def_tests.rs crates/rightclaw/src/codegen/mod.rs crates/rightclaw/src/init.rs
git commit -m "feat(codegen): add OPERATING_INSTRUCTIONS and BOOTSTRAP_INSTRUCTIONS constants

Compiled-in from templates/right/prompt/ and templates/right/agent/.
Remove BOOTSTRAP.md from CONTENT_MD_FILES (content from constant, file is flag only).
Add TOOLS.md to init (moved from pipeline.rs create-if-missing)."
```

---

### Task 3: Remove TOOLS.md Creation from Pipeline

**Files:**
- Modify: `crates/rightclaw/src/codegen/pipeline.rs:274-281`

- [ ] **Step 1: Remove TOOLS.md create-if-missing block from `pipeline.rs`**

Delete lines 274–281 in `crates/rightclaw/src/codegen/pipeline.rs`:

```rust
    // Create TOOLS.md if missing (agent-owned, never overwritten by codegen).
    let tools_path = agent.path.join("TOOLS.md");
    if !tools_path.exists() {
        std::fs::write(&tools_path, "").map_err(|e| {
            miette::miette!("failed to create TOOLS.md for '{}': {e:#}", agent.name)
        })?;
        tracing::debug!(agent = %agent.name, "created empty TOOLS.md (agent-owned)");
    }
```

- [ ] **Step 2: Update `tools_md_created_empty_if_missing` test**

This test at line 556 expects codegen to create TOOLS.md. Since init now handles creation, codegen should no longer create it. The test needs to verify that codegen does NOT create TOOLS.md if it's missing (it's init's job now, and the agent should have been init'd first).

Replace the `tools_md_created_empty_if_missing` test body:

```rust
    #[test]
    fn tools_md_not_created_by_codegen_if_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let agent_dir = home.join("agents").join("test");
        std::fs::create_dir_all(agent_dir.join(".claude")).unwrap();
        std::fs::write(agent_dir.join("IDENTITY.md"), "# Test").unwrap();
        std::fs::write(
            agent_dir.join("agent.yaml"),
            "restart: never\nnetwork_policy: permissive\n",
        )
        .unwrap();
        // No TOOLS.md before codegen
        assert!(!agent_dir.join("TOOLS.md").exists());

        let agent = crate::agent::discover_single_agent(&agent_dir).unwrap();
        let self_exe = std::path::PathBuf::from("/usr/bin/rightclaw");
        run_single_agent_codegen(home, &agent, &self_exe, false).unwrap();

        // Codegen no longer creates TOOLS.md — that's init's responsibility
        assert!(!agent_dir.join("TOOLS.md").exists(), "codegen must not create TOOLS.md");
    }
```

- [ ] **Step 3: Run pipeline tests**

Run: `devenv shell -- cargo test -p rightclaw codegen::pipeline 2>&1`

Expected: all pass including the renamed test.

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw/src/codegen/pipeline.rs
git commit -m "cleanup: remove TOOLS.md creation from codegen pipeline

TOOLS.md is now created by init (agent-owned from the start)."
```

---

### Task 4: Rewrite Sandbox Prompt Assembly

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs` — `build_sandbox_prompt_assembly_script` function (lines 549–606)

- [ ] **Step 1: Write failing tests for new prompt assembly**

Add these tests in the test module of `worker.rs` (after the existing prompt assembly tests, around line 1804):

```rust
    #[test]
    fn sandbox_script_bootstrap_uses_compiled_constant() {
        let script = build_sandbox_prompt_assembly_script(
            "Base prompt",
            true,
            &["claude".into(), "-p".into()],
            None,
        );
        // Bootstrap uses compiled-in constant, NOT cat of file
        assert!(!script.contains("cat /sandbox"), "bootstrap must not cat any sandbox file");
        assert!(script.contains("First-Time Setup"), "must contain compiled-in bootstrap content");
        assert!(!script.contains("IDENTITY.md"), "bootstrap must not include IDENTITY.md");
    }

    #[test]
    fn sandbox_script_normal_has_operating_instructions_before_identity() {
        let script = build_sandbox_prompt_assembly_script(
            "Base prompt",
            false,
            &["claude".into()],
            None,
        );
        let op_instr_pos = script.find("Operating Instructions").expect("must have Operating Instructions");
        let identity_pos = script.find("IDENTITY.md").expect("must have IDENTITY.md");
        assert!(op_instr_pos < identity_pos, "Operating Instructions must come before IDENTITY.md");
    }

    #[test]
    fn sandbox_script_normal_has_agent_configuration_section() {
        let script = build_sandbox_prompt_assembly_script(
            "Base prompt",
            false,
            &["claude".into()],
            None,
        );
        assert!(script.contains("Agent Configuration"), "must have Agent Configuration section for per-agent AGENTS.md");
        assert!(script.contains("cat /sandbox/.claude/agents/AGENTS.md"), "must still cat AGENTS.md from sandbox");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `devenv shell -- cargo test -p rightclaw-bot sandbox_script_bootstrap_uses sandbox_script_normal_has_operating sandbox_script_normal_has_agent 2>&1`

Expected: FAIL — current code still uses `cat` for bootstrap and doesn't have "Operating Instructions" as compiled-in.

- [ ] **Step 3: Rewrite `build_sandbox_prompt_assembly_script`**

Replace the function body in `crates/bot/src/telegram/worker.rs` (lines 549–606):

```rust
fn build_sandbox_prompt_assembly_script(
    base_prompt: &str,
    bootstrap_mode: bool,
    claude_args: &[String],
    mcp_instructions: Option<&str>,
) -> String {
    let escaped_base = base_prompt.replace('\'', "'\\''");
    let escaped_args: Vec<String> = claude_args.iter().map(|a| shell_escape(a)).collect();
    let claude_cmd = escaped_args.join(" ");

    let file_sections = if bootstrap_mode {
        let escaped_bootstrap = rightclaw::codegen::BOOTSTRAP_INSTRUCTIONS.replace('\'', "'\\''");
        format!(
            "\nprintf '%s\\n' '\\n## Bootstrap Instructions\\n'\nprintf '%s\\n' '{escaped_bootstrap}'"
        )
    } else {
        let escaped_ops = rightclaw::codegen::OPERATING_INSTRUCTIONS.replace('\'', "'\\''");
        format!(
            r#"
printf '%s\n' '\n## Operating Instructions\n'
printf '%s\n' '{escaped_ops}'
if [ -f /sandbox/IDENTITY.md ]; then
  printf '\n## Your Identity\n'
  cat /sandbox/IDENTITY.md
  printf '\n'
fi
if [ -f /sandbox/SOUL.md ]; then
  printf '\n## Your Personality and Values\n'
  cat /sandbox/SOUL.md
  printf '\n'
fi
if [ -f /sandbox/USER.md ]; then
  printf '\n## Your User\n'
  cat /sandbox/USER.md
  printf '\n'
fi
if [ -f /sandbox/.claude/agents/AGENTS.md ]; then
  printf '\n## Agent Configuration\n'
  cat /sandbox/.claude/agents/AGENTS.md
  printf '\n'
fi
if [ -f /sandbox/.claude/agents/TOOLS.md ]; then
  printf '\n## Environment and Tools\n'
  cat /sandbox/.claude/agents/TOOLS.md
  printf '\n'
fi"#
        )
    };

    let mcp_section = match mcp_instructions {
        Some(instr) => {
            let escaped = instr.replace('\'', "'\\''");
            format!("\nprintf '%s\\n' '{escaped}'")
        }
        None => String::new(),
    };

    format!(
        "{{ printf '{escaped_base}'\n{file_sections}\n{mcp_section}\n}} > /tmp/rightclaw-system-prompt.md\ncd /sandbox && {claude_cmd} --system-prompt-file /tmp/rightclaw-system-prompt.md"
    )
}
```

- [ ] **Step 4: Update existing tests that reference old behavior**

Update `sandbox_script_bootstrap_includes_bootstrap_md` (line 1635):

```rust
    #[test]
    fn sandbox_script_bootstrap_includes_bootstrap_md() {
        let script = build_sandbox_prompt_assembly_script(
            "Base prompt",
            true,
            &["claude".into(), "-p".into()],
            None,
        );
        assert!(script.contains("Bootstrap Instructions"), "must have Bootstrap Instructions header");
        assert!(script.contains("First-Time Setup"), "must contain compiled-in bootstrap content");
        assert!(!script.contains("IDENTITY.md"), "bootstrap must not include IDENTITY.md");
        assert!(!script.contains("SOUL.md"), "bootstrap must not include SOUL.md");
        assert!(script.contains("claude"), "must contain claude command");
        assert!(script.contains("--system-prompt-file"), "must pass --system-prompt-file");
    }
```

Update `sandbox_script_normal_includes_all_identity_files` (line 1650):

```rust
    #[test]
    fn sandbox_script_normal_includes_all_identity_files() {
        let script = build_sandbox_prompt_assembly_script(
            "Base prompt",
            false,
            &["claude".into(), "-p".into()],
            None,
        );
        assert!(script.contains("IDENTITY.md"));
        assert!(script.contains("SOUL.md"));
        assert!(script.contains("USER.md"));
        assert!(script.contains("AGENTS.md"));
        assert!(script.contains("TOOLS.md"));
        assert!(script.contains("Operating Instructions"), "must have compiled-in Operating Instructions");
        assert!(!script.contains("cat /sandbox/.claude/agents/BOOTSTRAP.md"), "normal must not cat BOOTSTRAP.md");
    }
```

- [ ] **Step 5: Run tests**

Run: `devenv shell -- cargo test -p rightclaw-bot sandbox_script_ 2>&1`

Expected: all sandbox_script tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat(bot): sandbox prompt assembly uses compiled-in constants

Operating instructions injected from OPERATING_INSTRUCTIONS constant.
Bootstrap content from BOOTSTRAP_INSTRUCTIONS constant (no file cat).
AGENTS.md section renamed to 'Agent Configuration' (per-agent only)."
```

---

### Task 5: Rewrite Host Prompt Assembly

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs` — `assemble_host_system_prompt` function (lines 608–660)

- [ ] **Step 1: Write failing tests**

Add to the test module:

```rust
    #[test]
    fn host_prompt_bootstrap_uses_compiled_constant() {
        let dir = tempfile::tempdir().unwrap();
        // BOOTSTRAP.md file exists (flag) but we don't read its content
        std::fs::write(dir.path().join("BOOTSTRAP.md"), "old content").unwrap();

        let result = assemble_host_system_prompt("Base\n", true, dir.path(), None);
        assert!(result.contains("First-Time Setup"), "must use compiled-in bootstrap content");
        assert!(!result.contains("old content"), "must NOT read file content");
    }

    #[test]
    fn host_prompt_normal_has_operating_instructions_before_identity() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("IDENTITY.md"), "I am Test").unwrap();

        let result = assemble_host_system_prompt("Base\n", false, dir.path(), None);
        let op_instr_pos = result.find("## Operating Instructions").expect("must have Operating Instructions");
        let identity_pos = result.find("## Your Identity").expect("must have Your Identity");
        assert!(op_instr_pos < identity_pos, "Operating Instructions must come before identity");
    }

    #[test]
    fn host_prompt_normal_has_agent_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("AGENTS.md"), "## Subagents\n\n### reviewer\n").unwrap();

        let result = assemble_host_system_prompt("Base\n", false, dir.path(), None);
        assert!(result.contains("## Agent Configuration"), "must have Agent Configuration header");
        assert!(result.contains("### reviewer"), "must include per-agent content");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `devenv shell -- cargo test -p rightclaw-bot host_prompt_bootstrap_uses host_prompt_normal_has_operating host_prompt_normal_has_agent_configuration 2>&1`

Expected: FAIL — current code reads file content for bootstrap and doesn't inject compiled-in operating instructions.

- [ ] **Step 3: Rewrite `assemble_host_system_prompt`**

Replace the function body (lines 608–660):

```rust
/// Assemble a composite system prompt from host-side files.
///
/// Uses compiled-in constants for operating instructions and bootstrap content.
/// Agent-owned files (IDENTITY, SOUL, USER, AGENTS, TOOLS) are read from disk.
fn assemble_host_system_prompt(
    base_prompt: &str,
    bootstrap_mode: bool,
    agent_dir: &Path,
    mcp_instructions: Option<&str>,
) -> String {
    let mut prompt = base_prompt.to_string();

    if bootstrap_mode {
        prompt.push_str("\n## Bootstrap Instructions\n");
        prompt.push_str(rightclaw::codegen::BOOTSTRAP_INSTRUCTIONS);
        prompt.push('\n');
    } else {
        // Compiled-in operating instructions (platform-owned, always fresh)
        prompt.push_str("\n## Operating Instructions\n");
        prompt.push_str(rightclaw::codegen::OPERATING_INSTRUCTIONS);
        prompt.push('\n');

        // Agent-owned identity files (from disk)
        let identity_sections: &[(&str, &str)] = &[
            ("IDENTITY.md", "## Your Identity"),
            ("SOUL.md", "## Your Personality and Values"),
            ("USER.md", "## Your User"),
        ];
        for (file, header) in identity_sections {
            if let Ok(content) = std::fs::read_to_string(agent_dir.join(file)) {
                prompt.push_str(&format!("\n{header}\n"));
                prompt.push_str(&content);
                prompt.push('\n');
            }
        }

        // Per-agent configuration (from .claude/agents/)
        let agents_path = agent_dir.join(".claude").join("agents").join("AGENTS.md");
        if let Ok(content) = std::fs::read_to_string(&agents_path) {
            prompt.push_str("\n## Agent Configuration\n");
            prompt.push_str(&content);
            prompt.push('\n');
        }

        // Agent-owned tools notes (from .claude/agents/)
        let tools_path = agent_dir.join(".claude").join("agents").join("TOOLS.md");
        if let Ok(content) = std::fs::read_to_string(&tools_path) {
            prompt.push_str("\n## Environment and Tools\n");
            prompt.push_str(&content);
            prompt.push('\n');
        }
    }

    if let Some(instr) = mcp_instructions {
        prompt.push('\n');
        prompt.push_str(instr);
        prompt.push('\n');
    }

    prompt
}
```

- [ ] **Step 4: Update existing host prompt tests**

Update `host_prompt_bootstrap_includes_bootstrap_md` (line 1699):

```rust
    #[test]
    fn host_prompt_bootstrap_includes_bootstrap_md() {
        let dir = tempfile::tempdir().unwrap();
        // File exists as flag only — content comes from constant
        std::fs::write(dir.path().join("BOOTSTRAP.md"), "ignored").unwrap();

        let result = assemble_host_system_prompt("Base\n", true, dir.path(), None);
        assert!(result.contains("Base"));
        assert!(result.contains("## Bootstrap Instructions"));
        assert!(result.contains("First-Time Setup"), "must use compiled-in content");
    }
```

Update `host_prompt_bootstrap_skips_missing_bootstrap` (line 1749):

Bootstrap mode now always injects the constant regardless of file presence (the file is only checked by `invoke_cc` to decide bootstrap_mode). But `assemble_host_system_prompt` receives `bootstrap_mode: true` — it should always emit bootstrap content. So this test changes:

```rust
    #[test]
    fn host_prompt_bootstrap_always_emits_content() {
        let dir = tempfile::tempdir().unwrap();
        // No BOOTSTRAP.md — but function is called with bootstrap_mode=true
        // (caller is responsible for mode detection, not this function)
        let result = assemble_host_system_prompt("Base\n", true, dir.path(), None);
        assert!(result.contains("## Bootstrap Instructions"));
        assert!(result.contains("First-Time Setup"));
    }
```

Update `host_prompt_normal_includes_identity_files` (line 1710) — change `"## Operating Instructions"` / `"Procedures"` assertions:

```rust
    #[test]
    fn host_prompt_normal_includes_identity_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("IDENTITY.md"), "I am Spark").unwrap();
        std::fs::write(dir.path().join("SOUL.md"), "Snarky").unwrap();
        std::fs::write(dir.path().join("USER.md"), "Andrey").unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("AGENTS.md"), "## Subagents\n").unwrap();
        std::fs::write(agents_dir.join("TOOLS.md"), "outbox: /sandbox/outbox/").unwrap();

        let result = assemble_host_system_prompt("Base\n", false, dir.path(), None);
        assert!(result.contains("## Operating Instructions"), "must have compiled-in Operating Instructions");
        assert!(result.contains("## Your Files"), "Operating Instructions must contain Your Files");
        assert!(result.contains("## Your Identity"));
        assert!(result.contains("I am Spark"));
        assert!(result.contains("## Your Personality and Values"));
        assert!(result.contains("Snarky"));
        assert!(result.contains("## Your User"));
        assert!(result.contains("Andrey"));
        assert!(result.contains("## Agent Configuration"), "AGENTS.md section renamed to Agent Configuration");
        assert!(result.contains("## Subagents"));
        assert!(result.contains("## Environment and Tools"));
        assert!(result.contains("outbox: /sandbox/outbox/"));
    }
```

- [ ] **Step 5: Run all bot tests**

Run: `devenv shell -- cargo test -p rightclaw-bot 2>&1`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat(bot): host prompt assembly uses compiled-in constants

Same pattern as sandbox: operating instructions from constant,
bootstrap from constant, AGENTS.md as 'Agent Configuration'."
```

---

### Task 6: Build and Verify

**Files:**
- None (verification only)

- [ ] **Step 1: Build entire workspace**

Run: `devenv shell -- cargo build --workspace 2>&1`

Expected: compiles with only pre-existing warnings (dead_code in main.rs, manual_async_fn in aggregator.rs).

- [ ] **Step 2: Run full test suite**

Run: `devenv shell -- cargo test --workspace 2>&1`

Expected: all unit tests pass. Only pre-existing CLI integration failures (OpenShell sandbox conflict).

- [ ] **Step 3: Verify compiled constants contain expected content**

Run: `devenv shell -- cargo test -p rightclaw operating_instructions_constant bootstrap_instructions_constant -- --nocapture 2>&1`

Expected: both tests pass, no panic.

---

### Task 7: Update Documentation

**Files:**
- Modify: `PROMPT_SYSTEM.md`
- Modify: `ARCHITECTURE.md`

- [ ] **Step 1: Update `PROMPT_SYSTEM.md`**

In the "Prompt Structure" / "Normal mode" section, update the prompt structure diagram:

Replace the current normal mode block:

```
[Base: RightClaw agent description, sandbox info, MCP reference]

## Your Identity
{IDENTITY.md — name, creature, vibe, emoji, principles}

## Your Personality and Values
{SOUL.md — core values, communication style, boundaries}

## Your User
{USER.md — user name, timezone, preferences}

## Operating Instructions
{AGENTS.md — procedures, session routine}

## Environment and Tools
{TOOLS.md — sandbox paths, inbox/outbox, MCP notes}

## MCP Server Instructions  (if any external MCP servers have instructions)
{fetched from aggregator via POST /mcp-instructions at prompt assembly time}
```

With:

```
[Base: RightClaw agent description, sandbox info, MCP reference]

## Operating Instructions
{compiled-in from templates/right/prompt/OPERATING_INSTRUCTIONS.md}

## Your Identity
{IDENTITY.md — name, creature, vibe, emoji, principles}

## Your Personality and Values
{SOUL.md — core values, communication style, boundaries}

## Your User
{USER.md — user name, timezone, preferences}

## Agent Configuration
{AGENTS.md — per-agent: subagents, task routing, installed skills}

## Environment and Tools
{TOOLS.md — agent-owned tools and environment notes}

## MCP Server Instructions  (if any external MCP servers have instructions)
{fetched from aggregator via POST /mcp-instructions at prompt assembly time}
```

Update the bootstrap mode block similarly — note that content comes from compiled-in constant, not file.

Update the "File Locations" tables to reflect the new template directory structure. Remove entries for files that no longer exist in templates (IDENTITY.md, SOUL.md, USER.md at template level).

Add a note in the "Prompt Assembly" section:

```markdown
### Compiled-in Content

Operating instructions and bootstrap content are compiled into the binary via
`include_str!()` from `templates/right/prompt/` and `templates/right/agent/`.
Changes to these files take effect on `cargo build` + restart — no file sync needed.
This eliminates the stale-template problem where changes to platform instructions
required manual re-init of existing agents.
```

- [ ] **Step 2: Update `ARCHITECTURE.md`**

In the "Directory Layout (Runtime)" section, the template structure is not documented (it's a source concern, not runtime). But update any references to AGENTS.md containing operating instructions.

In the "Prompting Architecture" subsection under "Data Flow", update the description:

Replace:
```
Every `claude -p` invocation gets a **composite system prompt** assembled from identity files.
```

With:
```
Every `claude -p` invocation gets a **composite system prompt** assembled from
compiled-in constants (operating instructions, bootstrap) and agent-owned files
(IDENTITY.md, SOUL.md, USER.md, AGENTS.md, TOOLS.md).
```

In the Configuration Hierarchy table, update the AGENTS.md entry:

| Scope | File | Source of Truth |
|-------|------|-----------------|
| Agent-owned | `agents/<name>/AGENTS.md` | Per-agent config (subagents, routing, skills) |

Remove or update any mention of AGENTS.md containing "procedures, session routine" — it's now just per-agent configuration.

- [ ] **Step 3: Commit**

```bash
git add PROMPT_SYSTEM.md ARCHITECTURE.md
git commit -m "docs: update prompt assembly docs for compiled-in operating instructions"
```

---

### Task 8: Migration — Replace Old AGENTS.md on Existing Agent

**Files:**
- None (manual host-side operation)

- [ ] **Step 1: Copy reduced AGENTS.md to existing agent**

The existing agent at `~/.rightclaw/agents/right/AGENTS.md` has the old full content (sections 1–8 + per-agent placeholders). Replace it with the new reduced template:

```bash
cp templates/right/agent/AGENTS.md ~/.rightclaw/agents/right/AGENTS.md
```

This is a one-time manual step for the existing "right" agent. New agents created via `rightclaw init` or `rightclaw agent init` will get the new template automatically.

- [ ] **Step 2: Verify the file was replaced**

```bash
head -5 ~/.rightclaw/agents/right/AGENTS.md
```

Expected:
```
# Agent Configuration

## Subagents

<!-- Define your subagents here. Each subagent is a specialized worker with its own permissions. -->
```

- [ ] **Step 3: No commit needed** (host-side runtime change, not source code)
