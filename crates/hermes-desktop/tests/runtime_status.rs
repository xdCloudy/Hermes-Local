use futures_util::{SinkExt, StreamExt};
use hermes_desktop::NativeApp;
use hermes_protocol::JsonRpcFrame;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn runtime_status_matches_agent_contract_and_ignores_future_fields() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept gateway");
        let mut socket = accept_async(stream).await.expect("websocket handshake");
        let message = socket
            .next()
            .await
            .expect("status request")
            .expect("valid websocket frame");
        let Message::Text(text) = message else {
            panic!("expected text JSON-RPC request");
        };
        let request: JsonRpcFrame = serde_json::from_str(&text).expect("JSON-RPC request");
        assert_eq!(request.method.as_deref(), Some("status.get"));
        assert_eq!(request.params, Some(json!({})));

        socket
            .send(Message::Text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request.id,
                    "result": {
                        "phase": "ready",
                        "agent_version": "0.9.0",
                        "future_runtime_metric": { "value": 42 }
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("send status response");
    });

    let data_dir = std::env::temp_dir().join(format!("hermes-local-rt01-{}", std::process::id()));
    let app = NativeApp::new(data_dir);
    app.services
        .connection
        .connect(&format!("ws://{address}"))
        .await
        .expect("connect gateway");

    let status = app.services.runtime.status().await.expect("runtime status");
    assert_eq!(status.phase, "ready");
    assert_eq!(status.agent_version.as_deref(), Some("0.9.0"));

    server.await.expect("gateway server");
}
