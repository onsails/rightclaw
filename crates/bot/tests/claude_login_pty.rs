//! Integration test: drive `claude` interactive login flow through a PTY.
//!
//! Requires `claude` (or `claude-bun`) in PATH. Uses a temporary HOME
//! to avoid polluting host CC config. Run with:
//!
//!   cargo test -p rightclaw-bot --test claude_login_pty -- --ignored --nocapture
//!
//! This test is `#[ignore]` because it spawns a real CC process and
//! requires network access for the OAuth URL to be generated.

use std::path::PathBuf;

use expectrl::{Expect, Session};

/// Find the claude binary — `claude` or `claude-bun`.
fn find_claude() -> PathBuf {
    which::which("claude")
        .or_else(|_| which::which("claude-bun"))
        .expect("claude binary not found in PATH")
}

/// Set up a temporary HOME with minimal CC config to bypass prompts.
fn setup_temp_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("failed to create temp HOME");

    // .claude/settings.json — skip dangerous mode prompt
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({
            "skipDangerousModePermissionPrompt": true,
            "autoMemoryEnabled": false,
        })
        .to_string(),
    )
    .unwrap();

    // .claude.json — trust the temp home as workspace, skip onboarding
    let home_path = home.path().to_string_lossy().to_string();
    std::fs::write(
        home.path().join(".claude.json"),
        serde_json::json!({
            "hasCompletedOnboarding": true,
            "projects": {
                home_path: {
                    "hasTrustDialogAccepted": true
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    home
}

#[test]
#[ignore]
fn claude_login_shows_oauth_url() {
    let home = setup_temp_home();
    let claude_bin = find_claude();

    println!("Using claude binary: {}", claude_bin.display());
    println!("Temp HOME: {}", home.path().display());

    // Spawn claude with PTY — pass /login as positional arg after --
    let mut cmd = std::process::Command::new(&claude_bin);
    cmd.args(["--dangerously-skip-permissions", "--", "/login"]);
    cmd.env("HOME", home.path());
    cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    cmd.current_dir(home.path());

    let mut session = Session::spawn(cmd).expect("failed to spawn claude with PTY");
    session.set_expect_timeout(Some(std::time::Duration::from_secs(30)));

    // Wait for login method menu
    println!("Waiting for login method menu...");
    let found = session.expect(expectrl::Regex("(Select login method|Claude account|subscription)"))
        .expect("login method menu did not appear within 30s");
    println!("Got menu: {:?}", String::from_utf8_lossy(found.as_bytes()));

    // Press Enter (select option 1 — Claude subscription)
    // CC TUI uses \r for Enter, not \n
    println!("Pressing Enter for Claude subscription...");
    session.send("\r").expect("failed to send Enter");

    // Wait for OAuth URL
    println!("Waiting for OAuth URL...");
    let found = session.expect(expectrl::Regex("https://[^ \\r\\n]+oauth[^ \\r\\n]+"))
        .expect("OAuth URL did not appear within 30s");
    let url = String::from_utf8_lossy(found.as_bytes());
    println!("OAuth URL: {}", url.trim());

    // Verify it's a valid auth URL
    assert!(
        url.contains("oauth") || url.contains("authorize"),
        "Expected OAuth URL, got: {url}"
    );
    assert!(
        url.contains("claude") || url.contains("anthropic"),
        "Expected Claude/Anthropic domain in URL, got: {url}"
    );

    println!("SUCCESS: OAuth URL extracted from interactive claude login flow");

    // Clean up — send /exit or just drop (kill_on_drop)
    let _ = session.send_line("/exit");
    std::thread::sleep(std::time::Duration::from_secs(1));
}

#[test]
#[ignore]
fn claude_auth_login_shows_url() {
    let home = setup_temp_home();
    let claude_bin = find_claude();

    println!("Using claude binary: {}", claude_bin.display());
    println!("Temp HOME: {}", home.path().display());

    // Spawn claude auth login directly
    let mut cmd = std::process::Command::new(&claude_bin);
    cmd.args(["auth", "login", "--claudeai"]);
    cmd.env("HOME", home.path());
    cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    cmd.current_dir(home.path());

    let mut session = Session::spawn(cmd).expect("failed to spawn claude auth login with PTY");
    session.set_expect_timeout(Some(std::time::Duration::from_secs(30)));

    // Wait for OAuth URL directly (auth login skips the interactive prompt)
    println!("Waiting for OAuth URL from claude auth login...");
    let found = session.expect(expectrl::Regex("https://[^ \\r\\n]+"))
        .expect("URL did not appear within 30s");
    let url = String::from_utf8_lossy(found.as_bytes());
    println!("URL: {}", url.trim());

    assert!(
        url.contains("claude") || url.contains("anthropic"),
        "Expected Claude/Anthropic domain in URL, got: {url}"
    );

    println!("SUCCESS: URL extracted from claude auth login");
}
