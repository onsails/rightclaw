# Domain Pitfalls

**Domain:** Multi-agent CLI runtime (Claude Code + OpenShell sandbox orchestration)
**Researched:** 2026-03-21

## Critical Pitfalls

Mistakes that cause rewrites, security incidents, or fundamental architecture failures.

### Pitfall 1: ClawHub Skill Supply Chain Poisoning

**What goes wrong:** ClawHub has had 824-1,184 confirmed malicious skills (roughly 20% of the ecosystem as of Feb 2026). The ClawHavoc campaign used typosquatting, compromised GitHub accounts, and automated uploads to flood ClawHub with infostealers, reverse shells, and credential exfiltrators hidden in SKILL.md files. One user uploaded 354 malicious packages via automated scripts.

**Why it happens:** SKILL.md files contain executable instructions that Claude Code follows as a trusted intermediary. A malicious skill can instruct the agent to exfiltrate `~/.clawdbot/.env`, open reverse shells, or present fake setup prompts that trick users into running malware. ClawHub has no pre-publish vetting -- it is an unvetted software supply chain.

**Consequences:** Stolen API keys, wallet private keys, SSH credentials, browser passwords. Full remote code execution on the host. This is not theoretical -- Trend Micro, Bitdefender, and VirusTotal all documented active campaigns distributing Atomic macOS Stealer via ClawHub skills.

**Prevention:**
- OpenShell sandbox enforcement is RightClaw's primary defense. Every agent runs sandboxed, so even a malicious skill cannot reach outside the policy boundary.
- The `/clawhub` skill MUST audit permissions before activation -- parse SKILL.md frontmatter, compare requested capabilities against the agent's OpenShell policy, and reject or warn on mismatches.
- Never auto-activate installed skills. Require explicit user confirmation after showing what the skill requests.
- Consider integrating VirusTotal skill scanning (they added native SKILL.md analysis support).

**Detection:** Skills requesting network access to unknown endpoints, skills with obfuscated shell commands in SKILL.md, skills from accounts with bulk uploads, skills with names mimicking popular crypto/trading tools.

**Phase relevance:** Must be addressed in the ClawHub skill installation phase. The policy gate is not optional -- it is the entire security value proposition of RightClaw over OpenClaw.

---

### Pitfall 2: OpenShell Alpha Instability -- Betting the Runtime on Unaudited Software

**What goes wrong:** OpenShell is 3 days old (released at GTC 2026-03-18). It is explicitly labeled "Alpha software -- single-player mode." Known issues include: WSL2 GPU passthrough DOA (sandbox creates but immediately dies), stale gateway state surviving `openshell gateway destroy`, provider ordering failures, silent model overrides via env vars. The policy YAML schema has no stability guarantees.

**Why it happens:** Alpha software by definition has breaking changes. The architecture is heavyweight (K3s cluster inside Docker), adding startup latency, memory overhead, and debugging complexity. No independent security audits exist. No production reference deployments.

**Consequences:** RightClaw's core value proposition (sandboxed agents) breaks when OpenShell breaks. Users hit opaque errors from the K3s/Docker stack underneath. Policy YAML format changes force rewrites of policy generation code. Gateway state corruption requires manual Docker volume cleanup.

**Prevention:**
- Abstract OpenShell behind a trait/interface in the Rust codebase. Never call `openshell` CLI directly from business logic -- wrap it so the sandbox backend can be swapped or mocked.
- Pin to a specific OpenShell version. Track the GitHub releases RSS. Test against new versions in CI before upgrading.
- Implement health checks: after `openshell sandbox create`, verify the sandbox is actually running (not the WSL2 "created but dead" failure mode).
- Design for graceful degradation: if OpenShell fails to start, give clear error messages, not cascading K3s/Docker stack traces.
- Ship a `--no-sandbox` flag for development/testing (with loud warnings), so users are not blocked by OpenShell bugs during their workflow.

**Detection:** `openshell sandbox create` returns 0 but subsequent commands return "sandbox not found." Gateway start hangs. Docker volume cleanup needed after failed runs.

**Phase relevance:** Phase 1 (core sandbox integration). The abstraction layer is foundational -- retrofitting it later means rewriting all sandbox interactions.

---

### Pitfall 3: OAuth Token Race Condition with Concurrent Agents

**What goes wrong:** When multiple Claude Code CLI processes run concurrently (which is RightClaw's entire purpose), they race on refreshing the single-use OAuth refresh token stored in `~/.claude/.credentials.json`. The first process to refresh succeeds and writes a new token. The second process sends the now-consumed refresh token and gets HTTP 404. Authentication is permanently broken for that agent with no automatic recovery.

**Why it happens:** Claude Code's OAuth flow was designed for single-instance use. The refresh token is single-use -- once consumed, it is gone. Multiple concurrent processes all see the same expiring token and all attempt to refresh simultaneously.

**Consequences:** In a 5-agent setup, 4 out of 5 agents silently lose authentication after the first token refresh cycle. Agents appear to be running but cannot make API calls. This is a showstopper for multi-agent runtimes.

**Prevention:**
- Use API keys (`ANTHROPIC_API_KEY`) instead of OAuth tokens for multi-agent setups. API keys do not have the single-use refresh problem.
- If OAuth is required: implement a token broker process that handles all token refreshes centrally and distributes valid tokens to agent processes.
- Each agent process needs its own credential isolation -- separate `CLAUDE_CONFIG_DIR` per agent, or a wrapper that serializes token refresh operations with file locking.
- Document this prominently in installation/setup guides. Users will hit this on day one if they use OAuth.

**Detection:** Agents suddenly returning auth errors after running fine for 1-2 hours (the typical token expiry window). HTTP 404 in Claude Code logs during token refresh.

**Phase relevance:** Phase 1 (agent launching). This must be solved before `rightclaw up` can reliably launch more than one agent.

---

### Pitfall 4: CVE-2026-25253 Pattern -- Unauthenticated WebSocket Exposure

**What goes wrong:** OpenClaw had a CVSS 8.8 RCE via unauthenticated WebSocket. If a user running OpenClaw visited a malicious page, that page's JavaScript could silently connect to the local OpenClaw service, steal auth tokens, and issue commands. Over 135,000 instances were found exposed on the public internet.

**Why it happens:** Developer tools default to binding on all interfaces (0.0.0.0) instead of localhost-only. No authentication on local management interfaces. Assumption that "local = safe" ignores browser-based attacks (CSRF, DNS rebinding).

**Consequences:** Remote code execution. Token theft. Full agent compromise from a single malicious page visit.

**Prevention:**
- Bind all management interfaces (process-compose TUI server, any status API) to 127.0.0.1 only. Never 0.0.0.0.
- If any WebSocket/HTTP management interface exists, require authentication even on localhost (CSRF/DNS rebinding attacks bypass same-origin for localhost).
- Audit process-compose's own network exposure -- its TUI server binds to a port for remote attach.
- Do not expose Claude Code sessions on any network interface.

**Detection:** `ss -tlnp` or `netstat` showing management ports bound to 0.0.0.0. Any process listening on non-localhost interfaces.

**Phase relevance:** Phase 1 (process-compose integration) and ongoing security review.

---

## Moderate Pitfalls

### Pitfall 5: Process Tree Signal Propagation Through Nested Wrappers

**What goes wrong:** RightClaw's process tree is deeply nested: `process-compose` -> `shell-wrapper.sh` -> `openshell sandbox create` -> `claude` (inside sandbox). When `rightclaw down` sends SIGTERM, process-compose sends it to the shell wrapper's process group. But the actual Claude Code process is inside an OpenShell sandbox (which runs in a K3s container). SIGTERM may not propagate through the container boundary, leaving orphaned Claude sessions and sandbox containers.

**Why it happens:** process-compose's `shutdown.parent_only: yes` sends signals only to the immediate child, not grandchildren. But even with process group signaling, the container boundary in OpenShell isolates the PID namespace. `openshell sandbox create` spawns a process inside K3s/Docker -- that process has a different PID namespace and may not receive signals from the host process group.

**Consequences:** Zombie Claude Code sessions consuming API quota. Orphaned OpenShell sandbox containers consuming memory/CPU. `rightclaw down` appearing to succeed while agents keep running. Stale state preventing clean restart.

**Prevention:**
- Implement explicit cleanup: `rightclaw down` must call `openshell sandbox destroy` for each agent, not just kill the wrapper process.
- Add shutdown hooks in the shell wrapper that trap SIGTERM and explicitly destroy the sandbox before exiting.
- Implement a health-check/liveness probe that detects orphaned sandboxes and cleans them up.
- On startup, scan for orphaned sandboxes from previous crashed runs and offer cleanup.

**Detection:** After `rightclaw down`, check `openshell sandbox list` for lingering sandboxes. Check `docker ps` for orphaned K3s containers. Check for Claude processes still running.

**Phase relevance:** Phase 1 (lifecycle management). Must be designed into the shell wrapper from the start.

---

### Pitfall 6: Cross-Platform Sandbox Behavior Divergence

**What goes wrong:** OpenShell uses Landlock LSM + seccomp-BPF on Linux and Docker Desktop on macOS. These are fundamentally different enforcement mechanisms with different capabilities, failure modes, and performance characteristics. A policy that works on Linux may behave differently on macOS, or vice versa.

**Why it happens:**
- Landlock requires kernel 5.13+ (filesystem) and 6.4+ (network). Older kernels silently degrade with `best_effort` compatibility mode -- the agent runs with fewer restrictions than the policy specifies.
- macOS Docker Desktop adds significant overhead (VM layer) compared to Linux's native kernel enforcement.
- Landlock's `best_effort` mode means the same policy YAML provides different security guarantees depending on the host kernel version. An agent on kernel 5.10 gets zero Landlock protection while appearing to be "sandboxed."

**Consequences:** False sense of security on older Linux kernels. Performance differences between platforms affecting agent responsiveness. Test-on-Mac, deploy-on-Linux (or vice versa) producing different security postures.

**Prevention:**
- Check kernel version on Linux and warn if Landlock ABI level is lower than expected. With `landlock.compatibility: hard_requirement`, the sandbox fails to create rather than silently degrading.
- Use `hard_requirement` as the default in RightClaw-generated policies, not `best_effort`. Failing loudly is better than silently running unsandboxed.
- Document minimum kernel requirements (6.4+ for full filesystem + network enforcement).
- Test on both platforms in CI. Do not assume cross-platform equivalence.

**Detection:** Check `cat /sys/kernel/security/lsm` for Landlock presence on Linux. Check kernel version against Landlock ABI compatibility table.

**Phase relevance:** Phase 1 (policy generation) and install script.

---

### Pitfall 7: CronSync Race Conditions and State Corruption

**What goes wrong:** CronSync reconciles declarative YAML specs against live Claude Code cron state using CronCreate/CronList/CronDelete. Multiple failure modes exist:
1. Two agents running CronSync simultaneously can double-create or double-delete jobs.
2. CronList returns session-scoped data -- if the Claude Code session restarts, all cron jobs are gone, but the YAML specs still exist, causing the next reconcile to re-create everything.
3. Lock files with heartbeat-based TTL can become stale if the locking agent crashes without releasing -- but the TTL must be long enough that normal execution does not expire it.

**Why it happens:** Claude Code cron is session-scoped (jobs die when the process exits). CronSync tries to impose declarative state on an ephemeral runtime. The lock-file concurrency model is inherently racy on network filesystems and requires careful TTL tuning.

**Consequences:** Duplicate cron executions consuming double API quota. Missing cron jobs after agent restarts. Lock contention causing all agents to skip their cron cycles. Stale lock files blocking cron execution until manual cleanup.

**Prevention:**
- Accept that cron state is ephemeral: on every agent startup, CronSync should reconcile from scratch (create all jobs from YAML specs, do not assume previous state).
- Use atomic file operations for lock files (O_CREAT|O_EXCL, not check-then-create).
- Implement aggressive lock TTL with heartbeat renewal (heartbeat every 30s, TTL of 120s).
- Include lock file cleanup in `rightclaw down` and startup orphan detection.
- CronSync should be idempotent: running it twice produces the same result as running it once.

**Detection:** Multiple cron job IDs for the same spec name. Lock files with timestamps older than 2x TTL. Cron jobs not firing after agent restart.

**Phase relevance:** CronSync implementation phase. The ephemeral-vs-declarative impedance mismatch is fundamental to the design.

---

### Pitfall 8: Context Window Exhaustion in Long-Running Agents

**What goes wrong:** Claude Code sessions accumulate context over time. A session that starts at 5K tokens balloons to 50K+ after 30 minutes. Long-running agents (the entire point of RightClaw) will hit context limits, triggering auto-compaction that degrades reasoning quality, or hitting rate limits (TPM) as each API call includes the full context window.

**Why it happens:** Every tool call, file read, bash output, and conversation turn adds to the context. Autonomous agents make many tool calls per task. Auto-compaction fires at ~90% context, but by then the model is already degraded. Rate limits are shared across all agents on the same subscription.

**Consequences:** Agent performance degrades silently over time. Rate limit errors cascade across all agents (shared quota). Auto-compaction loses important context, causing agents to "forget" what they were doing. Agents making 8-12 API calls per task cycle hit RPM limits quickly.

**Prevention:**
- Design agents for short-lived sessions with explicit handoff. Use MEMORY.md to persist important state across sessions.
- Include periodic `/compact` in agent HEARTBEAT.md (compact at 60% context, not 90%).
- Stagger agent activity to avoid simultaneous API call bursts (process-compose startup delay).
- Budget for Max plan or API keys with sufficient quota for the number of agents.
- Consider implementing agent "sleep" -- agents that are idle should not keep sessions open accumulating context.

**Detection:** Monitor `/cost` and context size. Watch for increasing latency in agent responses (a sign of context bloat). Rate limit errors in Claude Code logs.

**Phase relevance:** Agent lifecycle design (Phase 1) and HEARTBEAT.md template design.

---

### Pitfall 9: OpenClaw File Convention Assumptions That Do Not Hold

**What goes wrong:** RightClaw claims drop-in compatibility with OpenClaw conventions (SOUL.md, USER.md, IDENTITY.md, etc.). But OpenClaw's conventions evolved organically and have undocumented assumptions: file load order, which files are optional vs required, how `metadata.openclaw` gates skill activation, whether BOOTSTRAP.md self-deletion is atomic, etc.

**Why it happens:** OpenClaw grew from 1 hour of code to 300K lines in 3 months. Conventions were added ad-hoc. ClawHub skills assume specific OpenClaw behaviors (like unrestricted filesystem access to read/write agent files). RightClaw's sandbox enforcement may break skills that assume they can write anywhere.

**Consequences:** "Drop-in compatible" breaks when a popular ClawHub skill tries to write outside the sandbox. Users blame RightClaw, not the skill. Subtle behavior differences in file loading order cause agents to behave differently than in OpenClaw.

**Prevention:**
- Audit the top 50 most-installed ClawHub skills for filesystem access patterns. Ensure the default OpenShell policy grants read/write to the agent's own directory tree.
- Document exactly which OpenClaw conventions RightClaw supports and which it does not.
- BOOTSTRAP.md self-deletion must work within the sandbox -- ensure the agent has write access to its own directory.
- Test with real ClawHub skills, not just the file format spec.

**Detection:** Skills that silently fail (no error, but no effect). Skills that error on file writes outside the agent directory.

**Phase relevance:** Compatibility testing phase, before claiming "drop-in compatible" publicly.

---

## Minor Pitfalls

### Pitfall 10: process-compose TUI Detach/Attach State Loss

**What goes wrong:** `rightclaw up -d` launches process-compose in detached mode with TUI server. `rightclaw attach` connects to it. But process-compose's TUI server does not replay full log history on attach -- the user sees only new output, missing everything that happened before attach.

**Prevention:** Stream agent logs to files as well as TUI. `rightclaw logs <agent>` should read from log files, not depend on TUI attachment.

---

### Pitfall 11: Malvertising Targeting RightClaw/Claude Code Downloads

**What goes wrong:** Kaspersky documented ongoing malvertising campaigns targeting "Claude Code download" and "OpenClaw download" search queries. As RightClaw gains visibility, it becomes a target.

**Prevention:** Distribute only via GitHub releases and crates.io. Sign releases. Include checksum verification in install.sh. Document the canonical download URL prominently.

---

### Pitfall 12: Anthropic Policy Changes Breaking Multi-Agent Use

**What goes wrong:** Anthropic has already blocked OAuth tokens from third-party tools (Jan 2026, no announcement). They could further restrict Claude Code CLI usage patterns -- e.g., rate limiting concurrent sessions, requiring per-session authentication, or blocking programmatic launching.

**Prevention:** Use API keys, not OAuth. Design for the possibility that Anthropic adds restrictions. Keep Claude Code as a replaceable component behind an abstraction, so RightClaw could theoretically support other LLM backends.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Core sandbox integration | OpenShell alpha instability (#2), process tree signals (#5) | Abstract OpenShell behind trait, explicit sandbox destroy on shutdown |
| Agent launching | OAuth token race (#3), context exhaustion (#8) | Use API keys, design for short-lived sessions |
| process-compose integration | TUI state loss (#10), network exposure (#4) | Log to files, bind 127.0.0.1 only |
| ClawHub skill installation | Supply chain poisoning (#1), convention mismatch (#9) | Policy gate mandatory, audit top skills |
| CronSync | Race conditions (#7) | Idempotent reconciliation, atomic locks |
| Cross-platform support | Sandbox divergence (#6) | Use `hard_requirement`, check kernel version |
| Public release | Malvertising (#11), Anthropic policy (#12) | Signed releases, API key default |

## Sources

- [XDA: Please stop using OpenClaw](https://www.xda-developers.com/please-stop-using-openclaw/)
- [MetricNexus: Is OpenClaw Allowed in Claude Code?](https://metricnexus.ai/blog/is-openclaw-allowed-in-claude-code)
- [eSecurity Planet: Hundreds of Malicious Skills Found in ClawHub](https://www.esecurityplanet.com/threats/hundreds-of-malicious-skills-found-in-openclaws-clawhub/)
- [PointGuard AI: ClawHub Supply Chain Attack](https://www.pointguardai.com/ai-security-incidents/openclaw-clawhub-malicious-skills-supply-chain-attack)
- [Trend Micro: OpenClaw Skills Distributing Atomic macOS Stealer](https://www.trendmicro.com/en_us/research/26/b/openclaw-skills-used-to-distribute-atomic-macos-stealer.html)
- [VirusTotal: How OpenClaw Skills Are Being Weaponized](https://blog.virustotal.com/2026/02/from-automation-to-infection-how.html)
- [Bitdefender: OpenClaw Exploitation in Enterprise Networks](https://businessinsights.bitdefender.com/technical-advisory-openclaw-exploitation-enterprise-networks)
- [CyberPress: ClawHavoc Poisons ClawHub With 1,184 Malicious Skills](https://cyberpress.org/clawhavoc-poisons-openclaws-clawhub-with-1184-malicious-skills/)
- [NVIDIA OpenShell GitHub](https://github.com/NVIDIA/OpenShell)
- [NVIDIA OpenShell Developer Guide: Policies](https://docs.nvidia.com/openshell/latest/sandboxes/policies.html)
- [NVIDIA OpenShell Developer Guide: Default Policy](https://docs.nvidia.com/openshell/latest/reference/default-policy.html)
- [NemoClaw WSL2 Bug: Issue #208](https://github.com/NVIDIA/NemoClaw/issues/208)
- [GitHub: OAuth token refresh race condition (Issue #27933)](https://github.com/anthropics/claude-code/issues/27933)
- [Claude Code Docs: Scheduled Tasks](https://code.claude.com/docs/en/scheduled-tasks)
- [SitePoint: Claude Code Context Management](https://www.sitepoint.com/claude-code-context-management/)
- [SitePoint: Claude Code Rate Limits](https://www.sitepoint.com/claude-code-rate-limits-explained/)
- [Process Compose: Processes Lifetime](https://f1bonacc1.github.io/process-compose/launcher/)
- [Process Compose GitHub](https://github.com/F1bonacc1/process-compose)
- [Linux Kernel: Landlock Documentation](https://docs.kernel.org/userspace-api/landlock.html)
- [Pierce Freeman: Deep Dive on Agent Sandboxes](https://pierce.dev/notes/a-deep-dive-on-agent-sandboxes)
- [Immersive Labs: Why You Should Uninstall OpenClaw](https://www.immersivelabs.com/resources/c7-blog/openclaw-what-you-need-to-know-before-it-claws-its-way-into-your-organization)
- [TechRadar: Infostealers Disguised as Claude Code and OpenClaw](https://www.techradar.com/pro/security/infostealers-are-being-disguised-as-claude-code-openclaw-and-other-ai-developer-tools)
