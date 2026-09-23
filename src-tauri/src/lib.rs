mod bridge_ipc;
mod bridge_package;
mod fsutil;
mod imaging;
mod logging;
mod media;
mod models;
mod plugin;
mod plugin_ipc;
mod protocol;
mod rebased;
mod worker;

use std::sync::{Arc, Mutex, MutexGuard};

use worker::WorkerState;

pub(crate) fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    value.lock().map_err(|_| "应用状态锁已损坏。".to_string())
}

pub async fn run() -> Result<(), String> {
    let result = run_worker().await;
    if let Err(error) = &result {
        logging::warn(&format!("worker 异常退出：{error}"));
    }
    result
}

async fn run_worker() -> Result<(), String> {
    let state = Arc::new(WorkerState::load()?);
    logging::info(&format!(
        "worker {} 启动，pid {}",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));
    worker::start_environment_worker(Arc::clone(&state))?;
    bridge_ipc::start(Arc::clone(&state));
    plugin_ipc::start(Arc::clone(&state));
    loop {
        if state.is_shutting_down() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    logging::info("worker 退出");
    Ok(())
}
