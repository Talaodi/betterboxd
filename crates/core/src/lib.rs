//! Betterboxd 核心库（R1：全部业务逻辑收拢于此）。

pub mod agent;
pub mod config;
pub mod db;
pub mod llm;
pub mod session;
pub mod stats_guard;
pub mod tmdb;
pub mod tools;

pub use db::{Db, DbHandle};

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
