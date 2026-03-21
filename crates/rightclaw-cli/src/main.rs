use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rightclaw", version, about = "Multi-agent runtime for Claude Code")]
pub struct Cli {
    /// Path to RightClaw home directory
    #[arg(long, env = "RIGHTCLAW_HOME")]
    pub home: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize RightClaw home directory with default agent
    Init,
    /// List discovered agents and their status
    List,
}

fn main() -> miette::Result<()> {
    miette::set_hook(Box::new(|_| Box::new(miette::MietteHandlerOpts::new().build())))?;

    let cli = Cli::parse();

    let filter = if cli.verbose {
        "rightclaw=debug"
    } else {
        "rightclaw=info"
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    let home = rightclaw::config::resolve_home(
        cli.home.as_deref(),
        std::env::var("RIGHTCLAW_HOME").ok().as_deref(),
    )?;

    match cli.command {
        Commands::Init => {
            rightclaw::init::init_rightclaw_home(&home)?;
            println!("Initialized RightClaw at {}", home.display());
            println!(
                "Default agent 'right' created at {}/agents/right/",
                home.display()
            );
            Ok(())
        }
        Commands::List => {
            let agents_dir = home.join("agents");
            if !agents_dir.exists() {
                println!("No agents directory found. Run `rightclaw init` first.");
                return Ok(());
            }

            let agents = rightclaw::agent::discover_agents(&agents_dir)?;
            if agents.is_empty() {
                println!("No agents found in {}", agents_dir.display());
            } else {
                println!("Discovered {} agent(s):", agents.len());
                for agent in &agents {
                    let config_status = if agent.config.is_some() { "yes" } else { "no" };
                    let mcp_status = if agent.mcp_config_path.is_some() {
                        "yes"
                    } else {
                        "no"
                    };
                    println!(
                        "  {:<20} {}    config: {}    mcp: {}",
                        agent.name,
                        agent.path.display(),
                        config_status,
                        mcp_status,
                    );
                }
            }
            Ok(())
        }
    }
}
