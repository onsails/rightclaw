//! Generate OpenShell policy.yaml from agent configuration.

/// Generate an OpenShell policy YAML string.
///
/// `right_mcp_port`: TCP port for the host-side right MCP HTTP server.
/// `external_mcp_servers`: (name, domain) pairs for external MCP servers the agent needs.
pub fn generate_policy(right_mcp_port: u16, external_mcp_servers: &[(&str, &str)]) -> String {
    let mut policy = format!(
        r#"version: 1

filesystem_policy:
  include_workdir: true
  read_only:
    - /usr
    - /lib
    - /lib64
    - /etc
    - /proc
    - /dev/urandom
    - /dev/null
  read_write:
    - /tmp
    - /sandbox

landlock:
  compatibility: best_effort

process:
  run_as_user: sandbox
  run_as_group: sandbox

network_policies:
  anthropic:
    endpoints:
      - host: "*.anthropic.com"
        port: 443
        protocol: rest
        access: full
        tls: terminate
      - host: "anthropic.com"
        port: 443
        protocol: rest
        access: full
        tls: terminate
      - host: "*.claude.com"
        port: 443
        protocol: rest
        access: full
        tls: terminate
      - host: "claude.com"
        port: 443
        protocol: rest
        access: full
        tls: terminate
      - host: "*.claude.ai"
        port: 443
        protocol: rest
        access: full
        tls: terminate
    binaries:
      - path: "**"

  right:
    endpoints:
      - host: "host.docker.internal"
        port: {right_mcp_port}
        allowed_ips:
          - "172.16.0.0/12"
        protocol: rest
        access: full
    binaries:
      - path: "**"
"#
    );

    for (name, domain) in external_mcp_servers {
        policy.push_str(&format!(
            r#"
  {name}:
    endpoints:
      - host: "{domain}"
        port: 443
        protocol: rest
        access: full
        tls: terminate
    binaries:
      - path: "**"
"#
        ));
    }

    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_policy_with_right_mcp_port() {
        let policy = generate_policy(8100, &[]);
        assert!(policy.contains("host.docker.internal"));
        assert!(policy.contains("8100"));
        assert!(policy.contains("172.16.0.0/12"));
        assert!(policy.contains("right:"));
        assert!(policy.contains("*.anthropic.com"));
        assert!(policy.contains("*.claude.com"));
        assert!(policy.contains("best_effort"));
        assert!(policy.contains("version: 1"));
    }

    #[test]
    fn adds_external_mcp_domains() {
        let policy = generate_policy(
            8100,
            &[("notion", "mcp.notion.com"), ("linear", "mcp.linear.app")],
        );
        assert!(policy.contains("mcp.notion.com"));
        assert!(policy.contains("mcp.linear.app"));
        assert!(policy.contains("notion:"));
        assert!(policy.contains("linear:"));
    }

    #[test]
    fn base_policy_has_no_extra_domains() {
        let policy = generate_policy(9000, &[]);
        assert!(!policy.contains("mcp.notion.com"));
        assert!(policy.contains("9000"));
    }
}
