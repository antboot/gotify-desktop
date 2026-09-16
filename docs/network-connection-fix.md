# macOS 本地网络连接失败 (os error 65: No route to host) 修复文档

## 1. 问题现象

在 Gotify 客户端的「设置」界面中，输入局域网 Gotify 服务器地址（例如 `http://192.168.31.88:7777/`）和有效的客户端 Token 点击「测试连接」时，弹出错误提示：

```text
❌ 连接失败: IO error: No route to host (os error 65)。请检查服务器地址和 Token 是否正确
```

即使在 macOS 的「系统设置」->「隐私与安全性」->「本地网络」中已将应用权限打开，该错误依然存在。

---

## 2. 核心根本原因分析

从 macOS 26 升级到 macOS 27 后，底层出现此问题主要由于两大核心原因：

### 原因一：代码签名与 Info.plist 未绑定（系统隐私权限无法生效）

1. **缺少关键权限声明**：macOS 针对局域网访问要求应用在 `Info.plist` 中声明 `NSLocalNetworkUsageDescription`。
2. **代码签名与 Bundle ID 未绑定**：
   - 默认通过 `cargo tauri build` 生成的二进制采用 Ad-hoc 临时签名，其 Identifier 往往是编译器生成的哈希字符串（如 `gotify_notifier-xxx`），且 `Info.plist` 处于未绑定（`Info.plist=not bound`）、资源未密封（`Sealed Resources=none`）状态。
   - 而系统「本地网络」权限记录绑定的 Bundle Identifier 是 `com.gotify.notifier`。
   - macOS 27 内核网络扩展控制策略（NECP）检测到运行中进程的签名 ID 与系统授权的 Bundle ID 不匹配，出于防冒充隐私保护策略，在内核层拦截向局域网 IP 发起的 Socket 请求。
   - 为了防止恶意探测，系统将此类拦截统一以物理层不可达错误 `EHOSTUNREACH`（即 `os error 65: No route to host`）返回。

### 原因二：双网卡同网段路由冲突

当本机同时启用了两个处于同一网段的网卡（如内置 Ethernet `en0` 和外接 USB 网卡 `en9`）：
- macOS 27 收紧了静态网络服务顺序（Network Service Order），发往同一网段的流量会被固定绑定到硬件优先级更高的网卡上。
- 若该优先级更高的网卡当前未连通路由器或目标服务器，系统不会自动借道另一张网卡发包，直接报错 `No route to host`。

---

## 3. 完整修复方案

### 修复步骤 1：补全 Info.plist 权限声明

在 `src-tauri/Info.plist` 中加入本地网络权限声明：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSLocalNetworkUsageDescription</key>
    <string>Gotify Notifier 需要访问本地局域网以连接 Gotify 服务器获取推送通知。</string>
</dict>
</plist>
```

### 修复步骤 2：构建后自动重签名并密封 App Bundle

在 `build.sh` 中加入针对 macOS App Bundle 的签名命令，指定 `--identifier "com.gotify.notifier"` 并深度密封：

```bash
APP_PATH="src-tauri/target/release/bundle/macos/Gotify Notifier.app"
if [ -d "$APP_PATH" ]; then
    echo "🔏 正在对应用进行签名 (绑定 identifier com.gotify.notifier)..."
    codesign --force --deep --sign - --identifier "com.gotify.notifier" "$APP_PATH"
    echo "✅ 签名完成"
fi
```

验证签名状态：
```bash
codesign -dvvv "/Applications/Gotify Notifier.app"
```
确保输出中包含：
- `Identifier=com.gotify.notifier`
- `Info.plist entries=...`（已绑定）
- `Sealed Resources=...`（已密封）

### 修复步骤 3：优化 Rust 网络错误提示

在 `src-tauri/src/lib.rs` 的 `test_connection` 与 WebSocket 自动重连逻辑中，将系统原始错误转换为友好中文排查指引：
- `os error 65` / `No route to host`: 提示网络不可达、检查服务器状态或多网卡冲突
- `os error 61` / `Connection refused`: 提示连接被拒、检查服务是否运行及端口
- `os error 60` / `Operation timed out`: 提示连接超时
- `os error 50` / `Network is down`: 提示本机网络已断开
- `401` / `Unauthorized`: 提示客户端 Token 错误

### 修复步骤 4：排查并关闭同网段未使用的网卡

如果遇到多网卡冲突：
1. 打开「系统设置」->「网络」
2. 将未连通路由器的备用以太网卡设置为「停用」，确保所有局域网流量只经由正常连接路由器的物理网卡传输。

---

## 4. 日常编译与安装

修改代码后，直接运行：
```bash
./build.sh
```
构建脚本会自动完成编译和规范签名。若需要覆盖系统应用程序，只需执行：
```bash
rm -rf "/Applications/Gotify Notifier.app"
cp -R "src-tauri/target/release/bundle/macos/Gotify Notifier.app" "/Applications/"
codesign --force --deep --sign - --identifier "com.gotify.notifier" "/Applications/Gotify Notifier.app"
```
