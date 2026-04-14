use crate::codegen::{generate_system_prompt, BOOTSTRAP_SCHEMA_JSON, REPLY_SCHEMA_JSON};

#[test]
fn reply_schema_json_is_valid() {
    let parsed: serde_json::Value = serde_json::from_str(REPLY_SCHEMA_JSON)
        .expect("REPLY_SCHEMA_JSON must be valid JSON");
    assert!(parsed.get("required").is_some());
}

#[test]
fn bootstrap_schema_json_is_valid() {
    let parsed: serde_json::Value = serde_json::from_str(BOOTSTRAP_SCHEMA_JSON)
        .expect("BOOTSTRAP_SCHEMA_JSON must be valid JSON");
    let required = parsed.get("required").unwrap().as_array().unwrap();
    let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(required_strs.contains(&"content"), "must require content");
    assert!(required_strs.contains(&"bootstrap_complete"), "must require bootstrap_complete");
}

#[test]
fn bootstrap_schema_has_bootstrap_complete_field() {
    let parsed: serde_json::Value = serde_json::from_str(BOOTSTRAP_SCHEMA_JSON).unwrap();
    let props = parsed.get("properties").unwrap();
    assert!(props.get("bootstrap_complete").is_some(), "must have bootstrap_complete property");
}

#[test]
fn system_prompt_contains_agent_name() {
    let result = generate_system_prompt("mybot", &crate::agent::types::SandboxMode::Openshell, "/sandbox");
    assert!(result.contains("mybot"));
}

#[test]
fn system_prompt_contains_rightclaw_description() {
    let result = generate_system_prompt("test", &crate::agent::types::SandboxMode::Openshell, "/sandbox");
    assert!(result.contains("RightClaw"));
    assert!(result.contains("multi-agent runtime"));
}

#[test]
fn system_prompt_contains_sandbox_mode() {
    let openshell = generate_system_prompt("test", &crate::agent::types::SandboxMode::Openshell, "/sandbox");
    assert!(openshell.contains("OpenShell"));

    let none = generate_system_prompt("test", &crate::agent::types::SandboxMode::None, "/test/agent/home");
    assert!(none.contains("no sandbox"));
}

#[test]
fn system_prompt_mentions_right_mcp() {
    let result = generate_system_prompt("test", &crate::agent::types::SandboxMode::Openshell, "/sandbox");
    assert!(result.contains("right"));
    assert!(result.contains("MCP"));
}

#[test]
fn system_prompt_contains_ssh_block_for_openshell() {
    let result = generate_system_prompt("mybot", &crate::agent::types::SandboxMode::Openshell, "/sandbox");
    assert!(result.contains("rightclaw agent ssh mybot"), "openshell prompt must include SSH command");
    assert!(result.contains("interactive terminal"), "openshell prompt must explain when to use SSH");
}

#[test]
fn system_prompt_no_ssh_block_for_no_sandbox() {
    let result = generate_system_prompt("mybot", &crate::agent::types::SandboxMode::None, "/test/agent/home");
    assert!(!result.contains("rightclaw agent ssh"), "no-sandbox prompt must NOT include SSH command");
}

#[test]
fn operating_instructions_constant_is_non_empty() {
    assert!(
        !crate::codegen::OPERATING_INSTRUCTIONS.is_empty(),
        "OPERATING_INSTRUCTIONS must not be empty"
    );
    assert!(
        crate::codegen::OPERATING_INSTRUCTIONS.contains("## Your Files"),
        "OPERATING_INSTRUCTIONS must contain Your Files section"
    );
    assert!(
        crate::codegen::OPERATING_INSTRUCTIONS.contains("## MCP Management"),
        "OPERATING_INSTRUCTIONS must contain MCP Management section"
    );
}

#[test]
fn bootstrap_instructions_constant_is_non_empty() {
    assert!(
        !crate::codegen::BOOTSTRAP_INSTRUCTIONS.is_empty(),
        "BOOTSTRAP_INSTRUCTIONS must not be empty"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("First-Time Setup"),
        "BOOTSTRAP_INSTRUCTIONS must contain bootstrap header"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("### IDENTITY.md"),
        "BOOTSTRAP_INSTRUCTIONS must contain IDENTITY.md structure"
    );
    assert!(
        crate::codegen::BOOTSTRAP_INSTRUCTIONS.contains("### SOUL.md"),
        "BOOTSTRAP_INSTRUCTIONS must contain SOUL.md structure"
    );
}

#[test]
fn system_prompt_contains_home_dir() {
    let result = generate_system_prompt(
        "test",
        &crate::agent::types::SandboxMode::Openshell,
        "/my/custom/home",
    );
    assert!(
        result.contains("/my/custom/home"),
        "system prompt must contain the passed home_dir"
    );
}
