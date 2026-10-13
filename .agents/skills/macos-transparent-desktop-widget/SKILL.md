---
name: macos-transparent-desktop-widget
description: Implementation guide and engineering patterns for macOS transparent liquid-glass desktop widgets in Tauri, Electron, and native apps. Solves window levels, Stage Manager layering, Cocoa shadow clipping, and WebKit color stability.
---

# macOS Transparent Liquid-Glass Desktop Widget Skill

Use this skill whenever building or refactoring a desktop application to support a pinned, transparent, liquid-glass desktop widget mode on macOS (especially with Tauri, Rust, Electron, or Cocoa).

## 1. When to Use
- Implementing a persistent desktop widget that stays on the desktop background.
- Making a window transparent without black rectangular corner shadows or cutoffs.
- Preventing a desktop widget from covering macOS Stage Manager (台前调度) thumbnail cards while preserving mouse interactivity.
- Fixing color flickering/shifting in transparent webview windows caused by focus/unfocus WebKit backdrop-filter repaints.

---

## 2. Core Cocoa Architecture (macOS / Rust)

### 2.1 The Magic Window Level: `-1`
- **Level 0 (`kCGNormalWindowLevel`)**: Regular apps and Stage Manager tiles reside here. Setting widget to 0 causes it to pop over Stage Manager when clicked.
- **Level `-2147483623` (`kCGDesktopWindowLevel`)**: Finder desktop wallpaper layer. Finder steals all mouse clicks; webview cannot be clicked.
- **Level `-1` (`kCGNormalWindowLevel - 1`)**: 
  - Strictly below all Level 0 apps and Stage Manager tiles (widget NEVER covers Stage Manager).
  - Strictly above Finder wallpaper layer (mouse clicks, drags, sliders work 100%).

```rust
let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
msg_isize(ns_window, level_sel, -1);
```

### 2.2 Window Collection Behavior
Must include `NSWindowCollectionBehaviorCanJoinAllApplications` (`262144`):
```rust
// 1: CanJoinAllSpaces | 16: Stationary | 64: IgnoresCycle | 256: FullScreenAuxiliary | 262144: CanJoinAllApplications
let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
msg_usize(ns_window, behavior_sel, 1 | 16 | 64 | 256 | 262144);
```

### 2.3 Eliminating Black Corner Shadows
```rust
// Disable system shadow
let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
msg_usize(ns_window, shadow_sel, 0);

// Disable opacity
let opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as _);
msg_usize(ns_window, opaque_sel, 0);

// Clear background color
let nscolor_cls = objc_getClass(b"NSColor\0".as_ptr() as _);
let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as _);
let clear_color = f_clear(nscolor_cls, clear_sel);
let bg_sel = sel_registerName(b"setBackgroundColor:\0".as_ptr() as _);
f_bg(ns_window, bg_sel, clear_color);

// Invalidate shadow
let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
f_inval(ns_window, inval_sel);

// Order back to stay at bottom
let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
f_order(ns_window, order_back_sel, std::ptr::null_mut());
```

---

## 3. Webview CSS & Apple Liquid Glass Aesthetics

### 3.1 Avoiding Focus / Unfocus Color Shifting
WebKit automatically desaturates and dims `backdrop-filter` on inactive windows.
1. **Never use high `saturate()` or `brightness()`** filters.
2. Use stable Gaussian blur: `backdrop-filter: blur(32px) !important;`.
3. **Never apply `transition: background`** on the main card container.
4. Use constant semi-transparent liquid glass background:
   - Dark: `rgba(22, 28, 40, var(--widget-opacity, 0.75))`
   - Light: `rgba(245, 247, 252, var(--widget-opacity, 0.75))`

### 3.2 Eliminating Outer Black Halos
Never use outer `box-shadow` (macOS clipping creates black fringes). Use **inset glows and micro borders**:
```css
.desktop-widget-card {
    border-radius: 26px !important;
    background: rgba(22, 28, 40, var(--widget-opacity, 0.75)) !important;
    backdrop-filter: blur(32px) !important;
    -webkit-backdrop-filter: blur(32px) !important;
    border: 1px solid rgba(255, 255, 255, 0.16) !important;
    box-shadow:
        inset 0 1.5px 1px 0 rgba(255, 255, 255, 0.28),
        inset 0 -1px 1px 0 rgba(0, 0, 0, 0.25),
        inset 0 0 0 0.5px rgba(255, 255, 255, 0.08) !important;
}
```

---

## 4. Interaction & DPI Scaling Rules

1. **Drag vs Click Mutex**:
   When using `start_dragging_window`, exclude interactive elements using `event.target.closest(...)`.
2. **Always Use Logical Pixels**:
   Divide physical size by `window.scale_factor()` on Retina displays before saving or computing sizes to prevent recursive exponential window expansion.

---

## 5. macOS 27 Local Network Code Signing Fix (os error 65)
For apps connecting to LAN IP addresses (e.g. `192.168.x.x`), macOS NECP blocks ad-hoc binaries without bound bundle identifiers:
```bash
codesign --force --deep --sign - --identifier "com.yourdomain.app" "Target.app"
```
Ensure `Info.plist` contains `NSLocalNetworkUsageDescription`.
