# Architecture Research

**Domain:** Multi-agent CLI runtime (Rust CLI wrapping process-compose + OpenShell)
**Researched:** 2026-03-21
**Confidence:** HIGH

## System Overview

```
                            User
                             |
                      rightclaw CLI
                             |
              +--------------+--------------+
              |              |              |
        Agent Discovery  Config Gen    PC Lifecycle
        (scan agents/)   (YAML emit)   (spawn/attach/stop)
              |              |              |
              v              v              v
        +----------+  +-----------+  +----------------+
        | AgentDef |->| PCConfig  |->| process-compose|
        | (parsed) |  | Generator |  | (child proc)   |
        +----------+  +-----------+  +----------------+
              |                            |
              v                       per-agent process
        +----------+              +---------------------+
        | Policy   |              | Shell Wrapper       |
        | Resolver |              | (generated script)  |
        +----------+              +--------|------------+
              |                            |
              v                            v
        +----------+              +---------------------+
        | Merged   |              | openshell sandbox   |
        | Policy   |              | create --policy ... |
        | YAML     |              | -- claude ...       |
        +----------+              +---------------------+
                                           |
                                           v
                                  +---------------------+
                                  | Claude Code session |
                                  | (sandboxed agent)   |
                                  +---------------------+
```

### Component Responsibilities

| Component | Responsibility | Boundary |
|-----------|----------------|----------|
| **CLI (clap)** | Parse commands, dispatch to subsystems | Entry point; no business logic beyond arg routing |
| **Agent Discovery** | Scan `agents/` dir, parse `agent.yaml`, validate structure | Returns `Vec<AgentDef>`, never generates config |
| **Policy Resolver** | Find policy per agent (agent-local > `policies/` > default), merge skill policies | Returns `PathBuf` to resolved policy YAML |
| **Config Generator** | Emit `process-compose.yaml` from agent defs + resolved policies | Pure function: `(Vec<AgentDef>, Options) -> String` |
| **Shell Wrapper Generator** | Write per-agent `run-<name>.sh` scripts that invoke `openshell sandbox create` | Writes to temp dir, returns script paths |
| **PC Lifecycle** | Spawn/attach/status/restart/stop process-compose | Owns the child process handle |
| **ClawHub Client** | HTTP client for ClawHub API (search/install/uninstall skills) | Claude Code skill, not CLI component |
| **CronSync** | Reconcile cron YAML specs with live CC cron jobs | Claude Code skill, not CLI component |

## Recommended Project Structure

```
src/
├── main.rs                # Entry: clap CLI definition, dispatch
├── cli.rs                 # Clap command/subcommand definitions
├── agent/
│   ├── mod.rs             # AgentDef struct, discovery logic
│   ├── discovery.rs       # Scan agents/ dir, parse agent.yaml
│   └── types.rs           # AgentDef, AgentConfig structs
├── policy/
│   ├── mod.rs             # Policy resolver
│   └── merge.rs           # Merge agent policy + skill policies (future)
├── codegen/
│   ├── mod.rs             # Re-exports
│   ├── process_compose.rs # Generate process-compose.yaml
│   └── shell_wrapper.rs   # Generate per-agent run scripts
├── runtime/
│   ├── mod.rs             # PC lifecycle management
│   └── process_compose.rs # Spawn, attach, status, restart, down
└── error.rs               # Unified error types
```

### Structure Rationale

- **agent/:** Isolated from codegen so discovery can be tested independently. `AgentDef` is the central data structure everything else consumes.
- **policy/:** Separate from agent because policy resolution has its own search logic (agent dir, policies/ dir, defaults) and will grow when skill-level policies need merging.
- **codegen/:** Pure transformation layer. Takes typed data, emits files. No I/O side effects beyond writing to a temp dir. Easy to snapshot-test.
- **runtime/:** The only module that spawns child processes. Clean boundary for testing (mock the process spawner).

## Data Flow

### `rightclaw up` Flow

```
User: rightclaw up ~/project --agents watchdog,reviewer

1. CLI parses args
   ↓
2. Agent Discovery
   - Scans ~/project/agents/
   - Filters by --agents if specified
   - Parses agent.yaml per agent (or applies defaults)
   - Validates: IDENTITY.md must exist
   ↓ Vec<AgentDef>
3. Policy Resolution (per agent)
   - Check agents/<name>/policy.yaml → use if exists
   - Else check policies/<name>.yaml → use if exists
   - Else use built-in default policy
   ↓ Vec<(AgentDef, PathBuf)>
4. Shell Wrapper Generation
   - For each agent, emit /tmp/rightclaw/<hash>/run-<name>.sh:
     #!/bin/bash
     exec openshell sandbox create \
       --policy /abs/path/to/policy.yaml \
       -- claude \
         --append-system-prompt-file /abs/path/agents/<name>/IDENTITY.md \
         --dangerously-skip-permissions \
         -p /abs/path/to/project \
         --prompt "<start_prompt>"
   - chmod +x
   ↓ Vec<PathBuf> (script paths)
5. process-compose.yaml Generation
   - Map each agent to a process entry:
     - command: /tmp/rightclaw/<hash>/run-<name>.sh
     - working_dir: project path
     - availability: from agent.yaml or defaults
   - Write to /tmp/rightclaw/<hash>/process-compose.yaml
   ↓ PathBuf
6. process-compose Spawn
   - rightclaw up    → process-compose up -f <yaml>
   - rightclaw up -d → process-compose up -f <yaml> --tui=false
   ↓ Child process (or detached)
```

### `rightclaw attach` Flow

```
1. Find running process-compose socket
   - Default: /tmp/rightclaw/<hash>/process-compose.sock
   - Or from state file: /tmp/rightclaw/state.json
2. Execute: process-compose attach --unix-socket <sock>
```

### `rightclaw status` Flow

```
1. Find process-compose socket
2. Query via process-compose REST API or CLI:
   process-compose process list --unix-socket <sock> --output json
3. Format and display agent states
```

### `rightclaw restart <agent>` Flow

```
1. Find process-compose socket
2. Execute: process-compose process restart <agent> --unix-socket <sock>
```

### `rightclaw down` Flow

```
1. Find process-compose socket
2. Execute: process-compose down --unix-socket <sock>
3. Clean up /tmp/rightclaw/<hash>/
```

### Key Data Structures

```rust
/// Parsed from agents/<name>/agent.yaml (all optional with defaults)
struct AgentConfig {
    restart: RestartPolicy,        // default: OnFailure
    max_restarts: u32,             // default: 5
    backoff_seconds: u32,          // default: 10
    start_prompt: Option<String>,  // default: generic "restore context" prompt
}

/// Discovered from scanning agents/ directory
struct AgentDef {
    name: String,                  // directory name
    dir: PathBuf,                  // absolute path to agents/<name>/
    identity_file: PathBuf,        // agents/<name>/IDENTITY.md
    config: AgentConfig,           // parsed agent.yaml or defaults
    mcp_config: Option<PathBuf>,   // agents/<name>/.mcp.json if exists
    has_crons: bool,               // agents/<name>/crons/ exists
}

/// Runtime state persisted to /tmp/rightclaw/state.json
struct RuntimeState {
    project_dir: PathBuf,
    config_dir: PathBuf,           // /tmp/rightclaw/<hash>/
    socket_path: PathBuf,
    agents: Vec<String>,
    started_at: String,            // ISO 8601
}

enum RestartPolicy { Never, OnFailure, Always }
```

## Architectural Patterns

### Pattern 1: Code Generation over Runtime Templating

**What:** Generate static YAML and shell scripts to disk, then hand them to process-compose. Do not template at runtime or pass complex args through environment variables.

**When to use:** Always for this project. process-compose reads files; OpenShell reads files. Generating files is the natural interface.

**Trade-offs:**
- Pro: Debuggable (inspect generated files), testable (snapshot tests), no runtime templating bugs
- Pro: process-compose gets a normal YAML file it can hot-reload
- Con: Temp directory management, cleanup on crash

```rust
// codegen/process_compose.rs
pub fn generate(agents: &[AgentDef], opts: &GenOptions) -> String {
    let mut yaml = serde_yaml::Mapping::new();
    // ... build process entries from agent defs
    serde_yaml::to_string(&yaml).unwrap()
}

// In runtime: write to disk, then spawn process-compose pointing at it
```

### Pattern 2: Layered Defaults

**What:** Every config field has a built-in default. `agent.yaml` overrides. CLI flags override `agent.yaml`.

**When to use:** For agent configuration. Users should get working behavior with zero config.

**Trade-offs:**
- Pro: Zero-config MVP works immediately
- Con: Need clear documentation of default values and override order

### Pattern 3: Process Socket for Lifecycle

**What:** Use process-compose's Unix socket API for all lifecycle operations after initial spawn. Store socket path in state file.

**When to use:** For `status`, `restart`, `down`, `attach` commands.

**Trade-offs:**
- Pro: No PID tracking, no signal management, no race conditions
- Pro: process-compose handles all the hard process management
- Con: Depends on process-compose's socket being available and stable

### Pattern 4: Thin CLI, Fat Config

**What:** The CLI itself has minimal logic. It discovers, resolves, generates, and delegates. All intelligence is in the config generation and in the agents themselves (Claude Code sessions with skills).

**When to use:** This is the core architectural principle. RightClaw is a launcher, not an orchestrator.

**Trade-offs:**
- Pro: Simple codebase, easy to maintain
- Pro: Agents are autonomous (no coupling to the CLI)
- Con: Limited ability to coordinate between agents (by design for v1)

## Anti-Patterns

### Anti-Pattern 1: Building a Process Manager

**What people do:** Implement restart logic, health checks, log aggregation in the CLI.
**Why it's wrong:** process-compose already does all of this with a mature TUI. Reimplementing it means maintaining a buggy subset.
**Do this instead:** Generate correct process-compose config. Let process-compose handle process lifecycle. Use its REST API for queries.

### Anti-Pattern 2: Dynamic Policy Assembly at Runtime

**What people do:** Build OpenShell policies by merging fragments in memory and passing them through environment variables or stdin.
**Why it's wrong:** Policies are security-critical. Dynamic assembly is hard to audit, hard to debug, easy to get wrong.
**Do this instead:** Write resolved policies to disk as static YAML files. Each agent gets one policy file. Audit by reading the file.

### Anti-Pattern 3: Agent Coordination Through CLI

**What people do:** Build inter-agent messaging, shared state, or coordination protocols into the CLI.
**Why it's wrong:** Agents are independent Claude Code sessions. The CLI is a launcher. Mixing concerns makes both harder to maintain.
**Do this instead:** For v1, agents don't coordinate. For v2, use an MCP memory server that agents connect to independently. The CLI never mediates.

### Anti-Pattern 4: Hardcoded Agent Definitions

**What people do:** Define agent behavior in the CLI code rather than in the agent directory structure.
**Why it's wrong:** Makes adding/removing agents a code change instead of a directory operation.
**Do this instead:** The CLI discovers agents from `agents/`. It knows nothing about what a "watchdog" or "reviewer" does. An agent is just: a name (dir), an identity (IDENTITY.md), optional config (agent.yaml), optional policy.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| **process-compose** | Child process spawn + Unix socket API | Socket for lifecycle ops. CLI must find/store socket path. |
| **OpenShell** | Via generated shell wrapper scripts | `openshell sandbox create --policy <file> -- claude ...` |
| **Claude Code CLI** | Invoked inside OpenShell sandbox | `--append-system-prompt-file`, `--dangerously-skip-permissions`, `-p` |
| **ClawHub API** | HTTP client inside a Claude Code skill | Not a CLI concern. The `/clawhub` skill handles this. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| CLI -> Agent Discovery | Function call, returns `Vec<AgentDef>` | Pure data, no side effects |
| Agent Discovery -> Policy Resolver | Per-agent `AgentDef` -> `PathBuf` | File existence checks only |
| Policy Resolver -> Shell Wrapper Gen | `(AgentDef, PolicyPath)` -> script file | Writes to temp dir |
| Shell Wrapper Gen -> PC Config Gen | Script paths feed into YAML | Pure string generation |
| PC Config Gen -> PC Lifecycle | YAML path -> child process spawn | Only place with side effects |

## Build Order (Dependencies Between Components)

The build order is dictated by data flow dependencies. Each layer depends only on layers above it.

```
Phase 1: Foundation
├── error.rs          (no deps)
├── agent/types.rs    (no deps, just structs)
└── cli.rs            (clap defs, no logic)

Phase 2: Discovery
└── agent/discovery.rs  (depends on: types)
    - Scan agents/ dir
    - Parse agent.yaml (serde_yaml)
    - Validate structure

Phase 3: Policy Resolution
└── policy/mod.rs  (depends on: types)
    - Search order: agent dir > policies/ > default
    - Return path to resolved policy

Phase 4: Code Generation
├── codegen/shell_wrapper.rs  (depends on: types)
│   - Generate per-agent run-<name>.sh
│   - Template: openshell sandbox create --policy ... -- claude ...
└── codegen/process_compose.rs  (depends on: types)
    - Generate process-compose.yaml
    - Map AgentDef + script paths to PC process entries

Phase 5: Runtime
└── runtime/process_compose.rs  (depends on: all above)
    - Spawn process-compose
    - Store state (socket path, config dir)
    - Implement attach/status/restart/down

Phase 6: Wire It Up
└── main.rs  (depends on: all)
    - CLI dispatch: match subcommand, call subsystems
```

### Why This Order

1. **Types first** because everything depends on `AgentDef` and `AgentConfig`.
2. **Discovery before codegen** because codegen consumes discovered agents. You can test discovery in isolation with fixture directories.
3. **Policy resolution in parallel with discovery** — both are independent lookups, but policy feeds into shell wrapper gen.
4. **Codegen before runtime** because runtime just spawns what codegen produced. Codegen is pure and easily snapshot-tested.
5. **Runtime last** because it's the only module with real side effects (child processes, sockets). Hardest to test, fewest changes needed.

## Skill Management Architecture (ClawHub)

This is a **Claude Code skill**, not a CLI component. It runs inside an agent's CC session.

```
User: "install TheSethRose/agent-browser"
  ↓
Claude invokes /clawhub skill
  ↓
┌──────────────────────────────────────┐
│ /clawhub install                      │
│                                       │
│ 1. HTTP GET clawhub.dev/api/search    │
│    → find skill by name               │
│ 2. git clone skill repo               │
│    → into .claude/skills/<name>/      │
│ 3. Parse SKILL.md frontmatter         │
│    → extract metadata.openshell       │
│ 4. Policy gate                        │
│    → audit requested permissions      │
│    → block suspicious patterns        │
│    → prompt user for confirmation     │
│ 5. Register in skills/installed.json  │
│ 6. Hot-reload: tell OpenShell to      │
│    update network_policies if needed  │
└──────────────────────────────────────┘
```

The CLI has **no role** in skill management. Skills are installed and managed within agent sessions. The CLI only discovers what exists in `agents/<name>/skills/` for the purpose of policy resolution (if skill policies need merging).

## CronSync Architecture

Also a Claude Code skill, not a CLI component. Each agent runs its own CronSync independently.

```
/loop 5m /cronsync
  ↓ (every 5 min)
1. Read agents/<name>/crons/*.yaml → desired state
2. CronList tool → actual state
3. Load crons/state.json → name↔ID mapping
4. Reconcile:
   - Missing job → CronCreate, save ID
   - Orphan job  → CronDelete, remove ID
   - Changed spec → Delete + Create, update ID
   - Match → skip
5. Write updated crons/state.json
```

Lock-file concurrency control prevents overlapping cron executions. Lock files live in `crons/.locks/<name>.json` with heartbeat timestamps.

## Default "Right" Agent Bootstrap

```
agents/right/
├── IDENTITY.md      # "I am Right, your general-purpose assistant"
├── BOOTSTRAP.md     # First-run onboarding flow
├── SOUL.md          # Personality/values template
├── AGENTS.md        # Operational framework
├── MEMORY.md        # Empty, populated over time
├── agent.yaml       # restart: on_failure, defaults
└── skills/
    └── cronsync/
        └── SKILL.md

Bootstrap flow (BOOTSTRAP.md):
1. Detect first run (IDENTITY.md has placeholder values)
2. Ask user: name, vibe, personality preferences
3. Write IDENTITY.md with user's choices
4. Write USER.md with user context
5. Write SOUL.md with personality
6. Delete BOOTSTRAP.md (self-removing)
7. Continue as configured agent
```

## Temp Directory Layout

```
/tmp/rightclaw/<hash>/
├── process-compose.yaml    # Generated PC config
├── run-watchdog.sh          # Shell wrapper for watchdog agent
├── run-reviewer.sh          # Shell wrapper for reviewer agent
├── run-right.sh             # Shell wrapper for right agent
├── state.json               # Runtime state (socket path, agents, etc.)
└── process-compose.sock     # Unix socket (created by PC)
```

`<hash>` is derived from the absolute path of the project directory. This allows multiple rightclaw instances for different projects to coexist.

## Sources

- [process-compose documentation](https://f1bonacc1.github.io/process-compose/)
- [process-compose configuration reference](https://f1bonacc1.github.io/process-compose/configuration/)
- [NVIDIA OpenShell Developer Guide](https://docs.nvidia.com/openshell/latest/index.html)
- [OpenShell Policy Schema Reference](https://docs.nvidia.com/openshell/latest/reference/policy-schema.html)
- [OpenShell Sandbox Management](https://docs.nvidia.com/openshell/latest/sandboxes/manage-sandboxes.html)
- [OpenShell Custom Policies](https://docs.nvidia.com/openshell/latest/sandboxes/policies.html)
- [OpenShell GitHub](https://github.com/NVIDIA/OpenShell)

---
*Architecture research for: RightClaw multi-agent CLI runtime*
*Researched: 2026-03-21*
