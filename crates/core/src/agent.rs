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
v_movies: tmdb_id, title_zh, title_en, title_original, title_main, title_sub, year, release_date, runtime, original_language, genres(JSON数组), directors(JSON数组), my_rating(0-100可空), watched(0/1), in_watchlist, liked, lb_rating, lb_votes
v_diary_full: entry_id, movie_id, watched_date(YYYY-MM-DD), rating(0-100可空), in_theater, liked, ticket_price_cents(分), private_note, created_at(unix秒), rewatch_index(派生), title_zh, title_en, runtime, genres, directors, my_rating, tags(JSON数组), dimensions_flat(JSON数组 [{dimension,name}]，dimension∈地点|同伴|情绪|场景)
v_reviews_full: review_id, movie_id, title, body_md, body_len, rating, liked, created_at, title_zh, directors
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

fn month_start() -> i64 {
    use chrono::TimeZone;
    let now = chrono::Utc::now();
    chrono::Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .earliest()
        .map(|d| d.timestamp())
        .unwrap_or_else(|| now.timestamp())
}
use chrono::Datelike;

/// 预算预检：本月与累计（display_currency，未计价贡献 0）。
pub async fn budget_check(db: &DbHandle, config: &Config) -> Result<(), String> {
    let fx = config.billing.fx_rates.clone();
    let cur = config.billing.display_currency.clone();
    let ms = month_start();
    let rows = db
        .select_json(
            "SELECT input_cost, output_cost, currency, at FROM usage_records
             WHERE currency IS NOT NULL",
        )
        .await
        .map_err(|e| e.to_string())?;
    let cost = |since: Option<i64>| -> f64 {
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
    };
    let month = cost(Some(ms));
    let total = cost(None);
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
    cancel: CancellationToken,
    mut on_event: impl FnMut(AgentEvent) + Send,
) -> Result<RunSummary, String> {
    budget_check(db, config).await?;

    messages.push(json!({"role": "user", "content": user_text}));

    let mut system = format!(
        "你是 Betterboxd，一位中文影迷的观影数据助手。平等、简明、不谄媚。\n\
         【诚实边界】不编造票房/影评/榜单；影片事实必须来自工具返回；不知道就说不知道。\n\
         【工具纪律】涉及写操作先搜索确认影片；统计必须调用 run_stats 用 SQL 计算，\
         禁止自己数数或心算汇总；日期相对词（今年/上月）翻译为 SQLite 日期表达式。\n\
         {SCHEMA_DICTIONARY}\n\
         【隐私】用户的观影随记仅本地保存，你可以引用但不要复述到可导出内容中。"
    );
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
