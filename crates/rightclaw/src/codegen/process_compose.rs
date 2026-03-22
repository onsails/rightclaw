use std::path::Path;

use minijinja::{Environment, context};
use serde::Serialize;

use crate::agent::{AgentDef, RestartPolicy};

const PC_TEMPLATE: &str = include_str!("../../../../templates/process-compose.yaml.j2");

/// Template context for a single agent process entry.
#[derive(Debug, Serialize)]
struct ProcessAgent {
    name: String,
    wrapper_path: String,
    working_dir: String,
    restart_policy: String,
    backoff_seconds: u32,
    max_restarts: u32,
}

/// Map `RestartPolicy` to process-compose's expected string values.
fn restart_policy_str(policy: &RestartPolicy) -> &'static str {
    match policy {
        RestartPolicy::OnFailure => "on_failure",
        RestartPolicy::Always => "always",
        RestartPolicy::Never => "no",
    }
}

/// Generate a `process-compose.yaml` configuration for the given agents.
///
/// Shell wrapper paths are derived as `run_dir/<agent-name>.sh`.
/// Working directory is set to each agent's directory path (per D-05).
pub fn generate_process_compose(agents: &[AgentDef], run_dir: &Path) -> miette::Result<String> {
    let process_agents: Vec<ProcessAgent> = agents
        .iter()
        .map(|agent| {
            let (restart, backoff, max) = match &agent.config {
                Some(cfg) => (
                    restart_policy_str(&cfg.restart),
                    cfg.backoff_seconds,
                    cfg.max_restarts,
                ),
                None => (restart_policy_str(&RestartPolicy::default()), 5, 3),
            };

            ProcessAgent {
                name: agent.name.clone(),
                wrapper_path: run_dir
                    .join(format!("{}.sh", agent.name))
                    .display()
                    .to_string(),
                working_dir: agent.path.display().to_string(),
                restart_policy: restart.to_owned(),
                backoff_seconds: backoff,
                max_restarts: max,
            }
        })
        .collect();

    let mut env = Environment::new();
    env.add_template("pc", PC_TEMPLATE)
        .map_err(|e| miette::miette!("template parse error: {e:#}"))?;
    let tmpl = env.get_template("pc").expect("template just added");

    tmpl.render(context! { agents => process_agents })
        .map_err(|e| miette::miette!("template render error: {e:#}"))
}

#[cfg(test)]
#[path = "process_compose_tests.rs"]
mod tests;
