use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    bridge_ipc::{AckOutcome, BridgeHub, BridgeReport, BridgeSession, DesiredMode, ShowSpec},
    bridge_package::{self, BridgeInstall},
    fsutil::write_atomic,
    imaging, lock, logging,
    media::download_configured_media,
    models::RuntimeStatus,
    plugin::{hello_result, PLUGIN_ID, PLUGIN_PROTOCOL},
    protocol::{parse_configure, ConfigureSpec},
    rebased::{self, Installation, InstallationCache, ProcessTracker, RebasedProcess},
};

pub const MSG_UNCONFIGURED: &str = "尚未配置背景";
pub const MSG_BUSY: &str = "上一个操作还在进行中，请稍后再试。";
pub const MSG_WAITING: &str = "等待 Rebased 启动";
pub const MSG_EXISTING: &str =
    "Rebased 已在运行，但还没有加载桥接插件；点「立即接管」会重启 Rebased 完成接管。";
pub const MSG_ACTIVE: &str = "背景已应用到 Rebased";
pub const MSG_APPLYING: &str = "正在把背景应用到 Rebased…";
pub const MSG_CONNECTING: &str = "Rebased 正在启动，等待桥接插件连接…";
pub const MSG_PAUSED: &str = "已暂停 Rebased 背景";
pub const MSG_RESTORED: &str = "已恢复 Rebased 原始外观";
pub const MSG_DISABLED: &str =
    "Background Studio Bridge 在 Rebased 里被禁用了：请在 Rebased 的 Settings → Plugins 里启用它并重启 Rebased。";
const MSG_SCANNING: &str = "正在检测 Rebased…";

const CONFIGURE_ACK_TIMEOUT: Duration = Duration::from_secs(4);
const APPLY_ACK_TIMEOUT: Duration = Duration::from_secs(20);
const CLEAR_ACK_TIMEOUT: Duration = Duration::from_secs(10);
/// Close + relaunch + first ack must stay under the shell's 75 s `apply` timeout. Closing
/// gets the largest share because the user may have to answer Rebased's "Confirm Exit".
const CLOSE_TIMEOUT: Duration = Duration::from_secs(40);
const SESSION_BUDGET: Duration = Duration::from_secs(62);
const LAUNCH_ACK_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const STARTUP_GRACE_FILETIME: u64 = 90 * 10_000_000;

pub fn data_directory() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(value).join("RebasedBackgroundStudio"))
        .ok_or_else(|| "环境变量 LOCALAPPDATA 不存在。".to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Armed,
    Paused,
    Restored,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Armed => "armed",
            Self::Paused => "paused",
            Self::Restored => "restored",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "armed" => Some(Self::Armed),
            "paused" => Some(Self::Paused),
            "restored" => Some(Self::Restored),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct Configured {
    revision: String,
    show: ShowSpec,
}

struct Inner {
    configured: Option<Configured>,
    mode: Mode,
    activity: Option<String>,
    install_error: Option<String>,
}

#[derive(Clone, Debug)]
struct Snapshot {
    installation: Installation,
    processes: Vec<RebasedProcess>,
    bridge: BridgeInstall,
    bridge_disabled: bool,
}

/// State the 1 s environment tick carries over so it does not redo a full process
/// snapshot and re-parse product-info.json every second.
#[derive(Default)]
struct TickScanner {
    tracker: ProcessTracker,
    installations: InstallationCache,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResult {
    plugin_protocol: u32,
    plugin_id: &'static str,
    version: &'static str,
    phase: String,
    message: String,
    active_targets: u32,
    paused: bool,
    configured: bool,
    revision: Option<String>,
}

pub struct WorkerState {
    media_dir: PathBuf,
    state_file: PathBuf,
    inner: Mutex<Inner>,
    environment: Mutex<Option<Result<Snapshot, String>>>,
    scanner: Mutex<TickScanner>,
    hub: BridgeHub,
    busy: AtomicBool,
    shutting_down: AtomicBool,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl<'a> BusyGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct ActivityGuard<'a>(&'a WorkerState);

impl Drop for ActivityGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut inner) = lock(&self.0.inner) {
            inner.activity = None;
        }
    }
}

impl WorkerState {
    pub fn load() -> Result<Self, String> {
        Self::load_from(data_directory()?)
    }

    pub fn load_from(data_directory: PathBuf) -> Result<Self, String> {
        let media_dir = data_directory.join("media");
        fs::create_dir_all(&media_dir)
            .map_err(|error| format!("创建 {} 失败：{error}", media_dir.display()))?;
        let state_file = data_directory.join("state.json");
        let mode = read_mode(&state_file)?;
        let initial = match mode {
            Mode::Armed => DesiredMode::Hold,
            Mode::Paused | Mode::Restored => DesiredMode::Clear,
        };
        Ok(Self {
            media_dir,
            state_file,
            inner: Mutex::new(Inner {
                configured: None,
                mode,
                activity: None,
                install_error: None,
            }),
            environment: Mutex::new(None),
            scanner: Mutex::new(TickScanner::default()),
            hub: BridgeHub::new(initial),
            busy: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
        })
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn bridge(&self) -> &BridgeHub {
        &self.hub
    }

    pub fn hello() -> Value {
        hello_result()
    }

    /// The pipe client must be a genuine Rebased launcher process; anything else could
    /// otherwise learn which files the worker writes and feed back fake acknowledgements.
    pub fn verify_bridge_client(pid: u32) -> Result<(), String> {
        let image = rebased::process_image_path(pid)
            .ok_or_else(|| format!("读不到桥接客户端（pid {pid}）的进程路径。"))?;
        let home = Path::new(&image)
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("桥接客户端路径无效：{image}"))?;
        let installation = Installation::from_home(home)
            .map_err(|error| format!("桥接客户端 {image} 不是 Rebased：{error}"))?;
        if !installation.is_launcher(&image) {
            return Err(format!(
                "桥接客户端 {image} 不是 Rebased 启动器 {}。",
                installation.launcher.display()
            ));
        }
        Ok(())
    }

    pub fn status_value(&self) -> Result<Value, String> {
        let (configured, revision, mode, activity, install_error) = {
            let inner = lock(&self.inner)?;
            (
                inner.configured.is_some(),
                inner
                    .configured
                    .as_ref()
                    .map(|configured| configured.revision.clone()),
                inner.mode,
                inner.activity.clone(),
                inner.install_error.clone(),
            )
        };
        let environment = lock(&self.environment)?.clone();
        let status = compute_status(&StatusInputs {
            configured,
            mode,
            activity: activity.as_deref(),
            install_error: install_error.as_deref(),
            environment: environment.as_ref(),
            sessions: &self.hub.sessions(),
            desired_seq: self.hub.current().seq,
            now: rebased::now_filetime(),
        });
        let payload = StatusResult {
            plugin_protocol: PLUGIN_PROTOCOL,
            plugin_id: PLUGIN_ID,
            version: env!("CARGO_PKG_VERSION"),
            paused: status.phase == "paused",
            phase: status.phase,
            message: status.message,
            active_targets: status.active_targets,
            configured,
            revision,
        };
        serde_json::to_value(payload).map_err(|error| error.to_string())
    }

    pub async fn configure(self: &Arc<Self>, params: Value) -> Result<Value, String> {
        let spec = parse_configure(&params)?;
        let bytes = download_configured_media(&spec).await?;
        let state = Arc::clone(self);
        tokio::task::spawn_blocking(move || state.configure_blocking(spec, bytes))
            .await
            .map_err(|error| error.to_string())?
    }

    fn configure_blocking(&self, spec: ConfigureSpec, bytes: Vec<u8>) -> Result<Value, String> {
        let started = Instant::now();
        let rendered = imaging::render(&bytes, &spec.media.mime_type, &spec.display)?;
        let image = imaging::store(&self.media_dir, &rendered.png)?;
        let image_text = image
            .to_str()
            .ok_or_else(|| format!("缓存路径不是有效 UTF-8：{}", image.display()))?
            .to_string();
        if image_text.contains(',') {
            return Err(format!(
                "缓存路径含有逗号，IntelliJ 的背景设置无法表示它：{image_text}"
            ));
        }
        let show = ShowSpec {
            image: image_text,
            opacity: spec.display.opacity_percent(),
            fill: spec.display.fit.intellij(),
            anchor: spec.display.anchor.intellij(),
            text_contrast: spec.display.text_contrast_percent(),
            editor: spec.display.editor_enabled,
            frame: spec.display.frame_enabled,
        };
        let mode = {
            let mut inner = lock(&self.inner)?;
            inner.configured = Some(Configured {
                revision: spec.revision.clone(),
                show: show.clone(),
            });
            inner.install_error = None;
            inner.mode
        };
        logging::info(&format!(
            "configure {}：源 {} {} {} B → {} {}x{}，处理 {} ms",
            spec.revision,
            &spec.media.sha256[..12],
            spec.media.mime_type,
            spec.media.byte_size,
            file_name(&image),
            rendered.width,
            rendered.height,
            started.elapsed().as_millis()
        ));
        let result = if mode == Mode::Armed {
            let seq = self.hub.publish(DesiredMode::Show(show), false);
            match self.hub.wait_for_ack(seq, CONFIGURE_ACK_TIMEOUT) {
                AckOutcome::Reported(reports) => first_failure(&reports).map_or(Ok(()), Err),
                AckOutcome::NoSession | AckOutcome::TimedOut => Ok(()),
            }
        } else {
            Ok(())
        };
        imaging::prune(&self.media_dir, &[image]);
        result?;
        self.status_value()
    }

    pub fn apply_blocking(&self) -> Result<Value, String> {
        let Some(_busy) = BusyGuard::acquire(&self.busy) else {
            return Err(MSG_BUSY.to_string());
        };
        let show = lock(&self.inner)?
            .configured
            .as_ref()
            .map(|configured| configured.show.clone())
            .ok_or_else(|| MSG_UNCONFIGURED.to_string())?;
        self.set_mode(Mode::Armed)?;
        logging::info("apply：立即接管");
        let seq = self.hub.publish(DesiredMode::Show(show), true);
        if !self.hub.sessions().is_empty() {
            match self.hub.wait_for_ack(seq, APPLY_ACK_TIMEOUT) {
                AckOutcome::Reported(reports) => {
                    if let Some(error) = first_failure(&reports) {
                        return Err(error);
                    }
                    return self.status_value();
                }
                AckOutcome::TimedOut => {
                    return Err(format!(
                        "桥接插件 {} 秒内没有确认背景。",
                        APPLY_ACK_TIMEOUT.as_secs()
                    ))
                }
                AckOutcome::NoSession => {}
            }
        }
        let result = self.takeover_blocking(seq);
        if let Err(error) = &result {
            logging::warn(&format!("接管失败：{error}"));
        }
        result?;
        self.status_value()
    }

    /// Only restarts a Rebased that provably started before the current bridge jar existed,
    /// so a slideshow-triggered `apply` can never put Rebased into a restart loop.
    fn takeover_blocking(&self, seq: u64) -> Result<(), String> {
        let started = Instant::now();
        let scanned = scan_environment();
        self.store_environment(scanned.clone());
        let snapshot = scanned?;
        let installation = &snapshot.installation;
        if snapshot.bridge_disabled {
            return Err(MSG_DISABLED.to_string());
        }
        if snapshot.processes.is_empty() {
            if !matches!(snapshot.bridge, BridgeInstall::Current { .. }) {
                self.install_bridge(installation)?;
            }
            return Ok(());
        }
        if let BridgeInstall::Current { installed_at } = snapshot.bridge {
            if snapshot
                .processes
                .iter()
                .all(|process| process.created_at > installed_at)
            {
                let _activity = self.activity("正在等待 Rebased 的桥接插件连接…");
                if self.hub.wait_for_session(APPLY_ACK_TIMEOUT) {
                    return self.expect_ack(seq, APPLY_ACK_TIMEOUT);
                }
                let newest = snapshot
                    .processes
                    .iter()
                    .map(|process| process.created_at)
                    .max()
                    .unwrap_or(0);
                if rebased::now_filetime().saturating_sub(newest) < STARTUP_GRACE_FILETIME {
                    logging::info("Rebased 刚启动，桥接插件还在连接中");
                    return Ok(());
                }
                return Err(format!(
                    "Rebased 启动时桥接插件已经装好，但它没有连上 worker；请查看 {} 里 Background Studio Bridge 的日志。",
                    installation.log_file().display()
                ));
            }
        }

        let pids: Vec<u32> = snapshot
            .processes
            .iter()
            .map(|process| process.pid)
            .collect();
        {
            let _activity = self.activity("正在关闭 Rebased 以加载桥接插件…");
            let posted = rebased::request_close(&pids);
            logging::info(&format!(
                "请求关闭 Rebased（pid {pids:?}），发送了 {posted} 个 WM_CLOSE"
            ));
            if posted == 0 {
                return Err(
                    "找不到 Rebased 的窗口，没法正常关闭它；请手动重启 Rebased 后再试。"
                        .to_string(),
                );
            }
            let mut announced_dialog = false;
            let exited = wait_until(CLOSE_TIMEOUT, || {
                if rebased::running_processes(installation)?.is_empty() {
                    return Ok(true);
                }
                if !announced_dialog {
                    if let Some(title) = rebased::dialog_titles(&pids).into_iter().next() {
                        self.set_activity(&format!(
                            "Rebased 弹出了「{title}」，请在 Rebased 里确认退出（Exit），随后会自动重新打开…"
                        ));
                        announced_dialog = true;
                    }
                }
                Ok(false)
            })?;
            if !exited {
                return Err(format!(
                    "Rebased 在 {} 秒内没有退出。如果它弹出了「Confirm Exit」，请点 Exit 后再点「立即接管」。",
                    CLOSE_TIMEOUT.as_secs()
                ));
            }
        }
        if !matches!(
            bridge_package::inspect(installation)?,
            BridgeInstall::Current { .. }
        ) {
            self.install_bridge(installation)?;
        }
        let _activity = self.activity("正在启动 Rebased…");
        let pid = rebased::launch(installation)?;
        logging::info(&format!("已重新启动 Rebased，pid {pid}"));
        if !self
            .hub
            .wait_for_session(SESSION_BUDGET.saturating_sub(started.elapsed()))
        {
            logging::info("Rebased 已重新启动，桥接插件还在连接中");
            return Ok(());
        }
        self.expect_ack(seq, LAUNCH_ACK_TIMEOUT)
    }

    fn expect_ack(&self, seq: u64, limit: Duration) -> Result<(), String> {
        match self.hub.wait_for_ack(seq, limit) {
            AckOutcome::Reported(reports) => first_failure(&reports).map_or(Ok(()), Err),
            AckOutcome::TimedOut => Err(format!("桥接插件 {} 秒内没有确认背景。", limit.as_secs())),
            AckOutcome::NoSession => Err("桥接插件在确认背景之前断开了。".to_string()),
        }
    }

    pub fn pause_blocking(&self) -> Result<Value, String> {
        self.clear_blocking(Mode::Paused)
    }

    pub fn restore_blocking(&self) -> Result<Value, String> {
        self.clear_blocking(Mode::Restored)
    }

    fn clear_blocking(&self, mode: Mode) -> Result<Value, String> {
        let Some(_busy) = BusyGuard::acquire(&self.busy) else {
            return Err(MSG_BUSY.to_string());
        };
        self.set_mode(mode)?;
        logging::info(&format!("{}：清除 Rebased 背景", mode.as_str()));
        let seq = self.hub.publish(DesiredMode::Clear, false);
        match self.hub.wait_for_ack(seq, CLEAR_ACK_TIMEOUT) {
            AckOutcome::Reported(reports) => {
                if let Some(error) = first_failure(&reports) {
                    return Err(error);
                }
            }
            AckOutcome::TimedOut => {
                return Err(format!(
                    "桥接插件 {} 秒内没有确认清除背景。",
                    CLEAR_ACK_TIMEOUT.as_secs()
                ))
            }
            AckOutcome::NoSession => {}
        }
        self.status_value()
    }

    pub fn shutdown(&self) -> Result<Value, String> {
        self.shutting_down.store(true, Ordering::SeqCst);
        Ok(json!({ "shutdown": true, "keptTarget": true }))
    }

    fn tick(&self) {
        let wants_bridge = lock(&self.inner)
            .map(|inner| inner.mode == Mode::Armed && inner.configured.is_some())
            .unwrap_or(false);
        let mut scanned = self.scan_tick();
        let mut install_outcome = None;
        if let Ok(snapshot) = &mut scanned {
            let needs_install = match snapshot.bridge {
                BridgeInstall::Missing => true,
                BridgeInstall::Outdated => snapshot.processes.is_empty(),
                BridgeInstall::Current { .. } => false,
            };
            if wants_bridge
                && needs_install
                && !snapshot.bridge_disabled
                && !self.busy.load(Ordering::Acquire)
            {
                match self.install_bridge(&snapshot.installation) {
                    Ok(installed_at) => {
                        snapshot.bridge = BridgeInstall::Current { installed_at };
                        install_outcome = Some(None);
                    }
                    Err(error) => install_outcome = Some(Some(error)),
                }
            }
        }
        if let Some(outcome) = install_outcome {
            if let Ok(mut inner) = lock(&self.inner) {
                inner.install_error = outcome;
            }
        }
        self.store_environment(scanned);
    }

    fn scan_tick(&self) -> Result<Snapshot, String> {
        let (installation, processes) = {
            let mut scanner = lock(&self.scanner)?;
            let TickScanner {
                tracker,
                installations,
            } = &mut *scanner;
            let launchers = tracker.scan()?;
            let images: Vec<String> = launchers
                .iter()
                .map(|process| process.image.clone())
                .collect();
            let installation = rebased::discover_from(&images, installations)?;
            let processes = rebased::running_processes_in(&installation, &launchers)?;
            (installation, processes)
        };
        finish_snapshot(installation, processes)
    }

    fn install_bridge(&self, installation: &Installation) -> Result<u64, String> {
        let path = bridge_package::jar_path(installation);
        match bridge_package::install(installation) {
            Ok(installed_at) => {
                logging::info(&format!("已安装桥接插件到 {}", path.display()));
                Ok(installed_at)
            }
            Err(error) => {
                let message = format!("安装 Rebased 桥接插件失败：{error}");
                logging::warn(&message);
                Err(message)
            }
        }
    }

    fn store_environment(&self, scanned: Result<Snapshot, String>) {
        let summary = summarize(&scanned);
        let Ok(mut environment) = lock(&self.environment) else {
            return;
        };
        let previous = environment.as_ref().map(summarize);
        *environment = Some(scanned);
        drop(environment);
        if previous.as_deref() != Some(summary.as_str()) {
            logging::info(&format!("Rebased 环境：{summary}"));
        }
    }

    fn activity(&self, text: &str) -> ActivityGuard<'_> {
        self.set_activity(text);
        ActivityGuard(self)
    }

    fn set_activity(&self, text: &str) {
        if let Ok(mut inner) = lock(&self.inner) {
            inner.activity = Some(text.to_string());
        }
        logging::info(text);
    }

    fn set_mode(&self, mode: Mode) -> Result<(), String> {
        let body = serde_json::to_vec(&json!({ "mode": mode.as_str() }))
            .map_err(|error| error.to_string())?;
        write_atomic(&self.state_file, &body)?;
        lock(&self.inner)?.mode = mode;
        Ok(())
    }
}

pub fn start_environment_worker(state: Arc<WorkerState>) -> Result<(), String> {
    thread::Builder::new()
        .name("rebased-environment".to_string())
        .spawn(move || {
            while !state.is_shutting_down() {
                state.tick();
                thread::sleep(Duration::from_secs(1));
            }
        })
        .map(|_| ())
        .map_err(|error| format!("启动 Rebased 检测线程失败：{error}"))
}

fn read_mode(path: &Path) -> Result<Mode, String> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Mode::Armed),
        Err(error) => return Err(format!("读取 {} 失败：{error}", path.display())),
    };
    serde_json::from_slice::<Value>(&raw)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(Value::as_str)
                .and_then(Mode::parse)
        })
        .ok_or_else(|| format!("{} 内容无效，请删除它后重启插件。", path.display()))
}

fn scan_environment() -> Result<Snapshot, String> {
    let installation = rebased::discover()?;
    let processes = rebased::running_processes(&installation)?;
    finish_snapshot(installation, processes)
}

fn finish_snapshot(
    installation: Installation,
    processes: Vec<RebasedProcess>,
) -> Result<Snapshot, String> {
    let bridge = bridge_package::inspect(&installation)?;
    let bridge_disabled = bridge_package::is_disabled_in_ide(&installation)?;
    Ok(Snapshot {
        installation,
        processes,
        bridge,
        bridge_disabled,
    })
}

fn summarize(scanned: &Result<Snapshot, String>) -> String {
    match scanned {
        Err(error) => error.clone(),
        Ok(snapshot) => {
            let pids: Vec<u32> = snapshot
                .processes
                .iter()
                .map(|process| process.pid)
                .collect();
            let bridge = match snapshot.bridge {
                BridgeInstall::Missing => "未安装",
                BridgeInstall::Current { .. } => "已安装",
                BridgeInstall::Outdated => "旧版本",
            };
            format!(
                "{} {}，进程 {pids:?}，桥接{bridge}{}",
                snapshot.installation.home.display(),
                snapshot.installation.version,
                if snapshot.bridge_disabled {
                    "（已禁用）"
                } else {
                    ""
                }
            )
        }
    }
}

fn wait_until(
    limit: Duration,
    mut done: impl FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    let deadline = Instant::now() + limit;
    loop {
        if done()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn first_failure(reports: &[BridgeReport]) -> Option<String> {
    reports
        .iter()
        .find(|report| !report.ok)
        .map(|report| report.message.clone())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

struct StatusInputs<'a> {
    configured: bool,
    mode: Mode,
    activity: Option<&'a str>,
    install_error: Option<&'a str>,
    environment: Option<&'a Result<Snapshot, String>>,
    sessions: &'a [BridgeSession],
    desired_seq: u64,
    now: u64,
}

fn compute_status(inputs: &StatusInputs<'_>) -> RuntimeStatus {
    if let Some(activity) = inputs.activity {
        return RuntimeStatus::new("applying", activity, 0);
    }
    match inputs.mode {
        Mode::Paused => return RuntimeStatus::new("paused", MSG_PAUSED, 0),
        Mode::Restored => return RuntimeStatus::new("idle", MSG_RESTORED, 0),
        Mode::Armed => {}
    }
    if !inputs.configured {
        return RuntimeStatus::new("idle", MSG_UNCONFIGURED, 0);
    }
    if let Some(error) = inputs.install_error {
        return RuntimeStatus::new("error", error, 0);
    }
    if !inputs.sessions.is_empty() {
        let reports: Vec<&BridgeReport> = inputs
            .sessions
            .iter()
            .filter_map(|session| session.last_report.as_ref())
            .collect();
        if let Some(failed) = reports.iter().find(|report| !report.ok) {
            return RuntimeStatus::new("error", failed.message.clone(), 0);
        }
        let acknowledged = reports
            .iter()
            .filter(|report| report.seq >= inputs.desired_seq)
            .count() as u32;
        if acknowledged > 0 {
            return RuntimeStatus::new("active", MSG_ACTIVE, acknowledged);
        }
        return RuntimeStatus::new("applying", MSG_APPLYING, 0);
    }
    let snapshot = match inputs.environment {
        None => return RuntimeStatus::new("idle", MSG_SCANNING, 0),
        Some(Err(error)) => return RuntimeStatus::new("error", error.clone(), 0),
        Some(Ok(snapshot)) => snapshot,
    };
    if snapshot.bridge_disabled {
        return RuntimeStatus::new("error", MSG_DISABLED, 0);
    }
    if snapshot.processes.is_empty() {
        return RuntimeStatus::new("idle", MSG_WAITING, 0);
    }
    if let BridgeInstall::Current { installed_at } = snapshot.bridge {
        if snapshot
            .processes
            .iter()
            .all(|process| process.created_at > installed_at)
        {
            let newest = snapshot
                .processes
                .iter()
                .map(|process| process.created_at)
                .max()
                .unwrap_or(0);
            if inputs.now.saturating_sub(newest) < STARTUP_GRACE_FILETIME {
                return RuntimeStatus::new("connecting", MSG_CONNECTING, 0);
            }
            return RuntimeStatus::new(
                "error",
                format!(
                    "Rebased 已经带着桥接插件启动，但桥接插件没有连上；请查看 {} 里 Background Studio Bridge 的日志。",
                    snapshot.installation.log_file().display()
                ),
                0,
            );
        }
    }
    RuntimeStatus::new("idle", MSG_EXISTING, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use serde_json::json;
    use std::io::Cursor;

    const SECOND: u64 = 10_000_000;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("rebased-worker-{}", uuid::Uuid::new_v4()))
    }

    fn installation() -> Installation {
        Installation {
            home: PathBuf::from(r"D:\Rebased"),
            launcher: PathBuf::from(r"D:\Rebased\bin\rebased64.exe"),
            version: "1.1.18".to_string(),
            build: "262.1".to_string(),
            config_dir: PathBuf::from(r"C:\config"),
            plugins_dir: PathBuf::from(r"C:\config\plugins"),
            log_dir: PathBuf::from(r"C:\log"),
        }
    }

    fn snapshot(processes: &[(u32, u64)], bridge: BridgeInstall) -> Result<Snapshot, String> {
        Ok(Snapshot {
            installation: installation(),
            processes: processes
                .iter()
                .map(|(pid, created_at)| RebasedProcess {
                    pid: *pid,
                    created_at: *created_at,
                })
                .collect(),
            bridge,
            bridge_disabled: false,
        })
    }

    fn session(report: Option<(u64, bool, &str)>) -> BridgeSession {
        BridgeSession {
            pid: 1,
            bridge_version: "0.1.0".to_string(),
            ide_build: "RB-262".to_string(),
            last_report: report.map(|(seq, ok, message)| BridgeReport {
                seq,
                ok,
                message: message.to_string(),
                conflicts: String::new(),
            }),
        }
    }

    fn status(
        mode: Mode,
        environment: Option<&Result<Snapshot, String>>,
        sessions: &[BridgeSession],
    ) -> RuntimeStatus {
        compute_status(&StatusInputs {
            configured: true,
            mode,
            activity: None,
            install_error: None,
            environment,
            sessions,
            desired_seq: 5,
            now: 1_000 * SECOND,
        })
    }

    #[test]
    fn status_reflects_bridge_sessions() {
        let env = snapshot(
            &[(1, 10 * SECOND)],
            BridgeInstall::Current { installed_at: 0 },
        );
        let active = status(Mode::Armed, Some(&env), &[session(Some((5, true, "")))]);
        assert_eq!(
            (active.phase.as_str(), active.active_targets),
            ("active", 1)
        );
        let pending = status(Mode::Armed, Some(&env), &[session(Some((4, true, "")))]);
        assert_eq!(pending.phase, "applying");
        let failed = status(
            Mode::Armed,
            Some(&env),
            &[session(Some((5, false, "冲突")))],
        );
        assert_eq!(
            (failed.phase.as_str(), failed.message.as_str()),
            ("error", "冲突")
        );
        let paused = status(Mode::Paused, Some(&env), &[session(Some((5, true, "")))]);
        assert_eq!(paused.phase, "paused");
        let restored = status(Mode::Restored, Some(&env), &[]);
        assert_eq!(restored.message, MSG_RESTORED);
    }

    #[test]
    fn status_distinguishes_waiting_existing_and_stuck_bridges() {
        let waiting = snapshot(&[], BridgeInstall::Missing);
        assert_eq!(
            status(Mode::Armed, Some(&waiting), &[]).message,
            MSG_WAITING
        );

        let started_before_install = snapshot(
            &[(1, 100 * SECOND)],
            BridgeInstall::Current {
                installed_at: 200 * SECOND,
            },
        );
        assert_eq!(
            status(Mode::Armed, Some(&started_before_install), &[]).message,
            MSG_EXISTING
        );
        let outdated = snapshot(&[(1, 100 * SECOND)], BridgeInstall::Outdated);
        assert_eq!(
            status(Mode::Armed, Some(&outdated), &[]).message,
            MSG_EXISTING
        );

        let just_started = snapshot(
            &[(1, 950 * SECOND)],
            BridgeInstall::Current {
                installed_at: 200 * SECOND,
            },
        );
        assert_eq!(
            status(Mode::Armed, Some(&just_started), &[]).phase,
            "connecting"
        );
        let stuck = snapshot(
            &[(1, 300 * SECOND)],
            BridgeInstall::Current {
                installed_at: 200 * SECOND,
            },
        );
        let stuck = status(Mode::Armed, Some(&stuck), &[]);
        assert_eq!(stuck.phase, "error");
        assert!(stuck.message.contains("idea.log"), "{}", stuck.message);

        let mut disabled = snapshot(&[(1, 10)], BridgeInstall::Current { installed_at: 0 });
        disabled.as_mut().unwrap().bridge_disabled = true;
        assert_eq!(
            status(Mode::Armed, Some(&disabled), &[]).message,
            MSG_DISABLED
        );
        assert_eq!(status(Mode::Armed, None, &[]).message, MSG_SCANNING);
        let missing = Err::<Snapshot, String>(rebased::MSG_NOT_FOUND.to_string());
        assert_eq!(status(Mode::Armed, Some(&missing), &[]).phase, "error");
    }

    #[test]
    fn unconfigured_and_activity_take_priority() {
        let unconfigured = compute_status(&StatusInputs {
            configured: false,
            mode: Mode::Armed,
            activity: None,
            install_error: None,
            environment: None,
            sessions: &[],
            desired_seq: 1,
            now: 0,
        });
        assert_eq!(unconfigured.message, MSG_UNCONFIGURED);
        let busy = compute_status(&StatusInputs {
            configured: true,
            mode: Mode::Paused,
            activity: Some("正在启动 Rebased…"),
            install_error: None,
            environment: None,
            sessions: &[],
            desired_seq: 1,
            now: 0,
        });
        assert_eq!(busy.phase, "applying");
    }

    fn png(color: [u8; 3]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 6, Rgb(color)))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn configure_spec(bytes: &[u8], display: Value) -> ConfigureSpec {
        let params = json!({
            "schemaVersion": 1,
            "revision": "rev-1",
            "media": {
                "url": "http://127.0.0.1:9/media/a?token=t",
                "kind": "image",
                "mimeType": "image/png",
                "sha256": crate::protocol::hex_sha256(bytes),
                "byteSize": bytes.len(),
            },
            "display": display,
        });
        parse_configure(&params).unwrap()
    }

    #[test]
    fn configure_publishes_show_and_pause_restore_persist_mode() {
        let dir = temp_dir();
        let state = WorkerState::load_from(dir.clone()).unwrap();
        assert_eq!(state.hub.current().mode, DesiredMode::Hold);
        assert_eq!(state.apply_blocking().unwrap_err(), MSG_UNCONFIGURED);

        let bytes = png([10, 20, 30]);
        let spec = configure_spec(
            &bytes,
            json!({ "fit": "tile", "opacity": 0.5, "textContrast": 0.3, "frameEnabled": false }),
        );
        let status = state.configure_blocking(spec, bytes).unwrap();
        assert_eq!(status["configured"], true);
        assert_eq!(status["revision"], "rev-1");
        match state.hub.current().mode {
            DesiredMode::Show(show) => {
                assert_eq!((show.fill, show.opacity, show.frame), ("TILE", 50, false));
                assert_eq!(show.text_contrast, 30);
                assert!(Path::new(&show.image).starts_with(dir.join("media")));
                assert!(Path::new(&show.image).is_file());
            }
            other => panic!("unexpected {other:?}"),
        }

        let paused = state.pause_blocking().unwrap();
        assert_eq!(
            (paused["phase"].as_str(), paused["paused"].as_bool()),
            (Some("paused"), Some(true))
        );
        assert_eq!(state.hub.current().mode, DesiredMode::Clear);
        let reloaded = WorkerState::load_from(dir.clone()).unwrap();
        assert_eq!(reloaded.hub.current().mode, DesiredMode::Clear);
        assert_eq!(reloaded.status_value().unwrap()["phase"], "paused");

        let restored = state.restore_blocking().unwrap();
        assert_eq!(restored["message"], MSG_RESTORED);
        assert_eq!(restored["paused"], false);

        let shutdown = state.shutdown().unwrap();
        assert_eq!(shutdown, json!({ "shutdown": true, "keptTarget": true }));
        assert!(state.is_shutting_down());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_state_file_is_reported() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("state.json"), b"{\"mode\":\"dancing\"}").unwrap();
        let error = WorkerState::load_from(dir.clone()).err().unwrap();
        assert!(error.contains("state.json"), "{error}");
        let _ = fs::remove_dir_all(dir);
    }
}
