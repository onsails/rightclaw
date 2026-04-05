use std::path::PathBuf;

/// Credentials for a cloudflared Named Tunnel (credentials-file mode).
///
/// Fields match the cloudflared config YAML keys `tunnel` and `credentials-file`.
pub struct CloudflaredCredentials {
    /// TunnelID UUID from the credentials JSON (e.g. "e765cc71-d0c2-42a3-864b-81566f8817fd").
    pub tunnel_uuid: String,
    /// Absolute path to the credentials JSON file.
    pub credentials_file: PathBuf,
}

/// Generate a cloudflared tunnel config YAML.
///
/// When `credentials` is `Some`, the config includes `tunnel:` and
/// `credentials-file:` fields required for Named Tunnel mode.
///
/// `agents` is a list of `(name, path)` pairs used to build ingress rules.
/// Each agent is expected to expose an HTTP service; this function generates
/// a catch-all rule routing to localhost. Callers embed actual service ports
/// via their own process-compose setup — the ingress here is a minimal
/// placeholder that routes the primary hostname.
///
/// The final catch-all `service: http_status:404` is required by cloudflared.
pub fn generate_cloudflared_config(
    agents: &[(String, PathBuf)],
    tunnel_hostname: &str,
    credentials: Option<&CloudflaredCredentials>,
) -> miette::Result<String> {
    let mut out = String::new();

    if let Some(creds) = credentials {
        let uuid = creds.tunnel_uuid.replace('"', "\\\"");
        let creds_path = creds.credentials_file.display().to_string();
        out.push_str(&format!("tunnel: {uuid}\n"));
        out.push_str(&format!("credentials-file: {creds_path}\n"));
        out.push('\n');
    }

    // Ingress rules: one rule per agent, then a mandatory catch-all.
    out.push_str("ingress:\n");
    for (name, _path) in agents {
        let hostname = if agents.len() == 1 {
            // Single agent: use the tunnel hostname directly.
            tunnel_hostname.to_string()
        } else {
            // Multiple agents: use <name>.<tunnel_hostname>.
            format!("{name}.{tunnel_hostname}")
        };
        // Route to Claude Code's default MCP/plugin port or a placeholder.
        out.push_str(&format!(
            "  - hostname: {hostname}\n    service: http://localhost:8080\n"
        ));
    }
    // Required catch-all rule.
    out.push_str("  - service: http_status:404\n");

    Ok(out)
}

#[cfg(test)]
#[path = "cloudflared_tests.rs"]
mod tests;
