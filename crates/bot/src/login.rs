//! PTY-driven Claude login flow.
//!
//! Spawns `claude --dangerously-skip-permissions -- /login` via SSH into the sandbox,
//! drives through the login method menu, extracts the OAuth URL, waits for the auth
//! code from Telegram, and pipes it into the PTY session.

use std::path::Path;
use std::time::Duration;

use expectrl::Expect as _;
use tokio::sync::{mpsc, oneshot};

/// Result of a login attempt.
#[derive(Debug)]
pub enum LoginResult {
    /// Login succeeded — credentials stored in sandbox.
    Success,
    /// Login failed or timed out.
    Failed(String),
    /// Could not extract OAuth URL.
    NoUrl,
}

/// Messages sent from the login task to the auth watcher.
#[derive(Debug)]
pub enum LoginEvent {
    /// OAuth URL extracted — send to user.
    Url(String),
    /// CC is asking for the auth code — prompt user.
    WaitingForCode,
    /// Login completed successfully.
    Done,
    /// Login failed.
    Error(String),
}

/// Drive the interactive claude login flow via PTY over SSH.
///
/// Runs in a blocking thread (expectrl is sync). Communicates via channels:
/// - `event_tx`: sends LoginEvents to the async orchestrator
/// - `code_rx`: receives the auth code from the Telegram handler
pub fn run_login_pty(
    ssh_config_path: &Path,
    agent_name: &str,
    event_tx: mpsc::Sender<LoginEvent>,
    code_rx: oneshot::Receiver<String>,
) {
    let ssh_config = ssh_config_path.to_path_buf();
    let ssh_host = rightclaw::openshell::ssh_host(agent_name);

    // Build SSH command to launch claude inside sandbox
    let mut cmd = std::process::Command::new("ssh");
    cmd.arg("-t");
    cmd.arg("-F").arg(&ssh_config);
    cmd.arg(&ssh_host);
    cmd.arg("--");
    cmd.args(["claude", "--dangerously-skip-permissions", "--", "/login"]);

    let mut session = match expectrl::Session::spawn(cmd) {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.blocking_send(LoginEvent::Error(format!("failed to spawn PTY: {e:#}")));
            return;
        }
    };
    // Set wide terminal so URLs don't wrap across lines.
    if let Err(e) = session.get_process_mut().set_window_size(500, 50) {
        tracing::warn!("login: failed to set PTY window size: {e}");
    }
    session.set_expect_timeout(Some(Duration::from_secs(30)));

    // Wait for login method menu
    tracing::info!(agent = agent_name, "login: waiting for login method menu");
    match session.expect(expectrl::Regex("(Select login method|Claude account|subscription)")) {
        Ok(_) => {}
        Err(e) => {
            let _ = event_tx.blocking_send(LoginEvent::Error(format!("login menu did not appear: {e:#}")));
            return;
        }
    }

    // Press Enter to select option 1 (Claude subscription)
    if let Err(e) = session.send("\r") {
        let _ = event_tx.blocking_send(LoginEvent::Error(format!("failed to send Enter: {e:#}")));
        return;
    }
    tracing::info!(agent = agent_name, "login: selected Claude subscription");

    // Wait for "Browser didn't open" text which precedes the URL
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    match session.expect(expectrl::Regex("Browser didn't open")) {
        Ok(_) => {
            // Now read the next line which contains the URL
            session.set_expect_timeout(Some(Duration::from_secs(5)));
            match session.expect(expectrl::Regex("https://[^\\s\\x1b]+")) {
                Ok(found) => {
                    let raw = String::from_utf8_lossy(found.as_bytes());
                    let cleaned = strip_ansi(&raw);
                    // Extract just the https:// URL from the matched text
                    let url = extract_url_from_text(&cleaned);
                    tracing::info!(agent = agent_name, url = %url, "login: OAuth URL extracted");
                    let _ = event_tx.blocking_send(LoginEvent::Url(url));
                }
                Err(e) => {
                    let _ = event_tx.blocking_send(LoginEvent::Error(format!("URL not found after browser prompt: {e:#}")));
                    return;
                }
            }
        }
        Err(e) => {
            let _ = event_tx.blocking_send(LoginEvent::Error(format!("browser prompt did not appear: {e:#}")));
            return;
        }
    }

    // CC shows a prompt asking user to paste the code from browser.
    // Wait for it — look for "Paste" or "paste" specifically (not "code" which
    // appears in the URL we just captured).
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    match session.expect(expectrl::Regex("(?i)(paste|enter the code|enter code|authorization code)")) {
        Ok(found) => {
            let text = strip_ansi(&String::from_utf8_lossy(found.as_bytes()));
            tracing::info!(agent = agent_name, prompt = %text.trim(), "login: CC is asking for auth code");
            let _ = event_tx.blocking_send(LoginEvent::WaitingForCode);
        }
        Err(_) => {
            // No explicit paste prompt — CC might just show a text input.
            // Still ask user for code, but log the situation.
            tracing::info!(agent = agent_name, "login: no explicit paste prompt — asking for code anyway");
            let _ = event_tx.blocking_send(LoginEvent::WaitingForCode);
        }
    }

    // Wait for auth code from Telegram (up to 5 minutes)
    let code = match code_rx.blocking_recv() {
        Ok(c) => c,
        Err(_) => {
            let _ = event_tx.blocking_send(LoginEvent::Error("auth code channel closed (timeout?)".into()));
            return;
        }
    };

    // Small delay to ensure CC's input field is ready
    std::thread::sleep(Duration::from_millis(500));

    // Send auth code to CC (use \r — CC TUI needs carriage return, not newline)
    let code_with_cr = format!("{code}\r");
    tracing::info!(agent = agent_name, code_len = code.len(), "login: sending auth code");
    if let Err(e) = session.send(&code_with_cr) {
        let _ = event_tx.blocking_send(LoginEvent::Error(format!("failed to send auth code: {e:#}")));
        return;
    }

    // Wait for success indication — CC may show various messages after processing the code.
    // Also match "API key" which CC shows when creating the key from the OAuth token.
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    match session.expect(expectrl::Regex("(?i)(logged in|success|authenticated|welcome|API key|signed in)")) {
        Ok(found) => {
            let text = String::from_utf8_lossy(found.as_bytes());
            tracing::info!(agent = agent_name, response = %strip_ansi(&text).chars().take(200).collect::<String>(), "login: authentication successful");
            let _ = event_tx.blocking_send(LoginEvent::Done);
        }
        Err(e) => {
            tracing::warn!(agent = agent_name, "login: no success confirmation within 60s: {e:#}");
            // Check if credentials were saved despite no confirmation
            let _ = event_tx.blocking_send(LoginEvent::Error(
                "Login may have failed — no confirmation from Claude. Try sending a message to check.".into()
            ));
        }
    }

    // Exit claude
    let _ = session.send_line("/exit");
    std::thread::sleep(Duration::from_secs(1));
}

/// Extract the first `https://` URL from text that may contain surrounding content.
pub fn extract_url_from_text(text: &str) -> String {
    if let Some(start) = text.find("https://") {
        let url_part = &text[start..];
        let end = url_part.find(|c: char| c.is_whitespace() || c == '\x1b').unwrap_or(url_part.len());
        url_part[..end].to_string()
    } else {
        text.trim().to_string()
    }
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == '[' {
                // CSI sequence — consume until letter
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            in_escape = false;
            continue;
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_escapes() {
        let input = "\x1b[38;5;153mhttps://example.com\x1b[39m";
        assert_eq!(strip_ansi(input), "https://example.com");
    }

    #[test]
    fn strip_ansi_preserves_plain_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
    }

    #[test]
    fn strip_ansi_handles_empty() {
        assert_eq!(strip_ansi(""), "");
    }
}
