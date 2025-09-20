use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use serde_json::json;
use tracing::{error, instrument};

use crate::{app::bootstrap::AppState, dto::ws::WsQuery};

pub fn router() -> Router {
    Router::new().route("/ws", get(ws_handler))
}

#[instrument(skip(state, ws, query))]
pub async fn ws_handler(
    Extension(state): Extension<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, query, socket))
}

#[instrument(skip(state, socket))]
async fn handle_socket(state: AppState, query: WsQuery, mut socket: WebSocket) {
    match state.broadcaster.subscribe(query.session_id).await {
        Ok(mut rx) => {
            // Relay broadcasted JSON strings to this client.
            while let Ok(message) = rx.recv().await {
                if socket.send(Message::Text(message)).await.is_err() {
                    break;
                }
            }
        }
        Err(err) => {
            error!(%err, "failed to subscribe websocket");
            // Always send JSON-shaped error payloads
            let payload = json!({
                "event": "error",
                "data": { "message": err.to_string() }
            })
            .to_string();
            let _ = socket.send(Message::Text(payload)).await;
        }
    }
}
