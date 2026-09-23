use std::sync::Arc;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    logging, plugin::runtime_pipe_name, protocol::parse_request_line, worker::WorkerState,
};

pub fn start(state: Arc<WorkerState>) {
    tokio::spawn(async move {
        if let Err(error) = serve(Arc::clone(&state)).await {
            logging::warn(&format!("Background Studio 插件 IPC 失败：{error}"));
            let _ = state.shutdown();
        }
    });
}

#[cfg(windows)]
async fn serve(state: Arc<WorkerState>) -> Result<(), String> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let pipe_name = runtime_pipe_name()?;
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)
        .map_err(|error| format!("创建插件管道失败：{error}"))?;

    loop {
        if state.is_shutting_down() {
            break;
        }
        server
            .connect()
            .await
            .map_err(|error| format!("等待插件管道连接失败：{error}"))?;
        let connected = server;
        if state.is_shutting_down() {
            break;
        }
        server = ServerOptions::new()
            .create(&pipe_name)
            .map_err(|error| format!("重建插件管道失败：{error}"))?;

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_client(state, connected).await {
                logging::warn(&format!("插件 IPC 会话结束：{error}"));
            }
        });
    }
    Ok(())
}

#[cfg(not(windows))]
async fn serve(_state: Arc<WorkerState>) -> Result<(), String> {
    Err("插件 IPC 仅支持 Windows。".to_string())
}

#[cfg(windows)]
async fn handle_client(
    state: Arc<WorkerState>,
    client: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<(), String> {
    let (reader, mut writer) = tokio::io::split(client);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (id, result) = match parse_request_line(&line) {
            Ok(request) => {
                let id = request.id.clone();
                (id, dispatch(&state, request.cmd, request.params).await)
            }
            Err(error) => (String::new(), Err(error)),
        };
        let response = match result {
            Ok(value) => json!({ "id": id, "ok": true, "result": value }),
            Err(error) => json!({ "id": id, "ok": false, "error": error }),
        };
        let mut payload = serde_json::to_string(&response).map_err(|error| error.to_string())?;
        payload.push('\n');
        writer
            .write_all(payload.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        writer.flush().await.map_err(|error| error.to_string())?;
        if state.is_shutting_down() {
            break;
        }
    }
    Ok(())
}

async fn dispatch(
    state: &Arc<WorkerState>,
    cmd: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match cmd.as_str() {
        "hello" => Ok(WorkerState::hello()),
        "configure" => state.configure(params).await,
        "status" => state.status_value(),
        "apply" => run_blocking(state, WorkerState::apply_blocking).await,
        "pause" => run_blocking(state, WorkerState::pause_blocking).await,
        "restore" => run_blocking(state, WorkerState::restore_blocking).await,
        "shutdown" => state.shutdown(),
        other => Err(format!("未知命令：{other}")),
    }
}

/// apply/pause/restore wait on the bridge (and apply may restart Rebased); keep them off the
/// runtime worker threads so `status` stays answerable within the shell's 800 ms budget.
async fn run_blocking(
    state: &Arc<WorkerState>,
    task: fn(&WorkerState) -> Result<serde_json::Value, String>,
) -> Result<serde_json::Value, String> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || task(&state))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn dispatch_hello_status_and_unknown() {
        let dir = std::env::temp_dir().join(format!("rebased-ipc-{}", uuid::Uuid::new_v4()));
        let state = Arc::new(WorkerState::load_from(dir.clone()).unwrap());
        let hello = dispatch(&state, "hello".to_string(), json!(null))
            .await
            .unwrap();
        assert_eq!(hello["pluginProtocol"], 2);
        assert_eq!(hello["pluginId"], "rebased");
        assert_eq!(hello["capabilities"]["mediaKinds"], json!(["image"]));
        let status = dispatch(&state, "status".to_string(), json!(null))
            .await
            .unwrap();
        assert_eq!(status["configured"], false);
        assert_eq!(status["phase"], "idle");
        let error = dispatch(&state, "open-ui".to_string(), json!(null))
            .await
            .unwrap_err();
        assert!(error.contains("未知命令"));
        let video = dispatch(
            &state,
            "configure".to_string(),
            json!({
                "schemaVersion": 1,
                "revision": "r",
                "media": {
                    "url": "http://127.0.0.1:9/m",
                    "kind": "video",
                    "mimeType": "video/mp4",
                    "sha256": "a".repeat(64),
                    "byteSize": 1
                },
                "display": {}
            }),
        )
        .await
        .unwrap_err();
        assert!(video.contains("不支持视频"), "{video}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
