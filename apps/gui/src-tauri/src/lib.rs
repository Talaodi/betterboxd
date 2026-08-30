//! M0 验收命令：`chat_echo` —— 把消息按小块经 events 推回前端，
//! 验证 前端 invoke → core 异步任务 → emit → 前端 listen 的完整链路。
//! M1 将被真实 Agent Loop（LLM 流式 + 工具）替换，事件协议不变。

use tauri::{AppHandle, Emitter};

const EVENT_TOKEN: &str = "chat://token";

#[tauri::command]
fn chat_echo(app: AppHandle, message: String) {
    tauri::async_runtime::spawn(async move {
        // 按两个字符一块模拟流式输出，间隔 60ms
        let chars: Vec<char> = message.chars().collect();
        for chunk in chars.chunks(2) {
            let s: String = chunk.iter().collect();
            if app.emit(EVENT_TOKEN, format!("回显: {s}")).is_err() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![chat_echo])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
