---
name: macos-transparent-desktop-widget
description: Implementation guide and engineering patterns for macOS transparent liquid-glass desktop widgets in Tauri, Electron, and native apps. Solves window levels, Stage Manager layering, Cocoa shadow clipping, and WebKit color stability.
---

# macOS Transparent Liquid-Glass Desktop Widget Skill

Use this skill whenever building, refactoring, or optimizing a desktop widget application on macOS (Tauri, Rust, Electron, or native Cocoa/Swift). It provides battle-tested solutions for window levels, Stage Manager integration, system tray / status bar synchronization, Dock vs Menu Bar behavior, global unfocused mouse hover tracking, dual opacity states, and liquid glass styling.

---

## 1. When to Use
- Building a persistent desktop widget that stays anchored to the desktop wallpaper without obscuring Stage Manager.
- Implementing dual-mode switching: **Floating Foreground (置顶前台)** vs **Pinned Desktop Background (常驻桌面底层)**.
- Making transparent windows free from black rectangular corner shadows, halo cutoffs, or color-shifting glitches.
- Operating as a pure background service (MenuBar / Status Bar only, hidden from Dock by default) with runtime Dock toggle.
- Enabling mouse hover detection on background/unfocused windows without requiring the user to click or focus the window.
- Resolving macOS 15+ / 27 Local Network Privacy restrictions (`os error 65: No route to host`).

---

## 2. Core Cocoa Window Management (Rust / Objective-C)

### 2.1 Pinned Desktop Mode (常驻桌面底层)

#### 1. The Magic Window Level: `-1` (`kCGNormalWindowLevel - 1`)
- **Level 0 (`kCGNormalWindowLevel`)**: Standard application windows and Stage Manager thumbnail tiles. If set to 0, clicking the widget will bring it above Stage Manager.
- **Level `-2147483623` (`kCGDesktopWindowLevel`)**: Desktop wallpaper layer owned by Finder. Finder steals all mouse clicks; webviews become unclickable.
- **Level `-1`**:
  - Strictly below all Level 0 apps and Stage Manager thumbnail tiles (the widget never covers Stage Manager).
  - Strictly above Finder's wallpaper layer (mouse clicks, drags, sliders, context menus work 100%).

```rust
let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
msg_isize(ns_window, level_sel, -1);
```

#### 2. Collection Behavior
Declare the window as a global auxiliary desktop accessory:
```rust
// 1: CanJoinAllSpaces (visible across all Spaces/desktops)
// 16: Stationary (does not slide when switching Spaces)
// 64: IgnoresCycle (excluded from Cmd+Tab application cycling)
// 256: FullScreenAuxiliary (remains visible even over full-screen apps)
// 262144: CanJoinAllApplications (Stage Manager recognizes as desktop widget, never treated as a stage window)
let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
msg_usize(ns_window, behavior_sel, 1 | 16 | 64 | 256 | 262144);
```

#### 3. Eliminating Black Corner Shadows & Backdrops
```rust
// 1. Disable system shadow (prevents 4 black rectangular corners)
let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
msg_usize(ns_window, shadow_sel, 0); // hasShadow: NO

// 2. Disable opacity
let opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as _);
msg_usize(ns_window, opaque_sel, 0); // isOpaque: NO

// 3. Clear background color to transparent
let nscolor_cls = objc_getClass(b"NSColor\0".as_ptr() as _);
let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as _);
let clear_color = f_clear(nscolor_cls, clear_sel);
let bg_sel = sel_registerName(b"setBackgroundColor:\0".as_ptr() as _);
f_bg(ns_window, bg_sel, clear_color);

// 4. Invalidate shadow
let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
f_inval(ns_window, inval_sel);

// 5. Send behind normal windows onto wallpaper
let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
f_order(ns_window, order_back_sel, std::ptr::null_mut());
```

#### 4. Programmatic Traffic Light Button Hiding
Do not destroy `FullSizeContentView`. Instead, hide the 3 buttons:
```rust
let button_sel = sel_registerName(b"standardWindowButton:\0".as_ptr() as _);
let hidden_sel = sel_registerName(b"setHidden:\0".as_ptr() as _);
for btn_type in 0..=2 { // Close, Minimize, Zoom
    let btn = f_btn(ns_window, button_sel, btn_type);
    if !btn.is_null() {
        f_hide(btn, hidden_sel, 1); // hidden = YES
    }
}
```

---

### 2.2 Floating Foreground Mode (悬浮置顶前台)
When the user pins the widget to the top of all apps:
```rust
// 1. Restore normal window shadow
let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
msg_usize(ns_window, shadow_sel, 1); // hasShadow: YES

// 2. Window Level 3 (kCGFloatingWindowLevel)
msg_isize(ns_window, level_sel, 3);

// 3. Order front above all applications
let order_front_sel = sel_registerName(b"orderFront:\0".as_ptr() as _);
f_order(ns_window, order_front_sel, std::ptr::null_mut());
```

---

### 2.3 Prevent Stage Manager Edge Repulsion
Stage Manager calls `constrainFrameRect:toScreen:` on NSWindow to push windows away from screen edges. Hook and override this method:
```rust
unsafe extern "C" fn custom_constrain(
    _self: *mut c_void,
    _cmd:  *mut c_void,
    mut r: CGRect,
    _screen: *mut c_void,
) -> CGRect {
    // Only constrain y axis to keep on screen; do NOT constrain x axis
    r
}
```

---

## 3. Activation Policy: Dock vs Menu Bar (Status Bar)

Background widgets and notification daemons should run as **Menu Bar accessories by default** and not clutter the macOS Dock:

### 3.1 Initial Launch
```rust
// Default: Accessory (No Dock icon, runs in Menu Bar / Status Bar)
let show_in_dock = load_show_in_dock_config().unwrap_or(false);
let policy = if show_in_dock {
    tauri::ActivationPolicy::Regular
} else {
    tauri::ActivationPolicy::Accessory
};
app.set_activation_policy(policy);
```

### 3.2 Dynamic Runtime Toggle
Switching does not require an app restart:
```rust
#[tauri::command]
fn set_show_in_dock(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    save_show_in_dock_to_file(&enabled)?;
    let policy = if enabled {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    let _ = app.set_activation_policy(policy);
    // Update Menu Bar menu text and emit event to frontend
    update_tray_menu_text(app, enabled);
    Ok(())
}
```

---

## 4. Unfocused Global Mouse Tracking (无焦鼠标滑过已阅)

### 4.1 The Problem
When a desktop widget is in Level `-1` or accessory mode, it does **not** have active key window focus. Standard WebKit `:hover` and `onmouseenter` will fail to fire as the user moves their cursor over notifications.

### 4.2 The Solution: Hybrid NSTrackingArea & Cursor Polling
1. Register `NSTrackingArea` with `NSTrackingActiveAlways`:
   ```objc
   NSTrackingAreaOptions options = NSTrackingMouseMoved | 
                                  NSTrackingActiveAlways | 
                                  NSTrackingInVisibleRect;
   ```
2. Or run a lightweight 50ms polling loop on macOS querying cursor position relative to window bounds:
   ```rust
   // When cursor is within window bounds, emit "native-mouse-hover" { x, y }
   window.emit("native-mouse-hover", json!({ "x": cx, "y": cy }));
   ```
3. Frontend uses `document.elementFromPoint(x, y)` to trigger card hover effects and mark unread items as read even without window focus!

---

## 5. Dual-Opacity State Machine (置顶实感 vs 底层通透)

Desktop widgets need distinct visual weights depending on their mode:
- **Foreground (Floating)**: High opacity (`0.92`), dark contrast, crystal-clear text readability.
- **Background (Desktop)**: High translucency (`0.45`), ethereal liquid glass that seamlessly blends into wallpaper.

### 5.1 CSS Implementation
```css
/* Smooth breathing transition on opacity switch */
.desktop-widget-card {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 16px !important;
    overflow: hidden !important;
    transform: translateZ(0);
    -webkit-transform: translateZ(0);
    will-change: transform, backdrop-filter;
    -webkit-backface-visibility: hidden;
    backface-visibility: hidden;
    background: rgba(22, 28, 40, var(--widget-opacity, 0.45)) !important;
    backdrop-filter: blur(32px) !important;
    -webkit-backdrop-filter: blur(32px) !important;
    border: 1px solid rgba(255, 255, 255, 0.16) !important;
    /* Inset glow only, NO outer box-shadow to prevent black halo clippings */
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.28),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.25),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.08) !important;
}

/* Enable smooth transition only during explicit state changes */
.desktop-widget-card.is-transitioning {
    transition: background 0.35s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.35s ease, box-shadow 0.35s ease;
}

[data-theme="light"] .desktop-widget-card {
    background: rgba(245, 247, 252, var(--widget-opacity, 0.45)) !important;
    border: 1px solid rgba(255, 255, 255, 0.65) !important;
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.85),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.05),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.35) !important;
}
```

### 5.2 Avoiding WebKit Color Flashes & Compositor Drops
1. **Never use `clip-path` on elements with `backdrop-filter`**: WebKit breaks `CABackdropLayer` GPU compositing and falls back to an unstable masking FBO, causing 1-frame black/dark background flashes during DOM repaints. Use `border-radius` + `overflow: hidden` instead.
2. **Never use `saturate(...)` or `brightness(...)`** in `backdrop-filter`. WebKit dims them when inactive.
3. **Pin GPU Compositing Layer**: Add `will-change: transform, backdrop-filter;` and `-webkit-backface-visibility: hidden;` to keep the backdrop buffer in VRAM.
4. **Isolate `transition: background`**: Do not keep `transition: background` enabled permanently on the card; only apply it (via an `.is-transitioning` class) when user explicitly toggles modes or adjusts the slider, preventing 16ms repaint ticks from stretching into visible 350ms dark flashes.
5. **Zero DOM-Churn on Timers**: When empty or unchanged, never re-assign `innerHTML` or rewrite DOM attributes inside periodic timers.

---

## 6. Interaction & Context Menu Synchronization

### 6.1 Click-Outside Dismissal in Drag Regions
When `start_dragging_window` is called, Win32 / Cocoa enters a modal drag loop, preventing standard `click` events from firing on document.
- **Rule**: Register a capture-phase listener on `window`:
```javascript
window.addEventListener('pointerdown', (e) => {
    const menu = document.getElementById('contextMenu');
    if (menu && menu.style.display !== 'none' && !menu.contains(e.target)) {
        menu.style.display = 'none';
    }
}, { capture: true });
```
- Also close menus on `window.blur`.

### 6.2 Status Bar (Tray) & Card Menu Synchronization
Ensure text reflects current state bi-directionally:
- **Foreground Mode**: Menu shows `🖥️ 常驻桌面 (底层)` (Click to send to desktop).
- **Desktop Mode**: Menu shows `📌 悬浮置顶 (前台)` (Click to bring to front).
- **Dock Icon State**: Menu shows `🍎 显示 Dock 图标` vs `🍎 隐藏 Dock 图标`.
- Emit Tauri events on change to keep frontend settings and status bar menu strictly in sync.

---

## 7. Retina Scaling & Logical Coordinates
- Never save or compute physical pixels from `window.inner_size()`.
- Always divide by `window.scale_factor()`:
  ```rust
  let logical_w = ((size.width as f64) / factor).round() as u32;
  let logical_h = ((size.height as f64) / factor).round() as u32;
  ```
- Always invoke `window.set_size(LogicalSize)`.

---

## 8. macOS Local Network Code Signing (os error 65)
For apps connecting to LAN IP addresses (e.g. `192.168.x.x`):
- macOS 15/27 NECP blocks network access if ad-hoc signed without a bundle ID.
- In `Info.plist`, include:
  ```xml
  <key>NSLocalNetworkUsageDescription</key>
  <string>Gotify Notifier connects to your local Gotify server to receive real-time notifications.</string>
  ```
- Sign the app bundle:
  ```bash
  codesign --force --deep --sign - --identifier "com.gotify.notifier" "Gotify Notifier.app"
  ```
