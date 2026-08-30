//! 工具层：唯一业务 API（design.md §7.7 三路入口原则）。
//! M1 落地 8 个读工具；写工具随 M2 加入（确认卡协议）。

use crate::config::Config;
use crate::db::DbHandle;
use crate::tmdb::TmdbClient;
use serde_json::{Value, json};

pub struct ToolCtx {
    pub db: DbHandle,
    pub tmdb: TmdbClient,
    pub config: Config,
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
    // 若详情缺失（条目桩）则懒拉取补全
    let need_fetch: bool = ctx
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT COALESCE(fetched_at IS NULL, 1) FROM movies WHERE tmdb_id=?1",
                rusqlite::params![id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(1)
                != 0)
        })
        .await
        .map_err(|e| e.to_string())?;
    if need_fetch {
        let details = ctx
            .tmdb
            .movie_details(id)
            .await
            .map_err(|e| e.to_string())?;
        let directors: Vec<String> = details["credits"]["crew"]
            .as_array()
            .map(|crew| {
                crew.iter()
                    .filter(|c| c["job"] == "Director")
                    .filter_map(|c| c["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let genres: Vec<String> = details["genres"]
            .as_array()
            .map(|g| {
                g.iter()
                    .filter_map(|g| g["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let poster_owned = details["poster_path"].as_str().unwrap_or("").to_string();
        let runtime = details["runtime"].as_i64();
        ctx.db
            .call(move |c| {
                c.execute(
                    "UPDATE movies SET directors=?2, genres=?3, runtime=COALESCE(?4, runtime),
                       tagline=COALESCE(json_extract(?5,'$.tagline'), tagline),
                       overview=COALESCE(json_extract(?5,'$.overview'), overview),
                       posters=?6, fetched_at=?7 WHERE tmdb_id=?1",
                    rusqlite::params![
                        id,
                        serde_json::to_string(&directors).unwrap(),
                        serde_json::to_string(&genres).unwrap(),
                        runtime,
                        details.to_string(),
                        serde_json::to_string(&vec![poster_owned]).unwrap(),
                        crate::now()
                    ],
                )
            })
            .await
            .map_err(|e| e.to_string())?;
    }
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
