use crate::agent::AgentDef;

/// Generate system prompt content for an agent.
///
/// If the agent has a `crons/` directory, the prompt instructs Claude
/// to run `/cronsync` on startup to reconcile scheduled tasks.
/// Returns `None` if no system prompt is needed.
pub fn generate_system_prompt(agent: &AgentDef) -> Option<String> {
    let crons_dir = agent.path.join("crons");
    if !crons_dir.is_dir() {
        return None;
    }

    Some(
        "## RightClaw System Instructions\n\n\
         On startup, check if the `crons/` directory exists in your agent directory.\n\
         If it contains `.yaml` files, run `/cronsync` to reconcile scheduled tasks.\n\n\
         This ensures all declared cron jobs are active after agent restart or session expiry.\n"
            .to_owned(),
    )
}

#[cfg(test)]
#[path = "system_prompt_tests.rs"]
mod tests;
