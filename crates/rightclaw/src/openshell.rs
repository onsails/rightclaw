//! OpenShell gRPC client — mTLS connection, sandbox readiness polling.

use std::path::Path;
use std::time::Duration;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

use crate::openshell_proto::openshell::v1::open_shell_client::OpenShellClient;
use crate::openshell_proto::openshell::v1::GetSandboxRequest;

/// SANDBOX_PHASE_READY value from openshell.datamodel.v1.SandboxPhase.
const SANDBOX_PHASE_READY: i32 = 2;

/// Generate deterministic sandbox name from agent name.
pub fn sandbox_name(agent_name: &str) -> String {
    format!("rightclaw-{agent_name}")
}

/// SSH host alias for an agent's sandbox (used in SSH config).
pub fn ssh_host(agent_name: &str) -> String {
    format!("openshell-rightclaw-{agent_name}")
}

/// Connect to the OpenShell gRPC server with mTLS.
///
/// Reads CA cert, client cert, and client key from `mtls_dir`:
/// - `ca.crt`  — CA certificate
/// - `tls.crt` — client certificate
/// - `tls.key` — client private key
pub async fn connect_grpc(
    mtls_dir: &Path,
) -> miette::Result<OpenShellClient<Channel>> {
    let ca_pem = tokio::fs::read(mtls_dir.join("ca.crt"))
        .await
        .map_err(|e| miette::miette!("failed to read ca.crt from {}: {e:#}", mtls_dir.display()))?;

    let client_cert = tokio::fs::read(mtls_dir.join("tls.crt"))
        .await
        .map_err(|e| miette::miette!("failed to read tls.crt from {}: {e:#}", mtls_dir.display()))?;

    let client_key = tokio::fs::read(mtls_dir.join("tls.key"))
        .await
        .map_err(|e| miette::miette!("failed to read tls.key from {}: {e:#}", mtls_dir.display()))?;

    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .identity(Identity::from_pem(client_cert, client_key))
        .domain_name("localhost");

    let channel = Channel::from_static("https://127.0.0.1:8080")
        .tls_config(tls)
        .map_err(|e| miette::miette!("TLS config error: {e:#}"))?
        .connect()
        .await
        .map_err(|e| miette::miette!("gRPC connect to 127.0.0.1:8080 failed: {e:#}"))?;

    Ok(OpenShellClient::new(channel))
}

/// Check whether a sandbox has reached READY phase.
pub async fn is_sandbox_ready(
    client: &mut OpenShellClient<Channel>,
    name: &str,
) -> miette::Result<bool> {
    let resp = client
        .get_sandbox(GetSandboxRequest {
            name: name.to_owned(),
        })
        .await
        .map_err(|e| miette::miette!("GetSandbox RPC failed for '{name}': {e:#}"))?;

    let sandbox = resp
        .into_inner()
        .sandbox
        .ok_or_else(|| miette::miette!("GetSandbox returned empty response for '{name}'"))?;

    Ok(sandbox.phase == SANDBOX_PHASE_READY)
}

/// Poll until a sandbox reaches READY phase, or timeout.
pub async fn wait_for_ready(
    client: &mut OpenShellClient<Channel>,
    name: &str,
    timeout_secs: u64,
    poll_interval_secs: u64,
) -> miette::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let interval = Duration::from_secs(poll_interval_secs);

    loop {
        if is_sandbox_ready(client, name).await? {
            tracing::info!(sandbox = name, "sandbox is READY");
            return Ok(());
        }

        if tokio::time::Instant::now() + interval > deadline {
            return Err(miette::miette!(
                "sandbox '{name}' did not become READY within {timeout_secs}s"
            ));
        }

        tracing::debug!(sandbox = name, "sandbox not ready, polling again in {poll_interval_secs}s");
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_name_prefixes_agent_name() {
        assert_eq!(sandbox_name("brain"), "rightclaw-brain");
        assert_eq!(sandbox_name("worker-1"), "rightclaw-worker-1");
    }

    #[test]
    fn ssh_host_prefixes_sandbox_name() {
        assert_eq!(ssh_host("brain"), "openshell-rightclaw-brain");
        assert_eq!(ssh_host("worker-1"), "openshell-rightclaw-worker-1");
    }
}
