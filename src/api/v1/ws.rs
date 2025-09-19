use std::{borrow::Cow, collections::HashSet, net::SocketAddr};

use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Query,
    },
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use indexmap::IndexSet;
use tokio::sync::broadcast::error::RecvError;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use crate::{
    app::bootstrap::AppState, domain::models::SessionStatus, dto::ws::WsQuery, error::AppError,
    infra::ws::broadcaster::WsBroadcastMessage,
};

pub const WS_ROUTE: &str = "/api/v1/ws";

pub fn router() -> Router {
    Router::new().route(WS_ROUTE, get(ws_handler))
}

#[instrument(
    skip(state, ws, headers),
    fields(session_id = %query.session_id)
)]
pub async fn ws_handler(
    Extension(state): Extension<AppState>,
    Query(query): Query<WsQuery>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    match prepare_connection(&state, &query, remote_addr, &headers).await {
        Ok((streams, user_agent)) => {
            let session_id = query.session_id;
            let state_clone = state.clone();
            ws.on_upgrade(move |socket| {
                handle_socket(
                    state_clone,
                    session_id,
                    streams,
                    socket,
                    remote_addr,
                    user_agent,
                )
            })
            .into_response()
        }
        Err(response) => response,
    }
}

#[instrument(
    skip(state, headers),
    fields(session_id = %query.session_id)
)]
async fn prepare_connection(
    state: &AppState,
    query: &WsQuery,
    remote_addr: SocketAddr,
    headers: &HeaderMap,
) -> Result<(Vec<String>, Option<String>), Response> {
    let streams = parse_streams(&query.streams).map_err(|err| err.into_response())?;

    let session = state
        .sessions_service
        .get_session(query.session_id)
        .await
        .map_err(|err| err.into_response())?;

    let expected: IndexSet<String> = session
        .symbols
        .iter()
        .map(|symbol| format!("kline@{}:{}", session.interval.as_str(), symbol))
        .collect();

    let invalid: Vec<String> = streams
        .iter()
        .filter(|stream| !expected.contains(*stream))
        .cloned()
        .collect();

    if !invalid.is_empty() {
        return Err(AppError::Validation(format!(
            "unsupported streams requested: {}",
            invalid.join(",")
        ))
        .into_response());
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string());

    info!(
        session_id = %query.session_id,
        remote_addr = %remote_addr,
        streams = %streams.join(","),
        user_agent = user_agent.as_deref().unwrap_or(""),
        status = ?session.status,
        "websocket connection accepted"
    );

    if session.status != SessionStatus::Running {
        info!(
            session_id = %query.session_id,
            status = ?session.status,
            "session is not RUNNING yet; klines will stream once running"
        );
    }

    Ok((streams, user_agent))
}

fn parse_streams(raw: &str) -> Result<Vec<String>, AppError> {
    let mut unique = IndexSet::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        unique.insert(trimmed.to_string());
    }

    if unique.is_empty() {
        return Err(AppError::Validation(
            "streams query parameter cannot be empty".into(),
        ));
    }

    Ok(unique.into_iter().collect())
}

#[instrument(
    skip(state, socket, streams, user_agent),
    fields(session_id = %session_id, remote_addr = %remote_addr)
)]
async fn handle_socket(
    state: AppState,
    session_id: Uuid,
    streams: Vec<String>,
    mut socket: WebSocket,
    remote_addr: SocketAddr,
    user_agent: Option<String>,
) {
    let mut rx = match state.broadcaster.subscribe(session_id).await {
        Ok(rx) => rx,
        Err(err) => {
            error!(%err, "failed to subscribe websocket broadcaster");
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: close_code::PROTOCOL,
                    reason: Cow::from("subscription failed"),
                })))
                .await;
            return;
        }
    };

    let filters: HashSet<String> = streams.into_iter().collect();
    let mut first_broadcast_logged = false;
    let mut close_code_value = close_code::NORMAL;
    let mut close_reason = String::from("client_disconnected");

    loop {
        match rx.recv().await {
            Ok(message) => {
                if !filters.contains(message.stream()) {
                    continue;
                }

                if !first_broadcast_logged {
                    info!(
                        session_id = %session_id,
                        stream = %message.stream(),
                        close_time = message.close_time(),
                        "delivered first kline broadcast to client"
                    );
                    first_broadcast_logged = true;
                }

                let payload = match message.to_json_string() {
                    Ok(json) => json,
                    Err(err) => {
                        error!(%err, "failed to serialize websocket payload");
                        continue;
                    }
                };

                if let Err(err) = socket.send(Message::Text(payload)).await {
                    error!(%err, "failed to write websocket frame");
                    close_code_value = close_code::ABNORMAL;
                    close_reason = format!("send error: {err}");
                    break;
                }
            }
            Err(RecvError::Lagged(skipped)) => {
                warn!(
                    session_id = %session_id,
                    skipped,
                    "websocket consumer lagged; dropping stale messages"
                );
            }
            Err(RecvError::Closed) => {
                close_code_value = close_code::NORMAL;
                close_reason = "session ended".to_string();
                let _ = socket
                    .send(Message::Close(Some(CloseFrame {
                        code: close_code::NORMAL,
                        reason: Cow::from("session ended"),
                    })))
                    .await;
                break;
            }
        }
    }

    info!(
        session_id = %session_id,
        remote_addr = %remote_addr,
        user_agent = user_agent.as_deref().unwrap_or(""),
        close_code = close_code_value,
        close_reason = %close_reason,
        "websocket connection closed"
    );
}
