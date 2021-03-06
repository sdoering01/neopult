use crate::{
    config::{Config, WEB_ROOT},
    plugin_system::{ActionIdentifier, ClientCommand, Event, Notification, SystemInfo},
};
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Extension,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, get_service},
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{borrow::Cow, io, net::SocketAddr, sync::Arc};
use tokio::{
    sync::{
        broadcast::{self, error::RecvError},
        mpsc, oneshot,
    },
    time::{self, Duration, Instant},
};
use tower_http::{services::ServeDir, trace::TraceLayer};

// NOTE: Make sure to adjust the values in the client accordingly
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

const CLOSE_MSG_AUTH: Message = Message::Close(Some(CloseFrame {
    code: 1,
    reason: Cow::Borrowed("auth"),
}));
const CLOSE_MSG_AUTH_TIMEOUT: Message = Message::Close(Some(CloseFrame {
    code: 2,
    reason: Cow::Borrowed("auth_timeout"),
}));

#[derive(Debug)]
struct WebContext {
    notification_sender: broadcast::Sender<Notification>,
    event_sender: mpsc::Sender<Event>,
    websocket_password_hash: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerResponse {
    request_id: String,
    success: bool,
    message: Option<String>,
}

impl ServerResponse {
    fn new(request_id: String, success: bool, message: Option<String>) -> Self {
        Self {
            request_id,
            success,
            message,
        }
    }

    fn new_success(request_id: String) -> Self {
        Self::new(request_id, true, None)
    }

    fn new_internal_error(request_id: String) -> Self {
        Self::new(request_id, false, Some("Internal Server Error".to_string()))
    }

    fn from_error(request_id: String, error: anyhow::Error) -> Self {
        Self::new(request_id, false, Some(format!("{:?}", error)))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FromServerError {
    ParseError(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FromServer {
    Ping,
    Pong,
    SystemInfo(SystemInfo),
    Notification(Notification),
    Response(ServerResponse),
    Error(FromServerError),
}

#[derive(Debug, Deserialize, Serialize)]
struct ClientRequest {
    request_id: String,
    body: FromClientBody,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FromClient {
    Ping,
    Pong,
    Request(ClientRequest),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FromClientBody {
    CallAction(ActionIdentifier),
}

pub async fn start(
    config: Arc<Config>,
    event_sender: mpsc::Sender<Event>,
    notification_sender: broadcast::Sender<Notification>,
) -> anyhow::Result<()> {
    let websocket_password_hash = Sha256::new()
        .chain_update(config.websocket_password.as_bytes())
        .finalize();

    let ctx = Arc::new(WebContext {
        notification_sender,
        event_sender,
        websocket_password_hash: websocket_password_hash.to_vec(),
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .fallback(get_service(ServeDir::new(WEB_ROOT)).handle_error(handle_error))
        .layer(Extension(ctx))
        .layer(TraceLayer::new_for_http());
    let addr = SocketAddr::from(([0, 0, 0, 0], 4200 + config.channel as u16));
    info!("starting server on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn handle_error(_err: io::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(ctx): Extension<Arc<WebContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, ctx))
}

async fn websocket(stream: WebSocket, ctx: Arc<WebContext>) {
    let (mut sender, mut receiver) = stream.split();
    let mut is_authenticated = false;

    match time::timeout(AUTH_TIMEOUT, receiver.next()).await {
        Ok(msg) => {
            if let Some(Ok(Message::Text(auth_msg))) = msg {
                if let Some(got_password) = auth_msg.strip_prefix("Password ") {
                    let got_hash = Sha256::new().chain_update(got_password).finalize();
                    // Compare hashes of the passwords to prevent timing attacks
                    if *ctx.websocket_password_hash == *got_hash {
                        is_authenticated = true;
                    }
                }
            }
        }
        Err(_) => {
            let _ = sender.send(CLOSE_MSG_AUTH_TIMEOUT).await;
            return;
        }
    }

    if !is_authenticated {
        let _ = sender.send(CLOSE_MSG_AUTH).await;
        return;
    }

    let mut notification_receiver = ctx.notification_sender.subscribe();
    let event_sender = ctx.event_sender.clone();

    let (tx, rx) = oneshot::channel();
    event_sender
        .send(Event::FetchSystemInfo { reply_sender: tx })
        .await
        .expect("event receiver was closed");
    let system_info = rx.await.expect("fetch system info got no reply");
    let msg = FromServer::SystemInfo(system_info);
    let json = serde_json::to_string(&msg).expect("serialization failed");
    // Prevent accidental reuse when using variable with same name
    drop(msg);

    if sender.send(Message::Text(json)).await.is_err() {
        return;
    }

    let mut hb = Instant::now();
    let mut hb_interval = time::interval(HEARTBEAT_INTERVAL);

    loop {
        tokio::select!(
            _ = hb_interval.tick() => {
                if Instant::now().duration_since(hb) > CLIENT_TIMEOUT {
                    debug!("client timed out");
                    break;
                }

                let json = serde_json::to_string(&FromServer::Ping).expect("serialization failed");
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            },
            notification_result = notification_receiver.recv() => {
                let notification = match notification_result {
                    Ok(n) => n,
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(skipped)) => {
                        warn!("websocket lagged and skipped {} notifications", skipped);
                        continue;
                    }
                };

                let msg = FromServer::Notification(notification);
                let json = match serde_json::to_string(&msg) {
                    Ok(msg) => msg,
                    Err(e) => {
                        error!("could not serialize notification message {:?}: {}", msg, e);
                        continue;
                    }
                };

                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            command_option = receiver.next() => {
                match command_option {
                    Some(Ok(Message::Text(client_json))) => {
                        let client_msg: FromClient = match serde_json::from_str(&client_json) {
                            Ok(msg) => msg,
                            Err(e) => {
                                warn!("could not parse client request: {} -- request was: {}", e, client_json);
                                let server_msg = FromServer::Error(FromServerError::ParseError(e.to_string()));
                                let server_json = serde_json::to_string(&server_msg).expect("serialization failed");
                                if sender.send(Message::Text(server_json)).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                        };
                        match client_msg {
                            FromClient::Ping => {
                                hb = Instant::now();
                                let json = serde_json::to_string(&FromServer::Pong).expect("serialization failed");
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            },
                            FromClient::Pong => {
                                hb = Instant::now();
                            },
                            FromClient::Request(request) => {
                                let request_id = request.request_id;

                                match request.body {
                                    FromClientBody::CallAction(identifier) => {
                                        let (tx, rx) = oneshot::channel();
                                        let command = ClientCommand::CallAction {
                                            identifier: identifier.clone(),
                                            error_sender: tx
                                        };
                                        send_client_command(&event_sender, command).await;

                                        let response = match rx.await {
                                            Ok(Ok(_)) => ServerResponse::new_success(request_id),
                                            Ok(Err(e)) => {
                                                error!("error when calling action {}: {:?}", identifier, e);
                                                ServerResponse::from_error(request_id, e)
                                            },
                                            Err(_) => {
                                                error!("plugin system didn't reply to call action command");
                                                ServerResponse::new_internal_error(request_id)
                                            },
                                        };
                                        let json = serde_json::to_string(&FromServer::Response(response)).expect("serialization failed");
                                        if sender.send(Message::Text(json)).await.is_err() {
                                            break;
                                        }
                                    },
                                };
                            }
                        }
                    }
                    _ => {
                        break;
                    }
                }
            }
        );
    }
}

async fn send_client_command(event_sender: &mpsc::Sender<Event>, command: ClientCommand) {
    event_sender
        .send(Event::ClientCommand(command))
        .await
        .expect("event receiver was closed");
}
