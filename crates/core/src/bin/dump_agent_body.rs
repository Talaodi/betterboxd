//! 调试：打印 Agent 实际发出的请求体（不含 api_key），供 curl/node 直接复现。
use betterboxd_core::tools;

fn main() {
    let schemas = tools::registry().schemas();
    let body = serde_json::json!({
        "model": std::env::var("COURSE_MODEL").unwrap_or_default(),
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": [
            {"role": "system", "content": "你是 Betterboxd，一位中文影迷的观影数据助手。测试消息。"},
            {"role": "user", "content": "帮我搜一下盗梦空间这部电影"}
        ],
        "tools": schemas,
    });
    println!("{}", serde_json::to_string(&body).unwrap());
}
