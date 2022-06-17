use crate::plugin_system::{Event, Notification};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use log::{error, info, warn};
use serde_json;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc, oneshot,
};

#[derive(Debug)]
struct WebContext {
    notification_sender: broadcast::Sender<Notification>,
    event_sender: mpsc::Sender<Event>,
}

pub async fn start(
    event_sender: mpsc::Sender<Event>,
    notification_sender: broadcast::Sender<Notification>,
) -> anyhow::Result<()> {
    let ctx = Arc::new(WebContext {
        notification_sender,
        event_sender,
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .layer(Extension(ctx));
    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    info!("starting server on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(ctx): Extension<Arc<WebContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, ctx))
}

async fn websocket(stream: WebSocket, ctx: Arc<WebContext>) {
    let (mut sender, mut receiver) = stream.split();

    let mut notification_receiver = ctx.notification_sender.subscribe();
    let event_sender = ctx.event_sender.clone();

    loop {
        tokio::select!(
            notification_result = notification_receiver.recv() => {
                let notification = match notification_result {
                    Ok(n) => n,
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(skipped)) => {
                        warn!("websocket lagged and skipped {} notifications", skipped);
                        continue;
                    }
                };

                let json = match serde_json::to_string(&notification) {
                    Ok(msg) => msg,
                    Err(e) => {
                        error!("could not serialize notification {:?}: {}", notification, e);
                        continue;
                    }
                };

                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            command_option = receiver.next() => {
                match command_option {
                    Some(Ok(Message::Text(command))) => {
                        let command = command.trim().to_string();
                        let (tx, rx) = oneshot::channel();
                        if event_sender.send(Event::CliCommand { command, reply_sender: tx }).await.is_err() {
                            warn!("event receiver was closed");
                            break;
                        }
                        let reply = match rx.await {
                            Ok(r) => r,
                            Err(_) => "no reply".to_string(),
                        };

                        if sender.send(Message::Text(reply)).await.is_err() {
                            break;
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
