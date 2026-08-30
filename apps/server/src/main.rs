//! Betterboxd 后端：静态资源 + REST API + WebSocket 流式通道。
//!
//! M0 验收端点（假流式回显，M1 换成真实 Agent Loop，WS 协议不变）：
//! - `GET  /api/health`  健康检查
//! - `POST /api/echo`    `{message}` → `{echo}`（REST 冒烟）
//! - `GET  /ws/echo`     WebSocket：收一条文本 → 逐块推
//!   `{"type":"token","data":"…"}` × N → `{"type":"done"}`

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize)]
struct EchoIn {
    message: String,
}

#[derive(Serialize)]
struct EchoOut {
    echo: String,
}

async fn health() -> &'static str {
    "ok"
}

async fn echo_rest(Json(input): Json<EchoIn>) -> impl IntoResponse {
    Json(EchoOut { echo: format!("回显: {}", input.message) })
}

/// REST 冒烟路径（非流式）；真实统计/CRUD API 将沿用此形态。
async fn echo_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(Ok(Message::Text(text))) = socket.recv().await {
        // 逐块推送，模拟流式（60ms 间隔）
        let chars: Vec<char> = text.chars().collect();
        for chunk in chars.chunks(2) {
            let s: String = chunk.iter().collect();
            let frame = serde_json::json!({"type": "token", "data": format!("回显: {s}")});
            if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
        let done = serde_json::json!({"type": "done"});
        if socket.send(Message::Text(done.to_string().into())).await.is_err() {
            return;
        }
    }
}

#[derive(Clone)]
struct AppState {
    web_dist: String,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        web_dist: std::env::var("WEB_DIST")
            .unwrap_or_else(|_| "apps/web/dist".to_string()),
    };

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/echo", post(echo_rest))
        .route("/ws/echo", get(echo_ws))
        .fallback_service(tower_http::services::ServeDir::new(&state.web_dist))
        .with_state(state);

    let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Betterboxd server listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
