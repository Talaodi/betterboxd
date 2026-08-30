//! 手写 OpenAI 兼容 LLM 客户端的最底层：SSE 流解析与 tool_calls 分片累积。
//!
//! spike④（feasibility §2）：tool_calls 在流式下按 index 分片到达，
//! 此模块负责无状态解析 + 有状态累积，均可用纯合成数据单测。

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// 从字节流中切出的 SSE 事件（仅 data 行；注释/空行已忽略）。
#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub data: String,
}

/// 增量喂字节、吐完整事件。跨 chunk 的行拼接由内部缓冲处理。
#[derive(Default)]
pub struct SseBuffer {
    buf: Vec<u8>,
}

impl SseBuffer {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();
        // 以 \n 为界逐行扫描；\r\n 容忍（trim 掉 \r）
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            let line = line.trim_end_matches('\r');
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                events.push(SseEvent {
                    data: data.to_string(),
                });
            }
        }
        events
    }
}

/// OpenAI 兼容 chat.completion.chunk 的最小解析结构。
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<ToolCallFn>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallFn {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
}

/// 流式 tool_calls 累积器：按 index 合并 id/name/arguments 分片。
#[derive(Default)]
pub struct ToolCallAccumulator {
    slots: BTreeMap<usize, Slot>,
}

#[derive(Default)]
struct Slot {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CompletedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCallAccumulator {
    pub fn feed(&mut self, deltas: &[ToolCallDelta]) {
        for d in deltas {
            let slot = self.slots.entry(d.index).or_default();
            if let Some(id) = &d.id {
                slot.id.push_str(id);
            }
            if let Some(f) = &d.function {
                if let Some(n) = &f.name {
                    slot.name.push_str(n);
                }
                if let Some(a) = &f.arguments {
                    slot.arguments.push_str(a);
                }
            }
        }
    }

    pub fn finish(self) -> Vec<CompletedToolCall> {
        self.slots
            .into_values()
            .map(|s| CompletedToolCall {
                id: s.id,
                name: s.name,
                arguments: s.arguments,
            })
            .collect()
    }
}

/// 把一段完整的 SSE 字节流解析为 chunk 列表（测试与同步场景复用）。
pub fn parse_stream(bytes: &[u8]) -> (Vec<String>, Vec<String>, Option<Usage>) {
    let mut buf = SseBuffer::default();
    let mut texts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<String> = Vec::new();
    let mut acc = ToolCallAccumulator::default();
    let mut usage = None;
    for ev in buf.feed(bytes) {
        if ev.data == "[DONE]" {
            break;
        }
        if let Ok(chunk) = serde_json::from_str::<ChatChunk>(&ev.data) {
            if let Some(u) = chunk.usage {
                usage = Some(u);
            }
            for choice in chunk.choices {
                if let Some(t) = choice.delta.content {
                    texts.push(t);
                }
                acc.feed(&choice.delta.tool_calls);
            }
        }
    }
    for tc in acc.finish() {
        tool_calls.push(serde_json::to_string(&tc).unwrap());
    }
    (texts, tool_calls, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成一段"tool_calls 参数跨 3 个分片 + usage 收尾"的 SSE 流。
    const SSE_TOOL_CALLS: &[u8] = b"\
data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"manage_diary\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"action\\\":\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"add\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":30}}\n\n\
data: [DONE]\n\n";

    #[test]
    fn content_streams_and_done_terminates() {
        // 注意：字节串不能含非 ASCII，中文内容用原始字符串转字节
        let sse = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n\
             data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n\
             data: [DONE]\n\n",
            "你", "好"
        );
        let (texts, tools, usage) = parse_stream(sse.as_bytes());
        assert_eq!(texts.join(""), "你好");
        assert!(tools.is_empty());
        assert!(usage.is_none());
    }

    #[test]
    fn tool_calls_fragments_merge_by_index() {
        let (texts, tools, usage) = parse_stream(SSE_TOOL_CALLS);
        assert!(texts.is_empty());
        assert_eq!(tools.len(), 1);
        let tc: serde_json::Value = serde_json::from_str(&tools[0]).unwrap();
        assert_eq!(tc["id"], "call_1");
        assert_eq!(tc["name"], "manage_diary");
        // 分片拼接出的 arguments 必须是合法 JSON
        let args: serde_json::Value =
            serde_json::from_str(tc["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(args["action"], "add");
        assert_eq!(usage.unwrap().prompt_tokens, Some(120));
    }

    #[test]
    fn chunks_split_across_arbitrary_byte_boundaries() {
        // 任意切块（含切断多字节 UTF-8 的边界除外）都应解析出同样结果
        for size in [7usize, 13, 64] {
            let mut buf = SseBuffer::default();
            let mut n = 0;
            for chunk in SSE_TOOL_CALLS.chunks(size) {
                n += buf.feed(chunk).len();
            }
            assert!(n >= 6, "chunk size {size} 只解析出 {n} 个事件");
        }
    }

    #[test]
    fn parallel_tool_calls_keep_order() {
        let sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"t2","arguments":"{}"}},{"index":0,"id":"a","function":{"name":"t1","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}

"#
        .as_bytes();
        let (_, tools, _) = parse_stream(sse);
        // 按 index 排序（BTreeMap），与到达顺序无关
        assert_eq!(tools.len(), 2);
        assert!(tools[0].contains("\"t1\""));
        assert!(tools[1].contains("\"t2\""));
    }
}

// ============ 正式客户端（M1）============

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ChatClient {
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub text: String,
    pub tool_calls: Vec<CompletedToolCall>,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
    pub interrupted: bool,
}

impl ChatClient {
    pub fn new(endpoint: &str, api_key: &str, model: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    /// 流式对话。`extra` 合并进请求体（tools/temperature/max_tokens 等）。
    /// on_token 在每个内容分片到达时同步回调（用于 WS 推送）。
    pub async fn chat_stream(
        &self,
        messages: &[serde_json::Value],
        extra: Option<&serde_json::Value>,
        cancel: &CancellationToken,
        mut on_token: impl FnMut(&str),
    ) -> Result<Outcome, String> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(Value::Object(extra_map)) = extra {
            if let Value::Object(base) = &mut body {
                for (k, v) in extra_map {
                    base.insert(k.clone(), v.clone());
                }
            }
        }
        eprintln!("[llm] body model repr = {:?}", body["model"]);
        if std::env::var("BB_DEBUG_BODY").is_ok() {
            let _ = std::fs::write(
                "/tmp/bb_last_llm_body.json",
                serde_json::to_string_pretty(&body).unwrap_or_default(),
            );
        }

        let resp = tokio::select! {
            _ = cancel.cancelled() => return Ok(Outcome { interrupted: true, ..Default::default() }),
            r = self.http
                .post(format!("{}/chat/completions", self.endpoint))
                .bearer_auth(&self.api_key)
                .json(&body)
                .send() => r.map_err(|e| format!("LLM 网络错误: {e}"))?,
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("LLM HTTP {status}: {}", truncate(&body, 300)));
        }

        let mut out = Outcome::default();
        let mut buf = SseBuffer::default();
        let mut acc = ToolCallAccumulator::default();
        let mut stream = resp.bytes_stream();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    out.interrupted = true;
                    out.tool_calls = acc.finish();
                    return Ok(out);
                }
                chunk = stream.next() => {
                    let Some(bytes) = chunk else { break };
                    let bytes = bytes.map_err(|e| format!("流读取错误: {e}"))?;
                    for ev in buf.feed(&bytes) {
                        if ev.data == "[DONE]" { out.tool_calls = acc.finish(); return Ok(out); }
                        let Ok(chunk) = serde_json::from_str::<ChatChunk>(&ev.data) else { continue };
                        if let Some(u) = chunk.usage { out.usage = Some(u); }
                        for choice in chunk.choices {
                            if choice.finish_reason.is_some() { out.finish_reason = choice.finish_reason; }
                            if let Some(t) = choice.delta.content {
                                on_token(&t);
                                out.text.push_str(&t);
                            }
                            acc.feed(&choice.delta.tool_calls);
                        }
                    }
                }
            }
        }
        out.tool_calls = acc.finish();
        Ok(out)
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{}…", &s[..n]) }
}
