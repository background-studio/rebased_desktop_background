use std::{fs, io::ErrorKind, path::PathBuf};

use crate::{
    fsutil::write_atomic,
    rebased::{system_time_to_filetime, Installation},
};

pub const BRIDGE_JAR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/background-studio-bridge.jar"));
pub const BRIDGE_PLUGIN_ID: &str = "studio.background.rebased-bridge";
const BRIDGE_DIR_NAME: &str = "background-studio-bridge";
const BRIDGE_JAR_NAME: &str = "background-studio-bridge.jar";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeInstall {
    Missing,
    /// `installed_at` is the jar's FILETIME; a Rebased process created before it cannot
    /// have loaded this bridge.
    Current {
        installed_at: u64,
    },
    Outdated,
}

pub fn jar_path(installation: &Installation) -> PathBuf {
    installation
        .plugins_dir
        .join(BRIDGE_DIR_NAME)
        .join("lib")
        .join(BRIDGE_JAR_NAME)
}

pub fn inspect(installation: &Installation) -> Result<BridgeInstall, String> {
    let path = jar_path(installation);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BridgeInstall::Missing),
        Err(error) => return Err(format!("读取桥接插件 {} 失败：{error}", path.display())),
    };
    if bytes != BRIDGE_JAR {
        return Ok(BridgeInstall::Outdated);
    }
    Ok(BridgeInstall::Current {
        installed_at: modified_filetime(&path)?,
    })
}

pub fn install(installation: &Installation) -> Result<u64, String> {
    let path = jar_path(installation);
    write_atomic(&path, BRIDGE_JAR)?;
    modified_filetime(&path)
}

pub fn is_disabled_in_ide(installation: &Installation) -> Result<bool, String> {
    let path = installation.config_dir.join("disabled_plugins.txt");
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text.lines().any(|line| line.trim() == BRIDGE_PLUGIN_ID)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("读取 {} 失败：{error}", path.display())),
    }
}

fn modified_filetime(path: &std::path::Path) -> Result<u64, String> {
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("读取 {} 的修改时间失败：{error}", path.display()))?;
    Ok(system_time_to_filetime(modified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn fake_installation() -> Installation {
        let root = std::env::temp_dir().join(format!("rebased-bridge-{}", uuid::Uuid::new_v4()));
        Installation {
            home: root.join("home"),
            launcher: root.join("home\\bin\\rebased64.exe"),
            version: "1.1.18".to_string(),
            build: "262.1".to_string(),
            config_dir: root.join("config"),
            plugins_dir: root.join("config\\plugins"),
            log_dir: root.join("log"),
        }
    }

    #[test]
    fn embedded_jar_declares_the_bridge_plugin() {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(BRIDGE_JAR)).unwrap();
        let mut descriptor = String::new();
        archive
            .by_name("META-INF/plugin.xml")
            .unwrap()
            .read_to_string(&mut descriptor)
            .unwrap();
        assert!(descriptor.contains(&format!("<id>{BRIDGE_PLUGIN_ID}</id>")));
        assert!(descriptor.contains(&format!("<version>{}</version>", env!("CARGO_PKG_VERSION"))));
        assert!(archive
            .by_name("studio/background/rebased/bridge/BridgeLifecycle.class")
            .is_ok());
        assert!(archive
            .by_name("studio/background/rebased/bridge/BridgeBuildInfo.class")
            .is_ok());
        assert!(archive.file_names().all(|name| {
            name.starts_with("META-INF/") || name.starts_with("studio/background/rebased/bridge/")
        }));
    }

    #[test]
    fn install_inspect_and_disabled_detection() {
        let installation = fake_installation();
        assert_eq!(inspect(&installation).unwrap(), BridgeInstall::Missing);
        let installed_at = install(&installation).unwrap();
        assert_eq!(
            inspect(&installation).unwrap(),
            BridgeInstall::Current { installed_at }
        );
        fs::write(jar_path(&installation), b"old").unwrap();
        assert_eq!(inspect(&installation).unwrap(), BridgeInstall::Outdated);

        assert!(!is_disabled_in_ide(&installation).unwrap());
        fs::write(
            installation.config_dir.join("disabled_plugins.txt"),
            format!("com.example.other\n{BRIDGE_PLUGIN_ID}\r\n"),
        )
        .unwrap();
        assert!(is_disabled_in_ide(&installation).unwrap());
        let _ = fs::remove_dir_all(installation.home.parent().unwrap());
    }
}
