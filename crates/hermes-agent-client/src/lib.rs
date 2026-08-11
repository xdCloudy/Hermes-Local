//! Async, typed JSON-RPC gateway client for Hermes Agent.

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use hermes_protocol::{ConnectionState, GatewayEvent, JsonRpcFrame, RpcId};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, warn};
use url::Url;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const COMMAND_CAPACITY: usize = 128;
const EVENT_CAPACITY: usize = 512;

#[derive(Clone, Debug)]
pub struct GatewayOptions {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for GatewayOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum GatewayError {
    #[error("invalid WebSocket URL: {0}")]
    InvalidUrl(String),
    #[error("connection timed out")]
    ConnectTimeout,
    #[error("gateway is closed")]
    Closed,
    #[error("request timed out: {0}")]
    RequestTimeout(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("gateway error {code:?}: {message}")]
    Remote { code: Option<i64>, message: String },
}

type ResponseSender = oneshot::Sender<Result<Value, GatewayError>>;

enum Command {
    Request {
        id: RpcId,
        method: String,
        params: Value,
        response: ResponseSender,
    },
    Cancel {
        id: RpcId,
    },
    Close,
}

#[derive(Clone)]
pub struct GatewayClient {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<GatewayEvent>,
    state: watch::Receiver<ConnectionState>,
    request_timeout: Duration,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl GatewayClient {
    pub async fn connect(url: &str, options: GatewayOptions) -> Result<Self, GatewayError> {
        let parsed =
            Url::parse(url).map_err(|error| GatewayError::InvalidUrl(error.to_string()))?;
        if !matches!(parsed.scheme(), "ws" | "wss") {
            return Err(GatewayError::InvalidUrl(
                "scheme must be ws or wss".to_owned(),
            ));
        }

        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let (state_tx, state) = watch::channel(ConnectionState::Connecting);
        let socket = tokio::time::timeout(options.connect_timeout, connect_async(url))
            .await
            .map_err(|_| GatewayError::ConnectTimeout)?
            .map_err(|error| GatewayError::Transport(error.to_string()))?
            .0;
        let (commands, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor_events = events.clone();
        tokio::spawn(async move {
            run_actor(socket, command_rx, actor_events, state_tx).await;
        });

        Ok(Self {
            commands,
            events,
            state,
            request_timeout: options.request_timeout,
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    pub fn connection_state(&self) -> watch::Receiver<ConnectionState> {
        self.state.clone()
    }

    pub async fn request<P, R>(&self, method: &str, params: P) -> Result<R, GatewayError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params = serde_json::to_value(params)
            .map_err(|error| GatewayError::Protocol(error.to_string()))?;
        let id = RpcId::String(format!(
            "r{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (response, receiver) = oneshot::channel();
        self.commands
            .send(Command::Request {
                id: id.clone(),
                method: method.to_owned(),
                params,
                response,
            })
            .await
            .map_err(|_| GatewayError::Closed)?;

        let value = match tokio::time::timeout(self.request_timeout, receiver).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) => return Err(GatewayError::Closed),
            Err(_) => {
                let _ = self.commands.send(Command::Cancel { id }).await;
                return Err(GatewayError::RequestTimeout(method.to_owned()));
            }
        };
        serde_json::from_value(value).map_err(|error| GatewayError::Protocol(error.to_string()))
    }

    pub async fn close(&self) -> Result<(), GatewayError> {
        self.commands
            .send(Command::Close)
            .await
            .map_err(|_| GatewayError::Closed)
    }
}

async fn run_actor<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut commands: mpsc::Receiver<Command>,
    events: broadcast::Sender<GatewayEvent>,
    state: watch::Sender<ConnectionState>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut writer, mut reader) = socket.split();
    let mut pending = HashMap::<RpcId, ResponseSender>::new();
    state.send_replace(ConnectionState::Open);

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Request { id, method, params, response }) => {
                    let frame = JsonRpcFrame::request(id.clone(), method, params);
                    match serde_json::to_string(&frame) {
                        Ok(text) => {
                            pending.insert(id.clone(), response);
                            if let Err(error) = writer.send(Message::Text(text.into())).await {
                                if let Some(response) = pending.remove(&id) {
                                    let _ = response.send(Err(GatewayError::Transport(error.to_string())));
                                }
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = response.send(Err(GatewayError::Protocol(error.to_string())));
                        }
                    }
                }
                Some(Command::Cancel { id }) => {
                    pending.remove(&id);
                }
                Some(Command::Close) | None => {
                    let _ = writer.close().await;
                    break;
                }
            },
            message = reader.next() => match message {
                Some(Ok(Message::Text(text))) => handle_text(&text, &events, &mut pending),
                Some(Ok(Message::Binary(bytes))) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => handle_text(&text, &events, &mut pending),
                    Err(error) => warn!(%error, "ignored non-UTF-8 gateway frame"),
                },
                Some(Ok(Message::Ping(payload))) => {
                    if writer.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {},
                Some(Err(error)) => {
                    warn!(%error, "gateway receive failed");
                    break;
                }
            }
        }
    }

    for (_, response) in pending {
        let _ = response.send(Err(GatewayError::Closed));
    }
    state.send_replace(ConnectionState::Closed);
}

fn handle_text(
    text: &str,
    events: &broadcast::Sender<GatewayEvent>,
    pending: &mut HashMap<RpcId, ResponseSender>,
) {
    let frame = match serde_json::from_str::<JsonRpcFrame>(text) {
        Ok(frame) => frame,
        Err(error) => {
            warn!(%error, "ignored malformed gateway frame");
            return;
        }
    };
    if let Some(event) = GatewayEvent::from_frame(&frame) {
        let _ = events.send(event);
        return;
    }
    let Some(id) = frame.id else {
        debug!("ignored gateway frame without id or event method");
        return;
    };
    let Some(response) = pending.remove(&id) else {
        debug!(?id, "ignored response for unknown or timed-out request");
        return;
    };
    let result = match frame.error {
        Some(error) => Err(GatewayError::Remote {
            code: error.code,
            message: error.message,
        }),
        None => Ok(frame.result.unwrap_or(Value::Null)),
    };
    let _ = response.send(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    #[test]
    fn rejects_http_gateway_url() {
        let result = Url::parse("https://localhost:8080").expect("URL");
        assert!(!matches!(result.scheme(), "ws" | "wss"));
    }

    #[test]
    fn routes_unknown_events_without_schema_loss() {
        let (events, mut receiver) = broadcast::channel(4);
        let mut pending = HashMap::new();
        handle_text(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"future.event","future":42}}"#,
            &events,
            &mut pending,
        );
        let event = receiver.try_recv().expect("event");
        assert_eq!(event.kind, "future.event");
        assert_eq!(event.extra.get("future"), Some(&Value::from(42)));
    }

    #[tokio::test]
    async fn round_trips_requests_and_interleaved_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("handshake");
            let request = socket.next().await.expect("request").expect("frame");
            let Message::Text(request) = request else {
                panic!("expected text request");
            };
            let request: JsonRpcFrame = serde_json::from_str(&request).expect("JSON-RPC request");
            assert_eq!(request.method.as_deref(), Some("example.get"));
            socket
                .send(Message::Text(
                    r#"{"jsonrpc":"2.0","method":"event","params":{"type":"status.update","payload":{"phase":"ready"}}}"#
                        .into(),
                ))
                .await
                .expect("event");
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": { "answer": 42 }
            });
            socket
                .send(Message::Text(response.to_string().into()))
                .await
                .expect("response");
        });

        let client =
            GatewayClient::connect(&format!("ws://{address}/api/ws"), GatewayOptions::default())
                .await
                .expect("connect");
        let mut events = client.subscribe();
        let result: Value = client
            .request("example.get", serde_json::json!({ "id": 7 }))
            .await
            .expect("request");
        assert_eq!(result["answer"], 42);
        assert_eq!(events.recv().await.expect("event").kind, "status.update");
    }

    #[tokio::test]
    async fn times_out_an_unanswered_request_without_closing_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("handshake");
            let _ = socket.next().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}"),
            GatewayOptions {
                request_timeout: Duration::from_millis(20),
                ..GatewayOptions::default()
            },
        )
        .await
        .expect("connect");
        let result = client
            .request::<_, Value>("never.responds", serde_json::json!({}))
            .await;
        assert!(
            matches!(result, Err(GatewayError::RequestTimeout(method)) if method == "never.responds")
        );
        assert_eq!(*client.connection_state().borrow(), ConnectionState::Open);
    }
}
