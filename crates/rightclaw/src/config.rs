use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Resolve RIGHTCLAW_HOME: cli_home > env_home > ~/.rightclaw
pub fn resolve_home(cli_home: Option<&str>, env_home: Option<&str>) -> miette::Result<PathBuf> {
    if let Some(home) = cli_home {
        return Ok(PathBuf::from(home));
    }
    if let Some(home) = env_home {
        return Ok(PathBuf::from(home));
    }
    let home =
        dirs::home_dir().ok_or_else(|| miette::miette!("Could not determine home directory"))?;
    Ok(home.join(".rightclaw"))
}

/// Global RightClaw configuration stored at `~/.rightclaw/config.yaml`.
#[derive(Debug, Clone, Default)]
pub struct GlobalConfig {
    pub tunnel: Option<TunnelConfig>,
}

/// Cloudflare Named Tunnel configuration (per D-03).
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub token: String,
    pub url: String,
}

/// Helper structs for YAML deserialization via serde-saphyr.
#[derive(Debug, Deserialize)]
struct RawGlobalConfig {
    tunnel: Option<RawTunnelConfig>,
}

#[derive(Debug, Deserialize)]
struct RawTunnelConfig {
    token: String,
    url: String,
}

/// Read global config from `<home>/config.yaml`.
///
/// Returns `Ok(GlobalConfig::default())` if the file does not exist.
pub fn read_global_config(home: &Path) -> miette::Result<GlobalConfig> {
    let path = home.join("config.yaml");
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| miette::miette!("read config.yaml: {e:#}"))?;
    let raw: RawGlobalConfig = serde_saphyr::from_str(&content)
        .map_err(|e| miette::miette!("parse config.yaml: {e:#}"))?;
    Ok(GlobalConfig {
        tunnel: raw.tunnel.map(|t| TunnelConfig {
            token: t.token,
            url: t.url,
        }),
    })
}

/// Write global config to `<home>/config.yaml`.
///
/// Note: serde-saphyr is deserialize-only — YAML is written manually.
/// The schema is small and stable so this is not a maintenance burden.
pub fn write_global_config(home: &Path, config: &GlobalConfig) -> miette::Result<()> {
    let path = home.join("config.yaml");
    let mut content = String::new();
    if let Some(ref tunnel) = config.tunnel {
        content.push_str("tunnel:\n");
        // Escape quotes in token/url defensively
        let token = tunnel.token.replace('"', "\\\"");
        let url = tunnel.url.replace('"', "\\\"");
        content.push_str(&format!("  token: \"{token}\"\n"));
        content.push_str(&format!("  url: \"{url}\"\n"));
    }
    std::fs::write(&path, &content)
        .map_err(|e| miette::miette!("write config.yaml: {e:#}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_home_returns_cli_home_when_provided() {
        let result = resolve_home(Some("/custom/path"), Some("/env/path")).unwrap();
        assert_eq!(result, PathBuf::from("/custom/path"));
    }

    #[test]
    fn resolve_home_returns_env_home_when_cli_is_none() {
        let result = resolve_home(None, Some("/env/path")).unwrap();
        assert_eq!(result, PathBuf::from("/env/path"));
    }

    #[test]
    fn resolve_home_returns_default_when_both_none() {
        let result = resolve_home(None, None).unwrap();
        let expected = dirs::home_dir().unwrap().join(".rightclaw");
        assert_eq!(result, expected);
    }

    #[test]
    fn read_global_config_returns_default_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let config = read_global_config(dir.path()).unwrap();
        assert!(config.tunnel.is_none(), "no tunnel config when file absent");
    }

    #[test]
    fn write_then_read_global_config_roundtrips_tunnel() {
        let dir = TempDir::new().unwrap();
        let written = GlobalConfig {
            tunnel: Some(TunnelConfig {
                token: "tok123".to_string(),
                url: "example.com".to_string(),
            }),
        };
        write_global_config(dir.path(), &written).unwrap();
        let read = read_global_config(dir.path()).unwrap();
        let tunnel = read.tunnel.expect("tunnel should be present after write");
        assert_eq!(tunnel.token, "tok123");
        assert_eq!(tunnel.url, "example.com");
    }

    #[test]
    fn write_global_config_creates_valid_yaml() {
        let dir = TempDir::new().unwrap();
        let config = GlobalConfig {
            tunnel: Some(TunnelConfig {
                token: "mytoken".to_string(),
                url: "myurl.example.com".to_string(),
            }),
        };
        write_global_config(dir.path(), &config).unwrap();
        let content = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
        // Should be parseable by serde-saphyr
        let raw: RawGlobalConfig = serde_saphyr::from_str(&content).unwrap();
        let tunnel = raw.tunnel.expect("tunnel should parse from written YAML");
        assert_eq!(tunnel.token, "mytoken");
        assert_eq!(tunnel.url, "myurl.example.com");
    }

    #[test]
    fn read_global_config_parses_yaml_with_tunnel_fields() {
        let dir = TempDir::new().unwrap();
        let yaml = r#"tunnel:
  token: "testtoken"
  url: "test.example.com"
"#;
        std::fs::write(dir.path().join("config.yaml"), yaml).unwrap();
        let config = read_global_config(dir.path()).unwrap();
        let tunnel = config.tunnel.expect("tunnel should be parsed");
        assert_eq!(tunnel.token, "testtoken");
        assert_eq!(tunnel.url, "test.example.com");
    }
}
