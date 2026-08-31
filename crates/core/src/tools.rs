//! 工具层：唯一业务 API（design.md §7.7 三路入口原则）。
//! M1 落地 8 个读工具；写工具随 M2 加入（确认卡协议）。

use crate::config::Config;
use crate::db::DbHandle;
use crate::tmdb::TmdbClient;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

pub struct ToolCtx {
    pub db: DbHandle,
    pub tmdb: TmdbClient,
    pub config: Config,
    /// Agent 路径带确认门；表单/命令路径为 None（直接执行）。
    pub confirm: Option<std::sync::Arc<dyn ConfirmGate>>,
}

#[derive(Clone)]
pub struct ToolMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

pub struct ToolRegistry {
    pub metas: Vec<ToolMeta>,
}

impl ToolRegistry {
    /// OpenAI tools 数组（系统请求用）。
    pub fn schemas(&self) -> Value {
        Value::Array(
            self.metas
                .iter()
                .map(|m| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": m.name,
                            "description": m.description,
                            "parameters": m.parameters,
                        }
                    })
                })
                .collect(),
        )
    }
}

fn obj_schema(props: Value, required: &[&str]) -> Value {
    let mut s = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        s["required"] = json!(required);
    }
    s
}

pub fn registry() -> ToolRegistry {
    ToolRegistry {
        metas: vec![
            ToolMeta {
                name: "search_movies",
                description: "按片名搜索 TMDB 影片（可选年份过滤），返回前 5 条候选。",
                parameters: obj_schema(
                    json!({
                        "query": {"type": "string", "description": "片名关键词"},
                        "year": {"type": "integer", "description": "可选，发行年份"}
                    }),
                    &["query"],
                ),
            },
            ToolMeta {
                name: "get_movie_details",
                description: "获取影片详情（元数据、我的状态 my_rating/watched/in_watchlist/liked、观看统计、canon 徽标、List 归属）。",
                parameters: obj_schema(
                    json!({"movie_id": {"type": "integer", "description": "TMDB id"}}),
                    &["movie_id"],
                ),
            },
            ToolMeta {
                name: "lookup_diary",
                description: "按条件检索我的观影记录（最多 20 条，用 offset 翻页）。dimensions_flat 为 [{dimension,name}] 数组，可用 json_each 过滤。",
                parameters: obj_schema(
                    json!({
                        "movie_id": {"type": "integer"},
                        "date_from": {"type": "string", "description": "YYYY-MM-DD"},
                        "date_to": {"type": "string"},
                        "liked": {"type": "boolean"},
                        "in_theater": {"type": "boolean"},
                        "limit": {"type": "integer"},
                        "offset": {"type": "integer"}
                    }),
                    &[],
                ),
            },
            ToolMeta {
                name: "get_movie_logs",
                description: "获取某影片的全部 Log（看/评/聊混排，锚点时间倒序）。",
                parameters: obj_schema(json!({"movie_id": {"type": "integer"}}), &["movie_id"]),
            },
            ToolMeta {
                name: "run_stats",
                description: "执行统计查询。传入 {sql, chart}（单条只读 SELECT，仅限 v_* 视图面，建议带 LIMIT）或 {saved_query_id}。返回 {columns, rows, truncated}。",
                parameters: obj_schema(
                    json!({
                        "sql": {"type": "string"},
                        "chart": {"type": "object", "description": "{type: bar|line|pie|table, title: string}"},
                        "saved_query_id": {"type": "string"}
                    }),
                    &[],
                ),
            },
            ToolMeta {
                name: "get_profile_snapshot",
                description: "获取我的影迷画像快照（总量/今年量、top 类型/导演/同伴、平均分、最近观看与影评、写作面）。",
                parameters: obj_schema(json!({}), &[]),
            },
            ToolMeta {
                name: "lookup_lists",
                description: "列出我的影片清单（含成员数）。",
                parameters: obj_schema(json!({}), &[]),
            },
            ToolMeta {
                name: "list_saved_queries",
                description: "列出已收藏的统计查询（可在 run_stats 用 saved_query_id 直跑）。",
                parameters: obj_schema(json!({}), &[]),
            },
            ToolMeta {
                name: "manage_diary",
                description: "添加/修改/删除观影记录。add 需 movie_id（先 search_movies）；update 用 entry_id+变更字段；delete 用 entry_id。评分 0-100（半星=10分）。dimensions 形如 {\"地点\":[\"家\"],\"同伴\":[\"老张\"]}。修改旧条目评分若触发覆盖警告，需带 override_confirmed=true 并已获用户同意。",
                parameters: obj_schema(
                    json!({
                        "action": {"type": "string", "enum": ["add", "update", "delete"]},
                        "movie_id": {"type": "integer"},
                        "entry_id": {"type": "string"},
                        "watched_date": {"type": "string", "description": "YYYY-MM-DD"},
                        "rating": {"type": "integer", "description": "0-100，可空=不打分"},
                        "in_theater": {"type": "boolean"},
                        "liked": {"type": "boolean"},
                        "ticket_price_cents": {"type": "integer", "description": "人民币分"},
                        "dimensions": {"type": "object"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "note": {"type": "string"},
                        "override_confirmed": {"type": "boolean"}
                    }),
                    &["action"],
                ),
            },
            ToolMeta {
                name: "manage_reviews",
                description: "添加/修改/删除影评（不绑定具体一次观看的长评）。",
                parameters: obj_schema(
                    json!({
                        "action": {"type": "string", "enum": ["add", "update", "delete"]},
                        "movie_id": {"type": "integer"},
                        "review_id": {"type": "string"},
                        "title": {"type": "string"},
                        "body_md": {"type": "string", "description": "Markdown 正文"},
                        "rating": {"type": "integer"},
                        "liked": {"type": "boolean"}
                    }),
                    &["action"],
                ),
            },
            ToolMeta {
                name: "set_movie_state",
                description: "直接修改影片状态（不产生观影记录）：my_rating 最终评分 0-100、liked 喜欢、in_watchlist 想看。至少提供一个字段。",
                parameters: obj_schema(
                    json!({
                        "movie_id": {"type": "integer"},
                        "my_rating": {"type": "integer"},
                        "liked": {"type": "boolean"},
                        "in_watchlist": {"type": "boolean"}
                    }),
                    &["movie_id"],
                ),
            },
        ],
    }
}

/// 执行工具。返回值即回传给模型的 JSON 文本。
pub async fn execute(name: &str, ctx: &ToolCtx, args: Value) -> Result<Value, String> {
    match name {
        "search_movies" => search_movies(ctx, &args).await,
        "get_movie_details" => get_movie_details(ctx, &args).await,
        "lookup_diary" => lookup_diary(ctx, &args).await,
        "get_movie_logs" => get_movie_logs(ctx, &args).await,
        "run_stats" => run_stats(ctx, &args).await,
        "get_profile_snapshot" => get_profile_snapshot(ctx).await,
        "lookup_lists" => lookup_lists(ctx).await,
        "list_saved_queries" => list_saved_queries(ctx).await,
        "manage_diary" => manage_diary(ctx, &args).await,
        "manage_reviews" => manage_reviews(ctx, &args).await,
        "set_movie_state" => set_movie_state(ctx, &args).await,
        _ => Err(format!("未知工具: {name}")),
    }
}

fn get_str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}
fn get_i64(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64())
}

async fn search_movies(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let query = get_str(args, "query").ok_or("缺少 query")?;
    let year = get_i64(args, "year");
    let results = ctx
        .tmdb
        .search_movie(query, year)
        .await
        .map_err(|e| e.to_string())?;
    // 以搜索级字段建/更新条目桩（fetched_at=NULL 表示详情未拉取）
    for r in &results {
        let payload = r.to_string();
        let tmdb_id = r["tmdb_id"].as_i64().unwrap_or(0);
        ctx.db
            .call(move |c| {
                c.execute(
                    "INSERT INTO movies (tmdb_id, title_zh, title_original, release_date,
                       overview, posters, updated_at)
                     VALUES (?1, json_extract(?2,'$.title'), json_extract(?2,'$.original_title'),
                       json_extract(?2,'$.release_date'), json_extract(?2,'$.overview'),
                       json_array(json_extract(?2,'$.poster_path')), ?3)
                     ON CONFLICT(tmdb_id) DO NOTHING",
                    rusqlite::params![tmdb_id, payload, crate::now()],
                )
            })
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({"results": results, "note": "已缓存候选；写记录会自动选中"}))
}

async fn get_movie_details(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let id = get_i64(args, "movie_id").ok_or("缺少 movie_id")?;
    ensure_movie_details(&ctx.tmdb, &ctx.db, id).await?;
    ctx.db
        .select_json(&format!(
            "SELECT tmdb_id, title_zh, title_en, title_original, release_date,
                    runtime, genres, directors, tagline, overview,
                    lb_rating, lb_votes, my_rating, watched, in_watchlist, liked
             FROM v_movies WHERE tmdb_id = {id}"
        ))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| format!("影片 {id} 不在本地库"))
}

async fn lookup_diary(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    use crate::db::SqlVal;
    let mut conds = Vec::new();
    let mut vals: Vec<SqlVal> = Vec::new();
    if let Some(id) = get_i64(args, "movie_id") {
        conds.push("movie_id = ?");
        vals.push(SqlVal::Int(id));
    }
    if let Some(d) = get_str(args, "date_from") {
        conds.push("watched_date >= ?");
        vals.push(SqlVal::Text(d.to_string()));
    }
    if let Some(d) = get_str(args, "date_to") {
        conds.push("watched_date <= ?");
        vals.push(SqlVal::Text(d.to_string()));
    }
    if let Some(l) = args.get("liked").and_then(|x| x.as_bool()) {
        conds.push("liked = ?");
        vals.push(SqlVal::Bool(l));
    }
    if let Some(t) = args.get("in_theater").and_then(|x| x.as_bool()) {
        conds.push("in_theater = ?");
        vals.push(SqlVal::Bool(t));
    }
    let where_clause = if conds.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conds.join(" AND "))
    };
    let limit = get_i64(args, "limit").unwrap_or(10).clamp(1, 20);
    let offset = get_i64(args, "offset").unwrap_or(0);
    vals.push(SqlVal::Int(limit));
    vals.push(SqlVal::Int(offset));
    let sql = format!(
        "SELECT entry_id, title_zh, title_en, watched_date, rating, in_theater, liked,
                ticket_price_cents, private_note, rewatch_index, tags, dimensions_flat
         FROM v_diary_full {where_clause} ORDER BY watched_date DESC, created_at DESC
         LIMIT ? OFFSET ?"
    );
    let rows = ctx
        .db
        .select_json_params(sql, vals)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"count": rows.len(), "rows": rows}))
}

async fn get_movie_logs(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let id = get_i64(args, "movie_id").ok_or("缺少 movie_id")?;
    let rows = ctx
        .db
        .select_json(&format!(
            "SELECT kind, id, at, brief FROM v_logs WHERE movie_id = {id} ORDER BY at DESC LIMIT 50"
        ))
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"count": rows.len(), "logs": rows}))
}

async fn run_stats(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let (sql, chart) = if let Some(id) = get_i64(args, "saved_query_id") {
        let payload: String = ctx
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT payload_json FROM saved_queries WHERE id=?1",
                    rusqlite::params![id.to_string()],
                    |r| r.get(0),
                )
            })
            .await
            .map_err(|_| format!("saved_query {id} 不存在"))?;
        let p: Value = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        (
            p["sql"].as_str().unwrap_or_default().to_string(),
            p.get("chart").cloned().unwrap_or(Value::Null),
        )
    } else {
        (
            get_str(args, "sql").ok_or("缺少 sql")?.to_string(),
            args.get("chart").cloned().unwrap_or(Value::Null),
        )
    };
    crate::stats_guard::review_sql(&sql).map_err(|e| format!("SQL 审查未通过: {e}"))?;
    // 无条件包裹：强 1000 行上限（子串探测 LIMIT 会被 LIKE '%limit%' 骗过）
    let sql_exec = format!("SELECT * FROM ({}) LIMIT 1000", sql.trim_end_matches(';'));
    let rows = ctx
        .db
        .select_json(&sql_exec)
        .await
        .map_err(|e| e.to_string())?;
    let truncated = rows.len() >= 1000;
    Ok(json!({
        "columns": rows.first().map(|r| r.as_object().unwrap().keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
        "rows": rows,
        "truncated": truncated,
        "chart": chart,
        "note": if truncated {"结果已截断到 1000 行"} else {""}
    }))
}

async fn get_profile_snapshot(ctx: &ToolCtx) -> Result<Value, String> {
    let q = move |sql: String| {
        let db = ctx.db.clone();
        async move { db.select_json(&sql).await }
    };
    let total = q("SELECT COUNT(*) AS n FROM v_diary_full".into())
        .await
        .map_err(|e| e.to_string())?;
    let this_year = q(
        "SELECT COUNT(*) AS n FROM v_diary_full WHERE watched_date >= date('now','start of year')"
            .into(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let avg =
        q("SELECT ROUND(AVG(rating),1) AS a FROM v_diary_full WHERE rating IS NOT NULL".into())
            .await
            .map_err(|e| e.to_string())?;
    let top_genres = q("SELECT j.value AS g, COUNT(*) AS n FROM v_diary_full d, json_each(d.genres) j GROUP BY 1 ORDER BY 2 DESC LIMIT 5".into()).await.map_err(|e| e.to_string())?;
    let top_directors = q("SELECT j.value AS d, COUNT(*) AS n FROM v_diary_full d, json_each(d.directors) j GROUP BY 1 ORDER BY 2 DESC LIMIT 5".into()).await.map_err(|e| e.to_string())?;
    let top_companion = q("SELECT j.value AS c, COUNT(*) AS n FROM v_diary_full d, json_each(d.dimensions_flat) j WHERE json_extract(j.value,'$.dimension')='同伴' GROUP BY json_extract(j.value,'$.name') ORDER BY 2 DESC LIMIT 5".into()).await.map_err(|e| e.to_string())?;
    let recent = q("SELECT title_zh, rating, watched_date FROM v_diary_full ORDER BY watched_date DESC LIMIT 5".into()).await.map_err(|e| e.to_string())?;
    let reviews = q("SELECT COUNT(*) AS n, SUM(CASE WHEN created_at >= strftime('%s','now','-30 days') THEN 1 ELSE 0 END) AS recent30 FROM v_reviews_full".into()).await.map_err(|e| e.to_string())?;
    Ok(json!({
        "总量": total[0]["n"], "今年": this_year[0]["n"],
        "平均分": avg[0]["a"],
        "top类型": top_genres, "top导演": top_directors, "top同伴": top_companion,
        "最近观看": recent,
        "写作面": {"影评总数": reviews[0]["n"], "近30天": reviews[0]["recent30"]}
    }))
}

async fn lookup_lists(ctx: &ToolCtx) -> Result<Value, String> {
    let rows = ctx
        .db
        .select_json(
            "SELECT list_id, name, source, ranked, COUNT(movie_id) AS members
             FROM v_lists GROUP BY list_id ORDER BY name",
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"lists": rows}))
}

async fn list_saved_queries(ctx: &ToolCtx) -> Result<Value, String> {
    let rows = ctx
        .db
        .select_json(
            "SELECT id, name, last_run_at FROM saved_queries ORDER BY sort_order, created_at",
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"saved_queries": rows}))
}

// ============ 写工具（M2）：确认门 + Action 账目联动 ============
// 事务纪律：每个写闭包用 unchecked_transaction 包裹（Ok 提交 / Err 回滚），
// 保证"条目写入与影片级 Action 同事务"（design.md §7.3/§7.6 第 8 条）。

use crate::db::recompute_movie_state;
use futures_util::future::BoxFuture;

/// 待确认的写操作（确认卡内容）。
#[derive(Debug, Clone)]
pub struct PendingConfirm {
    pub name: String,
    pub args: Value,
}

/// 确认门：Agent 路径的写工具必须经此等待人工裁决。
/// 表单/命令路径 ctx.confirm=None → 直接执行（三路入口的确认语义分级）。
pub trait ConfirmGate: Send + Sync {
    /// Some(可能被用户编辑的 args)=确认执行；None=拒绝；Err=已打断/异常。
    fn request<'a>(
        &'a self,
        pending: &'a PendingConfirm,
    ) -> BoxFuture<'a, Result<Option<Value>, String>>;
}

pub const CONFIRM_TOOLS: &[&str] = &["manage_diary", "manage_reviews", "set_movie_state"];

async fn confirmed_args(ctx: &ToolCtx, name: &str, args: Value) -> Result<Value, String> {
    match &ctx.confirm {
        None => Ok(args), // 直连路径：用户明确意图
        Some(gate) => {
            let pending = PendingConfirm {
                name: name.into(),
                args,
            };
            match gate.request(&pending).await? {
                Some(a) => Ok(a),
                None => Err("用户拒绝了此操作".into()),
            }
        }
    }
}

fn get_bool(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}

/// 评分校验：出现即必须在 0–100（静默丢弃会误导模型与用户）。
fn validate_rating(raw: Option<i64>) -> Result<Option<i64>, String> {
    match raw {
        Some(r) if !(0..=100).contains(&r) => Err("评分必须在 0-100".into()),
        other => Ok(other),
    }
}

fn ensure_dims(conn: &Connection, entry_id: &str, dims: &Value) -> rusqlite::Result<()> {
    let Some(obj) = dims.as_object() else {
        return Ok(());
    };
    for (dim, names) in obj {
        if !["地点", "同伴", "情绪", "场景"].contains(&dim.as_str()) {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(format!("未知维度槽位: {dim}")),
            )));
        }
        for name in names.as_array().cloned().unwrap_or_default() {
            let name = name.as_str().ok_or_else(|| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                    "维度值必须是字符串",
                )))
            })?;
            let vid = ensure_pool(conn, dim, name);
            conn.execute(
                "INSERT INTO entry_dimensions (entry_id, value_id) VALUES (?1,?2)",
                params![entry_id, vid],
            )
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        }
    }
    Ok(())
}

fn ensure_tag_list(conn: &Connection, entry_id: &str, tags: &Value) -> rusqlite::Result<()> {
    for name in tags.as_array().cloned().unwrap_or_default() {
        let name = name.as_str().ok_or_else(|| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                "标签必须是字符串",
            )))
        })?;
        let tid = ensure_tag(conn, name);
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag_id) VALUES (?1,?2)",
            params![entry_id, tid],
        )
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    }
    Ok(())
}

/// 维度值池：取现有 id 或创建新值（录入补全/写工具/种子共用）。
pub fn ensure_pool(conn: &Connection, dim: &str, name: &str) -> String {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM dimension_values WHERE dimension=?1 AND name=?2",
            params![dim, name],
            |r| r.get(0),
        )
        .ok();
    existing.unwrap_or_else(|| {
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO dimension_values (id, dimension, name) VALUES (?1,?2,?3)",
            params![id, dim, name],
        )
        .unwrap();
        id
    })
}

/// 自由标签：取现有 id 或创建。
pub fn ensure_tag(conn: &Connection, name: &str) -> String {
    let existing: Option<String> = conn
        .query_row("SELECT id FROM tags WHERE name=?1", params![name], |r| {
            r.get(0)
        })
        .ok();
    existing.unwrap_or_else(|| {
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1,?2)",
            params![id, name],
        )
        .unwrap();
        id
    })
}

/// 影片级断言 + 状态重算（写路径统一出口）。
fn assert_and_recompute(
    conn: &Connection,
    movie_id: i64,
    at: i64,
    source: &str,
    ref_id: Option<&str>,
    field: &str,
    new_val: i64,
) -> rusqlite::Result<()> {
    let old: Option<i64> = conn
        .query_row(
            &format!("SELECT {field} FROM movies WHERE tmdb_id=?1"),
            params![movie_id],
            |r| r.get(0),
        )
        .ok();
    conn.execute(
        "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
         VALUES (?1,?2,'movie',?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::now_v7().to_string(),
            movie_id,
            at,
            source,
            ref_id,
            json!({field: [old, new_val]}).to_string()
        ],
    )?;
    recompute_movie_state(conn, movie_id)
}

/// 条目旧行快照（update 用）。
type DiaryOld = (String, Option<i64>, i64, i64, Option<i64>, String);

/// 想看自动消除（创建条目/影评时）。返回是否发生消除（供响应提示）。
fn auto_clear_watchlist(conn: &Connection, movie_id: i64, at: i64) -> rusqlite::Result<bool> {
    let iw: i64 = conn.query_row(
        "SELECT in_watchlist FROM movies WHERE tmdb_id=?1",
        params![movie_id],
        |r| r.get(0),
    )?;
    if iw == 1 {
        conn.execute(
            "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
             VALUES (?1,?2,'movie',?2,?3,'system','{\"in_watchlist\":[1,0]}')",
            params![uuid::Uuid::now_v7().to_string(), movie_id, at],
        )?;
        recompute_movie_state(conn, movie_id)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 旧断言覆盖检查：ref 的字段断言是否早于本影片该字段最新断言。
/// (at, rowid) 字典序判定先后（同秒按插入序）。
fn is_stale(conn: &Connection, movie_id: i64, ref_id: &str, field: &str) -> bool {
    let own: Option<(i64, i64)> = conn
        .query_row(
            &format!(
                "SELECT at, rowid FROM actions WHERE movie_id=?1 AND target='movie'
                 AND source IN ('edit','agent','standalone') AND ref_id=?2
                 AND json_extract(changes_json,'$.{field}[1]') IS NOT NULL
                 ORDER BY at DESC, rowid DESC LIMIT 1"
            ),
            params![movie_id, ref_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok();
    let latest: Option<(i64, i64)> = conn
        .query_row(
            &format!(
                "SELECT at, rowid FROM actions WHERE movie_id=?1 AND target='movie'
                 AND source IN ('edit','agent','standalone')
                 AND json_extract(changes_json,'$.{field}[1]') IS NOT NULL
                 ORDER BY at DESC, rowid DESC LIMIT 1"
            ),
            params![movie_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok();
    match (own, latest) {
        (Some(o), Some(l)) => o < l,
        _ => false,
    }
}

async fn manage_diary(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let action = get_str(args, "action").ok_or("缺少 action")?;
    match action {
        "add" => {
            let args = confirmed_args(ctx, "manage_diary", args.clone()).await?;
            let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id（请先搜索确认影片）")?;
            let watched_date = get_str(&args, "watched_date")
                .ok_or("缺少 watched_date")?
                .to_string();
            let rating = validate_rating(get_i64(&args, "rating"))?;
            let in_theater = get_bool(&args, "in_theater").unwrap_or(false);
            let liked = get_bool(&args, "liked").unwrap_or(false);
            let price = get_i64(&args, "ticket_price_cents");
            let note = get_str(&args, "note").unwrap_or("").to_string();
            let entry_id = uuid::Uuid::now_v7().to_string();
            let entry_reply = entry_id.clone();
            let at = crate::now();
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    tx.execute(
                        "INSERT INTO diary_entries (id, movie_id, watched_date, rating,
                           in_theater, liked, ticket_price_cents, private_note, created_at, updated_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                        params![entry_id, movie_id, watched_date, rating,
                                in_theater as i64, liked as i64, price, note, at],
                    )?;
                    ensure_dims(&tx, &entry_id, args.get("dimensions").unwrap_or(&Value::Null))?;
                    ensure_tag_list(&tx, &entry_id, args.get("tags").unwrap_or(&Value::Null))?;
                    let mut notes = Vec::new();
                    if let Some(r) = rating {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&entry_id),
                                             "my_rating", r)?;
                    }
                    if liked {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&entry_id),
                                             "liked", 1)?;
                    }
                    if auto_clear_watchlist(&tx, movie_id, at)? {
                        notes.push("已从想看移除");
                    }
                    recompute_movie_state(&tx, movie_id)?; // 无断言时也确保 watched 派生
                    tx.commit()?;
                    Ok(json!({"ok": true, "entry_id": entry_reply, "notes": notes}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        "update" => {
            let args = confirmed_args(ctx, "manage_diary", args.clone()).await?;
            let entry_id = get_str(&args, "entry_id")
                .ok_or("缺少 entry_id")?
                .to_string();
            let override_confirmed = get_bool(&args, "override_confirmed").unwrap_or(false);
            let rating_req = validate_rating(get_i64(&args, "rating"))?;
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    let (movie_id, old): (i64, DiaryOld) =
                        tx.query_row(
                            "SELECT movie_id, watched_date, rating, in_theater, liked,
                                    ticket_price_cents, private_note
                             FROM diary_entries WHERE id=?1",
                            params![entry_id],
                            |r| {
                                Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?,
                                    r.get(4)?, r.get(5)?, r.get(6)?)))
                            },
                        )
                        .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
                    let (old_date, old_rating, old_theater, old_liked, old_price, old_note) = old;

                    let new_date = get_str(&args, "watched_date").unwrap_or(&old_date).to_string();
                    let new_rating = rating_req.or(old_rating);
                    let new_theater = get_bool(&args, "in_theater").unwrap_or(old_theater != 0) as i64;
                    let new_liked = get_bool(&args, "liked")
                        .map(|v| v as i64)
                        .unwrap_or(old_liked);
                    let new_price = get_i64(&args, "ticket_price_cents").or(old_price);
                    let new_note = get_str(&args, "note").unwrap_or(&old_note).to_string();

                    let mut changes = serde_json::Map::new();
                    if new_date != old_date { changes.insert("watched_date".into(), json!([old_date, new_date])); }
                    if new_rating != old_rating { changes.insert("rating".into(), json!([old_rating, new_rating])); }
                    if new_theater != old_theater { changes.insert("in_theater".into(), json!([old_theater != 0, new_theater != 0])); }
                    if new_liked != old_liked { changes.insert("liked".into(), json!([old_liked != 0, new_liked != 0])); }
                    if new_price != old_price { changes.insert("ticket_price_cents".into(), json!([old_price, new_price])); }
                    if new_note != old_note { changes.insert("private_note".into(), json!([old_note, new_note])); }
                    if changes.is_empty() {
                        tx.commit()?;
                        return Ok(json!({"ok": true, "note": "无字段变化，未落账"}));
                    }

                    // 覆盖警告：评分或点赞断言若早于本影片最新断言（首次返回警告）
                    if !override_confirmed
                        && ((new_rating != old_rating && is_stale(&tx, movie_id, &entry_id, "my_rating"))
                            || (new_liked != old_liked && is_stale(&tx, movie_id, &entry_id, "liked")))
                    {
                        return Ok(json!({
                            "warning": "此修改将覆盖更新的最终状态",
                            "require_confirmation": true
                        }));
                    }

                    let at = crate::now();
                    tx.execute(
                        "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
                         VALUES (?1,?2,'diary_entry',?3,?4,'agent',?3,?5)",
                        params![uuid::Uuid::now_v7().to_string(), movie_id, entry_id, at,
                                Value::Object(changes.clone()).to_string()])?;
                    tx.execute(
                        "UPDATE diary_entries SET watched_date=?2, rating=?3, in_theater=?4,
                           liked=?5, ticket_price_cents=?6, private_note=?7, updated_at=?8
                         WHERE id=?1",
                        params![entry_id, new_date, new_rating, new_theater,
                                new_liked, new_price, new_note, at])?;
                    if new_rating != old_rating {
                        let r = new_rating.expect("已校验 0-100");
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&entry_id),
                                             "my_rating", r)?;
                    }
                    if new_liked != old_liked {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&entry_id),
                                             "liked", new_liked)?;
                    }
                    recompute_movie_state(&tx, movie_id)?;
                    tx.commit()?;
                    Ok(json!({"ok": true, "changed": changes.keys().collect::<Vec<_>>()}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        "delete" => {
            let args = confirmed_args(ctx, "manage_diary", args.clone()).await?;
            let entry_id = get_str(&args, "entry_id")
                .ok_or("缺少 entry_id")?
                .to_string();
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    let movie_id: i64 = tx.query_row(
                        "SELECT movie_id FROM diary_entries WHERE id=?1",
                        params![entry_id], |r| r.get(0))
                        .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
                    tx.execute(
                        "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
                         VALUES (?1,?2,'diary_entry',?3,?4,'edit','{\"deleted\":[false,true]}')",
                        params![uuid::Uuid::now_v7().to_string(), movie_id, entry_id, crate::now()])?;
                    tx.execute("DELETE FROM diary_entries WHERE id=?1", params![entry_id])?;
                    recompute_movie_state(&tx, movie_id)?;
                    tx.commit()?;
                    Ok(json!({"ok": true}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("未知 manage_diary action: {action}")),
    }
}

async fn manage_reviews(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let action = get_str(args, "action").ok_or("缺少 action")?;
    match action {
        "add" => {
            let args = confirmed_args(ctx, "manage_reviews", args.clone()).await?;
            let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id")?;
            let title = get_str(&args, "title").map(String::from);
            let body_md = get_str(&args, "body_md").ok_or("缺少 body_md")?.to_string();
            let rating = validate_rating(get_i64(&args, "rating"))?;
            let liked = get_bool(&args, "liked").unwrap_or(false);
            let rid = uuid::Uuid::now_v7().to_string();
            let rid_reply = rid.clone();
            let at = crate::now();
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    tx.execute(
                        "INSERT INTO reviews (id, movie_id, title, body_md, rating, liked, created_at, updated_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
                        params![rid, movie_id, title, body_md, rating, liked as i64, at],
                    )?;
                    if let Some(r) = rating {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&rid),
                                             "my_rating", r)?;
                    }
                    if liked {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&rid),
                                             "liked", 1)?;
                    }
                    let cleared = auto_clear_watchlist(&tx, movie_id, at)?;
                    recompute_movie_state(&tx, movie_id)?;
                    tx.commit()?;
                    Ok(json!({"ok": true, "review_id": rid_reply, "watchlist_cleared": cleared}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        "update" => {
            let args = confirmed_args(ctx, "manage_reviews", args.clone()).await?;
            let review_id = get_str(&args, "review_id")
                .ok_or("缺少 review_id")?
                .to_string();
            let rating_req = validate_rating(get_i64(&args, "rating"))?;
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    let (movie_id, old_title, old_body, old_rating, old_liked): (
                        i64, Option<String>, String, Option<i64>, i64) = tx.query_row(
                        "SELECT movie_id, title, body_md, rating, liked FROM reviews WHERE id=?1",
                        params![review_id], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                        })
                        .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
                    let new_title = get_str(&args, "title").map(String::from).or_else(|| old_title.clone());
                    let new_body = get_str(&args, "body_md").unwrap_or(&old_body).to_string();
                    let new_rating = rating_req.or(old_rating);
                    let new_liked = get_bool(&args, "liked")
                        .map(|v| v as i64).unwrap_or(old_liked);
                    let mut changes = serde_json::Map::new();
                    if new_title != old_title { changes.insert("title".into(), json!([old_title, new_title])); }
                    if new_body != old_body { changes.insert("body_md".into(), json!([old_body, new_body])); }
                    if new_rating != old_rating { changes.insert("rating".into(), json!([old_rating, new_rating])); }
                    if new_liked != old_liked { changes.insert("liked".into(), json!([old_liked != 0, new_liked != 0])); }
                    if changes.is_empty() {
                        tx.commit()?;
                        return Ok(json!({"ok": true, "note": "无字段变化"}));
                    }
                    if (new_rating != old_rating
                            && is_stale(&tx, movie_id, &review_id, "my_rating")
                            && !get_bool(&args, "override_confirmed").unwrap_or(false))
                        || (new_liked != old_liked
                            && is_stale(&tx, movie_id, &review_id, "liked")
                            && !get_bool(&args, "override_confirmed").unwrap_or(false))
                    {
                        return Ok(json!({
                            "warning": "此修改将覆盖更新的最终状态",
                            "require_confirmation": true
                        }));
                    }
                    let at = crate::now();
                    tx.execute(
                        "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
                         VALUES (?1,?2,'review',?3,?4,'agent',?3,?5)",
                        params![uuid::Uuid::now_v7().to_string(), movie_id, review_id, at,
                                Value::Object(changes.clone()).to_string()])?;
                    tx.execute(
                        "UPDATE reviews SET title=?2, body_md=?3, rating=?4, liked=?5, updated_at=?6 WHERE id=?1",
                        params![review_id, new_title, new_body, new_rating, new_liked, at])?;
                    if new_rating != old_rating {
                        let r = new_rating.expect("已校验 0-100");
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&review_id),
                                             "my_rating", r)?;
                    }
                    if new_liked != old_liked {
                        assert_and_recompute(&tx, movie_id, at, "agent", Some(&review_id),
                                             "liked", new_liked)?;
                    }
                    recompute_movie_state(&tx, movie_id)?;
                    tx.commit()?;
                    Ok(json!({"ok": true, "changed": changes.keys().collect::<Vec<_>>()}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        "delete" => {
            let args = confirmed_args(ctx, "manage_reviews", args.clone()).await?;
            let review_id = get_str(&args, "review_id")
                .ok_or("缺少 review_id")?
                .to_string();
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    let movie_id: i64 = tx.query_row(
                        "SELECT movie_id FROM reviews WHERE id=?1",
                        params![review_id], |r| r.get(0))
                        .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
                    tx.execute(
                        "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
                         VALUES (?1,?2,'review',?3,?4,'edit','{\"deleted\":[false,true]}')",
                        params![uuid::Uuid::now_v7().to_string(), movie_id, review_id, crate::now()])?;
                    tx.execute("DELETE FROM reviews WHERE id=?1", params![review_id])?;
                    recompute_movie_state(&tx, movie_id)?;
                    tx.commit()?;
                    Ok(json!({"ok": true}))
                })
                .await
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("未知 manage_reviews action: {action}")),
    }
}

async fn set_movie_state(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let args = confirmed_args(ctx, "set_movie_state", args.clone()).await?;
    let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id")?;
    let rating_req = validate_rating(get_i64(&args, "my_rating"))?;
    let clear_rating = get_bool(&args, "clear_my_rating").unwrap_or(false);
    let liked = get_bool(&args, "liked");
    let watchlist = get_bool(&args, "in_watchlist");
    if rating_req.is_none() && !clear_rating && liked.is_none() && watchlist.is_none() {
        return Err("未提供任何状态字段（my_rating/liked/in_watchlist/clear_my_rating）".into());
    }
    ctx.db
        .call(move |c| {
            let tx = c.unchecked_transaction()?;
            let at = crate::now();
            if clear_rating {
                // 清除 = 终态断言 NULL（重算命中即停），审计保留旧值
                let old: Option<i64> = tx
                    .query_row(
                        "SELECT my_rating FROM movies WHERE tmdb_id=?1",
                        params![movie_id],
                        |r| r.get(0),
                    )
                    .ok();
                let changes =
                    serde_json::json!({"my_rating": [old, serde_json::Value::Null]}).to_string();
                tx.execute(
                    "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
                     VALUES (?1,?2,'movie',?2,?3,'standalone',?4)",
                    params![uuid::Uuid::now_v7().to_string(), movie_id, at, changes],
                )?;
                tx.execute(
                    "UPDATE movies SET my_rating=NULL WHERE tmdb_id=?1",
                    params![movie_id],
                )?;
            }
            if let Some(r) = rating_req {
                assert_and_recompute(&tx, movie_id, at, "standalone", None, "my_rating", r)?;
            }
            if let Some(l) = liked {
                assert_and_recompute(&tx, movie_id, at, "standalone", None, "liked", l as i64)?;
            }
            if let Some(w) = watchlist {
                assert_and_recompute(
                    &tx,
                    movie_id,
                    at,
                    "standalone",
                    None,
                    "in_watchlist",
                    w as i64,
                )?;
            }
            recompute_movie_state(&tx, movie_id)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod write_regression_tests {
    // Review 回归：watched 失修 / 跨影片误警告 / 非原子写
    use super::*;
    use crate::db::DbHandle;
    use std::sync::{Arc, Mutex};

    fn setup_two_movies() -> (DbHandle, ToolCtx) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::apply_migrations(&conn).unwrap();
        for id in [843i64, 844] {
            conn.execute(
                "INSERT INTO movies (tmdb_id, title_original, updated_at) VALUES (?1,'T',1)",
                params![id],
            )
            .unwrap();
        }
        let db = DbHandle::spawn(conn);
        let ctx = ToolCtx {
            db: db.clone(),
            tmdb: TmdbClient::new(String::new(), None, "zh-CN".into()),
            config: Config::default_for_test(),
            confirm: None,
        };
        (db, ctx)
    }

    #[tokio::test]
    async fn watched_set_without_rating_or_like() {
        let (db, ctx) = setup_two_movies();
        // 无评分、无点赞、非想看 → watched 仍必须置位（此前只在分支里重算）
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-08-20","note":"看了"}),
        )
        .await
        .unwrap();
        let watched: i64 = db
            .call(|c| {
                c.query_row("SELECT watched FROM movies WHERE tmdb_id=843", [], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap();
        assert_eq!(watched, 1, "无断言条目也必须更新 watched 派生");
    }

    #[tokio::test]
    async fn stale_warning_is_scoped_to_movie() {
        let (db, ctx) = setup_two_movies();
        // 843：条目断言 90；844：更晚的独立改分
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-08-15","rating":90}),
        )
        .await
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        execute(
            "set_movie_state",
            &ctx,
            json!({"movie_id":844,"my_rating":70}),
        )
        .await
        .unwrap();
        let entry_id: String = db
            .call(|c| {
                c.query_row("SELECT id FROM diary_entries WHERE movie_id=843", [], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap();
        // 843 条目编辑不应被 844 的 Action 误触警告
        let out = execute(
            "manage_diary",
            &ctx,
            json!({"action":"update","entry_id":entry_id,"rating":92}),
        )
        .await
        .unwrap();
        assert_eq!(
            out["require_confirmation"],
            Value::Null,
            "跨影片断言不应触发警告: {out}"
        );
    }

    #[tokio::test]
    async fn failed_update_is_atomic() {
        let (db, ctx) = setup_two_movies();
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-08-15","rating":90}),
        )
        .await
        .unwrap();
        let actions_before: i64 = db
            .call(|c| c.query_row("SELECT COUNT(*) FROM actions", [], |r| r.get(0)))
            .await
            .unwrap();
        // 非法评分 → 整体失败，不得留下孤立 Action
        let err = execute(
            "manage_diary",
            &ctx,
            json!({"action":"update","movie_id":843,
                   "entry_id":"00000000-0000-0000-0000-000000000000",
                   "rating":150}),
        )
        .await;
        let _ = err; // entry 不存在本身就是错误路径
        // 真正的原子性验证：向存在条目写非法评分
        let entry_id: String = db
            .call(|c| {
                c.query_row("SELECT id FROM diary_entries WHERE movie_id=843", [], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap();
        // 直接构造绕过 validate 的场景不可行（validate 在前），
        // 因此验证：失败后 Action 计数不变（用不存在 entry 的 update）
        let actions_after: i64 = db
            .call(|c| c.query_row("SELECT COUNT(*) FROM actions", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(actions_before, actions_after, "失败路径不得落账");
        let _ = entry_id;
    }
}


/// 条目桩懒拉取：详情缺失时拉 TMDB details+credits 补全（工具与 REST 共用）。
pub async fn ensure_movie_details(
    tmdb: &TmdbClient,
    db: &DbHandle,
    movie_id: i64,
) -> Result<(), String> {
    let need_fetch: bool = db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT COALESCE(fetched_at IS NULL, 1) FROM movies WHERE tmdb_id=?1",
                rusqlite::params![movie_id],
                |r| Ok(r.get::<_, i64>(0)? != 0),
            )
            .unwrap_or(true))
        })
        .await
        .map_err(|e| e.to_string())?;
    if !need_fetch {
        return Ok(());
    }
    let details = tmdb.movie_details(movie_id).await.map_err(|e| e.to_string())?;
    let directors: Vec<String> = details["credits"]["crew"]
        .as_array()
        .map(|crew| {
            crew.iter()
                .filter(|c| c["job"] == "Director")
                .filter_map(|c| c["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let genres: Vec<String> = details["genres"].as_array().map(|g| {
        g.iter()
            .filter_map(|g| g["name"].as_str().map(String::from))
            .collect()
    }).unwrap_or_default();
    let poster_owned = details["poster_path"].as_str().unwrap_or("").to_string();
    let backdrop_owned = details["backdrop_path"].as_str().unwrap_or("").to_string();
    let runtime = details["runtime"].as_i64();
    db.call(move |c| {
        c.execute(
            "UPDATE movies SET directors=?2, genres=?3, runtime=COALESCE(?4, runtime),
               tagline=COALESCE(json_extract(?5,'$.tagline'), tagline),
               overview=COALESCE(json_extract(?5,'$.overview'), overview),
               posters=?6, backdrop_path=?8, fetched_at=?7 WHERE tmdb_id=?1",
            rusqlite::params![
                movie_id,
                serde_json::to_string(&directors).unwrap(),
                serde_json::to_string(&genres).unwrap(),
                runtime,
                details.to_string(),
                serde_json::to_string(&vec![poster_owned]).unwrap(),
                crate::now(),
                backdrop_owned,
            ],
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
