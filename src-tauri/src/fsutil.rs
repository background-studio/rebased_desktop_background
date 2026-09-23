use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use uuid::Uuid;

/// Writes next to the target and renames over it, so readers never see a partial file.
/// The temporary name never ends in `.jar`/`.png`, which keeps IntelliJ and our media
/// scanner from picking it up mid-write.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("路径没有父目录：{}", path.display()))?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("创建目录 {} 失败：{error}", directory.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("文件名无效：{}", path.display()))?;
    let temporary = directory.join(format!("{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
        drop(handle);
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("写入 {} 失败：{error}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_file() {
        let root = std::env::temp_dir().join(format!("rebased-fsutil-{}", Uuid::new_v4()));
        let path = root.join("nested").join("file.bin");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        let leftovers = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1);
        let _ = fs::remove_dir_all(root);
    }
}
