use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn log_path() -> Result<PathBuf, String> {
    Ok(crate::worker::data_directory()?.join("worker.log"))
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

fn write(level: &str, message: &str) {
    if cfg!(test) {
        return;
    }
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    let Ok(path) = log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(&path, path.with_extension("log.1"));
    }
    let line = format!(
        "{} [{level}] {message}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}
