use std::path::PathBuf;

use crate::agent::{AgentConfig, AgentDef, RestartPolicy};
use crate::codegen::generate_system_prompt;

fn make_agent_at(path: PathBuf) -> AgentDef {
    AgentDef {
        name: "testbot".to_owned(),
        path: path.clone(),
        identity_path: path.join("IDENTITY.md"),
        policy_path: path.join("policy.yaml"),
        config: Some(AgentConfig {
            restart: RestartPolicy::OnFailure,
            max_restarts: 3,
            backoff_seconds: 5,
            start_prompt: None,
        }),
        mcp_config_path: None,
        soul_path: None,
        user_path: None,
        memory_path: None,
        agents_path: None,
        tools_path: None,
        bootstrap_path: None,
        heartbeat_path: None,
    }
}

#[test]
fn returns_some_when_crons_dir_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().to_path_buf();
    std::fs::create_dir(agent_dir.join("crons")).unwrap();

    let agent = make_agent_at(agent_dir);
    let result = generate_system_prompt(&agent);

    assert!(result.is_some(), "expected Some when crons/ exists");
}

#[test]
fn returns_none_when_no_crons_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().to_path_buf();

    let agent = make_agent_at(agent_dir);
    let result = generate_system_prompt(&agent);

    assert!(result.is_none(), "expected None when no crons/ dir");
}

#[test]
fn returns_none_when_crons_is_file_not_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().to_path_buf();
    std::fs::write(agent_dir.join("crons"), "not a directory").unwrap();

    let agent = make_agent_at(agent_dir);
    let result = generate_system_prompt(&agent);

    assert!(
        result.is_none(),
        "expected None when crons is a file, not a directory"
    );
}

#[test]
fn content_contains_rightclaw_system_instructions() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().to_path_buf();
    std::fs::create_dir(agent_dir.join("crons")).unwrap();

    let agent = make_agent_at(agent_dir);
    let content = generate_system_prompt(&agent).unwrap();

    assert!(
        content.contains("RightClaw System Instructions"),
        "expected 'RightClaw System Instructions' in:\n{content}"
    );
}

#[test]
fn content_contains_cronsync_command() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_dir = tmp.path().to_path_buf();
    std::fs::create_dir(agent_dir.join("crons")).unwrap();

    let agent = make_agent_at(agent_dir);
    let content = generate_system_prompt(&agent).unwrap();

    assert!(
        content.contains("/cronsync"),
        "expected '/cronsync' in:\n{content}"
    );
}
