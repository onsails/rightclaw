//! Integration tests for `claude upgrade` inside an OpenShell sandbox.
//!
//! These tests require a running OpenShell gateway and an existing sandbox
//! named `rightclaw-rightclaw-test-lifecycle` with `storage.googleapis.com`
//! in its network policy.
//!
//! Run with: cargo test -p rightclaw-bot --test sandbox_upgrade

use std::process::Command;

/// Helper: run a command inside the test sandbox via `openshell sandbox exec`.
fn sandbox_exec(args: &[&str]) -> std::process::Output {
    Command::new("openshell")
        .args(["sandbox", "exec", "-n", "rightclaw-rightclaw-test-lifecycle", "--"])
        .args(args)
        .output()
        .expect("openshell binary must be in PATH")
}

/// `claude upgrade` completes without error.
#[test]
fn claude_upgrade_succeeds() {
    let output = sandbox_exec(&["claude", "upgrade"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "claude upgrade failed:\nstdout: {stdout}\nstderr: {stderr}"
    );
    // Must contain either "Successfully updated" or version info
    assert!(
        stdout.contains("Successfully updated") || stdout.contains("Current version"),
        "unexpected output:\n{stdout}"
    );
}

/// After upgrade, the binary exists in `.local/bin`.
#[test]
fn upgraded_binary_exists_in_local_bin() {
    let output = sandbox_exec(&["test", "-L", "/sandbox/.local/bin/claude"]);
    assert!(
        output.status.success(),
        "/sandbox/.local/bin/claude symlink does not exist — run claude_upgrade_succeeds first"
    );
}

/// The upgraded binary reports a valid version.
#[test]
fn upgraded_binary_reports_version() {
    let output = sandbox_exec(&["/sandbox/.local/bin/claude", "--version"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "version check failed");
    assert!(
        stdout.contains("Claude Code"),
        "expected 'Claude Code' in version output, got: {stdout}"
    );
}

/// With `.local/bin` prepended to PATH, `which claude` resolves to the upgraded path.
#[test]
fn path_precedence_favors_local_bin() {
    let output = Command::new("openshell")
        .args(["sandbox", "exec", "-n", "rightclaw-rightclaw-test-lifecycle", "--"])
        .args(["bash", "-c", "PATH=/sandbox/.local/bin:$PATH which claude"])
        .output()
        .expect("openshell binary must be in PATH");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert!(
        output.status.success(),
        "which claude failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout, "/sandbox/.local/bin/claude",
        "expected PATH to resolve to .local/bin/claude, got: {stdout}"
    );
}
