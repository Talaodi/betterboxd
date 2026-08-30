//! Betterboxd 核心库（R1：全部业务逻辑收拢于此）。

pub mod db;
pub mod llm;
pub mod stats_guard;

pub use db::Db;
