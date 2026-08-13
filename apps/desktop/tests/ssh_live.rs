//! Opt-in live OpenSSH interoperability coverage.
//!
//! This test is ignored by default because ordinary workspace CI is hermetic.
//! The dedicated `ssh-interoperability` workflow provisions a runner-local
//! OpenSSH server and executes this test explicitly.

#[path = "../src/base64.rs"]
mod base64_impl;
pub use base64_impl::{Engine, engine};
extern crate self as base64;

#[path = "../src/ssh.rs"]
mod ssh;

use std::env;

#[test]
#[ignore = "requires the explicitly provisioned SSH interoperability fixture"]
fn live_openssh_probe_reports_remote_runtime() {
    let host = env::var("HERMES_SSH_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let user = required("HERMES_SSH_TEST_USER");
    let port = required("HERMES_SSH_TEST_PORT")
        .parse::<u16>()
        .expect("HERMES_SSH_TEST_PORT must be a u16");
    let key = required("HERMES_SSH_TEST_KEY");
    let remote_hermes = required("HERMES_SSH_TEST_HERMES");

    let config = ssh::SshConfig::new(
        &host,
        Some(&user),
        Some(port),
        Some(&key),
        Some(&remote_hermes),
    )
    .expect("live SSH fixture config must be valid");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("Tokio runtime");
    let result = runtime.block_on(ssh::test_connection(&config));

    assert_eq!(result.ok, Some(true), "SSH probe failed: {result:?}");
    assert_eq!(result.reachable, Some(true), "SSH host was not reachable");
    assert!(
        result
            .remote_platform
            .as_deref()
            .is_some_and(|platform| platform.starts_with("Linux/")),
        "unexpected remote platform: {:?}",
        result.remote_platform
    );
    assert_eq!(
        result.remote_hermes_version.as_deref(),
        Some("hermes-live-ssh-test 0.0.1")
    );
    assert_eq!(
        result.remote_hermes_path.as_deref(),
        Some(remote_hermes.as_str())
    );
}

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required for the live SSH fixture"))
}
