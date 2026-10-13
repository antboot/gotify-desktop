# macOS 透明桌面小组件工程级技术实现指南 (AI 实施 Prompt 与落地手册)

> **导语**：本指南记录了在 **Rust + Tauri 2** 架构下，从 0 到 1 实现极具苹果质感、常驻桌面最底层、完美兼容台前调度（Stage Manager）且拥有无级透明度调节的“桌面小组件模式”全套工程解决方案。
> 你可以直接将本文档作为 Prompt 发送给任何 AI，要求其在其他项目（无论是 Tauri、Electron 还是原生 Swift/Cocoa）中照此方案实现透明桌面小组件。

---

## 目录
1. [核心挑战与避坑总览](#1-核心挑战与避坑总览)
2. [macOS Cocoa 底层窗口调度 (Rust Objective-C)](#2-macos-cocoa-底层窗口调度-rust-objective-c)
3. [Tauri 2 核心窗口配置](#3-tauri-2-核心窗口配置)
4. [前端 CSS 与苹果液态玻璃拟态美学](#4-前端-css-与苹果液态玻璃拟态美学)
5. [点击与拖拽交互互斥、DPI 缩放防炸裂处理](#5-点击与拖拽交互互斥dpi-缩放防炸裂处理)
6. [macOS 27 本地网络权限与签名避坑 (os error 65)](#6-macos-27-本地网络权限与签名避坑-os-error-65)
7. [可直接发给 AI 的标准 Prompt 提示词模板](#7-可直接发给-ai-的标准-prompt-提示词模板)

---

## 1. 核心挑战与避坑总览

在桌面端开发“透明桌面小组件”时，新手和常规实现往往会遇到 5 大致命坑点：
1. **四个黑色直角 / 矩形阴影毛刺**：Webview 透明了，但 macOS 系统窗口默认带有矩形投影 `NSWindow.hasShadow` 和黑色底色，导致圆角外围露出 4 个黑色直角。
2. **遮挡台前调度 (Stage Manager)**：普通窗口处于 `Level 0`，一旦被用户点击激活，窗口立刻升到顶层，直接压在 macOS 台前调度的侧边缩略图卡片上方。
3. **点击事件丢失**：如果盲目将窗口设置到系统壁纸层（如 `kCGDesktopWindowLevel = -2147483623`），macOS 访达（Finder）的桌面全屏视图会截获所有鼠标事件，导致小组件里的按钮、滚动条完全无法点击。
4. **点击小组件后颜色变来变去**：WebKit 在窗口处于活动（Key Window）和非活动（Inactive Window）状态时，会对带有 `saturate(...)` 的 `backdrop-filter` 动态去饱和与调暗，导致用户一点击小组件颜色就突变抽搐。
5. **Retina 屏幕物理/逻辑像素混淆**：在 2x/3x Retina 屏幕下，反复读取与设置窗口宽高会导致尺寸指数级膨胀爆炸。

---

## 2. macOS Cocoa 底层窗口调度 (Rust Objective-C)

在 Tauri / Rust 的 `macos_window` 模块中，必须通过 Cocoa Objective-C 运行时对窗口进行以下关键设置：

### 2.1 窗口层级设定为 `-1` (`kCGNormalWindowLevel - 1`)
这是实现“既不遮挡台前调度，又能正常点击”的核心魔法数值：
- **普通窗口 & 台前调度卡片**：位于 `kCGNormalWindowLevel` (即 `0`) 或更高。
- **访达桌面壁纸层**：位于 `kCGDesktopWindowLevel` (即 `-2147483623`)。
- **将小组件精准设为 `-1`**：
  - 严格低于所有正常窗口和台前调度卡片，**台前调度卡片永远位于小组件上方，绝不遮挡**。
  - 远远高于壁纸层（`-1 >> -2147483623`），**鼠标点击事件绝不会被桌面背景拦截**。

```rust
// 设置层级为 -1
let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
msg_isize(ns_window, level_sel, -1);
```

### 2.2 集合行为配置 (加入 `CanJoinAllApplications`)
告诉 macOS 系统窗口调度器与台前调度：该窗口是一个全局辅助小组件，不要将其纳为主舞台窗口：
```rust
// 1: NSWindowCollectionBehaviorCanJoinAllSpaces (所有桌面空间可见)
// 16: NSWindowCollectionBehaviorStationary (切换桌面时不跟着滑动)
// 64: NSWindowCollectionBehaviorIgnoresCycle (Cmd+Tab 轮选不选中)
// 256: NSWindowCollectionBehaviorFullScreenAuxiliary (全屏时作为辅助窗口)
// 262144: NSWindowCollectionBehaviorCanJoinAllApplications (Stage Manager 识别为桌面小组件，不作为独立舞台应用管理或遮挡卡片)
let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
msg_usize(ns_window, behavior_sel, 1 | 16 | 64 | 256 | 262144);
```

### 2.3 彻底消除系统直角黑色阴影与底色
```rust
// 禁用系统阴影 (避免 4 个黑色直角)
let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
msg_usize(ns_window, shadow_sel, 0); // NO

// 禁用不透明
let opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as _);
msg_usize(ns_window, opaque_sel, 0); // NO

// 背景色设为完全透明 clearColor
let nscolor_cls = objc_getClass(b"NSColor\0".as_ptr() as _);
let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as _);
let clear_color = f_clear(nscolor_cls, clear_sel);
let bg_sel = sel_registerName(b"setBackgroundColor:\0".as_ptr() as _);
f_bg(ns_window, bg_sel, clear_color);

// 强制刷新阴影
let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
f_inval(ns_window, inval_sel);

// 沉底
let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
f_order(ns_window, order_back_sel, std::ptr::null_mut());
```

### 2.4 防止 Stage Manager 强行将窗口推离 x=0
台前调度有时会调用 `constrainFrameRect:toScreen:` 强行将靠近屏幕左侧边缘的窗口往右推。可以通过在窗口的现有类上挂载方法覆写：
```rust
unsafe extern "C" fn custom_constrain(
    _self: *mut c_void,
    _cmd:  *mut c_void,
    mut r: CGRect,
    screen: *mut c_void,
) -> CGRect {
    // 仅约束 y 轴不超出屏幕，故意不对 x 轴做任何强行约束，允许小组件贴紧屏幕最左侧
    r
}
```

---

## 3. Tauri 2 核心窗口配置

在 `tauri.conf.json` 中配置主窗口为透明：
```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "decorations": true,
        "transparent": true,
        "shadow": false
      }
    ]
  }
}
```
进入小组件模式时动态切换：
```rust
let _ = window.set_decorations(false);
let _ = window.set_shadow(false);
```

---

## 4. 前端 CSS 与苹果液态玻璃拟态美学

### 4.1 彻底根除“点击小组件后颜色变来变去”的 Bug
**成因**：使用了 `saturate(200%+)`，且设置了 `transition: background`。当窗口激活（Key Window）与失焦（Inactive）时，WebKit 动态调节饱和度引发色彩剧变。

**解决方案**：
1. **绝对不要**在透明窗口上使用 `saturate(...)` 和 `brightness(...)`！
2. 改用系统原生平稳的高斯毛玻璃：`backdrop-filter: blur(32px) !important;`。
3. **移除卡片外壳上的 `transition: background`**。
4. 设定自然恒定的液态玻璃半透底色（通过 CSS 变量 `--widget-opacity` 控制透明度）。

### 4.2 消除外部黑边：严禁向外投影，纯靠内部发光 (Inset Box Shadow)
如果使用普通的 `box-shadow: 0 10px 20px ...`，在 macOS 透明窗口边缘会被剪裁成直角毛刺。
**必须全部使用内发光与高精度微边框实现浮雕感**：

```css
/* 容器必须完全透明 */
html.widget-mode,
html.widget-mode body {
    background: transparent !important;
    margin: 0 !important;
    padding: 0 !important;
    overflow: hidden !important;
}

/* 桌面小组件卡片：与 macOS 系统原生窗口一致的标准圆角 (16px) + 液态玻璃质感 */
.desktop-widget-card {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 16px !important;
    /* 恒定深色半透底色 */
    background: rgba(22, 28, 40, var(--widget-opacity, 0.75)) !important;
    backdrop-filter: blur(32px) !important;
    -webkit-backdrop-filter: blur(32px) !important;
    border: 1px solid rgba(255, 255, 255, 0.16) !important;
    /* 核心：纯内发光打造晶莹浮雕玻璃，杜绝任何外投影产生黑边 */
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.28),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.25),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.08) !important;
    display: flex !important;
    flex-direction: column !important;
    overflow: hidden !important;
    box-sizing: border-box !important;
    user-select: none;
    -webkit-user-select: none;
    pointer-events: auto !important;
    cursor: grab;
}

/* 浅色主题模式适配（晶莹水晶白） */
[data-theme="light"] .desktop-widget-card {
    background: rgba(245, 247, 252, var(--widget-opacity, 0.75)) !important;
    backdrop-filter: blur(32px) !important;
    -webkit-backdrop-filter: blur(32px) !important;
    border: 1px solid rgba(255, 255, 255, 0.65) !important;
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.85),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.05),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.35) !important;
}
```

---

## 5. 点击与拖拽交互互斥、DPI 缩放防炸裂处理

### 5.1 自由拖拽与按钮点击互斥
在卡片容器上绑定 `onmousedown="onWidgetMouseDown(event)"`，但必须精准过滤交互元素：
```javascript
function onWidgetMouseDown(event) {
    if (!event || event.button !== 0) return;
    // 排除交互按钮、滑块、列表项点击
    if (event.target.closest('.widget-btn') ||
        event.target.closest('.widget-actions') ||
        event.target.closest('#widgetOpacityPopover') ||
        event.target.closest('.widget-item')
    ) {
        return;
    }
    // 调用 Tauri 原生无缝拖拽
    if (window.__TAURI_INTERNALS__?.invoke) {
        window.__TAURI_INTERNALS__.invoke('start_dragging_window');
    }
}
```

### 5.2 窗口尺寸防膨胀（物理像素 vs 逻辑像素）
在切换窗口模式或调整大小时，必须严格使用**逻辑像素**：
```rust
let factor = window.scale_factor().unwrap_or(1.0);
if let Ok(size) = window.inner_size() {
    let logical_w = ((size.width as f64) / factor).round() as u32;
    let logical_h = ((size.height as f64) / factor).round() as u32;
    // 存储与复原必须始终基于逻辑像素！
}

// 设置尺寸必须使用 LogicalSize
window.set_size(tauri::Size::Logical(tauri::LogicalSize {
    width: target_w as f64,
    height: target_h as f64,
}))?;
```

---

## 6. macOS 27 本地网络权限与签名避坑 (os error 65)

若该桌面组件需要发起局域网连接（如连接路由器、NAS、局域网服务器）：
- **原因**：macOS 27 内核网络扩展（NECP）若检测到进程签名与系统本地网络隐私权限绑定的 Bundle Identifier 不一致，会拦截 Socket 发包，报错 `os error 65: No route to host`。
- **解决**：在构建后必须自动对 `.app` 进行深度签名：
```bash
codesign --force --deep --sign - --identifier "com.yourcompany.app" "Target.app"
```
确保包含 `Info.plist entries` 且 `Sealed Resources version=2`。

---

## 7. 可直接发给 AI 的标准 Prompt 提示词模板

```markdown
请帮我将当前应用实现一个类似 macOS 原生桌面挂件的“透明桌面小组件模式”：

【核心架构要求】
1. 窗口层级 (macOS Window Level)：
   - 在 macOS Cocoa 层必须将窗口 Level 精准设置为 -1 (kCGNormalWindowLevel - 1)。
   - 严禁设置为 0 (会被提升盖在台前调度 Stage Manager 上方)。
   - 严禁设置为 -2147483623 壁纸层 (会被 Finder 截获导致点击完全失效)。
   - 窗口 CollectionBehavior 必须添加 NSWindowCollectionBehaviorCanJoinAllApplications (262144)，声明为全局辅助挂件。

2. 视觉与消灭黑边：
   - 必须通过 Cocoa setHasShadow:NO, setOpaque:NO, setBackgroundColor:clearColor 彻底清除窗口直角黑边。
   - CSS 层面卡片外壳严禁使用向外投影 box-shadow，必须使用多层 inset 内发光打造 Apple 液态玻璃浮雕质感。
   - backdrop-filter 仅使用稳定的 blur(32px)，严禁使用高倍率 saturate() 和背景 transition，确保窗口在聚焦/失焦时底色完全恒定不跳变。

3. 交互与功能：
   - 支持空状态与消息/内容列表状态切换。
   - 支持无级透明度调节（通过 CSS 变量 --widget-opacity 控制）。
   - 卡片空白区域按住可自由拖拽窗口位置，且与卡片内部按钮、列表项点击严格互斥。
   - 所有窗口尺寸计算必须严格除以 scale_factor 转换为逻辑像素，杜绝 Retina 屏幕尺寸膨胀。
```
