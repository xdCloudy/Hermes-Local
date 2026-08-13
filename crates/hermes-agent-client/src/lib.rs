//! Async, typed JSON-RPC gateway client for Hermes Agent.

pub mod webhooks;

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use hermes_protocol::{ConnectionState, GatewayEvent, JsonRpcFrame, RpcId};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use tracing::{debug, warn};
use url::Url;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_mins(2);
const DEFAULT_RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(250);
const DEFAULT_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(5);
const MAX_GATEWAY_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
const COMMAND_CAPACITY: usize = 128;
const EVENT_CAPACITY: usize = 512;

type GatewaySocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone, Debug)]
pub struct GatewayOptions {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub reconnect_initial_delay: Duration,
    pub reconnect_max_delay: Duration,
}

impl Default for GatewayOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            reconnect_initial_delay: DEFAULT_RECONNECT_INITIAL_DELAY,
            reconnect_max_delay: DEFAULT_RECONNECT_MAX_DELAY,
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
    #[error("gateway is reconnecting")]
    Reconnecting,
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
    /// Connect to a Hermes Agent WebSocket endpoint and start the bounded client actor.
    ///
    /// After the initial handshake succeeds, transport loss is recovered inside the
    /// actor with bounded exponential backoff. Callers can observe
    /// [`ConnectionState::Error`], [`ConnectionState::Connecting`] and the eventual
    /// [`ConnectionState::Open`] transition through [`Self::connection_state`].
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::InvalidUrl`] for a non-WebSocket URL,
    /// [`GatewayError::ConnectTimeout`] when the initial handshake exceeds the configured timeout,
    /// or [`GatewayError::Transport`] when the initial WebSocket handshake fails.
    pub async fn connect(url: &str, options: GatewayOptions) -> Result<Self, GatewayError> {
        validate_gateway_url(url)?;
        let socket = connect_socket(url, options.connect_timeout).await?;

        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let (state_tx, state) = watch::channel(ConnectionState::Connecting);
        let (commands, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor_events = events.clone();
        let actor_url = url.to_owned();
        let actor_options = options.clone();
        tokio::spawn(async move {
            run_actor(
                actor_url,
                socket,
                command_rx,
                actor_events,
                state_tx,
                actor_options,
            )
            .await;
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

    /// Send a typed JSON-RPC request using the client's default request timeout.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when parameters or the response cannot be serialized,
    /// a timeout when the Agent does not answer in time, a remote error for JSON-RPC
    /// errors, [`GatewayError::Reconnecting`] while transport recovery is in progress,
    /// or [`GatewayError::Closed`] when the actor has stopped.
    pub async fn request<P, R>(&self, method: &str, params: P) -> Result<R, GatewayError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        self.request_with_timeout(method, params, self.request_timeout)
            .await
    }

    /// Send a typed JSON-RPC request with an explicit timeout.
    ///
    /// A timed-out request is removed from the actor's pending map so a late response
    /// cannot grow retained state or be delivered to a subsequent request.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when parameters or the response cannot be serialized,
    /// a timeout when the Agent does not answer within `timeout`, a remote error for
    /// JSON-RPC errors, [`GatewayError::Reconnecting`] while transport recovery is in
    /// progress, or [`GatewayError::Closed`] when the actor has stopped.
    pub async fn request_with_timeout<P, R>(
        &self,
        method: &str,
        params: P,
        timeout: Duration,
    ) -> Result<R, GatewayError>
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

        let value = match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) => return Err(GatewayError::Closed),
            Err(_) => {
                let _ = self.commands.send(Command::Cancel { id }).await;
                return Err(GatewayError::RequestTimeout(method.to_owned()));
            }
        };
        serde_json::from_value(value).map_err(|error| GatewayError::Protocol(error.to_string()))
    }

    /// Ask the client actor to close its WebSocket connection gracefully.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::Closed`] if the actor has already stopped and can no
    /// longer receive the close command.
    pub async fn close(&self) -> Result<(), GatewayError> {
        self.commands
            .send(Command::Close)
            .await
            .map_err(|_| GatewayError::Closed)
    }
}

fn validate_gateway_url(url: &str) -> Result<(), GatewayError> {
    let parsed = Url::parse(url).map_err(|error| GatewayError::InvalidUrl(error.to_string()))?;
    if !matches!(parsed.scheme(), "ws" | "wss") {
        return Err(GatewayError::InvalidUrl(
            "scheme must be ws or wss".to_owned(),
        ));
    }
    Ok(())
}

async fn connect_socket(url: &str, timeout: Duration) -> Result<GatewaySocket, GatewayError> {
    let mut websocket_config = WebSocketConfig::default();
    websocket_config.max_message_size = Some(MAX_GATEWAY_MESSAGE_BYTES);
    websocket_config.max_frame_size = Some(MAX_GATEWAY_MESSAGE_BYTES);
    tokio::time::timeout(
        timeout,
        connect_async_with_config(url, Some(websocket_config), false),
    )
    .await
    .map_err(|_| GatewayError::ConnectTimeout)?
    .map_err(|error| GatewayError::Transport(error.to_string()))
    .map(|(socket, _)| socket)
}

enum ConnectedExit {
    Shutdown,
    Lost(String),
}

async fn run_actor(
    url: String,
    mut socket: GatewaySocket,
    mut commands: mpsc::Receiver<Command>,
    events: broadcast::Sender<GatewayEvent>,
    state: watch::Sender<ConnectionState>,
    options: GatewayOptions,
) {
    let reconnect_initial_delay = options
        .reconnect_initial_delay
        .min(options.reconnect_max_delay);

    loop {
        state.send_replace(ConnectionState::Open);
        match run_connected(socket, &mut commands, &events).await {
            ConnectedExit::Shutdown => {
                state.send_replace(ConnectionState::Closed);
                return;
            }
            ConnectedExit::Lost(reason) => {
                warn!(%reason, "gateway transport lost; scheduling reconnect");
                state.send_replace(ConnectionState::Error);
            }
        }

        match reconnect(
            &url,
            &mut commands,
            &state,
            &options,
            reconnect_initial_delay,
        )
        .await
        {
            ReconnectExit::Shutdown => {
                state.send_replace(ConnectionState::Closed);
                return;
            }
            ReconnectExit::Connected(next_socket) => {
                socket = *next_socket;
            }
        }
    }
}

enum ReconnectExit {
    Shutdown,
    Connected(Box<GatewaySocket>),
}

async fn reconnect(
    url: &str,
    commands: &mut mpsc::Receiver<Command>,
    state: &watch::Sender<ConnectionState>,
    options: &GatewayOptions,
    initial_delay: Duration,
) -> ReconnectExit {
    let mut delay = initial_delay.min(options.reconnect_max_delay);

    loop {
        let mut sleeper = Box::pin(tokio::time::sleep(delay));
        loop {
            tokio::select! {
                command = commands.recv() => {
                    if handle_reconnect_command(command) {
                        return ReconnectExit::Shutdown;
                    }
                }
                () = &mut sleeper => break,
            }
        }

        state.send_replace(ConnectionState::Connecting);
        let mut connection = Box::pin(connect_socket(url, options.connect_timeout));
        let result = loop {
            tokio::select! {
                command = commands.recv() => {
                    if handle_reconnect_command(command) {
                        return ReconnectExit::Shutdown;
                    }
                }
                result = &mut connection => break result,
            }
        };

        match result {
            Ok(socket) => return ReconnectExit::Connected(Box::new(socket)),
            Err(error) => {
                warn!(%error, "gateway reconnect attempt failed");
                state.send_replace(ConnectionState::Error);
                delay = delay.saturating_mul(2).min(options.reconnect_max_delay);
            }
        }
    }
}

fn handle_reconnect_command(command: Option<Command>) -> bool {
    match command {
        Some(Command::Request { response, .. }) => {
            let _ = response.send(Err(GatewayError::Reconnecting));
            false
        }
        Some(Command::Cancel { .. }) => false,
        Some(Command::Close) | None => true,
    }
}

async fn run_connected(
    socket: GatewaySocket,
    commands: &mut mpsc::Receiver<Command>,
    events: &broadcast::Sender<GatewayEvent>,
) -> ConnectedExit {
    let (mut writer, mut reader) = socket.split();
    let mut pending = HashMap::<RpcId, ResponseSender>::new();

    let exit = loop {
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
                                break ConnectedExit::Lost(error.to_string());
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
                    break ConnectedExit::Shutdown;
                }
            },
            message = reader.next() => match message {
                Some(Ok(Message::Text(text))) => handle_text(&text, events, &mut pending),
                Some(Ok(Message::Binary(bytes))) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => handle_text(&text, events, &mut pending),
                    Err(error) => warn!(%error, "ignored non-UTF-8 gateway frame"),
                },
                Some(Ok(Message::Ping(payload))) => {
                    if let Err(error) = writer.send(Message::Pong(payload)).await {
                        break ConnectedExit::Lost(error.to_string());
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    break ConnectedExit::Lost("remote gateway closed the WebSocket".to_owned());
                }
                None => {
                    break ConnectedExit::Lost("gateway stream ended".to_owned());
                }
                Some(Ok(_)) => {},
                Some(Err(error)) => {
                    warn!(%error, "gateway receive failed");
                    break ConnectedExit::Lost(error.to_string());
                }
            }
        }
    };

    if !pending.is_empty() {
        let error = match &exit {
            ConnectedExit::Shutdown => GatewayError::Closed,
            ConnectedExit::Lost(reason) => GatewayError::Transport(reason.clone()),
        };
        for (_, response) in pending {
            let _ = response.send(Err(error.clone()));
        }
    }

    exit
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

    #[tokio::test]
    async fn rejects_http_gateway_url_before_network_io() {
        let result = GatewayClient::connect(
            "https://localhost:8080",
            GatewayOptions {
                connect_timeout: Duration::from_millis(1),
                ..GatewayOptions::default()
            },
        )
        .await;
        assert!(matches!(result, Err(GatewayError::InvalidUrl(_))));
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
        client.close().await.expect("close");
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
        client.close().await.expect("close");
    }

    #[tokio::test]
    async fn reconnects_after_transport_loss_and_accepts_new_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("first accept");
            let mut socket = accept_async(stream).await.expect("first handshake");
            let request = socket.next().await.expect("first request").expect("frame");
            let Message::Text(request) = request else {
                panic!("expected first text request");
            };
            let request: JsonRpcFrame =
                serde_json::from_str(&request).expect("first JSON-RPC request");
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": { "answer": 1 }
            });
            socket
                .send(Message::Text(response.to_string().into()))
                .await
                .expect("first response");
            socket
                .send(Message::Close(None))
                .await
                .expect("close first connection");

            tokio::time::sleep(Duration::from_millis(100)).await;

            let (stream, _) = listener.accept().await.expect("second accept");
            let mut socket = accept_async(stream).await.expect("second handshake");
            let request = socket.next().await.expect("second request").expect("frame");
            let Message::Text(request) = request else {
                panic!("expected second text request");
            };
            let request: JsonRpcFrame =
                serde_json::from_str(&request).expect("second JSON-RPC request");
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": { "answer": 2 }
            });
            socket
                .send(Message::Text(response.to_string().into()))
                .await
                .expect("second response");
        });

        let client = GatewayClient::connect(
            &format!("ws://{address}"),
            GatewayOptions {
                connect_timeout: Duration::from_millis(250),
                request_timeout: Duration::from_secs(1),
                reconnect_initial_delay: Duration::from_millis(10),
                reconnect_max_delay: Duration::from_millis(40),
            },
        )
        .await
        .expect("connect");
        let mut state = client.connection_state();

        let first: Value = client
            .request("first.get", serde_json::json!({}))
            .await
            .expect("first request");
        assert_eq!(first["answer"], 1);

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                state.changed().await.expect("state channel");
                if matches!(
                    *state.borrow(),
                    ConnectionState::Error | ConnectionState::Connecting
                ) {
                    break;
                }
            }
        })
        .await
        .expect("degraded transition");

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if *state.borrow() == ConnectionState::Open {
                    break;
                }
                state.changed().await.expect("state channel");
            }
        })
        .await
        .expect("reconnect");

        let second: Value = client
            .request("second.get", serde_json::json!({}))
            .await
            .expect("second request");
        assert_eq!(second["answer"], 2);
        client.close().await.expect("close");
    }

    #[tokio::test]
    async fn rejects_requests_while_reconnecting_without_losing_actor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("handshake");
            socket
                .send(Message::Close(None))
                .await
                .expect("close connection");
            tokio::time::sleep(Duration::from_millis(250)).await;
        });

        let client = GatewayClient::connect(
            &format!("ws://{address}"),
            GatewayOptions {
                connect_timeout: Duration::from_millis(50),
                request_timeout: Duration::from_millis(100),
                reconnect_initial_delay: Duration::from_millis(100),
                reconnect_max_delay: Duration::from_millis(100),
            },
        )
        .await
        .expect("connect");
        let mut state = client.connection_state();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                state.changed().await.expect("state channel");
                if *state.borrow() == ConnectionState::Error {
                    break;
                }
            }
        })
        .await
        .expect("error transition");

        let result = client
            .request::<_, Value>("during.reconnect", serde_json::json!({}))
            .await;
        assert!(matches!(result, Err(GatewayError::Reconnecting)));

        client.close().await.expect("close");
        server.await.expect("server");
    }
}
