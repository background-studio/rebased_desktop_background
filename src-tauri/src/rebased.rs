use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

pub const MSG_NOT_FOUND: &str =
    "没有找到 Rebased：注册表 Software\\detachhead\\Rebased 和正在运行的进程里都没有它的安装位置。";

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
    for image in platform::running_images("rebased64.exe")? {
        if let Some(home) = Path::new(&image).parent().and_then(Path::parent) {
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
        match Installation::from_home(home) {
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

    use super::{normalize_path, RebasedProcess};

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

    use super::RebasedProcess;

    pub fn process_image_path(_pid: u32) -> Option<String> {
        None
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
}
