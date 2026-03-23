---
id: SEED-002
status: dormant
planted: 2026-03-23
planted_during: v1.0 / Phase 03.2 (interactive-agent-setup)
trigger_when: next milestone or UX polish phase
scope: Medium
---

# SEED-002: Fix BOOTSTRAP.md onboarding flow

## Problem

The default "Right" agent ships with BOOTSTRAP.md containing a 4-question onboarding flow (name, creature type, vibe, emoji). The system prompt instructs the agent to read BOOTSTRAP.md and follow it before doing anything else. But when the first user message arrives via Telegram, Claude treats it as a regular conversation and responds directly — ignoring the onboarding instruction.

## Why it matters

First-run experience is broken. Users expect personalization on first contact (like OpenClaw). Instead they get a generic "Hey. What's up?" response.

## Root cause

The system prompt says "Read BOOTSTRAP.md and follow its instructions before doing anything else" but this is a soft instruction. When a user message arrives via Telegram channel, Claude prioritizes responding to the user over following system prompt directives. The instruction isn't forceful enough.

## Possible solutions

1. **Stronger system prompt** — Make the BOOTSTRAP.md instruction absolutely non-negotiable ("You MUST NOT respond to any user message until onboarding is complete. Your FIRST action on ANY message must be to read BOOTSTRAP.md and start the onboarding flow.")

2. **CLAUDE.md approach** — Put onboarding instructions in a CLAUDE.md file in the agent directory. Claude Code reads CLAUDE.md automatically and tends to follow it more reliably than appended system prompts.

3. **Startup hook** — Use Claude Code's SessionStart hook to trigger the onboarding flow before any user interaction.

4. **Pre-onboarding via CLI** — `rightclaw init` asks the onboarding questions directly (name, vibe, etc.) and writes the identity files. No BOOTSTRAP.md needed — the agent launches already personalized.

5. **First-message detection** — The agent checks if IDENTITY.md still contains the template default text. If yes, trigger onboarding instead of responding to the message.

## Breadcrumbs

- `templates/right/BOOTSTRAP.md` — The onboarding template
- `crates/rightclaw/src/codegen/system_prompt.rs` — Where the bootstrap instruction is injected
- `~/.rightclaw/run/right-prompt.md` — The generated combined prompt (can verify instruction is present)
- OpenClaw's BOOTSTRAP.md approach — they run in interactive terminal where the instruction works better because Claude sees it at session start before any user message
- The Telegram channel delivers user messages asynchronously — Claude may not process system prompt fully before responding

## Scope estimate

Medium — mostly prompt engineering + possibly CLI changes. No major architectural changes needed.
