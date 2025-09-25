use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    time::Duration,
};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, Query,
    },
    response::Response,
    routing::get,
    Extension, Router,
};
use serde_json::{json, Value};
use tokio::{
    select,
    sync::broadcast::error::RecvError,
    time::{interval, MissedTickBehavior},
};
use tracing::{debug, error, info, instrument, warn, Span};
use uuid::Uuid;

use crate::{app::bootstrap::AppState, error::AppError};

const CLOSE_MISSING_SESSION: &str = "missing sessionId query param";
const CLOSE_INVALID_SESSION: &str = "invalid sessionId";
const CLOSE_SESSION_DISABLED: &str = "session disabled";
const CLOSE_SESSION_UNAVAILABLE: &str = "session not available";
const CLOSE_INVALID_STREAM: &str = "invalid stream name";
const CLOSE_NO_STREAMS: &str = "no streams requested";
const CLOSE_SUBSCRIPTION_FAILED: &str = "subscription failed";

#[derive(Clone, Copy, Debug)]
enum StreamMode {
    Single,
    Combined,
}

pub fn router() -> Router {
    Router::new()
        .route("/ws/:stream", get(ws_single_handler))
        .route("/stream", get(ws_combined_handler))
}

#[instrument(
    skip(state, ws, params),
    fields(session_id = tracing::field::Empty, stream = %stream)
)]
pub async fn ws_single_handler(
    Extension(state): Extension<AppState>,
    Path(stream): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    let session_id = match extract_session_id(&params) {
        Ok(id) => id,
        Err(reason) => return close_upgrade(ws, reason),
    };

    let session_id_display = tracing::field::display(session_id);
    Span::current().record("session_id", &session_id_display);

    let session = match validate_session(&state, session_id).await {
        Ok(session) => session,
        Err(SessionValidation::Missing) => {
            return close_upgrade(ws, CLOSE_SESSION_UNAVAILABLE);
        }
        Err(SessionValidation::Disabled) => {
            return close_upgrade(ws, CLOSE_SESSION_DISABLED);
        }
    };

    let canonical_stream = match parse_stream_name(&stream) {
        Ok(stream) => stream,
        Err(err) => {
            warn!(error = %err, "invalid stream in /ws route");
            return close_upgrade(ws, CLOSE_INVALID_STREAM);
        }
    };

    ws.on_upgrade(move |socket| {
        let allowed = vec![canonical_stream];
        handle_socket(
            state,
            session.session_id,
            allowed,
            StreamMode::Single,
            socket,
        )
    })
}

#[instrument(
    skip(state, ws, params),
    fields(session_id = tracing::field::Empty, streams = tracing::field::Empty)
)]
pub async fn ws_combined_handler(
    Extension(state): Extension<AppState>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    let session_id = match extract_session_id(&params) {
        Ok(id) => id,
        Err(reason) => return close_upgrade(ws, reason),
    };

    let session_id_display = tracing::field::display(session_id);
    Span::current().record("session_id", &session_id_display);

    let streams_param = params.get("streams").cloned();
    let Some(streams_raw) = streams_param else {
        return close_upgrade(ws, CLOSE_NO_STREAMS);
    };

    let streams = match parse_streams(&streams_raw) {
        Ok(streams) if !streams.is_empty() => streams,
        Ok(_) => {
            return close_upgrade(ws, CLOSE_NO_STREAMS);
        }
        Err(err) => {
            warn!(error = %err, "invalid streams in /stream route");
            return close_upgrade(ws, CLOSE_INVALID_STREAM);
        }
    };

    let span_streams = streams.join("/");
    Span::current().record("streams", &tracing::field::display(&span_streams));

    let session = match validate_session(&state, session_id).await {
        Ok(session) => session,
        Err(SessionValidation::Missing) => {
            return close_upgrade(ws, CLOSE_SESSION_UNAVAILABLE);
        }
        Err(SessionValidation::Disabled) => {
            return close_upgrade(ws, CLOSE_SESSION_DISABLED);
        }
    };

    ws.on_upgrade(move |socket| {
        handle_socket(
            state,
            session.session_id,
            streams,
            StreamMode::Combined,
            socket,
        )
    })
}

async fn validate_session(
    state: &AppState,
    session_id: Uuid,
) -> Result<crate::domain::models::SessionConfig, SessionValidation> {
    match state.sessions_service.get_session(session_id).await {
        Ok(session) => {
            if session.enabled {
                Ok(session)
            } else {
                info!(%session_id, "ws session disabled");
                Err(SessionValidation::Disabled)
            }
        }
        Err(AppError::NotFound(_)) => {
            warn!(%session_id, "ws session not found");
            Err(SessionValidation::Missing)
        }
        Err(err) => {
            error!(error = %err, %session_id, "ws session validation failed");
            Err(SessionValidation::Missing)
        }
    }
}

enum SessionValidation {
    Missing,
    Disabled,
}

fn extract_session_id(params: &HashMap<String, String>) -> Result<Uuid, &'static str> {
    let Some(raw) = params.get("sessionId") else {
        return Err(CLOSE_MISSING_SESSION);
    };

    match Uuid::parse_str(raw) {
        Ok(uuid) => Ok(uuid),
        Err(_) => Err(CLOSE_INVALID_SESSION),
    }
}

fn parse_stream_name(stream: &str) -> Result<String, String> {
    let trimmed = stream.trim();
    let (symbol_part, rest) = trimmed
        .split_once('@')
        .ok_or_else(|| format!("invalid stream format: {trimmed}"))?;

    let rest_lower = rest.to_lowercase();
    let Some(interval_part) = rest_lower.strip_prefix("kline_") else {
        return Err(format!("invalid stream format: {trimmed}"));
    };

    if interval_part.is_empty() {
        return Err(format!("invalid interval in stream: {trimmed}"));
    }

    let symbol_lower = symbol_part.to_lowercase();
    if symbol_lower.is_empty() {
        return Err(format!("invalid symbol in stream: {trimmed}"));
    }

    Ok(format!("{}@kline_{}", symbol_lower, interval_part))
}

fn parse_streams(raw: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for part in raw.split('/') {
        if part.is_empty() {
            return Err("empty stream entry".to_string());
        }

        let stream = parse_stream_name(part)?;
        if seen.insert(stream.clone()) {
            out.push(stream);
        }
    }

    Ok(out)
}

#[derive(Debug)]
struct BinanceEvent {
    stream: String,
    data: Value,
}

fn convert_to_binance_event(raw: &str) -> Option<BinanceEvent> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let event_type = value.get("event")?.as_str()?;
    if event_type != "kline" {
        return None;
    }

    let data = value.get("data")?;
    let symbol = data.get("symbol")?.as_str()?;
    let interval = data.get("interval")?.as_str()?;
    let open_time = data.get("openTime")?.as_i64()?;
    let close_time = data.get("closeTime")?.as_i64()?;
    let open = data.get("open")?.as_f64()?;
    let high = data.get("high")?.as_f64()?;
    let low = data.get("low")?.as_f64()?;
    let close = data.get("close")?.as_f64()?;
    let volume = data.get("volume")?.as_f64()?;

    let symbol_upper = symbol.to_uppercase();
    let symbol_lower = symbol.to_lowercase();
    let interval_lower = interval.to_lowercase();

    let quote_volume = volume * close;

    let payload = json!({
        "e": "kline",
        "E": close_time,
        "s": symbol_upper,
        "k": {
            "t": open_time,
            "T": close_time,
            "s": symbol_upper,
            "i": interval,
            "f": 0,
            "L": 0,
            "o": format_number(open),
            "c": format_number(close),
            "h": format_number(high),
            "l": format_number(low),
            "v": format_number(volume),
            "n": 0,
            "x": true,
            "q": format_number(quote_volume),
            "V": format_number(0.0),
            "Q": format_number(0.0),
            "B": format_number(0.0),
        }
    });

    Some(BinanceEvent {
        stream: format!("{}@kline_{}", symbol_lower, interval_lower),
        data: payload,
    })
}

fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let mut s = format!("{value:.15}");
    if let Some(pos) = s.find('.') {
        let mut end = s.len();
        while end > pos + 1 && s.as_bytes()[end - 1] == b'0' {
            end -= 1;
        }
        if end > pos && s.as_bytes()[end - 1] == b'.' {
            end -= 1;
        }
        s.truncate(end);
    }

    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

#[instrument(
    skip(state, socket),
    fields(
        session_id = %session_id,
        mode = ?mode,
        streams = %streams.join(","),
    )
)]
async fn handle_socket(
    state: AppState,
    session_id: Uuid,
    streams: Vec<String>,
    mode: StreamMode,
    mut socket: WebSocket,
) {
    let allowed: HashSet<String> = streams.iter().cloned().collect();

    let mut rx = match state.broadcaster.subscribe(session_id).await {
        Ok(rx) => rx,
        Err(err) => {
            error!(error = %err, %session_id, "failed to subscribe websocket");
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 1011,
                    reason: Cow::from(CLOSE_SUBSCRIPTION_FAILED),
                })))
                .await;
            return;
        }
    };

    debug!(%session_id, "binance-compatible websocket connected");

    let mut ping_interval = interval(Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let _ = ping_interval.tick().await;

    loop {
        select! {
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(err) = socket.send(Message::Pong(payload)).await {
                            warn!(error = %err, %session_id, "failed to reply pong");
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        debug!(%session_id, "received pong from client");
                    }
                    Some(Ok(Message::Close(frame))) => {
                        debug!(%session_id, "client closed websocket");
                        let _ = socket.send(Message::Close(frame)).await;
                        break;
                    }
                    Some(Err(err)) => {
                        warn!(error = %err, %session_id, "websocket receive error");
                        break;
                    }
                    None => {
                        debug!(%session_id, "websocket stream ended");
                        break;
                    }
                }
            }
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(payload) => {
                        if let Some(event) = convert_to_binance_event(&payload) {
                            if allowed.contains(&event.stream) {
                                let BinanceEvent { stream, data } = event;
                                let message = match mode {
                                    StreamMode::Single => serde_json::to_string(&data),
                                    StreamMode::Combined => serde_json::to_string(&json!({
                                        "stream": stream,
                                        "data": data,
                                    })),
                                };

                                match message {
                                    Ok(text) => {
                                        if let Err(err) = socket.send(Message::Text(text)).await {
                                            debug!(error = %err, %session_id, "failed to send websocket message");
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        error!(error = %err, %session_id, "failed to serialize event");
                                    }
                                }
                            }
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        warn!(%session_id, skipped = skipped, "broadcast lagged; dropping to latest and continuing");
                        continue;
                    }
                    Err(RecvError::Closed) => {
                        debug!(%session_id, "broadcast channel closed, terminating websocket");
                        let reason = match state.sessions_service.get_session(session_id).await {
                            Ok(session) if !session.enabled => Cow::from(CLOSE_SESSION_DISABLED),
                            Ok(_) => Cow::from("session closed"),
                            Err(AppError::NotFound(_)) => Cow::from("session deleted"),
                            Err(_) => Cow::from("session closed"),
                        };
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: 1000,
                                reason,
                            })))
                            .await;
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                if let Err(err) = socket.send(Message::Ping(Vec::new())).await {
                    debug!(error = %err, %session_id, "failed to send ping");
                    break;
                }
            }
        }
    }

    debug!(%session_id, "binance-compatible websocket disconnected");
}

fn close_upgrade(ws: WebSocketUpgrade, reason: &str) -> Response {
    let message = reason.to_string();
    ws.on_upgrade(move |mut socket| async move {
        let reason = Cow::Owned(message);
        let _ = socket
            .send(Message::Close(Some(CloseFrame { code: 1008, reason })))
            .await;
    })
}
