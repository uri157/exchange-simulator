use std::{borrow::Cow, time::Duration};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use serde_json::json;
use tokio::{
    select,
    sync::broadcast::error::RecvError,
    time::{interval, MissedTickBehavior},
};
use tracing::{debug, error, instrument, warn};
use tungstenite::protocol::frame::coding::CloseCode;

use crate::{app::bootstrap::AppState, dto::ws::WsQuery};

pub fn router() -> Router {
    Router::new().route("/ws", get(ws_handler))
}

#[instrument(
    skip(state, ws),
    fields(session_id = %query.session_id, streams = query.streams.as_deref().unwrap_or(""))
)]
pub async fn ws_handler(
    Extension(state): Extension<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(err) = state.sessions_service.get_session(query.session_id).await {
        warn!(error = %err, session_id = %query.session_id, "ws session validation failed");
        let body = Json(json!({
            "event": "error",
            "data": {"message": "session not available"}
        }));
        return (StatusCode::FORBIDDEN, body).into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(state, query, socket))
}

#[instrument(
    skip(state, socket),
    fields(session_id = %query.session_id, streams = query.streams.as_deref().unwrap_or(""))
)]
async fn handle_socket(state: AppState, query: WsQuery, mut socket: WebSocket) {
    let session_id = query.session_id;

    let mut rx = match state.broadcaster.subscribe(session_id).await {
        Ok(rx) => rx,
        Err(err) => {
            error!(%err, "failed to subscribe websocket");
            let payload = json!({
                "event": "error",
                "data": { "message": "subscription failed" }
            })
            .to_string();
            let _ = socket.send(Message::Text(payload)).await;
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Error,
                    reason: Cow::from("subscription failed"),
                })))
                .await;
            return;
        }
    };

    debug!(session_id = %session_id, "websocket connected");

    let mut ping_interval = interval(Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Discard the immediate first tick so we wait a full interval before sending the first ping.
    let _ = ping_interval.tick().await;

    let mut stats_interval = interval(Duration::from_secs(12));
    stats_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let _ = stats_interval.tick().await;

    'outer: loop {
        select! {
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(err) = socket.send(Message::Pong(payload)).await {
                            warn!(error = %err, session_id = %session_id, "failed to reply pong");
                            break 'outer;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        debug!(session_id = %session_id, "received pong from client");
                    }
                    Some(Ok(Message::Close(frame))) => {
                        debug!(session_id = %session_id, "client closed websocket");
                        let _ = socket.send(Message::Close(frame)).await;
                        break 'outer;
                    }
                    Some(Err(err)) => {
                        warn!(error = %err, session_id = %session_id, "websocket receive error");
                        break 'outer;
                    }
                    None => {
                        debug!(session_id = %session_id, "websocket stream ended");
                        break 'outer;
                    }
                }
            }
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(message) => {
                        if let Err(err) = socket.send(Message::Text(message)).await {
                            debug!(error = %err, session_id = %session_id, "failed to send websocket message");
                            break 'outer;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        warn!(session_id = %session_id, skipped, "broadcast lagged, closing websocket");
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Normal,
                                reason: Cow::from("session closed"),
                            })))
                            .await;
                        break 'outer;
                    }
                    Err(RecvError::Closed) => {
                        debug!(session_id = %session_id, "broadcast channel closed, terminating websocket");
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Normal,
                                reason: Cow::from("session closed"),
                            })))
                            .await;
                        break 'outer;
                    }
                }
            }
            _ = ping_interval.tick() => {
                if let Err(err) = socket.send(Message::Ping(Vec::new())).await {
                    debug!(error = %err, session_id = %session_id, "failed to send ping");
                    break 'outer;
                }
            }
            _ = stats_interval.tick() => {
                let connections = state.broadcaster.subscriber_count(session_id).await;
                let payload = json!({
                    "event": "stats",
                    "data": {"connections": connections}
                })
                .to_string();

                if let Err(err) = socket.send(Message::Text(payload)).await {
                    debug!(error = %err, session_id = %session_id, "failed to send stats payload");
                    break 'outer;
                }
            }
        }
    }

    debug!(session_id = %session_id, "websocket disconnected");
}
