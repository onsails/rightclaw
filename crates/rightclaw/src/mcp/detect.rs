use std::path::Path;

use crate::mcp::credentials::{read_oauth_metadata, CredentialError};

/// Auth state for an MCP server's OAuth token.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Present,
    Missing,
    Expired,
}

impl std::fmt::Display for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthState::Present => write!(f, "present"),
            AuthState::Missing => write!(f, "auth required"),
            AuthState::Expired => write!(f, "expired"),
        }
    }
}

/// Auth status for a single MCP OAuth server entry.
pub struct ServerStatus {
    pub name: String,
    pub url: String,
    pub state: AuthState,
}

/// Return auth status for all OAuth-candidate servers in an agent's .mcp.json.
///
/// OAuth candidates = entries with a "url" field (HTTP/SSE transport).
/// Stdio entries (command+args only, e.g. rightmemory) are silently skipped.
/// Returns Ok(vec![]) when .mcp.json does not exist.
///
/// Checks Authorization header in .mcp.json directly (no credentials file).
pub fn mcp_auth_status(
    agent_mcp_path: &Path,
) -> Result<Vec<ServerStatus>, CredentialError> {
    if !agent_mcp_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(agent_mcp_path)?;
    let root: serde_json::Value = serde_json::from_str(&content)?;

    let servers = match root.get("mcpServers").and_then(|v| v.as_object()) {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut results: Vec<ServerStatus> = Vec::new();

    for (name, entry) in servers {
        let url = match entry.get("url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => continue, // stdio server -- skip silently
        };

        // Check for Authorization header
        let has_bearer = entry
            .get("headers")
            .and_then(|v| v.get("Authorization"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.starts_with("Bearer "));

        let state = if !has_bearer {
            AuthState::Missing
        } else {
            // Check expiry via _rightclaw_oauth metadata
            let metadata = read_oauth_metadata(agent_mcp_path, name)?;
            match metadata {
                Some(meta) if meta.expires_at > 0 && meta.expires_at < now_unix => {
                    AuthState::Expired
                }
                _ => AuthState::Present, // No metadata, or expires_at=0, or not expired
            }
        };

        results.push(ServerStatus {
            name: name.clone(),
            url,
            state,
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::credentials::{write_bearer_to_mcp_json, write_oauth_metadata, OAuthMetadata};
    use tempfile::tempdir;

    /// Write .mcp.json with servers. url=Some means HTTP server, url=None means stdio.
    fn write_mcp_json(
        dir: &std::path::Path,
        servers: &[(&str, Option<&str>)],
    ) -> std::path::PathBuf {
        let path = dir.join(".mcp.json");
        let mut map = serde_json::Map::new();
        for (name, url_opt) in servers {
            let mut entry = serde_json::Map::new();
            if let Some(url) = url_opt {
                entry.insert("url".to_string(), serde_json::Value::String(url.to_string()));
            } else {
                entry.insert(
                    "command".to_string(),
                    serde_json::Value::String("some-binary".to_string()),
                );
            }
            map.insert(name.to_string(), serde_json::Value::Object(entry));
        }
        let v = serde_json::json!({ "mcpServers": map });
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        path
    }

    // Test: Authorization header present -> Present
    #[test]
    fn bearer_header_present_is_present() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("notion", Some("https://mcp.notion.com/mcp"))]);
        write_bearer_to_mcp_json(&mcp, "notion", "tok123").unwrap();

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, AuthState::Present);
    }

    // Test: Authorization header + expires_at far future -> Present
    #[test]
    fn bearer_with_far_future_expiry_is_present() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("notion", Some("https://mcp.notion.com/mcp"))]);
        write_bearer_to_mcp_json(&mcp, "notion", "tok").unwrap();
        write_oauth_metadata(
            &mcp,
            "notion",
            &OAuthMetadata {
                refresh_token: Some("rt".to_string()),
                expires_at: 9_999_999_999,
                client_id: None,
                client_secret: None,
            },
        )
        .unwrap();

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result[0].state, AuthState::Present);
    }

    // Test: Authorization header + expires_at = 0 -> Present (non-expiring)
    #[test]
    fn bearer_with_zero_expiry_is_present() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("linear", Some("https://mcp.linear.app/mcp"))]);
        write_bearer_to_mcp_json(&mcp, "linear", "tok").unwrap();
        write_oauth_metadata(
            &mcp,
            "linear",
            &OAuthMetadata {
                refresh_token: None,
                expires_at: 0,
                client_id: None,
                client_secret: None,
            },
        )
        .unwrap();

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result[0].state, AuthState::Present);
    }

    // Test: Authorization header + expires_at in the past -> Expired
    #[test]
    fn bearer_with_past_expiry_is_expired() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("notion", Some("https://mcp.notion.com/mcp"))]);
        write_bearer_to_mcp_json(&mcp, "notion", "tok").unwrap();
        write_oauth_metadata(
            &mcp,
            "notion",
            &OAuthMetadata {
                refresh_token: Some("rt".to_string()),
                expires_at: 1, // far past
                client_id: None,
                client_secret: None,
            },
        )
        .unwrap();

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result[0].state, AuthState::Expired);
    }

    // Test: url but no Authorization header -> Missing
    #[test]
    fn no_auth_header_is_missing() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("notion", Some("https://mcp.notion.com/mcp"))]);

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, AuthState::Missing);
    }

    // Test: stdio server (no url) -> skipped
    #[test]
    fn stdio_server_without_url_is_skipped() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(dir.path(), &[("rightmemory", None)]);

        let result = mcp_auth_status(&mcp).unwrap();
        assert!(result.is_empty());
    }

    // Test: absent .mcp.json -> empty vec
    #[test]
    fn absent_mcp_json_returns_empty_vec() {
        let dir = tempdir().unwrap();
        let mcp = dir.path().join("nonexistent.mcp.json");

        let result = mcp_auth_status(&mcp).unwrap();
        assert!(result.is_empty());
    }

    // Test: results sorted by name
    #[test]
    fn results_are_sorted_by_name() {
        let dir = tempdir().unwrap();
        let mcp = write_mcp_json(
            dir.path(),
            &[
                ("zebra", Some("https://zebra.example.com/mcp")),
                ("apple", Some("https://apple.example.com/mcp")),
                ("mango", Some("https://mango.example.com/mcp")),
            ],
        );

        let result = mcp_auth_status(&mcp).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "apple");
        assert_eq!(result[1].name, "mango");
        assert_eq!(result[2].name, "zebra");
    }

    // Test: signature has single Path parameter (compile-time check)
    #[test]
    fn signature_takes_only_agent_mcp_path() {
        // If mcp_auth_status required two Path args, this wouldn't compile
        let dir = tempdir().unwrap();
        let mcp = dir.path().join("nonexistent.mcp.json");
        let _result: Result<Vec<ServerStatus>, CredentialError> = mcp_auth_status(&mcp);
    }
}
