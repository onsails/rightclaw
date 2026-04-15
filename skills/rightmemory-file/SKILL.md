---
name: rightmemory
description: Manage your long-term memory via MEMORY.md
---

# Memory Management

Your long-term memory is stored in `MEMORY.md` in your home directory.
This file is automatically injected into your system prompt at the start
of each interaction (truncated to 200 lines).

## How to use

Use Claude Code's built-in `Edit` and `Write` tools to manage MEMORY.md:

- **Add an entry:** Append a new line or section to MEMORY.md
- **Update an entry:** Edit an existing line to correct or refine it
- **Remove stale entries:** Delete lines that are no longer relevant

## What to save

- User preferences ("prefers dark mode", "uses vim keybindings")
- Correct API formats after fixing validation errors
- Project decisions that affect future work
- Lessons learned / mistakes to avoid
- Important facts about the user's environment or workflow

## What NOT to save

- Regular conversation content (it's already in session context)
- Information that's in code, configs, or documentation
- Temporary debugging notes or one-off commands

## Keep it concise

MEMORY.md is truncated to **200 lines** in your prompt. If it grows
too large, periodically review and:
- Remove entries that are no longer relevant
- Consolidate related entries into single lines
- Remove duplicates
