use std::sync::Arc;

use hermes_core::{AppServices, ConnectionService, ServiceError, ServiceFuture, ServiceResult};
use hermes_protocol::{
    ConnectionConfig, ConnectionConfigInput, ConnectionMode, ConnectionOauthLoginResult,
    ConnectionOauthLogoutResult, ConnectionProbeResult, ConnectionState, ConnectionTestResult,
};

use crate::ssh::{self, SshConfig};

/// Add native SSH probe parity without exposing process authority to the shared
/// Dioxus UI. This adapter is intentionally narrow: connection mutation remains
/// delegated to the underlying native service until the owned remote lifecycle
/// and tunnel lease are ported as the next slice.
pub fn install_ssh_probe(services: &mut AppServices) {
    let inner = services.connection.clone();
    services.connection = Arc::new(SshProbeConnection { inner });
}

struct SshProbeConnection {
    inner: Arc<dyn ConnectionService>,
}

impl SshProbeConnection {
    async fn ssh_config(&self, input: &ConnectionConfigInput) -> ServiceResult<SshConfig> {
        let current = self.inner.config(input.profile.as_deref()).await?;
        let host = input.ssh_host.as_deref().unwrap_or(&current.ssh_host);
        let user = input.ssh_user.as_deref().unwrap_or(&current.ssh_user);
        let key_path = input
            .ssh_key_path
            .as_deref()
            .unwrap_or(&current.ssh_key_path);
        let remote_hermes_path = input
            .ssh_remote_hermes_path
            .as_deref()
            .unwrap_or(&current.ssh_remote_hermes_path);
        let port = input.ssh_port.unwrap_or(current.ssh_port);
        SshConfig::new(
            host,
            Some(user),
            port,
            Some(key_path),
            Some(remote_hermes_path),
        )
        .map_err(ServiceError::InvalidInput)
    }
}

impl ConnectionService for SshProbeConnection {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState> {
        self.inner.initialize()
    }

    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState> {
        self.inner.connect(websocket_url)
    }

    fn disconnect(&self) -> ServiceFuture<'_, ()> {
        self.inner.disconnect()
    }

    fn state(&self) -> ServiceResult<ConnectionState> {
        self.inner.state()
    }

    fn config(&self, profile: Option<&str>) -> ServiceFuture<'_, ConnectionConfig> {
        self.inner.config(profile)
    }

    fn save_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        self.inner.save_config(input)
    }

    fn apply_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        self.inner.apply_config(input)
    }

    fn test_config(
        &self,
        input: &ConnectionConfigInput,
    ) -> ServiceFuture<'_, ConnectionTestResult> {
        let input = input.clone();
        Box::pin(async move {
            if input.mode != ConnectionMode::Ssh {
                return self.inner.test_config(&input).await;
            }
            let config = self.ssh_config(&input).await?;
            Ok(ssh::test_connection(&config).await)
        })
    }

    fn probe_config(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionProbeResult> {
        self.inner.probe_config(remote_url)
    }

    fn oauth_login(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLoginResult> {
        self.inner.oauth_login(remote_url)
    }

    fn oauth_logout(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLogoutResult> {
        self.inner.oauth_logout(remote_url)
    }
}
