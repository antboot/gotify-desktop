# macOS & Windows 透明桌面小组件工程级技术实现指南 (落地手册与 AI Prompt 模板)

> **导语**：本指南记录了在 **Rust + Tauri 2** 架构下，从 0 到 1 实现极具苹果质感、常驻桌面最底层、完美兼容台前调度（Stage Manager）且支持前台置顶/底层壁纸双透明度智能切换的“纯粹桌面小组件”全套工程解决方案。
> 无论是在 macOS、Windows 还是跨平台应用（Tauri、Electron、Swift/Cocoa）中，均可直接依此实现与优化。

---

## 目录
1. [核心架构理念：彻底拥抱纯桌面小组件](#1-核心架构理念彻底拥抱纯桌面小组件)
2. [macOS Cocoa 窗口调度底层 (Rust Objective-C)](#2-macos-cocoa-窗口调度底层-rust-objective-c)
3. [Windows Win32 窗口调度底层 (Rust Win32 API)](#3-windows-win32-窗口调度底层-rust-win32-api)
4. [双形态与双透明度系统 (Dual-Opacity Architecture)](#4-双形态与双透明度系统-dual-opacity-architecture)
5. [后台常驻与 Dock 栏 / 任务栏控制 (Activation Policy)](#5-后台常驻与-dock-栏--任务栏控制-activation-policy)
6. [全局无焦鼠标滑过已阅 (Global Unfocused Hover)](#6-全局无焦鼠标滑过已阅-global-unfocused-hover)
7. [前端 CSS 与苹果液态玻璃拟态美学](#7-前端-css-与苹果液态玻璃拟态美学)
8. [交互与事件捕获防穿透 (Capture Phase Menu Interception)](#8-交互与事件捕获防穿透-capture-phase-menu-interception)
9. [Retina 屏幕防膨胀与逻辑像素规则](#9-retina-屏幕防膨胀与逻辑像素规则)
10. [macOS 本地网络权限与签名避坑 (os error 65)](#10-macos-本地网络权限与签名避坑-os-error-65)
11. [原生液态玻璃 NSGlassEffectView（macOS 26+）](#11-原生液态玻璃-nsglasseffectviewmacos-26)
12. [非激活窗口与焦点态稳定性（反光闪跳根治）](#12-非激活窗口与焦点态稳定性反光闪跳根治)
13. [安全与性能加固（CSP、IPC 与资源回收）](#13-安全与性能加固cspipc-与资源回收)
14. [可直接发给 AI 的标准 Prompt 提示词模板](#14-可直接发给-ai-的标准-prompt-提示词模板)

---

## 1. 核心架构理念：彻底拥抱纯桌面小组件

在早期的桌面客户端中，许多项目同时维护了“常规完整应用窗口”与“桌面小组件模式”两套逻辑。实践证明：
- 维护双套 CSS、双套窗口尺寸记忆、模式切换时的闪烁过渡非常臃肿；
- 作为消息通知、状态监控类工具，用户最需要的是一个**全天候静默融入桌面、随时可读、不挡视线的桌面环境挂件**。
- **架构定型**：彻底剥离传统常规窗口，专注打磨“桌面小组件卡片”，并在此基础上提供两种工作形态：
  1. **常驻桌面 (底层壁纸模式)**：沉于所有窗口与台前调度之下，高通透（45%），融入桌面；
  2. **悬浮置顶 (前台焦点模式)**：浮于所有应用之上，高对比度（92%），清晰易读。

---

## 2. macOS Cocoa 窗口调度底层 (Rust Objective-C)

在 Tauri / Rust 的 `macos_window` 模块中，通过 Cocoa Objective-C 运行时精准控制：

### 2.1 常驻桌面模式 (Pinned to Desktop)
```rust
// 1. 核心层级 Level = -1 (kCGNormalWindowLevel - 1)
// - 普通窗口 & 台前调度卡片处于 Level 0：小组件绝不遮挡台前调度与普通窗口
// - 访达壁纸处于 Level -2147483623：小组件远高于壁纸，鼠标交互 100% 灵敏
let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
msg_isize(ns_window, level_sel, -1);

// 2. 集合行为：声明为全局桌面小组件
// 1: CanJoinAllSpaces | 16: Stationary | 64: IgnoresCycle | 256: FullScreenAuxiliary | 262144: CanJoinAllApplications
let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
msg_usize(ns_window, behavior_sel, 1 | 16 | 64 | 256 | 262144);

// 3. 消除直角黑色阴影与底色
msg_usize(ns_window, sel_registerName(b"setHasShadow:\0".as_ptr() as _), 0); // hasShadow = NO
msg_usize(ns_window, sel_registerName(b"setOpaque:\0".as_ptr() as _), 0);    // isOpaque = NO
// 注入 clearColor 背景
let clear_color = f_clear(objc_getClass(b"NSColor\0".as_ptr() as _), sel_registerName(b"clearColor\0".as_ptr() as _));
f_bg(ns_window, sel_registerName(b"setBackgroundColor:\0".as_ptr() as _), clear_color);

// 4. 隐藏交通灯红黄绿按钮（保持 FullSizeContentView 完整不塌陷）
for btn_type in 0..=2 {
    let btn = f_btn(ns_window, sel_registerName(b"standardWindowButton:\0".as_ptr() as _), btn_type);
    if !btn.is_null() {
        f_hide(btn, sel_registerName(b"setHidden:\0".as_ptr() as _), 1);
    }
}

// 5. 沉底排布
f_order(ns_window, sel_registerName(b"orderBack:\0".as_ptr() as _), std::ptr::null_mut());
```

### 2.2 悬浮置顶模式 (Floating Always-on-Top)
```rust
// 1. 恢复系统阴影
msg_usize(ns_window, sel_registerName(b"setHasShadow:\0".as_ptr() as _), 1);

// 2. 窗口层级设置为 3 (kCGFloatingWindowLevel)
msg_isize(ns_window, level_sel, 3);

// 3. 提升至最前台
f_order(ns_window, sel_registerName(b"orderFront:\0".as_ptr() as _), std::ptr::null_mut());
```

---

## 3. Windows Win32 窗口调度底层 (Rust Win32 API)

在 Windows 系统中，确保无边框透明卡片不被遮挡的核心在于 **Win32 扩展样式与 Z-Order**：
```rust
const HWND_TOPMOST: isize = -1;
const HWND_BOTTOM: isize = 1;
const HWND_NOTOPMOST: isize = -2;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOPMOST: isize = 0x00000008;

pub fn set_desktop_pin(hwnd: isize, pin: bool) {
    unsafe {
        if pin {
            // 常驻桌面底层：剥离 WS_EX_TOPMOST，降级为普通窗口并沉入底层壁纸
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if (ex_style & WS_EX_TOPMOST) != 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !WS_EX_TOPMOST);
            }
            SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        } else {
            // 悬浮置顶：显式赋予 WS_EX_TOPMOST，并调用 HWND_TOPMOST 提升至最高层级
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if (ex_style & WS_EX_TOPMOST) == 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOPMOST);
            }
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }
}
```

---

## 4. 双形态与双透明度系统 (Dual-Opacity Architecture)

用户在使用桌面组件时有两个截然不同的诉求：
1. **工作阅读时**：希望悬浮置顶，但如果透明度太高背景杂乱会看不清文字；
2. **待机闲置时**：希望静卧在壁纸底层，晶莹通透，不抢视觉焦点。

### 架构方案：
- **前台置顶**：默认透明度 `0.92 (92%)`，实感高对比度；
- **常驻桌面**：默认透明度 `0.45 (45%)`，通透环境玻璃；
- **状态流转**：
  - 切换形态时，CSS `transition: background 0.35s ease` 提供丝滑呼吸过渡；
  - 调节透明度滑块时，自动持久化至当前活跃形态对应的 `floating_opacity` 或 `desktop_opacity`。

---

## 5. 后台常驻与 Dock 栏 / 任务栏控制 (Activation Policy)

### 5.1 纯后台服务哲学
- **默认不出现在 Dock 栏 / 任务栏**：
  - macOS 启动时设定 `tauri::ActivationPolicy::Accessory`；
  - Windows 启动时设定 `window.set_skip_taskbar(true)`；
  - 保持像所有专业后台守护服务一样，**仅在系统任务栏托盘或顶部菜单栏常驻**。

### 5.2 快捷开关
在托盘右键菜单和设置界面中提供一键切换：
- 勾选“在 Dock 栏/任务栏中显示”时，动态切换为 `Regular` 或 `skip_taskbar(false)`；
- 托盘菜单文本实时显示 `🍎 显示/隐藏 Dock 图标` 或 `🪟 显示/隐藏任务栏图标`，两处双向毫秒级同步。

---

## 6. 全局无焦鼠标滑过已阅 (Global Unfocused Hover)

### 6.1 痛点
当应用处于底层小组件模式或 Accessory 模式时，窗口没有系统焦点（Inactive Window）。普通的 WebKit CSS `:hover` 和 JavaScript `onmouseenter` 在无焦点窗口上通常不会被触发！

### 6.2 解决方案
- 在后端启动轻量无焦光标跟踪任务（或 Cocoa `NSTrackingArea` 带 `NSTrackingActiveAlways`）；
- 轮询不能固定 50ms 硬跑，否则待机功耗偏高。按状态自适应：光标在组件内 **40ms**、组件可见但光标在外 **100ms**、所有组件隐藏 **500ms**；
- 遮挡检测（`CGWindowListCopyWindowInfo` 前到后扫描）结果缓存几百毫秒，跳过 `alpha≈0` 与 `layer>=20` 的 Dock 全屏透明窗口；
- 仅当取整后的坐标变化时才发送事件，系统休眠或全部隐藏时暂停轮询；
- 当光标位于小组件矩形内时，向 Webview 发送 `native-mouse-hover` 坐标；
- 前端通过 `document.elementFromPoint(x, y)` 命中卡片，触发已阅流光动效并在移出时平滑标记已阅，**用户无需点击激活窗口即可划过即读**！

---

## 7. 前端 CSS 与苹果液态玻璃拟态美学

### 7.1 彻底根除“点击小组件后颜色变来变去”
- **严禁使用 `saturate(...)` 和 `brightness(...)`**：WebKit 在窗口激活和失焦时会对高饱和度毛玻璃进行动态调光，造成色彩抽搐。
- **纯高斯毛玻璃**：`backdrop-filter: blur(32px) !important;`。
- **恒定半透底色**：通过 `--widget-opacity` 变量统一驱动。

### 7.2 纯内发光打造晶莹浮雕（杜绝外投影黑边）
> ⚠️ 不要在带 `backdrop-filter` 的元素上使用 `clip-path`：WebKit 会破坏 `CABackdropLayer` 的 GPU 合成，回退到不稳定的遮罩 FBO，重绘时出现 1 帧黑块/暗块闪烁。用 `border-radius` + `overflow: hidden` 裁剪即可。

```css
.desktop-widget-card {
    border-radius: 16px !important;
    overflow: hidden !important;
    transform: translateZ(0);
    background: rgba(22, 28, 40, var(--widget-opacity, 0.45)) !important;
    backdrop-filter: blur(32px) !important;
    -webkit-backdrop-filter: blur(32px) !important;
    border: 1px solid rgba(255, 255, 255, 0.16) !important;
    transition: background 0.35s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.35s ease, box-shadow 0.35s ease;
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.28),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.25),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.08) !important;
}
```

---

## 8. 交互与事件捕获防穿透 (Capture Phase Menu Interception)

### 8.1 痛点
卡片绝大部分区域需要响应窗口拖拽（`start_dragging_window`）。但在调用原生系统拖拽捕获时，Windows/macOS 进入模态捕获循环，导致浏览器的 `mouseup` 和 `click` 根本不会分发到 `document`，从而导致右键菜单点击空白处无法收起！

### 8.2 解决方案
- 在 `window` 监听**捕获阶段（Capture Phase）**的 `pointerdown`：
```javascript
window.addEventListener('pointerdown', (e) => {
    const menu = document.getElementById('contextMenu');
    if (menu && menu.style.display !== 'none' && !menu.contains(e.target)) {
        menu.style.display = 'none';
    }
}, { capture: true });
```
- 在窗口失焦时（`window.addEventListener('blur')`）自动隐藏右键菜单与透明度弹窗。

---

## 9. Retina 屏幕防膨胀与逻辑像素规则
所有窗口位置与尺寸的计算与存储，必须严格除以 `window.scale_factor()` 转换为**逻辑像素**（Logical Pixels），严禁直接保存物理像素（Physical Pixels），否则在 2x/3x Retina 屏幕每次启动尺寸都会翻倍膨胀！

---

## 10. macOS 本地网络权限与签名避坑 (os error 65)
针对访问局域网 IP（`192.168.x.x`）的组件应用，macOS 26/27 的 NECP 策略要求**权限声明 + 签名绑定 + 资源密封**三者同时满足，否则内核会以 `EHOSTUNREACH (os error 65: No route to host)` 静默拦截：
1. `src-tauri/Info.plist` 注入权限描述与 ATS 本地网络豁免：
   ```xml
   <key>NSLocalNetworkUsageDescription</key>
   <string>连接本地 Gotify 服务器以实时接收推送通知</string>
   <key>NSAppTransportSecurity</key>
   <dict>
       <key>NSAllowsArbitraryLoads</key><true/>
       <key>NSAllowsLocalNetworking</key><true/>
   </dict>
   ```
2. 提供 `src-tauri/Entitlements.plist`：
   ```xml
   <key>com.apple.security.network.client</key><true/>
   ```
3. 打包后执行**绑定 identifier 的深度签名**（优先使用 Apple Development 证书；无证书时使用带 designated requirement 的 ad-hoc 签名）：
   ```bash
   codesign --force --deep --sign "$IDENTITY" \
     --entitlements src-tauri/Entitlements.plist \
     --identifier "com.gotify.notifier" "Gotify Notifier.app"
   ```
4. 验证签名确实绑定（而不是仅有签名）：
   ```bash
   codesign -dvvv "Gotify Notifier.app" | grep -E "Identifier=|Info.plist|Sealed"
   # Identifier=com.gotify.notifier
   # Info.plist entries=16
   # Sealed Resources version=2 rules=13 files=1
   codesign --verify --deep --strict --verbose=2 "Gotify Notifier.app"
   ```
5. 覆盖安装到 `/Applications` 后清理隔离属性并刷新 LaunchServices，始终从 `/Applications` 启动：
   ```bash
   xattr -cr "/Applications/Gotify Notifier.app"
   /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "/Applications/Gotify Notifier.app"
   ```
6. **双网卡同网段冲突**：若 `en0` 与 `en9` 同时处于 `192.168.x.0/24`，macOS 27 会把流量固定到服务顺序更高的网卡；该网卡断开时不会自动改走另一张，直接报 `No route to host`。禁用闲置网卡或在「系统设置 → 网络 → 服务顺序」中调整即可。
7. 完整排查记录见仓库根目录 [network-connection-fix.md](network-connection-fix.md)。

---

## 11. 原生液态玻璃 NSGlassEffectView（macOS 26+）

CSS `backdrop-filter` 无法复刻 Apple 真实玻璃透镜。macOS 26+ 可直接嵌入系统 `NSGlassEffectView`：

```objc
Class GlassCls = NSClassFromString(@"NSGlassEffectView");
NSView *glassView = [[GlassCls alloc] initWithFrame:contentView.bounds];
[glassView setValue:@(20.0) forKey:@"cornerRadius"];
[contentView addSubview:glassView positioned:NSWindowBelow relativeTo:nil]; // 放在 WebView 下面
```

要点：
1. **WebView 必须透明**，玻璃层才能透出桌面与后方窗口。
2. **重建判定**：把 `variant / radius / inset` 存入窗口与玻璃视图的 associated object，参数变化才重建，否则复用现有视图。
3. **私有光学参数**（backdrop 的 `filters.glassBackground.*`）：
   - `inputInnerRefractionAmount` / `inputOuterRefractionAmount`：透镜折射曲率
   - `inputBlurRadius`：模糊（0 = 水晶水滴）
   - `inputKeyFillHighlightAmount` / `inputBlurFillLightenOpacity`：边缘高光与填充
   - `inputLumaNoiseScale`：纹理噪点
   - `CASDFLayer` 镜面层（`@2` / `@3`）：`opacity ≈ 0.75 × 反光系数`
4. **持久化用户调优**：反光、折射、模糊、弧高写入 associated object，防止 Apple 内部重建图层后丢失；同时镜像到 `NSUserDefaults`，并在安装玻璃视图时优先恢复。关联对象只存在于当前进程，重启/重新登录后若不落盘，新建的 `NSGlassEffectView` 会退回 Apple 默认反光，只能依赖前端 IPC 重新下发（时序不可靠）。
5. **焦点/外观变化后的恢复**：Apple 会在窗口获焦/失焦时重写高光。可 swizzle `-[NSGlassEffectView _windowChangedKeyState]`，并在 30/120/400/900ms 异步重刷用户参数；必要时以 16ms 守卫循环持续约 640ms 压住设定值。
   - **真正的关键在图层层，而非时序**：SwiftUI 宿主会在首帧渲染时把 `@2`/`@3` 的 opacity 重置回 `1.0`，而这次渲染可能晚于所有短促重刷（登录繁忙时尤其明显）。非激活面板之后不会再获得焦点事件，于是高光永久停在 Apple 默认满亮度——这就是“重启后偶尔变亮”的根因。做法：给每个 `@2`/`@3` 层用 associated object 打上目标 opacity，并 swizzle `-[CALayer setOpacity:]`，对已打标层用目标值替换任何后续写入，SwiftUI 便再也改不回去。再加一个 100ms、持续约 5 秒的补标循环兜住晚创建的图层，并在遮挡/切屏/切换空间/唤醒/外观变化时重新武装。
6. **首帧 CSS**：若 WebView 卡片用 CSS 变量镜像高光，必须在 `<head>` 里从 `localStorage` 同步注入持久化值，并在脚本解析阶段调用一次应用函数。否则变量回退值（通常是 `1`）会在异步初始化完成前画出一圈默认高光——这是另一个独立的“偶发变亮”来源。
7. `subduedState` 固定为 `0`；如系统仍会变暗/脉冲，可把 `scrimState`、`interactionState` 一并锁定为 `0`。
8. 参数快速切换时递归清理 CALayer 动画（`removeAllAnimations`），避免过渡拖影。
9. NSGlassEffectView 仅 macOS 26+ 可用，需用 `NSClassFromString` 探测并回退到 CSS 玻璃。

---

## 12. 非激活窗口与焦点态稳定性（反光闪跳根治）

### 12.1 典型症状
点击任一小组件时，它的镜面反光会“跳”一下；同时另一个小组件也会跟着闪/联动。

### 12.2 根因
点击会激活 App，AppKit 随后需要指定一个 Key Window。仅在**被点击的窗口**上返回 `canBecomeKeyWindow = NO` 并不能阻止激活流程 —— 系统会绕过它，把同一 App 的**另一个窗口**设为 Key。该窗口收到 `windowDidBecomeKey` 后，`NSGlassEffectView` 会按“焦点态”重绘高光，于是出现闪跳，并导致两个组件互相联动。

### 12.3 正确做法：让小组件窗口永不参与 App 激活
对每个被纳管的小组件窗口（底层与置顶两种形态都要）执行：
```rust
let nonact = sel_registerName(b"_setNonactivatingPanel:\0".as_ptr() as _);
msg_usize(win, nonact, 1);
let prev = sel_registerName(b"_setPreventsActivation:\0".as_ptr() as _);
msg_usize(win, prev, 1);
```
同时：
- `canBecomeKeyWindow` / `canBecomeMainWindow` 保持返回 `NO`；
- 小组件窗口禁止调用 `NSApp.activateIgnoringOtherApps:`、`makeKeyAndOrderFront:` 或 Tauri `set_focus()`。

### 12.4 验证标准
- Rust 侧 `WindowEvent::Focused` 对小组件窗口**永不触发**；
- 点击后 `[window isKeyWindow] == false`；
- 代价：小组件无法获得键盘输入，所有交互需保持鼠标可完成（拖拽、滑杆、右键菜单）；设置窗口仍可作为普通 Key Window。

### 12.5 沉底逻辑要跟着改
窗口不再有焦点事件后，不能再用 `Focused(false)` 触发沉底。应改为：
- 鼠标离开组件时（原生 50ms 光标轮询）**立即**把窗口沉回桌面层级；
- 玻璃“激活高光”状态可继续保持约 3 秒后平滑褪去。

---

## 13. 安全与性能加固（CSP、IPC 与资源回收）

### 13.1 CSP 与内联事件的“假死”陷阱
**症状**：加上 CSP 后，悬停、滚动、拖拽都正常，但所有按钮、菜单项和 `onclick="..."` 全部无响应，像主线程卡死；而 `addEventListener` 注册的事件仍然能触发。

**根因**：`script-src` 一旦包含 nonce 或 hash，浏览器（WKWebView）会忽略 `'unsafe-inline'`。Tauri 2 会在运行时向 CSP 注入 nonce/hash（`replace_csp_nonce`），于是内联属性处理器被拦截。

**处理原则**：
1. 优先改写为 `addEventListener` + 事件委托，内联属性与 nonce-based CSP 不兼容。
2. 存量内联处理器较多时，在 `tauri.conf.json` 设 `"csp": null`，改为每个页面写 `<meta http-equiv="Content-Security-Policy">`，保留 `'unsafe-inline'`，同时继续限制 `object-src 'none'`、`base-uri 'self'`、`connect-src 'self' ipc: http://ipc.localhost`。
3. 验证必须用真实点击（CGEvent 或自动化脚本），不要只看界面是否渲染。
4. 不要把 `connect-src` 放开成 `*`。

### 13.2 渲染与 IPC 性能
- 原生玻璃调参滑杆：CSS 变量逐帧更新，原生 IPC 合并到约 15 次/秒（66ms），参数相同则跳过调用。
- 通知列表按 id 做 keyed DOM 复用，避免整块 `innerHTML` 重写；Markdown 渲染结果按消息缓存。
- 轮询等热路径降噪，不在每个 tick 打印日志。

### 13.3 资源回收与生命周期
- 应用盒（App Shelf）关闭时先持久化位置/尺寸，再 `window.close()` 销毁 WebView，下次打开按需重建；图标以 PNG 文件存放在配置目录（`.gotify_icons`），并一次性迁移历史 base64 数据。
- 睡眠唤醒、网络恢复、窗口重新聚焦时用 `tokio::sync::Notify` 打断指数退避并立即重连；轮询中用 `SystemTime` 跳变检测唤醒。

### 13.4 输入与访问面收敛
- `marked` / `DOMPurify` 本地 vendored（附 LICENSE），不依赖 CDN；Markdown 渲染前统一净化。
- 外链在 Rust 侧二次校验协议（仅 `http` / `https` / `mailto`）并拒绝控制字符。
- 打开应用 / 在访达中显示只接受应用盒中已收录的路径。
- 移除不需要的 `tauri-plugin-shell` 与 `shell:allow-open` 能力。

### 13.5 构建参数
- `bundle.macOS.minimumSystemVersion` 至少设为 `11.0`，否则使用 `@available` 的 Objective-C 代码会因 `___isPlatformVersionAtLeast` 未定义而链接失败。

---

## 14. 可直接发给 AI 的标准 Prompt 提示词模板

```markdown
请帮我将当前应用实现为极具苹果质感的“纯桌面小组件”模式，严格遵循以下全套工程规范：

【1. 架构定位】
- 移除臃肿的传统常规窗口，专注为纯桌面卡片挂件模式；
- 支持两种工作形态：常驻桌面底层壁纸 (Level -1) 与 悬浮置顶前台 (Level 3 / HWND_TOPMOST)；
- 默认作为后台辅助服务运行在系统托盘/状态栏（macOS ActivationPolicyAccessory / Windows skip_taskbar），默认不占用 Dock 栏或任务栏；
- 在托盘菜单与设置页中提供“在 Dock/任务栏中显示”的快捷开关与实时双向同步。

【2. 双透明度美学】
- 悬浮置顶模式：不透明度设为 0.92，高实感对比，确保文字清晰易读；
- 常驻桌面模式：不透明度设为 0.45，高通透液态玻璃，自然融入桌面壁纸；
- CSS 卡片背景使用 0.35s 缓动呼吸过渡；
- 严禁在 backdrop-filter 中使用 saturate() 与 brightness() 避免窗口失焦抽搐；
- 严禁向外投影，必须使用纯 inset 内发光与微边框打造浮雕感。

【3. 系统窗口与事件底层】
- macOS 端：底层模式精准 Level -1，禁用 hasShadow, isOpaque，设置 clearColor 并隐藏交通灯按钮；
- Windows 端：置顶模式显式注入 WS_EX_TOPMOST 并设置 HWND_TOPMOST；
- 拖拽与点击互斥，在 window 捕获阶段拦截 pointerdown，保证点击卡片空白或拖拽时右键菜单能立即收起；
- 支持无焦点全局光标悬停检测 (native-mouse-hover)，光标划过通知卡片自动平滑标记已阅；
- 小组件窗口使用非激活面板（_setNonactivatingPanel / _setPreventsActivation），绝不成为 Key Window，点击时禁止触发系统焦点态玻璃重绘；
- 鼠标离开组件时立即沉回桌面层级，玻璃高光状态延迟约 3 秒后褪去。

【4. 原生液态玻璃（macOS 26+）】
- 嵌入系统 NSGlassEffectView，WebView 保持透明，玻璃层置于其下；
- 暴露折射、模糊、反光亮度、反光厚度等可调参数，并持久化到本地配置；
- 在窗口焦点/外观变化后用异步重刷恢复用户参数，禁止在 AppKit 回调中同步执行 CATransaction flush（会死锁）。

【5. 安全与性能】
- 前端依赖（marked / DOMPurify）本地随包内置，Markdown 渲染前必须净化；
- 外链协议在 Rust 侧白名单校验（仅 http/https/mailto），移除 shell 插件与任意命令打开能力；
- 每个窗口配置 CSP；若使用内联 onclick，必须避免 nonce-based script-src，或改用 addEventListener；
- 原生 IPC 合并到约 15 次/秒，列表按 id 复用 DOM，Markdown 结果缓存；
- 光标轮询自适应（40/100/500ms）并缓存遮挡检测结果；辅助 WebView 关闭即销毁；
- 睡眠唤醒/网络恢复时立即打断退避并重连；
- tauri.conf.json 设置 bundle.macOS.minimumSystemVersion = "11.0" 以上。
```
