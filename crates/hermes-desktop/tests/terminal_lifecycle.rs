use std::{fs, path::Path, time::Duration};

use hermes_core::ServiceError;
use hermes_desktop::NativeApp;
use uuid::Uuid;

fn test_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("hermes-terminal-lifecycle-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("terminal test root");
    root
}

#[tokio::test]
async fn pty_round_trip_resize_and_synchronous_dispose() {
    let root = test_root();
    let app = NativeApp::new(root.clone());
    let terminal = app.services.terminal.clone();

    let id = terminal.start(&root, 80, 24).await.expect("start PTY");
    terminal
        .write(&id, b"echo HERMES_PTY_LIFECYCLE\r\n")
        .await
        .expect("write PTY");

    let mut collected = Vec::new();
    for _ in 0..80 {
        collected.extend(terminal.read(&id).await.expect("read PTY"));
        if String::from_utf8_lossy(&collected).contains("HERMES_PTY_LIFECYCLE") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        String::from_utf8_lossy(&collected).contains("HERMES_PTY_LIFECYCLE"),
        "PTY did not return the echo marker: {}",
        String::from_utf8_lossy(&collected)
    );

    terminal.resize(&id, 100, 35).await.expect("resize PTY");
    terminal
        .dispose_now(&id)
        .expect("dispose PTY synchronously");
    assert!(matches!(
        terminal.read(&id).await,
        Err(ServiceError::NotFound(_))
    ));

    let second = terminal.start(&root, 80, 24).await.expect("restart PTY");
    terminal.dispose(&second).await.expect("async dispose PTY");
    assert!(matches!(
        terminal.read(&second).await,
        Err(ServiceError::NotFound(_))
    ));

    fs::remove_dir_all(root).expect("remove terminal test root");
}

#[tokio::test]
async fn rejects_invalid_terminal_dimensions_and_cwd() {
    let root = test_root();
    let app = NativeApp::new(root.clone());
    let terminal = app.services.terminal.clone();

    assert!(matches!(
        terminal.start(&root, 0, 24).await,
        Err(ServiceError::InvalidInput(_))
    ));
    assert!(matches!(
        terminal
            .start(
                Path::new("Z:/definitely-not-a-real-hermes-directory"),
                80,
                24
            )
            .await,
        Err(ServiceError::InvalidInput(_))
    ));

    fs::remove_dir_all(root).expect("remove terminal test root");
}

#[test]
fn dioxus_terminal_owns_cleanup_through_typed_service() {
    let terminal = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/terminal.rs"
    ));
    let ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/lib.rs"
    ));

    assert!(terminal.contains("services.terminal.clone()"));
    assert!(terminal.contains("use_drop(move ||"));
    assert!(terminal.contains("cleanup_service.dispose_now(&id)"));
    assert!(terminal.contains("start(Path::new(&cwd), next_cols, next_rows).await"));
    assert!(terminal.contains("read_service.read(&id).await"));
    assert!(terminal.contains("service.write(&id, &bytes).await"));
    assert!(terminal.contains("service.resize(&id, next_cols, next_rows).await"));
    assert!(terminal.contains("service.dispose(&id).await"));
    assert!(!terminal.contains("Command::new"));
    assert!(!terminal.contains("std::process"));
    assert!(!terminal.contains("portable_pty"));
    assert!(ui.contains("mod terminal;"));
    assert!(ui.contains("use terminal::Terminal;"));
    assert!(!ui.contains("simple_surface!(\n    Terminal,"));
}
