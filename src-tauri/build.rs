use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use zip::{write::SimpleFileOptions, CompressionMethod, DateTime, ZipWriter};

const PACKAGE: &str = "studio/background/rebased/bridge";

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let bridge = manifest_dir.join("..").join("bridge");
    println!("cargo:rerun-if-changed={}", bridge.display());
    println!("cargo:rerun-if-env-changed=JAVA_HOME");
    println!("cargo:rerun-if-env-changed=REBASED_HOME");

    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let work = out_dir.join("bridge-build");
    if work.exists() {
        fs::remove_dir_all(&work).expect("clean bridge build directory");
    }
    let generated = work.join("gen").join(PACKAGE);
    fs::create_dir_all(&generated).expect("create generated source directory");
    let build_info = generated.join("BridgeBuildInfo.java");
    fs::write(
        &build_info,
        format!(
            "package studio.background.rebased.bridge;\n\n\
             final class BridgeBuildInfo {{\n  static final String VERSION = \"{version}\";\n\n  \
             private BridgeBuildInfo() {{\n  }}\n}}\n"
        ),
    )
    .expect("write BridgeBuildInfo.java");

    let mut sources = Vec::new();
    collect_files(&bridge.join("src"), "java", &mut sources);
    sources.push(build_info);
    sources.sort();

    let classes = work.join("classes");
    fs::create_dir_all(&classes).expect("create classes directory");
    let javac = javac_path();
    let mut command = Command::new(&javac);
    command
        .args([
            "--release",
            "21",
            "-encoding",
            "UTF-8",
            "-proc:none",
            "-Xlint:all",
        ])
        .args([
            "-Xlint:-options",
            "-Xlint:-classfile",
            "-implicit:none",
            "-d",
        ])
        .arg(&classes);
    match env::var_os("REBASED_HOME") {
        // Compile against the real IDE jars to prove the stubs match the running platform.
        Some(home) => {
            command
                .arg("-cp")
                .arg(PathBuf::from(home).join("lib").join("*"));
        }
        None => {
            command.arg("-sourcepath").arg(bridge.join("stubs"));
        }
    }
    command.args(&sources);
    let status = command.status().unwrap_or_else(|error| {
        panic!(
            "无法运行 javac（{}）：{error}。桥接插件需要 JDK 21+，可以把 JAVA_HOME 指向 Rebased 自带的 jbr 目录。",
            javac.display()
        )
    });
    assert!(status.success(), "javac 编译 Rebased 桥接插件失败");

    let plugin_xml = fs::read_to_string(bridge.join("resources/META-INF/plugin.xml"))
        .expect("read plugin.xml")
        .replace("@VERSION@", &version);
    let mut class_files = Vec::new();
    collect_files(&classes, "class", &mut class_files);
    let mut entries: Vec<(String, Vec<u8>)> = class_files
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(&classes)
                .expect("class inside output")
                .to_string_lossy()
                .replace('\\', "/");
            assert!(
                name.starts_with(PACKAGE),
                "桥接 jar 里混进了非桥接类：{name}"
            );
            (name, fs::read(path).expect("read class file"))
        })
        .collect();
    entries.push((
        "META-INF/MANIFEST.MF".to_string(),
        b"Manifest-Version: 1.0\r\n\r\n".to_vec(),
    ));
    entries.push(("META-INF/plugin.xml".to_string(), plugin_xml.into_bytes()));
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let jar_path = out_dir.join("background-studio-bridge.jar");
    let file = fs::File::create(&jar_path).expect("create bridge jar");
    let mut jar = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    for (name, bytes) in entries {
        jar.start_file(name, options).expect("start jar entry");
        jar.write_all(&bytes).expect("write jar entry");
    }
    jar.finish().expect("finish bridge jar");
}

fn javac_path() -> PathBuf {
    let executable = if cfg!(windows) { "javac.exe" } else { "javac" };
    match env::var_os("JAVA_HOME") {
        Some(home) => {
            let path = PathBuf::from(home).join("bin").join(executable);
            assert!(
                path.is_file(),
                "JAVA_HOME 下没有 {}，请指向 JDK 21+（例如 Rebased 自带的 jbr）。",
                path.display()
            );
            path
        }
        None => PathBuf::from(executable),
    }
}

fn collect_files(directory: &Path, extension: &str, output: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("读取 {} 失败：{error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_files(&path, extension, output);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
}
