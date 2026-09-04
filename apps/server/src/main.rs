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
    /// 独立只读统计连接（评审缺陷 5）
    stats_db: DbHandle,
    sessions: Arc<SessionStore>,
    config: Arc<Mutex<Config>>,
    /// 档案切换后原子重建（评审缺陷 3）：config_save 写入，读侧每轮快照
    client: Arc<std::sync::RwLock<Option<betterboxd_core::llm::ChatClient>>>,
    tmdb: Arc<std::sync::RwLock<TmdbClient>>,
    profile_name: Arc<Mutex<String>>,
    profile_model: Arc<Mutex<String>>,
    pending: Arc<Mutex<std::collections::HashMap<String, PendingRoute>>>,
    posters_dir: std::path::PathBuf,
    /// 当前数据目录（存档界面显示）
    data_dir: std::path::PathBuf,
}

/// 确认卡等待路由：call_id → 回传通道（+原始请求参数，供前端缺参时回退）。
struct PendingRoute {
    tx: tokio::sync::oneshot::Sender<Result<Option<serde_json::Value>, String>>,
    args: serde_json::Value,
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
            pricing: betterboxd_core::config::Pricing::default(),
            currency: "CNY".into(),
            budget: None,
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
        self.pending.lock().unwrap().insert(
            call_id.clone(),
            PendingRoute { tx: otx, args: pending_req.args.clone() },
        );
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
        stats_db: Some(app.stats_db.clone()),
        tmdb: app.tmdb.read().unwrap().clone(),
        config: (*app.config.lock().unwrap()).clone(),
        confirm: None, // REST = 用户明确意图
        source: "edit", // GUI/REST = 用户直接操作（审计区分 AI 写入）
        sessions: Some(app.sessions.clone()),
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
    // guard 必须绑定为局部变量跨 await（临时 guard 会导致 Future 非 Send/悬垂）
    let tmdb = app.tmdb.read().unwrap().clone();
    let _ = betterboxd_core::tools::ensure_movie_details(&tmdb, &app.db, id).await;
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
            "SELECT l.id AS list_id, l.name, li.rank, l.ranked, l.updated_at,
                    (SELECT COUNT(*) FROM list_items x WHERE x.list_id = l.id) AS item_count,
                    (SELECT x.movie_id FROM list_items x WHERE x.list_id = l.id
                      ORDER BY x.rank IS NULL, x.rank, x.added_at LIMIT 1) AS cover_movie_id
             FROM list_items li JOIN lists l ON l.id = li.list_id
             WHERE li.movie_id={id}
             ORDER BY li.rank IS NULL, li.rank, l.updated_at DESC"
        ))
        .await;
    // Log 富化：三类各查一次全量（量小），按 id 挂 payload（前端渲染互动卡片）
    let diary = app
        .db
        .select_json(&format!(
            "SELECT entry_id, movie_id, watched_date, rating, liked, in_theater, ticket_price_cents,
                    private_note, rewatch_index, tags, dimensions_flat,
                    title_main, title_sub, year
             FROM v_diary_full WHERE movie_id={id}"
        ))
        .await
        .unwrap_or_default();
    let reviews = app
        .db
        .select_json(&format!(
            "SELECT review_id, title, body_md, rating, liked, created_at, signature_date
             FROM v_reviews_full WHERE movie_id={id}"
        ))
        .await
        .unwrap_or_default();
    let chats = app
        .db
        .select_json(&format!(
            "SELECT id, title, last_message_at FROM chat_sessions WHERE movie_id={id}"
        ))
        .await
        .unwrap_or_default();
    match (movie, logs, lists) {
        (Ok(mut m), Ok(mut logs), Ok(lists)) => {
            for log in logs.iter_mut() {
                let lid = log["id"].as_str().unwrap_or("").to_string();
                match log["kind"].as_str().unwrap_or("") {
                    "watch" => {
                        if let Some(d) = diary.iter().find(|d| d["entry_id"] == log["id"]) {
                            log["diary"] = d.clone();
                        }
                    }
                    "review" => {
                        if let Some(r) = reviews.iter().find(|r| r["review_id"] == log["id"]) {
                            log["review"] = r.clone();
                        }
                    }
                    "chat" => {
                        if let Some(c) = chats.iter().find(|c| c["id"].as_str() == Some(lid.as_str())) {
                            log["chat"] = c.clone();
                        }
                    }
                    _ => {}
                }
            }
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

/// 标签自动补全面（P3）：值池 + 本人历史值按维度分组，自由标签高频。
async fn taxonomy(State(app): State<App>) -> Response {
    let dims = app
        .db
        .select_json(
            "SELECT dv.dimension AS dimension, dv.name AS name,
                    COALESCE(ed.cnt, 0) AS used
             FROM dimension_values dv
             LEFT JOIN (SELECT value_id, COUNT(*) AS cnt FROM entry_dimensions GROUP BY value_id) ed
               ON ed.value_id = dv.id
             ORDER BY dv.dimension, used DESC, dv.name",
        )
        .await;
    let tags = app
        .db
        .select_json(
            "SELECT t.name AS name, COUNT(et.entry_id) AS used
             FROM tags t LEFT JOIN entry_tags et ON et.tag_id = t.id
             GROUP BY t.id ORDER BY used DESC, t.name",
        )
        .await;
    match (dims, tags) {
        (Ok(dims), Ok(tags)) => {
            let mut grouped: serde_json::Map<String, Value> = serde_json::Map::new();
            for d in dims {
                grouped
                    .entry(d["dimension"].as_str().unwrap_or("").to_string())
                    .or_insert_with(|| Value::Array(vec![]))
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": d["name"], "used": d["used"]}));
            }
            Json(json!({
                "dimensions": grouped,
                "tags": tags.iter().map(|t| json!({"name": t["name"], "used": t["used"]})).collect::<Vec<_>>(),
            }))
            .into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "查询失败").into_response(),
    }
}

async fn movie_state(
    State(app): State<App>,
    AxPath(id): AxPath<i64>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    // 评分只能经 Diary/Review（署名日期绑定）；like 走本端点（影片级状态量）
    for banned in ["my_rating", "clear_my_rating"] {
        if args.get(banned).is_some() {
            return (
                StatusCode::BAD_REQUEST,
                "评分只能通过记一笔（Diary）或影评（Review）修改",
            )
                .into_response();
        }
    }
    let mut full = args.clone();
    full["movie_id"] = json!(id);
    match tools::execute("set_movie_state", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// 单条观影记录（确认卡 update 回填数据源）。
async fn diary_get(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法条目 ID").into_response();
    }
    match app
        .db
        .select_json_params(
            "SELECT entry_id, movie_id, title_main, title_sub, watched_date, rating, liked,
                    in_theater, ticket_price_cents, private_note, rewatch_index, tags, dimensions_flat
             FROM v_diary_full WHERE entry_id=?1"
                .into(),
            vec![betterboxd_core::db::SqlVal::Text(id)],
        )
        .await
    {
        Ok(rows) => match rows.into_iter().next() {
            Some(entry) => Json(json!({"entry": entry})).into_response(),
            None => (StatusCode::NOT_FOUND, "条目不存在").into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
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
        .clamp(1, 500); // LIMIT -1 = 无限制（缺陷 18）
    let offset = q
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
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
                    signature_date, title_zh, title_original AS title_sub, my_rating
             FROM v_reviews_full ORDER BY created_at DESC LIMIT 200",
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

async fn lists_list(State(app): State<App>) -> Response {
    match app
        .db
        .select_json(
            "SELECT l.id, l.name, l.description, l.source, l.ranked, l.created_at, l.updated_at,
                    (SELECT COUNT(*) FROM list_items li WHERE li.list_id = l.id) AS item_count,
                    (SELECT movie_id FROM list_items li WHERE li.list_id = l.id
                      ORDER BY li.rank IS NULL, li.rank, li.added_at LIMIT 1) AS cover_movie_id
             FROM lists l ORDER BY l.updated_at DESC",
        )
        .await
    {
        Ok(rows) => Json(json!({"lists": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn list_detail(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    // list id 为服务端生成的 uuid，仅允许安全字符（防注入）
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法清单 ID").into_response();
    }
    let meta = app
        .db
        .select_json(&format!(
            "SELECT id, name, description, source, ranked, created_at, updated_at
             FROM lists WHERE id='{id}'"
        ))
        .await;
    let Ok(meta) = meta else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "查询失败").into_response();
    };
    let Some(m) = meta.into_iter().next() else {
        return (StatusCode::NOT_FOUND, "清单不存在").into_response();
    };
    // 两种清单都按 rank（位置序）展示，unranked 只是不显示数字
    let order = "rank IS NULL, rank, added_at";
    match app
        .db
        .select_json(&format!(
            "SELECT li.rank, li.added_at, m.tmdb_id, m.title_main, m.title_sub, m.year,
                    m.my_rating, m.liked, m.watched, m.posters
             FROM list_items li JOIN v_movies m ON m.tmdb_id = li.movie_id
             WHERE li.list_id='{id}' ORDER BY {order}"
        ))
        .await
    {
        Ok(rows) => Json(json!({ "list": m, "items": rows })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn review_get(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法影评 ID").into_response();
    }
    match app
        .db
        .select_json_params(
            "SELECT review_id, movie_id, title, body_md, rating, liked, created_at, updated_at,
                    signature_date
             FROM v_reviews_full WHERE review_id=?1"
                .into(),
            vec![betterboxd_core::db::SqlVal::Text(id)],
        )
        .await
    {
        Ok(rows) => match rows.into_iter().next() {
            Some(review) => Json(json!({"review": review})).into_response(),
            None => (StatusCode::NOT_FOUND, "影评不存在").into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 清单写路径统一走 manage_lists 工具（REST 直连 = 用户明确意图，confirm=None）。
async fn list_add(State(app): State<App>, Json(mut args): Json<serde_json::Value>) -> Response {
    args["action"] = json!("create");
    match tools::execute("manage_lists", &tool_ctx(&app), args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn list_update(
    State(app): State<App>,
    AxPath(id): AxPath<String>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let mut full = args.clone();
    full["action"] = json!("update");
    full["list_id"] = json!(id);
    match tools::execute("manage_lists", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn list_delete(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    match tools::execute(
        "manage_lists",
        &tool_ctx(&app),
        json!({"action": "delete", "list_id": id}),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn list_item_add(
    State(app): State<App>,
    AxPath(id): AxPath<String>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let mut full = args.clone();
    full["action"] = json!("add_item");
    full["list_id"] = json!(id);
    match tools::execute("manage_lists", &tool_ctx(&app), full).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// rank 编辑（拖拽留待后续）：仅 manual 清单。
async fn list_item_rank(
    State(app): State<App>,
    AxPath((id, movie_id)): AxPath<(String, i64)>,
    Json(args): Json<serde_json::Value>,
) -> Response {
    let rank = args["rank"].as_i64();
    let r = app
        .db
        .call(move |c| {
            let src: String = c
                .query_row("SELECT source FROM lists WHERE id=?1", rusqlite::params![id], |r| r.get(0))
                .map_err(|_| rusqlite::Error::InvalidParameterName("清单不存在".into()))?;
            if src == "letterboxd" {
                return Err(rusqlite::Error::InvalidParameterName("Letterboxd 镜像清单只读".into()));
            }
            // unranked 也维护顺序（位置即排序，仅不展示数字）
            let n = c.execute(
                "UPDATE list_items SET rank=?3 WHERE list_id=?1 AND movie_id=?2",
                rusqlite::params![id, movie_id, rank],
            )?;
            if n == 0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "影片 {movie_id} 不在清单 {id} 中"
                )));
            }
            c.execute(
                "UPDATE lists SET updated_at=?2 WHERE id=?1",
                rusqlite::params![id, betterboxd_core::now()],
            )?;
            Ok(())
        })
        .await;
    match r {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn list_item_remove(
    State(app): State<App>,
    AxPath((id, movie_id)): AxPath<(String, i64)>,
) -> Response {
    match tools::execute(
        "manage_lists",
        &tool_ctx(&app),
        json!({"action": "remove_item", "list_id": id, "movie_id": movie_id}),
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
    let page = q.get("page").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1).clamp(1, 500);
    // 直接走共享搜索（不走 AI 工具的前 5 条截断，Search 页消费全量）
    match betterboxd_core::tools::search_and_cache(&tool_ctx(&app), query, year, page).await {
        Ok((results, total_pages)) => {
            Json(json!({"results": results, "total_pages": total_pages})).into_response()
        }
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
    let tmdb = app.tmdb.read().unwrap().clone();
    let r = tmdb
        .download_image(&format!("https://image.tmdb.org/t/p/w1280{bp}"))
        .await;
    match r {
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
    let tmdb = app.tmdb.read().unwrap().clone();
    let r = tmdb.download_poster(&pp).await;
    match r {
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
        Ok(rows) => (
            agent::cost_of(&rows, &fx, &cur, Some(ms)),
            agent::cost_of(&rows, &fx, &cur, None),
            rows.len(),
        ),
        Err(_) => (0.0, 0.0, 0),
    };
    Json(json!({"display_currency": cur, "month": month, "total": total, "calls": calls}))
        .into_response()
}

/// 统计执行（Stats 页重跑）：{saved_query_id} 直跑并刷新 last_run_at，
/// 或 {sql, chart} 直接执行。SQL 审查 + 强制 1000 行上限。
/// 统计项目的创建/发现走控制台对话（run_stats / manage_saved_queries 工具）。
async fn stats_run(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    // saved_query_id 优先
    if let Some(id) = args["saved_query_id"].as_str().map(String::from) {
        let id_query = id.clone();
        let payload: Option<String> = app
            .db
            .call(move |c| {
                Ok(c.query_row(
                    "SELECT payload_json FROM saved_queries WHERE id=?1",
                    rusqlite::params![id_query],
                    |r| r.get(0),
                )
                .ok())
            })
            .await
            .unwrap_or(None);
        let Some(payload) = payload else {
            return (StatusCode::NOT_FOUND, "统计项目不存在").into_response();
        };
        let p: Result<Value, _> = serde_json::from_str(&payload);
        let Ok(p) = p else {
            return (StatusCode::INTERNAL_SERVER_ERROR, "payload 损坏").into_response();
        };
        let sql = p["sql"].as_str().unwrap_or_default().to_string();
        let chart = p.get("chart").cloned().unwrap_or(json!({"type": "table"}));
        return match tools::execute_stats_ro(&app.stats_db, &sql, chart).await {
            Ok(mut out) => {
                let _ = app
                    .db
                    .call(move |c| {
                        c.execute(
                            "UPDATE saved_queries SET last_run_at=?1 WHERE id=?2",
                            rusqlite::params![betterboxd_core::now(), id],
                        )
                    })
                    .await;
                out["sql"] = json!(sql);
                out["ok"] = json!(true);
                Json(out).into_response()
            }
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        };
    }
    // 直接执行 SQL（Stats 页备用）
    let Some(sql) = args["sql"].as_str().map(String::from) else {
        return (
            StatusCode::BAD_REQUEST,
            "缺少 saved_query_id 或 sql（统计项目请在控制台对话创建）",
        )
            .into_response();
    };
    let chart = args
        .get("chart")
        .cloned()
        .unwrap_or(json!({"type": "table"}));
    match tools::execute_stats_ro(&app.stats_db, &sql, chart).await {
        Ok(mut out) => {
            out["sql"] = json!(sql);
            out["ok"] = json!(true);
            Json(out).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
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
             FROM chat_sessions s LEFT JOIN v_movies m ON m.tmdb_id = s.movie_id
             ORDER BY s.last_message_at DESC LIMIT 200",
        )
        .await
    {
        Ok(rows) => Json(json!({"chats": rows})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 删除会话：索引行 + JSON 落盘文件一并移除（Log 流中的聊记录随之消失）。
async fn chat_delete(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法会话 ID").into_response();
    }
    let id_db = id.clone();
    let n = app
        .db
        .call(move |c| {
            c.execute("DELETE FROM chat_sessions WHERE id=?1", rusqlite::params![id_db])
        })
        .await
        .unwrap_or(0);
    let _ = std::fs::remove_file(
        data_dir_path().join(format!(".betterboxd/sessions/{id}.json")),
    );
    if n > 0 {
        Json(json!({"ok": true})).into_response()
    } else {
        (StatusCode::NOT_FOUND, "会话不存在").into_response()
    }
}

/// LLM 消息数组 → UI 消息数组（还原工具名，丢弃空 assistant 与工具载荷）。
fn llm_to_ui_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut call_names: std::collections::HashMap<String, String> = Default::default();
    let mut ui = Vec::new();
    for m in messages {
        match m["role"].as_str().unwrap_or("") {
            "user" => {
                let t = m["content"].as_str().unwrap_or("");
                if !t.is_empty() {
                    ui.push(json!({"role": "user", "text": t}));
                }
            }
            "assistant" => {
                if let Some(tcs) = m["tool_calls"].as_array() {
                    for tc in tcs {
                        if let (Some(id), Some(name)) =
                            (tc["id"].as_str(), tc["function"]["name"].as_str())
                        {
                            call_names.insert(id.to_string(), name.to_string());
                        }
                    }
                }
                if let Some(t) = m["content"].as_str().filter(|t| !t.is_empty()) {
                    let mut v = json!({"role": "assistant", "text": t});
                    if let Some(u) = m.get("usage") {
                        v["usage"] = u.clone();
                    }
                    if let Some(c) = m.get("cost") {
                        v["cost"] = c.clone();
                    }
                    ui.push(v);
                }
            }
            "tool" => {
                if let Some(cid) = m["tool_call_id"].as_str() {
                    let name = call_names.get(cid).cloned().unwrap_or_else(|| "tool".into());
                    ui.push(json!({"role": "tool", "name": name}));
                }
            }
            _ => {}
        }
    }
    ui
}

/// 会话导出（本项目 JSON 格式）：id/scope/movie_id/entry_id/review_id/title/created_at/messages。
async fn chat_export(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法会话 ID").into_response();
    }
    match app.sessions.load(&id) {
        Some(s) => Json(json!({
            "id": s.id, "scope": s.scope, "movie_id": s.movie_id,
            "entry_id": s.entry_id, "review_id": s.review_id,
            "title": s.title, "created_at": s.created_at, "messages": s.messages,
        }))
        .into_response(),
        None => (StatusCode::NOT_FOUND, "会话不存在").into_response(),
    }
}

/// 会话导入（本项目格式）：id 冲突（已存在/JSON 非法）则生成新 id；标题/消息保留。
async fn chat_import(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let mut v = args;
    let mut id = v["id"].as_str().unwrap_or("").to_string();
    if id.is_empty()
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || app.sessions.load(&id).is_some()
    {
        id = uuid::Uuid::now_v7().to_string();
        v["id"] = json!(id);
    }
    if !v["messages"].is_array() {
        v["messages"] = json!([]);
    }
    if v["title"].as_str().unwrap_or("").is_empty() {
        v["title"] = json!("导入会话");
    }
    let s = betterboxd_core::session::ChatSession {
        id: v["id"].as_str().unwrap_or("").to_string(),
        scope: v["scope"].as_str().unwrap_or("global").to_string(),
        movie_id: v["movie_id"].as_i64(),
        entry_id: v["entry_id"].as_str().map(String::from),
        review_id: v["review_id"].as_str().map(String::from),
        title: v["title"].as_str().unwrap_or("").to_string(),
        created_at: v["created_at"].as_i64().unwrap_or_else(betterboxd_core::now),
        messages: serde_json::from_value(v["messages"].clone()).unwrap_or_default(),
    };
    match app.sessions.save(&s).await {
        Ok(_) => Json(json!({"ok": true, "session_id": s.id})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("导入失败: {e}")).into_response(),
    }
}

/// 修改会话标题（report b80caa5：Session 标题可改）。
async fn chat_title(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let Some(id) = args["id"].as_str().filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) else {
        return (StatusCode::BAD_REQUEST, "非法会话 ID").into_response();
    };
    let Some(title) = args["title"].as_str().map(String::from) else {
        return (StatusCode::BAD_REQUEST, "缺少 title").into_response();
    };
    let title = title.chars().take(60).collect::<String>();
    let Some(mut sess) = app.sessions.load(id) else {
        return (StatusCode::NOT_FOUND, "会话不存在").into_response();
    };
    sess.title = title;
    match app.sessions.save(&sess).await {
        Ok(_) => Json(json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// 单个会话的历史消息（前端恢复/查看用）。
async fn chat_detail(State(app): State<App>, AxPath(id): AxPath<String>) -> Response {
    // 会话 id 为 uuid，仅允许安全字符
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return (StatusCode::BAD_REQUEST, "非法会话 ID").into_response();
    }
    match app.sessions.load(&id) {
        Some(s) => Json(json!({
            "session": {"id": s.id, "scope": s.scope, "movie_id": s.movie_id, "title": s.title},
            "messages": llm_to_ui_messages(&s.messages),
        }))
        .into_response(),
        None => (StatusCode::NOT_FOUND, "会话不存在").into_response(),
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
    /// 讨论某 Diary 条目（P4.1）：注入条目全文+Action 史；movie_id 从条目行派生
    entry_id: Option<String>,
    /// 讨论某影评（P4.1）：注入影评全文+Action 史；movie_id 从影评行派生
    review_id: Option<String>,
    /// 按 session id 精确恢复（Chats 页打开指定会话）
    session_id: Option<String>,
    /// 跳过恢复，强制开新会话（控制台「新对话」按钮）
    fresh: Option<String>,
}

async fn ws_chat(
    State(app): State<App>,
    Query(q): Query<WsChatQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let movie_id = q.movie_id;
    let entry_id = q
        .entry_id
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let review_id = q
        .review_id
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let session_id = q
        .session_id
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let fresh = matches!(q.fresh.as_deref(), Some("1") | Some("true"));
    ws.on_upgrade(move |socket| {
        handle_chat(app, socket, movie_id, entry_id, review_id, session_id, fresh)
    })
}

async fn handle_chat(
    app: App,
    socket: WebSocket,
    ws_movie_id: Option<i64>,
    ws_entry_id: Option<String>,
    ws_review_id: Option<String>,
    ws_session_id: Option<String>,
    fresh: bool,
) {
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

    // 恢复优先级：session_id 精确恢复 > entry/review 匹配（P4.1）> 同 movie 最近 > 新建。
    // scope 统一 "movie"（用户裁定：详情页/Diary/Review 入口不分 scope），entry_id/review_id 仅决定注入内容。
    let resumed = if fresh {
        None
    } else if let Some(sid) = &ws_session_id {
        app.sessions.load(sid)
    } else if let Some(eid) = &ws_entry_id {
        app.sessions.find_by_column("entry_id", eid).await
    } else if let Some(rid) = &ws_review_id {
        app.sessions.find_by_column("review_id", rid).await
    } else {
        let scope = if ws_movie_id.is_some() { "movie" } else { "global" };
        app.sessions.find_latest(scope, ws_movie_id).await
    };
    // 新建：entry/review 入口的 movie_id 从行派生（WS 不要求同时传 movie_id）
    let is_fresh_session = resumed.is_none();
    // 开场白触发判定（hello 帧带标记；budget 前置检查在此完成一次）
    let opening_wanted = is_fresh_session;
    let mut current = match resumed {
        Some(s) => s,
        None => {
            let derive_mid = |sql: String, id: String| {
                let db = app.db.clone();
                async move {
                    db.select_json_params(sql, vec![betterboxd_core::db::SqlVal::Text(id)])
                        .await
                        .ok()
                        .and_then(|r| r.first().and_then(|x| x["movie_id"].as_i64()))
                }
            };
            if let Some(mid) = ws_movie_id {
                let t = movie_display_title(&app.db, Some(mid)).await;
                let mut s = app.sessions
                    .new_session("movie", Some(mid), ws_entry_id.clone(), ws_review_id.clone());
                s.title = format!("新建 {} 讨论", t);
                s
            } else if let Some(eid) = ws_entry_id {
                let mid = derive_mid(
                    "SELECT movie_id FROM diary_entries WHERE id=?1".into(),
                    eid.clone(),
                )
                .await;
                let t = movie_display_title(&app.db, mid).await;
                let mut s = app.sessions.new_session("movie", mid, Some(eid), None);
                s.title = format!("新建 {} 讨论", t);
                s
            } else if let Some(rid) = ws_review_id {
                let mid = derive_mid(
                    "SELECT movie_id FROM reviews WHERE id=?1".into(),
                    rid.clone(),
                )
                .await;
                let t = movie_display_title(&app.db, mid).await;
                let mut s = app.sessions.new_session("movie", mid, None, Some(rid));
                s.title = format!("新建 {} 讨论", t);
                s
            } else {
                let mut s = app.sessions.new_session("global", None, None, None);
                s.title = "新建控制台会话".into();
                s
            }
        }
    };
    // 恢复的会话自带 movie 归属——上下文注入跟随会话而非查询参数
    let session_movie_id = current.movie_id;
    let opening_triggered = opening_wanted;
    let hello = serde_json::json!({"type": "hello", "session_id": current.id,
        "opening": opening_triggered});
    let _ = tx.send(Message::Text(hello.to_string().into()));
    // 新会话立即落盘：Charts 列表无需等首轮结束即显示（report 3703c41 2 条）
    if is_fresh_session {
        let _ = app.sessions.save(&current).await;
    }

    let cancel: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));

    // 上下文注入（P4.1）：entry/review scope = 条目/影评全文 + Action 史 + 影片元数据；
    // movie scope = 影片元数据 + 最近记录（现状）。注入对每轮生效；开场白亦基于此。
    let context_injection = if let Some(eid) = &current.entry_id {
        let mut ctx_parts = Vec::new();
        if let Ok(rows) = app.db.select_json(&format!(
            "SELECT * FROM v_diary_full WHERE entry_id='{}'", eid.replace('\'', "")
        )).await
            && let Some(d) = rows.first() {
                let dims = d["dimensions_flat"].as_str().unwrap_or("[]");
                let tags = d["tags"].as_str().unwrap_or("[]");
                ctx_parts.push(format!(
                    "讨论对象：一条观影记录——\n影片: {}（{}）\n观看日期: {}（第{}次观看）\n当次评分: {}\n影院观影: {}，票价: {}\n随记: {}\n维度标注: {}\n标签: {}",
                    d["title_main"].as_str().unwrap_or("?"),
                    d["year"].as_str().unwrap_or(""),
                    d["watched_date"].as_str().unwrap_or("?"),
                    d["rewatch_index"].as_i64().unwrap_or(1),
                    d["rating"].as_i64().map(|r| format!("{r}/100")).unwrap_or("未评".into()),
                    if d["in_theater"].as_i64() == Some(1) { "是" } else { "否" },
                    d["ticket_price_cents"].as_i64().map(|c| format!("¥{:.2}", c as f64 / 100.0)).unwrap_or("-".into()),
                    d["private_note"].as_str().unwrap_or("（无）"),
                    dims, tags
                ));
                // 该条目 Action 史（变更时序，含评分变更）
                if let Ok(acts) = app.db.select_json(&format!(
                    "SELECT at, source, changes_json FROM v_actions
                     WHERE target='diary_entry' AND target_id='{}' ORDER BY at", eid.replace('\'', "")
                )).await
                    && !acts.is_empty() {
                        let hist: Vec<String> = acts.iter().map(|a| format!(
                            "- {} ({}) {}", 
                            chrono::DateTime::from_timestamp(a["at"].as_i64().unwrap_or(0), 0)
                                .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
                                .unwrap_or_default(),
                            a["source"].as_str().unwrap_or("?"),
                            a["changes_json"].as_str().unwrap_or("")
                        )).collect();
                        ctx_parts.push(format!("这条记录的变更史（时序）:\n{}", hist.join("\n")));
                    }
            }
        if let Some(mid) = session_movie_id
            && let Ok(rows) = app.db.select_json(&format!(
                "SELECT title_main, year, directors, genres, my_rating FROM v_movies WHERE tmdb_id={mid}"
            )).await
                && let Some(m) = rows.first() {
                    ctx_parts.push(format!("所属影片: {}（{}），导演 {}，我的最终评分 {}",
                        m["title_main"].as_str().unwrap_or("?"), m["year"].as_str().unwrap_or(""),
                        m["directors"].as_str().unwrap_or("?"),
                        m["my_rating"].as_i64().map(|r| r.to_string()).unwrap_or("未评".into())));
                }
        ctx_parts.join("\n\n")
    } else if let Some(rid) = &current.review_id { 
        let mut ctx_parts = Vec::new();
        if let Ok(rows) = app.db.select_json(&format!(
            "SELECT * FROM v_reviews_full WHERE review_id='{}'", rid.replace('\'', "")
        )).await
            && let Some(r) = rows.first() {
                let y = chrono::DateTime::from_timestamp(r["created_at"].as_i64().unwrap_or(0), 0)
                    .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                let _ = &y;
                ctx_parts.push(format!(
                    "讨论对象：一篇影评——\n影片: {}（题材: {}）\n影评标题: {}\n署名评分: {}\n写作时间: {}\n正文:\n{}",
                    r["title_zh"].as_str().or(r["title_en"].as_str()).unwrap_or("?"),
                    r["genres"].as_str().unwrap_or(""),
                    r["title"].as_str().unwrap_or("（无题）"),
                    r["rating"].as_i64().map(|x| format!("{x}/100")).unwrap_or("未评".into()),
                    chrono::DateTime::from_timestamp(r["created_at"].as_i64().unwrap_or(0), 0)
                        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    r["body_md"].as_str().unwrap_or("")
                ));
                if let Ok(acts) = app.db.select_json(&format!(
                    "SELECT at, source, changes_json FROM v_actions
                     WHERE target='review' AND target_id='{}' ORDER BY at", rid.replace('\'', "")
                )).await
                    && !acts.is_empty() {
                        let hist: Vec<String> = acts.iter().map(|a| format!(
                            "- {} ({}) {}",
                            chrono::DateTime::from_timestamp(a["at"].as_i64().unwrap_or(0), 0)
                                .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
                                .unwrap_or_default(),
                            a["source"].as_str().unwrap_or("?"),
                            a["changes_json"].as_str().unwrap_or("")
                        )).collect();
                        ctx_parts.push(format!("这篇影评的变更史（时序）:\n{}", hist.join("\n")));
                    }
            }
        if let Some(mid) = session_movie_id
            && let Ok(rows) = app.db.select_json(&format!(
                "SELECT title_main, year, directors, my_rating FROM v_movies WHERE tmdb_id={mid}"
            )).await
                && let Some(m) = rows.first() {
                    ctx_parts.push(format!("所属影片: {}（{}），导演 {}，我的最终评分 {}",
                        m["title_main"].as_str().unwrap_or("?"), m["year"].as_str().unwrap_or(""),
                        m["directors"].as_str().unwrap_or("?"),
                        m["my_rating"].as_i64().map(|r| r.to_string()).unwrap_or("未评".into())));
                }
        ctx_parts.join("\n\n")
    } else if let Some(mid) = session_movie_id {
        let mut ctx_parts = Vec::new();
        if let Ok(rows) = app.db.select_json(&format!(
            "SELECT title_main, title_sub, year, directors, genres, runtime,
                    my_rating, liked, in_watchlist, overview
             FROM v_movies WHERE tmdb_id={mid}"
        )).await {
            for m in rows.iter().take(1) {
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

    // —— P4 补充1：新 movie 系会话 → 服务端主动开场白（流式，落为第一条 assistant 消息）——
    // 注入上下文（条目/影评/影片 + 画像）在 opening system 里复述给用户，用户可直接看到 AI 读到了什么。
    let opening_handle = if opening_wanted {
        let client = app.client.read().unwrap().clone();
        // 开场白也为 LLM 调用：预算（缓存用量）拦截前置，超限直接提示（不跑生成）
        let budget_ok = {
            let cfg = app.config.lock().unwrap().clone();
            match cfg.active() {
                Ok(p) => {
                    let cost: f64 = app
                        .db
                        .select_json_params(
                            "SELECT cost FROM profile_usage WHERE profile_name=?1".into(),
                            vec![betterboxd_core::db::SqlVal::Text(p.name.clone())],
                        )
                        .await
                        .unwrap_or_default()
                        .first()
                        .and_then(|r| r["cost"].as_f64())
                        .unwrap_or(0.0);
                    betterboxd_core::agent::check_profile_budget(p.budget, cost)
                }
                Err(_) => Ok(()),
            }
        };
        if let Err(e) = budget_ok {
            let _ = tx.send(Message::Text(
                serde_json::json!({"type": "done", "interrupted": true,
                    "steps": 1, "tokens": {"prompt": 0, "completion": 0},
                    "aborted": Some(e)})
                .to_string()
                .into(),
            ));
            None
        } else if client.is_some() {
            // 已配置 AI：正常开场
            let tx_o = tx.clone();
            let sessions = app.sessions.clone();
            let db = app.db.clone();
            let cfg = app.config.lock().unwrap().clone();
            let ctx_inj = context_injection.clone();
            let pname = app.profile_name.lock().unwrap().clone();
            let pmodel = app.profile_model.lock().unwrap().clone();
            let token = CancellationToken::new();
            *cancel.lock().unwrap() = Some(token.clone());
            let mut cur = current.clone();
            Some(tokio::spawn(async move {
                let Some(client) = client else { return cur };
                let tx_ev = tx_o.clone();
                let on_event = move |ev: agent::AgentEvent| {
                    if let agent::AgentEventKind::Token(t) = ev.kind {
                        let _ = tx_ev.send(Message::Text(
                            serde_json::json!({"type": "token", "data": t}).to_string().into(),
                        ));
                    }
                };
                let run = agent::opening_message(
                    &client, &cfg, &db, &ctx_inj, token, on_event,
                )
                .await;
                match run {
                    Ok((text, (pt, ct, hit, miss))) => {
                        cur.messages
                            .push(serde_json::json!({"role": "assistant", "content": text}));
                        if cur.title.is_empty() {
                            cur.title = "新建控制台会话".to_string();
                        }
                        // 开场白同样记账（含成本；profile_usage 累加）
                        // （cost attach 在 log_llm_usage 返回后，与 run 一致）
                        let pricing = cfg.active().map(|p| (p.pricing.clone(), p.currency.clone()));
                        let (pz, cz) = pricing.unwrap_or_default();
                        let (cost, cur_) = log_llm_usage(
                            &db, &cur.id, &pname, &pmodel, pt, ct, hit, miss, &pz, &cz,
                        )
                        .await;
                        attach_usage(
                            &mut cur.messages,
                            serde_json::json!({"prompt": pt, "completion": ct, "hit": hit, "miss": miss}),
                            serde_json::json!({"value": cost, "currency": cur_}),
                        );
                        let _ = sessions.save(&cur).await;
                        let _ = tx_o.send(Message::Text(
                            serde_json::json!({"type": "done", "interrupted": false,
                                "steps": 1, "tokens": {"prompt": pt, "completion": ct,
                                    "hit": hit, "miss": miss},
                                "cost": {"value": cost, "currency": cur_}})
                            .to_string()
                            .into(),
                        ));
                    }
                    Err(e) if e.contains("已取消") => {
                        let _ = tx_o.send(Message::Text(
                            serde_json::json!({"type": "done", "interrupted": true,
                                "steps": 1, "tokens": {"prompt": 0, "completion": 0}})
                            .to_string()
                            .into(),
                        ));
                    }
                    Err(e) => {
                        let _ = tx_o.send(Message::Text(
                            serde_json::json!({"type": "error", "message": e}).to_string().into(),
                        ));
                    }
                }
                cur
            }))
        } else {
            // 未配置 AI：新建会话即报错（error 帧 → 前端 ⚠ 气泡；不再静默）
            let _ = tx.send(Message::Text(
                serde_json::json!({"type": "error",
                    "message": "尚未配置 AI 模型：请先到「设置」→「模型配置」添加配置后再开始对话"})
                .to_string()
                .into(),
            ));
            None
        }
    } else {
        None
    };
    if let Some(mut oh) = opening_handle {
        // 开场白阶段：完成前不处理用户消息（防乱序），可打断
        loop {
            tokio::select! {
                res = &mut oh => {
                    if let Ok(done_session) = res { current = done_session; }
                    *cancel.lock().unwrap() = None;
                    break;
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
                                                "message": "助手正在开场，请稍候…"})
                                                .to_string().into()));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {
                            // 断连：中止开场白，等任务收尾保存后结束整个连接
                            if let Some(c) = cancel.lock().unwrap().as_ref() { c.cancel(); }
                            let _ = oh.await; // 断连收尾：开场白已存盘，无需更新局部 current
                            return;
                        }
                    }
                }
            }
        }
    }

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

    

    let client = app.client.read().unwrap().clone();
    let Some(client) = client else {
            let _ = tx.send(Message::Text(
                serde_json::json!({"type": "error",
                    "message": "未配置模型档案，请先在设置页或 config.toml 配置"})
                .to_string()
                .into(),
            ));
            continue;
        };
        let tmdb = app.tmdb.read().unwrap().clone();
        let cfg_snapshot = app.config.lock().unwrap().clone();
        let mut task_session = current.clone();
        let ctx_inj_task = context_injection.clone();

        // —— 运行态：Agent 在独立任务中跑，主循环只盯 interrupt/断连 ——
        let tx_task = tx.clone();
        let sessions = app.sessions.clone();
        let db = app.db.clone();
        let stats_db = app.stats_db.clone();
        let pending = app.pending.clone();
        let sessions_store = app.sessions.clone();
        let pname = app.profile_name.lock().unwrap().clone();
        let pmodel = app.profile_model.lock().unwrap().clone();
        let mut run_handle = tokio::spawn(async move {
            let ctx = ToolCtx {
                db: db.clone(),
                stats_db: Some(stats_db.clone()),
                tmdb,
                config: cfg_snapshot.clone(),
                confirm: Some(std::sync::Arc::new(WsGate {
                    tx: tx_task.clone(),
                    pending,
                    cancel: token.clone(),
                })),
                source: "agent",
                sessions: Some(sessions_store),
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
                &ctx_inj_task,
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
                    let pricing = cfg_snapshot.active().map(|p| (p.pricing.clone(), p.currency.clone())).unwrap_or_default();
                    let (cost, cur) = log_llm_usage(
                        &db,
                        &task_session.id,
                        &pname,
                        &pmodel,
                        summary.usage_tokens.0,
                        summary.usage_tokens.1,
                        summary.usage_cache.0,
                        summary.usage_cache.1,
                        &pricing.0,
                        &pricing.1,
                    )
                    .await;
                    // usage/cost 持久化到最后一条 assistant 消息（历史恢复也有显示）
                    attach_usage(
                        &mut task_session.messages,
                        serde_json::json!({
                            "prompt": summary.usage_tokens.0,
                            "completion": summary.usage_tokens.1,
                            "hit": summary.usage_cache.0,
                            "miss": summary.usage_cache.1,
                        }),
                        serde_json::json!({"value": cost, "currency": cur}),
                    );
                    let _ = sessions.save(&task_session).await;
                    let tx_done = tx_task.clone();
                    let done = serde_json::json!({
                        "type": "done",
                        "interrupted": summary.interrupted,
                        "steps": summary.steps,
                        "tokens": {"prompt": summary.usage_tokens.0,
                                    "completion": summary.usage_tokens.1,
                                    "hit": summary.usage_cache.0,
                                    "miss": summary.usage_cache.1},
                        "cost": {"value": cost, "currency": cur},
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
    let cfg = app.config.lock().unwrap().clone();
    if let Some(new_profiles) = new_cfg.get_mut("profiles").and_then(|p| p.as_array_mut()) {
        // 缺陷 14：掩码 key 按**档案名**回填（下标在重排/插入时会错位）
        for np in new_profiles.iter_mut() {
            let masked = np.get("api_key").and_then(|k| k.as_str()).map(|s| s.contains("...")).unwrap_or(false);
            if masked
                && let Some(name) = np.get("name").and_then(|n| n.as_str())
                && let Some(old) = cfg.profiles.iter().find(|p| p.name == name)
            {
                np["api_key"] = serde_json::json!(old.api_key.clone());
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
            // TMDB Key 更新：重建 TmdbClient（与 AI 配置分离，独立一行）
            *app.tmdb.write().unwrap() = TmdbClient::new(
                c.tmdb.key.clone(),
                c.tmdb.proxy.clone(),
                c.tmdb.language.clone(),
            );
            // 缺陷 3：档案切换后原子重建 client（下轮对话即生效）
            let (client, pname, pmodel) = match c.active() {
                Ok(p) => (
                    Some(betterboxd_core::llm::ChatClient::new(
                        &p.endpoint, &p.api_key, &p.model,
                    )),
                    p.name.clone(),
                    p.model.clone(),
                ),
                Err(_) => (None, String::new(), String::new()),
            };
            *app.client.write().unwrap() = client;
            *app.profile_name.lock().unwrap() = pname;
            *app.profile_model.lock().unwrap() = pmodel;
            *app.config.lock().unwrap() = c;
            axum::Json(json!({"ok": true})).into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, format!("配置解析失败: {e}")).into_response(),
    }
}

/// 记账：按 pricing 计算本轮成本（输入分缓存命中/未命中，输出按未命中单价×1M；输出缓存命中罕见默认 0）
/// → 写 usage_records（前三笔在 server 落地后才有 currency）→ profile_usage「缓存用量」累加。
/// 返回 (本轮成本, 计价货币)。价格字段为每百万 token。
pub async fn log_llm_usage(
    db: &DbHandle,
    session_id: &str,
    pname: &str,
    pmodel: &str,
    pt: u64,
    ct: u64,
    cache_hit: u64,
    cache_miss: u64,
    pricing: &betterboxd_core::config::Pricing,
    currency: &str,
) -> (f64, String) {
    let miss = pt.saturating_sub(cache_hit) as f64 / 1e6;
    let hit = cache_hit as f64 / 1e6;
    let out = ct as f64 / 1e6;
    let in_cash = hit * pricing.input_cached + miss * pricing.input_uncached;
    let out_cash = out * pricing.output_uncached;
    let input_cost = in_cash;
    let output_cost = out_cash;
    let cost = in_cash + out_cash;
    let (sid, pn, m, cur) = (
        session_id.to_string(),
        pname.to_string(),
        pmodel.to_string(),
        currency.to_string(),
    );
    let db2 = db.clone();
    let (ic, oc) = (input_cost, output_cost);
    let _ = db2
        .call(move |c| {
            c.execute(
                "INSERT INTO usage_records (id, session_id, profile_name, model,
                   prompt_tokens, completion_tokens, input_cost, output_cost, currency, at, kind)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'llm')",
                rusqlite::params![
                    uuid::Uuid::now_v7().to_string(), sid, pn, m,
                    pt as i64, ct as i64, ic, oc, cur, betterboxd_core::now()
                ],
            )
        })
        .await;
    // 预算「缓存用量」累加（每配置独立）
    let (pn2, co, cu) = (pname.to_string(), cost, currency.to_string());
    let db3 = db.clone();
    let _ = db3
        .call(move |c| {
            c.execute(
                "INSERT INTO profile_usage (profile_name, cost, currency, updated_at)
                 VALUES (?1,?2,?3,?4)
                 ON CONFLICT(profile_name) DO UPDATE SET
                   cost = cost + ?2, currency = ?3, updated_at = ?4",
                rusqlite::params![pn2, co, cu, betterboxd_core::now()],
            )
        })
        .await;
    (cost, currency.to_string())
}


// ============ 存档（P4 完善：report 2026-09-04）============
// 存档列表 ~/.config/betterboxd/archives.json：{active_dir, archives:[{name, dir}]}
fn read_archives() -> serde_json::Value {
    std::fs::read_to_string(archives_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({"active_dir": null, "archives": []}))
}
fn write_archives(v: &serde_json::Value) -> Result<(), String> {
    if let Some(p) = archives_path().parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        archives_path(),
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

async fn archives_list(State(app): State<App>) -> Response {
    let mut v = read_archives();
    // 保证当前目录必在列表（新环境/被清理后也能「进入当前」；name=目录名）
    let cur = app.data_dir.to_string_lossy().to_string();
    let mut list = v["archives"].as_array().cloned().unwrap_or_default();
    if !list.iter().any(|a| a["dir"].as_str() == Some(cur.as_str())) {
        let name = std::path::Path::new(&cur)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "当前存档".to_string());
        list.push(serde_json::json!({"name": name, "dir": cur}));
        v["archives"] = serde_json::Value::Array(list);
        let _ = write_archives(&v);
    }
    let active = v["active_dir"].as_str().map(String::from).or(Some(app.data_dir.to_string_lossy().to_string()));
    // missing 标记：文件夹已不存在（用户手动删除）→ 选择屏点入时提示并自动去除
    let list = v["archives"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut a| {
            let d = a["dir"].as_str().unwrap_or("");
            a["missing"] = serde_json::json!(!d.is_empty() && !std::path::Path::new(d).exists());
            a
        })
        .collect::<Vec<_>>();
    Json(serde_json::json!({"active_dir": active, "archives": list, "current": cur}))
        .into_response()
}

/// 新建存档：parent/name 目录 + 初始化（config.toml 模板 + 空库迁移）。模拟示例数据用 seed_demo。
async fn archive_create(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let name = args["name"].as_str().unwrap_or("").trim().to_string();
    let parent = args["parent_dir"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() || parent.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "需要 name 与 parent_dir").into_response();
    }
    let dir = std::path::PathBuf::from(&parent).join(&name);
    let lib = dir.join(".betterboxd");
    if lib.join("data.db").exists() {
        return (axum::http::StatusCode::CONFLICT, "该目录已存在存档").into_response();
    }
    std::fs::create_dir_all(lib.join("sessions")).expect("建目录失败");
    // config.toml 模板：沿用当前 tmdb key/proxy，空 profile 交由设置页填写
    let cur_cfg = app.config.lock().unwrap().clone();
    let tmpl = format!(
        "# Betterboxd 存档配置\n# 未配置模型：请到「设置」→「模型配置」添加（保存后自动生效）\n",
    );
    let proxy_line = cur_cfg
        .tmdb
        .proxy
        .clone()
        .filter(|s| !s.trim().is_empty())
        .map(|p| format!("proxy = \"{}\"\n", p.replace('"', "\\\"")))
        .unwrap_or_default();
    let cfg_toml = format!(
        "{}[tmdb]\nkey = \"{}\"\n{}language = \"{}\"\n",
        tmpl,
        cur_cfg.tmdb.key.replace('"', "\\\""),
        proxy_line,
        cur_cfg.tmdb.language,
    );
    let _ = std::fs::write(lib.join("config.toml"), cfg_toml);
    // 空库 + 迁移
    match rusqlite::Connection::open(&lib.join("data.db")) {
        Ok(c) => {
            let _ = betterboxd_core::db::apply_migrations(&c);
        }
        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("开库失败: {e}")).into_response(),
    }
    // 注册 + 设为 active
    let mut v = read_archives();
    let list = v["archives"].as_array().cloned().unwrap_or_default();
    let mut list = list;
    list.retain(|a| a["dir"].as_str() != Some(dir.to_string_lossy().as_ref()));
    list.push(serde_json::json!({"name": name, "dir": dir.to_string_lossy().to_string(),
        "last_accessed": betterboxd_core::now()}));
    v["archives"] = serde_json::Value::Array(list);
    v["active_dir"] = serde_json::json!(dir.to_string_lossy().to_string());
    if write_archives(&v).is_err() {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "存档列表写入失败").into_response();
    }
    restart_into(dir, name)
}

/// 载入（注册）既有目录：不校验内部格式（report 裁定可按 UB 处理），加入列表并激活。
async fn archive_register(State(_app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let dir = args["dir"].as_str().unwrap_or("").trim().to_string();
    let name = args["name"].as_str().filter(|s| !s.is_empty()).map(String::from)
        .or_else(|| some_name_from_dir(&dir));
    if dir.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "需要 dir").into_response();
    }
    let p = std::path::PathBuf::from(&dir);
    if !p.join(".betterboxd").is_dir() {
        return (axum::http::StatusCode::BAD_REQUEST, "该目录没有 .betterboxd 结构——按 Undefined Behavior 尝试载入？请先确认").into_response();
    }
    let mut v = read_archives();
    let mut list = v["archives"].as_array().cloned().unwrap_or_default();
    list.retain(|a| a["dir"].as_str() != Some(p.to_string_lossy().as_ref()));
    let nm = name.unwrap_or_else(|| some_name_from_dir(&dir).unwrap_or_else(|| "存档".into()));
    list.push(serde_json::json!({"name": nm, "dir": p.to_string_lossy().to_string()}));
    v["archives"] = serde_json::Value::Array(list);
    v["active_dir"] = serde_json::json!(p.to_string_lossy().to_string());
    if write_archives(&v).is_err() {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "存档列表写入失败").into_response();
    }
    restart_into(p, nm)
}
fn some_name_from_dir(dir: &str) -> Option<String> {
    std::path::Path::new(dir)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
}

/// 切换/激活：写列表 + 重启 server（report 裁定：重启方案）。当前响应先返回，延迟退出让前端看到。
fn restart_into(dir: std::path::PathBuf, name: String) -> Response {
    let exe = std::env::current_exe().unwrap_or(std::path::PathBuf::from("betterboxd-server"));
    let cwd = std::env::current_dir().unwrap_or_default();
    let exe2 = exe.clone();
    let mut cmd = std::process::Command::new(exe2);
    cmd.arg("--data-dir").arg(&dir).current_dir(&cwd);
    // stdin 置空避免占用终端
    let _ = std::process::Command::spawn(&mut cmd);
    let resp = Json(serde_json::json!({"ok": true,
        "restart": true, "dir": dir.to_string_lossy().to_string(),
        "name": name, "message": "存档已激活，服务器重启中…"})).into_response();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        std::process::exit(0);
    });
    resp
}

/// 彻底删除存档：confirm 后物理删除目录 + 移出列表。拒绝删除当前活动目录。
async fn archive_delete(State(_app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let dir = args["dir"].as_str().unwrap_or("").to_string();
    if dir.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "需要 dir").into_response();
    }
    let mut v = read_archives();
    if v["active_dir"].as_str() == Some(dir.as_str()) {
        return (axum::http::StatusCode::CONFLICT, "不能删除当前正在使用的存档").into_response();
    }
    let list = v["archives"].as_array().cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = list
        .into_iter()
        .filter(|a| a["dir"].as_str() != Some(dir.as_str()))
        .collect();
    v["archives"] = serde_json::Value::Array(list);
    if let Err(e) = write_archives(&v) {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("列表写入失败: {e}")).into_response();
    }
    let p = std::path::PathBuf::from(&dir);
    match std::fs::remove_dir_all(&p) {
        Ok(_) => Json(serde_json::json!({"ok": true, "deleted": true})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("目录删除失败: {e}（已从列表移除）")).into_response(),
    }
}

/// 从列表移除（不删文件）；若为当前存档仅移出。
async fn archive_remove(State(_app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let dir = args["dir"].as_str().unwrap_or("").to_string();
    let mut v = read_archives();
    let list = v["archives"].as_array().cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = list
        .into_iter()
        .filter(|a| a["dir"].as_str() != Some(dir.as_str()))
        .collect();
    v["archives"] = serde_json::Value::Array(list);
    if v["active_dir"].as_str() == Some(dir.as_str()) {
        v["active_dir"] = serde_json::Value::Null;
    }
    if write_archives(&v).is_err() {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "写失败").into_response();
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

// ============ 用量 API（report 2026-09-04）============
/// 各 profile 计量：总用量（usage_records 历史累计）+ 缓存用量（profile_usage，预算判断+Reset）。
async fn usage_profiles(State(app): State<App>) -> Response {
    let totals = app
        .db
        .select_json(
            "SELECT profile_name, SUM(input_cost) AS input, SUM(output_cost) AS output,
                    SUM(input_cost + output_cost) AS total
             FROM usage_records WHERE input_cost IS NOT NULL GROUP BY profile_name",
        )
        .await
        .unwrap_or_default();
    let caches = app
        .db
        .select_json("SELECT profile_name, cost, currency FROM profile_usage")
        .await
        .unwrap_or_default();
    let cfg = app.config.lock().unwrap().clone();
    let mut out = Vec::new();
    for p in &cfg.profiles {
        let total = totals
            .iter()
            .find(|t| t["profile_name"].as_str() == Some(p.name.as_str()))
            .map(|t| t["total"].as_f64().unwrap_or(0.0))
            .unwrap_or(0.0);
        let cache = caches
            .iter()
            .find(|c| c["profile_name"].as_str() == Some(p.name.as_str()))
            .map(|c| {
                (
                    c["cost"].as_f64().unwrap_or(0.0),
                    c["currency"].as_str().unwrap_or("CNY").to_string(),
                )
            })
            .unwrap_or((0.0, p.currency.clone()));
        out.push(serde_json::json!({
            "profile_name": p.name, "total": total,
            "cache_cost": cache.0, "cache_currency": cache.1,
            "budget": p.budget, "currency": p.currency,
            "pricing": {
                "input_cached": p.pricing.input_cached,
                "input_uncached": p.pricing.input_uncached,
                "output_cached": p.pricing.output_cached,
                "output_uncached": p.pricing.output_uncached,
            }
        }));
    }
    Json(serde_json::json!({"profiles": out})).into_response()
}

/// Reset：缓存用量清零（= 重置当前预算周期）。
async fn usage_reset(State(app): State<App>, Json(args): Json<serde_json::Value>) -> Response {
    let name = args["profile_name"].as_str().unwrap_or("");
    if name.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "缺 profile_name").into_response();
    }
    let n = name.to_string();
    let _ = app
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE profile_usage SET cost=0, updated_at=?2 WHERE profile_name=?1",
                rusqlite::params![n, betterboxd_core::now()],
            ))
        })
        .await;
    Json(serde_json::json!({"ok": true})).into_response()
}


/// 把本轮 usage/cost 附加到会话最后一条 assistant 消息（持久化，恢复会话也有用量显示）。
fn attach_usage(messages: &mut [serde_json::Value], usage: serde_json::Value, cost: serde_json::Value) {
    for m in messages.iter_mut().rev() {
        if m["role"].as_str() == Some("assistant") {
            if m["content"].as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                m["usage"] = usage;
                m["cost"] = cost;
            }
            break;
        }
    }
}

// ============ 文件系统目录浏览（存档选择窗口用；localhost 单机）============
/// GET /api/fs/list?path=：列目录（仅目录），返回 parent/home/错误信息。
/// 起始（path 空/省略）：home 目录跨平台解析。
async fn fs_list(State(_app): State<App>, Query(q): Query<std::collections::HashMap<String, String>>) -> Response {
    let home = dirs_home();
    let cur_raw = q.get("path").map(String::as_str).unwrap_or("");
    let cur = if cur_raw.is_empty() {
        home.clone()
    } else {
        std::path::PathBuf::from(cur_raw)
    };
    let read = std::fs::read_dir(&cur);
    match read {
        Ok(rd) => {
            let mut dirs: Vec<serde_json::Value> = Vec::new();
            for en in rd.flatten() {
                if let Ok(meta) = en.metadata() {
                    if meta.is_dir() {
                        dirs.push(serde_json::json!({
                            "name": en.file_name().to_string_lossy().to_string(),
                            "path": en.path().to_string_lossy().to_string(),
                        }));
                    }
                }
            }
            dirs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            let parent = cur.parent().map(|p| p.to_string_lossy().to_string());
            Json(serde_json::json!({
                "current": cur.to_string_lossy().to_string(),
                "parent": parent,
                "home": home.to_string_lossy().to_string(),
                "dirs": dirs,
            })).into_response()
        }
        Err(_) => {
            let parent = cur.parent().map(|p| p.to_string_lossy().to_string());
            Json(serde_json::json!({
                "current": cur.to_string_lossy().to_string(),
                "parent": parent,
                "home": home.to_string_lossy().to_string(),
                "dirs": [],
                "error": "无法读取该目录",
            })).into_response()
        }
    }
}


/// 影片显示名（标题用；无数据回退 "影片"）。
async fn movie_display_title(db: &DbHandle, mid: Option<i64>) -> String {
    let Some(mid) = mid else { return "影片".into() };
    db.select_json(&format!(
        "SELECT COALESCE(title_main, title_zh, '影片') AS t FROM v_movies WHERE tmdb_id={mid}"
    ))
    .await
    .ok()
    .and_then(|r| r.first().and_then(|x| x["t"].as_str().map(String::from)))
    .unwrap_or_else(|| "影片".into())
}

fn data_dir_path() -> std::path::PathBuf {
    // 统一各路数据目录解析：config_save/chat_delete 等曾用 BB_DATA-only 版本，
    // 存档切换（--data-dir 重启）后写错目录——全部收敛到 resolve_data_dir()。
    resolve_data_dir()
}

/// 存档列表（用户级）：~/.config/betterboxd/archives.json
fn archives_path() -> std::path::PathBuf {
    dirs_home().join(".config").join("betterboxd").join("archives.json")
}
fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("BB_HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// 解析启动数据目录：--data-dir 参数 > BB_DATA env > 存档 active_dir > 默认 data/
fn resolve_data_dir() -> std::path::PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--data-dir" {
            if let Some(d) = args.next() {
                return std::path::PathBuf::from(d);
            }
        }
    }
    if let Ok(d) = std::env::var("BB_DATA") {
        return std::path::PathBuf::from(d);
    }
    if let Ok(raw) = std::fs::read_to_string(archives_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(d) = v["active_dir"].as_str() {
                let p = std::path::PathBuf::from(d);
                if p.join(".betterboxd").exists() {
                    return p;
                }
            }
        }
    }
    std::path::PathBuf::from("data")
}

#[tokio::main]
async fn main() {
    let data_dir = resolve_data_dir();
    let lib_dir = data_dir.join(".betterboxd");
    std::fs::create_dir_all(lib_dir.join("sessions")).expect("创建数据目录失败");
    let config = ensure_config(&lib_dir);
    let db_path = lib_dir.join("data.db");
    let conn = betterboxd_core::Db::open(&db_path)
        .expect("打开数据库失败")
        .into_inner();
    let db = DbHandle::spawn(conn);
    let sessions = Arc::new(SessionStore::new(&lib_dir, db.clone()));
    // 独立只读统计连接（评审缺陷 5）：统计四入口全部走它，query_only 纵深防御
    let stats_conn =
        betterboxd_core::db::Db::open_stats_conn(&db_path).expect("只读统计连接打开失败");
    let stats_db = DbHandle::spawn(stats_conn);
    // 容错：无活动档案时 client=None（设置页补配后生效），不 panic（存档模板可为空配置）
    let (client, pname, pmodel) = match config.active() {
        Ok(p) => (
            Some(betterboxd_core::llm::ChatClient::new(
                &p.endpoint,
                &p.api_key,
                &p.model,
            )),
            p.name.clone(),
            p.model.clone(),
        ),
        Err(_) => (None, String::new(), String::new()),
    };
    let client = Arc::new(std::sync::RwLock::new(client));
    let tmdb = Arc::new(std::sync::RwLock::new(TmdbClient::new(
        config.tmdb.key.clone(),
        config.tmdb.proxy.clone(),
        config.tmdb.language.clone(),
    )));
    let (pname, pmodel) = (pname, pmodel);
    let posters_dir = lib_dir.join("posters");
    std::fs::create_dir_all(&posters_dir).ok();
    let app = Arc::new(AppState {
        db,
        stats_db,
        sessions,
        config: Arc::new(Mutex::new(config)),
        client,
        tmdb,
        profile_name: Arc::new(Mutex::new(pname)),
        profile_model: Arc::new(Mutex::new(pmodel)),
        pending: Arc::new(Mutex::new(std::collections::HashMap::new())),
        posters_dir,
        data_dir: data_dir.clone(),
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
        .route("/api/diary/{id}", get(diary_get))
        .route("/api/diary/{id}", put(diary_update).delete(diary_delete))
        .route("/api/reviews", get(reviews_list).post(review_add))
        .route(
            "/api/reviews/{id}",
            put(review_update).delete(review_delete),
        )
        .route("/api/tmdb/search", get(tmdb_search))
        .route("/api/taxonomy", get(taxonomy))
        .route("/api/poster/{id}", get(poster))
        .route("/api/backdrop/{id}", get(backdrop))
        .route("/api/usage/summary", get(usage_summary))
        .route("/api/usage/profiles", get(usage_profiles))
        .route("/api/usage/reset", post(usage_reset))
        .route("/api/archives", get(archives_list))
        .route("/api/fs/list", get(fs_list))
        .route("/api/archives/create", post(archive_create))
        .route("/api/archives/register", post(archive_register))
        .route("/api/archives/remove", post(archive_remove))
        .route("/api/archives/delete", post(archive_delete))
        .route("/api/chats/title", put(chat_title))
        .route("/api/tools/execute", post(tools_execute))
        .route("/api/stats/run", post(stats_run))
        .route("/api/saved-queries", get(saved_queries_list))
        .route("/api/saved-queries/{id}", delete(saved_query_delete))
        .route("/api/chats", get(chats_list))
        .route("/api/chats/export/{id}", get(chat_export))
        .route("/api/chats/import", post(chat_import))
        .route("/api/chats/{id}", get(chat_detail).delete(chat_delete))
        .route("/api/lists", get(lists_list).post(list_add))
        .route(
            "/api/lists/{id}",
            get(list_detail).put(list_update).delete(list_delete),
        )
        .route("/api/lists/{id}/items", post(list_item_add))
        .route(
            "/api/lists/{id}/items/{movie_id}",
            put(list_item_rank).delete(list_item_remove),
        )
        .route("/api/reviews/{id}", get(review_get))
        .route("/api/config", get(config_get).post(config_save))
        .fallback_service(tower_http::services::ServeDir::new("apps/web/dist"))
        .with_state(app);

    // 默认只监听回环（评审缺陷 9）：私密随记不应对局域网开放
    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    println!("Betterboxd server: http://localhost:3000");
    // 存档切换重启：旧进程 400ms 后才退出，子进程启动时可能端口未释放 → 重试绑定（≤6 秒）
    let mut listener = None;
    for _ in 0..15 {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
            Err(e) => {
                eprintln!("bind 失败: {e}");
                std::process::exit(1);
            }
        }
    }
    let listener = listener.expect("端口 3000 持续被占用，无法启动");
    axum::serve(listener, router).await.unwrap();
}

/// 把前端确认结果路由到等待中的确认门。
/// 前端确认卡可编辑参数：frame.args 为对象时用之，缺失/非对象时回退原请求参数。
fn route_confirm(app: &App, frame: &serde_json::Value) {
    let Some(call_id) = frame["call_id"].as_str() else {
        return;
    };
    let decision = frame["decision"].as_str().unwrap_or("reject");
    let pending = app.pending.lock().unwrap().remove(call_id);
    if let Some(route) = pending {
        let payload = if decision == "confirm" {
            let edited = frame["args"].as_object().map(|_| frame["args"].clone());
            Ok(Some(edited.unwrap_or_else(|| route.args.clone())))
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
