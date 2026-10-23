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
- Embedding Apple's **native `NSGlassEffectView` liquid glass** (macOS 26+) with custom refraction / blur / specular tuning.
- Eliminating **focus-state glass flashes** and cross-widget linkage caused by AppKit key-window assignment.
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

### 4.2 Occlusion-Aware Cursor Polling ( Recommended )
Unfocused/Low-level windows frequently lose `mouseenter`/`mouseleave`; poll the cursor every ~50 ms instead.
Also verify the cursor is not covered by another app, otherwise "hover" fires under overlapping windows:
```rust
// CGWindowListCopyWindowInfo returns windows front-to-back.
// The first window whose bounds contain the cursor must be ours.
// Skip: alpha <= 0.001 (invisible helpers) and layer >= 20 (Dock's full-screen transparent window).
let topmost_is_ours = /* front-to-back scan */;
```
Without the `layer >= 20` / `alpha ~ 0` filters the widget will *never* count as hovered on a normal desktop, because the Dock owns a full-screen transparent window above it.

### 4.3 The Solution: Hybrid NSTrackingArea & Cursor Polling
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

### 4.4 Multi-Widget Event Isolation (Tauri)
`emit_to(window_label, ...)` only filters listeners that were registered with an explicit target.
A JS listener created with the default `{ kind: 'Any' }` receives events from **all** windows, so two widgets cross-trigger each other:
```js
// WRONG: receives the other widget's focus/hover events too
await listen('widget-focus', handler);

// RIGHT:
await listen('widget-focus', handler, { target: { kind: 'WebviewWindow', label: 'app_shelf' } });
```

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
- Also add ATS local networking:
  ```xml
  <key>NSAppTransportSecurity</key>
  <dict>
      <key>NSAllowsArbitraryLoads</key><true/>
      <key>NSAllowsLocalNetworking</key><true/>
  </dict>
  ```
- Sign the app bundle with a stable identifier **and an entitlements file**:
  ```bash
  codesign --force --deep --sign "$IDENTITY" \
    --entitlements src-tauri/Entitlements.plist \
    --identifier "com.gotify.notifier" "Gotify Notifier.app"
  ```
  `Entitlements.plist` should contain `com.apple.security.network.client` (and `network.server` if needed).
- Verify the signature is actually *bound*, not just present:
  ```bash
  codesign -dvvv "Gotify Notifier.app" | grep -E "Identifier=|Info.plist|Sealed Resources"
  # Identifier=com.gotify.notifier
  # Info.plist entries=16
  # Sealed Resources version=2 rules=13 files=1
  codesign --verify --deep --strict --verbose=2 "Gotify Notifier.app"
  ```
  If you see `Info.plist=not bound` / `Sealed Resources=none`, macOS 27 NECP will silently block LAN sockets with `EHOSTUNREACH (os error 65)` even when Local Network permission is enabled.
- **Run the app from `/Applications`**, not from `target/debug|release/bundle`, after signing. Re-register with LaunchServices after replacing it:
  ```bash
  xattr -cr "/Applications/Gotify Notifier.app"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "/Applications/Gotify Notifier.app"
  ```
- **Dual NIC same-subnet conflict**: if two interfaces share `192.168.x.0/24`, macOS 27 pins traffic to the higher-priority service and returns `No route to host` when that NIC is unplugged. Disable the unused NIC or fix the service order.

---

## 9. Native Liquid Glass: `NSGlassEffectView` (macOS 26+)

`backdrop-filter` cannot reproduce Apple's real glass lens. On macOS 26+ you can embed the system view:

```objc
Class GlassCls = NSClassFromString(@"NSGlassEffectView");
NSView *glassView = [[GlassCls alloc] initWithFrame:contentView.bounds];
[glassView setValue:@(20.0) forKey:@"cornerRadius"];   // squircle radius
[contentView addSubview:glassView positioned:NSWindowBelow relativeTo:nil]; // BELOW the webview
```

Key rules:
1. **Webview must be transparent** (`setValue:@NO forKey:@"drawsBackground"`, clear background) so the glass below shows through.
2. **Rebuild detection**: store `variant/radius/inset` in `objc_setAssociatedObject` on both the window and the glass view; rebuild when they differ, otherwise reuse.
3. **Private optical knobs** live on the backdrop layer (`CALayer` sublayer) filters:
   - `filters.glassBackground.inputInnerRefractionAmount`, `inputOuterRefractionAmount` – lens curvature
   - `inputBlurRadius` – blur (0 = crystal clear droplet)
   - `inputKeyFillHighlightAmount`, `inputBlurFillLightenOpacity` – edge light / fill
   - `inputLumaNoiseScale`, arc height – texture & arc
   - `CASDFLayer` specular sublayers (`@2`, `@3`): `opacity ≈ 0.75 * highlightMultiplier`
4. **Persist user tuning** as associated objects (`highlight`, `refraction`, `blur`, `arcHeight`) so it survives Apple's internal layer rebuilds.
5. **Restoring after focus/appearance changes**: Apple rewrites the highlight when the window becomes/resigns key. Swizzle `-[NSGlassEffectView _windowChangedKeyState]` and re-apply the user's tuning asynchronously (e.g. at 30/120/400/900 ms) plus a 16 ms guard loop for ~640 ms.
6. Keep `subduedState` pinned to `0`, and force `scrimState` / `interactionState` to `0` if the OS dims or pulses the panel.
7. Kill stray CA transitions when snapping parameters:
   ```objc
   for (CALayer *l in layer.sublayers) { [l removeAllAnimations]; recurse(l); }
   ```
8. NSGlassEffectView is **macOS 26+ only** — guard with `NSClassFromString` and fall back to CSS glass on older systems.

---

## 10. Non-Activating Widget Windows (critical for desktop widgets)

### 10.1 Symptom
Clicking a widget makes its specular highlight flash, **and the other widget's glass flashes too** (cross-widget linkage).

### 10.2 Root cause
Clicking activates the app. AppKit then needs a key window. `canBecomeKeyWindow → NO` on the clicked window is **not enough** — during activation AppKit may bypass it and assign key status to another window of the same app. That window receives `windowDidBecomeKey`, and `NSGlassEffectView` re-renders the focus-state highlight (a visible jump), while your top-level tuning gets temporarily replaced by Apple's defaults.

### 10.3 Fix: prevent activation entirely
On every managed widget window (both desktop-pinned and floating):
```rust
let nonact = sel_registerName(b"_setNonactivatingPanel:\0".as_ptr() as _);
msg_usize(win, nonact, 1);
let prev = sel_registerName(b"_setPreventsActivation:\0".as_ptr() as _);
msg_usize(win, prev, 1);
```
Keep `canBecomeKeyWindow` / `canBecomeMainWindow` returning `NO`, and never call `NSApp.activateIgnoringOtherApps:`, `makeKeyAndOrderFront:` or Tauri `set_focus()` for these windows.

### 10.4 Verify
- Tauri `WindowEvent::Focused` must never fire for widget windows.
- `[window isKeyWindow] == false` after clicking.
- Consequence: widgets never receive keyboard input — keep all widget UI mouse-driven (drag, sliders, context menus). A separate Settings window can still be a normal key window.

### 10.5 Consequence for "sinking"
Because focus/resign events no longer fire, do **not** rely on `Focused(false)` to drop the window back to desktop level. Sink explicitly:
- on cursor leave (from the 50 ms hover poller), or
- after an idle timer.
Keeping the "glass active" state for ~3 s while sinking the z-order immediately on mouse-leave feels best.

---

## 11. Deadlock & Re-entrancy Rules for AppKit Callbacks

1. **Never call `[CATransaction flush]` synchronously inside an AppKit callback.** Key-state / appearance callbacks run inside the event loop; flushing re-enters tao/wry redraw and deadlocks against the event-loop mutex. The app freezes completely with 100% CPU on one thread.
   - Apply parameters immediately with `flush = NO` (same transaction), then schedule follow-up applies on later runloop turns (`dispatch_after`) with `flush = YES`.
2. **Re-entrancy guard.** Tuning code usually calls `set_subduedState:`, whose swizzle calls back into tuning → infinite recursion/stack overflow. Use a static `BOOL applying` flag.
3. **Do not call the original** `_windowChangedKeyState` if you fully own the appearance; call it only if you want Apple's default focus styling.
4. Diagnose hangs with `sample <pid> 2 -file /tmp/hang.txt` or `~/Library/Logs/DiagnosticReports/*.ips`; `lldb -p` is often blocked on hardened apps.

---

## 12. Build-System Pitfalls

- The `cc` crate does **not** track `.m` sources by default. Without
  ```rust
  println!("cargo:rerun-if-changed=src/macos_app_info.m");
  ```
  in `build.rs`, Objective-C edits silently reuse stale object files — you can spend hours debugging code that was never compiled.
- Launch the built binary directly to capture stdout/stderr:
  ```bash
  nohup "path/App.app/Contents/MacOS/app" > /tmp/app.log 2>&1 &
  ```
- After changing window/signature metadata, re-run `lsregister` and launch from `/Applications`.
