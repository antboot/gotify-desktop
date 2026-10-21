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
11. [可直接发给 AI 的标准 Prompt 提示词模板](#11-可直接发给-ai-的标准-prompt-提示词模板)

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
- 当光标位于小组件矩形内时，向 Webview 发送 `native-mouse-hover` 坐标；
- 前端通过 `document.elementFromPoint(x, y)` 命中卡片，触发已阅流光动效并在移出时平滑标记已阅，**用户无需点击激活窗口即可划过即读**！

---

## 7. 前端 CSS 与苹果液态玻璃拟态美学

### 7.1 彻底根除“点击小组件后颜色变来变去”
- **严禁使用 `saturate(...)` 和 `brightness(...)`**：WebKit 在窗口激活和失焦时会对高饱和度毛玻璃进行动态调光，造成色彩抽搐。
- **纯高斯毛玻璃**：`backdrop-filter: blur(32px) !important;`。
- **恒定半透底色**：通过 `--widget-opacity` 变量统一驱动。

### 7.2 纯内发光打造晶莹浮雕（杜绝外投影黑边）
```css
.desktop-widget-card {
    border-radius: 16px !important;
    clip-path: inset(0 round 16px) !important;
    -webkit-clip-path: inset(0 round 16px) !important;
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
针对访问局域网 IP（`192.168.x.x`）的组件应用：
1. `Info.plist` 注入权限描述：
   ```xml
   <key>NSLocalNetworkUsageDescription</key>
   <string>连接本地 Gotify 服务器以实时接收推送通知</string>
   ```
2. 打包后执行深度自签名：
   ```bash
   codesign --force --deep --sign - --identifier "com.gotify.notifier" "Gotify Notifier.app"
   ```

---

## 11. 可直接发给 AI 的标准 Prompt 提示词模板

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
- 支持无焦点全局光标悬停检测 (native-mouse-hover)，光标划过通知卡片自动平滑标记已阅。
```
