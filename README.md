# Rebased Background Studio

[![org](https://img.shields.io/badge/org-background--studio-0ea5e9)](https://github.com/background-studio)
[![release](https://img.shields.io/github/v/release/background-studio/rebased_desktop_background)](https://github.com/background-studio/rebased_desktop_background/releases)

属于 [Background Studio](https://github.com/background-studio) 组织。这是给
[Background Studio 壳](https://github.com/background-studio/background-studio)
用的纯 Rust 无界面 worker（Named Pipe 协议 2），外加一个装进 Rebased 的小桥接插件。
没有独立安装器，也没有常驻 UI。

协议见 [docs/plugin-protocol.md](./docs/plugin-protocol.md) 和根级 [plugin.json](./plugin.json)。

一个面向 Windows 上 [Rebased](https://github.com/DetachHead/rebased)（基于 IntelliJ
平台的 Git 客户端）的可逆背景插件。背景通过 IntelliJ 自带的背景图绘制器显示，
只用运行时的 JVM 系统属性，不写 Rebased 的设置，不改 Rebased 的安装目录。

> 非 Rebased 官方产品。Rebased 归其作者所有；IntelliJ 是 JetBrains s.r.o. 的商标。

## 工作方式

- worker 从壳收到回环媒体地址，下载、校验后缩放到长边 3840 以内，按设置做模糊和遮罩，
  存成 `%LOCALAPPDATA%\RebasedBackgroundStudio\media\<sha256>.png`。
- 配置好背景后，worker 把桥接插件 `Background Studio Bridge` 放进 Rebased 的插件目录
  （例如 `%APPDATA%\detachhead\IdeaIC1.1\plugins\background-studio-bridge`）。
- Rebased 启动后，桥接插件通过本机 Named Pipe 连上 worker（worker 会核对对方确实是
  Rebased 进程），拿到图片后设置 `idea.background.editor` / `idea.background.frame` 并重绘。
- 这两个属性只活在 Rebased 进程里：关掉 Rebased 就没了，下次启动由桥接插件重新向
  worker 要。

## 功能

- 由壳 `configure` 下发回环媒体 URL 和显示参数，热更新已连接的 Rebased
- 覆盖、原始尺寸、平铺；九宫格对齐
- 背景不透明度、模糊、遮罩颜色与强度
- 分别控制「编辑器与面板」（Log、提交等大部分界面）和「空编辑区」
- 配置后自动接管新启动的 Rebased；`apply` 可重启已在运行、还没加载桥接的 Rebased
- 一键暂停或恢复原始外观，状态跨 worker 重启保留；`shutdown` 保留当前背景

支持的图片格式：PNG、JPEG、WebP、GIF（取第一帧）。首版不支持视频。

## 使用须知

- **第一次接管需要重启 Rebased。** IntelliJ 平台只在启动时加载插件。点「立即接管」时
  worker 会发送普通的关闭请求（等同点窗口的关闭按钮，不会强杀），装好桥接后重新打开
  Rebased。如果 Rebased 弹出「Confirm Exit」，点 Exit 即可；勾选 *Don't ask again*
  后以后就不会再问。之后再启动 Rebased 都会自动带上背景，不需要再重启。
- **Rebased 里用 Set Background Image 设过的背景优先级更高。** 这种情况下插件会报错并
  指出冲突位置；在 Rebased 里按 `Ctrl+Shift+A` 搜索 Set Background Image 清除即可。
- **「空编辑区」只在没有打开文件的编辑区显示**，在 Log 视图里看不到，这是 IntelliJ
  本身的行为。
- **在 Background Studio 里卸载本插件后，桥接插件会留在 Rebased 里。** 没有 worker 时它
  什么都不做；要彻底移除，在 Rebased 的 Settings → Plugins 里卸载
  Background Studio Bridge，或关闭 Rebased 后删除上面的插件目录。

worker 自己的数据都在 `%LOCALAPPDATA%\RebasedBackgroundStudio\`：`state.json`
（暂停 / 恢复状态）、`media\`（处理后的图片缓存）、`worker.log`（诊断日志）。
桥接插件的日志写在 Rebased 的 `idea.log` 里。

## 开发

要求 Rust stable、Visual Studio Build Tools 的「使用 C++ 的桌面开发」工作负载，
以及 JDK 21+（用来编译桥接插件，可以直接用 Rebased 自带的 `jbr`）。

```powershell
$env:JAVA_HOME = 'D:\Rebased\jbr'
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

桥接插件源码在 [bridge/](./bridge/)。`build.rs` 用 `javac --release 21` 编译它，默认
对照 [bridge/stubs/](./bridge/stubs/) 里的最小 IntelliJ API 声明；设置
`REBASED_HOME` 指向 Rebased 安装目录时改为对照真实的 `lib\*.jar` 编译，用来确认
stubs 和目标平台一致。编好的 jar 通过 `include_bytes!` 嵌进 worker。

`cargo build --release` 产出 `src-tauri/target/release/rebased-background-studio.exe`；
发布 zip 会把它命名为 `Rebased Background Studio.exe`。壳启动时会带 `--plugin`，worker
会忽略多余参数，直接进入 Named Pipe 服务。

## 发布

推送与 `src-tauri/Cargo.toml` 版本一致的 `v*` 标签会触发 GitHub Actions：
测试、`cargo build --release`，然后上传
`RebasedBackgroundStudio-<version>-plugin.zip`（exe + `plugin.json` + 图标）。
不生成 NSIS。
