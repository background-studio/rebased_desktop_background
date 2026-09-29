# Background Studio 插件协议

本插件是纯 Rust 无界面 worker，只由 Background Studio 壳启动；Rebased 这一侧由
worker 自动安装的桥接插件负责显示。

- `pluginProtocol`: `2`
- `pluginId`: `rebased`
- 可执行文件：`Rebased Background Studio.exe`
- Pipe：`\\.\pipe\background-studio-rebased`
- 清单：根级 `plugin.json`
- Release 产物：`RebasedBackgroundStudio-<version>-plugin.zip`

## 传输

Named Pipe，每行一个 JSON。

请求：

```json
{"id":"...","cmd":"...","params":{}}
```

成功：`{"id":"...","ok":true,"result":{...}}`

失败：`{"id":"...","ok":false,"error":"..."}`

## 命令

| 命令 | 作用 |
|------|------|
| `hello` | 返回 `pluginProtocol:2`、`pluginId`、`version`、`capabilities`（含 `perWindowMedia:true`） |
| `configure` | 校验并下载回环媒体，预处理成 PNG，保存最近一次有效配置；未暂停时热更新已连接的 Rebased；可选 `independentWindows` / `targetId` |
| `status` | `phase` / `message` / `activeTargets` / `paused` / `configured` / `revision`；独立模式另有 `independentWindows` / `targetIds` / `configuredTargetIds` |
| `apply` | 使用最近一次有效 configure 重新武装；Rebased 在跑但还没加载桥接时会重启它 |
| `pause` | 清除背景，停止自动接管，状态写入磁盘 |
| `restore` | 恢复 Rebased 原始外观，停止自动接管，状态写入磁盘 |
| `shutdown` | 结束 worker，不改当前 Rebased；返回 `shutdown:true`、`keptTarget:true` |

`configure.params`：

```json
{
  "schemaVersion": 1,
  "revision": "...",
  "media": {
    "url": "http://127.0.0.1:<port>/...",
    "kind": "image",
    "mimeType": "image/png",
    "sha256": "<64 hex>",
    "byteSize": 123
  },
  "display": {
    "fit": "cover",
    "anchor": "center",
    "opacity": 0.2,
    "blur": 0,
    "overlayColor": "#101416",
    "overlayOpacity": 0,
    "textContrast": 0.5,
    "editorEnabled": true,
    "frameEnabled": true
  }
}
```

`display` 字段都可省略，缺省值见 `plugin.json` 的 `settingsSchema`；出现未知字段直接报错。

| 字段 | 取值 | 落到 Rebased |
|------|------|-------------|
| `fit` | `cover` / `original` / `tile` | `SCALE` / `PLAIN` / `TILE` |
| `anchor` | `top-left` … `bottom-right` 九宫格 | `TOP_LEFT` … `BOTTOM_RIGHT` |
| `opacity` | 0–1 | 取整成 0–100 的不透明度 |
| `blur` / `overlayColor` / `overlayOpacity` | 0–40 / `#RRGGBB` / 0–0.9 | worker 预处理时烘进图片 |
| `textContrast` | 0–1 | 取整成 0–100，桥接在内存里把编辑器文字的 HSB 亮度往远离编辑器底色的方向推（深色底 `B + (1-B)·k`，浅色底 `B·(1-k)`）；只在 `editorEnabled` 时生效，0 表示不动 |
| `editorEnabled` / `frameEnabled` | 布尔 | `idea.background.editor` / `idea.background.frame` |

`hello.capabilities.maxMediaBytes` 与 Manifest 都声明 64 MiB；`perWindowMedia` 为 `true`。
媒体 URL 仅允许 `http://127.0.0.1` 或 `http://localhost`，拒绝 userinfo、fragment、非回环、端口 0 和
超长 JSON。下载使用 `no_proxy`、不跟随重定向，并校验 `Content-Type`、
`Content-Length`、`byteSize`、`sha256`、`mimeType`/`kind`。只接受
PNG、JPEG、WebP、GIF；`kind:"video"` 直接报错。

#### 可选扩展：按窗口独立媒体

仅在运行中 worker 的 `hello.capabilities.perWindowMedia === true` 时协商启用。旧宿主不传
`independentWindows` 时保持原来的单媒体 `show` 行为。

- 基础 `configure` 可带 `independentWindows: true`，启动窗口发现；保留 `media`/`display`，但不把基础媒体覆盖到已分配窗口。
- `status` 增加 `independentWindows`、`targetIds: string[]`、`configuredTargetIds: string[]`。
  `targetIds` 来自桥接对打开项目窗口的探测；ID 为 1–64 个 ASCII 字母、数字、`-` 或 `_`
  （桥接侧为 `pid + "-" + 无横线 UUID`，项目存续期内固定）。最多 128 个窗口。
- 单窗口 `configure` 额外带 `independentWindows: true` 与 `targetId`。未先建立独立模式时拒绝定向配置。
- 多窗口配置缓存在内存中；窗口关闭后从 `configuredTargetIds` 释放；断连再连期间保留缓存，等桥接重新 report 后再按存活窗口修剪。
- `pause` / `restore` 后 `status.paused` 均为 `true`（宿主只看该字段，避免继续轮询下发 configure）；`apply` 重新武装。
- 独立模式下 worker 向桥接下发全量 `windows` 快照；本进程现存但快照缺项的目标由桥接清除；未知 ID 忽略；缓存 PNG 按当前快照引用保留。

## 自动接管

worker 不会主动打开 Rebased。

- 配置后（且未暂停），worker 每秒检查一次 Rebased 安装，缺桥接插件就装上；
  已装的是旧版本时，等 Rebased 退出后再替换。
- 装好桥接后用户再普通启动 Rebased：桥接插件启动时连上 worker，自动显示当前背景，
  不需要重启。
- 桥接装好之前就在跑的 Rebased：不会自动关闭；`apply` 才会发送普通关闭请求（不强杀），
  等它退出、装好桥接后重新打开。Rebased 弹出「Confirm Exit」时 `status.message` 会提示
  用户点 Exit。
- 只有能确认 Rebased 启动早于桥接 jar 时才会重启，所以轮播触发的 `apply` 不会让
  Rebased 反复重启。
- `pause` / `restore` 之后，`configure` 只更新配置、不显示；`status.paused` 在这两种模式下都为 `true`，
  避免宿主只看 `paused` 时继续轮询下发 configure；手动 `apply` 重新武装。
  这两个状态保存在 `%LOCALAPPDATA%\RebasedBackgroundStudio\state.json`，跨 worker 重启保留。
- `shutdown` 或壳停用插件只结束 worker，Rebased 保留当前背景；新 worker 起来后在收到
  `configure` 之前只让桥接保持现状。

`status.phase` 为 `error` 的常见原因：Rebased 里用 Set Background Image 设了自己的背景
（优先级更高，`message` 会列出冲突位置）、桥接插件在 Rebased 里被禁用、Rebased 启动时
桥接已经装好却迟迟没有连上（去看 `idea.log`）。

## 桥接协议（worker ↔ Rebased 桥接插件）

仅供 worker 和随它发布的桥接插件之间使用，壳不需要关心。

- Pipe：`\\.\pipe\background-studio-rebased-bridge`，worker 是服务端，拒绝远程客户端。
- worker 用 `GetNamedPipeClientProcessId` 找到客户端进程，要求它是已安装 Rebased 的
  启动器，并且和 `hello.pid` 一致；校验不过、或 5 秒内没发合法的 hello，都会先回
  `{"type":"reject","message":"..."}` 再断开；
  桥接收到 reject 后 30 秒再试，其他断线 3 秒后重连。
- 每行一个 JSON，单行最长 256 KiB（窗口快照需要更大额度）。

桥接连上后先发：

```json
{"type":"hello","bridgeProtocol":2,"bridgeVersion":"<worker-version>","pid":1234,"ideBuild":"IC-262.10968.SNAPSHOT"}
```

`bridgeProtocol` 必须为 `2`。旧桥接（协议 1）会被明确拒绝并提示重启 Rebased 加载新版桥接，不会静默降级。

窗口 ID 必须带所属 Rebased 的 `<pid>-` 前缀，worker 会核对进程归属。短暂断线时保留
最近的窗口发现结果 30 秒，避免另一个进程仍在上报时误删断线窗口的配置；重新连接并
上报后，以新结果为准。超出宽限期仍未重连则释放该进程的窗口配置。

worker 随后下发目标，目标变化时立即推送，空闲时每 2 秒重发一次当前目标作心跳（以便 report 发现新窗口）：

```json
{"type":"desired","seq":3,"mode":"show","image":"C:\\...\\media\\<sha256>.png","opacity":20,"fill":"SCALE","anchor":"CENTER","textContrast":50,"editor":true,"frame":true}
```

独立窗口模式改为：

```json
{"type":"desired","seq":4,"mode":"windows","windows":{"<targetId>":{"image":"C:\\...\\media\\<sha256>.png","opacity":20,"fill":"SCALE","anchor":"CENTER","textContrast":50,"editor":true,"frame":true}}}
```

`mode` 为 `hold`（保持现状，仍扫描窗口）、`clear`（清除插件设置的背景和文字增强）、`show`（旧宿主单媒体）或 `windows`（全量快照）。
`windows` 只包含应显示的窗口；本进程现存但快照缺项的目标会被清除；未知 ID 忽略。`image` 必须是
`%LOCALAPPDATA%\RebasedBackgroundStudio\media\` 下的 `<sha256>.png`。

桥接对每条 `desired` 按顺序回报，15 秒内没回报 worker 就断开重来；每次 report 都必须带当前 `targetIds`（即便 seq 未变也要刷新发现结果）：

```json
{"type":"report","seq":3,"ok":true,"message":"","conflicts":"","targetIds":["1234-abcdef0123456789abcdef01234567"]}
```
