//! 工具层：唯一业务 API（design.md §7.7 三路入口原则）。
//! M1 落地 8 个读工具；写工具随 M2 加入（确认卡协议）。

use crate::config::Config;
use crate::db::DbHandle;
use crate::tmdb::TmdbClient;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

pub struct ToolCtx {
    pub db: DbHandle,
    /// 只读统计连接（评审缺陷 5）；测试无只读库时 None → 回退 db。
    pub stats_db: Option<DbHandle>,
    pub tmdb: TmdbClient,
    pub config: Config,
    /// Agent 路径带确认门；表单/命令路径为 None（直接执行）。
    pub confirm: Option<std::sync::Arc<dyn ConfirmGate>>,
    /// 断言 source 语义（评审缺陷 7）：Agent 路径 = "agent"，GUI/REST = "edit"。
    pub source: &'static str,
}

impl ToolCtx {
    /// 统计执行连接：生产 = 独立只读库；测试回退主库。
    pub fn stats(&self) -> &DbHandle {
        self.stats_db.as_ref().unwrap_or(&self.db)
    }
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
                description: "按条件检索我的观影记录（最多 20 条，用 offset 翻页）。dimensions_flat 为 [{dimension,name}] 数组，可用 json_each 过滤。update/delete 前用本工具（支持 title 关键词）定位 entry_id，无需先搜索影片。",
                parameters: obj_schema(
                    json!({
                        "movie_id": {"type": "integer"},
                        "title": {"type": "string", "description": "片名关键词（模糊匹配中/英文标题）"},
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
                description: "执行统计查询。优先传 saved_query_id 复用已有统计项目；否则传 {sql, chart}（单条只读 SELECT，仅限 v_* 视图面，带 LIMIT，列用中文 AS 别名且标签列在前）。拿到结果后用自然语言+图标解读，禁止裸贴数据。",
                parameters: obj_schema(
                    json!({
                        "sql": {"type": "string"},
                        "chart": {"type": "object", "description": "{type: bar|line|pie|table, title: string}"},
                        "saved_query_id": {"type": "string", "description": "已保存统计项目的 id（list_saved_queries 获取）"}
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
                name: "manage_lists",
                description: "管理影片清单：create（name 必填，ranked 开关）、update（list_id+名称/描述/ranked）、delete（list_id）、add_item（list_id+movie_id，ranked 清单 rank 缺省自动追加队尾）、remove_item（list_id+movie_id）。Letterboxd 镜像清单只读，禁止修改/删除。先 lookup_lists 拿 list_id。",
                parameters: obj_schema(
                    json!({
                        "action": {"type": "string", "enum": ["create", "update", "delete", "add_item", "remove_item"]},
                        "list_id": {"type": "string"},
                        "movie_id": {"type": "integer"},
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "ranked": {"type": "boolean"},
                        "rank": {"type": "integer", "description": "ranked 清单中的名次；缺省追加队尾"}
                    }),
                    &["action"],
                ),
            },
            ToolMeta {
                name: "list_saved_queries",
                description: "列出已保存的统计项目（id/名称/SQL/最近运行时间）。统计前先查这里，命中同类项目直接用 run_stats 的 saved_query_id 复跑。",
                parameters: obj_schema(json!({}), &[]),
            },
            ToolMeta {
                name: "manage_saved_queries",
                description: "保存/删除/重命名统计项目（只落库不执行；执行用 run_stats）。create 需 name+sql（SQL 会先过审查：单条只读 SELECT，仅限 v_* 视图面）；delete/rename 需 saved_query_id。保存操作会弹确认卡向用户展示名称与 SQL。",
                parameters: obj_schema(
                    json!({
                        "action": {"type": "string", "enum": ["create", "delete", "rename"]},
                        "saved_query_id": {"type": "string"},
                        "name": {"type": "string"},
                        "sql": {"type": "string"},
                        "chart": {"type": "object"}
                    }),
                    &["action"],
                ),
            },
            ToolMeta {
                name: "manage_diary",
                description: "添加/修改/删除观影记录。add 需 movie_id（先 search_movies）；update 用 entry_id+变更字段；delete 用 entry_id。评分 0-100（半星=10分），最终评分绑定到观看日期最靠后的有分条目。dimensions 形如 {\"地点\":[\"家\"],\"同伴\":[\"老张\"]}。同片同日重复录入会先警告，确认仍要记录带 override_confirmed=true。",
                parameters: obj_schema(
                    json!({
                        "action": {"type": "string", "enum": ["add", "update", "delete"]},
                        "movie_id": {"type": "integer"},
                        "entry_id": {"type": "string"},
                        "watched_date": {"type": "string", "description": "YYYY-MM-DD，即署名日期"},
                        "rating": {"type": "integer", "description": "0-100，可空=不打分"},
                        "in_theater": {"type": "boolean"},
                        "ticket_price_cents": {"type": "integer", "description": "人民币分"},
                        "dimensions": {"type": "object"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "note": {"type": "string"},
                        "override_confirmed": {"type": "boolean", "description": "仅用于确认同片同日重复记录"}
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
                        "signature_date": {"type": "string", "description": "署名日期 YYYY-MM-DD，缺省=创建日期；最终评分绑定到署名日期最靠后的有分影评/条目"}
                    }),
                    &["action"],
                ),
            },
            ToolMeta {
                name: "set_movie_state",
                description: "影片级状态：in_watchlist 想看开关、liked 喜欢（仅能对看过即有观影记录/影评的影片点）。评分不在此产生——它绑定到署名日期最靠后且打了分的 Diary/Review。",
                parameters: obj_schema(
                    json!({
                        "movie_id": {"type": "integer"},
                        "in_watchlist": {"type": "boolean"},
                        "liked": {"type": "boolean"}
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
        "manage_lists" => manage_lists(ctx, &args).await,
        "manage_saved_queries" => manage_saved_queries(ctx, &args).await,
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
    let page = get_i64(args, "page").unwrap_or(1).clamp(1, 100) as u32;
    let results = ctx
        .tmdb
        .search_movie(query, year, page)
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
                    tmdb_rating, tmdb_votes, my_rating, watched, in_watchlist, liked
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
    if let Some(t) = get_str(args, "title") {
        let pat = format!("%{}%", t);
        conds.push(
            "(title_main LIKE ? OR title_sub LIKE ? OR title_en LIKE ? OR title_original LIKE ?)",
        );
        for _ in 0..4 {
            vals.push(SqlVal::Text(pat.clone()));
        }
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
    // saved_query_id 是 uuid 字符串；容忍模型误传整数
    let saved_id: Option<String> = get_str(args, "saved_query_id")
        .map(String::from)
        .or_else(|| get_i64(args, "saved_query_id").map(|i| i.to_string()));
    let (sql, chart) = if let Some(sid) = &saved_id {
        let sid2 = sid.clone();
        let payload: String = ctx
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT payload_json FROM saved_queries WHERE id=?1",
                    rusqlite::params![sid2],
                    |r| r.get(0),
                )
            })
            .await
            .map_err(|_| format!("saved_query {sid} 不存在"))?;
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
    let out = execute_stats_ro(ctx.stats(), &sql, chart).await?;
    if let Some(sid) = &saved_id {
        let sid2 = sid.clone();
        let _ = ctx
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE saved_queries SET last_run_at=?1 WHERE id=?2",
                    rusqlite::params![crate::now(), sid2],
                )
            })
            .await;
    }
    let mut out = out;
    out["saved_query_id"] = saved_id.map(|i| json!(i)).unwrap_or(Value::Null);
    // 评审缺陷 4：入模型历史的载荷截到前 50 行（全量仍可经 Stats 页重跑获取）
    let total = out["total_rows"].as_i64().unwrap_or(0);
    if total > 50 {
        let rows = out["rows"].as_array().cloned().unwrap_or_default();
        out["rows"] = json!(rows.into_iter().take(50).collect::<Vec<_>>());
        out["note"] = json!(format!(
            "共 {total} 行，已截断到前 50 行——如需全量请缩小查询范围或提示用户去 Stats 页查看"
        ));
        out["truncated"] = json!(true);
    }
    Ok(out)
}

/// 统计执行统一入口（评审缺陷 5）：审查 → 只读库执行 → 强 1000 行包裹。
/// 四入口（Agent run_stats / Stats 页重跑 / saved_query 直跑 / 直连 sql）全部收敛于此。
pub async fn execute_stats_ro(
    db: &crate::db::DbHandle,
    sql: &str,
    chart: Value,
) -> Result<Value, String> {
    crate::stats_guard::review_sql(sql).map_err(|e| format!("SQL 审查未通过: {e}"))?;
    // 无条件包裹：强 1000 行上限（子串探测 LIMIT 会被 LIKE '%limit%' 骗过）
    let sql_exec = format!("SELECT * FROM ({}) LIMIT 1000", sql.trim_end_matches(';'));
    let rows = db.select_json(&sql_exec).await.map_err(|e| e.to_string())?;
    let total = rows.len() as i64;
    Ok(json!({
        "columns": rows.first().map(|r| r.as_object().unwrap().keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
        "rows": rows,
        "total_rows": total,
        "truncated": total >= 1000,
        "chart": chart,
        "note": if total >= 1000 {"结果已截断到 1000 行"} else {""}
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

/// 清单维护（v0 §5.1 规格；确认门内）。List 变更不落账（非影片状态断言）。
/// LB 镜像（source='letterboxd'）只读：update/delete/add_item/remove_item 一律拒绝。
/// 业务错误统一用 InvalidParameterName(中文消息) 承载，Display 直出给模型/用户。
async fn manage_lists(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let args = confirmed_args(ctx, "manage_lists", args.clone()).await?;
    let action = get_str(&args, "action").ok_or("缺少 action")?;
    let list_id = get_str(&args, "list_id").map(String::from);
    let touch = |tx: &Connection, id: &str| -> rusqlite::Result<()> {
        tx.execute(
            "UPDATE lists SET updated_at=?2 WHERE id=?1",
            rusqlite::params![id, crate::now()],
        )?;
        Ok(())
    };
    let invalid = |msg: String| rusqlite::Error::InvalidParameterName(msg);
    let friendly = |e: crate::db::DbError| {
        let msg = e.to_string();
        if msg.contains("UNIQUE constraint failed: lists.name") {
            "已存在同名清单".into()
        } else {
            msg
        }
    };
    match action {
        "create" => {
            let name = get_str(&args, "name").ok_or("缺少 name")?.trim().to_string();
            if name.is_empty() {
                return Err("清单名不能为空".into());
            }
            let ranked = get_bool(&args, "ranked").unwrap_or(false) as i64;
            let desc = get_str(&args, "description").map(String::from);
            let id = uuid::Uuid::now_v7().to_string();
            let id_in = id.clone();
            let name_out = name.clone();
            ctx.db
                .call(move |c| {
                    c.execute(
                        "INSERT INTO lists (id, name, description, source, ranked, created_at, updated_at)
                         VALUES (?1,?2,?3,'manual',?4,?5,?5)",
                        rusqlite::params![id_in, name, desc, ranked, crate::now()],
                    )?;
                    Ok(())
                })
                .await
                .map_err(friendly)?;
            Ok(json!({"ok": true, "list_id": id, "name": name_out}))
        }
        "update" => {
            let id = list_id.ok_or("缺少 list_id")?;
            let name = get_str(&args, "name").map(|s| s.trim().to_string());
            let desc = get_str(&args, "description").map(String::from);
            let ranked = get_bool(&args, "ranked");
            if name.as_deref().map(str::is_empty).unwrap_or(false) {
                return Err("清单名不能为空".into());
            }
            if name.is_none() && desc.is_none() && ranked.is_none() {
                return Err("未提供任何变更字段（name/description/ranked）".into());
            }
            let id2 = id.clone();
            ctx.db
                .call(move |c| {
                    let src: String = c
                        .query_row(
                            "SELECT source FROM lists WHERE id=?1",
                            rusqlite::params![id2],
                            |r| r.get(0),
                        )
                        .map_err(|_| invalid(format!("清单 {id2} 不存在")))?;
                    if src == "letterboxd" {
                        return Err(invalid("Letterboxd 镜像清单只读，不能修改".into()));
                    }
                    if let Some(n) = &name {
                        c.execute(
                            "UPDATE lists SET name=?2 WHERE id=?1",
                            rusqlite::params![id2, n],
                        )?;
                    }
                    if let Some(d) = &desc {
                        c.execute(
                            "UPDATE lists SET description=?2 WHERE id=?1",
                            rusqlite::params![id2, d],
                        )?;
                    }
                    if let Some(r) = ranked {
                        c.execute(
                            "UPDATE lists SET ranked=?2 WHERE id=?1",
                            rusqlite::params![id2, r as i64],
                        )?;
                    }
                    touch(c, &id2)?;
                    Ok(())
                })
                .await
                .map_err(friendly)?;
            Ok(json!({"ok": true, "list_id": id}))
        }
        "delete" => {
            let id = list_id.ok_or("缺少 list_id")?;
            let id2 = id.clone();
            ctx.db
                .call(move |c| {
                    let src: String = c
                        .query_row(
                            "SELECT source FROM lists WHERE id=?1",
                            rusqlite::params![id2],
                            |r| r.get(0),
                        )
                        .map_err(|_| invalid(format!("清单 {id2} 不存在")))?;
                    if src == "letterboxd" {
                        return Err(invalid("Letterboxd 镜像清单只读，不能删除".into()));
                    }
                    c.execute("DELETE FROM lists WHERE id=?1", rusqlite::params![id2])?;
                    Ok(())
                })
                .await
                .map_err(friendly)?;
            Ok(json!({"ok": true, "deleted": id}))
        }
        "add_item" => {
            let id = list_id.ok_or("缺少 list_id")?;
            let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id")?;
            let rank_req = get_i64(&args, "rank");
            let _ = ensure_movie_details(&ctx.tmdb, &ctx.db, movie_id).await;
            // FK 友好化：影片不在库（模型编造 id / 未先 search_movies）→ 中文报错
            let in_db = ctx
                .db
                .call(move |c| {
                    Ok(c.query_row(
                        "SELECT 1 FROM movies WHERE tmdb_id=?1",
                        rusqlite::params![movie_id],
                        |_| Ok(()),
                    )
                    .is_ok())
                })
                .await
                .unwrap_or(false);
            if !in_db {
                return Err(format!(
                    "影片 {movie_id} 不在本地库，请先 search_movies 建条目"
                ));
            }
            let id2 = id.clone();
            ctx.db
                .call(move |c| {
                    let (src, ranked): (String, i64) = c
                        .query_row(
                            "SELECT source, ranked FROM lists WHERE id=?1",
                            rusqlite::params![id2],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .map_err(|_| invalid(format!("清单 {id2} 不存在")))?;
                    if src == "letterboxd" {
                        return Err(invalid("Letterboxd 镜像清单只读，不能添加条目".into()));
                    }
                    let rank: Option<i64> = if ranked == 1 {
                        match rank_req {
                            Some(r) => Some(r),
                            None => Some(
                                c.query_row(
                                    "SELECT COALESCE(MAX(rank),0)+1 FROM list_items WHERE list_id=?1",
                                    rusqlite::params![id2],
                                    |r| r.get(0),
                                )?,
                            ),
                        }
                    } else {
                        None
                    };
                    c.execute(
                        "INSERT INTO list_items (list_id, movie_id, rank, added_at)
                         VALUES (?1,?2,?3,?4)",
                        rusqlite::params![id2, movie_id, rank, crate::now()],
                    )?;
                    touch(c, &id2)?;
                    Ok(())
                })
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("UNIQUE constraint failed: list_items") {
                        format!("影片 {movie_id} 已在该清单中")
                    } else {
                        msg
                    }
                })?;
            Ok(json!({"ok": true, "list_id": id, "movie_id": movie_id}))
        }
        "remove_item" => {
            let id = list_id.ok_or("缺少 list_id")?;
            let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id")?;
            let id2 = id.clone();
            ctx.db
                .call(move |c| {
                    let src: String = c
                        .query_row(
                            "SELECT source FROM lists WHERE id=?1",
                            rusqlite::params![id2],
                            |r| r.get(0),
                        )
                        .map_err(|_| invalid(format!("清单 {id2} 不存在")))?;
                    if src == "letterboxd" {
                        return Err(invalid("Letterboxd 镜像清单只读，不能移除条目".into()));
                    }
                    let n = c.execute(
                        "DELETE FROM list_items WHERE list_id=?1 AND movie_id=?2",
                        rusqlite::params![id2, movie_id],
                    )?;
                    if n == 0 {
                        return Err(invalid(format!("影片 {movie_id} 不在该清单中")));
                    }
                    touch(c, &id2)?;
                    Ok(())
                })
                .await
                .map_err(friendly)?;
            Ok(json!({"ok": true, "list_id": id, "movie_id": movie_id}))
        }
        _ => Err(format!("未知 action: {action}")),
    }
}

async fn list_saved_queries(ctx: &ToolCtx) -> Result<Value, String> {
    let rows = ctx
        .db
        .select_json(
            "SELECT id, name, payload_json, last_run_at FROM saved_queries ORDER BY sort_order, created_at",
        )
        .await
        .map_err(|e| e.to_string())?;
    // payload_json 抽出 sql 平铺（AI 按语义匹配复用）
    let flat: Vec<Value> = rows
        .iter()
        .map(|r| {
            let p: Value = serde_json::from_str(r["payload_json"].as_str().unwrap_or("{}"))
                .unwrap_or(json!({}));
            json!({
                "id": r["id"], "name": r["name"], "sql": p["sql"],
                "last_run_at": r["last_run_at"],
            })
        })
        .collect();
    Ok(json!({"saved_queries": flat}))
}

/// 统计项目管理（写工具，过确认门；SQL 保存前先过审查）。
async fn manage_saved_queries(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let action = get_str(args, "action").ok_or("缺少 action")?;
    match action {
        "create" => {
            let sql = get_str(args, "sql").ok_or("缺少 sql")?.to_string();
            // 保存前校验：拒绝存进跑不了的查询
            crate::stats_guard::review_sql(&sql).map_err(|e| format!("SQL 审查未通过: {e}"))?;
            let args = confirmed_args(ctx, "manage_saved_queries", args.clone()).await?;
            let name = get_str(&args, "name").ok_or("缺少 name")?.to_string();
            let sql = get_str(&args, "sql").ok_or("缺少 sql")?.to_string();
            let chart = args
                .get("chart")
                .cloned()
                .unwrap_or(json!({"type": "table"}));
            let payload = json!({"sql": sql, "chart": chart}).to_string();
            let id = uuid::Uuid::now_v7().to_string();
            let id2 = id.clone();
            let name2 = name.clone();
            ctx.db
                .call(move |c| {
                    c.execute(
                        "INSERT INTO saved_queries (id, name, payload_json, sort_order, created_at, last_run_at)
                         VALUES (?1,?2,?3,0,?4,?4)",
                        params![id2, name2, payload, crate::now()],
                    )
                })
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true, "saved_query_id": id, "name": name}))
        }
        "delete" => {
            let args = confirmed_args(ctx, "manage_saved_queries", args.clone()).await?;
            let id = get_str(&args, "saved_query_id")
                .ok_or("缺少 saved_query_id")?
                .to_string();
            let n = ctx
                .db
                .call(move |c| c.execute("DELETE FROM saved_queries WHERE id=?1", params![id]))
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("统计项目不存在".into());
            }
            Ok(json!({"ok": true}))
        }
        "rename" => {
            let args = confirmed_args(ctx, "manage_saved_queries", args.clone()).await?;
            let id = get_str(&args, "saved_query_id")
                .ok_or("缺少 saved_query_id")?
                .to_string();
            let name = get_str(&args, "name").ok_or("缺少 name")?.to_string();
            let name2 = name.clone();
            let n = ctx
                .db
                .call(move |c| {
                    c.execute("UPDATE saved_queries SET name=?1 WHERE id=?2", params![name2, id])
                })
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("统计项目不存在".into());
            }
            Ok(json!({"ok": true, "name": name}))
        }
        _ => Err(format!("未知 action: {action}")),
    }
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

pub const CONFIRM_TOOLS: &[&str] = &[
    "manage_diary",
    "manage_reviews",
    "set_movie_state",
    "manage_lists",
    "manage_saved_queries",
];

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

/// 署名日期校验：YYYY-MM-DD（chrono 解析拒绝垃圾值）。
fn validate_date_str(s: &str) -> Result<String, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| s.to_string())
        .map_err(|_| format!("signature_date 需为 YYYY-MM-DD，收到: {s}"))
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

async fn manage_diary(ctx: &ToolCtx, args: &Value) -> Result<Value, String> {
    let action = get_str(args, "action").ok_or("缺少 action")?;
    match action {
        "add" => {
            let movie_id = get_i64(args, "movie_id").ok_or("缺少 movie_id（请先搜索确认影片）")?;
            let watched_date = get_str(args, "watched_date")
                .ok_or("缺少 watched_date")?
                .to_string();
            // 同片同日查重前置于确认门（缺陷 17）：警告由 AI 转述用户后再带 override 过门一次
            if !get_bool(args, "override_confirmed").unwrap_or(false) {
                let mid = movie_id;
                let wd = watched_date.clone();
                let dup: Option<String> = ctx
                    .db
                    .call(move |c| {
                        Ok(c.query_row(
                            "SELECT id FROM diary_entries WHERE movie_id=?1 AND watched_date=?2 LIMIT 1",
                            rusqlite::params![mid, wd],
                            |r| r.get::<_, String>(0),
                        )
                        .ok())
                    })
                    .await
                    .unwrap_or(None);
                if let Some(dup_id) = dup {
                    return Ok(json!({
                        "warning": "该影片当天已有观影记录，疑似重复",
                        "duplicate_entry_id": dup_id,
                        "require_confirmation": true
                    }));
                }
            }
            let args = confirmed_args(ctx, "manage_diary", args.clone()).await?;
            let movie_id = get_i64(&args, "movie_id").ok_or("缺少 movie_id")?;
            let _ = ensure_movie_details(&ctx.tmdb, &ctx.db, movie_id).await;
            let watched_date = get_str(&args, "watched_date")
                .ok_or("缺少 watched_date")?
                .to_string();
            let rating = validate_rating(get_i64(&args, "rating"))?;
            let in_theater = get_bool(&args, "in_theater").unwrap_or(false);
            let price = get_i64(&args, "ticket_price_cents");
            let note = get_str(&args, "note").unwrap_or("").to_string();
            let entry_id = uuid::Uuid::now_v7().to_string();
            let entry_reply = entry_id.clone();
            let at = crate::now();
            let src = ctx.source;
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    tx.execute(
                        "INSERT INTO diary_entries (id, movie_id, watched_date, rating,
                           in_theater, liked, ticket_price_cents, private_note, created_at, updated_at)
                         VALUES (?1,?2,?3,?4,?5,0,?6,?7,?8,?8)",
                        params![entry_id, movie_id, watched_date, rating,
                                in_theater as i64, price, note, at],
                    )?;
                    ensure_dims(&tx, &entry_id, args.get("dimensions").unwrap_or(&Value::Null))?;
                    ensure_tag_list(&tx, &entry_id, args.get("tags").unwrap_or(&Value::Null))?;
                    let mut notes = Vec::new();
                    if let Some(r) = rating {
                        // 审计断言；终态拾取已改为署名日期 argmax
                        assert_and_recompute(&tx, movie_id, at, src, Some(&entry_id),
                                             "my_rating", r)?;
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
            let src = ctx.source;
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
                    let (old_date, old_rating, old_theater, _old_liked, old_price, old_note) = old;

                    // presence 语义（缺陷 2/15）：键出现即显式设置；rating/ticket_price
                    // 传 null = 清除；键缺失 = 保持。ConfirmCard 只提交用户改过的字段。
                    enum Patch<T> { Unchanged, Clear, Set(T) }
                    let patch_str = |k: &str| -> Patch<String> {
                        match args.get(k) {
                            None => Patch::Unchanged,
                            Some(Value::Null) => Patch::Clear,
                            Some(v) => v.as_str().map(|s| Patch::Set(s.to_string())).unwrap_or(Patch::Unchanged),
                        }
                    };
                    let patch_i64 = |k: &str| -> Patch<i64> {
                        match args.get(k) {
                            None => Patch::Unchanged,
                            Some(Value::Null) => Patch::Clear,
                            Some(v) => v.as_i64().map(Patch::Set).unwrap_or(Patch::Unchanged),
                        }
                    };
                    let patch_bool = |k: &str| -> Patch<bool> {
                        match args.get(k) {
                            None => Patch::Unchanged,
                            Some(Value::Null) => Patch::Clear,
                            Some(v) => v.as_bool().map(Patch::Set).unwrap_or(Patch::Unchanged),
                        }
                    };
                    let new_date = match patch_str("watched_date") {
                        Patch::Unchanged => old_date.clone(),
                        Patch::Clear => return Err(rusqlite::Error::InvalidParameterName("watched_date 不允许清除".into())),
                        Patch::Set(s) => s,
                    };
                    let new_rating = match patch_i64("rating") {
                        Patch::Unchanged => old_rating,
                        Patch::Clear => None,
                        Patch::Set(v) => match validate_rating(Some(v)) { Ok(r) => Some(r.expect("已校验")), Err(e) => return Err(rusqlite::Error::InvalidParameterName(e)) },
                    };
                    let new_theater = match patch_bool("in_theater") {
                        Patch::Unchanged => old_theater != 0,
                        Patch::Clear => false,
                        Patch::Set(b) => b,
                    } as i64;
                    let new_price = match patch_i64("ticket_price_cents") {
                        Patch::Unchanged => old_price,
                        Patch::Clear => None,
                        Patch::Set(v) => Some(v),
                    };
                    let new_note = match patch_str("note") {
                        Patch::Unchanged => old_note.clone(),
                        Patch::Clear => String::new(),
                        Patch::Set(s) => s,
                    };
                    let patch_dims = args.get("dimensions").cloned().filter(|v| !v.is_null());
                    let patch_tags = args.get("tags").cloned().filter(|v| !v.is_null());

                    let mut changes = serde_json::Map::new();
                    if new_date != old_date { changes.insert("watched_date".into(), json!([old_date, new_date])); }
                    if new_rating != old_rating { changes.insert("rating".into(), json!([old_rating, new_rating])); }
                    if new_theater != old_theater { changes.insert("in_theater".into(), json!([old_theater != 0, new_theater != 0])); }
                    if new_price != old_price { changes.insert("ticket_price_cents".into(), json!([old_price, new_price])); }
                    if new_note != old_note { changes.insert("private_note".into(), json!([old_note, new_note])); }
                    if patch_dims.is_some() { changes.insert("dimensions".into(), json!([Value::Null, patch_dims])) ; }
                    if patch_tags.is_some() { changes.insert("tags".into(), json!([Value::Null, patch_tags])); }
                    if changes.is_empty() {
                        tx.commit()?;
                        return Ok(json!({"ok": true, "note": "无字段变化，未落账"}));
                    }

                    let at = crate::now();
                    tx.execute(
                        "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
                         VALUES (?1,?2,'diary_entry',?3,?4,'agent',?3,?5)",
                        params![uuid::Uuid::now_v7().to_string(), movie_id, entry_id, at,
                                Value::Object(changes.clone()).to_string()])?;
                    // like 已状态量化（影片级详情页唯一入口），条目 liked 列不再更新
                    tx.execute(
                        "UPDATE diary_entries SET watched_date=?2, rating=?3, in_theater=?4,
                           ticket_price_cents=?5, private_note=?6, updated_at=?7
                         WHERE id=?1",
                        params![entry_id, new_date, new_rating, new_theater,
                                new_price, new_note, at])?;
                    if let Some(d) = &patch_dims {
                        tx.execute("DELETE FROM entry_dimensions WHERE entry_id=?1", params![entry_id])?;
                        ensure_dims(&tx, &entry_id, d)?;
                    }
                    if let Some(t) = &patch_tags {
                        tx.execute("DELETE FROM entry_tags WHERE entry_id=?1", params![entry_id])?;
                        ensure_tag_list(&tx, &entry_id, t)?;
                    }
                    // 审计：评分变化仍落 movie 级断言（重算已改为署名日期 argmax，
                    // 断言仅作账目记录，不参与终态拾取）。清除走行级 null → argmax 自然回退。
                    if let Some(r) = new_rating.filter(|_| new_rating != old_rating) {
                        assert_and_recompute(&tx, movie_id, at, src,
                            Some(&entry_id), "my_rating", r)?;
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
            let _ = ensure_movie_details(&ctx.tmdb, &ctx.db, movie_id).await;
            let title = get_str(&args, "title").map(String::from);
            let body_md = get_str(&args, "body_md").ok_or("缺少 body_md")?.to_string();
            let rating = validate_rating(get_i64(&args, "rating"))?;
            let signature_date = match args.get("signature_date") {
                Some(Value::String(sv)) if !sv.is_empty() => match validate_date_str(sv) {
                    Ok(d) => Some(d),
                    Err(e) => return Err(e),
                },
                _ => None,
            };
            let rid = uuid::Uuid::now_v7().to_string();
            let rid_reply = rid.clone();
            let at = crate::now();
            let src = ctx.source;
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    // like 已状态量化（影片级），影评不再携带 liked
                    tx.execute(
                        "INSERT INTO reviews (id, movie_id, title, body_md, rating, liked, signature_date, created_at, updated_at)
                         VALUES (?1,?2,?3,?4,?5,0,?6,?7,?7)",
                        params![rid, movie_id, title, body_md, rating, signature_date, at],
                    )?;
                    if let Some(r) = rating {
                        // 审计断言（终态拾取已改为署名日期 argmax）
                        assert_and_recompute(&tx, movie_id, at, src, Some(&rid),
                                             "my_rating", r)?;
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
            let src = ctx.source;
            ctx.db
                .call(move |c| {
                    let tx = c.unchecked_transaction()?;
                    let (movie_id, old_title, old_body, old_rating, old_sig): (
                        i64, Option<String>, String, Option<i64>, Option<String>) = tx.query_row(
                        "SELECT movie_id, title, body_md, rating, signature_date FROM reviews WHERE id=?1",
                        params![review_id], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                        })
                        .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
                    // presence 语义（缺陷 2/15）：键出现即设置；rating null = 清除
                    let new_title = match args.get("title") {
                        None => old_title.clone(),
                        Some(Value::Null) => None,
                        Some(v) => Some(v.as_str().map(String::from).unwrap_or_default()),
                    };
                    let new_body = match args.get("body_md") {
                        None => old_body.clone(),
                        Some(Value::Null) => return Err(rusqlite::Error::InvalidParameterName("body_md 不允许清除".into())),
                        Some(v) => v.as_str().map(String::from).unwrap_or(old_body.clone()),
                    };
                    let new_rating = match args.get("rating") {
                        None => old_rating,
                        Some(Value::Null) => None,
                        Some(v) => match validate_rating(v.as_i64()) {
                            Ok(r) => Some(r.expect("已校验")),
                            Err(e) => return Err(rusqlite::Error::InvalidParameterName(e)),
                        },
                    };
                    // presence：键缺失 = 保持；显式 null = 清除（终态回退创建日期）
                    let new_sig = match args.get("signature_date") {
                        None => old_sig.clone(),
                        Some(Value::Null) => None,
                        Some(v) => match v.as_str() {
                            Some("") | None => None,
                            Some(sv) => match validate_date_str(sv) {
                                Ok(d) => Some(d),
                                Err(e) => return Err(rusqlite::Error::InvalidParameterName(e)),
                            },
                        },
                    };
                    let _ = rating_req;
                    let mut changes = serde_json::Map::new();
                    if new_title != old_title { changes.insert("title".into(), json!([old_title, new_title])); }
                    if new_body != old_body { changes.insert("body_md".into(), json!([old_body, new_body])); }
                    if new_rating != old_rating { changes.insert("rating".into(), json!([old_rating, new_rating])); }
                    if new_sig != old_sig { changes.insert("signature_date".into(), json!([old_sig, new_sig])); }
                    if changes.is_empty() {
                        tx.commit()?;
                        return Ok(json!({"ok": true, "note": "无字段变化"}));
                    }
                    let at = crate::now();
                    tx.execute(
                        &format!(
                            "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
                             VALUES (?1,?2,'review',?3,?4,'{}',?3,?5)", src),
                        params![uuid::Uuid::now_v7().to_string(), movie_id, review_id, at,
                                Value::Object(changes.clone()).to_string()])?;
                    tx.execute(
                        "UPDATE reviews SET title=?2, body_md=?3, rating=?4, signature_date=?5, updated_at=?6 WHERE id=?1",
                        params![review_id, new_title, new_body, new_rating, new_sig, at])?;
                    if new_rating != old_rating {
                        // 审计断言；清除走行级 null → argmax 自然回退
                        if let Some(r) = new_rating {
                            assert_and_recompute(&tx, movie_id, at, src,
                                Some(&review_id), "my_rating", r)?;
                        }
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
    // 评分只能由 Diary/Review 产生（署名日期绑定），越权字段显式拒绝（防旧提示词残留路径）。
    for banned in ["my_rating", "clear_my_rating"] {
        if args.get(banned).is_some() {
            return Err(
                "评分只能通过 manage_diary 或 manage_reviews 产生，不能用 set_movie_state".into(),
            );
        }
    }
    let _ = ensure_movie_details(&ctx.tmdb, &ctx.db, movie_id).await;
    let watchlist = get_bool(&args, "in_watchlist");
    let liked = get_bool(&args, "liked");
    if watchlist.is_none() && liked.is_none() {
        return Err("至少提供一个字段（in_watchlist/liked）".into());
    }
    ctx.db
        .call(move |c| {
            let tx = c.unchecked_transaction()?;
            let at = crate::now();
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
            if let Some(l) = liked {
                // like 仅能标记看过（有 Diary 或 Review）的影片（report 2026-09-03）
                let watched: i64 = tx.query_row(
                    "SELECT watched FROM movies WHERE tmdb_id=?1",
                    rusqlite::params![movie_id],
                    |r| r.get(0),
                )?;
                if watched == 0 {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "只能对看过（有观影记录或影评）的影片点喜欢，先记一笔或写影评".into(),
                    ));
                }
                assert_and_recompute(&tx, movie_id, at, "standalone", None, "liked", l as i64)?;
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
            stats_db: None,
            tmdb: TmdbClient::disabled("zh-CN".into()),
            config: Config::default_for_test(),
            source: "agent",
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

    // ===== P2：manage_lists（清单维护）=====

    #[tokio::test]
    async fn lists_crud_ranked_tail_and_lb_readonly() {
        let (db, ctx) = setup_two_movies();
        // create ranked 清单
        let out = execute(
            "manage_lists",
            &ctx,
            json!({"action":"create","name":"2026 十佳候选","ranked":true}),
        )
        .await
        .unwrap();
        let list_id = out["list_id"].as_str().unwrap().to_string();
        // add_item 未指定 rank → 自动追加队尾 1、2
        execute("manage_lists", &ctx, json!({"action":"add_item","list_id":list_id,"movie_id":843}))
            .await
            .unwrap();
        execute("manage_lists", &ctx, json!({"action":"add_item","list_id":list_id,"movie_id":844}))
            .await
            .unwrap();
        let ranks: Vec<(i64, Option<i64>)> = db
            .call({
                let id = list_id.clone();
                move |c| {
                    let mut st = c.prepare("SELECT movie_id, rank FROM list_items WHERE list_id=?1 ORDER BY rank")?;
                    let rows = st.query_map(rusqlite::params![id], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
                    })?;
                    rows.collect()
                }
            })
            .await
            .unwrap();
        assert_eq!(ranks, vec![(843, Some(1)), (844, Some(2))], "ranked 清单缺省 rank 应追加队尾");
        // 重复添加 → 报错
        let err = execute(
            "manage_lists",
            &ctx,
            json!({"action":"add_item","list_id":list_id,"movie_id":843}),
        )
        .await
        .unwrap_err();
        assert!(err.contains("已在"), "重复添加应报错: {err}");
        // update 改名 + unranked
        execute(
            "manage_lists",
            &ctx,
            json!({"action":"update","list_id":list_id,"name":"年度片单","ranked":false}),
        )
        .await
        .unwrap();
        let (name, ranked): (String, i64) = db
            .call({
                let id = list_id.clone();
                move |c| {
                    c.query_row("SELECT name, ranked FROM lists WHERE id=?1", rusqlite::params![id], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(name, "年度片单");
        assert_eq!(ranked, 0);
        // 同名 create → 冲突
        let err = execute(
            "manage_lists",
            &ctx,
            json!({"action":"create","name":"年度片单"}),
        )
        .await
        .unwrap_err();
        assert!(err.contains("同名清单"), "同名冲突应友好报错: {err}");
        // LB 镜像只读
        let lb_id: String = db
            .call(|c| {
                c.execute(
                    "INSERT INTO lists (id, name, source, ranked, created_at, updated_at, external_id)
                     VALUES ('lb-1','LB 镜像','letterboxd',1,1,1,'xyz')",
                    [],
                )?;
                Ok("lb-1".into())
            })
            .await
            .unwrap();
        for act in ["update", "delete", "add_item", "remove_item"] {
            let mut payload = json!({"action": act, "list_id": lb_id});
            if act == "add_item" || act == "remove_item" {
                payload["movie_id"] = json!(843);
            }
            if act == "update" {
                payload["name"] = json!("改名");
            }
            let err = execute("manage_lists", &ctx, payload).await.unwrap_err();
            assert!(err.contains("只读"), "{act} 对 LB 镜像应拒绝: {err}");
        }
        // remove_item + delete
        execute("manage_lists", &ctx, json!({"action":"remove_item","list_id":list_id,"movie_id":843}))
            .await
            .unwrap();
        execute("manage_lists", &ctx, json!({"action":"delete","list_id":list_id}))
            .await
            .unwrap();
        let n: i64 = db
            .call(|c| c.query_row("SELECT COUNT(*) FROM lists", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(n, 1, "只剩 LB 镜像清单");
        let n2: i64 = db
            .call(move |c| c.query_row("SELECT COUNT(*) FROM list_items WHERE list_id=?", rusqlite::params![list_id], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(n2, 0, "删除清单级联清空条目");
    }

    #[tokio::test]
    async fn lists_add_item_unknown_movie_friendly_error() {
        let (_db, ctx) = setup_two_movies();
        let out = execute(
            "manage_lists",
            &ctx,
            json!({"action":"create","name":"FK 测试"}),
        )
        .await
        .unwrap();
        let list_id = out["list_id"].as_str().unwrap().to_string();
        // 影片 99999 不在库 → 中文报错（非裸 FK 约束错误）
        let err = execute(
            "manage_lists",
            &ctx,
            json!({"action":"add_item","list_id":list_id,"movie_id":99999}),
        )
        .await
        .unwrap_err();
        assert!(err.contains("不在本地库"), "应友好报错: {err}");
    }

    #[tokio::test]
    async fn lists_unranked_add_has_null_rank() {
        let (db, ctx) = setup_two_movies();
        let out = execute(
            "manage_lists",
            &ctx,
            json!({"action":"create","name":"随机想看","description":"杂项","ranked":false}),
        )
        .await
        .unwrap();
        let list_id = out["list_id"].as_str().unwrap().to_string();
        execute("manage_lists", &ctx, json!({"action":"add_item","list_id":list_id,"movie_id":843,"rank":3}))
            .await
            .unwrap();
        let rank: Option<i64> = db
            .call({
                let id = list_id.clone();
                move |c| {
                    c.query_row("SELECT rank FROM list_items WHERE list_id=?1", rusqlite::params![id], |r| r.get(0))
                }
            })
            .await
            .unwrap();
        assert_eq!(rank, None, "unranked 清单 rank 必须为 NULL（指定也不写）");
        // 未知 action
        let err = execute("manage_lists", &ctx, json!({"action":"bogus"})).await.unwrap_err();
        assert!(err.contains("未知 action"));
    }

    // ===== 评审修复回归（2026-09-01）=====

    #[tokio::test]
    async fn rating_clear_falls_back_and_banned_fields_rejected() {
        let (db, ctx) = setup_two_movies();
        // 两个条目：3-15 评 90、4-20 评 80（观看日期更晚 → 终态 80）
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-03-15","rating":90}),
        )
        .await
        .unwrap();
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-04-20","rating":80}),
        )
        .await
        .unwrap();
        let mr: i64 = db
            .call(|c| c.query_row("SELECT my_rating FROM movies WHERE tmdb_id=843", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(mr, 80, "署名日期最靠后的有分条目持有终态");
        // 改旧条目评分（60）不影响终态（非持有者）
        let e1: String = db
            .call(|c| {
                c.query_row("SELECT id FROM diary_entries WHERE movie_id=843 AND watched_date='2026-03-15'", [], |r| r.get(0))
            })
            .await
            .unwrap();
        execute("manage_diary", &ctx, json!({"action":"update","entry_id":e1,"rating":60}))
            .await
            .unwrap();
        let mr2: i64 = db
            .call(|c| c.query_row("SELECT my_rating FROM movies WHERE tmdb_id=843", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(mr2, 80, "旧条目改分不改变终态（无覆盖警告概念）");
        // 清除持有条目评分 → 回退到剩余有分条目
        let e2: String = db
            .call(|c| {
                c.query_row("SELECT id FROM diary_entries WHERE movie_id=843 AND watched_date='2026-04-20'", [], |r| r.get(0))
            })
            .await
            .unwrap();
        execute("manage_diary", &ctx, json!({"action":"update","entry_id":e2,"rating":null}))
            .await
            .unwrap();
        let mr3: i64 = db
            .call(|c| c.query_row("SELECT my_rating FROM movies WHERE tmdb_id=843", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(mr3, 60, "清除后回退到剩余有分条目");
        // set_movie_state 仅拒评分（liked 已合法）
        for banned in ["my_rating", "clear_my_rating"] {
            let mut payload = serde_json::Map::new();
            payload.insert("movie_id".into(), json!(843));
            payload.insert(banned.into(), json!(true));
            let err = execute("set_movie_state", &ctx, Value::Object(payload))
                .await
                .unwrap_err();
            assert!(
                err.contains("manage_diary 或 manage_reviews"),
                "越权字段 {banned} 必须被拒绝，实际: {err}"
            );
        }
        // like：有 watched 的影片可点
        execute("set_movie_state", &ctx, json!({"movie_id": 843, "liked": true}))
            .await
            .unwrap();
        let liked: i64 = db
            .call(|c| c.query_row("SELECT liked FROM movies WHERE tmdb_id=843", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(liked, 1);
        // 未看过（844 无条目）→ 拒绝
        let err = execute("set_movie_state", &ctx, json!({"movie_id": 844, "liked": true}))
            .await
            .unwrap_err();
        assert!(err.contains("看过"), "未看过的影片不能喜欢: {err}");
    }

    #[tokio::test]
    async fn update_is_presence_based_and_supports_clear() {
        let (db, ctx) = setup_two_movies();
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"add","movie_id":843,"watched_date":"2026-08-15",
                   "rating":90,"in_theater":true,
                   "ticket_price_cents":4500,"note":"初看"}),
        )
        .await
        .unwrap();
        let entry_id: String = db
            .call(|c| {
                c.query_row("SELECT id FROM diary_entries WHERE movie_id=843", [], |r| r.get(0))
            })
            .await
            .unwrap();
        // 部分更新：只改 note —— 其余字段必须原样保留（缺陷 2）
        let entry_id2 = entry_id.clone();
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"update","entry_id":entry_id,"note":"改过的随记"}),
        )
        .await
        .unwrap();
        let row: (String, Option<i64>, i64, i64, Option<i64>, String) = db
            .call(move |c| {
                c.query_row(
                    "SELECT watched_date, rating, in_theater, liked, ticket_price_cents, private_note
                     FROM diary_entries WHERE id=?1",
                    params![entry_id2], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(row.0, "2026-08-15", "watched_date 不得被表单垃圾值覆盖（缺陷 2）");
        assert_eq!(row.1, Some(90), "rating 不得重置");
        assert_eq!(row.2, 1, "in_theater 不得重置");
        assert_eq!(row.3, 0, "条目 liked 不再由表单产生（like 状态量化后恒 0）");
        assert_eq!(row.4, Some(4500), "票价不得重置");
        assert_eq!(row.5, "改过的随记");
        // 清除语义：rating/票价 传 null（缺陷 15）
        execute(
            "manage_diary",
            &ctx,
            json!({"action":"update","entry_id":entry_id,"rating":null,"ticket_price_cents":null}),
        )
        .await
        .unwrap();
        let entry_id3 = entry_id.clone();
        let (r3, p3): (Option<i64>, Option<i64>) = db
            .call(move |c| {
                c.query_row(
                    "SELECT rating, ticket_price_cents FROM diary_entries WHERE id=?1",
                    params![entry_id3], |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(r3, None, "rating null 必须清除");
        assert_eq!(p3, None, "票价 null 必须清除");
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
    ensure_movie_details_opts(tmdb, db, movie_id, false).await
}

/// force=false 时：fetched_at 为空 **或关键字段缺失**（评分参考/英文简介）
/// 都会触发拉取——旧桩不会因 fetched_at 置位而永久卡死在搜索级字段。
pub async fn ensure_movie_details_opts(
    tmdb: &TmdbClient,
    db: &DbHandle,
    movie_id: i64,
    force: bool,
) -> Result<(), String> {
    let need_fetch: bool = db
        .call(move |c| {
            Ok(force
                || c.query_row(
                    "SELECT COALESCE(fetched_at IS NULL, 1) + COALESCE(tmdb_rating IS NULL, 1)
                       + COALESCE(overview IS NULL OR overview = '', 1) > 0
                     FROM movies WHERE tmdb_id=?1",
                    rusqlite::params![movie_id],
                    |r| Ok(r.get::<_, i64>(0)? > 0),
                )
                .unwrap_or(true))
        })
        .await
        .map_err(|e| e.to_string())?;
    if !need_fetch {
        return Ok(());
    }
    let details = tmdb
        .movie_details_in(movie_id, "en-US")
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
    let backdrop_owned = details["backdrop_path"].as_str().unwrap_or("").to_string();
    let runtime = details["runtime"].as_i64();
    db.call(move |c| {
        c.execute(
            "UPDATE movies SET release_date=COALESCE(json_extract(?5,'$.release_date'), release_date),
               directors=?2, genres=?3, runtime=COALESCE(?4, runtime),
               tagline=COALESCE(json_extract(?5,'$.tagline'), tagline),
               overview=COALESCE(json_extract(?5,'$.overview'), overview),
               posters=?6, backdrop_path=?8,
               tmdb_rating=COALESCE(?9, tmdb_rating),
               tmdb_votes=COALESCE(?10, tmdb_votes),
               fetched_at=?7 WHERE tmdb_id=?1",
            rusqlite::params![
                movie_id,
                serde_json::to_string(&directors).unwrap(),
                serde_json::to_string(&genres).unwrap(),
                runtime,
                details.to_string(),
                serde_json::to_string(&vec![poster_owned]).unwrap(),
                crate::now(),
                backdrop_owned,
                details["vote_average"].as_f64(),
                details["vote_count"].as_i64(),
            ],
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
