# Architecture

## Workspace

Three crates in a Cargo workspace:

| Crate | Path | Role |
|-------|------|------|
| **rightclaw** | `crates/rightclaw/` | Core library — agent discovery, codegen, config, memory, runtime, MCP, OpenShell |
| **rightclaw-cli** | `crates/rightclaw-cli/` | CLI binary (`rightclaw`) + memory MCP server (stdio/HTTP) |
| **rightclaw-bot** | `crates/bot/` | Telegram bot runtime (teloxide) + cron engine |

## Module Map

### rightclaw (core)

```
src/
├── agent/
│   ├── discovery.rs    # Scan agents/ dir, validate names, parse agent.yaml
│   └── types.rs        # AgentDef, AgentConfig, RestartPolicy, SandboxOverrides
├── config/
│   └── config.rs       # GlobalConfig (tunnel, chrome), RIGHTCLAW_HOME resolution
├── codegen/
│   ├── agent_def.rs    # .md with frontmatter for CC agent definition
│   ├── claude_json.rs  # .claude.json — trust, onboarding, credential/plugin symlinks
│   ├── settings.rs     # .claude/settings.json — behavioral flags
│   ├── mcp_config.rs   # .mcp.json — rightmemory + external MCP entries
│   ├── policy.rs       # OpenShell policy.yaml — network/filesystem sandbox rules
│   ├── process_compose.rs  # process-compose.yaml via minijinja
│   ├── skills.rs       # Install built-in skills to .claude/skills/
│   ├── telegram.rs     # Telegram-specific codegen helpers
│   ├── cloudflared.rs  # Tunnel entry generation
│   └── plugin.rs       # Plugin symlink management
├── memory/
│   ├── store.rs        # SQLite: store, recall, search (FTS5/BM25), forget
│   ├── migrations.rs   # Schema versioning (rusqlite_migration)
│   ├── guard.rs        # Prompt injection detection
│   └── error.rs        # MemoryError types
├── runtime/
│   ├── state.rs        # RuntimeState persistence (JSON)
│   ├── pc_client.rs    # process-compose REST API client (port 18927)
│   └── deps.rs         # Binary availability checks
├── mcp/
│   ├── credentials.rs  # OAuth token persistence
│   ├── detect.rs       # MCP server detection from .claude.json
│   ├── oauth.rs        # OAuth flow initiation
│   └── refresh.rs      # Token refresh
├── openshell.rs        # gRPC mTLS — sandbox spawn, readiness polling, SSH
├── doctor.rs           # Diagnostic checks (deps, structure, MCP, sandbox, tunnel)
├── init.rs             # rightclaw init workflow
└── error.rs            # AgentError (miette diagnostics)
```

### rightclaw-cli

```
src/
├── main.rs               # CLI dispatcher (clap): init, list, doctor, up, down, status, restart, attach, pair, config, memory, mcp, bot, memory-server
├── memory_server.rs      # MCP stdio server — store/recall/search/forget + cron + mcp management
└── memory_server_http.rs # Optional HTTP transport for MCP
```

### rightclaw-bot

```
src/
├── lib.rs              # Entry: resolve agent dir, open memory.db, start teloxide
├── telegram/
│   ├── mod.rs          # Token resolution (env > file > yaml)
│   ├── bot.rs          # Bot adaptor: CacheMe<Throttle<Bot>>
│   ├── dispatch.rs     # Long-polling dispatcher setup
│   ├── handler.rs      # /start, /reset, /mcp, /doctor + text→worker routing
│   ├── worker.rs       # Per-session CC invocation, reply parsing, media upload
│   ├── session.rs      # telegram_sessions table (chat_id, thread_id, uuid)
│   ├── filter.rs       # Allowed chat ID enforcement
│   └── oauth_callback.rs  # Axum OAuth redirect server
├── cron.rs             # Cron engine: load specs, lock check, invoke CC, notify
└── error.rs            # BotError types
```

## Data Flow

### Agent Lifecycle

```
rightclaw init
  ├─ Create ~/.rightclaw/agents/<name>/ with template files
  ├─ Write IDENTITY.md, SOUL.md, USER.md, agent.yaml
  ├─ Generate .claude/settings.json, .claude.json
  ├─ Symlink credentials + plugins from ~/.claude/
  ├─ Detect Telegram token, cloudflared tunnel, Chrome binary
  └─ Write ~/.rightclaw/config.yaml

rightclaw up [--agents x,y] [--detach] [--no-sandbox]
  ├─ Discover agents from agents/ directory
  ├─ Per agent:
  │   ├─ Parse agent.yaml → AgentConfig
  │   ├─ Codegen: agent_def.md, settings.json, .claude.json, .mcp.json
  │   ├─ Generate OpenShell policy.yaml (rightmemory + external MCPs)
  │   ├─ Spawn sandbox (openshell gRPC), poll until READY
  │   └─ Open + migrate memory.db
  ├─ Install built-in skills
  ├─ Generate process-compose.yaml (minijinja)
  └─ Launch process-compose (TUI or detached)

rightclaw bot --agent <name>  (spawned by process-compose)
  ├─ Resolve token, open memory.db
  ├─ Start teloxide long-polling dispatcher
  ├─ Start cron engine (agents/crons/*.yaml)
  └─ Per message:
      ├─ Route to worker task via DashMap<(chat_id, thread_id), Sender>
      ├─ Worker: debounce 60s → invoke CC → parse reply JSON
      └─ Send Telegram response (text + media)

rightclaw down
  └─ POST /project/stop to process-compose REST API
```

### Configuration Hierarchy

| Scope | File | Source of Truth |
|-------|------|-----------------|
| Global | `~/.rightclaw/config.yaml` | Tunnel, Chrome config |
| Per-agent | `agents/<name>/agent.yaml` | Restart, model, telegram, sandbox overrides, env |
| Generated | `agents/<name>/.claude/settings.json` | CC behavioral flags (regenerated on `up`) |
| Generated | `agents/<name>/.claude.json` | Trust, onboarding suppression (read-modify-write) |
| Generated | `agents/<name>/.mcp.json` | MCP server entries (rightmemory + external) |
| Generated | `run/policies/<agent>.yaml` | OpenShell sandbox policy |

### Memory Schema (SQLite)

```
memories        (id, content, tags, stored_by, source_tool, created_at, deleted_at, importance)
memory_events   (memory_id, event_type, actor, timestamp)
memories_fts    (FTS5 virtual table — BM25 ranking)
telegram_sessions (chat_id, effective_thread_id, session_uuid, created_at)
cron_runs       (job_name, run_id, started_at, completed_at, exit_code, log_path)
```

## Key Types

```rust
AgentDef        // Discovered agent: name, path, identity, config, optional files
AgentConfig     // From agent.yaml: restart, model, telegram, sandbox overrides, env
GlobalConfig    // From config.yaml: tunnel, chrome
RuntimeState    // Persisted JSON: agents, socket_path, started_at
MemoryEntry     // SQLite row: id, content, tags, stored_by, importance
WorkerContext   // Per-session: chat_id, thread_id, agent_dir, bot, db, ssh config
ProcessInfo     // From process-compose API: name, status, pid, exit_code
```

## External Integrations

| System | Protocol | Notes |
|--------|----------|-------|
| process-compose | REST API (TCP :18927) | Health, process CRUD, logs, shutdown |
| Claude Code CLI | Subprocess (`claude -p`) | HOME override to agent dir, env injection |
| OpenShell | gRPC + mTLS (:8080) | Sandbox create/poll, policy upload, SSH config |
| Telegram | teloxide long-polling | CacheMe<Throttle<Bot>> adaptor, per-agent allowlist |
| Cloudflare Tunnel | CLI (`cloudflared`) | Named tunnel, DNS CNAME, credentials file |
| MCP | stdio + optional HTTP | rightmemory built-in, external via OAuth |

## Security Model

- **Sandbox isolation**: OpenShell (bwrap on Linux, Seatbelt on macOS) — filesystem + network policies per agent
- **Prompt injection detection**: Pattern matching in memory guard before SQLite insert
- **Chat ID allowlist**: Empty = block all (secure default); per-agent in agent.yaml
- **Credential sharing**: Symlinks from host `~/.claude/` — no credential duplication
- **Protected MCP**: "rightmemory" cannot be removed via `/mcp remove`
- **OAuth CSRF**: Token matching in callback server

## Directory Layout (Runtime)

```
~/.rightclaw/
├── config.yaml
├── agents/<name>/
│   ├── IDENTITY.md, SOUL.md, USER.md, AGENTS.md, ...
│   ├── agent.yaml
│   ├── memory.db
│   ├── crons/*.yaml
│   ├── staging/
│   └── .claude/
│       ├── settings.json, .mcp.json
│       ├── .credentials.json → ~/.claude/.credentials.json
│       ├── plugins → ~/.claude/plugins
│       └── skills/
├── run/
│   ├── process-compose.yaml
│   ├── policies/<agent>.yaml
│   └── runtime-state.json
└── scripts/
    └── cloudflared-start.sh
```
