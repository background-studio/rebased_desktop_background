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
只使用运行时绘制器（旧宿主使用 JVM 系统属性），不写 Rebased 的设置，不改 Rebased 的安装目录。

> 非 Rebased 官方产品。Rebased 归其作者所有；IntelliJ 是 JetBrains s.r.o. 的商标。

## 工作方式

- worker 从壳收到回环媒体地址，下载、校验后缩放到长边 3840 以内，按设置做模糊和遮罩，
  存成 `%LOCALAPPDATA%\RebasedBackgroundStudio\media\<sha256>.png`。
- 配置好背景后，worker 把桥接插件 `Background Studio Bridge` 放进 Rebased 的插件目录
  （例如 `%APPDATA%\detachhead\IdeaIC1.1\plugins\background-studio-bridge`）。
- Rebased 启动后，桥接插件通过本机 Named Pipe 连上 worker（worker 会核对对方确实是
  Rebased 进程），上报打开的项目窗口；壳分别选图后，桥接将图片挂到各窗口自己的
  `idea.background.editor` / `idea.background.frame` 原生绘制器上。
- 窗口 ID 和绘制器只活在 Rebased 进程里，关闭窗口会释放；不会将背景写进项目设置。
  旧宿主没有协商独立窗口时仍使用原来的单媒体模式。

## 功能

- 由壳 `configure` 下发回环媒体 URL 和显示参数，热更新已连接的 Rebased
- 多项目窗口独立背景与轮播：通过 `perWindowMedia` 接入壳的逐窗口随机/顺序选图和计时，
  图片充足时优先避开其他窗口的图片；打开新窗口不改变旧窗口的选择。需支持独立窗口的宿主
  和新版桥接同时生效；欢迎页不作为项目窗口。桥接约每 2 秒上报窗口，壳约每 2 秒分配媒体。
- 覆盖、原始尺寸、平铺；九宫格对齐
- 背景不透明度、模糊、遮罩颜色与强度
- 文字对比度增强：背景图会把字底下的底色提亮，显示背景时桥接插件在内存里把编辑器文字
  颜色往远离底色的方向推（深色主题调亮、浅色主题调暗），色相和饱和度不变；只作用于编辑器
  文字，不改 Rebased 的配色方案，暂停或恢复时一起撤掉
- 分别控制「编辑器与面板」（Log、提交等大部分界面）和「空编辑区」
- 兼容 Islands 主题的面板背景限制和表格单元格绘制：Log 主体也显示窗口独立背景，
  不只覆盖外围；暂停/恢复会撤销窗口级绘制变换，并还原主题的原始背景限制。
- 显示背景时去除 Islands 区块边缘额外的深色填充，保留主题原有细分界线和圆角；
  普通面板的实色底色在合成图片前统一为窗口底色，区块内边距先重置底色再绘制一次背景，
  不叠加阴影遮罩；文字、线条、输入控件及非普通底色的选中行不改色。不修改全局主题颜色，
  暂停/恢复原始外观时撤销绘制变换并还原区块绘制器。
- 配置后自动接管新启动的 Rebased；`apply` 可重启已在运行、还没加载桥接的 Rebased
- 一键暂停或恢复原始外观，状态跨 worker 重启保留；`shutdown` 保留当前背景

支持的图片格式：PNG、JPEG、WebP、GIF（取第一帧）。首版不支持视频。

## 使用须知

- **从旧版升级后需要重启 Rebased，才能加载新版桥接。** worker 不会静默关闭现有窗口；
  可手动退出 Rebased 后等待桥接更新再打开，或在壳里使用「立即接管」。仅更新 worker
  不能让已经加载的旧桥接支持多窗口。
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
./bridge/tests/Run-BridgeUnitTests.ps1 -JdkHome $env:JAVA_HOME -WindowTests
./bridge/tests/Run-NativePainterApiTest.ps1 -RebasedHome 'D:\Rebased'
cargo build --release --manifest-path src-tauri/Cargo.toml
```

桥接插件源码在 [bridge/](./bridge/)。`build.rs` 用 `javac --release 21` 编译它，默认
对照 [bridge/stubs/](./bridge/stubs/) 里的最小 IntelliJ API 声明；设置
`REBASED_HOME` 指向 Rebased 安装目录时改为对照真实的 `lib\*.jar` 编译，用来确认
stubs 和目标平台一致。编好的 jar 通过 `include_bytes!` 嵌进 worker。

Java 测试覆盖嵌套协议、消息限额、按项目文字增强和窗口发现/隔离/关闭/恢复。
`-WindowTests` 要求桌面环境（创建但不显示 Swing 窗口）；CI 无桌面时可省略该选项，
其余测试仍执行。`Run-NativePainterApiTest.ps1` 对真实 Rebased jar 检查内部窗口绘制接口，
不启动 IDE、不修改用户设置；内部接口不兼容时明确失败，不退回全局同步背景。

`cargo build --release` 产出 `src-tauri/target/release/rebased-background-studio.exe`；
发布 zip 会把它命名为 `Rebased Background Studio.exe`。壳启动时会带 `--plugin`，worker
会忽略多余参数，直接进入 Named Pipe 服务。

## 发布

推送与 `src-tauri/Cargo.toml` 版本一致的 `v*` 标签会触发 GitHub Actions：
测试、`cargo build --release`，然后上传
`RebasedBackgroundStudio-<version>-plugin.zip`（exe + `plugin.json` + 图标）。
不生成 NSIS。
