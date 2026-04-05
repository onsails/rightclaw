use std::path::PathBuf;

use super::generate_cloudflared_config;

#[test]
fn generates_ingress_with_catch_all() {
    let agents = vec![("agent".to_string(), PathBuf::from("/tmp/agents/agent"))];
    let yaml = generate_cloudflared_config(&agents, "right.example.com", None).unwrap();
    assert!(
        yaml.contains("ingress:"),
        "ingress section missing: {yaml}"
    );
    assert!(
        yaml.contains("service: http_status:404"),
        "catch-all rule missing: {yaml}"
    );
}

#[test]
fn single_agent_uses_tunnel_hostname_directly() {
    let agents = vec![("right".to_string(), PathBuf::from("/tmp/agents/right"))];
    let yaml = generate_cloudflared_config(&agents, "right.example.com", None).unwrap();
    assert!(
        yaml.contains("hostname: right.example.com"),
        "single agent should use tunnel hostname directly: {yaml}"
    );
}

#[test]
fn multi_agent_prefixes_name() {
    let agents = vec![
        ("alice".to_string(), PathBuf::from("/tmp/agents/alice")),
        ("bob".to_string(), PathBuf::from("/tmp/agents/bob")),
    ];
    let yaml = generate_cloudflared_config(&agents, "example.com", None).unwrap();
    assert!(
        yaml.contains("hostname: alice.example.com"),
        "alice prefix missing: {yaml}"
    );
    assert!(
        yaml.contains("hostname: bob.example.com"),
        "bob prefix missing: {yaml}"
    );
}

#[test]
fn credentials_embedded_when_provided() {
    use super::CloudflaredCredentials;
    let agents = vec![("agent".to_string(), PathBuf::from("/tmp/agents/agent"))];
    let creds = CloudflaredCredentials {
        tunnel_uuid: "aaaabbbb-0000-1111-2222-ccccddddeeee".to_string(),
        credentials_file: PathBuf::from(
            "/home/user/.rightclaw/tunnel/aaaabbbb-0000-1111-2222-ccccddddeeee.json",
        ),
    };
    let yaml = generate_cloudflared_config(&agents, "right.example.com", Some(&creds)).unwrap();
    assert!(
        yaml.contains("tunnel: aaaabbbb-0000-1111-2222-ccccddddeeee"),
        "tunnel UUID missing: {yaml}"
    );
    assert!(
        yaml.contains("credentials-file: /home/user/.rightclaw/tunnel/aaaabbbb-0000-1111-2222-ccccddddeeee.json"),
        "credentials-file missing: {yaml}"
    );
}

#[test]
fn no_credentials_section_when_none() {
    let agents = vec![("agent".to_string(), PathBuf::from("/tmp/agents/agent"))];
    let yaml = generate_cloudflared_config(&agents, "right.example.com", None).unwrap();
    assert!(
        !yaml.contains("tunnel: "),
        "tunnel: field must be absent when no credentials: {yaml}"
    );
    assert!(
        !yaml.contains("credentials-file:"),
        "credentials-file must be absent when no credentials: {yaml}"
    );
}
