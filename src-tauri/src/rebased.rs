use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

pub const MSG_NOT_FOUND: &str =
    "没有找到 Rebased：注册表 Software\\detachhead\\Rebased 和正在运行的进程里都没有它的安装位置。";
const LAUNCHER_NAME: &str = "rebased64.exe";
/// A PID recycled between two incremental scans is only noticed by a full scan.
const FULL_SCAN_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub home: PathBuf,
    pub launcher: PathBuf,
    pub version: String,
    pub build: String,
    pub config_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebasedProcess {
    pub pid: u32,
    /// FILETIME ticks (100 ns since 1601-01-01 UTC).
    pub created_at: u64,
}

/// A running process whose executable is named like the Rebased launcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherProcess {
    pub pid: u32,
    pub created_at: u64,
    pub image: String,
}

pub enum Probe {
    Launcher(LauncherProcess),
    /// Another program, or one this worker may not open at all (system services, other
    /// users); access rights do not change while a process lives.
    Other,
    /// Opened but not readable right now, e.g. while exiting; asked again next scan.
    Unavailable,
}

pub trait ProcessSource {
    fn process_ids(&self) -> Result<Vec<u32>, String>;
    /// Every running process with a flag telling whether its name matches the launcher.
    fn named_processes(&self, exe_name: &str) -> Result<Vec<(u32, bool)>, String>;
    fn probe(&self, pid: u32, exe_name: &str) -> Probe;
    /// The same checks a snapshot applies (image path readable and named `exe_name`, same
    /// creation time), so exiting processes drop out exactly when they would there.
    fn still_matches(&self, pid: u32, exe_name: &str, created_at: u64) -> bool;
}

pub struct SystemProcesses;

impl ProcessSource for SystemProcesses {
    fn process_ids(&self) -> Result<Vec<u32>, String> {
        platform::process_ids()
    }

    fn named_processes(&self, exe_name: &str) -> Result<Vec<(u32, bool)>, String> {
        platform::named_processes(exe_name)
    }

    fn probe(&self, pid: u32, exe_name: &str) -> Probe {
        platform::probe(pid, exe_name)
    }

    fn still_matches(&self, pid: u32, exe_name: &str, created_at: u64) -> bool {
        platform::still_matches(pid, exe_name, created_at)
    }
}

/// Follows launcher-named processes across the worker's 1 s ticks. A full Toolhelp snapshot
/// costs ~10 ms on a machine with ~500 processes, so between full scans only PIDs that
/// appeared since the previous scan are opened, and known launchers are re-validated by
/// creation time.
pub struct ProcessTracker {
    known: HashMap<u32, Option<LauncherProcess>>,
    last_full_scan: Option<Instant>,
}

impl Default for ProcessTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTracker {
    pub fn new() -> Self {
        Self {
            known: HashMap::new(),
            last_full_scan: None,
        }
    }

    pub fn scan(&mut self) -> Result<Vec<LauncherProcess>, String> {
        self.scan_with(&SystemProcesses, Instant::now())
    }

    pub fn scan_with(
        &mut self,
        source: &impl ProcessSource,
        now: Instant,
    ) -> Result<Vec<LauncherProcess>, String> {
        let full_scan_due = self
            .last_full_scan
            .is_none_or(|last| now.saturating_duration_since(last) >= FULL_SCAN_INTERVAL);
        if full_scan_due {
            self.full_scan(source)?;
            self.last_full_scan = Some(now);
        } else {
            self.incremental_scan(source)?;
        }
        let mut launchers: Vec<LauncherProcess> = self.known.values().flatten().cloned().collect();
        launchers.sort_by_key(|process| process.pid);
        Ok(launchers)
    }

    fn full_scan(&mut self, source: &impl ProcessSource) -> Result<(), String> {
        let processes = source.named_processes(LAUNCHER_NAME)?;
        self.known.clear();
        for (pid, named) in processes {
            if !named {
                self.known.insert(pid, None);
                continue;
            }
            match source.probe(pid, LAUNCHER_NAME) {
                Probe::Launcher(process) => {
                    self.known.insert(pid, Some(process));
                }
                Probe::Other => {
                    self.known.insert(pid, None);
                }
                Probe::Unavailable => {}
            }
        }
        Ok(())
    }

    fn incremental_scan(&mut self, source: &impl ProcessSource) -> Result<(), String> {
        let ids = source.process_ids()?;
        let live: HashSet<u32> = ids.iter().copied().collect();
        self.known.retain(|pid, process| {
            live.contains(pid)
                && process.as_ref().is_none_or(|process| {
                    source.still_matches(*pid, LAUNCHER_NAME, process.created_at)
                })
        });
        for pid in ids {
            if let Entry::Vacant(slot) = self.known.entry(pid) {
                match source.probe(pid, LAUNCHER_NAME) {
                    Probe::Launcher(process) => {
                        slot.insert(Some(process));
                    }
                    Probe::Other => {
                        slot.insert(None);
                    }
                    Probe::Unavailable => {}
                }
            }
        }
        Ok(())
    }
}

/// `Installation::from_home` parses a ~136 KB product-info.json; reuse the result until
/// that file or bin\idea.properties changes.
#[derive(Default)]
pub struct InstallationCache {
    entries: HashMap<PathBuf, (InstallationStamp, Installation)>,
}

type InstallationStamp = [Option<(SystemTime, u64)>; 2];

impl InstallationCache {
    fn from_home(&mut self, home: &Path) -> Result<Installation, String> {
        let stamp = [
            file_stamp(&home.join("product-info.json")),
            file_stamp(&home.join("bin").join("idea.properties")),
        ];
        if let Some((cached_stamp, installation)) = self.entries.get(home) {
            if *cached_stamp == stamp && installation.launcher.is_file() {
                return Ok(installation.clone());
            }
        }
        let installation = Installation::from_home(home)?;
        self.entries
            .insert(home.to_path_buf(), (stamp, installation.clone()));
        Ok(installation)
    }
}

fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductInfo {
    name: String,
    version: String,
    build_number: String,
    data_directory_name: String,
    product_vendor: String,
    launch: Vec<LaunchInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchInfo {
    os: String,
    arch: String,
    launcher_path: String,
}

impl Installation {
    pub fn from_home(home: &Path) -> Result<Self, String> {
        let info_path = home.join("product-info.json");
        let raw = fs::read_to_string(&info_path)
            .map_err(|error| format!("读取 {} 失败：{error}", info_path.display()))?;
        let info: ProductInfo = serde_json::from_str(&raw)
            .map_err(|error| format!("解析 {} 失败：{error}", info_path.display()))?;
        if info.name != "Rebased" {
            return Err(format!(
                "{} 不是 Rebased（name={}）。",
                home.display(),
                info.name
            ));
        }
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "amd64"
        };
        let launch = info
            .launch
            .iter()
            .find(|launch| launch.os == "Windows" && launch.arch == arch)
            .ok_or_else(|| format!("product-info.json 里没有 Windows/{arch} 的启动项。"))?;
        let launcher = home.join(launch.launcher_path.replace('/', "\\"));
        if !launcher.is_file() {
            return Err(format!("Rebased 启动器不存在：{}", launcher.display()));
        }
        let properties_path = home.join("bin").join("idea.properties");
        let properties = match fs::read_to_string(&properties_path) {
            Ok(text) => parse_properties(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(format!("读取 {} 失败：{error}", properties_path.display())),
        };
        let selector = Path::new(&info.product_vendor).join(&info.data_directory_name);
        let config_dir = match properties.get("idea.config.path") {
            Some(value) => expand_property("idea.config.path", value, home)?,
            None => env_dir("APPDATA")?.join(&selector),
        };
        let plugins_dir = match properties.get("idea.plugins.path") {
            Some(value) => expand_property("idea.plugins.path", value, home)?,
            None => config_dir.join("plugins"),
        };
        let log_dir = match (
            properties.get("idea.log.path"),
            properties.get("idea.system.path"),
        ) {
            (Some(value), _) => expand_property("idea.log.path", value, home)?,
            (None, Some(value)) => expand_property("idea.system.path", value, home)?.join("log"),
            (None, None) => env_dir("LOCALAPPDATA")?.join(&selector).join("log"),
        };
        Ok(Self {
            home: home.to_path_buf(),
            launcher,
            version: info.version,
            build: info.build_number,
            config_dir,
            plugins_dir,
            log_dir,
        })
    }

    pub fn log_file(&self) -> PathBuf {
        self.log_dir.join("idea.log")
    }

    pub fn is_launcher(&self, image_path: &str) -> bool {
        normalize_path(image_path) == normalize_path(&self.launcher.to_string_lossy())
    }
}

pub fn discover() -> Result<Installation, String> {
    discover_from(
        &platform::running_images(LAUNCHER_NAME)?,
        &mut InstallationCache::default(),
    )
}

pub fn discover_from(
    running_images: &[String],
    cache: &mut InstallationCache,
) -> Result<Installation, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut push = |home: PathBuf| {
        let key = normalize_path(&home.to_string_lossy());
        if !candidates
            .iter()
            .any(|known| normalize_path(&known.to_string_lossy()) == key)
        {
            candidates.push(home);
        }
    };
    for image in running_images {
        if let Some(home) = Path::new(image).parent().and_then(Path::parent) {
            push(home.to_path_buf());
        }
    }
    for home in platform::registry_homes() {
        push(home);
    }
    if candidates.is_empty() {
        return Err(MSG_NOT_FOUND.to_string());
    }
    let mut errors = Vec::new();
    for home in &candidates {
        match cache.from_home(home) {
            Ok(installation) => return Ok(installation),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "找到了 Rebased 路径但都不可用：{}",
        errors.join("；")
    ))
}

pub fn running_processes(installation: &Installation) -> Result<Vec<RebasedProcess>, String> {
    platform::processes_with_image(&installation.launcher)
}

/// Same result as [`running_processes`], taken from a [`ProcessTracker`] scan.
pub fn running_processes_in(
    installation: &Installation,
    launchers: &[LauncherProcess],
) -> Result<Vec<RebasedProcess>, String> {
    let named_like_tracker = installation
        .launcher
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(LAUNCHER_NAME));
    if !named_like_tracker {
        return running_processes(installation);
    }
    Ok(launchers
        .iter()
        .filter(|process| installation.is_launcher(&process.image))
        .map(|process| RebasedProcess {
            pid: process.pid,
            created_at: process.created_at,
        })
        .collect())
}

pub fn process_image_path(pid: u32) -> Option<String> {
    platform::process_image_path(pid)
}

/// Posts WM_CLOSE to every visible top-level window, exactly like clicking the title-bar
/// close button, so Rebased saves state and may still ask the user to confirm. Minimized
/// windows are restored first, otherwise that confirmation would open out of sight.
pub fn request_close(pids: &[u32]) -> usize {
    platform::post_close(pids)
}

/// Titles of visible owned windows (dialogs) of the given processes, e.g. "Confirm Exit".
pub fn dialog_titles(pids: &[u32]) -> Vec<String> {
    platform::dialog_titles(pids)
}

pub fn launch(installation: &Installation) -> Result<u32, String> {
    platform::launch_detached(&installation.launcher)
}

pub fn now_filetime() -> u64 {
    system_time_to_filetime(SystemTime::now())
}

pub fn system_time_to_filetime(time: SystemTime) -> u64 {
    const UNIX_EPOCH_AS_FILETIME: u64 = 116_444_736_000_000_000;
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => UNIX_EPOCH_AS_FILETIME + (duration.as_nanos() / 100) as u64,
        Err(_) => 0,
    }
}

pub fn normalize_path(value: &str) -> String {
    let value = value.replace('/', "\\");
    let value = value.strip_prefix(r"\\?\").unwrap_or(&value);
    value.trim_end_matches('\\').to_lowercase()
}

fn env_dir(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("环境变量 {name} 不存在。"))
}

fn parse_properties(text: &str) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some(split) = line.find(['=', ':']) else {
            continue;
        };
        let key = line[..split].trim().to_string();
        let raw_value = line[split + 1..].trim();
        let mut value = String::with_capacity(raw_value.len());
        let mut characters = raw_value.chars();
        while let Some(character) = characters.next() {
            if character != '\\' {
                value.push(character);
                continue;
            }
            match characters.next() {
                Some('t') => value.push('\t'),
                Some('n') => value.push('\n'),
                Some('r') => value.push('\r'),
                Some(other) => value.push(other),
                None => {}
            }
        }
        properties.insert(key, value);
    }
    properties
}

fn expand_property(key: &str, value: &str, home: &Path) -> Result<PathBuf, String> {
    let user_home = env_dir("USERPROFILE")?;
    let expanded = value
        .replace("${user.home}", &user_home.to_string_lossy())
        .replace("${idea.home.path}", &home.to_string_lossy())
        .replace("${idea.home}", &home.to_string_lossy());
    if expanded.contains("${") {
        return Err(format!(
            "bin\\idea.properties 的 {key}={value} 用了不支持的变量。"
        ));
    }
    Ok(PathBuf::from(expanded.replace('/', "\\")))
}

#[cfg(windows)]
mod platform {
    use std::{
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, BOOL, ERROR_SUCCESS, FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            ProcessStatus::K32EnumProcesses,
            Registry::{
                RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
                HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, RRF_RT_REG_SZ,
            },
            Threading::{
                GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, CREATE_NEW_PROCESS_GROUP,
                DETACHED_PROCESS, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
            WM_CLOSE,
        },
    };

    use super::{normalize_path, LauncherProcess, Probe, RebasedProcess};

    struct HandleGuard(HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn query_image_path(handle: HANDLE) -> Option<String> {
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }

    fn query_creation_time(handle: HANDLE) -> Option<u64> {
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let ok =
            unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
        if ok == 0 {
            return None;
        }
        Some(((creation.dwHighDateTime as u64) << 32) | u64::from(creation.dwLowDateTime))
    }

    fn open_process(pid: u32) -> Option<HandleGuard> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return None;
        }
        Some(HandleGuard(handle))
    }

    fn for_each_process(mut visit: impl FnMut(u32, &str)) -> Result<(), String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Err("无法创建进程快照。".to_string());
        }
        let _guard = HandleGuard(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            visit(entry.th32ProcessID, &from_wide(&entry.szExeFile));
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        Ok(())
    }

    pub fn process_image_path(pid: u32) -> Option<String> {
        query_image_path(open_process(pid)?.0)
    }

    pub fn running_images(exe_name: &str) -> Result<Vec<String>, String> {
        let mut images = Vec::new();
        for_each_process(|pid, name| {
            if name.eq_ignore_ascii_case(exe_name) {
                if let Some(image) = process_image_path(pid) {
                    images.push(image);
                }
            }
        })?;
        Ok(images)
    }

    pub fn processes_with_image(launcher: &Path) -> Result<Vec<RebasedProcess>, String> {
        let target = normalize_path(&launcher.to_string_lossy());
        let target_name = launcher
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| "Rebased 启动器路径无效。".to_string())?;
        let mut processes = Vec::new();
        for_each_process(|pid, name| {
            if !name.eq_ignore_ascii_case(&target_name) {
                return;
            }
            let Some(handle) = open_process(pid) else {
                return;
            };
            if let (Some(image), Some(created_at)) =
                (query_image_path(handle.0), query_creation_time(handle.0))
            {
                if normalize_path(&image) == target {
                    processes.push(RebasedProcess { pid, created_at });
                }
            }
        })?;
        Ok(processes)
    }

    pub fn process_ids() -> Result<Vec<u32>, String> {
        let mut ids = vec![0u32; 1024];
        loop {
            let capacity = (ids.len() * std::mem::size_of::<u32>()) as u32;
            let mut written = 0u32;
            if unsafe { K32EnumProcesses(ids.as_mut_ptr(), capacity, &mut written) } == 0 {
                return Err("无法枚举进程。".to_string());
            }
            // A completely filled buffer may have been truncated.
            if written < capacity {
                ids.truncate(written as usize / std::mem::size_of::<u32>());
                return Ok(ids);
            }
            ids.resize(ids.len() * 2, 0);
        }
    }

    pub fn named_processes(exe_name: &str) -> Result<Vec<(u32, bool)>, String> {
        let mut processes = Vec::new();
        for_each_process(|pid, name| processes.push((pid, name.eq_ignore_ascii_case(exe_name))))?;
        Ok(processes)
    }

    pub fn probe(pid: u32, exe_name: &str) -> Probe {
        let Some(handle) = open_process(pid) else {
            return Probe::Other;
        };
        let Some(image) = query_image_path(handle.0) else {
            return Probe::Unavailable;
        };
        let named = Path::new(&image)
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(exe_name));
        if !named {
            return Probe::Other;
        }
        match query_creation_time(handle.0) {
            Some(created_at) => Probe::Launcher(LauncherProcess {
                pid,
                created_at,
                image,
            }),
            None => Probe::Unavailable,
        }
    }

    pub fn still_matches(pid: u32, exe_name: &str, created_at: u64) -> bool {
        let Some(handle) = open_process(pid) else {
            return false;
        };
        query_image_path(handle.0).is_some_and(|image| {
            Path::new(&image)
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(exe_name))
        }) && query_creation_time(handle.0) == Some(created_at)
    }

    pub fn registry_homes() -> Vec<PathBuf> {
        let mut homes = Vec::new();
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let mut key: HKEY = std::ptr::null_mut();
            let path = wide(r"Software\detachhead\Rebased");
            let status = unsafe {
                RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut key)
            };
            if status != ERROR_SUCCESS {
                continue;
            }
            let mut builds = Vec::new();
            for index in 0.. {
                let mut name = [0u16; 256];
                let mut length = name.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        key,
                        index,
                        name.as_mut_ptr(),
                        &mut length,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if status != ERROR_SUCCESS {
                    break;
                }
                builds.push(String::from_utf16_lossy(&name[..length as usize]));
            }
            builds.sort_by(|left, right| right.cmp(left));
            for build in builds {
                let subkey = wide(&build);
                let mut buffer = [0u16; 1024];
                let mut size = (buffer.len() * 2) as u32;
                let status = unsafe {
                    RegGetValueW(
                        key,
                        subkey.as_ptr(),
                        std::ptr::null(),
                        RRF_RT_REG_SZ,
                        std::ptr::null_mut(),
                        buffer.as_mut_ptr().cast(),
                        &mut size,
                    )
                };
                if status == ERROR_SUCCESS {
                    let home = from_wide(&buffer);
                    if !home.is_empty() {
                        homes.push(PathBuf::from(home));
                    }
                }
            }
            unsafe {
                RegCloseKey(key);
            }
        }
        homes
    }

    struct WindowSearch<'a> {
        pids: &'a [u32],
        top_level: Vec<HWND>,
        owned: Vec<HWND>,
    }

    unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam as *mut WindowSearch);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if search.pids.contains(&pid) && IsWindowVisible(hwnd) != 0 {
            if GetWindow(hwnd, GW_OWNER).is_null() {
                search.top_level.push(hwnd);
            } else {
                search.owned.push(hwnd);
            }
        }
        1
    }

    fn find_windows(pids: &[u32]) -> WindowSearch<'_> {
        let mut search = WindowSearch {
            pids,
            top_level: Vec::new(),
            owned: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(collect_window),
                &mut search as *mut WindowSearch as LPARAM,
            );
        }
        search
    }

    pub fn post_close(pids: &[u32]) -> usize {
        let windows = find_windows(pids).top_level;
        let mut restored = false;
        for &hwnd in &windows {
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                    restored = true;
                }
            }
        }
        if restored {
            // IntelliJ places "Confirm Exit" relative to the frame; while the frame is still
            // minimized the dialog lands in the screen's top-left corner.
            std::thread::sleep(std::time::Duration::from_millis(600));
        }
        let mut posted = 0;
        for hwnd in windows {
            unsafe {
                SetForegroundWindow(hwnd);
                if PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 {
                    posted += 1;
                }
            }
        }
        posted
    }

    pub fn dialog_titles(pids: &[u32]) -> Vec<String> {
        find_windows(pids)
            .owned
            .into_iter()
            .filter_map(|hwnd| {
                let mut buffer = [0u16; 256];
                let length =
                    unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
                (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
            })
            .collect()
    }

    pub fn launch_detached(launcher: &Path) -> Result<u32, String> {
        let directory = launcher
            .parent()
            .ok_or_else(|| "Rebased 启动器路径无效。".to_string())?;
        let child = Command::new(launcher)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|error| format!("启动 Rebased 失败：{error}"))?;
        Ok(child.id())
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::{Path, PathBuf};

    use super::{Probe, RebasedProcess};

    pub fn process_image_path(_pid: u32) -> Option<String> {
        None
    }

    pub fn process_ids() -> Result<Vec<u32>, String> {
        Err("仅支持 Windows。".to_string())
    }

    pub fn named_processes(_exe_name: &str) -> Result<Vec<(u32, bool)>, String> {
        Err("仅支持 Windows。".to_string())
    }

    pub fn probe(_pid: u32, _exe_name: &str) -> Probe {
        Probe::Unavailable
    }

    pub fn still_matches(_pid: u32, _exe_name: &str, _created_at: u64) -> bool {
        false
    }

    pub fn running_images(_exe_name: &str) -> Result<Vec<String>, String> {
        Err("仅支持 Windows。".to_string())
    }

    pub fn processes_with_image(_launcher: &Path) -> Result<Vec<RebasedProcess>, String> {
        Err("仅支持 Windows。".to_string())
    }

    pub fn registry_homes() -> Vec<PathBuf> {
        Vec::new()
    }

    pub fn post_close(_pids: &[u32]) -> usize {
        0
    }

    pub fn dialog_titles(_pids: &[u32]) -> Vec<String> {
        Vec::new()
    }

    pub fn launch_detached(_launcher: &Path) -> Result<u32, String> {
        Err("仅支持 Windows。".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_home(properties: Option<&str>) -> PathBuf {
        let home = std::env::temp_dir().join(format!("rebased-home-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin").join("rebased64.exe"), b"").unwrap();
        fs::write(
            home.join("product-info.json"),
            r#"{
              "name": "Rebased", "version": "1.1.18", "buildNumber": "262.10968.SNAPSHOT",
              "dataDirectoryName": "IdeaIC1.1", "productVendor": "detachhead",
              "launch": [
                {"os": "Windows", "arch": "amd64", "launcherPath": "bin/rebased64.exe"},
                {"os": "Windows", "arch": "aarch64", "launcherPath": "bin/rebased64.exe"}
              ]
            }"#,
        )
        .unwrap();
        if let Some(properties) = properties {
            fs::write(home.join("bin").join("idea.properties"), properties).unwrap();
        }
        home
    }

    #[test]
    fn derives_default_directories_from_product_info() {
        let home = fake_home(Some(
            "# idea.config.path=${user.home}/nope\nidea.max.intellisense.filesize=2500\n",
        ));
        let installation = Installation::from_home(&home).unwrap();
        let appdata = PathBuf::from(std::env::var_os("APPDATA").unwrap());
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
        assert_eq!(installation.version, "1.1.18");
        assert_eq!(installation.launcher, home.join("bin\\rebased64.exe"));
        assert_eq!(
            installation.config_dir,
            appdata.join("detachhead\\IdeaIC1.1")
        );
        assert_eq!(
            installation.plugins_dir,
            appdata.join("detachhead\\IdeaIC1.1\\plugins")
        );
        assert_eq!(
            installation.log_dir,
            local.join("detachhead\\IdeaIC1.1\\log")
        );
        assert!(installation.is_launcher(&format!(
            r"\\?\{}",
            home.join("BIN\\Rebased64.EXE").display()
        )));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn honours_path_overrides_and_rejects_unknown_variables() {
        let home = fake_home(Some(
            "idea.config.path=${idea.home.path}/data/config\nidea.system.path=D\\:\\\\Portable\\\\system\n",
        ));
        let installation = Installation::from_home(&home).unwrap();
        assert_eq!(installation.config_dir, home.join("data\\config"));
        assert_eq!(installation.plugins_dir, home.join("data\\config\\plugins"));
        assert_eq!(
            installation.log_dir,
            PathBuf::from(r"D:\Portable\system\log")
        );
        let _ = fs::remove_dir_all(&home);

        let home = fake_home(Some("idea.plugins.path=${custom.root}/plugins\n"));
        let error = Installation::from_home(&home).unwrap_err();
        assert!(error.contains("不支持的变量"), "{error}");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn rejects_other_products() {
        let home = fake_home(None);
        let raw = fs::read_to_string(home.join("product-info.json"))
            .unwrap()
            .replace("\"Rebased\"", "\"IntelliJ IDEA\"");
        fs::write(home.join("product-info.json"), raw).unwrap();
        assert!(Installation::from_home(&home)
            .unwrap_err()
            .contains("不是 Rebased"));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn converts_system_time_to_filetime() {
        assert_eq!(system_time_to_filetime(UNIX_EPOCH), 116_444_736_000_000_000);
        assert!(now_filetime() > 116_444_736_000_000_000);
    }

    #[derive(Clone)]
    struct FakeProcess {
        image: String,
        created_at: u64,
        openable: bool,
        readable: bool,
    }

    #[derive(Default)]
    struct FakeProcesses {
        running: std::cell::RefCell<std::collections::BTreeMap<u32, FakeProcess>>,
        probes: std::cell::Cell<usize>,
    }

    impl FakeProcesses {
        fn start(&self, pid: u32, image: &str, created_at: u64) {
            self.running.borrow_mut().insert(
                pid,
                FakeProcess {
                    image: image.to_string(),
                    created_at,
                    openable: true,
                    readable: true,
                },
            );
        }

        fn edit(&self, pid: u32, change: impl FnOnce(&mut FakeProcess)) {
            change(self.running.borrow_mut().get_mut(&pid).unwrap());
        }

        fn exit(&self, pid: u32) {
            self.running.borrow_mut().remove(&pid);
        }

        fn take_probes(&self) -> usize {
            self.probes.replace(0)
        }
    }

    impl ProcessSource for FakeProcesses {
        fn process_ids(&self) -> Result<Vec<u32>, String> {
            Ok(self.running.borrow().keys().copied().collect())
        }

        fn named_processes(&self, exe_name: &str) -> Result<Vec<(u32, bool)>, String> {
            Ok(self
                .running
                .borrow()
                .iter()
                .map(|(pid, process)| {
                    let name = process.image.rsplit('\\').next().unwrap();
                    (*pid, name.eq_ignore_ascii_case(exe_name))
                })
                .collect())
        }

        fn probe(&self, pid: u32, exe_name: &str) -> Probe {
            self.probes.set(self.probes.get() + 1);
            let running = self.running.borrow();
            let Some(process) = running.get(&pid).filter(|process| process.openable) else {
                return Probe::Other;
            };
            if !process.readable {
                return Probe::Unavailable;
            }
            if !process
                .image
                .rsplit('\\')
                .next()
                .unwrap()
                .eq_ignore_ascii_case(exe_name)
            {
                return Probe::Other;
            }
            Probe::Launcher(LauncherProcess {
                pid,
                created_at: process.created_at,
                image: process.image.clone(),
            })
        }

        fn still_matches(&self, pid: u32, exe_name: &str, created_at: u64) -> bool {
            self.running.borrow().get(&pid).is_some_and(|process| {
                process.openable
                    && process.readable
                    && process
                        .image
                        .rsplit('\\')
                        .next()
                        .unwrap()
                        .eq_ignore_ascii_case(exe_name)
                    && process.created_at == created_at
            })
        }
    }

    const LAUNCHER: &str = r"D:\Rebased\bin\rebased64.exe";

    fn pids(launchers: &[LauncherProcess]) -> Vec<(u32, u64)> {
        launchers
            .iter()
            .map(|process| (process.pid, process.created_at))
            .collect()
    }

    #[test]
    fn tracker_only_opens_new_processes_between_full_scans() {
        let system = FakeProcesses::default();
        system.start(4, r"C:\Windows\System32\ntoskrnl.exe", 1);
        system.start(100, r"C:\Windows\explorer.exe", 2);
        system.start(200, LAUNCHER, 3);
        let mut tracker = ProcessTracker::new();
        let start = Instant::now();

        assert_eq!(
            pids(&tracker.scan_with(&system, start).unwrap()),
            [(200, 3)]
        );
        assert_eq!(
            system.take_probes(),
            1,
            "a full scan only opens launcher-named processes"
        );

        let tick = start + Duration::from_secs(1);
        assert_eq!(pids(&tracker.scan_with(&system, tick).unwrap()), [(200, 3)]);
        assert_eq!(system.take_probes(), 0, "nothing new, nothing opened");

        system.start(300, r"C:\Apps\notepad.exe", 4);
        system.start(301, LAUNCHER, 5);
        system.exit(200);
        let tick = start + Duration::from_secs(2);
        assert_eq!(pids(&tracker.scan_with(&system, tick).unwrap()), [(301, 5)]);
        assert_eq!(system.take_probes(), 2);
    }

    #[test]
    fn tracker_notices_a_recycled_launcher_pid() {
        let system = FakeProcesses::default();
        system.start(200, LAUNCHER, 3);
        let mut tracker = ProcessTracker::new();
        let start = Instant::now();
        tracker.scan_with(&system, start).unwrap();

        system.exit(200);
        system.start(200, LAUNCHER, 9);
        let tick = start + Duration::from_secs(1);
        assert_eq!(pids(&tracker.scan_with(&system, tick).unwrap()), [(200, 9)]);

        system.exit(200);
        system.start(200, r"C:\Apps\notepad.exe", 10);
        let tick = start + Duration::from_secs(2);
        assert!(tracker.scan_with(&system, tick).unwrap().is_empty());
    }

    #[test]
    fn tracker_retries_unreadable_but_not_inaccessible_processes() {
        let system = FakeProcesses::default();
        let mut tracker = ProcessTracker::new();
        let start = Instant::now();
        tracker.scan_with(&system, start).unwrap();

        system.start(4, r"C:\Windows\System32\csrss.exe", 1);
        system.edit(4, |process| process.openable = false);
        system.start(200, LAUNCHER, 3);
        system.edit(200, |process| process.readable = false);
        let tick = start + Duration::from_secs(1);
        assert!(tracker.scan_with(&system, tick).unwrap().is_empty());
        assert_eq!(system.take_probes(), 2);

        system.edit(200, |process| process.readable = true);
        let tick = start + Duration::from_secs(2);
        assert_eq!(pids(&tracker.scan_with(&system, tick).unwrap()), [(200, 3)]);
        assert_eq!(
            system.take_probes(),
            1,
            "the inaccessible process is not asked again"
        );
    }

    #[test]
    fn tracker_drops_an_exiting_launcher_like_a_snapshot_does() {
        let system = FakeProcesses::default();
        system.start(200, LAUNCHER, 3);
        let mut tracker = ProcessTracker::new();
        let start = Instant::now();
        tracker.scan_with(&system, start).unwrap();

        system.edit(200, |process| process.readable = false);
        let tick = start + Duration::from_secs(1);
        assert!(tracker.scan_with(&system, tick).unwrap().is_empty());
    }

    #[test]
    fn full_scan_recovers_a_launcher_behind_a_recycled_pid() {
        let system = FakeProcesses::default();
        system.start(300, r"C:\Apps\notepad.exe", 1);
        let mut tracker = ProcessTracker::new();
        let start = Instant::now();
        tracker.scan_with(&system, start).unwrap();

        system.exit(300);
        system.start(300, LAUNCHER, 2);
        let tick = start + Duration::from_secs(1);
        assert!(tracker.scan_with(&system, tick).unwrap().is_empty());
        assert_eq!(
            pids(
                &tracker
                    .scan_with(&system, start + FULL_SCAN_INTERVAL)
                    .unwrap()
            ),
            [(300, 2)]
        );
    }

    #[test]
    fn running_processes_in_keeps_only_this_installation() {
        let home = fake_home(None);
        let installation = Installation::from_home(&home).unwrap();
        let launchers = [
            LauncherProcess {
                pid: 1,
                created_at: 10,
                image: installation.launcher.to_string_lossy().to_uppercase(),
            },
            LauncherProcess {
                pid: 2,
                created_at: 20,
                image: LAUNCHER.to_string(),
            },
        ];
        assert_eq!(
            running_processes_in(&installation, &launchers).unwrap(),
            [RebasedProcess {
                pid: 1,
                created_at: 10
            }]
        );
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn installation_cache_reparses_only_when_product_info_changes() {
        let home = fake_home(None);
        let info = home.join("product-info.json");
        let mut cache = InstallationCache::default();
        assert_eq!(cache.from_home(&home).unwrap().version, "1.1.18");

        let written_at = fs::metadata(&info).unwrap().modified().unwrap();
        let same_length = fs::read_to_string(&info)
            .unwrap()
            .replace("1.1.18", "1.1.19");
        fs::write(&info, same_length).unwrap();
        let file = fs::File::options().write(true).open(&info).unwrap();
        file.set_modified(written_at).unwrap();
        drop(file);
        assert_eq!(
            cache.from_home(&home).unwrap().version,
            "1.1.18",
            "unchanged size and time reuse the parsed result"
        );

        let file = fs::File::options().write(true).open(&info).unwrap();
        file.set_modified(written_at + Duration::from_secs(5))
            .unwrap();
        drop(file);
        assert_eq!(cache.from_home(&home).unwrap().version, "1.1.19");

        fs::remove_file(home.join("bin").join("rebased64.exe")).unwrap();
        assert!(
            cache.from_home(&home).is_err(),
            "a deleted launcher is noticed"
        );
        let _ = fs::remove_dir_all(home);
    }
}
