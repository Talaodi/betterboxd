//! spike①：课程平台端点能力探测。
//! 依次验证：① 非流式对话 ② 流式 + usage ③ tool_calls 支持。
//! 用法：`set -a; source spikes/local.env; cargo run -p betterboxd-core --bin spike_llm`

use futures_util::StreamExt;
use serde_json::{json, Value};

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn envs() -> (String, String, String) {
    let ep = std::env::var("COURSE_ENDPOINT").expect("缺少 COURSE_ENDPOINT（如 https://xx/v1）");
    let key = std::env::var("COURSE_KEY").expect("缺少 COURSE_KEY");
    let model = std::env::var("COURSE_MODEL").expect("缺少 COURSE_MODEL");
    (ep.trim_end_matches('/').to_string(), key, model)
}

async fn plain_chat(ep: &str, key: &str, model: &str) -> Result<Value, String> {
    let resp = client()
        .post(format!("{ep}/chat/completions"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "messages": [{"role":"user","content":"回复两个字：收到"}]
        }))
        .send().await.map_err(|e| format!("网络: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| format!("非 JSON 响应: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }
    let text = body["choices"][0]["message"]["content"]
        .as_str().unwrap_or("(空)").to_string();
    Ok(json!({"text": text, "usage_present": body["usage"].is_object()}))
}

async fn stream_chat(ep: &str, key: &str, model: &str) -> Result<bool, String> {
    let resp = client()
        .post(format!("{ep}/chat/completions"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": [{"role":"user","content":"数到三"}]
        }))
        .send().await.map_err(|e| format!("网络: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut got_chunk = false;
    let mut got_usage = false;
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(Ok(bytes)) = stream.next().await {
        buf.extend_from_slice(&bytes);
        let s = String::from_utf8_lossy(&buf);
        if s.contains("\"delta\"") || s.contains("\"content\"") { got_chunk = true; }
        if s.contains("\"usage\"") { got_usage = true; }
        if got_chunk && got_usage { break; }
        if buf.len() > 64 * 1024 { break; }
    }
    let _ = buf;
    Ok(got_chunk && got_usage)
}

async fn tool_call(ep: &str, key: &str, model: &str) -> Result<bool, String> {
    let resp = client()
        .post(format!("{ep}/chat/completions"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "messages": [{"role":"user","content":"帮我记录：今天看了盗梦空间，9分。请调用工具完成。"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "manage_diary",
                    "description": "添加/修改/删除观影记录",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "action": {"type":"string","enum":["add"]},
                            "movie_title": {"type":"string"},
                            "rating": {"type":"integer"}
                        },
                        "required": ["action","movie_title"]
                    }
                }
            }]
        }))
        .send().await.map_err(|e| format!("网络: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let msg = &body["choices"][0]["message"];
    Ok(msg["tool_calls"].as_array().map(|a| !a.is_empty()).unwrap_or(false))
}

#[tokio::main]
async fn main() {
    let (ep, key, model) = envs();
    println!("端点: {ep}  模型: {model}\n");
    match plain_chat(&ep, &key, &model).await {
        Ok(v) => println!("① 非流式对话: ✅ 回复={:?} usage={}", v["text"], v["usage_present"]),
        Err(e) => println!("① 非流式对话: ❌ {e}"),
    }
    match stream_chat(&ep, &key, &model).await {
        Ok(true) => println!("② 流式+usage: ✅"),
        Ok(false) => println!("② 流式+usage: ⚠ 有流但 usage 缺失（Q9 记次数兜底）"),
        Err(e) => println!("② 流式+usage: ❌ {e}"),
    }
    match tool_call(&ep, &key, &model).await {
        Ok(true) => println!("③ tool_calls: ✅ —— Agent 完整形态可用"),
        Ok(false) => println!("③ tool_calls: ⚠ 未产生工具调用（可能不支持，Agent 降级为纯对话+引导命令）"),
        Err(e) => println!("③ tool_calls: ❌ {e}"),
    }
}
