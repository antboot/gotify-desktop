# Gotify Desktop Notifier 🔔

<p align="center">
  <img src="icons/128x128@2x.png" width="100" height="100" alt="Gotify Notifier Icon" />
</p>

<p align="center">
  <b>精雕细琢的下一代 Gotify 桌面通知客户端 · 深度融合 Apple 原生液态玻璃桌面小组件</b>
</p>

<p align="center">
  <a href="https://github.com/antboot/gotify-desktop/releases"><img src="https://img.shields.io/github/v/release/antboot/gotify-desktop?style=flat-square&color=blue" alt="Latest Release" /></a>
  <a href="https://tauri.app"><img src="https://img.shields.io/badge/Tauri-v2-24C8DB?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2021-orange?style=flat-square&logo=rust&logoColor=white" alt="Rust" /></a>
  <a href="https://github.com/antboot/gotify-desktop/blob/main/LICENSE"><img src="https://img.shields.io/github/license/antboot/gotify-desktop?style=flat-square&color=green" alt="License" /></a>
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square" alt="Cross Platform" />
</p>

---

## 📸 界面预览 (Screenshots)

### 🖥️ 苹果液态玻璃桌面小组件（原生沉底 · 完美兼容台前调度）
> 自由钉在桌面最底层，支持无级调节玻璃透明度；不遮挡其他软件，与 macOS 台前调度（Stage Manager）无缝协作。

<p align="center">
  <img src="assets/screenshots/widget-empty.png" width="48%" alt="小组件常驻桌面·待机状态" />
  &nbsp;
  <img src="assets/screenshots/widget-active.png" width="48%" alt="小组件通知展开·富文本与图文支持" />
</p>

---

## ✨ 核心特性 (Features)

- 🪟 **专注纯粹的桌面小组件架构 (Pure Desktop Widget)**
  - **原生沉底**：基于 macOS Cocoa 底层窗口优化（Level `-1`），小组件始终安静沉在壁纸上，绝不遮挡 **台前调度（Stage Manager）** 的缩略图与应用窗口。
  - **Apple 原生液态玻璃（macOS 26+）**：直接嵌入系统 `NSGlassEffectView`，还原真实水滴透镜折射、边缘高光与 Squircle 连续大圆角；内置 4 套原生光学预设，并可无级调节**折射弧度、模糊、反光亮度、反光厚度**。
  - **零焦点闪烁**：小组件采用非激活窗口（不抢 App 焦点、不成为 Key Window），点击、切换窗口时反光不再闪跳；主组件与应用盒完全独立、互不干扰。
  - **即移即沉**：鼠标离开组件立即沉回桌面底层，玻璃高光状态保持约 3 秒后平滑褪去，不遮挡任何正常工作窗口。
  - **随心摆放**：支持自由拖拽小组件到桌面任意角落，自动防抖记忆窗口坐标与状态。

- 💧 **置顶前台与常驻桌面双形态 · 智能双透明度系统**
  - **悬浮置顶 (前台)**：自动切换至 **`0.92` (92%)** 高对比度实感透明度，文字与通知内容清晰锐利，Win32 / Cocoa 级别绝对置顶，绝不被任何其他应用遮挡。
  - **常驻桌面 (底层)**：自动切换至 **`0.45` (45%)** 高通透度环境玻璃，自然融入桌面壁纸。
  - **丝滑呼吸过渡**：切换形态时背景透明度平滑过渡，并在调节透明度时独立持久化保存两套参数。

- 🗂️ **桌面应用盒 (App Shelf)**
  - 把常用 App / 文件夹拖进应用盒即可固定，单击启动、右键在访达中显示；
  - 图标首次读取后落盘为 PNG，配置文件不再塞入 base64；
  - 关闭应用盒即销毁其 WebView 进程，下次打开按需重建，闲置时不占内存。

- 🍎 **纯后台常驻守护 · 默认不占 Dock 栏与任务栏**
  - **原生后台体验**：默认作为后台辅助服务（`Accessory` 模式）运行，仅在 macOS 顶部菜单栏与 Windows 系统托盘常驻，还你清爽干净的 Dock 栏。
  - **托盘快捷开关**：托盘右键菜单提供 `🍎 显示/隐藏 Dock 图标` 与 `🪟 显示/隐藏任务栏图标` 快捷开关，并与设置窗口实时双向毫秒级同步。

- 👁️ **全局无焦鼠标滑过已阅 (Global Unfocused Hover)**
  - 针对后台挂件窗口无系统焦点的痛点，原生捕获光标轨迹，鼠标掠过通知卡片自动激发流光动效并在移出后平滑标记已阅，**无需点击激活窗口即可划过即读**。

- 🔔 **强大的 Gotify 实时通知**
  - **低延迟长连接**：基于 WebSocket 的实时消息通道，内置智能断线重连机制。
  - **系统原生通知**：支持各平台原生系统级横幅与声音提醒。
  - **富媒体与 Markdown 解析**：完美支持 Markdown 排版、图片快照直显、点击链接跳转。

- ⚡ **极致轻量高效**
  - 基于 **Rust + Tauri 2** 构建，安装包仅数兆大小、内存占用远低于 Electron。

## 🔐 安全与性能

- **前端依赖本地化**：`marked`、`DOMPurify` 随包内置，不依赖 CDN，离线可用。
- **Markdown 净化**：消息渲染前统一经 DOMPurify 处理；外链仅允许 `http(s)` / `mailto`，并在 Rust 侧二次校验协议。
- **收紧 IPC 面**：每个窗口配置 CSP；移除 `shell` 插件及 `shell:allow-open` 权限，切断“消息 → 任意命令”链路。
- **应用盒路径校验**：打开/在访达中显示仅接受已收录的应用路径，避免任意路径调用。
- **列表与渲染优化**：消息列表按 id 增量复用 DOM，Markdown 结果缓存；应用盒关闭即销毁 WebView 释放内存。
- **输入合并**：玻璃滑杆的原生 IPC 合并到约 15 次/秒，CSS 侧仍逐帧跟手。
- **快速重连**：睡眠唤醒、网络恢复或窗口重新聚焦时立即打断退避等待并重连。
- **自适应轮询**：光标轮询按悬停/可见/隐藏分别降频，锁屏休眠时暂停，遮挡检测带缓存。

---

## 🚀 下载与安装 (Installation)

### 系统要求
- **macOS 11+（Apple Silicon / Intel）**：macOS 26 / 27 会自动启用原生液态玻璃 `NSGlassEffectView`，旧系统回退到 CSS 玻璃；当前发布包为 Apple Silicon 版本。
- **Windows 10/11 (x64)**：使用下方预编译安装包。
- 源码构建需要 Rust >= 1.77.2 与 Node.js 18+。

### 方式一：直接下载安装包（推荐）
前往 [Releases 页面](https://github.com/antboot/gotify-desktop/releases/latest) 下载适用于你系统的预编译安装包：
- **macOS (Apple Silicon)**：下载 `.dmg` 文件，拖入 `Applications` 即可使用（已使用开发者证书签名，可正常通过本地网络权限）。
- **Windows**：下载 `Gotify_Notifier_*_x64-setup.exe`（安装引导版）、`.msi`（企业安装版）或 `.zip`（便携免安装绿色版）。

> 📌 **版本策略**：全平台共用一个版本号（当前 `0.5.0`）。如果只有 macOS 端需要更新，只替换现有 Release 中的 macOS 产物，不提升版本号，避免与 Windows 端版本错位。具体命令见 [发布指南](GITHUB_ACTIONS_GUIDE.md)。

### 方式二：源码构建

确保本地已安装 [Rust](https://rustup.rs/) (>= 1.77.2)：

```bash
# 1. 克隆仓库
git clone https://github.com/antboot/gotify-desktop.git
cd gotify-desktop

# 2. 运行开发模式
npx @tauri-apps/cli dev

# 3. 构建正式发行版（推荐使用脚本：自动签名、密封资源并安装）
./build.sh          # Release 构建 + 签名 + 打包 DMG + 安装到 /Applications
./build.sh --run    # 构建完成后立即启动
```

> ⚠️ **macOS 局域网连接必读**：macOS 26/27 的本地网络隐私（NECP）要求 App 声明 `NSLocalNetworkUsageDescription`，且签名必须绑定 `com.gotify.notifier` 并密封 Info.plist，否则会报 `os error 65: No route to host`。请务必使用 `./build.sh` 构建，并参考 [network-connection-fix.md](network-connection-fix.md)。

构建生成的安装包位于 `src-tauri/target/release/bundle/` 目录下。

---

## 🛠️ 配置说明 (Configuration)

首次打开应用后，右键点击小组件（或系统托盘图标）选择“设置”：

1. **服务器地址**：例如 `http://192.168.1.100:8080` 或 `https://gotify.example.com`
2. **客户端 Token**：在 Gotify Web 端创建 Client 并获取生成的 Token
3. **点击连接**：系统会自动测试通信并建立 WebSocket 监听通道。

也可以通过系统环境变量预先注入：
```bash
export GOTIFY_SERVER_URL="https://gotify.example.com"
export GOTIFY_CLIENT_TOKEN="your_token_here"
```

---

## 📚 延伸技术指南与 Skills
- [桌面小组件架构设计与 AI 提示词指南](DESKTOP_WIDGET_IMPLEMENTATION_GUIDE.md)
- [macOS 桌面小组件工程 Skills](.agents/skills/macos-transparent-desktop-widget/SKILL.md)
- [macOS 本地网络连接失败修复文档（os error 65）](network-connection-fix.md)
- [GitHub Actions 多平台自动编译指南](GITHUB_ACTIONS_GUIDE.md)

---

## 🤝 鸣谢与生态

- 核心协议：[Gotify](https://gotify.net/)
- 桌面框架：[Tauri 2](https://tauri.app/)
- 语言生态：[Rust](https://www.rust-lang.org/)

---

## 📄 开源许可证

本项目基于 [MIT License](LICENSE) 开源协议。
