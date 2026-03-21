use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn rightclaw() -> Command {
    Command::cargo_bin("rightclaw").unwrap()
}

#[test]
fn test_help_output() {
    rightclaw()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Multi-agent runtime"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("list"));
}

#[test]
fn test_init_creates_structure() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_str().unwrap();

    rightclaw()
        .args(["--home", home, "init"])
        .assert()
        .success();

    assert!(dir.path().join("agents/right/IDENTITY.md").exists());
    assert!(dir.path().join("agents/right/policy.yaml").exists());
    assert!(dir.path().join("agents/right/SOUL.md").exists());
    assert!(dir.path().join("agents/right/AGENTS.md").exists());
}

#[test]
fn test_init_twice_fails() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_str().unwrap();

    rightclaw()
        .args(["--home", home, "init"])
        .assert()
        .success();

    rightclaw()
        .args(["--home", home, "init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

#[test]
fn test_list_after_init() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_str().unwrap();

    rightclaw()
        .args(["--home", home, "init"])
        .assert()
        .success();

    rightclaw()
        .args(["--home", home, "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("right"))
        .stdout(predicate::str::contains("1 agent"));
}

#[test]
fn test_list_empty() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_str().unwrap();
    fs::create_dir(dir.path().join("agents")).unwrap();

    rightclaw()
        .args(["--home", home, "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No agents found"));
}

#[test]
fn test_list_no_agents_dir() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_str().unwrap();

    rightclaw()
        .args(["--home", home, "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rightclaw init"));
}
