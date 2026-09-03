//! Agent Loop：手写循环 + 停止条件（design.md §7.6 第 19 条）。
//! 上限 12 步 / 同工具连续失败 3 次熔断 / 预算预检 / CancellationToken 打断。

use crate::config::Config;
use crate::db::DbHandle;
use crate::llm::ChatClient;
use crate::tools::{self, ToolCtx, ToolRegistry};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

pub const MAX_STEPS: usize = 12;
pub const TOOL_FAIL_BREAKER: usize = 3;

/// 视图 Schema 字典（Prompt 第 ③ 层；静态文本，AI 写对 SQL 的前提）。
pub const SCHEMA_DICTIONARY: &str = r#"【统计视图字典（仅可查询以下视图）】
v_movies: tmdb_id, title_zh, title_en, title_original, title_main, title_sub, year, release_date, runtime, original_language, genres(JSON数组), directors(JSON数组), tagline, overview, tmdb_rating(社区评分10分制), tmdb_votes, my_rating(0-100可空), watched(0/1), in_watchlist, liked
v_diary_full: entry_id, movie_id, watched_date(YYYY-MM-DD), rating(0-100可空), in_theater, liked, ticket_price_cents(分), private_note, created_at(unix秒), rewatch_index(派生), title_main, title_sub, title_zh, title_en, year, runtime, genres, directors, my_rating, tags(JSON数组), dimensions_flat(JSON数组 [{dimension,name}]，dimension∈地点|场景|同伴)
v_reviews_full: review_id, movie_id, title, body_md, body_len, rating, liked, created_at, updated_at, title_zh, title_en, title_original, genres, directors, my_rating
v_actions: id, movie_id, target(movie|diary_entry|review), target_id, at(unix秒), source(edit|standalone|agent|import|system), changes_json, is_active(目标行存活=1)
v_logs: kind(watch|review|chat), id, movie_id, at(日期), brief
v_lists: list_id, name, source, ranked, external_id, movie_id, rank, added_at
v_canon: canon_key, canon_name, edition, rank, tmdb_id
要求：单条 SELECT；仅用以上视图；数值统计请用 SQL 聚合（禁止凭感觉报数）。"#;

#[derive(Debug, Clone)]
pub struct AgentEvent {
    pub kind: AgentEventKind,
}
#[derive(Debug, Clone)]
pub enum AgentEventKind {
    Token(String),
    ToolStart { name: String, args: String },
    ToolDone { name: String, ok: bool },
}

pub struct RunSummary {
    pub steps: usize,
    pub interrupted: bool,
    pub usage_tokens: (u64, u64), // prompt, completion（可中断缺失记 0）
    pub aborted_reason: Option<String>,
}

/// 本月起始 unix 秒（预算/用量汇总共用）。
pub fn month_start_pub() -> i64 {
    use chrono::TimeZone;
    let now = chrono::Utc::now();
    chrono::Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .earliest()
        .map(|d| d.timestamp())
        .unwrap_or_else(|| now.timestamp())
}
use chrono::Datelike;

/// 用量行成本换算（display_currency；缺陷 19：budget_check 与 usage_summary 共用）。
pub fn cost_of(rows: &[Value], fx: &std::collections::HashMap<String, f64>, cur: &str, since: Option<i64>) -> f64 {
    rows.iter()
        .filter(|r| {
            since
                .map(|s| r["at"].as_i64().unwrap_or(0) >= s)
                .unwrap_or(true)
        })
        .map(|r| {
            let c = r["input_cost"].as_f64().unwrap_or(0.0)
                + r["output_cost"].as_f64().unwrap_or(0.0);
            let currency = r["currency"].as_str().unwrap_or("");
            let rate = if currency == cur {
                1.0
            } else {
                fx.get(currency).copied().unwrap_or(0.0)
            };
            c * rate
        })
        .sum::<f64>()
}

/// 预算预检：本月与累计（display_currency，未计价贡献 0）。
pub async fn budget_check(db: &DbHandle, config: &Config) -> Result<(), String> {
    let fx = config.billing.fx_rates.clone();
    let cur = config.billing.display_currency.clone();
    let ms = month_start_pub();
    let rows = db
        .select_json(
            "SELECT input_cost, output_cost, currency, at FROM usage_records
             WHERE currency IS NOT NULL",
        )
        .await
        .map_err(|e| e.to_string())?;
    let month = cost_of(&rows, &fx, &cur, Some(ms));
    let total = cost_of(&rows, &fx, &cur, None);
    if let Some(b) = config.billing.budget_monthly
        && month >= b
    {
        return Err(format!(
            "本月预算已耗尽（{month:.2}/{b} CNY），可在设置页调整"
        ));
    }
    if let Some(b) = config.billing.budget_total
        && total >= b
    {
        return Err(format!("累计预算已耗尽（{total:.2}/{b} CNY）"));
    }
    Ok(())
}

/// 运行一轮对话：追加 user 消息 → Loop → 返回 (完成文本, summary)。
/// messages 由调用方（会话层）持有，Loop 直接就地追加 assistant/tool 消息。
#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &ChatClient,
    config: &Config,
    db: &DbHandle,
    registry: &ToolRegistry,
    ctx: &ToolCtx,
    messages: &mut Vec<Value>,
    user_text: &str,
    context_injection: &str,
    cancel: CancellationToken,
    mut on_event: impl FnMut(AgentEvent) + Send,
) -> Result<RunSummary, String> {
    // 评审缺陷 10：先入历史再预检——预算拒绝时用户消息也已落盘（刷新不丢）
    messages.push(json!({"role": "user", "content": user_text}));

    // 上下文溢出预警（缺陷 4 残余风险）：字符×1.5 保守估算 vs context_length
    if let Ok(p) = config.active() {
        let chars: usize = messages
            .iter()
            .map(|m| {
                m["content"]
                    .as_str()
                    .map(|s| s.len())
                    .unwrap_or_else(|| m["content"].to_string().len())
            })
            .sum();
        let est_tokens = (chars as f64 * 1.5) as u64;
        if est_tokens > p.context_length.saturating_sub(p.max_output_tokens.unwrap_or(4096)) {
            return Err(
                "会话上下文已接近模型上限，请到 Chats 页点「+ 新建」开启新会话后继续"
                    .into(),
            );
        }
    }

    budget_check(db, config).await?;

    // BUG-1 修复：模型此前无任何日期参照，「今天/昨天」只能靠猜 → 落库错误日期。
    // 每轮实时计算（跨午夜天然正确），不引入 get_now 工具。
    let today = chrono::Local::now();
    let weekday_cn = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"]
        [today.weekday().num_days_from_sunday() as usize];
    let mut system = format!(
        "你是 Betterboxd，一位中文影迷的观影数据助手。平等、简明、不谄媚。\n\
         【当前日期】{}（{}）。\n\
         【诚实边界】不编造票房/影评/榜单；影片事实必须来自工具返回；不知道就说不知道。\n\
         【工具纪律】涉及 add 写操作先搜索确认影片；update/delete 用 lookup_diary 定位 entry_id 即可（无需搜索影片）；统计必须调用 run_stats 用 SQL 计算，\
         禁止自己数数或心算汇总；日期相对词（今年/上月）翻译为 SQLite 日期表达式。\
         SQL 的输出列必须用中文 AS 别名（如 AS 月份、AS 平均分），标签列在前。\n\
         【统计工作流】①统计前先 list_saved_queries：已有同类项目→用 run_stats 的\
         saved_query_id 直跑（智能复用）；②没有→写新 SQL 跑 run_stats，\
         并用自然语言+图标（📈🏆✨等）解读结果，禁止裸贴 JSON/表格；\
         ③用户随口求统计→解读完主动问「要保存为统计项目吗」；\
         ④用户明确要建统计项目→直接调 manage_saved_queries(create)\
         （确认卡会展示名称+SQL）；⑤SQL 执行报错时自行修正重试（最多2次）；\
         ⑥能用 SQL 聚合的（计数/均值/排序/窗口）一律 run_stats；主观/文本分析（风格、情感、主题、写作特点）→ lookup_diary/lookup_reviews 取样阅读后自行总结，禁止用 run_stats 硬算文本字段。\
         展示表格用标准 GFM 语法：表头行与分隔行都以 | 开头结尾，分隔行只含 | 和 -（如 |---|---|），禁用 + 连接。\n\
         【日期解析铁律】用户消息中的四位数字年份（如 2016）几乎总是影片上映年份，\
         绝不是观看日期！观看日期必须：用户明说日期→用之；（今天、昨天等相对词）→以【当前日期】为基准换算成真实日期；\
         都没有→先问用户，禁止编造。manage_diary 返回重复警告时向用户确认是否仍要记录。\n\
         {SCHEMA_DICTIONARY}\n\
         【隐私】用户的观影随记仅本地保存，你可以引用但不要复述到可导出内容中。",
        today.format("%Y-%m-%d"),
        weekday_cn
    );
    // 画像智能注入（P4.2）：所有 scope 生效；快照轻量（get_profile_snapshot 工具保留供复查）
    if let Ok(p) = tools::profile_snapshot(db).await {
        system.push_str(&format!(
            "\n【你的影迷画像（用户本人的观影档案快照）】{}",
            serde_json::to_string(&p).unwrap_or_default()
        ));
    }
    if !context_injection.is_empty() {
        system.push_str(&format!("\n【当前上下文】\n{context_injection}"));
    }
    if let Ok(p) = config.active() {
        system.push_str(&format!("\n【当前模型档案】{}", p.name));
    }

    let mut steps = 0usize;
    let mut prompt_total = 0u64;
    let mut completion_total = 0u64;
    let mut last_fail: Option<String> = None;
    let mut fail_streak = 0usize;
    let mut interrupted = false;
    let mut aborted_reason = None;

    while steps < MAX_STEPS {
        if cancel.is_cancelled() {
            interrupted = true;
            break;
        }
        steps += 1;

        let mut all_messages = vec![json!({"role": "system", "content": system})];
        all_messages.extend(messages.iter().cloned());
        // 采样参数从活动档案读取（M1：仅 temperature/top_p/max_tokens）
        let mut sampling = json!({});
        if let Ok(p) = config.active() {
            if let Some(t) = p.temperature {
                sampling["temperature"] = json!(t);
            }
            if let Some(t) = p.top_p {
                sampling["top_p"] = json!(t);
            }
            if let Some(m) = p.max_output_tokens {
                sampling["max_tokens"] = json!(m);
            }
        }

        let outcome = {
            let cancel = cancel.clone();
            let mut emit = |t: &str| {
                on_event(AgentEvent {
                    kind: AgentEventKind::Token(t.into()),
                })
            };
            client
                .chat_stream(
                    &all_messages,
                    Some(registry.schemas()),
                    Some(sampling.clone()),
                    &cancel,
                    &mut emit,
                )
                .await
        };
        match outcome {
            Err(e) => {
                aborted_reason = Some(e);
                break;
            }
            Ok(o) if o.interrupted => {
                interrupted = true;
                if let Some(u) = &o.usage {
                    prompt_total += u.prompt_tokens.unwrap_or(0);
                    completion_total += u.completion_tokens.unwrap_or(0);
                }
                break;
            }
            Ok(o) => {
                prompt_total += o.usage.as_ref().and_then(|u| u.prompt_tokens).unwrap_or(0);
                completion_total += o
                    .usage
                    .as_ref()
                    .and_then(|u| u.completion_tokens)
                    .unwrap_or(0);

                if o.tool_calls.is_empty() {
                    messages.push(json!({"role": "assistant", "content": o.text}));
                    return Ok(RunSummary {
                        steps,
                        interrupted,
                        usage_tokens: (prompt_total, completion_total),
                        aborted_reason,
                    });
                }

                // assistant(tool_calls) 消息入历史
                let tc_json: Vec<Value> = o
                    .tool_calls
                    .iter()
                    .map(|t| {
                        json!({"id": t.id, "type": "function",
                               "function": {"name": t.name, "arguments": t.arguments}})
                    })
                    .collect();
                messages.push(json!({
                    "role": "assistant",
                    "content": if o.text.is_empty() { Value::Null } else { Value::String(o.text.clone()) },
                    "tool_calls": tc_json
                }));

                for tc in &o.tool_calls {
                    let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
                    on_event(AgentEvent {
                        kind: AgentEventKind::ToolStart {
                            name: tc.name.clone(),
                            args: tc.arguments.clone(),
                        },
                    });
                    let result = tools::execute(&tc.name, ctx, args).await;
                    let ok = result.is_ok();
                    // 严格"同工具连续失败"：成功或换工具都重置
                    if ok {
                        last_fail = None;
                        fail_streak = 0;
                    } else if last_fail.as_deref() == Some(tc.name.as_str()) {
                        fail_streak += 1;
                    } else {
                        last_fail = Some(tc.name.clone());
                        fail_streak = 1;
                    }
                    let payload = match &result {
                        Ok(v) => v.to_string(),
                        Err(e) => json!({"error": e}).to_string(),
                    };
                    // 评审缺陷 4：载荷上限 16KB（UTF-8 安全截断），防单步上下文炸弹
                    const MAX_TOOL_PAYLOAD: usize = 16 * 1024;
                    let payload = if payload.len() > MAX_TOOL_PAYLOAD {
                        let mut cut = MAX_TOOL_PAYLOAD;
                        while cut > 0 && !payload.is_char_boundary(cut) {
                            cut -= 1;
                        }
                        format!("{}\n…[结果超过16KB已截断]", &payload[..cut])
                    } else {
                        payload
                    };
                    on_event(AgentEvent {
                        kind: AgentEventKind::ToolDone {
                            name: tc.name.clone(),
                            ok,
                        },
                    });
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": payload
                    }));
                    if fail_streak >= TOOL_FAIL_BREAKER {
                        aborted_reason = Some(format!(
                            "工具 {} 连续失败 {TOOL_FAIL_BREAKER} 次，已熔断",
                            tc.name
                        ));
                        return Ok(RunSummary {
                            steps,
                            interrupted: false,
                            usage_tokens: (prompt_total, completion_total),
                            aborted_reason,
                        });
                    }
                }
            }
        }
    }

    if interrupted {
        return Ok(RunSummary {
            steps,
            interrupted: true,
            usage_tokens: (prompt_total, completion_total),
            aborted_reason,
        });
    }
    if aborted_reason.is_none() {
        aborted_reason = Some(format!("达到最大步数 {MAX_STEPS}"));
    }
    Err(aborted_reason.unwrap_or_else(|| "Agent 异常终止".into()))
}

/// 会话开场白（P4 补充 1）：新 movie 系会话建立后，服务端主动生成第一条引导消息。
/// 不走完整 agent loop（无工具），把注入上下文复述成自然引导；流式经 on_event 推送。
pub async fn opening_message(
    client: &ChatClient,
    _config: &Config,
    db: &DbHandle,
    context_injection: &str,
    cancel: CancellationToken,
    mut on_event: impl FnMut(AgentEvent) + Send,
) -> Result<(String, (u64, u64)), String> {
    let today = chrono::Local::now();
    let weekday_cn = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"]
        [today.weekday().num_days_from_sunday() as usize];
    let mut system = format!(
        "你是 Betterboxd，一位中文影迷的观影数据助手。平等、简明、不谄媚。\n\
         【当前日期】{}（{}）。\n\
         用户刚进入一个新会话，请发出第一条消息主动开启话题：基于下方上下文，\
         用 2-4 句话点出这部片/这条记录/这篇影评的要点（评分、日期、关键维度等你看到的事实），\
         再抛出一个具体的讨论方向（问句收尾）。不要罗列全部数据，不要用列表，像朋友聊天一样自然。\
         直接输出消息正文，不要任何前缀或解释。",
        today.format("%Y-%m-%d"),
        weekday_cn
    );
    if let Ok(p) = tools::profile_snapshot(db).await {
        system.push_str(&format!(
            "\n【你的影迷画像（用户本人的观影档案快照）】{}",
            serde_json::to_string(&p).unwrap_or_default()
        ));
    }
    if !context_injection.is_empty() {
        system.push_str(&format!("\n【当前上下文】\n{context_injection}"));
    }
    let mut emit = |t: &str| {
        on_event(AgentEvent {
            kind: AgentEventKind::Token(t.into()),
        })
    };
    let o = client
        .chat_stream(
            &[json!({"role": "system", "content": system})],
            None,
            None,
            &cancel,
            &mut emit,
        )
        .await?;
    let usage = (
        o.usage.as_ref().and_then(|u| u.prompt_tokens).unwrap_or(0),
        o.usage.as_ref().and_then(|u| u.completion_tokens).unwrap_or(0),
    );
    Ok((o.text, usage))
}
