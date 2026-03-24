pub mod claude_json;
pub mod process_compose;
pub mod settings;
pub mod shell_wrapper;
pub mod system_prompt;

pub use claude_json::{create_credential_symlink, generate_agent_claude_json};
pub use process_compose::generate_process_compose;
pub use settings::generate_settings;
pub use shell_wrapper::generate_wrapper;
pub use system_prompt::generate_combined_prompt;
