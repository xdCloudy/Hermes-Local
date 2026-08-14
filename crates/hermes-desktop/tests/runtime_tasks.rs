use futures_util::{SinkExt, StreamExt};
use hermes_core::ServiceError;
use hermes_desktop::NativeApp;
use hermes_protocol::JsonRpcFrame;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn runtime_task_service_matches_agent_contract_and_rejects_path_injection() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept gateway");
        let mut socket = accept_async(stream).await.expect("websocket handshake");

        let expected = [
            (
                "tasks.list",
                json!({}),
                json!({
                    "tasks": [{
                        "id": "task-1",
                        "action": "security",
                        "status": "succeeded",
                        "progress": { "percent": 100 },
                        "stage": "report",
                        "output": "x".repeat(300_000),
                        "outputTruncated": true,
                        "completedAt": "2026-08-14T00:00:00Z",
                        "exitCode": 0,
                        "result": {
                            "kind": "report",
                            "path": "security/reports/latest-scan.json"
                        }
                    }]
                }),
            ),
            (
                "tasks.start",
                json!({
                    "action": "security-scan",
                    "input": { "scope": "workspace" }
                }),
                json!({
                    "id": "task-2",
                    "name": "Security scan",
                    "state": "queued"
                }),
            ),
            (
                "tasks.cancel",
                json!({ "task_id": "task-2" }),
                json!({ "ok": true }),
            ),
        ];

        for (method, params, result) in expected {
            let message = socket
                .next()
                .await
                .expect("request frame")
                .expect("valid websocket frame");
            let Message::Text(text) = message else {
                panic!("expected text JSON-RPC request");
            };
            let request: JsonRpcFrame = serde_json::from_str(&text).expect("JSON-RPC request");
            assert_eq!(request.method.as_deref(), Some(method));
            assert_eq!(request.params, Some(params));

            socket
                .send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": result
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("send response");
        }
    });

    let data_dir = std::env::temp_dir().join(format!(
        "hermes-local-rt02-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let app = NativeApp::new(data_dir);
    app.services
        .connection
        .connect(&format!("ws://{address}"))
        .await
        .expect("connect gateway");

    let tasks = app.services.runtime.actions().await.expect("list tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "task-1");
    assert_eq!(tasks[0].name, "security");
    assert_eq!(tasks[0].state, "succeeded");
    assert_eq!(tasks[0].progress, Some(1.0));
    assert_eq!(tasks[0].stage.as_deref(), Some("report"));
    assert_eq!(tasks[0].output.len(), 256 * 1024);
    assert!(tasks[0].output_truncated);
    assert_eq!(tasks[0].exit_code, Some(0));
    assert_eq!(
        tasks[0].result.as_ref().map(|result| result.path.as_str()),
        Some("security/reports/latest-scan.json")
    );

    let started = app
        .services
        .runtime
        .start_action("security-scan", json!({ "scope": "workspace" }))
        .await
        .expect("start task");
    assert_eq!(started.id, "task-2");
    assert_eq!(started.state, "queued");

    let invalid_action = app
        .services
        .runtime
        .start_action("../other", Value::Null)
        .await;
    assert!(matches!(invalid_action, Err(ServiceError::InvalidInput(_))));

    app.services
        .runtime
        .cancel_action("task-2")
        .await
        .expect("cancel task");

    let invalid_task = app
        .services
        .runtime
        .cancel_action("task?profile=other")
        .await;
    assert!(matches!(invalid_task, Err(ServiceError::InvalidInput(_))));

    server.await.expect("gateway server");
}
