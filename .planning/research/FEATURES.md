# Feature Landscape

**Domain:** Multi-agent CLI runtime for Claude Code (OpenClaw-compatible)
**Researched:** 2026-03-21
**Confidence:** MEDIUM-HIGH (extensive OpenClaw ecosystem data, some RightClaw-specific gaps)

---

## Table Stakes

Features users expect from any multi-agent CLI runtime. Missing = product feels incomplete or broken.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `up` / `down` lifecycle commands | OpenClaw has `gateway start/stop/restart`. Users expect single-command launch/teardown. | Low | RightClaw wraps process-compose; straightforward |
| Agent status visibility | OpenClaw has `gateway status`, `channels status --probe`. Users need to know what's running. | Low | process-compose TUI provides this free |
| Per-agent workspace isolation | OpenClaw gives each agent its own agentDir, session store, auth profile. Mixing causes collisions. | Medium | Directory structure + process-compose config generation |
| Workspace file conventions (SOUL.md, AGENTS.md, USER.md, IDENTITY.md, MEMORY.md, TOOLS.md) | 5,700+ ClawHub skills assume these files exist. Breaking convention = ecosystem incompatibility. | Low | File templates, not runtime logic |
| BOOTSTRAP.md first-run flow | OpenClaw's onboarding creates identity files on first conversation, then self-deletes. Users expect guided setup. | Medium | Claude Code session handles this; RightClaw just seeds the file |
| Skill installation from ClawHub | 13,700+ skills on ClawHub. Users expect `install <slug>` to work. No skill access = no ecosystem value. | Medium | HTTP API integration, local file placement, dependency checking |
| Restart individual agents | OpenClaw has per-agent restart. Agents crash, get stuck, or need config reload. | Low | process-compose handles natively |
| Detached/background mode | OpenClaw runs as a daemon. Users want agents running while terminal is free. | Low | process-compose `-d` flag, `attach` for reconnection |
| Health check / doctor command | OpenClaw's `doctor --fix` is one of the 6 most-used commands. Users hit setup issues constantly. | Medium | Check dependencies (process-compose, OpenShell, Claude Code CLI), validate configs |
| Logging and log tailing | OpenClaw's `logs --follow` is top-6 most-used. Debugging agents without logs is impossible. | Low | process-compose provides per-process logging |
| Per-agent configuration | OpenClaw supports per-agent tools, skills, auth. Users need different agents to behave differently. | Medium | `agent.yaml` for restart policy, backoff, start prompt; `.mcp.json` for MCP servers |
| Installation script | OpenClaw ships `curl | bash` installer. Users expect one-liner setup. | Medium | Must install rightclaw binary, process-compose, and OpenShell |
| Selective agent launch | OpenClaw has `agents add/remove`. Users don't always want all agents running. | Low | `rightclaw up --agents <list>` already planned |

## Differentiators

Features that set RightClaw apart. Not expected (OpenClaw lacks them or does them poorly), but highly valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Sandbox-by-default (OpenShell)** | OpenClaw's sandbox is opt-in, permissive-by-default, and has been bypassed multiple times (Snyk Labs, ClawHavoc). RightClaw enforces kernel-level sandboxing on every agent. This is the #1 differentiator. | High | OpenShell policy YAML per agent; Landlock LSM + Seccomp BPF on Linux, Docker Desktop on macOS |
| **Declarative security policies** | OpenClaw has 3 confusing permission layers (agent tools, sandbox tools, sandbox network). RightClaw: one YAML policy per agent, deny-by-default, readable. | Medium | OpenShell policy format handles fs/network/process restrictions declaratively |
| **Skill audit before activation** | ClawHub has 820+ known malicious skills (20% of registry at one point). RightClaw gates skill installation behind policy audit. "What permissions does this skill need?" before it runs. | Medium | Parse SKILL.md frontmatter `metadata.openclaw.requires`, compare against agent policy |
| **No daemon / gateway complexity** | OpenClaw runs a WebSocket gateway on port 18789, manages auth tokens, session stores, channel bindings. RightClaw: process-compose launches Claude Code sessions. No gateway, no auth layer, no port binding. Dramatically simpler attack surface. | Low | This is a simplification, not a feature to build |
| **Lightweight Rust binary** | OpenClaw is 430,000+ lines of Node.js, 2GB+ RAM. Competitors like OpenFang (32MB binary, 180ms cold start) and ZeroClaw prove users want lean runtimes. | Medium | Rust compilation, but the binary is the CLI orchestrator only |
| **process-compose TUI for free** | OpenClaw built Mission Control dashboards. RightClaw gets process visibility, log streaming, and restart controls from process-compose's built-in TUI. Zero UI code to maintain. | Low | Already decided in PROJECT.md |
| **Cron lock-file concurrency** | OpenClaw's cron stores jobs in `jobs.json` that corrupts on manual edits while gateway runs. RightClaw uses heartbeat-based lock files with UTC ISO 8601 timestamps. No central store to corrupt. | Medium | CronSync skill handles this inside Claude Code sessions |

## Anti-Features

Things to deliberately NOT build. Either because they're out of scope, add attack surface, or are solved better elsewhere.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Messaging channel bridges (Telegram, Discord, Slack, WhatsApp, Signal, iMessage)** | OpenClaw's core value prop is chat platform bridging. RightClaw is a CLI runtime for developers, not a personal assistant gateway. Channel bridges add massive attack surface (135K+ exposed OpenClaw instances). | Agents run as Claude Code sessions in terminal. Users interact via `rightclaw attach` or process-compose TUI. |
| **Built-in web UI / dashboard** | OpenClaw has Mission Control, ClawPort, Control UI. Building UI = perpetual maintenance burden. process-compose TUI covers 95% of needs. | Use process-compose TUI. If users want dashboards, that's a community project. |
| **Session management / persistence** | OpenClaw manages JSONL session stores, session branching, named sessions. Claude Code handles its own sessions. | Let Claude Code manage its sessions. RightClaw manages processes, not conversations. |
| **Model management / multi-provider** | OpenClaw supports Claude, GPT, Gemini, DeepSeek, Ollama. RightClaw is Claude Code-specific. | Users configure Claude Code's model settings directly. Not our concern. |
| **Agent-to-agent communication** | OpenClaw's acpx enables ACP sessions between agents. Phase 1 scope is autonomous agents. Shared memory is Phase 2. | Agents are independent. Phase 2 adds MCP memory server for coordination. |
| **Built-in secrets management** | OpenClaw's secrets system (refs, providers, audit) is complex and has had plaintext storage vulnerabilities. | Agents inherit environment from their shell wrapper. Use system-level secrets management (env vars, vault). |
| **Plugin system** | OpenClaw has `plugins install/enable/disable/doctor`. Another extensibility layer = another attack vector. | Skills (ClawHub-compatible) are the extensibility mechanism. One format, one surface. |
| **Workflow engine (Lobster equivalent)** | OpenClaw's Lobster is a typed pipeline runtime with approval gates. Interesting but orthogonal to agent orchestration. | Claude Code sessions handle task execution. Complex workflows belong in skills or user-defined agents. |
| **Central orchestrator / master agent** | Some OpenClaw setups use "master + satellite" fleets of 15+ agents. Centralized control contradicts autonomous agent philosophy. | Each agent is autonomous per PROJECT.md. Coordination via shared conventions (MEMORY.md, crons), not hierarchy. |
| **`clawhub` CLI as a dependency** | OpenClaw ecosystem has a separate `clawhub` CLI binary. Adding it as a dependency adds supply chain risk. | RightClaw's `/clawhub` skill talks to ClawHub HTTP API directly. No external CLI needed. |

## Feature Dependencies

```
Installation script
  -> Health check / doctor (validates install)
    -> Agent workspace creation (needs dependencies present)
      -> BOOTSTRAP.md first-run flow (needs workspace files)
        -> Agent lifecycle (up/down/status/restart)

OpenShell policy per agent
  -> Shell wrapper per agent (extracts policy, wraps openshell sandbox create)
    -> Agent lifecycle (agents launch through wrapper)

Skill installation (ClawHub HTTP API)
  -> Policy gate / audit (check skill requirements against agent policy)
    -> Skill activation (copy to agent's skills/ directory)

CronSync skill
  -> Lock-file concurrency control (heartbeat-based)
    -> Cron YAML specs in agent dirs
```

## MVP Recommendation

**Phase 1 priorities (must ship to be usable):**

1. **Agent lifecycle management** (`up`, `down`, `status`, `restart`, `attach`) -- table stakes, low complexity
2. **Per-agent workspace with OpenClaw file conventions** -- table stakes for ecosystem compatibility
3. **OpenShell sandbox enforcement** -- the core differentiator, high complexity but non-negotiable
4. **Per-agent shell wrapper** (policy extraction + `openshell sandbox create` invocation) -- enables sandboxing
5. **Default "Right" agent with BOOTSTRAP.md** -- proves the runtime works out of the box
6. **Installation script** -- users must be able to install in one command
7. **Health check command** -- users will hit setup issues immediately

**Phase 2 priorities (ship after core works):**

1. **ClawHub skill installation** (`/clawhub` skill via HTTP API) -- ecosystem access
2. **Skill policy audit gate** -- security differentiator for skills
3. **CronSync skill with lock-file concurrency** -- automation capability
4. **Per-agent `agent.yaml` configuration** (restart policies, backoff, custom prompts)

**Defer entirely:**

- Shared memory / MCP memory server (explicitly out of scope per PROJECT.md)
- Messaging channel bridges (anti-feature)
- Web UI / dashboard (anti-feature)
- Agent-to-agent communication (Phase 2+ if ever)

## Key Insights from OpenClaw User Pain Points

These findings should directly inform RightClaw's design decisions:

| OpenClaw Problem | RightClaw Opportunity | Confidence |
|-----------------|----------------------|------------|
| Security is opt-in and broken (bypass exploits, 135K exposed instances, 820+ malicious skills) | Sandbox-by-default with deny-by-default policies. This is the entire reason RightClaw exists. | HIGH |
| Runaway costs ($200-750/month, heartbeat token sink of 170K-210K tokens per run) | No built-in heartbeat mechanism burning tokens in background. CronSync is explicit, user-controlled. HEARTBEAT.md is an agent-level concern, not runtime overhead. | HIGH |
| Breaking updates and regressions (v2026.3.2 broke exec tools) | Rust binary with fewer moving parts. process-compose is stable. OpenShell is the only alpha dependency. | MEDIUM |
| Complex setup (430K lines of code, 100+ CLI subcommands, Node 22+ / Bun runtime) | Single Rust binary + process-compose + OpenShell. Target: 5 subcommands that cover 95% of use. | HIGH |
| Context compression and agent wandering | Not RightClaw's problem -- Claude Code handles its own context. But good AGENTS.md templates help. | MEDIUM |
| 3 confusing permission layers | One OpenShell policy YAML per agent. Readable, auditable, version-controllable. | HIGH |
| Name changes causing doc chaos (Clawdbot -> Moltbot -> OpenClaw) | Pick a name, keep it. RightClaw. Done. | HIGH |

## Sources

- [OpenClaw CLI Reference](https://docs.openclaw.ai/cli) -- HIGH confidence, official docs
- [OpenClaw Agent Runtime](https://docs.openclaw.ai/concepts/agent) -- HIGH confidence, official docs
- [OpenClaw Skills](https://docs.openclaw.ai/tools/skills) -- HIGH confidence, official docs
- [OpenClaw Cron vs Heartbeat](https://docs.openclaw.ai/automation/cron-vs-heartbeat) -- HIGH confidence, official docs
- [OpenClaw Security](https://docs.openclaw.ai/gateway/security) -- HIGH confidence, official docs
- [OpenClaw Onboarding Wizard](https://docs.openclaw.ai/start/wizard) -- HIGH confidence, official docs
- [ClawHub Skill Directory](https://github.com/openclaw/clawhub) -- HIGH confidence, official repo
- [OpenClaw Memory Files Explained](https://openclaw-setup.me/blog/openclaw-memory-files/) -- MEDIUM confidence, community
- [Top 20 OpenClaw Problems](https://getmilo.dev/blog/top-openclaw-problems-how-to-fix-them) -- MEDIUM confidence, community analysis
- [OpenClaw Review: Good, Bad, and Malware](https://everydayaiblog.com/openclaw-moltbot-ai-assistant-review/) -- MEDIUM confidence, independent review
- [Snyk Labs: Bypassing OpenClaw Sandbox](https://labs.snyk.io/resources/bypass-openclaw-security-sandbox/) -- HIGH confidence, security research
- [NVIDIA NemoClaw at GTC 2026](https://particula.tech/blog/nvidia-nemoclaw-openclaw-enterprise-security) -- MEDIUM confidence, press coverage
- [ClawHub Security Analysis (Straiker)](https://www.straiker.ai/blog/built-on-clawhub-spread-on-moltbook-the-new-agent-to-agent-attack-chain) -- HIGH confidence, security firm
- [OpenClaw Heartbeat Token Cost Discussion](https://github.com/openclaw/openclaw/discussions/11042) -- HIGH confidence, GitHub discussion
- [OpenFang Agent OS](https://agentnativedev.medium.com/i-ignored-30-openclaw-alternatives-until-openfang-ff11851b83f1) -- LOW confidence, single source
