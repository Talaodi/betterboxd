//! 会话持久化（R5）：ChatSession JSON 落盘 + chat_sessions 索引行同步。

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: String,
    pub scope: String, // global | movie | diary_entry | review
    pub movie_id: Option<i64>,
    pub entry_id: Option<String>,
    pub review_id: Option<String>,
    pub title: String,
    pub created_at: i64,
    pub messages: Vec<Value>,
}

pub struct SessionStore {
    dir: PathBuf,
    db: crate::db::DbHandle,
}

impl SessionStore {
    pub fn new(data_dir: &Path, db: crate::db::DbHandle) -> Self {
        let dir = data_dir.join("sessions");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, db }
    }

    pub fn new_session(
        &self,
        scope: &str,
        movie_id: Option<i64>,
        entry_id: Option<String>,
        review_id: Option<String>,
    ) -> ChatSession {
        ChatSession {
            id: uuid::Uuid::now_v7().to_string(),
            scope: scope.into(),
            movie_id,
            entry_id,
            review_id,
            title: String::new(),
            created_at: crate::now(),
            messages: vec![],
        }
    }

    pub fn load(&self, id: &str) -> Option<ChatSession> {
        let raw = std::fs::read_to_string(self.dir.join(format!("{id}.json"))).ok()?;
        let v: Value = serde_json::from_str(&raw).ok()?;
        Some(ChatSession {
            id: v["id"].as_str()?.into(),
            scope: v["scope"].as_str()?.into(),
            movie_id: v["movie_id"].as_i64(),
            entry_id: v["entry_id"].as_str().map(String::from),
            review_id: v["review_id"].as_str().map(String::from),
            title: v["title"].as_str().unwrap_or("").into(),
            created_at: v["created_at"].as_i64().unwrap_or(0),
            messages: v["messages"].as_array().cloned().unwrap_or_default(),
        })
    }

    /// 保存 JSON + 同步索引行（Log 流 join 依赖此行）。
    pub async fn save(&self, s: &ChatSession) -> Result<(), String> {
        let v = json!({
            "id": s.id, "scope": s.scope, "movie_id": s.movie_id,
            "entry_id": s.entry_id, "review_id": s.review_id,
            "title": s.title, "created_at": s.created_at,
            "messages": s.messages,
        });
        // 原子写：临时文件 + rename，防止崩溃产生截断 JSON
        let tmp = self.dir.join(format!("{}.tmp", s.id));
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.dir.join(format!("{}.json", s.id)))
            .map_err(|e| e.to_string())?;
        let (id, scope, movie_id, entry_id, review_id, title, created, last) = (
            s.id.clone(),
            s.scope.clone(),
            s.movie_id,
            s.entry_id.clone(),
            s.review_id.clone(),
            s.title.clone(),
            s.created_at,
            crate::now(),
        );
        self.db
            .call(move |c| {
                c.execute(
                    "INSERT INTO chat_sessions (id, scope, movie_id, entry_id, review_id,
                       title, created_at, last_message_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                     ON CONFLICT(id) DO UPDATE SET title=?6, last_message_at=?8,
                       movie_id=?3, entry_id=?4, review_id=?5",
                    rusqlite::params![
                        id, scope, movie_id, entry_id, review_id, title, created, last
                    ],
                )
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
