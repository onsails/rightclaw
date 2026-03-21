# Technology Stack

**Project:** RightClaw
**Researched:** 2026-03-21
**Overall Confidence:** HIGH

## Async vs Sync Decision

**Decision: Use Tokio (async).** RightClaw makes HTTP calls to process-compose's REST API and to ClawHub's HTTP API. `reqwest` (the only serious Rust HTTP client) is async-first. The CLI also needs to monitor multiple agent processes and react to signals concurrently. Fighting async at the boundary with `block_on` wrappers everywhere is worse than embracing it. The compile-time cost is acceptable for a CLI of this complexity.

## Recommended Stack

### Core Framework

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| clap | 4.6.0 | CLI argument parsing | Industry standard. Derive API is ergonomic. Powers ripgrep, fd, bat. Subcommand model maps perfectly to `rightclaw up/down/status/attach/restart`. | HIGH |
| tokio | 1.50.0 | Async runtime | Required by reqwest. Process monitoring and HTTP calls benefit from async. Use `features = ["full"]` for process spawning + signal handling. | HIGH |
| serde | 1.0.228 | Serialization framework | Non-negotiable for any Rust project touching structured data. | HIGH |

### YAML Parsing

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| serde-saphyr | 0.0.22 | YAML deserialization | `serde_yaml` is deprecated (archived March 2024). `serde-saphyr` is the best replacement: panic-free on malformed input, no unsafe code, 1000+ passing tests including full yaml-test-suite, outperforms alternatives in benchmarks. The `serde_yml` fork (0.0.12) has lower adoption and less rigorous testing. | MEDIUM |

**Why not `serde_yml`:** Lower test coverage, less active development, just a fork of the deprecated crate without fundamental improvements. `serde-saphyr` is a ground-up rewrite with better safety properties.

**Why not `saphyr-serde`:** Part of the saphyr project's own serde layer, but not yet released as stable. `serde-saphyr` is independent and already battle-tested.

**Risk:** `serde-saphyr` is at 0.0.22 -- pre-1.0 means possible breaking changes. Pin the exact version and vendor the dependency if stability is critical.

### Template Generation

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| minijinja | 2.18.0 | Generate process-compose.yaml | Jinja2-compatible, minimal dependencies (only serde), actively maintained by Armin Ronacher. Used by crates.io itself. Perfect for generating YAML configs from agent directory scan results. Avoids string concatenation for YAML generation. | HIGH |

**Why not Tera:** Heavier dependency tree, diverges from Jinja2 syntax. MiniJinja is more focused and lighter.

**Why not Askama:** Compile-time templates are overkill here -- we need runtime template rendering since the template content may vary.

**Why not raw string formatting:** Process-compose YAML has enough structure (nested maps, lists, conditional fields) that a template engine prevents YAML formatting bugs.

### HTTP Client

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| reqwest | 0.13.2 | HTTP client for process-compose REST API and ClawHub API | 300M+ downloads. Async-first, ergonomic, handles JSON/headers/auth. Only serious choice for async Rust HTTP. | HIGH |

**Why not `ureq`:** `ureq` 3.x is sync-only (no async). Since we already committed to tokio for process monitoring, using reqwest avoids a split personality.

### Error Handling

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| thiserror | 2.0.18 | Structured error types | Derive macro for library-quality error enums. Use for internal error types (AgentError, PolicyError, ConfigError). | HIGH |
| miette | 7.6.0 | User-facing diagnostic errors | Compiler-style error output with source snippets, labels, suggestions. When a user's `agent.yaml` has invalid config, miette shows exactly where. Better UX than color-eyre for config-heavy CLIs. | HIGH |

**Why not `color-eyre`:** Repository archived (Aug 2024). `miette` is actively maintained and provides richer diagnostic output for configuration errors, which is the primary error surface in RightClaw.

**Why not `anyhow`:** Too generic. `thiserror` for structured errors + `miette` for presentation is the 2025-2026 best practice for CLIs that parse user config.

### Process Management

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| std::process::Command | stdlib | Spawn process-compose and openshell | Standard library is sufficient for launching external processes. No need for exotic crates. | HIGH |
| tokio::process | 1.50.0 (via tokio) | Async process spawning/monitoring | Async `Child` with `kill_on_drop`. Use for launching `process-compose up` and monitoring its lifecycle. | HIGH |
| ctrlc | 3.5.2 | Signal handling (SIGINT/SIGTERM) | Cross-platform Ctrl+C handling. Triggers graceful shutdown: sends `process-compose down` before exit. Simple API, well-maintained. | HIGH |

**Why not `signal-hook`:** More power than needed. RightClaw only needs SIGINT/SIGTERM to trigger graceful shutdown. `ctrlc` with `termination` feature covers this.

**Why not `nix` crate for signals:** Requires unsafe, Unix-only. `ctrlc` abstracts this cleanly.

### Logging/Tracing

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| tracing | 0.1.44 | Structured logging | Ecosystem standard. Structured spans for "agent-x starting", "policy validation", etc. | HIGH |
| tracing-subscriber | 0.3.23 | Log output formatting | `fmt` subscriber with `EnvFilter` for `RUST_LOG` support. Use `json` feature for machine-readable logs in detached mode. | HIGH |

### Testing

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| assert_cmd | 2.2.0 | CLI integration testing | Run `rightclaw` binary, assert on stdout/stderr/exit codes. The standard for Rust CLI testing. | HIGH |
| predicates | 3.1.4 | Assertion helpers | Pairs with assert_cmd. `predicate::str::contains("agent started")` style assertions. | HIGH |
| tempfile | (latest) | Temp directories for test agent layouts | Create isolated agent directory structures per test. | HIGH |

**Why not `trycmd`:** Snapshot testing is good for stable CLIs. RightClaw is greenfield -- output format will change frequently. `assert_cmd` gives more control during early development. Consider `trycmd` later for regression testing once output stabilizes.

### Supporting Libraries

| Library | Version | Purpose | When to Use | Confidence |
|---------|---------|---------|-------------|------------|
| dirs | latest | XDG/platform directories | Locate config dirs, cache dirs for RightClaw state. | HIGH |
| which | latest | Find executables in PATH | Verify `process-compose`, `openshell`, `claude` are installed before `rightclaw up`. | HIGH |
| walkdir | latest | Directory traversal | Scan `agents/` directory to discover agent subdirectories. | HIGH |

## External Dependencies (Not Rust Crates)

| Technology | Version | Purpose | Integration Pattern |
|------------|---------|---------|---------------------|
| process-compose | v1.100.0+ | Process orchestration + TUI | RightClaw generates `process-compose.yaml`, launches `process-compose up`. Controls via REST API on `localhost:8080` (or UDS). Auth via `PC_API_TOKEN`. |
| OpenShell | latest (alpha) | Sandbox enforcement | CLI invocation: `openshell sandbox create --policy <path> -- claude`. Each agent process in process-compose.yaml wraps its command with openshell. |
| Claude Code CLI | latest | AI agent sessions | Launched inside OpenShell sandboxes. Not directly managed by RightClaw -- process-compose handles lifecycle. |

## Integration Patterns

### process-compose Integration

```
rightclaw up
  1. Scan agents/ directory
  2. For each agent: read agent.yaml, resolve policy.yaml
  3. Render process-compose.yaml via minijinja template
  4. Launch: process-compose up -f <generated>.yaml [-t=false for detached]
  5. Monitor via REST API: GET http://localhost:{port}/process/{name}/
  6. Auth: PC_API_TOKEN env var, X-PC-Token-Key header

rightclaw status  -> GET /processes (via REST API or `process-compose process list`)
rightclaw restart -> POST /process/{name}/restart (via REST API)
rightclaw down    -> process-compose down (or POST /project/stop)
rightclaw attach  -> process-compose attach (delegates to PC's TUI client)
```

**REST API** is preferred over shelling out to `process-compose` CLI for status/restart/stop because it avoids spawning subprocesses for every operation and provides structured JSON responses.

**Unix Domain Sockets** are preferred over TCP for local communication -- use `process-compose -U` to auto-create socket at `<TempDir>/process-compose-<pid>.sock`. Store the socket path in RightClaw's state file for `rightclaw status` to reconnect.

### OpenShell Integration

```
# Per-agent command in process-compose.yaml:
openshell sandbox create --policy agents/{name}/policy.yaml -- claude --profile agents/{name}

# Policy hot-reload (network/inference sections):
openshell policy set --sandbox {id} --policy agents/{name}/policy.yaml
```

OpenShell is invoked purely as a CLI wrapper. No Rust SDK exists -- it is itself a Rust binary but exposes no library API. Design for resilience: OpenShell is alpha software, so wrap all invocations with timeout + error recovery.

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| YAML parsing | serde-saphyr | serde_yml | Less rigorous testing, simple fork of deprecated crate |
| YAML parsing | serde-saphyr | serde_yaml | Deprecated and archived since March 2024 |
| Template engine | minijinja | tera | Heavier deps, diverges from Jinja2, overkill for YAML generation |
| Template engine | minijinja | string formatting | Error-prone for nested YAML structures |
| HTTP client | reqwest | ureq | Sync-only, conflicts with async architecture |
| Error handling | miette + thiserror | color-eyre | color-eyre archived, miette better for config-error-heavy CLI |
| Error handling | miette + thiserror | anyhow | Too generic, no structured error types |
| Signal handling | ctrlc | signal-hook | Overpowered for SIGINT/SIGTERM only |
| CLI testing | assert_cmd | trycmd | Too rigid for early-stage development |
| Async runtime | tokio | smol/async-std | Ecosystem is tokio. reqwest needs tokio. No contest. |

## Cargo.toml Dependencies

```toml
[package]
name = "rightclaw"
version = "0.1.0"
edition = "2024"

[dependencies]
clap = { version = "4.6", features = ["derive"] }
tokio = { version = "1.50", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde-saphyr = "0.0.22"
minijinja = "2.18"
reqwest = { version = "0.13", features = ["json"] }
thiserror = "2.0"
miette = { version = "7.6", features = ["fancy"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
ctrlc = { version = "3.5", features = ["termination"] }
dirs = "6"
which = "7"
walkdir = "2"

[dev-dependencies]
assert_cmd = "2.2"
predicates = "3.1"
tempfile = "3"
```

**Note:** Pin exact versions in `Cargo.lock` (automatic). The ranges above allow compatible updates within semver. For `serde-saphyr` specifically, consider pinning `=0.0.22` since pre-1.0 crates may break on minor bumps.

## Sources

- [clap on crates.io](https://crates.io/crates/clap) - v4.6.0, verified 2026-03-21
- [reqwest on crates.io](https://crates.io/crates/reqwest) - v0.13.2, verified 2026-03-21
- [serde-saphyr on crates.io](https://crates.io/crates/serde-saphyr) - v0.0.22, verified 2026-03-21
- [minijinja on crates.io](https://crates.io/crates/minijinja) - v2.18.0, verified 2026-03-21
- [miette on crates.io](https://crates.io/crates/miette) - v7.6.0, verified 2026-03-21
- [thiserror on crates.io](https://crates.io/crates/thiserror) - v2.0.18, verified 2026-03-21
- [tokio on crates.io](https://crates.io/crates/tokio) - v1.50.0, verified 2026-03-21
- [ctrlc on crates.io](https://crates.io/crates/ctrlc) - v3.5.2, verified 2026-03-21
- [process-compose GitHub releases](https://github.com/F1bonacc1/process-compose/releases) - v1.100.0
- [process-compose remote client docs](https://f1bonacc1.github.io/process-compose/client/)
- [NVIDIA OpenShell GitHub](https://github.com/NVIDIA/OpenShell)
- [NVIDIA OpenShell Developer Guide](https://docs.nvidia.com/openshell/latest/index.html)
- [serde_yaml deprecation discussion](https://users.rust-lang.org/t/serde-yaml-deprecation-alternatives/108868)
- [Rust CLI testing with assert_cmd](https://alexwlchan.net/2025/testing-rust-cli-apps-with-assert-cmd/)
- [Async Rust: When to Use It](https://www.wyeworks.com/blog/2025/02/25/async-rust-when-to-use-it-when-to-avoid-it/)
