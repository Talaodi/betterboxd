//! SQL 安全审查器（spike②）：AI 生成的统计 SQL 在执行前的静态防线。
//!
//! 规则（design.md §7.6 第 3 条 / §7.9）：
//! - 单条语句；语句类型必须是 Query（SELECT/WITH）；
//! - FROM/JOIN 触达的关系 ∈ 白名单（v_* 只读视图）∪ 本语句 CTE 名
//!   ∪ SQLite 表值函数（json_each/json_tree）；
//! - 非 Query 语句（INSERT/PRAGMA/ATTACH…）被"必须是 Query"整体挡掉。

use sqlparser::ast::{ObjectName, Query, Statement, Visit, Visitor};
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;
use std::ops::ControlFlow;

/// 允许 AI 统计触碰的关系（design.md §7.9 SQL 执行面）。
pub const WHITELIST: &[&str] = &[
    "v_movies",
    "v_diary_full",
    "v_reviews_full",
    "v_actions",
    "v_logs",
    "v_lists",
    "v_canon",
];

/// 允许出现在 FROM 中的 SQLite 表值函数。
pub const TABLE_FUNCTIONS: &[&str] = &["json_each", "json_tree"];

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct GuardError(pub String);

/// 审查一条 AI 生成的统计 SQL。通过 = 可交给 query_only 连接执行。
pub fn review_sql(sql: &str) -> Result<(), GuardError> {
    let stmts = Parser::new(&SQLiteDialect {})
        .try_with_sql(sql)
        .map_err(|e| GuardError(format!("SQL 解析失败: {e}")))?
        .parse_statements()
        .map_err(|e| GuardError(format!("SQL 解析失败: {e}")))?;

    if stmts.len() != 1 {
        return Err(GuardError(format!(
            "只允许单条 SELECT 语句（收到 {} 条）",
            stmts.len()
        )));
    }
    let query = match &stmts[0] {
        Statement::Query(q) => q,
        other => {
            return Err(GuardError(format!(
                "只允许查询语句，收到: {}",
                keyword_of(other)
            )));
        }
    };

    let mut visitor = RelationVisitor::default();
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            visitor
                .cte_names
                .insert(cte.alias.name.value.to_lowercase());
        }
    }
    if query.visit(&mut visitor).is_break() || !visitor.errors.is_empty() {
        return Err(GuardError(visitor.errors.join("; ")));
    }
    Ok(())
}

fn keyword_of(stmt: &Statement) -> String {
    match stmt {
        Statement::Insert(_) => "INSERT".into(),
        Statement::Update { .. } => "UPDATE".into(),
        Statement::Delete(_) => "DELETE".into(),
        Statement::Pragma { .. } => "PRAGMA".into(),
        Statement::AttachDatabase { .. } => "ATTACH".into(),
        Statement::CreateTable(_) => "CREATE TABLE".into(),
        _ => "非 SELECT 语句".into(),
    }
}

#[derive(Default)]
struct RelationVisitor {
    cte_names: std::collections::HashSet<String>,
    errors: Vec<String>,
}

impl Visitor for RelationVisitor {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
        // 进入子查询/WITH 体前注册其 CTE 名
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.cte_names.insert(cte.alias.name.value.to_lowercase());
            }
        }
        ControlFlow::<()>::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<()> {
        let name = relation
            .0
            .last()
            .map(|part| part.as_ident().map(|i| i.value.clone()).unwrap_or_default())
            .unwrap_or_default();
        let lower = name.to_lowercase();
        let ok = WHITELIST.iter().any(|w| *w == lower)
            || TABLE_FUNCTIONS.iter().any(|f| *f == lower)
            || self.cte_names.contains(&lower);
        if !ok {
            self.errors
                .push(format!("不允许访问关系 `{name}`（仅限 v_* 视图面）"));
            return ControlFlow::<()>::Break(());
        }
        ControlFlow::<()>::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_json_each_table_valued_function_parses() {
        // spike② 核心验证点：SQLite 表值函数 json_each 出现在 FROM 中
        let sql = "SELECT j.value AS place, AVG(d.ticket_price_cents) AS p
                   FROM v_diary_full d, json_each(d.dimensions_flat) j
                   WHERE d.ticket_price_cents IS NOT NULL
                   GROUP BY j.value ORDER BY p DESC";
        review_sql(sql).expect("json_each 表值函数应可解析并通过白名单");
    }

    #[test]
    fn accepts_window_and_cte_over_views() {
        let sql = "WITH d AS (
                     SELECT movie_id, rating,
                            LAG(watched_date) OVER (ORDER BY watched_date) AS prev
                     FROM v_diary_full)
                   SELECT movie_id, MAX(julianday(watched_date) - julianday(prev))
                   FROM d GROUP BY movie_id";
        review_sql(sql).expect("窗口/CTE/白名单视图应通过");
    }

    #[test]
    fn accepts_scalar_subquery_in_select() {
        let sql = "SELECT d.title_zh,
                     (SELECT MIN(b.watched_date) FROM v_diary_full b
                       WHERE b.movie_id <> d.movie_id)
                   FROM v_diary_full d";
        review_sql(sql).expect("SELECT 内相关子查询应通过");
    }

    #[test]
    fn rejects_write_and_pragma_and_multi_statement() {
        assert!(review_sql("INSERT INTO v_movies VALUES (1)").is_err());
        assert!(review_sql("DELETE FROM v_diary_full").is_err());
        assert!(review_sql("PRAGMA database_list").is_err());
        assert!(review_sql("ATTACH DATABASE 'x' AS y").is_err());
        assert!(review_sql("SELECT 1; SELECT 2").is_err());
        assert!(review_sql("CREATE TABLE t(x)").is_err());
    }

    #[test]
    fn rejects_non_whitelisted_table_but_allows_cte() {
        let e = review_sql("SELECT * FROM diary_entries").unwrap_err();
        assert!(e.to_string().contains("diary_entries"));
        assert!(review_sql("SELECT * FROM movies m JOIN v_logs l ON 1=1").is_err());
        review_sql(
            "WITH recent AS (SELECT * FROM v_diary_full)
                    SELECT * FROM recent",
        )
        .expect("CTE 名不应被白名单误杀");
        // CTE 体内仍受白名单约束
        assert!(review_sql("WITH bad AS (SELECT * FROM movies) SELECT * FROM bad").is_err());
        // 表值函数白名单之外的名字不可冒用
        assert!(review_sql("SELECT * FROM evil_func(1)").is_err());
    }
}
