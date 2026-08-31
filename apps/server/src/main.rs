//! Betterboxd 后端：REST + WebSocket(Agent) + 静态托管。
//! M1：控制台真实对话、会话持久化（R5）、预算预检、熔断/打断。

use axum::{
    Json, Router,
    extract::{
        Path as AxPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use betterboxd_core::agent;
use betterboxd_core::config::Config;
use betterboxd_core::db::DbHandle;
use betterboxd_core::session::SessionStore;
use betterboxd_core::tmdb::TmdbClient;
use betterboxd_core::tools::{self, ToolCtx, ToolRegistry};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

type App = Arc<AppState>;

#[derive(Clone)]
struct AppState {
    db: DbHandle,
    sessions: Arc<SessionStore>,
    config: Arc<Mutex<Config>>,
    client: Option<betterboxd_core::llm::ChatClient>,
    tmdb: TmdbClient,
    profile_name: String,
    profile_model: String,
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRoute>>>,
    posters_dir: std::path::PathBuf,
}

/// 确认卡等待路由：call_id → 回传通道（+关联取消令牌）。
struct PendingRoute {
    tx: tokio::sync::oneshot::Sender<Result<Option<serde_json::Value>, String>>,
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

// ============ 确认门（WS 版）：写工具 ↔ 前端确认卡 ============

struct WsGate {
    tx: tokio::sync::mpsc::UnboundedSender<Message>,
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRoute>>>,
    cancel: CancellationToken,
}

impl betterboxd_core::tools::ConfirmGate for WsGate {
    fn request<'a>(
        &'a self,
        pending_req: &'a betterboxd_core::tools::PendingConfirm,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<serde_json::Value>, String>> {
        let call_id = uuid::Uuid::now_v7().to_string();
        let frame = serde_json::json!({
            "type": "confirm",
            "call_id": call_id,
            "name": pending_req.name,
            "args": pending_req.args,
        });
        let (otx, orx) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(call_id.clone(), PendingRoute { tx: otx });
        let _ = self.tx.send(Message::Text(frame.to_string().into()));

        Box::pin(async move {
            let result = tokio::select! {
                _ = self.cancel.cancelled() => {
                    Err("已打断，待确认操作已取消".to_string())
                }
                r = orx => {
                    match r {
                        Ok(Ok(Some(args))) => Ok(Some(args)),
                        Ok(Ok(None)) => Ok(None),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err("确认通道关闭".to_string()),
                    }
                }
            };
            self.pending.lock().unwrap().remove(&call_id);
            result
        })
    }
}

// ============ REST API（表单/前端直连路径；写操作不过确认门）============

fn tool_ctx(app: &App) -> ToolCtx {
    ToolCtx {
        db: app.db.clone(),
        tmdb: app.tmdb.clone(),
        config: (*app.config.lock().unwrap()).clone(),
        confirm: None, // REST = 用户明确意图
    }
}

async fn movies_list(
    State(app): State<App>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let filter = q.get("filter").map(String::as_str).unwrap_or("watched");
    let where_clause = match filter {
        "watched" => "WHERE watched=1",
        "watchlist" => "WHERE in_watchlist=1",
        _ => "",
    };
    let sort = q.get("sort").map(String::as_str).unwrap_or("recent");
    let order = match sort {
        "rating" => "my_rating DESC, updated_at DESC",
        "title" => "title_main ASC",
        _ => "updated_at DESC",
    };
    let sql = format!(
        "SELECT tmdb_id, title_main, title_sub, year, my_rating, liked, in_watchlist,
                watched, posters FROM v_movies {where_clause} ORDER BY {order} LIMIT 500"
    );
    match app.db.select_json(&sql).await {
        Ok(rows) => Json(json!({"movies": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn movie_detail(State(app): State<App>, AxPath(id): AxPath<i64>) -> Response {
    // 懒拉取：条目桩首次被访问时补全详情（directors/runtime 等）
    let _ = betterboxd_core::tools::ensure_movie_details(&app.tmdb, &app.db, id).await;
    let movie = app
        .db
        .select_json(&format!("SELECT * FROM v_movies WHERE tmdb_id={id}"))
        .await;
    let logs = app
        .db
        .select_json(&format!(
            "SELECT kind, id, at, brief FROM v_logs WHERE movie_id={id} ORDER BY at DESC LIMIT 100"
        ))
        .await;
    let lists = app
        .db
        .select_json(&format!(
            "SELECT list_id, name, rank, ranked FROM v_lists WHERE movie_id={id}"
        ))
        .await;
    match (movie, logs, lists) {
        (Ok(mut m), Ok(logs), Ok(lists)) => {
            let movie_json = m
                .pop()
                .map(|mv| json!({"movie": mv, "logs": logs, "lists": lists}));
            match movie_json {
                Some(v) => Json(v).into_response(),
                None => (StatusCode::NOT_FOUND, "影片不存在").into_response(),
            }
        }
        (Err(e), ..) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "查询失败").into_response(),
    }
}

async fn movie_state(
    State(app): State<App>,
    AxPath(id): AxPath<i64>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let mut full = args.clone();
    full["movie_id"] = json!(id);
    match tools::execute("set_movie_state", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn diary_list(
    State(app): State<App>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .min(500);
    let offset = q
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let sql = format!(
        "SELECT entry_id, movie_id, title_main, title_sub, watched_date, rating, liked,
                in_theater, ticket_price_cents, private_note, rewatch_index, tags, dimensions_flat
         FROM v_diary_full ORDER BY watched_date DESC, created_at DESC LIMIT {limit} OFFSET {offset}"
    );
    match app.db.select_json(&sql).await {
        Ok(rows) => Json(json!({"entries": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn diary_add(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    match tools::execute("manage_diary", &tool_ctx(&app), args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn diary_update(
    State(app): State<App>,
    AxPath(id): AxPath<String>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let mut full = args.clone();
    full["action"] = json!("update");
    full["entry_id"] = json!(id);
    match tools::execute("manage_diary", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn diary_delete(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    match tools::execute(
        "manage_diary",
        &tool_ctx(&app),
        json!({"action": "delete", "entry_id": id}),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn reviews_list(State(app): State<App>) -> Response {
    match app
        .db
        .select_json(
            "SELECT review_id, movie_id, title, body_md, rating, liked, created_at,
                    title_zh, title_sub FROM v_reviews_full ORDER BY created_at DESC LIMIT 200",
        )
        .await
    {
        Ok(rows) => Json(json!({"reviews": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn review_add(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    match tools::execute("manage_reviews", &tool_ctx(&app), args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn review_update(
    State(app): State<App>,
    AxPath(id): AxPath<String>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let mut full = args.clone();
    full["action"] = json!("update");
    full["review_id"] = json!(id);
    match tools::execute("manage_reviews", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn review_delete(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    match tools::execute(
        "manage_reviews",
        &tool_ctx(&app),
        json!({"action": "delete", "review_id": id}),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn tmdb_search(
    State(app): State<App>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(query) = q.get("q") else {
        return (StatusCode::BAD_REQUEST, "缺少 q 参数").into_response();
    };
    let year = q.get("year").and_then(|s| s.parse::<i64>().ok());
    match tools::execute(
        "search_movies",
        &tool_ctx(&app),
        json!({"query": query, "year": year}),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}

/// 横向剧照（详情页 hero 背景）：w1280 档，同海报的下载-落盘-回源模式。
async fn backdrop(State(app): State<App>, AxPath(id): AxPath<i64>) -> Response {
    let file = app.posters_dir.join(format!("{id}_bg.jpg"));
    if let Ok(bytes) = std::fs::read(&file) {
        return ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], bytes).into_response();
    }
    let bp: Option<String> = app
        .db
        .select_json(&format!(
            "SELECT backdrop_path FROM v_movies WHERE tmdb_id={id}"
        ))
        .await
        .ok()
        .and_then(|rows| rows.first().cloned())
        .and_then(|r| r["backdrop_path"].as_str().map(String::from));
    let Some(bp) = bp.filter(|p| !p.is_empty()) else {
        return (StatusCode::NOT_FOUND, "无剧照").into_response();
    };
    match app
        .tmdb
        .download_image(&format!("https://image.tmdb.org/t/p/w1280{bp}"))
        .await
    {
        Ok(bytes) => {
            let _ = std::fs::write(&file, &bytes);
            ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], bytes).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}

async fn poster(State(app): State<App>, AxPath(id): AxPath<i64>) -> Response {
    let file = app.posters_dir.join(format!("{id}.jpg"));
    if let Ok(bytes) = std::fs::read(&file) {
        return ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], bytes).into_response();
    }
    let poster_path: Option<String> = app
        .db
        .select_json(&format!(
            "SELECT json_extract(posters, '$[0]') AS p FROM v_movies WHERE tmdb_id={id}"
        ))
        .await
        .ok()
        .and_then(|rows| rows.first().cloned())
        .and_then(|r| r["p"].as_str().map(String::from));
    let Some(pp) = poster_path.filter(|p| !p.is_empty()) else {
        return (StatusCode::NOT_FOUND, "无海报记录").into_response();
    };
    match app.tmdb.download_poster(&pp).await {
        Ok(bytes) => {
            let _ = std::fs::write(&file, &bytes);
            ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], bytes).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}

/// 用量汇总（侧栏徽标）。
async fn usage_summary(State(app): State<App>) -> Response {
    let fx = app.config.lock().unwrap().billing.fx_rates.clone();
    let cur = app.config.lock().unwrap().billing.display_currency.clone();
    let ms = agent::month_start_pub();
    let rows = app
        .db
        .select_json(
            "SELECT input_cost, output_cost, currency, at FROM usage_records WHERE currency IS NOT NULL",
        )
        .await;
    let (month, total, calls) = match rows {
        Ok(rows) => {
            let conv = |r: &serde_json::Value| -> f64 {
                let c = r["input_cost"].as_f64().unwrap_or(0.0)
                    + r["output_cost"].as_f64().unwrap_or(0.0);
                let currency = r["currency"].as_str().unwrap_or("");
                c * if currency == cur {
                    1.0
                } else {
                    fx.get(currency).copied().unwrap_or(0.0)
                }
            };
            let month = rows
                .iter()
                .filter(|r| r["at"].as_i64().unwrap_or(0) >= ms)
                .map(conv)
                .sum::<f64>();
            let total = rows.iter().map(conv).sum::<f64>();
            (month, total, rows.len())
        }
        Err(_) => (0.0, 0.0, 0),
    };
    Json(json!({"display_currency": cur, "month": month, "total": total, "calls": calls}))
        .into_response()
}

/// 直译管线：自然语言 → LLM 生成 SQL → 审查 → 执行 → 图表。
/// 仪表盘"新建查询"与控制台 `/stats` 共用此端点。
async fn stats_run(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let Some(nl) = args["nl"].as_str() else {
        return (StatusCode::BAD_REQUEST, "缺少 nl（自然语言需求）").into_response();
    };
    let client = match &app.client {
        Some(c) => c.clone(),
        None => return (StatusCode::BAD_REQUEST, "未配置模型档案").into_response(),
    };
    let schema = betterboxd_core::agent::SCHEMA_DICTIONARY;
    let prompt = format!(
        "你是统计查询生成器。根据用户的自然语言需求，生成一条 SQLite SELECT 语句。\n\
         只返回 JSON: {{\"sql\": \"...\", \"chart\": {{\"type\": \"bar|line|pie|table\", \"title\": \"...\"}}}}\n\
         {schema}\n\
         用户需求: {nl}"
    );
    let messages = vec![serde_json::json!({"role": "user", "content": prompt})];

    // 重试 ≤2 次
    let mut last_err = String::new();
    for _ in 0..3 {
        let token = CancellationToken::new();
        let outcome = client
            .chat_stream(&messages, None, None, &token, |_| {})
            .await;
        let Ok(outcome) = outcome else {
            last_err = "LLM 调用失败".into();
            continue;
        };
        // 从回复中提取 JSON（可能被 markdown 代码块包裹）
        let raw = outcome.text.trim();
        let json_str = raw
            .strip_prefix("```json").and_then(|s| s.strip_suffix("```"))
            .or_else(|| raw.strip_prefix("```").and_then(|s| s.strip_suffix("```")))
            .unwrap_or(raw)
            .trim();
        let parsed: Result<Value, _> = serde_json::from_str(json_str);
        let Ok(query) = parsed else {
            last_err = "LLM 回复不是有效 JSON".into();
            continue;
        };
        let sql = query["sql"].as_str().unwrap_or_default().to_string();
        let chart = query.get("chart").cloned().unwrap_or(json!({"type": "table"}));

        match betterboxd_core::stats_guard::review_sql(&sql) {
            Err(e) => { last_err = format!("SQL 审查未通过: {e}"); continue; }
            Ok(()) => {
                let sql_exec = if !sql.to_lowercase().contains("limit") {
                    format!("SELECT * FROM ({}) LIMIT 1000", sql.trim_end_matches(';'))
                } else {
                    sql.trim_end_matches(';').to_string()
                };
                match app.db.select_json(&sql_exec).await {
                    Ok(rows) => {
                        return Json(json!({
                            "ok": true, "sql": sql, "chart": chart,
                            "columns": rows.first().map(|r| {
                                r.as_object().unwrap().keys().cloned().collect::<Vec<_>>()
                            }).unwrap_or_default(),
                            "rows": rows, "truncated": rows.len() >= 1000,
                        })).into_response();
                    }
                    Err(e) => { last_err = format!("SQL 执行失败: {e}"); continue; }
                }
            }
        }
    }
    (StatusCode::BAD_REQUEST, format!("直译管线 3 次尝试均失败: {last_err}")).into_response()
}

async fn saved_queries_list(State(app): State<App>) -> Response {
    match app
        .db
        .select_json(
            "SELECT id, name, payload_json, sort_order, last_run_at FROM saved_queries ORDER BY sort_order, created_at",
        )
        .await
    {
        Ok(rows) => Json(json!({"queries": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn saved_query_create(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let Some(name) = args["name"].as_str().map(String::from) else {
        return (StatusCode::BAD_REQUEST, "缺少 name").into_response();
    };
    let payload = args["payload"].to_string();
    let id = uuid::Uuid::now_v7().to_string();
    let id_reply = id.clone();
    match app.db.call(move |c| {
        c.execute(
            "INSERT INTO saved_queries (id, name, payload_json, sort_order, created_at, last_run_at)
             VALUES (?1,?2,?3,?4,?5,?5)",
            rusqlite::params![id, name, payload, 0, betterboxd_core::now()],
        )
    })
    .await
    {
        Ok(_) => Json(json!({"ok": true, "id": id_reply})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn saved_query_delete(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    match app.db.call(move |c| {
        c.execute("DELETE FROM saved_queries WHERE id=?1", rusqlite::params![id])
    })
    .await
    {
        Ok(n) if n > 0 => Json(json!({"ok": true})).into_response(),
        _ => (StatusCode::NOT_FOUND, "未找到").into_response(),
    }
}

async fn chats_list(State(app): State<App>) -> Response {
    match app
        .db
        .select_json(
            "SELECT s.id, s.scope, s.movie_id, s.title, s.created_at, s.last_message_at,
                    m.title_main AS movie_title, m.tmdb_id
             FROM chat_sessions s LEFT JOIN movies m ON m.tmdb_id = s.movie_id
             ORDER BY s.last_message_at DESC LIMIT 200",
        )
        .await
    {
        Ok(rows) => Json(json!({"chats": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 打断后的确认卡直连执行（design.md 打断三段语义③）。
async fn tools_execute(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let Some(name) = args["name"].as_str().map(String::from) else {
        return (StatusCode::BAD_REQUEST, "缺少 name").into_response();
    };
    if !betterboxd_core::tools::CONFIRM_TOOLS.contains(&name.as_str()) {
        return (StatusCode::FORBIDDEN, "该工具不允许直连执行").into_response();
    }
    match tools::execute(&name, &tool_ctx(&app), args["args"].clone()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

#[derive(Deserialize)]
struct WsChatQuery {
    movie_id: Option<i64>,
}

async fn ws_chat(
    State(app): State<App>,
    Query(q): Query<WsChatQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let movie_id = q.movie_id;
    ws.on_upgrade(move |socket| handle_chat(app, socket, movie_id))
}

async fn handle_chat(app: App, socket: WebSocket, ws_movie_id: Option<i64>) {
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

    let mut current = match ws_movie_id {
        Some(mid) => app.sessions.new_session("movie", Some(mid), None, None),
        None => app.sessions.new_session("global", None, None, None),
    };
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
        if frame["type"].as_str() == Some("confirm") {
            route_confirm(&app, &frame);
            continue;
        }
        if frame["type"].as_str() != Some("user") {
            continue;
        }
        let Some(user_text) = frame["text"].as_str().map(String::from) else {
            continue;
        };
        let token = CancellationToken::new();
        *cancel.lock().unwrap() = Some(token.clone());

        // 构建 movie 上下文注入（scope=movie 时）
    let context_injection = if let Some(mid) = ws_movie_id {
        let mut ctx_parts = Vec::new();
        if let Ok(rows) = app.db.select_json(&format!(
            "SELECT title_main, title_sub, year, directors, genres, runtime,
                    my_rating, liked, in_watchlist, overview
             FROM v_movies WHERE tmdb_id={mid}"
        )).await {
            if let Some(m) = rows.first() {
                ctx_parts.push(format!("影片: {} ({})", m["title_main"].as_str().unwrap_or("?"),
                    m["year"].as_str().unwrap_or("")));
                if let Some(d) = m["directors"].as_str() { ctx_parts.push(format!("导演: {}", d)); }
                if let Some(r) = m["my_rating"].as_i64() { ctx_parts.push(format!("我的最终评分: {r}")); }
                if m["in_watchlist"].as_i64() == Some(1) { ctx_parts.push("在想看清单".into()); }
            }
        }
        if let Ok(logs) = app.db.select_json(&format!(
            "SELECT kind, at, brief FROM v_logs WHERE movie_id={mid} ORDER BY at DESC LIMIT 10"
        )).await {
            let log_strs: Vec<String> = logs.iter()
                .map(|l| format!("{} {}: {}", l["kind"].as_str().unwrap_or(""), l["at"].as_str().unwrap_or(""), l["brief"].as_str().unwrap_or("")))
                .collect();
            if !log_strs.is_empty() { ctx_parts.push(format!("最近记录:\n{}", log_strs.join("\n"))); }
        }
        ctx_parts.join("\n")
    } else { String::new() };

    let Some(client) = app.client.clone() else {
            let _ = tx.send(Message::Text(
                serde_json::json!({"type": "error",
                    "message": "未配置模型档案，请先在设置页或 config.toml 配置"})
                .to_string()
                .into(),
            ));
            continue;
        };
        let tmdb = app.tmdb.clone();
        let cfg_snapshot = app.config.lock().unwrap().clone();
        let mut task_session = current.clone();

        // —— 运行态：Agent 在独立任务中跑，主循环只盯 interrupt/断连 ——
        let tx_task = tx.clone();
        let sessions = app.sessions.clone();
        let db = app.db.clone();
        let pending = app.pending.clone();
        let pname = app.profile_name.clone();
        let pmodel = app.profile_model.clone();
        let mut run_handle = tokio::spawn(async move {
            let ctx = ToolCtx {
                db: db.clone(),
                tmdb,
                config: cfg_snapshot.clone(),
                confirm: Some(std::sync::Arc::new(WsGate {
                    tx: tx_task.clone(),
                    pending,
                    cancel: token.clone(),
                })),
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
                &context_injection,
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
                    if let Ok(done_session) = res {
                        current = done_session;
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
                                    Some("confirm") => route_confirm(&app, &f),
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


// ============ 配置端点（R3）============

/// 配置读取（api_key 打码）。
async fn config_get(State(app): State<App>) -> Response {
    let cfg = app.config.lock().unwrap().clone();
    let mut v = serde_json::to_value(&cfg).unwrap_or(json!({}));
    if let Some(profiles) = v.get_mut("profiles").and_then(|p| p.as_array_mut()) {
        for p in profiles {
            if let Some(key) = p.get_mut("api_key").and_then(|k| k.as_str()) {
                let masked = if key.len() > 8 {
                    format!("{}...{}", &key[..4], &key[key.len()-4..])
                } else { "****".into() };
                p["api_key"] = json!(masked);
            }
        }
    }
    Json(v).into_response()
}

/// 配置写入。
async fn config_save(State(app): State<App>, Json(mut new_cfg): Json<serde_json::Value>) -> Response {
    let mut cfg = app.config.lock().unwrap().clone();
    if let Some(new_profiles) = new_cfg.get_mut("profiles").and_then(|p| p.as_array_mut()) {
        for (i, np) in new_profiles.iter_mut().enumerate() {
            if let Some(key_str) = np.get("api_key").and_then(|k| k.as_str()) {
                if key_str.contains("...") {
                    if let Some(old) = cfg.profiles.get(i) {
                        np["api_key"] = serde_json::json!(old.api_key.clone());
                    }
                }
            }
        }
    }
    let parsed: Result<Config, _> = serde_json::from_value(new_cfg);
    match parsed {
        Ok(c) => {
            let path = data_dir_path().join(".betterboxd/config.toml");
            if c.save(&path).is_err() {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "配置保存失败").into_response();
            }
            *app.config.lock().unwrap() = c;
            axum::Json(json!({"ok": true})).into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, format!("配置解析失败: {e}")).into_response(),
    }
}

fn data_dir_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("BB_DATA").unwrap_or_else(|_| "data".into()))
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
    let client = Some(betterboxd_core::llm::ChatClient::new(
        &profile.endpoint,
        &profile.api_key,
        &profile.model,
    ));
    let tmdb = TmdbClient::new(
        config.tmdb.key.clone(),
        config.tmdb.proxy.clone(),
        config.tmdb.language.clone(),
    );
    let (pname, pmodel) = (profile.name.clone(), profile.model.clone());
    let posters_dir = lib_dir.join("posters");
    std::fs::create_dir_all(&posters_dir).ok();
    let app = Arc::new(AppState {
        db,
        sessions,
        config: Arc::new(Mutex::new(config)),
        client,
        tmdb,
        profile_name: pname,
        profile_model: pmodel,
        pending: Arc::new(Mutex::new(std::collections::HashMap::new())),
        posters_dir,
    });

    let router = Router::new()
        .route("/api/health", get(health))
        .route("/api/echo", post(echo_rest))
        .route("/ws/chat", get(ws_chat))
        .route("/ws/echo", get(ws_echo))
        .route("/api/movies", get(movies_list))
        .route("/api/movie/{id}", get(movie_detail))
        .route("/api/movie/{id}/state", post(movie_state))
        .route("/api/diary", get(diary_list).post(diary_add))
        .route("/api/diary/{id}", put(diary_update).delete(diary_delete))
        .route("/api/reviews", get(reviews_list).post(review_add))
        .route(
            "/api/reviews/{id}",
            put(review_update).delete(review_delete),
        )
        .route("/api/tmdb/search", get(tmdb_search))
        .route("/api/poster/{id}", get(poster))
        .route("/api/backdrop/{id}", get(backdrop))
        .route("/api/usage/summary", get(usage_summary))
        .route("/api/tools/execute", post(tools_execute))
        .route("/api/stats/run", post(stats_run))
        .route("/api/saved-queries", get(saved_queries_list).post(saved_query_create))
        .route("/api/saved-queries/{id}", delete(saved_query_delete))
                .route("/api/chats", get(chats_list))
        .route("/api/config", get(config_get).post(config_save))
        .fallback_service(tower_http::services::ServeDir::new("apps/web/dist"))
        .with_state(app);

    let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
    println!("Betterboxd server: http://localhost:3000");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}

/// 把前端确认结果路由到等待中的确认门。
fn route_confirm(app: &App, frame: &serde_json::Value) {
    let Some(call_id) = frame["call_id"].as_str() else {
        return;
    };
    let decision = frame["decision"].as_str().unwrap_or("reject");
    let pending = app.pending.lock().unwrap().remove(call_id);
    if let Some(route) = pending {
        let payload = if decision == "confirm" {
            Ok(Some(frame["args"].clone()))
        } else {
            Ok(None)
        };
        let _ = route.tx.send(payload);
    }
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
