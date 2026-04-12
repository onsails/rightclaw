# MCP Instructions via Internal API + AGENTS.md Fix

## Problem

Two bugs discovered when agent (wbsbrain/Sonnet) was asked to add Composio MCP:

1. **MCP_INSTRUCTIONS.md never reaches the system prompt.** We added the file to `CONTENT_MD_FILES` and `@./MCP_INSTRUCTIONS.md` in the agent definition (`right.md`), but the bot doesn't use `--agent` — it assembles the system prompt manually via `build_sandbox_prompt_assembly_script` / `assemble_host_system_prompt`. Neither function includes MCP_INSTRUCTIONS.md. So upstream MCP server instructions are silently lost.

2. **Agent ignores `/mcp add` instructions.** AGENTS.md describes the commands but doesn't tell the agent *what to do* when the user asks to connect an MCP server. Sonnet read "agents cannot add servers" and tried to help by asking for API keys directly, instead of directing the user to use `/mcp add` and `/mcp auth`.

## Decision

### MCP Instructions delivery

Replace the file-based `MCP_INSTRUCTIONS.md` with a new `POST /mcp-instructions` endpoint on the internal API (Unix socket). Bot fetches instructions at prompt assembly time and inlines them into the composite system prompt.

Why not keep the file:
- The file is a cross-process sync mechanism, but the internal API already solves this
- File requires sync to `.claude/agents/`, sandbox upload, create-if-missing in pipeline — all unnecessary complexity
- File was never actually read by the prompt assembly (the bug)
- The `@` reference in agent def is dead code (bot uses `--system-prompt-file`, not `--agent`)

### AGENTS.md MCP section

Rewrite as action-oriented instructions with explicit behavioral rules.

## MCP Instructions Endpoint

### Server side (`internal_api.rs`)

New endpoint `POST /mcp-instructions`:

```rust
#[derive(Deserialize)]
struct McpInstructionsRequest {
    agent: String,
}

#[derive(Serialize)]
struct McpInstructionsResponse {
    instructions: String,  // markdown, may be just the header if no servers have instructions
}
```

Handler reads from SQLite via `db_list_servers()` → `generate_mcp_instructions_md()`. SQLite is the right source — it persists across aggregator reconnects, and ProxyBackend writes instructions there on every `connect()`.

### Client side (`internal_client.rs`)

New method:

```rust
pub async fn mcp_instructions(&self, agent: &str) -> Result<String, InternalClientError>
```

Returns the `instructions` field from the response.

### Bot integration (`worker.rs`)

In `invoke_cc`, before prompt assembly:

1. Call `ctx.internal_client.mcp_instructions(&ctx.agent_name).await`
2. If successful and non-empty (more than just header), pass the content to both `build_sandbox_prompt_assembly_script` and `assemble_host_system_prompt`
3. Both functions get a new parameter `mcp_instructions: Option<&str>` — appended as `## MCP Server Instructions` section
4. On error: log warning, proceed without instructions (non-fatal — don't block message processing)

The non-fatal handling is justified: MCP instructions are supplementary context. If the aggregator is temporarily unreachable (restart, race condition at startup), the agent should still function. This is a read-only informational fetch, not a mutation.

### Cleanup

Remove:
- `MCP_INSTRUCTIONS.md` from `CONTENT_MD_FILES` in `agent_def.rs`
- `@./MCP_INSTRUCTIONS.md` from `generate_agent_definition()` in `agent_def.rs`
- Create-if-missing block for `MCP_INSTRUCTIONS.md` in `pipeline.rs`
- `regenerate_mcp_instructions()` in `aggregator.rs`
- `sync_mcp_instructions()` in `handler.rs`
- Calls to `sync_mcp_instructions()` after `/mcp add` and `/mcp remove` in `handler.rs`
- Tests: `mcp_instructions_md_created_if_missing`, `mcp_instructions_md_not_overwritten_if_exists`, `mcp_instructions_in_content_md_files`, `agent_def_includes_mcp_instructions_ref` in `pipeline.rs`
- Test assertion for MCP_INSTRUCTIONS ordering in `agent_def_tests.rs`

Keep:
- `generate_mcp_instructions_md()` in `mcp_instructions.rs` — reused by the new endpoint
- `db_update_instructions()` — ProxyBackend still writes to SQLite on connect
- `instructions` column in `mcp_servers` table — persistence layer

### WorkerContext change

`WorkerContext` needs access to `InternalClient`. Currently `InternalClient` is wrapped as `InternalApi(Arc<InternalClient>)` and injected into the handler via dptree DI (`dispatch.rs:99`). It's available where `WorkerContext` is constructed (`handler.rs:180`). Add a field:

```rust
pub internal_client: Arc<rightclaw::mcp::internal_client::InternalClient>,
```

Set it from `internal_api.0.clone()` in the handler. This is the same pattern as other Arc fields in WorkerContext.

## AGENTS.md MCP Section

Replace current section:

```markdown
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
```

Key changes:
- "You CANNOT" — imperative, not descriptive
- Step-by-step action plan for "user wants to connect MCP"
- "NEVER ask for API keys" — blocks the exact failure mode observed
- OAuth mentioned explicitly as authentication mechanism
- Removed "Usage instructions from connected servers are automatically included in your context via MCP_INSTRUCTIONS.md" — no longer a file, instructions appear in system prompt transparently

## Documentation Updates

### ARCHITECTURE.md

- Remove `MCP_INSTRUCTIONS.md` from module map (`codegen/mcp_instructions.rs` stays — still used)
- Remove from Configuration Hierarchy table
- Remove from Directory Layout
- Update Data Flow: "ProxyBackend writes instructions to SQLite → bot fetches via internal API at prompt assembly time"
- Add `/mcp-instructions` to Internal REST API description

### PROMPT_SYSTEM.md

- Update to reflect that MCP instructions are fetched from internal API, not read from file

### CLAUDE.md

No changes needed — `with_instructions()` convention doesn't apply here.

## Files Changed

| File | Change |
|------|--------|
| `crates/rightclaw-cli/src/internal_api.rs` | Add `POST /mcp-instructions` endpoint |
| `crates/rightclaw/src/mcp/internal_client.rs` | Add `mcp_instructions()` method |
| `crates/bot/src/telegram/worker.rs` | Fetch instructions in `invoke_cc`, pass to prompt assembly functions |
| `crates/bot/src/telegram/handler.rs` | Remove `sync_mcp_instructions()` and its calls |
| `crates/rightclaw-cli/src/aggregator.rs` | Remove `regenerate_mcp_instructions()` |
| `crates/rightclaw/src/codegen/agent_def.rs` | Remove `MCP_INSTRUCTIONS.md` from `CONTENT_MD_FILES` and agent def |
| `crates/rightclaw/src/codegen/pipeline.rs` | Remove create-if-missing block and related tests |
| `crates/rightclaw/src/codegen/agent_def_tests.rs` | Remove MCP_INSTRUCTIONS ordering assertion |
| `templates/right/AGENTS.md` | Rewrite MCP Management section |
| `ARCHITECTURE.md` | Update MCP instructions delivery description |

## Testing

### New tests

- `internal_api.rs`: `mcp_instructions_returns_empty_for_no_servers`, `mcp_instructions_unknown_agent_returns_404`
- `worker.rs`: `build_sandbox_prompt_assembly_script` with MCP instructions parameter (extend existing tests)
- `worker.rs`: `assemble_host_system_prompt` with MCP instructions parameter (extend existing tests)

### Modified tests

- `pipeline.rs`: Remove `mcp_instructions_md_created_if_missing`, `mcp_instructions_md_not_overwritten_if_exists`, `mcp_instructions_in_content_md_files`, `agent_def_includes_mcp_instructions_ref`
- `agent_def_tests.rs`: Remove MCP_INSTRUCTIONS ordering check

### Kept tests

- `mcp_instructions.rs`: All 4 tests for `generate_mcp_instructions_md()` stay (function still used)
