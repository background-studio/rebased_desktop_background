use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, watch},
    time::{sleep_until, timeout, Instant as TokioInstant},
};

use crate::{
    logging,
    protocol::{valid_target_id, MAX_WINDOWS},
    worker::WorkerState,
};

pub const BRIDGE_PROTOCOL: u64 = 2;
pub const MAX_BRIDGE_LINE_BYTES: usize = 256 * 1024;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const REPORT_TIMEOUT: Duration = Duration::from_secs(15);
const HEARTBEAT: Duration = Duration::from_secs(2);
const DISCOVERY_GRACE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowSpec {
    pub image: String,
    pub opacity: u8,
    pub fill: &'static str,
    pub anchor: &'static str,
    pub text_contrast: u8,
    pub editor: bool,
    pub frame: bool,
}

/// What every connected bridge should display. `Hold` keeps whatever the IDE shows right
/// now, which is what a freshly started worker wants until the shell sends `configure`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesiredMode {
    Hold,
    Clear,
    Show(ShowSpec),
    Windows(BTreeMap<String, ShowSpec>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desired {
    pub seq: u64,
    pub mode: DesiredMode,
}

impl Desired {
    pub fn to_json(&self) -> Value {
        match &self.mode {
            DesiredMode::Hold => json!({ "type": "desired", "seq": self.seq, "mode": "hold" }),
            DesiredMode::Clear => json!({ "type": "desired", "seq": self.seq, "mode": "clear" }),
            DesiredMode::Show(show) => json!({
                "type": "desired",
                "seq": self.seq,
                "mode": "show",
                "image": show.image,
                "opacity": show.opacity,
                "fill": show.fill,
                "anchor": show.anchor,
                "textContrast": show.text_contrast,
                "editor": show.editor,
                "frame": show.frame,
            }),
            DesiredMode::Windows(windows) => {
                let mut map = serde_json::Map::new();
                for (target_id, show) in windows {
                    map.insert(target_id.clone(), show_spec_json(show));
                }
                json!({
                    "type": "desired",
                    "seq": self.seq,
                    "mode": "windows",
                    "windows": map,
                })
            }
        }
    }

    fn describe(&self) -> String {
        match &self.mode {
            DesiredMode::Hold => format!("#{} hold", self.seq),
            DesiredMode::Clear => format!("#{} clear", self.seq),
            DesiredMode::Show(show) => format!(
                "#{} show {} opacity={} {} {} textContrast={} editor={} frame={}",
                self.seq,
                file_name(&show.image),
                show.opacity,
                show.fill,
                show.anchor,
                show.text_contrast,
                show.editor,
                show.frame
            ),
            DesiredMode::Windows(windows) => {
                format!("#{} windows count={}", self.seq, windows.len())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeReport {
    pub seq: u64,
    pub ok: bool,
    pub message: String,
    pub conflicts: String,
    pub target_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct BridgeSession {
    pub pid: u32,
    pub bridge_version: String,
    pub ide_build: String,
    pub last_report: Option<BridgeReport>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AckOutcome {
    NoSession,
    Reported(Vec<BridgeReport>),
    TimedOut,
}

pub struct BridgeHub {
    desired: watch::Sender<Desired>,
    sessions: Mutex<HashMap<u64, BridgeSession>>,
    known_targets: Mutex<HashMap<u32, (Vec<String>, Instant)>>,
    changed: Condvar,
    next_session: AtomicU64,
}

impl BridgeHub {
    pub fn new(initial: DesiredMode) -> Self {
        let (desired, _) = watch::channel(Desired {
            seq: 1,
            mode: initial,
        });
        Self {
            desired,
            sessions: Mutex::new(HashMap::new()),
            known_targets: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
            next_session: AtomicU64::new(1),
        }
    }

    /// Returns the sequence number the bridges must acknowledge. `force` bumps it even when
    /// the mode is unchanged so bridges re-apply and re-check for conflicting user settings.
    pub fn publish(&self, mode: DesiredMode, force: bool) -> u64 {
        let mut seq = 0;
        let mut published = None;
        self.desired.send_if_modified(|desired| {
            if !force && desired.mode == mode {
                seq = desired.seq;
                return false;
            }
            desired.seq += 1;
            desired.mode = mode;
            seq = desired.seq;
            published = Some(desired.describe());
            true
        });
        if let Some(description) = published {
            logging::info(&format!("发布桥接目标 {description}"));
        }
        seq
    }

    pub fn current(&self) -> Desired {
        self.desired.borrow().clone()
    }

    pub fn sessions(&self) -> Vec<BridgeSession> {
        match self.sessions.lock() {
            Ok(sessions) => sessions.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn discovered_target_ids(&self) -> Vec<String> {
        let Ok(sessions) = self.sessions.lock() else {
            return Vec::new();
        };
        let Ok(mut known) = self.known_targets.lock() else {
            return Vec::new();
        };
        known.retain(|pid, (_, seen)| {
            seen.elapsed() < DISCOVERY_GRACE
                || sessions
                    .values()
                    .any(|session| session.pid == *pid && session.last_report.is_some())
        });
        let mut ids = Vec::new();
        for (targets, _) in known.values() {
            for id in targets {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.clone());
                }
            }
        }
        ids.sort();
        ids
    }

    pub fn owner_discovery_expired(&self, target: &str) -> bool {
        let Some(pid) = target
            .split('-')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return false;
        };
        let Ok(sessions) = self.sessions.lock() else {
            return false;
        };
        if sessions
            .values()
            .any(|session| session.pid == pid && session.last_report.is_some())
        {
            return false;
        }
        self.known_targets.lock().is_ok_and(|known| {
            known
                .get(&pid)
                .is_none_or(|(_, seen)| seen.elapsed() >= DISCOVERY_GRACE)
        })
    }

    pub fn wait_for_ack(&self, seq: u64, limit: Duration) -> AckOutcome {
        let deadline = Instant::now() + limit;
        let Ok(mut sessions) = self.sessions.lock() else {
            return AckOutcome::TimedOut;
        };
        loop {
            if sessions.is_empty() {
                return AckOutcome::NoSession;
            }
            let reports: Vec<BridgeReport> = sessions
                .values()
                .filter_map(|session| session.last_report.clone())
                .filter(|report| report.seq >= seq)
                .collect();
            if reports.len() == sessions.len() {
                return AckOutcome::Reported(reports);
            }
            let now = Instant::now();
            if now >= deadline {
                return AckOutcome::TimedOut;
            }
            sessions = match self.changed.wait_timeout(sessions, deadline - now) {
                Ok((guard, _)) => guard,
                Err(_) => return AckOutcome::TimedOut,
            };
        }
    }

    pub fn wait_for_session(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let Ok(mut sessions) = self.sessions.lock() else {
            return false;
        };
        while sessions.is_empty() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            sessions = match self.changed.wait_timeout(sessions, deadline - now) {
                Ok((guard, _)) => guard,
                Err(_) => return false,
            };
        }
        true
    }

    fn subscribe(&self) -> watch::Receiver<Desired> {
        self.desired.subscribe()
    }

    pub(crate) fn register(&self, session: BridgeSession) -> u64 {
        let id = self.next_session.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(report) = &session.last_report {
                if let Ok(mut known) = self.known_targets.lock() {
                    known.insert(session.pid, (report.target_ids.clone(), Instant::now()));
                }
            }
            sessions.insert(id, session);
        }
        self.changed.notify_all();
        id
    }

    pub(crate) fn record_report(&self, id: u64, report: BridgeReport) {
        let mut changed_text = None;
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get_mut(&id) {
                if let Ok(mut known) = self.known_targets.lock() {
                    known.insert(session.pid, (report.target_ids.clone(), Instant::now()));
                }
                let previous = session.last_report.replace(report.clone());
                let differs = previous.as_ref().is_none_or(|previous| {
                    previous.seq != report.seq
                        || previous.ok != report.ok
                        || previous.message != report.message
                        || previous.target_ids != report.target_ids
                });
                if differs {
                    changed_text = Some(format!(
                        "桥接回报 pid {} #{} ok={} {}",
                        session.pid, report.seq, report.ok, report.message
                    ));
                }
            }
        }
        self.changed.notify_all();
        if let Some(text) = changed_text {
            logging::info(&text);
        }
    }

    pub(crate) fn remove(&self, id: u64) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&id);
        }
        self.changed.notify_all();
    }
}

pub fn start(state: Arc<WorkerState>) {
    tokio::spawn(async move {
        if let Err(error) = serve(Arc::clone(&state)).await {
            logging::warn(&format!("桥接管道失败：{error}"));
            let _ = state.shutdown();
        }
    });
}

#[cfg(windows)]
async fn serve(state: Arc<WorkerState>) -> Result<(), String> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let pipe_name = crate::plugin::BRIDGE_PIPE_NAME;
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(pipe_name)
        .map_err(|error| format!("创建桥接管道失败：{error}"))?;
    loop {
        if state.is_shutting_down() {
            break;
        }
        server
            .connect()
            .await
            .map_err(|error| format!("等待桥接连接失败：{error}"))?;
        let connected = server;
        if state.is_shutting_down() {
            break;
        }
        server = ServerOptions::new()
            .create(pipe_name)
            .map_err(|error| format!("重建桥接管道失败：{error}"))?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_client(state, connected).await {
                logging::warn(&format!("桥接会话结束：{error}"));
            }
        });
    }
    Ok(())
}

#[cfg(not(windows))]
async fn serve(_state: Arc<WorkerState>) -> Result<(), String> {
    Err("桥接管道仅支持 Windows。".to_string())
}

#[cfg(windows)]
fn client_process_id(
    pipe: &tokio::net::windows::named_pipe::NamedPipeServer,
) -> Result<u32, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;

    let mut pid = 0u32;
    let ok = unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) };
    if ok == 0 {
        return Err(format!(
            "读取桥接客户端 pid 失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(pid)
}

#[cfg(windows)]
async fn handle_client(
    state: Arc<WorkerState>,
    pipe: tokio::net::windows::named_pipe::NamedPipeServer,
) -> Result<(), String> {
    let client_pid = client_process_id(&pipe)?;
    let verdict = WorkerState::verify_bridge_client(client_pid);
    let (reader, mut writer) = tokio::io::split(pipe);
    let mut reader = BufReader::new(reader);
    if let Err(reason) = verdict {
        send_reject(&mut writer, &reason).await;
        return Err(reason);
    }
    let hello = match timeout(HELLO_TIMEOUT, read_bounded_line(&mut reader)).await {
        Ok(Ok(Some(line))) => parse_hello(&line, client_pid),
        Ok(Ok(None)) => Err("桥接插件没发 hello 就断开了。".to_string()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("桥接插件 5 秒内没有发送 hello。".to_string()),
    };
    let session = match hello {
        Ok(session) => session,
        Err(reason) => {
            send_reject(&mut writer, &reason).await;
            return Err(reason);
        }
    };
    logging::info(&format!(
        "桥接已连接：Rebased pid {}，build {}，桥接 {}",
        session.pid, session.ide_build, session.bridge_version
    ));
    let hub = state.bridge();
    let id = hub.register(session);
    let result = run_session(hub, id, reader, writer).await;
    hub.remove(id);
    logging::info(&format!("桥接已断开：pid {client_pid}"));
    result
}

async fn send_reject<W: AsyncWrite + Unpin>(writer: &mut W, reason: &str) {
    logging::warn(&format!("拒绝桥接连接：{reason}"));
    let _ = write_json(writer, &json!({ "type": "reject", "message": reason })).await;
}

async fn run_session<R, W>(
    hub: &BridgeHub,
    id: u64,
    mut reader: R,
    mut writer: W,
) -> Result<(), String>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let (line_tx, mut line_rx) = mpsc::channel::<Result<String, String>>(4);
    let reader_task = tokio::spawn(async move {
        loop {
            match read_bounded_line(&mut reader).await {
                Ok(Some(line)) => {
                    if line_tx.send(Ok(line)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = line_tx.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    let result = session_loop(hub, id, &mut line_rx, &mut writer).await;
    reader_task.abort();
    result
}

async fn session_loop<W: AsyncWrite + Unpin>(
    hub: &BridgeHub,
    id: u64,
    lines: &mut mpsc::Receiver<Result<String, String>>,
    writer: &mut W,
) -> Result<(), String> {
    let mut desired = hub.subscribe();
    let mut awaiting: VecDeque<(u64, TokioInstant)> = VecDeque::new();
    let current = desired.borrow_and_update().clone();
    write_json(writer, &current.to_json()).await?;
    awaiting.push_back((current.seq, TokioInstant::now()));
    let mut heartbeat = TokioInstant::now() + HEARTBEAT;
    loop {
        let report_deadline = awaiting
            .front()
            .map(|(_, sent)| *sent + REPORT_TIMEOUT)
            .unwrap_or(heartbeat);
        tokio::select! {
            line = lines.recv() => {
                let line = match line {
                    None => return Ok(()),
                    Some(Err(error)) => return Err(error),
                    Some(Ok(line)) => line,
                };
                let report = parse_report(&line)?;
                let pid = hub.sessions.lock().map_err(|_| "桥接会话锁失效。")?
                    .get(&id).map(|session| session.pid).ok_or("桥接会话已关闭。")?;
                validate_report_owner(&report, pid)?;
                match awaiting.pop_front() {
                    Some((seq, _)) if seq == report.seq => hub.record_report(id, report),
                    expected => {
                        return Err(format!(
                            "桥接回报 seq {}，但 worker 在等 {:?}。",
                            report.seq,
                            expected.map(|(seq, _)| seq)
                        ))
                    }
                }
            }
            changed = desired.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let current = desired.borrow_and_update().clone();
                write_json(writer, &current.to_json()).await?;
                awaiting.push_back((current.seq, TokioInstant::now()));
                heartbeat = TokioInstant::now() + HEARTBEAT;
            }
            _ = sleep_until(heartbeat) => {
                if awaiting.is_empty() {
                    let current = hub.current();
                    write_json(writer, &current.to_json()).await?;
                    awaiting.push_back((current.seq, TokioInstant::now()));
                }
                heartbeat = TokioInstant::now() + HEARTBEAT;
            }
            _ = sleep_until(report_deadline), if !awaiting.is_empty() => {
                return Err(format!(
                    "桥接插件 {} 秒内没有回报。",
                    REPORT_TIMEOUT.as_secs()
                ));
            }
        }
    }
}

async fn write_json<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> Result<(), String> {
    let mut payload = serde_json::to_string(value).map_err(|error| error.to_string())?;
    payload.push('\n');
    if payload.len() > MAX_BRIDGE_LINE_BYTES {
        return Err("桥接消息过长。".to_string());
    }
    writer
        .write_all(payload.as_bytes())
        .await
        .map_err(|error| format!("写入桥接管道失败：{error}"))?;
    writer
        .flush()
        .await
        .map_err(|error| format!("写入桥接管道失败：{error}"))
}

async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<String>, String> {
    let mut buffer = Vec::new();
    let read = reader
        .take(MAX_BRIDGE_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut buffer)
        .await
        .map_err(|error| format!("读取桥接管道失败：{error}"))?;
    if read == 0 {
        return Ok(None);
    }
    if buffer.len() > MAX_BRIDGE_LINE_BYTES {
        return Err("桥接消息过长。".to_string());
    }
    if buffer.pop() != Some(b'\n') {
        return Err("桥接消息在行中途断开。".to_string());
    }
    if buffer.last() == Some(&b'\r') {
        buffer.pop();
    }
    String::from_utf8(buffer)
        .map(Some)
        .map_err(|_| "桥接消息不是有效 UTF-8。".to_string())
}

fn parse_object(line: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err("桥接消息必须是 JSON 对象。".to_string()),
        Err(error) => Err(format!("桥接消息不是有效 JSON：{error}")),
    }
}

fn bounded_string(object: &Map<String, Value>, key: &str, max: usize) -> Result<String, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("桥接消息缺少字符串字段 {key}。"))?;
    if value.chars().count() > max {
        return Err(format!("桥接消息字段 {key} 过长。"));
    }
    Ok(value.to_string())
}

fn parse_hello(line: &str, client_pid: u32) -> Result<BridgeSession, String> {
    let hello = parse_object(line)?;
    if hello.get("type").and_then(Value::as_str) != Some("hello") {
        return Err("桥接插件的第一条消息必须是 hello。".to_string());
    }
    let protocol = hello.get("bridgeProtocol").and_then(Value::as_u64);
    if protocol != Some(BRIDGE_PROTOCOL) {
        return Err(format!(
            "桥接协议 {protocol:?} 与 worker 的 {BRIDGE_PROTOCOL} 不兼容，请重启 Rebased 加载新版桥接插件。"
        ));
    }
    let pid = hello.get("pid").and_then(Value::as_u64);
    if pid != Some(u64::from(client_pid)) {
        return Err(format!(
            "hello.pid={pid:?} 与管道客户端 pid {client_pid} 不一致。"
        ));
    }
    let bridge_version = bounded_string(&hello, "bridgeVersion", 64)?;
    if bridge_version.is_empty() {
        return Err("hello.bridgeVersion 为空。".to_string());
    }
    Ok(BridgeSession {
        pid: client_pid,
        bridge_version,
        ide_build: bounded_string(&hello, "ideBuild", 128)?,
        last_report: None,
    })
}

fn parse_report(line: &str) -> Result<BridgeReport, String> {
    let report = parse_object(line)?;
    if report.get("type").and_then(Value::as_str) != Some("report") {
        return Err("桥接插件发来了非 report 消息。".to_string());
    }
    Ok(BridgeReport {
        seq: report
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| "report 缺少 seq。".to_string())?,
        ok: report
            .get("ok")
            .and_then(Value::as_bool)
            .ok_or_else(|| "report 缺少 ok。".to_string())?,
        message: bounded_string(&report, "message", 2048)?,
        conflicts: bounded_string(&report, "conflicts", 1024)?,
        target_ids: parse_target_ids(&report)?,
    })
}

fn parse_target_ids(report: &Map<String, Value>) -> Result<Vec<String>, String> {
    let values = report
        .get("targetIds")
        .and_then(Value::as_array)
        .ok_or_else(|| "report 缺少 targetIds。".to_string())?;
    if values.len() > MAX_WINDOWS {
        return Err(format!("report.targetIds 超过 {MAX_WINDOWS} 个窗口上限。"));
    }
    let mut ids = Vec::new();
    for value in values {
        let id = value
            .as_str()
            .ok_or_else(|| "report.targetIds 必须是字符串数组。".to_string())?;
        if !valid_target_id(id) {
            return Err("report.targetIds 包含无效 targetId。".to_string());
        }
        if !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    Ok(ids)
}

fn validate_report_owner(report: &BridgeReport, pid: u32) -> Result<(), String> {
    let prefix = format!("{pid}-");
    if report
        .target_ids
        .iter()
        .any(|target| !target.starts_with(&prefix))
    {
        return Err("桥接 targetId 不属于当前 Rebased 进程。".to_string());
    }
    Ok(())
}

fn show_spec_json(show: &ShowSpec) -> Value {
    json!({
        "image": show.image,
        "opacity": show.opacity,
        "fill": show.fill,
        "anchor": show.anchor,
        "textContrast": show.text_contrast,
        "editor": show.editor,
        "frame": show.frame,
    })
}

pub fn windows_desired(windows: BTreeMap<String, ShowSpec>) -> Result<DesiredMode, String> {
    if windows.len() > MAX_WINDOWS {
        return Err(format!("独立窗口配置超过 {MAX_WINDOWS} 个上限。"));
    }
    for target_id in windows.keys() {
        if !valid_target_id(target_id) {
            return Err("独立窗口 targetId 无效。".to_string());
        }
    }
    let mode = DesiredMode::Windows(windows);
    let json = Desired {
        seq: u64::MAX,
        mode: mode.clone(),
    }
    .to_json();
    if serde_json::to_vec(&json)
        .map_err(|error| error.to_string())?
        .len()
        + 1
        > MAX_BRIDGE_LINE_BYTES
    {
        return Err("窗口背景快照超过桥接消息大小上限。".to_string());
    }
    Ok(mode)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, split};

    fn show(image: &str) -> DesiredMode {
        DesiredMode::Show(ShowSpec {
            image: image.to_string(),
            opacity: 20,
            fill: "SCALE",
            anchor: "CENTER",
            text_contrast: 50,
            editor: true,
            frame: false,
        })
    }

    #[test]
    fn publish_only_bumps_on_change_or_force() {
        let hub = BridgeHub::new(DesiredMode::Hold);
        assert_eq!(hub.current().seq, 1);
        assert_eq!(hub.publish(DesiredMode::Hold, false), 1);
        assert_eq!(hub.publish(show("a.png"), false), 2);
        assert_eq!(hub.publish(show("a.png"), false), 2);
        assert_eq!(hub.publish(show("a.png"), true), 3);
        let json = hub.current().to_json();
        assert_eq!(json["mode"], "show");
        assert_eq!(json["opacity"], 20);
        assert_eq!(json["textContrast"], 50);
        assert_eq!(json["frame"], false);
        assert_eq!(
            hub.wait_for_ack(3, Duration::from_millis(10)),
            AckOutcome::NoSession
        );
    }

    #[test]
    fn validates_hello_and_report() {
        let hello = r#"{"type":"hello","bridgeProtocol":2,"bridgeVersion":"0.1.0","pid":42,"ideBuild":"RB-262.1"}"#;
        let session = parse_hello(hello, 42).unwrap();
        assert_eq!(session.ide_build, "RB-262.1");
        assert!(parse_hello(hello, 43).unwrap_err().contains("不一致"));
        let old = hello.replace("\"bridgeProtocol\":2", "\"bridgeProtocol\":1");
        let error = parse_hello(&old, 42).unwrap_err();
        assert!(error.contains("不兼容"), "{error}");
        assert!(error.contains("加载新版桥接插件"), "{error}");
        let report = parse_report(
            r#"{"type":"report","seq":7,"ok":false,"message":"冲突","conflicts":"全局·编辑器","targetIds":["42-abcdef0123456789abcdef01234567"]}"#,
        )
        .unwrap();
        assert_eq!(report.seq, 7);
        assert!(!report.ok);
        assert_eq!(
            report.target_ids,
            vec!["42-abcdef0123456789abcdef01234567".to_string()]
        );
        assert!(
            parse_report(r#"{"type":"report","seq":7,"ok":true,"message":"","conflicts":""}"#)
                .is_err()
        );
        assert!(parse_report(
            r#"{"type":"report","seq":7,"ok":true,"message":"","conflicts":"","targetIds":["../bad"]}"#
        )
        .is_err());
    }

    #[test]
    fn windows_desired_json_and_limits() {
        let mut windows = BTreeMap::new();
        windows.insert(
            "1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ShowSpec {
                image: r"C:\media\a.png".to_string(),
                opacity: 20,
                fill: "SCALE",
                anchor: "CENTER",
                text_contrast: 50,
                editor: true,
                frame: true,
            },
        );
        let mode = windows_desired(windows).unwrap();
        let desired = Desired { seq: 9, mode };
        let json = desired.to_json();
        assert_eq!(json["mode"], "windows");
        assert_eq!(json["seq"], 9);
        assert_eq!(
            json["windows"]["1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]["opacity"],
            20
        );
        let mut too_many = BTreeMap::new();
        for i in 0..=MAX_WINDOWS {
            too_many.insert(
                format!("{i}-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                ShowSpec {
                    image: "a.png".into(),
                    opacity: 1,
                    fill: "SCALE",
                    anchor: "CENTER",
                    text_contrast: 0,
                    editor: true,
                    frame: true,
                },
            );
        }
        assert!(windows_desired(too_many).is_err());
        let DesiredMode::Show(oversized) = show(&"x".repeat(MAX_BRIDGE_LINE_BYTES)) else {
            unreachable!()
        };
        assert!(windows_desired(BTreeMap::from([("1-window".into(), oversized)])).is_err());
    }

    #[test]
    fn window_discovery_survives_one_process_disconnect_and_expires() {
        let hub = BridgeHub::new(DesiredMode::Hold);
        let report = |pid| BridgeReport {
            seq: 1,
            ok: true,
            message: String::new(),
            conflicts: String::new(),
            target_ids: vec![format!("{pid}-window")],
        };
        let session = |pid| BridgeSession {
            pid,
            bridge_version: "test".into(),
            ide_build: "test".into(),
            last_report: Some(report(pid)),
        };
        let a = hub.register(session(1));
        let b = hub.register(session(2));
        hub.remove(b);
        hub.record_report(a, report(1));
        assert_eq!(hub.discovered_target_ids(), vec!["1-window", "2-window"]);
        assert!(!hub.owner_discovery_expired("2-window"));
        let reconnected = hub.register(BridgeSession {
            last_report: None,
            ..session(2)
        });
        assert_eq!(hub.discovered_target_ids(), vec!["1-window", "2-window"]);
        hub.record_report(reconnected, report(2));
        hub.remove(reconnected);
        hub.known_targets.lock().unwrap().get_mut(&2).unwrap().1 = Instant::now() - DISCOVERY_GRACE;
        assert_eq!(hub.discovered_target_ids(), vec!["1-window"]);
        assert!(hub.owner_discovery_expired("2-window"));
        assert!(validate_report_owner(&report(2), 1).is_err());
        assert!(validate_report_owner(&report(2), 2).is_ok());
    }

    #[tokio::test]
    async fn bounded_reader_rejects_oversized_lines() {
        let payload = format!("{}\n", "x".repeat(MAX_BRIDGE_LINE_BYTES + 1));
        let mut reader = BufReader::new(payload.as_bytes());
        assert!(read_bounded_line(&mut reader).await.is_err());
        let mut reader = BufReader::new(&b"{\"a\":1}\r\nrest"[..]);
        assert_eq!(
            read_bounded_line(&mut reader).await.unwrap().unwrap(),
            "{\"a\":1}"
        );
        assert!(read_bounded_line(&mut reader).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_forwards_desired_and_records_reports() {
        let hub = Arc::new(BridgeHub::new(show("first.png")));
        let (worker_side, bridge_side) = duplex(64 * 1024);
        let (worker_read, worker_write) = split(worker_side);
        let id = hub.register(BridgeSession {
            pid: 1,
            bridge_version: "test".to_string(),
            ide_build: "test".to_string(),
            last_report: None,
        });
        let session_hub = Arc::clone(&hub);
        let session = tokio::spawn(async move {
            run_session(&session_hub, id, BufReader::new(worker_read), worker_write).await
        });

        let (bridge_read, mut bridge_write) = split(bridge_side);
        let mut bridge_read = BufReader::new(bridge_read);
        let first =
            parse_object(&read_bounded_line(&mut bridge_read).await.unwrap().unwrap()).unwrap();
        assert_eq!(first["seq"], 1);
        assert!(first["image"].as_str().unwrap().ends_with("first.png"));
        write_json(
            &mut bridge_write,
            &json!({"type":"report","seq":1,"ok":true,"message":"","conflicts":"","targetIds":["1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}),
        )
        .await
        .unwrap();

        let seq = hub.publish(show("second.png"), false);
        let second =
            parse_object(&read_bounded_line(&mut bridge_read).await.unwrap().unwrap()).unwrap();
        assert_eq!(second["seq"], seq);
        write_json(
            &mut bridge_write,
            &json!({"type":"report","seq":seq,"ok":false,"message":"冲突","conflicts":"x","targetIds":["1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","1-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"]}),
        )
        .await
        .unwrap();
        let waiter = Arc::clone(&hub);
        let outcome =
            tokio::task::spawn_blocking(move || waiter.wait_for_ack(seq, Duration::from_secs(5)))
                .await
                .unwrap();
        match outcome {
            AckOutcome::Reported(reports) => {
                assert_eq!(reports.len(), 1);
                assert!(!reports[0].ok);
                assert_eq!(reports[0].message, "冲突");
                assert_eq!(
                    hub.discovered_target_ids(),
                    vec![
                        "1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                        "1-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()
                    ]
                );
            }
            other => panic!("unexpected {other:?}"),
        }

        write_json(
            &mut bridge_write,
            &json!({"type":"report","seq":99,"ok":true,"message":"","conflicts":"","targetIds":[]}),
        )
        .await
        .unwrap();
        let error = session.await.unwrap().unwrap_err();
        assert!(error.contains("seq 99"), "{error}");
    }
}
