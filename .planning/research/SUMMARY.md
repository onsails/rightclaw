# Project Research Summary

**Project:** RightClaw
**Domain:** Multi-agent CLI runtime (Rust CLI wrapping process-compose + OpenShell)
**Researched:** 2026-03-21
**Confidence:** MEDIUM-HIGH

## Executive Summary

RightClaw is a Rust CLI that orchestrates multiple sandboxed Claude Code agent sessions. It generates process-compose configuration from a declarative agent directory structure, wraps each agent in an OpenShell sandbox for kernel-level security enforcement, and delegates all process lifecycle management to process-compose. The recommended approach is a "thin CLI, fat config" architecture: RightClaw discovers agents, resolves policies, generates static YAML/shell scripts, and hands off to process-compose. It builds no process manager, no web UI, no messaging bridges. The entire security value proposition rests on OpenShell sandbox-by-default with deny-by-default policies -- this is what distinguishes RightClaw from OpenClaw, which has been repeatedly compromised (CVE-2026-25253, ClawHavoc supply chain attacks, 135K+ exposed instances).

The stack is well-established Rust ecosystem tooling: clap for CLI, tokio + reqwest for async HTTP to process-compose's REST API, minijinja for YAML generation, miette + thiserror for user-facing diagnostics. The only risky dependency is OpenShell itself -- 3 days old, alpha quality, heavyweight K3s/Docker architecture. This must be abstracted behind a trait so the sandbox backend can be mocked, swapped, or degraded gracefully. The second critical risk is the OAuth token race condition: multiple concurrent Claude Code processes race on refreshing a single-use token, breaking authentication for all but one agent. The recommended mitigation is to default to API keys and document OAuth's unsuitability for multi-agent use.

The codebase should stay small (under 2K lines for v1). Six modules handle the entire pipeline: CLI parsing, agent discovery, policy resolution, shell wrapper generation, process-compose config generation, and process-compose lifecycle management. Each module has clear data flow boundaries and can be tested independently.

## Key Findings

### Recommended Stack

The stack is mature and high-confidence. All core crates are well-maintained with millions of downloads. The only pre-1.0 dependency is `serde-saphyr` (0.0.22) for YAML parsing, chosen because `serde_yaml` is deprecated and `serde-saphyr` is the only ground-up rewrite with full test coverage. Pin its exact version.

**Core technologies:**
- **clap 4.6 + tokio 1.50:** CLI framework and async runtime. Async is justified by reqwest (HTTP client for process-compose REST API) and concurrent process monitoring
- **minijinja 2.18:** Template engine for generating process-compose.yaml. Prevents YAML formatting bugs from string concatenation. Lighter than Tera, more flexible than Askama
- **reqwest 0.13:** HTTP client for process-compose REST API (status, restart, stop). Structured JSON responses beat shelling out to the PC CLI
- **miette 7.6 + thiserror 2.0:** Compiler-style diagnostic errors for config validation. When agent.yaml or policy.yaml has issues, users see exactly where. Replaces the archived color-eyre
- **serde-saphyr 0.0.22:** YAML parsing replacement for the deprecated serde_yaml. Pin exact version
- **process-compose v1.100.0+:** External process orchestrator with TUI, REST API, Unix socket communication. RightClaw generates config; PC handles lifecycle
- **OpenShell (alpha):** NVIDIA's kernel-level sandbox. Landlock + seccomp on Linux, Docker Desktop on macOS. The core differentiator but also the riskiest dependency

### Expected Features

**Must have (table stakes):**
- Agent lifecycle commands: `up`, `down`, `status`, `restart`, `attach`
- Per-agent workspace isolation with OpenClaw file conventions (IDENTITY.md, SOUL.md, AGENTS.md, MEMORY.md)
- Detached/background mode with reattach capability
- Health check / `doctor` command (validate dependencies are installed)
- Logging and log tailing
- Per-agent configuration via `agent.yaml` (restart policy, backoff, start prompt)
- Installation script (one-liner setup)
- Selective agent launch (`--agents` flag)

**Should have (differentiators):**
- Sandbox-by-default via OpenShell -- the entire reason RightClaw exists
- Declarative security policies (one YAML per agent, deny-by-default)
- Skill audit gate before ClawHub skill activation (parse permissions, compare against policy)
- No daemon / gateway complexity (dramatically simpler than OpenClaw's WebSocket architecture)
- CronSync with lock-file concurrency control

**Defer (v2+):**
- Agent-to-agent communication / shared memory (MCP memory server)
- Messaging channel bridges (anti-feature)
- Web UI / dashboard (anti-feature, process-compose TUI suffices)
- Plugin system (skills are the extensibility mechanism)
- Workflow engine (Lobster equivalent)

### Architecture Approach

The architecture follows a strict pipeline: discover agents from filesystem, resolve policies per agent, generate static shell wrappers and process-compose YAML to a temp directory, then spawn process-compose pointing at the generated config. All lifecycle operations after spawn go through process-compose's Unix socket API. The CLI never manages processes directly.

**Major components:**
1. **Agent Discovery** -- scan `agents/` dir, parse `agent.yaml`, return `Vec<AgentDef>`
2. **Policy Resolver** -- search order: agent dir > `policies/` dir > built-in default. Returns path to resolved policy YAML
3. **Config Generator (codegen)** -- pure transformation: agents + policies -> shell wrapper scripts + process-compose.yaml. Writes to temp dir
4. **Runtime (PC Lifecycle)** -- spawn process-compose, store socket path in state.json, implement attach/status/restart/down via socket API

### Critical Pitfalls

1. **ClawHub supply chain poisoning** -- 820-1,184 confirmed malicious skills on ClawHub. Every skill installation must pass through a policy gate that audits requested permissions against the agent's sandbox policy. Never auto-activate skills.

2. **OpenShell alpha instability** -- 3 days old, K3s/Docker architecture, no stability guarantees. Abstract behind a trait. Implement health checks after sandbox creation. Ship `--no-sandbox` flag for development. Pin version and test upgrades in CI.

3. **OAuth token race condition** -- Multiple concurrent Claude Code processes race on single-use refresh token. 4 of 5 agents lose auth after first token expiry. Default to API keys. Document OAuth as unsupported for multi-agent use.

4. **Process tree signal propagation** -- SIGTERM from `rightclaw down` does not propagate through OpenShell's container boundary. Shell wrappers must trap SIGTERM and explicitly call `openshell sandbox destroy`. Scan for orphaned sandboxes on startup.

5. **Cross-platform sandbox divergence** -- Landlock on Linux (kernel 6.4+ for full enforcement) vs Docker Desktop on macOS. Use `hard_requirement` compatibility mode so sandboxes fail loudly rather than silently degrading security.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Project Scaffold and Core Types

**Rationale:** Everything depends on the data structures (`AgentDef`, `AgentConfig`, `RestartPolicy`) and error types. Establishing the Cargo project with the correct dependency set and module structure unblocks all subsequent phases.
**Delivers:** Compilable project with CLI skeleton (clap subcommand definitions), core types, error types, and a passing `rightclaw --help` test.
**Addresses:** Project structure from ARCHITECTURE.md
**Avoids:** N/A (foundation phase)

### Phase 2: Agent Discovery and Policy Resolution

**Rationale:** Discovery and policy resolution are the first two pipeline stages. They are pure functions operating on the filesystem with no external dependencies (no process-compose, no OpenShell). Highly testable with fixture directories.
**Delivers:** `rightclaw` can scan an `agents/` directory, parse `agent.yaml` files, resolve policies per agent, and report what it found. No launching yet.
**Addresses:** Per-agent workspace isolation, per-agent configuration, layered defaults pattern
**Avoids:** Hardcoded agent definitions (anti-pattern #4 from ARCHITECTURE.md)

### Phase 3: Code Generation (Shell Wrappers + process-compose YAML)

**Rationale:** Codegen is the pure transformation layer that converts discovered agents into launchable artifacts. It depends on Phase 2's output types but has no runtime side effects. Snapshot-testable.
**Delivers:** Given `Vec<AgentDef>`, generates shell wrapper scripts and process-compose.yaml to a temp directory. Verifiable by inspecting generated files.
**Addresses:** Code generation over runtime templating pattern, declarative security policies
**Avoids:** Dynamic policy assembly at runtime (anti-pattern #2), YAML formatting bugs

### Phase 4: Runtime Lifecycle (up/down/attach/status/restart)

**Rationale:** This is the first phase with real side effects -- spawning process-compose, managing Unix sockets, handling signals. Depends on all preceding phases. Must address signal propagation and graceful shutdown from the start.
**Delivers:** Full `rightclaw up/down/status/restart/attach` lifecycle. Working multi-agent launch with sandbox enforcement.
**Addresses:** Agent lifecycle commands, detached mode, restart individual agents, logging
**Avoids:** Process tree signal propagation pitfall (#5), network exposure pitfall (#4 -- bind 127.0.0.1 only)

### Phase 5: Health Check, Installation, and Default Agent

**Rationale:** Once the runtime works, users need to install it and get a working first experience. The `doctor` command validates the toolchain. The default "Right" agent with BOOTSTRAP.md proves the system works out of the box.
**Delivers:** `rightclaw doctor` (dependency validation), `install.sh` script, default "Right" agent with BOOTSTRAP.md onboarding flow
**Addresses:** Health check / doctor command, installation script, BOOTSTRAP.md first-run flow
**Avoids:** Malvertising pitfall (#11 -- signed releases, checksum verification)

### Phase 6: ClawHub Skill Installation and Policy Gate

**Rationale:** Skill installation is the ecosystem access point but also the primary attack vector. Must come after sandbox enforcement is solid. The policy gate is the security differentiator.
**Delivers:** `/clawhub` Claude Code skill for searching/installing/auditing ClawHub skills. Policy gate that compares skill requirements against agent sandbox policy.
**Addresses:** Skill installation from ClawHub, skill audit before activation
**Avoids:** Supply chain poisoning pitfall (#1), convention mismatch pitfall (#9)

### Phase 7: CronSync and Automation

**Rationale:** Cron-based automation depends on stable agent lifecycle and skill infrastructure. The ephemeral-vs-declarative impedance mismatch requires careful design.
**Delivers:** CronSync Claude Code skill with idempotent reconciliation, lock-file concurrency control, heartbeat-based TTL
**Addresses:** CronSync skill, cron lock-file concurrency
**Avoids:** CronSync race conditions pitfall (#7)

### Phase Ordering Rationale

- **Types before logic:** Phases 1-2 establish data structures that all subsequent phases consume. Changing `AgentDef` late is expensive.
- **Pure before effectful:** Phases 2-3 are pure functions (filesystem reads, string generation). Phase 4 introduces side effects (child processes, sockets). This ordering maximizes testability early.
- **Security before ecosystem:** Sandbox enforcement (Phase 4) must be solid before exposing users to ClawHub's supply chain (Phase 6). Installing skills without a working sandbox defeats the purpose.
- **Architecture dependency chain:** Discovery -> Policy -> Codegen -> Runtime mirrors the data flow. Each phase's output is the next phase's input.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4 (Runtime Lifecycle):** OpenShell sandbox create/destroy behavior under failure conditions. Process-compose Unix socket API specifics. Signal propagation through nested process trees. OAuth vs API key credential management.
- **Phase 6 (ClawHub Skills):** ClawHub HTTP API endpoints (underdocumented). SKILL.md frontmatter format for permission parsing. Top-50 skill filesystem access patterns for policy compatibility.
- **Phase 7 (CronSync):** Claude Code cron API (CronCreate/CronList/CronDelete tool specifics). Lock file atomicity guarantees across filesystems.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Scaffold):** Standard Rust CLI project setup. clap derive API is well-documented.
- **Phase 2 (Discovery):** Directory traversal + YAML parsing. Textbook Rust.
- **Phase 3 (Codegen):** Template rendering + file writing. minijinja has good docs.
- **Phase 5 (Doctor/Install):** Standard dependency checking pattern. Shell script installation is well-understood.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crates verified on crates.io with current versions. Only risk is serde-saphyr being pre-1.0 |
| Features | MEDIUM-HIGH | Extensive OpenClaw ecosystem data provides clear table stakes. Some RightClaw-specific UX decisions remain open |
| Architecture | HIGH | Clean pipeline architecture with well-defined boundaries. process-compose integration pattern is sound |
| Pitfalls | HIGH | Multiple independent security research sources confirm the threat landscape. OAuth race condition has a GitHub issue with reproduction steps |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **OpenShell sandbox create latency:** No benchmarks found for sandbox startup time. If K3s container creation adds 5+ seconds per agent, the `rightclaw up` experience suffers. Needs measurement during Phase 4 implementation.
- **OpenShell policy YAML schema stability:** No stability guarantees. The schema could change between alpha releases. The abstraction trait in code mitigates this, but policy YAML files users write could break.
- **Claude Code CLI flags for multi-agent use:** `--append-system-prompt-file` and `--dangerously-skip-permissions` are documented but their interaction with sandbox restrictions is untested.
- **process-compose Unix Domain Socket API:** Documentation exists but edge cases (socket cleanup after crash, multiple concurrent clients) need validation.
- **Anthropic policy on concurrent CLI sessions:** No official stance on running multiple Claude Code instances. Could be restricted without notice.

## Sources

### Primary (HIGH confidence)
- [process-compose documentation](https://f1bonacc1.github.io/process-compose/) -- configuration, REST API, lifecycle management
- [NVIDIA OpenShell Developer Guide](https://docs.nvidia.com/openshell/latest/index.html) -- policies, sandbox management, default policy
- [OpenClaw official docs](https://docs.openclaw.ai/) -- CLI reference, agent runtime, security model
- [Snyk Labs: Bypassing OpenClaw Sandbox](https://labs.snyk.io/resources/bypass-openclaw-security-sandbox/) -- security research
- [GitHub: OAuth token refresh race condition](https://github.com/anthropics/claude-code/issues/27933) -- reproduction steps
- Crate registrations verified on crates.io (clap 4.6, tokio 1.50, reqwest 0.13, miette 7.6, serde-saphyr 0.0.22, minijinja 2.18)

### Secondary (MEDIUM confidence)
- [Trend Micro: OpenClaw Skills Distributing Atomic macOS Stealer](https://www.trendmicro.com/en_us/research/26/b/openclaw-skills-used-to-distribute-atomic-macos-stealer.html)
- [VirusTotal: How OpenClaw Skills Are Being Weaponized](https://blog.virustotal.com/2026/02/from-automation-to-infection-how.html)
- [CyberPress: ClawHavoc Campaign](https://cyberpress.org/clawhavoc-poisons-openclaws-clawhub-with-1184-malicious-skills/)
- [OpenClaw community pain points](https://getmilo.dev/blog/top-openclaw-problems-how-to-fix-them)

### Tertiary (LOW confidence)
- [OpenFang Agent OS comparison](https://agentnativedev.medium.com/i-ignored-30-openclaw-alternatives-until-openfang-ff11851b83f1) -- single source, useful for competitive context only

---
*Research completed: 2026-03-21*
*Ready for roadmap: yes*
