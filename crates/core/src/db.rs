//! 存储层：SQLite 连接、迁移、业务表与只读视图面。
//!
//! 设计要点（design.md §7.8/§7.9）：
//! - 单库 `data.db`，WAL；日期 = TEXT YYYY-MM-DD；时刻 = INTEGER unix 秒 UTC；
//! - 主键 uuid v7 存 TEXT；枚举 TEXT + CHECK；
//! - AI 统计只允许访问 `v_*` 只读视图。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

/// M001：初始全量 schema（业务表 + 索引 + 只读视图面）。
const M001: &str = r#"
-- ============ 影片域 ============
CREATE TABLE movies (
    tmdb_id           INTEGER PRIMARY KEY,
    title_zh          TEXT,
    title_en          TEXT,
    title_original    TEXT,
    release_date      TEXT,
    runtime           INTEGER,
    original_language TEXT,
    spoken_languages  TEXT NOT NULL DEFAULT '[]',
    directors         TEXT NOT NULL DEFAULT '[]',
    genres            TEXT NOT NULL DEFAULT '[]',
    posters           TEXT NOT NULL DEFAULT '[]',
    tagline           TEXT,
    overview          TEXT,
    lb_rating         REAL,
    lb_votes          INTEGER,
    my_rating         INTEGER CHECK(my_rating IS NULL OR (my_rating BETWEEN 0 AND 100)),
    watched           INTEGER NOT NULL DEFAULT 0,
    in_watchlist      INTEGER NOT NULL DEFAULT 0,
    liked             INTEGER NOT NULL DEFAULT 0,
    fetched_at        INTEGER,
    updated_at        INTEGER NOT NULL
);

CREATE TABLE diary_entries (
    id                 TEXT PRIMARY KEY,
    movie_id           INTEGER NOT NULL REFERENCES movies(tmdb_id),
    watched_date       TEXT NOT NULL,
    rating             INTEGER CHECK(rating IS NULL OR rating BETWEEN 0 AND 100),
    in_theater         INTEGER NOT NULL DEFAULT 0,
    liked              INTEGER NOT NULL DEFAULT 0,
    ticket_price_cents INTEGER,
    private_note       TEXT NOT NULL DEFAULT '',
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);
CREATE INDEX idx_diary_movie ON diary_entries(movie_id);
CREATE INDEX idx_diary_date  ON diary_entries(watched_date);

CREATE TABLE reviews (
    id         TEXT PRIMARY KEY,
    movie_id   INTEGER NOT NULL REFERENCES movies(tmdb_id),
    title      TEXT,
    body_md    TEXT NOT NULL,
    rating     INTEGER CHECK(rating IS NULL OR rating BETWEEN 0 AND 100),
    liked      INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_reviews_movie ON reviews(movie_id);

-- ============ 标签域 ============
CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE);

CREATE TABLE entry_tags (
    entry_id TEXT NOT NULL REFERENCES diary_entries(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id),
    PRIMARY KEY (entry_id, tag_id)
);
CREATE INDEX idx_entry_tags_tag ON entry_tags(tag_id);

CREATE TABLE dimension_values (
    id        TEXT PRIMARY KEY,
    dimension TEXT NOT NULL CHECK(dimension IN ('地点','同伴','情绪','场景')),
    name      TEXT NOT NULL,
    UNIQUE (dimension, name)
);

CREATE TABLE entry_dimensions (
    entry_id TEXT NOT NULL REFERENCES diary_entries(id) ON DELETE CASCADE,
    value_id TEXT NOT NULL REFERENCES dimension_values(id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, value_id)
);
CREATE INDEX idx_entry_dimensions_value ON entry_dimensions(value_id);

-- ============ 清单域 ============
CREATE TABLE lists (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    source      TEXT NOT NULL CHECK(source IN ('manual','letterboxd')),
    ranked      INTEGER NOT NULL DEFAULT 0,
    external_id TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE list_items (
    list_id  TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    movie_id INTEGER NOT NULL REFERENCES movies(tmdb_id),
    rank     INTEGER,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (list_id, movie_id)
);
CREATE INDEX idx_list_items_movie ON list_items(movie_id);

-- ============ 影史知识 ============
CREATE TABLE canons (
    key     TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    edition TEXT
);

CREATE TABLE canon_films (
    canon_key TEXT NOT NULL REFERENCES canons(key) ON DELETE CASCADE,
    rank      INTEGER NOT NULL,
    tmdb_id   INTEGER NOT NULL,
    PRIMARY KEY (canon_key, tmdb_id)
);

-- ============ 变更账目 ============
CREATE TABLE actions (
    id           TEXT PRIMARY KEY,
    movie_id     INTEGER NOT NULL REFERENCES movies(tmdb_id),
    target       TEXT NOT NULL CHECK(target IN ('movie','diary_entry','review')),
    target_id    TEXT NOT NULL,
    at           INTEGER NOT NULL,
    source       TEXT NOT NULL CHECK(source IN ('edit','standalone','agent','import','system')),
    ref_id       TEXT,
    changes_json TEXT NOT NULL
);
CREATE INDEX idx_actions_movie  ON actions(movie_id, at);
CREATE INDEX idx_actions_target ON actions(target, target_id);

-- ============ 查询域 ============
CREATE TABLE saved_queries (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    payload_json TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    last_run_at INTEGER
);

CREATE TABLE usage_records (
    id                TEXT PRIMARY KEY,
    session_id        TEXT,
    profile_name      TEXT NOT NULL,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER,
    completion_tokens INTEGER,
    input_cost        REAL,
    output_cost       REAL,
    currency          TEXT,
    at                INTEGER NOT NULL,
    kind              TEXT NOT NULL DEFAULT 'llm'
);
CREATE INDEX idx_usage_at ON usage_records(at);

-- ============ 会话索引（消息正文在 sessions/*.json）============
CREATE TABLE chat_sessions (
    id              TEXT PRIMARY KEY,
    scope           TEXT NOT NULL CHECK(scope IN ('global','movie','diary_entry','review')),
    movie_id        INTEGER,
    entry_id        TEXT,
    review_id       TEXT,
    title           TEXT NOT NULL DEFAULT '',
    created_at      INTEGER NOT NULL,
    last_message_at INTEGER NOT NULL
);
CREATE INDEX idx_sessions_movie ON chat_sessions(movie_id);

-- ============ 只读视图面（AI 统计唯一入口）============

-- 影片全量目录（含状态量；含无观影记录影片）
CREATE VIEW v_movies AS
SELECT tmdb_id,
       title_zh, title_en, title_original,
       COALESCE(title_zh, title_original, title_en)      AS title_main,
       COALESCE(title_original, title_en, title_zh)      AS title_sub,
       release_date,
       substr(release_date, 1, 4)                        AS year,
       runtime, original_language, spoken_languages,
       directors, genres, posters, tagline, overview,
       lb_rating, lb_votes,
       my_rating, watched, in_watchlist, liked,
       fetched_at, updated_at
FROM movies;

-- 观影条目富视图（含派生 rewatch_index；标签/维度聚 JSON）
CREATE VIEW v_diary_full AS
SELECT e.id                                   AS entry_id,
       e.movie_id,
       e.watched_date,
       e.rating,
       e.in_theater,
       e.liked,
       e.ticket_price_cents,
       e.private_note,
       e.created_at,
       e.updated_at,
       ROW_NUMBER() OVER (
           PARTITION BY e.movie_id
           ORDER BY e.watched_date, e.created_at
       )                                      AS rewatch_index,
       m.tmdb_id,
       m.title_zh, m.title_en, m.title_original,
       m.release_date, m.runtime, m.genres, m.directors,
       m.my_rating, m.in_watchlist, m.liked   AS movie_liked,
       m.watched,
       (SELECT json_group_array(t.name)
          FROM entry_tags et JOIN tags t ON t.id = et.tag_id
         WHERE et.entry_id = e.id)            AS tags,
       (SELECT json_group_array(json_object('dimension', dv.dimension, 'name', dv.name))
          FROM entry_dimensions ed JOIN dimension_values dv ON dv.id = ed.value_id
         WHERE ed.entry_id = e.id)            AS dimensions_flat
FROM diary_entries e
JOIN movies m ON m.tmdb_id = e.movie_id;

-- 影评统计面
CREATE VIEW v_reviews_full AS
SELECT r.id AS review_id, r.movie_id, r.title, r.body_md,
       length(r.body_md) AS body_len,
       r.rating, r.liked, r.created_at, r.updated_at,
       m.title_zh, m.title_en, m.title_original, m.genres, m.directors,
       m.my_rating
FROM reviews r JOIN movies m ON m.tmdb_id = r.movie_id;

-- 变更史统计面（is_active = 目标行仍存活，供状态时序重构）
CREATE VIEW v_actions AS
SELECT a.id, a.movie_id, a.target, a.target_id, a.at, a.source, a.ref_id,
       a.changes_json,
       CASE a.target
           WHEN 'movie'       THEN 1
           WHEN 'diary_entry' THEN EXISTS(SELECT 1 FROM diary_entries d WHERE d.id = a.target_id)
           WHEN 'review'      THEN EXISTS(SELECT 1 FROM reviews rv WHERE rv.id = a.target_id)
           ELSE 0
       END AS is_active
FROM actions a;
CREATE INDEX idx_actions_active ON actions(target, target_id, at);

-- 日志流（看/评/聊 三类 UNION；锚点时间统一 TEXT 日期）
CREATE VIEW v_logs AS
SELECT 'watch' AS kind, e.id AS id, e.movie_id, e.watched_date AS at,
       CASE WHEN e.private_note = '' THEN m.title_zh
            ELSE substr(e.private_note, 1, 60) END AS brief
FROM diary_entries e JOIN movies m ON m.tmdb_id = e.movie_id
UNION ALL
SELECT 'review', r.id, r.movie_id,
       strftime('%Y-%m-%d', r.created_at, 'unixepoch'),
       COALESCE(r.title, substr(r.body_md, 1, 60))
FROM reviews r
UNION ALL
SELECT 'chat', s.id, s.movie_id,
       strftime('%Y-%m-%d', s.last_message_at, 'unixepoch'),
       s.title
FROM chat_sessions s
WHERE s.scope <> 'global';

-- 清单统计面
CREATE VIEW v_lists AS
SELECT l.id AS list_id, l.name, l.source, l.ranked, l.external_id,
       li.movie_id, li.rank, li.added_at
FROM lists l LEFT JOIN list_items li ON li.list_id = l.id;

-- 影史榜单知识面
CREATE VIEW v_canon AS
SELECT c.key AS canon_key, c.name AS canon_name, c.edition,
       f.rank, f.tmdb_id
FROM canons c JOIN canon_films f ON f.canon_key = c.key;
"#;

/// 迁移清单：(版本号, SQL)。追加迁移时在末尾 push，不改历史。
const MIGRATIONS: &[(i64, &str)] = &[(1, M001)];

/// 顺序应用未执行的迁移（自由函数，供种子/测试复用）。
pub fn apply_migrations(conn: &Connection) -> Result<(), DbError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for &(version, sql) in MIGRATIONS {
        if version > current {
            conn.execute_batch(&format!(
                "BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"
            ))?;
        }
    }
    Ok(())
}

/// 存储错误。
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("库文件 IO: {0}")]
    Io(#[from] std::io::Error),
}

/// 轻量封装：Mutex 保护的连接。M1 引入专用 DB 线程后保持同接口。
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        Self::from_conn(conn)
    }

    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Self, DbError> {
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// 顺序应用未执行的迁移；每个迁移一个事务（design.md §7.6 第 18 条）。
    pub fn migrate(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for &(version, sql) in MIGRATIONS {
            if version > current {
                conn.execute_batch(&format!(
                    "BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"
                ))?;
            }
        }
        Ok(())
    }

    /// 受控访问连接（M0 内部/M1 移入 DB 线程）。
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, DbError> {
        let conn = self.conn.lock().unwrap();
        Ok(f(&conn)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn ts(offset: i64) -> i64 {
        1_750_000_000 + offset
    }

    fn seed_movie(conn: &Connection, tmdb_id: i64, title: &str) {
        conn.execute(
            "INSERT INTO movies (tmdb_id, title_zh, title_original, release_date,
             runtime, my_rating, in_watchlist, updated_at)
             VALUES (?1, ?2, ?2, '2000-01-01', 98, NULL, 1, ?3)",
            params![tmdb_id, title, ts(0)],
        )
        .unwrap();
    }

    fn seed_entry(conn: &Connection, id: &str, movie_id: i64, date: &str, rating: Option<i64>) {
        conn.execute(
            "INSERT INTO diary_entries (id, movie_id, watched_date, rating,
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, movie_id, date, rating, ts(1)],
        )
        .unwrap();
    }

    #[test]
    fn migrate_twice_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        let v: i64 = db
            .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn rewatch_index_survives_backfill() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            seed_movie(c, 843, "花样年华");
            seed_entry(c, "e-new", 843, "2026-08-15", Some(90)); // 先录入 2026 的
            seed_entry(c, "e-old", 843, "2019-08-20", Some(70)); // 回插 2019 的
            Ok(())
        })
        .unwrap();
        let rows: Vec<(String, i64)> = db
            .with_conn(|c| {
                let mut st = c.prepare(
                    "SELECT entry_id, rewatch_index FROM v_diary_full ORDER BY rewatch_index",
                )?;
                let rows = st
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(rows, vec![("e-old".into(), 1), ("e-new".into(), 2)]);
    }

    #[test]
    fn v_actions_is_active_tracks_target_row() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            seed_movie(c, 1, "T");
            seed_entry(c, "e1", 1, "2026-08-15", Some(90));
            for (id, target, tid) in [("a1", "diary_entry", "e1"), ("a2", "movie", "1")] {
                c.execute(
                    "INSERT INTO actions (id, movie_id, target, target_id, at,
                     source, changes_json) VALUES (?1, 1, ?2, ?3, ?4, 'edit', '{}')",
                    params![id, target, tid, ts(2)],
                )
                .unwrap();
            }
            Ok(())
        })
        .unwrap();
        let (e1_active, movie_active): (i64, i64) = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT MAX(CASE WHEN target_id='e1' THEN is_active END),
                            MAX(CASE WHEN target='movie' THEN is_active END)
                     FROM v_actions",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(e1_active, 1);
        assert_eq!(movie_active, 1);
        // 删除条目 → 其历史断言 is_active 归零（账目保留）
        db.with_conn(|c| c.execute("DELETE FROM diary_entries WHERE id='e1'", []))
            .unwrap();
        let e1_active: i64 = db
            .with_conn(|c| {
                c.query_row("SELECT is_active FROM v_actions WHERE id='a1'", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(e1_active, 0);
    }

    #[test]
    fn v_logs_unions_three_kinds() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            seed_movie(c, 1, "花样年华");
            seed_entry(c, "e1", 1, "2026-08-15", Some(90));
            c.execute(
                "INSERT INTO reviews (id, movie_id, title, body_md, created_at, updated_at)
                 VALUES ('r1', 1, '重看霓虹', '正文', ?1, ?1)",
                params![ts(3)],
            )
            .unwrap();
            c.execute(
                "INSERT INTO chat_sessions (id, scope, movie_id, title,
                 created_at, last_message_at) VALUES ('s1', 'movie', 1, '聊聊它', ?1, ?1)",
                params![ts(4)],
            )
            .unwrap();
            c.execute(
                "INSERT INTO chat_sessions (id, scope, title, created_at, last_message_at)
                 VALUES ('s2', 'global', '闲聊', ?1, ?1)",
                params![ts(5)],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        let kinds: Vec<(String,)> = db
            .with_conn(|c| {
                let mut st = c.prepare("SELECT kind FROM v_logs ORDER BY kind")?;
                let rows = st
                    .query_map([], |r| Ok((r.get(0)?,)))?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(rows)
            })
            .unwrap();
        // global 会话不入日志流
        assert_eq!(
            kinds,
            vec![("chat".into(),), ("review".into(),), ("watch".into(),)]
        );
    }

    #[test]
    fn dimensions_flat_json_shape() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            seed_movie(c, 1, "T");
            seed_entry(c, "e1", 1, "2026-08-15", None);
            for (id, dim, name) in [
                ("v1", "地点", "家"),
                ("v2", "同伴", "独看"),
                ("v3", "同伴", "老张"),
            ] {
                c.execute(
                    "INSERT INTO dimension_values (id, dimension, name) VALUES (?1,?2,?3)",
                    params![id, dim, name],
                )
                .unwrap();
                c.execute(
                    "INSERT INTO entry_dimensions (entry_id, value_id) VALUES ('e1', ?1)",
                    params![id],
                )
                .unwrap();
            }
            Ok(())
        })
        .unwrap();
        let flat: String = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT dimensions_flat FROM v_diary_full WHERE entry_id='e1'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&flat).unwrap();
        let companions: Vec<&str> = arr
            .iter()
            .filter(|v| v["dimension"] == "同伴")
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert_eq!(companions.len(), 2, "同维度多值：{flat}");
    }
}

// ============ DB 专用线程（M1）：异步安全访问 ============

use tokio::sync::oneshot;

type Job = Box<dyn FnOnce(&Connection) + Send>;

/// 克隆型句柄：所有查询串行执行在专用线程上，异步侧 await 结果。
#[derive(Clone)]
pub struct DbHandle {
    tx: tokio::sync::mpsc::UnboundedSender<Job>,
}

impl DbHandle {
    pub fn spawn(conn: Connection) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        std::thread::spawn(move || {
            while let Some(job) = rx.blocking_recv() {
                job(&conn);
            }
        });
        Self { tx }
    }

    pub async fn call<R, F>(&self, f: F) -> Result<R, DbError>
    where
        R: Send + 'static,
        F: FnOnce(&Connection) -> rusqlite::Result<R> + Send + 'static,
    {
        let (otx, orx) = oneshot::channel();
        self.tx
            .send(Box::new(move |conn| {
                let _ = otx.send(f(conn));
            }))
            .map_err(|_| DbError::Io(std::io::Error::other("DB 线程已关闭")))?;
        match orx.await {
            Ok(r) => r.map_err(DbError::Sqlite),
            Err(_) => Err(DbError::Io(std::io::Error::other("DB 任务被丢弃"))),
        }
    }

    /// 任意只读 SELECT → JSON 行数组（供工具层与统计）。
    pub async fn select_json(&self, sql: &str) -> Result<Vec<serde_json::Value>, DbError> {
        let sql = sql.to_string();
        self.call(move |c| {
            let mut st = c.prepare(&sql)?;
            let cols: Vec<String> = st.column_names().into_iter().map(String::from).collect();
            let mut rows = st.query([])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                let mut obj = serde_json::Map::new();
                for (i, col) in cols.iter().enumerate() {
                    let v: serde_json::Value = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => n.into(),
                        rusqlite::types::ValueRef::Real(f) => f.into(),
                        rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into(),
                        rusqlite::types::ValueRef::Blob(b) => format!("<blob {}B>", b.len()).into(),
                    };
                    obj.insert(col.clone(), v);
                }
                out.push(Value::Object(obj));
            }
            Ok(out)
        })
        .await
    }
}

use serde_json::Value as JsonValue;
type Value = JsonValue;

impl Db {
    /// 交给 DbHandle::spawn 前取出连接。
    pub fn into_inner(self) -> Connection {
        self.conn.into_inner().unwrap()
    }

    /// 统计连接（独立只读）。当前被参数化路径取代，保留给 M2 统计页复用。
    #[allow(dead_code)]
    pub fn open_stats_conn(path: &Path) -> Result<Connection, DbError> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.pragma_update(None, "query_only", "ON")?;
        Ok(conn)
    }
}

/// 工具层参数值（避免 SQL 字符串拼接成为习惯）。
#[derive(Debug, Clone)]
pub enum SqlVal {
    Text(String),
    Int(i64),
    Real(f64),
    Bool(bool),
    Null,
}

impl rusqlite::ToSql for SqlVal {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            SqlVal::Text(s) => s.to_sql(),
            SqlVal::Int(n) => n.to_sql(),
            SqlVal::Real(f) => f.to_sql(),
            SqlVal::Bool(b) => Ok(rusqlite::types::ToSqlOutput::Owned(
                rusqlite::types::Value::Integer(*b as i64),
            )),
            SqlVal::Null => rusqlite::types::Null.to_sql(),
        }
    }
}

impl DbHandle {
    /// 带绑定参数的 SELECT → JSON 行。
    pub async fn select_json_params(
        &self,
        sql: String,
        vals: Vec<SqlVal>,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        self.call(move |c| {
            let mut st = c.prepare(&sql)?;
            let cols: Vec<String> = st.column_names().into_iter().map(String::from).collect();
            let mut rows = st.query(rusqlite::params_from_iter(vals))?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                let mut obj = serde_json::Map::new();
                for (i, col) in cols.iter().enumerate() {
                    let v: serde_json::Value = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => n.into(),
                        rusqlite::types::ValueRef::Real(f) => f.into(),
                        rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into(),
                        rusqlite::types::ValueRef::Blob(b) => format!("<blob {}B>", b.len()).into(),
                    };
                    obj.insert(col.clone(), v);
                }
                out.push(serde_json::Value::Object(obj));
            }
            Ok(out)
        })
        .await
    }
}

// ============ 影片级状态重算（Action 账目 → 缓存列）============

/// 影片级状态重算规则（design.md 定稿）：
/// - my_rating/liked：target=movie 的断言 Action，按 at 倒序取第一条
///   source ∈ {edit,agent,standalone} 且 changes 含该字段的**新值**；
///   带 ref_id 的断言若引用的条目/影评已被删除（撤销语义）则跳过；
/// - watched：派生（存在 Diary 条目或 Review 即真）；
/// - in_watchlist：状态量，任意 source 的最后一条变更生效
///   （system 自动消除是合法变更）。
pub fn recompute_movie_state(conn: &Connection, movie_id: i64) -> rusqlite::Result<()> {
    let pick = |field: &str| -> rusqlite::Result<Option<i64>> {
        let mut st = conn.prepare(
            "SELECT json_extract(changes_json, ?2), ref_id
             FROM actions
             WHERE movie_id = ?1 AND target = 'movie'
               AND source IN ('edit','agent','standalone')
               AND json_extract(changes_json, ?2) IS NOT NULL
             ORDER BY at DESC, rowid DESC",
        )?;
        let rows = st.query_map(rusqlite::params![movie_id, format!("$.{field}[1]")], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (new_val, ref_id) = row?;
            if let Some(rid) = ref_id {
                // 撤销检查：引用的条目/影评必须仍存活
                let alive: i64 = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM diary_entries WHERE id=?1)
                              + EXISTS(SELECT 1 FROM reviews WHERE id=?1)",
                        rusqlite::params![rid],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if alive == 0 {
                    continue; // 已被删除，跳过该断言
                }
            }
            return Ok(Some(new_val));
        }
        Ok(None)
    };

    let my_rating: Option<i64> = pick("my_rating")?;
    let liked: Option<i64> = pick("liked")?;
    let watched: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM diary_entries WHERE movie_id=?1)
              + EXISTS(SELECT 1 FROM reviews WHERE movie_id=?1) > 0",
        rusqlite::params![movie_id],
        |r| r.get(0),
    )?;
    // in_watchlist：最后一条变更（含 system），at 倒序
    let in_watchlist: Option<i64> = conn
        .query_row(
            "SELECT json_extract(changes_json, '$.in_watchlist[1]')
             FROM actions
             WHERE movie_id=?1 AND target='movie'
               AND json_extract(changes_json,'$.in_watchlist[1]') IS NOT NULL
             ORDER BY at DESC LIMIT 1",
            rusqlite::params![movie_id],
            |r| r.get(0),
        )
        .ok();

    conn.execute(
        "UPDATE movies SET my_rating=?2, liked=COALESCE(?3, 0), watched=?4,
           in_watchlist=COALESCE(?5, in_watchlist) WHERE tmdb_id=?1",
        rusqlite::params![movie_id, my_rating, liked, watched, in_watchlist],
    )?;
    Ok(())
}

/// 全量重算（诊断向导/种子数据用）。
pub fn recompute_all_movie_states(conn: &Connection) -> rusqlite::Result<()> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT tmdb_id FROM movies")?;
        st.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in ids {
        recompute_movie_state(conn, id)?;
    }
    Ok(())
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use rusqlite::params;

    fn mk_movie(c: &Connection, id: i64) {
        c.execute(
            "INSERT INTO movies (tmdb_id, title_original, updated_at) VALUES (?1,'T',1)",
            params![id],
        )
        .unwrap();
    }
    fn mk_entry(c: &Connection, id: &str, movie: i64, rating: Option<i64>) {
        c.execute(
            "INSERT INTO diary_entries (id, movie_id, watched_date, rating, created_at, updated_at)
                   VALUES (?1,?2,'2026-08-15',?3,1,1)",
            params![id, movie, rating],
        )
        .unwrap();
    }
    fn assert_action(
        c: &Connection,
        id: &str,
        movie: i64,
        at: i64,
        src: &str,
        ref_id: Option<&str>,
        field: &str,
        old: Option<i64>,
        new: i64,
    ) {
        let changes = serde_json::json!({ field: [old, new] }).to_string();
        c.execute("INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
                   VALUES (?1,?2,'movie',?2,?3,?4,?5,?6)",
                  params![id, movie, at, src, ref_id, changes]).unwrap();
    }

    #[test]
    fn recompute_follows_rules() {
        let c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::apply_migrations(&c).unwrap();
        mk_movie(&c, 1);

        // 3 月条目断言 88（edit）→ 6 月独立改分 85（standalone）→ 最终 85
        mk_entry(&c, "e1", 1, Some(88));
        assert_action(&c, "a1", 1, 100, "edit", Some("e1"), "my_rating", None, 88);
        assert_action(
            &c,
            "a2",
            1,
            200,
            "standalone",
            None,
            "my_rating",
            Some(88),
            85,
        );
        recompute_movie_state(&c, 1).unwrap();
        let (mr, watched): (Option<i64>, i64) = c
            .query_row(
                "SELECT my_rating, watched FROM movies WHERE tmdb_id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((mr, watched), (Some(85), 1));

        // 独立改分时间必须严格晚于条目断言（同秒以 rowid 决胜，此处跨秒更稳）
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // 8 月建 Review 断言 95（edit, ref=review）→ 最终 95
        c.execute(
            "INSERT INTO reviews (id, movie_id, body_md, rating, created_at, updated_at)
                   VALUES ('r1',1,'…',95,300,300)",
            [],
        )
        .unwrap();
        assert_action(
            &c,
            "a3",
            1,
            300,
            "edit",
            Some("r1"),
            "my_rating",
            Some(85),
            95,
        );
        recompute_movie_state(&c, 1).unwrap();
        let mr: i64 = c
            .query_row("SELECT my_rating FROM movies WHERE tmdb_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(mr, 95);

        // import 断言不计入：600 时导入旧档断言 60 → 仍 95
        assert_action(&c, "a4", 1, 600, "import", None, "my_rating", Some(95), 60);
        recompute_movie_state(&c, 1).unwrap();
        let mr: i64 = c
            .query_row("SELECT my_rating FROM movies WHERE tmdb_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(mr, 95);

        // 删除 Review（撤销 r1 断言）→ 回退到 85（a2）
        c.execute("DELETE FROM reviews WHERE id='r1'", []).unwrap();
        c.execute(
            "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
                   VALUES ('a5',1,'review','r1',700,'edit','{\"deleted\":[false,true]}')",
            [],
        )
        .unwrap();
        recompute_movie_state(&c, 1).unwrap();
        let mr: i64 = c
            .query_row("SELECT my_rating FROM movies WHERE tmdb_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(mr, 85);

        // liked：变更才传播
        assert_action(&c, "a6", 1, 800, "edit", None, "liked", Some(0), 1);
        recompute_movie_state(&c, 1).unwrap();
        let liked: i64 = c
            .query_row("SELECT liked FROM movies WHERE tmdb_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(liked, 1);

        // in_watchlist：system 自动消除生效
        c.execute("UPDATE movies SET in_watchlist=1 WHERE tmdb_id=1", [])
            .unwrap();
        assert_action(&c, "a7", 1, 900, "system", None, "in_watchlist", Some(1), 0);
        recompute_movie_state(&c, 1).unwrap();
        let iw: i64 = c
            .query_row("SELECT in_watchlist FROM movies WHERE tmdb_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(iw, 0);
    }
}
