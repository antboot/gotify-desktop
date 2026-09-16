# Gotify<img width="378" height="873" alt="截屏2026-01-17 02 49 51" src="https://github.com/user-attachments/assets/95df44ec-30d6-4cbc-a327-99cff3ff425b" />
 Notifier

一个使用 Rust + Tauri 构建的 Gotify 通知客户端，支持 Windows、macOS 和 Linux。

## 🚀 最新版本 v0.1.1

### ✨ 新功能
- 🔒 **安全修复**: 移除了所有硬编码配置信息
- 🌍 **环境变量支持**: 支持通过环境变量配置服务器信息
- 🎨 **自动主题切换**: 根据系统深浅色模式自动切换界面主题
- 📱 **多平台支持**: Windows/macOS/Linux 完整支持
- ⚙️ **灵活配置**: 支持图形界面、配置文件、环境变量三种配置方式

## 功能特性

- ✅ 实时接收 Gotify 服务器推送的消息
- ✅ macOS 系统原生通知![Uploading 截屏2026-01-17 02.49.51.png…]()

- ✅ 系统托盘图标，后台运行
- ✅ 优雅的用户界面
- ✅ 消息历史记录
- ✅ 消息优先级显示
- ✅ WebSocket 自动重连
- ✅ 自动主题切换

## 安装要求

在构建应用之前，确保你的系统已安装：

1. **Rust** (最新稳定版)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Node.js** (可选，如果需要使用前端构建工具)

3. **Xcode Command Line Tools** (macOS)
   ```bash
   xcode-select --install
   ```

## 构建步骤

### 开发模式

```bash
# 安装 Tauri CLI
cargo install tauri-cli

# 运行开发模式
cargo tauri dev
```

### 生产构建

#### 本地构建（当前平台）

```bash
# 构建当前平台的应用
cargo tauri build

# macOS: target/release/bundle/macos/Gotify Notifier.app
# Windows: target/release/bundle/msi/*.msi
# Linux: target/release/bundle/deb/*.deb
```

#### 使用 GitHub Actions 多平台构建（推荐）

项目已配置 GitHub Actions，可自动构建所有平台的安装包：

**方法 1：手动触发构建**
1. 将代码推送到 GitHub
2. 进入仓库的 Actions 页面
3. 选择 "Build and Release" 工作流
4. 点击 "Run workflow" 手动触发
5. 构建完成后在 Artifacts 中下载安装包

**方法 2：自动发布（推荐）**
```bash
# 使用提供的部署脚本
./deploy.sh

# 或手动执行
git tag v0.1.0
git push origin v0.1.0
```

构建将自动开始，完成后会创建 GitHub Release。

详细说明请查看 [GitHub Actions 使用指南](./GITHUB_ACTIONS_GUIDE.md)

## 配置说明

### 安全提醒 ⚠️

**重要安全信息**：
- 应用不会硬编码任何服务器地址或Token
- 所有配置信息存储在本地配置文件中
- 请勿在公共仓库中提交包含敏感信息的配置文件
- 建议使用环境变量配置敏感信息

### 配置方法

#### 方法 1：图形界面配置（推荐）

1. 启动 Gotify Notifier
2. 点击右键托盘图标 → "设置"
3. 填写服务器地址和Client Token
4. 点击 "保存设置"

#### 方法 2：配置文件

在用户目录创建 `~/.gotify_config.json`：

```json
{
  "server_url": "https://your-gotify-server.com",
  "client_token": "your-client-token"
}
```

#### 方法 3：环境变量

设置环境变量（推荐用于CI/CD）：

```bash
# Linux/macOS
export GOTIFY_SERVER_URL="https://your-gotify-server.com"
export GOTIFY_CLIENT_TOKEN="your-client-token"

# Windows
set GOTIFY_SERVER_URL=https://your-gotify-server.com
set GOTIFY_CLIENT_TOKEN=your-client-token
```

支持的环境变量：
- `GOTIFY_SERVER_URL` 或 `GOTIFY_URL`
- `GOTIFY_CLIENT_TOKEN` 或 `GOTIFY_TOKEN`

### 配置优先级

1. **环境变量**（最高优先级）
2. **配置文件** `~/.gotify_config.json`
3. **图形界面设置**（用户交互保存）

如果所有配置源都无效，应用会提示用户进行配置。

## 项目结构

```
gotify/
├── src/
│   └── main.rs           # Rust 后端代码
├── ui/
│   ├── index.html        # 前端界面
│   ├── styles.css        # 样式文件
│   └── app.js            # 前端逻辑
├── icons/                # 应用图标
├── Cargo.toml            # Rust 依赖配置
├── tauri.conf.json       # Tauri 配置
└── build.rs              # 构建脚本
```

## 技术栈

- **后端**: Rust
- **框架**: Tauri 1.5
- **WebSocket**: tokio-tungstenite
- **前端**: 原生 HTML/CSS/JavaScript
- **异步运行时**: Tokio

## 核心功能说明

### WebSocket 连接

应用使用 WebSocket 连接到 Gotify 服务器的 `/stream` 端点：

```
wss://your-server.com/stream?token=YOUR_CLIENT_TOKEN
```

### 系统通知

收到消息后会自动触发 macOS 系统通知，显示：
- 消息标题
- 消息内容

### 消息优先级

- 🔴 高优先级 (8-10)：红色标记
- 🟡 中优先级 (5-7)：黄色标记
- 🟢 低优先级 (0-4)：绿色标记

## 常见问题

### Q: 连接显示 403 错误？

**A**: 你可能使用了错误的 Token 类型。请确保：
- 使用 **Client Token**（从 Clients 页面创建）
- 不要使用 Application Token（从 Applications 页面创建）

### Q: 收不到通知？

**A**: 请检查：
1. macOS 系统设置中是否允许应用发送通知
2. WebSocket 连接状态是否为 "已连接"
3. Gotify 服务器是否正常运行

### Q: macOS 提示 "No route to host (os error 65)" 或局域网连接失败？

**A**: 
1. **本地网络权限与签名未绑定**：macOS 15+（macOS 27）对局域网权限进行了严格的 Bundle ID 和代码签名比对。请使用 `./build.sh` 进行标准重签名构建，或参考详细文档。
2. **多网卡同网段冲突**：若本机同时启用了内置 Ethernet 和外接 USB 网卡且处于同一网段，请在「系统设置」->「网络」中停用未联网的冗余网卡。
3. 详细排查与修复说明参见：[macOS 本地网络连接修复文档](./docs/network-connection-fix.md)。

### Q: 如何让应用开机自启动？

**A**: 
1. 打开 "系统偏好设置" > "用户与群组"
2. 选择 "登录项"
3. 点击 "+" 添加 Gotify Notifier.app

## 开发说明

### 添加新功能

1. Rust 后端：修改 `src/main.rs`
2. 前端界面：修改 `ui/` 目录下的文件
3. 配置更改：修改 `tauri.conf.json`

### 调试

```bash
# 查看控制台输出
cargo tauri dev

# Rust 日志会在终端显示
# 前端日志在开发者工具中查看 (Cmd+Opt+I)
```

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！

## 联系方式

如有问题或建议，欢迎创建 Issue。
