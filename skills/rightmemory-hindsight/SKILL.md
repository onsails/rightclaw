---
name: rightmemory
description: Manage your long-term memory powered by Hindsight
---

# Memory Management

Your memory is powered by Hindsight. It works in two ways:

**Automatic:** Your conversations are retained and relevant context
is recalled before each interaction. You don't need to do anything
for this to work.

**Explicit tools** — use when automatic isn't enough:

- `mcp__right__memory_retain(content, context)` — save a fact permanently
  - Use for: user preferences, correct API formats, decisions,
    lessons learned, project conventions
  - `context` is a short label: "user preference", "api format",
    "project decision", "mistake to avoid"
  - Example: after fixing a wrong API call, retain the correct format

- `mcp__right__memory_recall(query)` — search your memory
  - Use before: answering questions about past work, making decisions
    that might have prior context
  - Returns ranked results from semantic + keyword + graph search

- `mcp__right__memory_reflect(query)` — deep analysis across memories
  - Use for: synthesizing patterns, comparing past decisions,
    understanding evolution of a project
  - More expensive than recall — use when you need reasoning, not lookup

## When to use explicit retain

- You discovered a user preference ("prefers tabs over spaces")
- You fixed a tool call after a validation error (save correct format)
- A decision was made that affects future work
- You learned something non-obvious about the codebase or APIs

## When NOT to retain explicitly

- Regular conversation — auto-retain handles this
- Information already in files (code, configs, docs)
- Temporary/ephemeral context (debugging steps, one-off commands)
