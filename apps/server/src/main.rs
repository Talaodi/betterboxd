//! Betterboxd 后端：REST + WebSocket(Agent) + 静态托管。
//! M1：控制台真实对话、会话持久化（R5）、预算预检、熔断/打断。

use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::{get, post},
};
use betterboxd_core::agent;
use betterboxd_core::config::Config;
use betterboxd_core::db::DbHandle;
use betterboxd_core::session::SessionStore;
use betterboxd_core::tmdb::TmdbClient;
use betterboxd_core::tools::{self, ToolCtx, ToolRegistry};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

type App = Arc<AppState>;

#[derive(Clone)]
struct AppState {
    db: DbHandle,
    sessions: Arc<SessionStore>,
    config: Arc<Mutex<Config>>,
    client: betterboxd_core::llm::ChatClient,
    tmdb: TmdbClient,
    profile_name: String,
    profile_model: String,
}

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
    Json(EchoOut {
        echo: format!("回显: {}", input.message),
    })
}

/// 从 spikes/local.env 引导生成库配置（仅开发期一次性）。
fn ensure_config(lib_dir: &std::path::Path) -> Config {
    let cfg_path = lib_dir.join("config.toml");
    if cfg_path.exists() {
        return Config::load(&cfg_path).expect("config.toml 解析失败");
    }
    let mut kv = std::collections::HashMap::new();
    let env_file = std::path::Path::new("spikes/local.env");
    if let Ok(raw) = std::fs::read_to_string(env_file) {
        for line in raw.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let mut v = v.trim().to_string();
                // 剥离 shell 风格成对引号（source 语义一致化）
                if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
                    || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
                {
                    v = v[1..v.len() - 1].to_string();
                }
                if !v.is_empty() {
                    kv.insert(k.trim().to_string(), v);
                }
            }
        }
    }
    let cfg = Config {
        profiles: vec![betterboxd_core::config::Profile {
            name: "课程平台".into(),
            endpoint: kv.get("COURSE_ENDPOINT").cloned().unwrap_or_default(),
            api_key: kv.get("COURSE_KEY").cloned().unwrap_or_default(),
            model: kv.get("COURSE_MODEL").cloned().unwrap_or_default(),
            context_length: 8192,
            thinking_mode: "off".into(),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            extra_body_json: None,
        }],
        active_profile: Some("课程平台".into()),
        tmdb: betterboxd_core::config::TmdbCfg {
            key: kv.get("TMDB_KEY").cloned().unwrap_or_default(),
            proxy: None,
            language: "zh-CN".into(),
        },
        billing: Default::default(),
        display: Default::default(),
    };
    cfg.save(&cfg_path).expect("写入 config.toml 失败");
    println!("已从 spikes/local.env 生成 {}", cfg_path.display());
    cfg
}

async fn ws_chat(State(app): State<App>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_chat(app, socket))
}

async fn handle_chat(app: App, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // 写任务：统一出口，避免 sink 双向借用
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(frame).await.is_err() {
                break;
            }
        }
    });

    let mut current = app.sessions.new_session("global", None, None, None);
    let hello = serde_json::json!({"type": "hello", "session_id": current.id});
    let _ = tx.send(Message::Text(hello.to_string().into()));

    let cancel: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));

    'outer: loop {
        // —— 空闲态：等待用户消息 ——
        let Some(Ok(Message::Text(text))) = stream.next().await else {
            break 'outer;
        };
        let Ok(frame) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if frame["type"].as_str() != Some("user") {
            continue; // 空闲期 interrupt 无对象可打断
        }
        let Some(user_text) = frame["text"].as_str().map(String::from) else {
            continue;
        };
        let token = CancellationToken::new();
        *cancel.lock().unwrap() = Some(token.clone());

        let profile = match app.config.lock().unwrap().active().cloned() {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(Message::Text(
                    serde_json::json!({"type": "error", "message": e})
                        .to_string()
                        .into(),
                ));
                continue;
            }
        };
        let client = app.client.clone();
        let tmdb = app.tmdb.clone();
        let cfg_snapshot = app.config.lock().unwrap().clone();
        let mut task_session = current.clone();

        // —— 运行态：Agent 在独立任务中跑，主循环只盯 interrupt/断连 ——
        let tx_task = tx.clone();
        let sessions = app.sessions.clone();
        let db = app.db.clone();
        let pname = app.profile_name.clone();
        let pmodel = app.profile_model.clone();
        let mut run_handle = tokio::spawn(async move {
            let ctx = ToolCtx {
                db: db.clone(),
                tmdb,
                config: cfg_snapshot.clone(),
            };
            let tx_ev = tx_task.clone();
            let on_event = move |ev: agent::AgentEvent| {
                let f = match ev.kind {
                    agent::AgentEventKind::Token(t) => {
                        serde_json::json!({"type": "token", "data": t})
                    }
                    agent::AgentEventKind::ToolStart { name, args } => {
                        serde_json::json!({"type": "tool", "name": name, "args": args})
                    }
                    agent::AgentEventKind::ToolDone { name, ok } => {
                        serde_json::json!({"type": "tool_done", "name": name, "ok": ok})
                    }
                };
                let _ = tx_ev.send(Message::Text(f.to_string().into()));
            };
            let run = agent::run(
                &client,
                &cfg_snapshot,
                &db,
                &ToolRegistry {
                    metas: tools::registry().metas,
                },
                &ctx,
                &mut task_session.messages,
                &user_text,
                token,
                on_event,
            )
            .await;

            if task_session.title.is_empty() && !user_text.is_empty() {
                task_session.title = user_text.chars().take(30).collect();
            }
            let _ = sessions.save(&task_session).await;

            match run {
                Ok(summary) => {
                    let (pid, pt, ct) = (
                        task_session.id.clone(),
                        summary.usage_tokens.0 as i64,
                        summary.usage_tokens.1 as i64,
                    );
                    let _ = db
                        .call(move |c| {
                            c.execute(
                                "INSERT INTO usage_records (id, session_id, profile_name,
                               model, prompt_tokens, completion_tokens, at, kind)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,'llm')",
                                rusqlite::params![
                                    uuid::Uuid::now_v7().to_string(),
                                    pid,
                                    pname,
                                    pmodel,
                                    pt,
                                    ct,
                                    betterboxd_core::now()
                                ],
                            )
                        })
                        .await;
                    let tx_done = tx_task.clone();
                    let done = serde_json::json!({
                        "type": "done",
                        "interrupted": summary.interrupted,
                        "steps": summary.steps,
                        "tokens": {"prompt": summary.usage_tokens.0,
                                    "completion": summary.usage_tokens.1},
                        "aborted": summary.aborted_reason,
                    });
                    let _ = tx_done.send(Message::Text(done.to_string().into()));
                }
                Err(e) => {
                    let _ = tx_task.send(Message::Text(
                        serde_json::json!({"type": "error", "message": e})
                            .to_string()
                            .into(),
                    ));
                }
            }
            task_session
        });

        loop {
            tokio::select! {
                res = &mut run_handle => {
                    match res {
                        Ok(done_session) => current = done_session,
                        Err(_) => {}
                    }
                    break; // 回到空闲态
                }
                m = stream.next() => {
                    match m {
                        Some(Ok(Message::Text(t))) => {
                            if let Ok(f) = serde_json::from_str::<serde_json::Value>(&t) {
                                match f["type"].as_str() {
                                    Some("interrupt") => {
                                        if let Some(c) = cancel.lock().unwrap().as_ref() { c.cancel(); }
                                    }
                                    Some("user") => {
                                        let _ = tx.send(Message::Text(
                                            serde_json::json!({"type": "error",
                                                "message": "当前有任务在执行，请先停止"})
                                                .to_string().into()));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {
                            // 连接断开：打断任务后收尾
                            if let Some(c) = cancel.lock().unwrap().as_ref() { c.cancel(); }
                            let _ = (&mut run_handle).await;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    drop(tx);
}

#[tokio::main]
async fn main() {
    let data_dir =
        std::path::PathBuf::from(std::env::var("BB_DATA").unwrap_or_else(|_| "data".into()));
    let lib_dir = data_dir.join(".betterboxd");
    std::fs::create_dir_all(lib_dir.join("sessions")).expect("创建数据目录失败");
    let config = ensure_config(&lib_dir);
    let conn = betterboxd_core::Db::open(&lib_dir.join("data.db"))
        .expect("打开数据库失败")
        .into_inner();
    let db = DbHandle::spawn(conn);
    let sessions = Arc::new(SessionStore::new(&lib_dir, db.clone()));
    // 客户端启动时构建一次（连接池复用）；档案切换（M2）时重建
    let profile = config.active().expect("缺少活动模型档案").clone();
    let client =
        betterboxd_core::llm::ChatClient::new(&profile.endpoint, &profile.api_key, &profile.model);
    let tmdb = TmdbClient::new(
        config.tmdb.key.clone(),
        config.tmdb.proxy.clone(),
        config.tmdb.language.clone(),
    );
    let (pname, pmodel) = (profile.name.clone(), profile.model.clone());
    let app = Arc::new(AppState {
        db,
        sessions,
        config: Arc::new(Mutex::new(config)),
        client,
        tmdb,
        profile_name: pname,
        profile_model: pmodel,
    });

    let router = Router::new()
        .route("/api/health", get(health))
        .route("/api/echo", post(echo_rest))
        .route("/ws/chat", get(ws_chat))
        .route("/ws/echo", get(ws_echo))
        .fallback_service(tower_http::services::ServeDir::new(
            data_dir.join("../apps/web/dist"),
        ))
        .with_state(app);

    let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
    println!("Betterboxd server [build b2]: http://localhost:3000");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}

async fn ws_echo(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.recv().await {
            let frame = serde_json::json!({"type": "token", "data": format!("回显: {text}")});
            if socket
                .send(Message::Text(frame.to_string().into()))
                .await
                .is_err()
            {
                return;
            }
            let done = serde_json::json!({"type": "done"});
            if socket
                .send(Message::Text(done.to_string().into()))
                .await
                .is_err()
            {
                return;
            }
        }
    })
}
