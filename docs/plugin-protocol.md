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
| `hello` | 返回 `pluginProtocol:2`、`pluginId`、`version`、`capabilities` |
| `configure` | 校验并下载回环媒体，预处理成 PNG，保存最近一次有效配置；未暂停时热更新已连接的 Rebased |
| `status` | `phase` / `message` / `activeTargets` / `paused` / `configured` / `revision` |
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
| `editorEnabled` / `frameEnabled` | 布尔 | `idea.background.editor` / `idea.background.frame` |

`hello.capabilities.maxMediaBytes` 与 Manifest 都声明 64 MiB。媒体 URL 仅允许
`http://127.0.0.1` 或 `http://localhost`，拒绝 userinfo、fragment、非回环、端口 0 和
超长 JSON。下载使用 `no_proxy`、不跟随重定向，并校验 `Content-Type`、
`Content-Length`、`byteSize`、`sha256`、`mimeType`/`kind`。只接受
PNG、JPEG、WebP、GIF；`kind:"video"` 直接报错。

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
- `pause` / `restore` 之后，`configure` 只更新配置、不显示；手动 `apply` 重新武装。
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
- 每行一个 JSON，单行最长 16 KiB。

桥接连上后先发：

```json
{"type":"hello","bridgeProtocol":1,"bridgeVersion":"0.1.0","pid":1234,"ideBuild":"IC-262.10968.SNAPSHOT"}
```

worker 随后下发目标，目标变化时立即推送，空闲时每 10 秒重发一次当前目标作心跳：

```json
{"type":"desired","seq":3,"mode":"show","image":"C:\\...\\media\\<sha256>.png","opacity":20,"fill":"SCALE","anchor":"CENTER","editor":true,"frame":true}
```

`mode` 为 `hold`（保持现状）、`clear`（清除插件设置的背景）或 `show`。`image` 必须是
`%LOCALAPPDATA%\RebasedBackgroundStudio\media\` 下的 `<sha256>.png`。

桥接对每条 `desired` 按顺序回报，15 秒内没回报 worker 就断开重来：

```json
{"type":"report","seq":3,"ok":true,"message":"","conflicts":""}
```
