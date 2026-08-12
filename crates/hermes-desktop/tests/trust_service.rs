use futures_util::{SinkExt, StreamExt};
use hermes_core::ServiceError;
use hermes_desktop::NativeApp;
use hermes_protocol::JsonRpcFrame;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn trust_service_matches_agent_contract_and_rejects_invalid_policy() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept gateway");
        let mut socket = accept_async(stream).await.expect("websocket handshake");
        let expected = [
            (
                "trust.get",
                json!({}),
                json!({
                    "policy": "ask",
                    "skills": [],
                    "mcp_servers": [],
                    "future_diagnostic": { "state": "ok" }
                }),
            ),
            (
                "trust.set_policy",
                json!({ "policy": "allowlist" }),
                json!({
                    "policy": "allowlist",
                    "skills": [],
                    "mcp_servers": []
                }),
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

    let data_dir = std::env::temp_dir().join(format!("hermes-local-ag03-{}", std::process::id()));
    let app = NativeApp::new(data_dir);
    app.services
        .connection
        .connect(&format!("ws://{address}"))
        .await
        .expect("connect gateway");

    let snapshot = app.services.trust.snapshot().await.expect("trust snapshot");
    assert_eq!(snapshot.policy, "ask");

    let updated = app
        .services
        .trust
        .set_policy("allowlist")
        .await
        .expect("set trust policy");
    assert_eq!(updated.policy, "allowlist");

    let invalid = app.services.trust.set_policy("../other").await;
    assert!(matches!(invalid, Err(ServiceError::InvalidInput(_))));

    server.await.expect("gateway server");
}
