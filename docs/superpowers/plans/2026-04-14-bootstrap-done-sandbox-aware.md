# bootstrap_done sandbox-aware file verification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bootstrap_done` check identity files inside the OpenShell sandbox (via gRPC exec) instead of on the host filesystem, so agents get a success signal mid-session.

**Architecture:** Add `mtls_dir: Option<PathBuf>` to `RightBackend`. At aggregator init, parse each agent's `agent.yaml` to determine sandbox mode. If openshell, pass the mTLS dir so `call_bootstrap_done` can exec `test -f` inside the sandbox via gRPC. For `mode: none` agents, keep existing host-side logic.

**Tech Stack:** Rust, OpenShell gRPC (tonic), rmcp

---

### Task 1: Add `mtls_dir` field to `RightBackend`

**Files:**
- Modify: `crates/rightclaw-cli/src/right_backend.rs:27-38`
- Modify: `crates/rightclaw-cli/src/right_backend_tests.rs:10-16`

- [ ] **Step 1: Update `RightBackend` struct and constructor**

In `crates/rightclaw-cli/src/right_backend.rs`, add the field and update `new()`:

```rust
pub struct RightBackend {
    conn_cache: ConnCache,
    agents_dir: PathBuf,
    mtls_dir: Option<PathBuf>,
}

impl RightBackend {
    pub fn new(agents_dir: PathBuf, mtls_dir: Option<PathBuf>) -> Self {
        Self {
            conn_cache: Arc::new(DashMap::new()),
            agents_dir,
            mtls_dir,
        }
    }
```

- [ ] **Step 2: Fix test helper `make_backend`**

In `crates/rightclaw-cli/src/right_backend_tests.rs`, update `make_backend` to pass `None` for `mtls_dir` (host-side tests):

```rust
fn make_backend() -> (RightBackend, PathBuf, TempDir) {
    let tmp = TempDir::new().expect("tempdir");
    let agents_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    let backend = RightBackend::new(agents_dir.clone(), None);
    (backend, agents_dir, tmp)
}
```

- [ ] **Step 3: Fix `main.rs` call site**

In `crates/rightclaw-cli/src/main.rs:436`, update `RightBackend::new` to pass `mtls_dir` based on sandbox mode. Replace:

```rust
let right = right_backend::RightBackend::new(agents_dir.clone());
```

With:

```rust
let mtls_dir = match rightclaw::agent::discovery::parse_agent_config(&agent_dir) {
    Ok(Some(config))
        if *config.sandbox_mode() == rightclaw::agent::SandboxMode::Openshell =>
    {
        match rightclaw::openshell::preflight_check() {
            rightclaw::openshell::OpenShellStatus::Ready(dir) => Some(dir),
            _ => None,
        }
    }
    _ => None,
};
let right = right_backend::RightBackend::new(agents_dir.clone(), mtls_dir);
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --workspace`
Expected: compiles clean (no behavior change yet — `mtls_dir` is stored but unused)

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw-cli/src/right_backend.rs crates/rightclaw-cli/src/right_backend_tests.rs crates/rightclaw-cli/src/main.rs
git commit -m "refactor: add mtls_dir field to RightBackend for sandbox-aware bootstrap"
```

---

### Task 2: Implement sandbox-aware `call_bootstrap_done`

**Files:**
- Modify: `crates/rightclaw-cli/src/right_backend.rs:440-466`

- [ ] **Step 1: Make `call_bootstrap_done` async and add sandbox path**

Replace the current `call_bootstrap_done` method (lines 440-466) with:

```rust
async fn call_bootstrap_done(&self, agent_name: &str) -> Result<CallToolResult, anyhow::Error> {
    let agent_dir = self.agents_dir.join(agent_name);
    let required = ["IDENTITY.md", "SOUL.md", "USER.md"];

    let missing: Vec<&str> = if let Some(mtls_dir) = &self.mtls_dir {
        // Sandbox mode: check files inside the sandbox via gRPC exec.
        let sandbox_name = rightclaw::openshell::sandbox_name(agent_name);
        let mut client = rightclaw::openshell::connect_grpc(mtls_dir)
            .await
            .context("bootstrap_done: failed to connect to OpenShell gRPC")?;
        let sandbox_id =
            rightclaw::openshell::resolve_sandbox_id(&mut client, &sandbox_name)
                .await
                .context("bootstrap_done: failed to resolve sandbox ID")?;

        let mut missing = Vec::new();
        for &file in &required {
            let path = format!("/sandbox/{file}");
            let (_, exit_code) =
                rightclaw::openshell::exec_in_sandbox(&mut client, &sandbox_id, &["test", "-f", &path])
                    .await
                    .with_context(|| format!("bootstrap_done: exec test -f {path} failed"))?;
            if exit_code != 0 {
                missing.push(file);
            }
        }
        missing
    } else {
        // Host mode: check files on the host filesystem.
        required
            .iter()
            .filter(|f| !agent_dir.join(f).exists())
            .copied()
            .collect()
    };

    if missing.is_empty() {
        let bootstrap_path = agent_dir.join("BOOTSTRAP.md");
        if bootstrap_path.exists() {
            std::fs::remove_file(&bootstrap_path)
                .context("failed to remove BOOTSTRAP.md")?;
        }
        Ok(CallToolResult::success(vec![Content::text(
            "Bootstrap complete! IDENTITY.md, SOUL.md, and USER.md verified. \
             Your identity files are now active.",
        )]))
    } else {
        Ok(CallToolResult::error(vec![Content::text(format!(
            "Cannot complete bootstrap — missing files: {}. \
             Create them first, then call bootstrap_done again.",
            missing.join(", ")
        ))]))
    }
}
```

- [ ] **Step 2: Update dispatch to `.await` the now-async method**

In `tools_call` (line 142), change:

```rust
"bootstrap_done" => self.call_bootstrap_done(agent_name),
```

to:

```rust
"bootstrap_done" => self.call_bootstrap_done(agent_name).await,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --workspace`
Expected: compiles clean

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p rightclaw-cli -- right_backend`
Expected: all existing tests pass (they use `mtls_dir: None` so hit the host-side path)

- [ ] **Step 5: Commit**

```bash
git add crates/rightclaw-cli/src/right_backend.rs
git commit -m "fix: bootstrap_done checks files in sandbox via gRPC exec for openshell agents"
```

---

### Task 3: Integration tests for sandbox-aware bootstrap_done

**Files:**
- Modify: `crates/rightclaw-cli/src/right_backend_tests.rs`

These tests create an ephemeral sandbox, following the established pattern from `crates/bot/src/sync.rs` tests: cleanup leftover → create fresh → poll exec readiness → test → delete.

- [ ] **Step 1: Write integration test — files present in sandbox**

Add to `crates/rightclaw-cli/src/right_backend_tests.rs`:

```rust
/// Helper: spin up an ephemeral sandbox for testing.
/// Returns (SandboxExec, sandbox_name). Caller must delete sandbox after use.
async fn create_test_sandbox(
    mtls_dir: &std::path::Path,
    sandbox_name: &str,
) -> rightclaw::sandbox_exec::SandboxExec {
    let mut grpc_client = rightclaw::openshell::connect_grpc(mtls_dir)
        .await
        .expect("gRPC connect");

    // Clean up leftover from a previous failed run.
    if rightclaw::openshell::sandbox_exists(&mut grpc_client, sandbox_name)
        .await
        .unwrap()
    {
        rightclaw::openshell::delete_sandbox(sandbox_name).await;
        rightclaw::openshell::wait_for_deleted(&mut grpc_client, sandbox_name, 60, 2)
            .await
            .expect("cleanup of leftover sandbox failed");
    }

    // Create sandbox with minimal policy.
    let policy_dir = tempfile::tempdir().unwrap();
    let policy_path = policy_dir.path().join("policy.yaml");
    std::fs::write(
        &policy_path,
        "\
version: 1
filesystem_policy:
  include_workdir: true
  read_write:
    - /tmp
    - /sandbox
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  outbound:
    endpoints:
      - host: \"**.*\"
        port: 443
        protocol: rest
        access: full
        tls: terminate
    binaries:
      - path: \"**\"
",
    )
    .unwrap();

    let _child = rightclaw::openshell::spawn_sandbox(sandbox_name, &policy_path, None)
        .expect("failed to spawn sandbox");
    rightclaw::openshell::wait_for_ready(&mut grpc_client, sandbox_name, 120, 2)
        .await
        .expect("sandbox did not become READY");

    let sandbox_id =
        rightclaw::openshell::resolve_sandbox_id(&mut grpc_client, sandbox_name)
            .await
            .expect("resolve sandbox_id");

    let sbox = rightclaw::sandbox_exec::SandboxExec::new(
        mtls_dir.to_path_buf(),
        sandbox_name.to_owned(),
        sandbox_id,
    );

    // Poll exec until ready — OpenShell reports READY before exec transport is available.
    for attempt in 1..=20 {
        match sbox.exec(&["echo", "ready"]).await {
            Ok((out, 0)) if out.trim() == "ready" => break,
            _ if attempt == 20 => panic!("exec not ready after 20 attempts"),
            _ => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
        }
    }

    sbox
}

#[tokio::test]
async fn bootstrap_done_sandbox_files_present() {
    let sandbox_name = "rightclaw-test-bootstrap-present";

    let mtls_dir = match rightclaw::openshell::preflight_check() {
        rightclaw::openshell::OpenShellStatus::Ready(dir) => dir,
        other => panic!("OpenShell not ready: {other:?}"),
    };

    let sbox = create_test_sandbox(&mtls_dir, sandbox_name).await;

    // Create identity files inside sandbox.
    for name in ["IDENTITY.md", "SOUL.md", "USER.md"] {
        let (_, code) = sbox
            .exec(&["sh", "-c", &format!("echo '# test' > /sandbox/{name}")])
            .await
            .unwrap();
        assert_eq!(code, 0, "failed to create {name} in sandbox");
    }

    // Set up RightBackend with mtls_dir pointing at the sandbox.
    // Agent name must match: sandbox_name = "rightclaw-{agent_name}"
    // so agent_name = "test-bootstrap-present"
    let agent_name = "test-bootstrap-present";
    let tmp = TempDir::new().unwrap();
    let agents_dir = tmp.path().join("agents");
    let agent_dir = agents_dir.join(agent_name);
    std::fs::create_dir_all(&agent_dir).unwrap();
    // Create BOOTSTRAP.md on host to verify it gets removed.
    std::fs::write(agent_dir.join("BOOTSTRAP.md"), "bootstrap").unwrap();
    // Create data.db so tools_call doesn't fail.
    let _conn = rightclaw::memory::open_connection(&agent_dir).unwrap();

    let backend = RightBackend::new(agents_dir, Some(mtls_dir.clone()));
    let result = backend
        .tools_call(agent_name, &agent_dir, "bootstrap_done", json!({}))
        .await
        .expect("bootstrap_done should succeed");

    let text = format!("{:?}", result);
    assert!(
        text.contains("Bootstrap complete"),
        "expected success, got: {text}"
    );
    assert!(
        !agent_dir.join("BOOTSTRAP.md").exists(),
        "BOOTSTRAP.md should be removed from host"
    );

    // Cleanup.
    rightclaw::openshell::delete_sandbox(sandbox_name).await;
}
```

- [ ] **Step 2: Write integration test — files missing in sandbox**

Add to the same file:

```rust
#[tokio::test]
async fn bootstrap_done_sandbox_files_missing() {
    let sandbox_name = "rightclaw-test-bootstrap-missing";

    let mtls_dir = match rightclaw::openshell::preflight_check() {
        rightclaw::openshell::OpenShellStatus::Ready(dir) => dir,
        other => panic!("OpenShell not ready: {other:?}"),
    };

    let sbox = create_test_sandbox(&mtls_dir, sandbox_name).await;

    // Create only IDENTITY.md — SOUL.md and USER.md are missing.
    let (_, code) = sbox
        .exec(&["sh", "-c", "echo '# test' > /sandbox/IDENTITY.md"])
        .await
        .unwrap();
    assert_eq!(code, 0);

    let agent_name = "test-bootstrap-missing";
    let tmp = TempDir::new().unwrap();
    let agents_dir = tmp.path().join("agents");
    let agent_dir = agents_dir.join(agent_name);
    std::fs::create_dir_all(&agent_dir).unwrap();
    let _conn = rightclaw::memory::open_connection(&agent_dir).unwrap();

    let backend = RightBackend::new(agents_dir, Some(mtls_dir.clone()));
    let result = backend
        .tools_call(agent_name, &agent_dir, "bootstrap_done", json!({}))
        .await
        .expect("bootstrap_done should return Ok (tool-level error)");

    let text = format!("{:?}", result);
    assert!(
        text.contains("missing files"),
        "expected missing files error, got: {text}"
    );
    assert!(
        text.contains("SOUL.md"),
        "should mention SOUL.md as missing, got: {text}"
    );
    assert!(
        text.contains("USER.md"),
        "should mention USER.md as missing, got: {text}"
    );

    // Cleanup.
    rightclaw::openshell::delete_sandbox(sandbox_name).await;
}
```

- [ ] **Step 3: Run all right_backend tests**

Run: `cargo test -p rightclaw-cli -- right_backend`
Expected: all tests pass — both unit (host-side) and integration (sandbox-side)

- [ ] **Step 4: Commit**

```bash
git add crates/rightclaw-cli/src/right_backend_tests.rs
git commit -m "test: integration tests for sandbox-aware bootstrap_done"
```

---

### Task 4: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: clean build

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 3: Rebuild, restart MCP server, verify in production sandbox**

Rebuild the binary, restart the `right-mcp-server` process via process-compose, then trigger a bootstrap in the production sandbox to confirm `bootstrap_done` succeeds mid-session.

```bash
cargo build --workspace
curl -s -X POST "http://localhost:18927/process/restart/right-mcp-server"
```

Then verify by sending a bootstrap message to the agent and checking logs for `bootstrap_done` success.
