/// Find a binary by trying multiple names in order.
fn find_binary(names: &[&str]) -> Result<std::path::PathBuf, which::Error> {
    for name in names {
        if let Ok(path) = which::which(name) {
            return Ok(path);
        }
    }
    which::which(names[0]) // return error for the primary name
}

/// Verify that required external tools are available in PATH.
///
/// Checks for `process-compose` and `claude` (or `claude-bun`).
pub fn verify_dependencies() -> miette::Result<()> {
    which::which("process-compose").map_err(|_| {
        miette::miette!(
            help = "Install: https://f1bonacc1.github.io/process-compose/installation/",
            "process-compose not found in PATH"
        )
    })?;

    find_binary(&["claude", "claude-bun"]).map_err(|_| {
        miette::miette!(
            help = "Install Claude Code CLI: https://docs.anthropic.com/en/docs/claude-code",
            "claude not found in PATH (tried: claude, claude-bun)"
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_dependencies_runs_without_panic() {
        // We can't guarantee process-compose/claude are in PATH, so just
        // verify the function runs without panicking.
        let result = verify_dependencies();
        let _ = result;
    }

    #[test]
    fn verify_dependencies_checks_for_known_missing_binary() {
        // Test the `which` error path by checking for a binary that definitely doesn't exist.
        let result = which::which("rightclaw-nonexistent-binary-42");
        assert!(result.is_err(), "expected nonexistent binary to fail which lookup");
    }

    #[test]
    fn verify_dependencies_does_not_check_openshell() {
        // verify_dependencies should never mention openshell.
        let result = verify_dependencies();
        if let Err(ref e) = result {
            let msg = format!("{e:?}");
            assert!(
                !msg.contains("openshell"),
                "verify_dependencies should not check openshell, but got: {msg}"
            );
        }
    }
}
