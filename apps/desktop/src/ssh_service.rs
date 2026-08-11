use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use hermes_core::{AppServices, ConnectionService, ServiceError, ServiceFuture, ServiceResult};
use hermes_protocol::{
    ConnectionConfig, ConnectionConfigInput, ConnectionMode, ConnectionOauthLoginResult,
    ConnectionOauthLogoutResult, ConnectionProbeResult, ConnectionState, ConnectionTestResult,
};

use crate::{
    ssh::{self, SshConfig},
    ssh_lifecycle::{self, SshLifecycleConfig},
};

#[derive(Debug)]
enum NativeSshLease {
    Unix(ssh_lifecycle::SshLease),
    #[cfg(windows)]
    Windows(crate::ssh_windows::WindowsSshLease),
}

impl NativeSshLease {
    fn websocket_url(&self) -> ServiceResult<String> {
        match self {
            Self::Unix(lease) => lease.websocket_url(),
            #[cfg(windows)]
            Self::Windows(lease) => lease.websocket_url(),
        }
    }
}

/// Install the native SSH connection boundary without exposing process or token
/// authority to shared Dioxus UI code. SSH probes and owned remote lifecycle
/// both remain Desktop-native; all other connection modes delegate unchanged.
pub fn install_ssh_probe(services: &mut AppServices, data_dir: PathBuf) {
    let inner = services.connection.clone();
    services.connection = Arc::new(SshProbeConnection {
        inner,
        data_dir,
        active_lease: Mutex::new(None),
    });
}

struct SshProbeConnection {
    inner: Arc<dyn ConnectionService>,
    data_dir: PathBuf,
    active_lease: Mutex<Option<NativeSshLease>>,
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

    async fn remote_profile(&self, input: &ConnectionConfigInput) -> ServiceResult<String> {
        let current = self.inner.config(input.profile.as_deref()).await?;
        Ok(input
            .ssh_remote_profile
            .as_deref()
            .unwrap_or(&current.ssh_remote_profile)
            .trim()
            .to_owned())
    }

    fn clear_lease(&self) -> ServiceResult<()> {
        self.active_lease
            .lock()
            .map_err(|_| ServiceError::Platform("SSH lease lock was poisoned".into()))?
            .take();
        Ok(())
    }

    fn store_lease(&self, lease: NativeSshLease) -> ServiceResult<()> {
        self.active_lease
            .lock()
            .map_err(|_| ServiceError::Platform("SSH lease lock was poisoned".into()))?
            .replace(lease);
        Ok(())
    }

    async fn open_native_lease(
        &self,
        ssh: SshConfig,
        profile_scope: String,
        remote_profile: String,
    ) -> ServiceResult<NativeSshLease> {
        let probe = ssh::test_connection(&ssh).await;
        if probe.ok != Some(true) || probe.reachable != Some(true) {
            return Err(ServiceError::Transport(
                probe.error.unwrap_or_else(|| "SSH remote probe failed".into()),
            ));
        }
        let platform = probe.remote_platform.as_deref().ok_or_else(|| {
            ServiceError::Transport("SSH probe did not report a remote platform".into())
        })?;

        if platform.starts_with("Windows/") || platform == "Windows" {
            #[cfg(windows)]
            {
                return crate::ssh_windows::connect(
                    &ssh,
                    &profile_scope,
                    &remote_profile,
                    &self.data_dir,
                )
                .await
                .map(NativeSshLease::Windows);
            }
            #[cfg(not(windows))]
            {
                return Err(ServiceError::Unavailable(
                    "Windows-host SSH lifecycle requires the Windows Desktop build".into(),
                ));
            }
        }

        if platform.starts_with("Linux/")
            || platform == "Linux"
            || platform.starts_with("Darwin/")
            || platform == "Darwin"
        {
            return ssh_lifecycle::connect(&SshLifecycleConfig {
                ssh,
                profile_scope,
                remote_profile,
                data_dir: self.data_dir.clone(),
            })
            .await
            .map(NativeSshLease::Unix);
        }

        Err(ServiceError::Unavailable(format!(
            "unsupported SSH remote platform: {platform}"
        )))
    }

    async fn apply_ssh(&self, input: &ConnectionConfigInput) -> ServiceResult<ConnectionConfig> {
        let ssh = self.ssh_config(input).await?;
        let remote_profile = self.remote_profile(input).await?;
        let profile_scope = input.profile.clone().unwrap_or_default();
        let saved = self.inner.save_config(input).await?;
        self.inner.disconnect().await?;
        self.clear_lease()?;

        let lease = self
            .open_native_lease(ssh, profile_scope, remote_profile)
            .await?;
        let websocket_url = lease.websocket_url()?;
        if let Err(error) = self.inner.connect(&websocket_url).await {
            drop(lease);
            return Err(error);
        }
        if let Err(error) = self.store_lease(lease) {
            let _ = self.inner.disconnect().await;
            return Err(error);
        }
        Ok(saved)
    }

    fn input_from_config(config: &ConnectionConfig) -> ConnectionConfigInput {
        ConnectionConfigInput {
            mode: ConnectionMode::Ssh,
            profile: config.profile.clone(),
            ssh_host: Some(config.ssh_host.clone()),
            ssh_user: Some(config.ssh_user.clone()),
            ssh_port: Some(config.ssh_port),
            ssh_key_path: Some(config.ssh_key_path.clone()),
            ssh_remote_hermes_path: Some(config.ssh_remote_hermes_path.clone()),
            ssh_remote_profile: Some(config.ssh_remote_profile.clone()),
            ..ConnectionConfigInput::default()
        }
    }
}

impl ConnectionService for SshProbeConnection {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState> {
        Box::pin(async move {
            let config = self.inner.config(None).await?;
            if config.mode != ConnectionMode::Ssh {
                return self.inner.initialize().await;
            }
            self.apply_ssh(&Self::input_from_config(&config)).await?;
            Ok(ConnectionState::Open)
        })
    }

    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState> {
        self.inner.connect(websocket_url)
    }

    fn disconnect(&self) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            let result = self.inner.disconnect().await;
            let lease_result = self.clear_lease();
            result?;
            lease_result
        })
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
        let input = input.clone();
        Box::pin(async move {
            if input.mode == ConnectionMode::Ssh {
                return self.apply_ssh(&input).await;
            }
            let result = self.inner.apply_config(&input).await;
            self.clear_lease()?;
            result
        })
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
