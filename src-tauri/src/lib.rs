use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;
use tauri::{Emitter, Manager, State};
use tauri::tray::{TrayIconBuilder, TrayIcon};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use image::GenericImageView;
use std::collections::HashMap;

// ── macOS: Override constrainFrameRect safely on tao's existing per-window class ──
// tao creates a unique NSWindow subclass per window. We add constrainFrameRect to THAT
// existing class via class_addMethod — NO object_setClass, NO isa change.
// This prevents Stage Manager from pushing the window away from x=0.
#[cfg(target_os = "macos")]
mod macos_window {
    use std::ffi::{c_char, c_void};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // The pointer of the window we want to override, so our constrainFrameRect
    // implementation can return early for other windows (if any exist).
    static FLOATING_WIN_PTR: AtomicUsize = AtomicUsize::new(0);

    #[link(name = "objc", kind = "dylib")]
    extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn object_getClass(obj: *mut c_void) -> *mut c_void;
        fn class_addMethod(
            cls: *mut c_void,
            name: *mut c_void,
            imp: unsafe extern "C" fn(),
            types: *const c_char,
        ) -> bool;
        fn class_getInstanceMethod(cls: *mut c_void, sel: *mut c_void) -> *mut c_void;
        fn method_setImplementation(method: *mut c_void, imp: unsafe extern "C" fn()) -> unsafe extern "C" fn();
        fn objc_msgSend();
    }

    extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: unsafe extern "C" fn(*mut c_void),
        );
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGPoint { x: f64, y: f64 }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGSize  { width: f64, height: f64 }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGRect  { origin: CGPoint, size: CGSize }

    /// Our constrainFrameRect override: removes x constraint, keeps y clamped to screen.
    unsafe extern "C" fn custom_constrain(
        _self: *mut c_void,
        _cmd:  *mut c_void,
        mut r: CGRect,
        screen: *mut c_void,
    ) -> CGRect {
        // Only override for the pinned Gotify window.
        if _self != FLOATING_WIN_PTR.load(Ordering::Relaxed) as *mut c_void {
            // For any other window: just return frame unchanged (safe fallback).
            return r;
        }
        if !screen.is_null() {
            let frame_sel = sel_registerName(b"frame\0".as_ptr() as _);
            let msg: unsafe extern "C" fn(*mut c_void, *mut c_void) -> CGRect =
                std::mem::transmute(objc_msgSend as *const ());
            let sf = msg(screen, frame_sel);
            // Clamp y so window doesn't go above top of screen or below bottom.
            let max_y = sf.origin.y + sf.size.height;
            if r.origin.y + r.size.height > max_y {
                r.origin.y = max_y - r.size.height;
            }
            if r.origin.y < sf.origin.y {
                r.origin.y = sf.origin.y;
            }
            // x is intentionally NOT constrained.
        }
        r
    }

    unsafe fn msg_isize(obj: *mut c_void, sel: *mut c_void, val: isize) {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
            std::mem::transmute(objc_msgSend as *const ());
        f(obj, sel, val);
    }
    unsafe fn msg_usize(obj: *mut c_void, sel: *mut c_void, val: usize) {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        f(obj, sel, val);
    }

    struct FloatCtx { ns_window: *mut c_void, enable: bool }
    unsafe impl Send for FloatCtx {}

    unsafe extern "C" fn do_set_floating(ctx: *mut c_void) {
        let boxed = Box::from_raw(ctx as *mut FloatCtx);
        let win = boxed.ns_window;
        if win.is_null() { return; }

        let level_sel    = sel_registerName(b"setLevel:\0".as_ptr() as _);
        let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
        let constrain_sel = sel_registerName(b"constrainFrameRect:toScreen:\0".as_ptr() as _);
        let types = b"{CGRect={CGPoint=dd}{CGSize=dd}}@:{CGRect={CGPoint=dd}{CGSize=dd}}@\0"
            .as_ptr() as *const c_char;
        let imp_fn: unsafe extern "C" fn() = std::mem::transmute(custom_constrain as *const ());

        if boxed.enable {
            // Store window pointer so custom_constrain knows which window to override.
            FLOATING_WIN_PTR.store(win as usize, Ordering::Relaxed);

            // Add constrainFrameRect to the window's existing class (tao's per-window subclass).
            // class_addMethod only succeeds if the method is NOT already in this exact class
            // (it's OK if it's in a superclass — that's what we want to override).
            let cls = object_getClass(win);
            let added = class_addMethod(cls, constrain_sel, imp_fn, types);
            if !added {
                // Method was already added on a previous toggle — replace its implementation.
                let m = class_getInstanceMethod(cls, constrain_sel);
                if !m.is_null() {
                    method_setImplementation(m, imp_fn);
                }
            }

            // NSFloatingWindowLevel = 3 (float above all apps)
            msg_isize(win, level_sel, 3);
            // NSWindowCollectionBehaviorCanJoinAllApplications (262144) = Stage Manager ignores management
            // NSWindowCollectionBehaviorStationary (16)                 = don't move when spaces change
            // NSWindowCollectionBehaviorCanJoinAllSpaces (1)
            // NSWindowCollectionBehaviorFullScreenAuxiliary (256)
            // NSWindowCollectionBehaviorIgnoresCycle (64)
            msg_usize(win, behavior_sel, 262144 | 256 | 64 | 16 | 1);
        } else {
            FLOATING_WIN_PTR.store(0, Ordering::Relaxed);
            msg_isize(win, level_sel, 0);
            msg_usize(win, behavior_sel, 0);
        }
    }

    pub fn schedule_set_floating(ns_window: *mut c_void, enable: bool) {
        if ns_window.is_null() { return; }
        let ctx = Box::into_raw(Box::new(FloatCtx { ns_window, enable }));
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ctx as *mut c_void, do_set_floating);
        }
    }

    struct DesktopPinCtx { ns_window: *mut c_void, enable: bool }
    unsafe impl Send for DesktopPinCtx {}

    unsafe extern "C" fn do_set_desktop_pin(ctx: *mut c_void) {
        let boxed = Box::from_raw(ctx as *mut DesktopPinCtx);
        let win = boxed.ns_window;
        if win.is_null() { return; }

        let level_sel    = sel_registerName(b"setLevel:\0".as_ptr() as _);
        let behavior_sel = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
        let constrain_sel = sel_registerName(b"constrainFrameRect:toScreen:\0".as_ptr() as _);
        let types = b"{CGRect={CGPoint=dd}{CGSize=dd}}@:{CGRect={CGPoint=dd}{CGSize=dd}}@\0"
            .as_ptr() as *const c_char;
        let imp_fn: unsafe extern "C" fn() = std::mem::transmute(custom_constrain as *const ());

        if boxed.enable {
            FLOATING_WIN_PTR.store(win as usize, Ordering::Relaxed);
            let cls = object_getClass(win);
            let added = class_addMethod(cls, constrain_sel, imp_fn, types);
            if !added {
                let m = class_getInstanceMethod(cls, constrain_sel);
                if !m.is_null() {
                    method_setImplementation(m, imp_fn);
                }
            }

            // 消除 macOS 窗口四角黑色直角系统阴影与背景
            let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
            msg_usize(win, shadow_sel, 0); // hasShadow: NO

            let opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as _);
            msg_usize(win, opaque_sel, 0); // isOpaque: NO

            let nscolor_cls = objc_getClass(b"NSColor\0".as_ptr() as _);
            if !nscolor_cls.is_null() {
                let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as _);
                let f_clear: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let clear_color = f_clear(nscolor_cls, clear_sel);
                if !clear_color.is_null() {
                    let bg_sel = sel_registerName(b"setBackgroundColor:\0".as_ptr() as _);
                    let f_bg: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    f_bg(win, bg_sel, clear_color);
                }
            }

            // 桌面小组件需要支持点击交互（按钮、卡片、切换尺寸、缩放手柄）。
            // 在 macOS 中，负数 Level（如 kCGDesktopWindowLevel = -2147483623）属于壁纸层，
            // 访达（Finder）的桌面全屏视图会截获所有鼠标事件，导致 Webview 完全无法接收点击。
            // 因此必须使用 Normal Level (0)，配合 orderBack: 将其置于普通应用窗口底层。
            msg_isize(win, level_sel, 0);

            // 1: canJoinAllSpaces (所有桌面空间可见)
            // 16: stationary (切换桌面时不移动)
            // 64: ignoresCycle (Cmd+Tab 轮巡不选中)
            // 256: fullScreenAuxiliary (全屏时作为辅助窗口)
            msg_usize(win, behavior_sel, 1 | 16 | 64 | 256);

            let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
            let f_inval: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_inval(win, inval_sel);

            // orderBack: 沉在底层壁纸上
            let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
            let f_order: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_order(win, order_back_sel, std::ptr::null_mut());
        } else {
            FLOATING_WIN_PTR.store(0, Ordering::Relaxed);

            // 恢复系统窗口正常阴影
            let shadow_sel = sel_registerName(b"setHasShadow:\0".as_ptr() as _);
            msg_usize(win, shadow_sel, 1); // hasShadow: YES

            let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
            let f_inval: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_inval(win, inval_sel);

            // Normal Window Level = 0
            msg_isize(win, level_sel, 0);
            msg_usize(win, behavior_sel, 0);

            // orderFront:
            let order_front_sel = sel_registerName(b"orderFront:\0".as_ptr() as _);
            let f_order: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_order(win, order_front_sel, std::ptr::null_mut());
        }
    }

    unsafe extern "C" fn do_order_back(ctx: *mut c_void) {
        let win = ctx;
        if win.is_null() { return; }
        let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
        let f_order: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        f_order(win, order_back_sel, std::ptr::null_mut());
    }

    pub fn schedule_order_back(ns_window: *mut c_void) {
        if ns_window.is_null() { return; }
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ns_window, do_order_back);
        }
    }

    pub fn schedule_set_desktop_pin(ns_window: *mut c_void, enable: bool) {
        if ns_window.is_null() { return; }
        let ctx = Box::into_raw(Box::new(DesktopPinCtx { ns_window, enable }));
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ctx as *mut c_void, do_set_desktop_pin);
        }
    }
}


#[derive(Clone, Serialize, Deserialize, Debug)]
struct GotifyMessage {
    id: u64,
    appid: u64,
    message: String,
    title: String,
    priority: u32,
    date: String,
    extras: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct ConnectionConfig {
    server_url: String,
    client_token: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct SoundConfig {
    sound_enabled: bool,
    min_priority: u32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        SoundConfig {
            sound_enabled: true,
            min_priority: 0,
        }
    }
}

fn load_sound_config_from_file() -> SoundConfig {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_sound_config.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<SoundConfig>(&content) {
            return config;
        }
    }
    SoundConfig::default()
}

#[tauri::command]
fn get_sound_config() -> SoundConfig {
    load_sound_config_from_file()
}

#[tauri::command]
fn save_sound_config(sound_enabled: bool, min_priority: u32) -> Result<(), String> {
    let config = SoundConfig {
        sound_enabled,
        min_priority,
    };
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_sound_config.json", config_dir);
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("保存声音设置失败: {}", e))
}


#[derive(Clone, Serialize, Deserialize, Debug)]
struct WindowPosition {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn default_widget_opacity() -> f64 {
    0.70
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WidgetConfig {
    pub is_widget_mode: bool,
    pub widget_x: Option<i32>,
    pub widget_y: Option<i32>,
    pub widget_width: u32,
    pub widget_height: u32,
    pub normal_x: Option<i32>,
    pub normal_y: Option<i32>,
    pub normal_width: u32,
    pub normal_height: u32,
    #[serde(default = "default_widget_opacity")]
    pub opacity: f64,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        WidgetConfig {
            is_widget_mode: false,
            widget_x: None,
            widget_y: None,
            widget_width: 320,
            widget_height: 220,
            normal_x: None,
            normal_y: None,
            normal_width: 380,
            normal_height: 600,
            opacity: 0.70,
        }
    }
}

fn load_widget_config_from_file() -> WidgetConfig {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_widget_config.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(mut config) = serde_json::from_str::<WidgetConfig>(&content) {
            // 防御性过滤：防止历史由于 Retina 缩放倍率混淆导致尺寸失控
            if config.widget_width < 200 || config.widget_width > 800 {
                config.widget_width = 340;
            }
            if config.widget_height < 140 || config.widget_height > 800 {
                config.widget_height = 240;
            }
            if config.normal_width < 320 || config.normal_width > 1000 {
                config.normal_width = 380;
            }
            if config.normal_height < 400 || config.normal_height > 1000 {
                config.normal_height = 600;
            }
            if let Some(x) = config.normal_x {
                if x > 4000 || x < -2000 { config.normal_x = None; }
            }
            if let Some(y) = config.normal_y {
                if y > 4000 || y < -2000 { config.normal_y = None; }
            }
            return config;
        }
    }
    WidgetConfig::default()
}

fn save_widget_config_to_file(config: &WidgetConfig) -> Result<(), String> {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_widget_config.json", config_dir);
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("保存小组件配置失败: {}", e))
}

struct AppState {
    config: Arc<Mutex<Option<ConnectionConfig>>>,
    ws_running: Arc<Mutex<bool>>,
    window_position: Arc<Mutex<Option<WindowPosition>>>,
    tray_icon: Arc<Mutex<Option<TrayIcon>>>,
    /// Windows 贴边防抖令牌：每次 Moved 事件自增，spawn 出去的 snap 任务
    /// 先等待后比对，令牌一致才执行，否则放弃（同一时刻只有最后一次生效）
    #[allow(dead_code)]
    snap_debounce_token: Arc<AtomicU64>,
}

// 获取跨平台的配置目录
fn get_config_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    }
}

#[tauri::command]
async fn save_config_to_file(
    state: State<'_, AppState>,
    server_url: String,
    client_token: String,
) -> Result<String, String> {
    let config = ConnectionConfig {
        server_url,
        client_token,
    };

    let mut state_config = state.config.lock().unwrap();
    *state_config = Some(config.clone());

    // 保存到文件
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_config.json", config_dir);

    match std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()) {
        Ok(_) => {
            println!("✅ 配置已保存到文件: {}", config_path);
            Ok("Configuration saved successfully".to_string())
        },
        Err(e) => {
            eprintln!("❌ 保存配置失败: {}", e);
            Err(format!("Failed to save config: {}", e))
        }
    }
}

#[tauri::command]
async fn test_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    println!("🧪 测试调用设置窗口...");
    show_settings_window(app).await
}

#[tauri::command]
async fn get_system_theme() -> Result<String, String> {
    println!("🎨 获取系统主题...");

    #[cfg(target_os = "macos")]
    {
        // macOS 主题检测
        use std::process::Command;

        let output = Command::new("defaults")
            .args(&["read", "-g", "AppleInterfaceStyle"])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                let theme = String::from_utf8_lossy(&result.stdout).trim().to_string();
                if theme == "Dark" {
                    println!("🌙 检测到 macOS 深色模式");
                    Ok("dark".to_string())
                } else {
                    println!("☀️ 检测到 macOS 浅色模式");
                    Ok("light".to_string())
                }
            }
            _ => {
                println!("☀️ macOS 主题检测失败或为浅色模式");
                Ok("light".to_string())
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows 主题检测 - 使用注册表
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key_result = hkcu.open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            KEY_READ
        );

        match key_result {
            Ok(key) => {
                match key.get_value::<u32, _>("AppsUseLightTheme") {
                    Ok(value) => {
                        if value == 0 {
                            println!("🌙 检测到 Windows 深色模式");
                            Ok("dark".to_string())
                        } else {
                            println!("☀️ 检测到 Windows 浅色模式");
                            Ok("light".to_string())
                        }
                    }
                    Err(_) => {
                        println!("⚠️ Windows 主题检测失败，使用默认浅色模式");
                        Ok("light".to_string())
                    }
                }
            }
            Err(_) => {
                println!("⚠️ Windows 注册表访问失败，使用默认浅色模式");
                Ok("light".to_string())
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux 等其他系统 - 简单的默认值
        println!("☀️ 非 macOS/Windows 系统，使用默认浅色模式");
        Ok("light".to_string())
    }
}

#[tauri::command]
async fn show_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    // 检查是否已有设置窗口
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    // 创建设置窗口
    let _settings_window = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into())
    )
    .title("Gotify 设置")
    .inner_size(800.0, 600.0)
    .min_inner_size(700.0, 500.0)
    .center()
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

    Ok(())
}

fn load_config_from_file() -> Option<ConnectionConfig> {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_config.json", config_dir);

    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str::<ConnectionConfig>(&content) {
                Ok(config) => {
                    println!("✅ 从文件加载配置成功: {}", config_path);
                    Some(config)
                },
                Err(e) => {
                    eprintln!("❌ 解析配置文件失败: {}", e);
                    // 如果配置文件解析失败，尝试从环境变量加载
                    load_config_from_env()
                }
            }
        },
        Err(_) => {
            println!("ℹ️ 未找到配置文件，尝试从环境变量加载");
            load_config_from_env()
        }
    }
}

fn load_config_from_env() -> Option<ConnectionConfig> {
    let server_url = std::env::var("GOTIFY_SERVER_URL")
        .or_else(|_| std::env::var("GOTIFY_URL"))
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    let client_token = std::env::var("GOTIFY_CLIENT_TOKEN")
        .or_else(|_| std::env::var("GOTIFY_TOKEN"))
        .unwrap_or_else(|_| "".to_string());

    // 只有当Token不为空时才返回配置
    if !client_token.is_empty() {
        println!("✅ 从环境变量加载配置成功");
        Some(ConnectionConfig {
            server_url,
            client_token,
        })
    } else {
        println!("ℹ️ 未找到有效的配置，请通过设置界面配置");
        None
    }
}

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<Option<ConnectionConfig>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.clone())
}

#[tauri::command]
async fn save_config(
    state: State<'_, AppState>,
    server_url: String,
    client_token: String,
) -> Result<String, String> {
    save_config_to_file(state, server_url, client_token).await
}

#[tauri::command]
async fn test_connection(
    server_url: String,
    client_token: String,
) -> Result<String, String> {
    // 构建 WebSocket URL
    let ws_url = {
        let server_url = server_url.trim_end_matches('/');
        let protocol = if server_url.starts_with("https://") {
            "wss://"
        } else {
            "ws://"
        };
        let host = server_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        format!("{}{}/stream?token={}", protocol, host, client_token)
    };

    println!("🧪 测试连接到: {}", ws_url);

    // 尝试解析 URL
    let url = url::Url::parse(&ws_url)
        .map_err(|e| format!("URL 解析失败: {}", e))?;

    // 尝试连接 WebSocket
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        connect_async(url)
    ).await {
        Ok(Ok((ws_stream, _))) => {
            println!("✅ WebSocket 连接测试成功");
            drop(ws_stream); // 立即关闭测试连接
            Ok("连接成功".to_string())
        }
        Ok(Err(e)) => {
            eprintln!("❌ WebSocket 连接失败: {}", e);
            let err_str = e.to_string();
            // 将常见系统网络错误翻译成友好提示
            let friendly = if err_str.contains("os error 65") || err_str.contains("No route to host") {
                "网络不可达（No route to host）。请检查：\n① 服务器是否开机/在线\n② 本机与服务器的网络是否连通".to_string()
            } else if err_str.contains("os error 61") || err_str.contains("Connection refused") {
                "连接被拒绝（Connection refused）。请检查：\n① Gotify 服务器进程是否运行\n② 端口号是否正确".to_string()
            } else if err_str.contains("os error 60") || err_str.contains("Operation timed out") {
                "连接超时，服务器未响应。请检查服务器地址是否可访问".to_string()
            } else if err_str.contains("os error 50") || err_str.contains("Network is down") {
                "本机网络已断开，请检查 Wi-Fi 或网线连接".to_string()
            } else if err_str.contains("401") || err_str.contains("Unauthorized") {
                "认证失败（401）。请检查客户端 Token 是否正确".to_string()
            } else if err_str.contains("DNS") || err_str.contains("failed to lookup") {
                "DNS 解析失败，无法找到服务器。请检查服务器地址".to_string()
            } else {
                format!("连接失败: {}。\n请检查服务器地址和 Token 是否正确", e)
            };
            Err(friendly)
        }
        Err(_) => {
            eprintln!("❌ 连接超时");
            Err("连接超时（10秒），服务器未响应。请检查服务器地址是否可访问".to_string())
        }
    }
}

#[tauri::command]
async fn start_websocket(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = {
        let cfg = state.config.lock().unwrap();
        cfg.clone()
    };

    let config = config.ok_or("No configuration found. Please configure first.")?;
    
    let mut ws_running = state.ws_running.lock().unwrap();
    if *ws_running {
        return Err("WebSocket is already running".to_string());
    }
    *ws_running = true;
    drop(ws_running);

    let ws_running_clone = state.ws_running.clone();
    
    tokio::spawn(async move {
        if let Err(e) = run_websocket(app, config, ws_running_clone).await {
            eprintln!("WebSocket error: {}", e);
        }
    });

    Ok("WebSocket connection started".to_string())
}

#[tauri::command]
async fn stop_websocket(state: State<'_, AppState>) -> Result<String, String> {
    let mut ws_running = state.ws_running.lock().unwrap();
    *ws_running = false;
    Ok("WebSocket connection stopped".to_string())
}

#[tauri::command]
async fn is_websocket_running(state: State<'_, AppState>) -> Result<bool, String> {
    let ws_running = state.ws_running.lock().unwrap();
    Ok(*ws_running)
}

#[tauri::command]
async fn snap_window_to_edge(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    edge: String,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("无法获取主窗口")?;
    let size = window.outer_size().map_err(|e| format!("获取大小失败: {}", e))?;
    let mut position = window.outer_position().map_err(|e| format!("获取位置失败: {}", e))?;

    if let Ok(Some(monitor)) = window.current_monitor() {
        let mon_pos = monitor.position();
        let mon_size = monitor.size();
        let scale = monitor.scale_factor();
        let border_offset = (7.0 * scale).round() as i32;

        let mon_left = mon_pos.x;
        let mon_right = mon_pos.x + mon_size.width as i32;
        let win_w = size.width as i32;

        match edge.as_str() {
            "left" => {
                position.x = mon_left - border_offset;
            }
            "right" => {
                position.x = mon_right - win_w + border_offset;
            }
            _ => {}
        }
        let _ = window.set_position(tauri::Position::Physical(position));
        let _ = save_window_position_auto(app.clone(), state).await;
    }
    Ok(())
}

#[tauri::command]
async fn save_window_position_auto(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 小组件模式下有单独的 widget 配置，不覆盖主窗口的常规位置
    let widget_cfg = load_widget_config_from_file();
    if widget_cfg.is_widget_mode {
        return Ok(());
    }

    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => return Ok(()),
    };
    
    // 统一使用 outer_position 获取左上角坐标，outer_size 获取完整窗口大小
    #[allow(unused_mut)]
    let mut position = window.outer_position().map_err(|e| format!("获取位置失败: {}", e))?;
    let size = window.outer_size().map_err(|e| format!("获取大小失败: {}", e))?;

    #[cfg(target_os = "windows")]
    {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let mon_pos = monitor.position();
            let mon_size = monitor.size();
            let scale = monitor.scale_factor();
            let border_offset = (7.0 * scale).round() as i32;
            let snap_threshold = (32.0 * scale).round() as i32;

            let mon_left = mon_pos.x;
            let mon_right = mon_pos.x + mon_size.width as i32;
            let mon_top = mon_pos.y;
            let mon_bottom = mon_pos.y + mon_size.height as i32;

            let win_w = size.width as i32;
            let win_h = size.height as i32;

            let mut snapped = false;

            // 1. 左贴边磁性吸附 (检测窗口可视左边缘是否靠近屏幕左侧)
            let left_dist = ((position.x + border_offset) - mon_left).abs();
            if left_dist <= snap_threshold {
                position.x = mon_left - border_offset;
                snapped = true;
            } else {
                // 2. 右贴边磁性吸附 (检测窗口可视右边缘是否靠近屏幕右侧)
                let right_dist = ((position.x + win_w - border_offset) - mon_right).abs();
                if right_dist <= snap_threshold {
                    position.x = mon_right - win_w + border_offset;
                    snapped = true;
                }
            }

            // 3. 顶贴边磁性吸附
            let top_dist = (position.y - mon_top).abs();
            if top_dist <= snap_threshold {
                position.y = mon_top;
                snapped = true;
            } else {
                // 4. 底贴边磁性吸附
                let bottom_dist = ((position.y + win_h - border_offset) - mon_bottom).abs();
                if bottom_dist <= snap_threshold {
                    position.y = mon_bottom - win_h + border_offset;
                    snapped = true;
                }
            }

            if snapped {
                println!("🧲 Windows 贴边磁性吸附生效: x={}, y={}", position.x, position.y);
                let _ = window.set_position(tauri::Position::Physical(position));
            }
        }
    }
    
    let window_pos = WindowPosition {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    
    let mut window_position = state.window_position.lock().unwrap();
    *window_position = Some(window_pos.clone());
    
    // 保存到文件
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_window_position.json", config_dir);
    
    match std::fs::write(&config_path, serde_json::to_string_pretty(&window_pos).unwrap()) {
        Ok(_) => {
            println!("✅ 窗口位置已自动保存: x={}, y={}, w={}, h={}", window_pos.x, window_pos.y, window_pos.width, window_pos.height);
            Ok(())
        },
        Err(e) => {
            eprintln!("❌ 保存窗口位置失败: {}", e);
            Err(format!("保存失败: {}", e))
        }
    }
}

#[tauri::command]
async fn save_window_position(
    state: State<'_, AppState>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let position = WindowPosition { x, y, width, height };
    let mut window_position = state.window_position.lock().unwrap();
    *window_position = Some(position.clone());
    
    // 保存到文件
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_window_position.json", config_dir);
    
    match std::fs::write(&config_path, serde_json::to_string_pretty(&position).unwrap()) {
        Ok(_) => {
            println!("✅ 窗口位置已保存: x={}, y={}, w={}, h={}", x, y, width, height);
            Ok(())
        },
        Err(e) => {
            eprintln!("❌ 保存窗口位置失败: {}", e);
            Err(format!("保存失败: {}", e))
        }
    }
}

#[tauri::command]
async fn load_window_position(state: State<'_, AppState>) -> Result<Option<WindowPosition>, String> {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_window_position.json", config_dir);
    
    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str::<WindowPosition>(&content) {
                Ok(position) => {
                    let mut window_position = state.window_position.lock().unwrap();
                    *window_position = Some(position.clone());
                    println!("✅ 窗口位置已加载: x={}, y={}, w={}, h={}", position.x, position.y, position.width, position.height);
                    Ok(Some(position))
                },
                Err(e) => {
                    eprintln!("❌ 解析窗口位置失败: {}", e);
                    Ok(None)
                }
            }
        },
        Err(_) => {
            println!("ℹ️ 未找到保存的窗口位置");
            Ok(None)
        }
    }
}



async fn run_websocket(
    app: tauri::AppHandle,
    config: ConnectionConfig,
    ws_running: Arc<Mutex<bool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 构建 WebSocket URL
    let ws_url = {
        let server_url = config.server_url.trim_end_matches('/');
        let protocol = if server_url.starts_with("https://") {
            "wss://"
        } else {
            "ws://"
        };
        let host = server_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        format!("{}{}/stream?token={}", protocol, host, config.client_token)
    };

    println!("Connecting to WebSocket: {}", ws_url);
    let url = url::Url::parse(&ws_url)?;

    let mut reconnect_attempt = 0u32;

    while *ws_running.lock().unwrap() {
        if reconnect_attempt > 0 {
            println!("🔄 尝试重连中 (第 {} 次)...", reconnect_attempt);
            let _ = app.emit("websocket-status", "reconnecting");
        } else {
            println!("🔌 正在建立 WebSocket 连接...");
            let _ = app.emit("websocket-status", "connecting");
        }

        match connect_async(url.clone()).await {
            Ok((ws_stream, _)) => {
                println!("✅ WebSocket 连接建立成功");
                reconnect_attempt = 0;
                let _ = app.emit("websocket-status", "connected");

                let (mut write, mut read) = ws_stream.split();
                let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(15));
                ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    if !*ws_running.lock().unwrap() {
                        println!("⏹️ 收到停止 WebSocket 指令");
                        break;
                    }

                    tokio::select! {
                        _ = ping_interval.tick() => {
                            if let Err(e) = write.send(Message::Ping(vec![])).await {
                                eprintln!("⚠️ 发送 Ping 失败: {}, 准备重连", e);
                                break;
                            }
                        }
                        msg = tokio::time::timeout(std::time::Duration::from_secs(45), read.next()) => {
                            match msg {
                                Ok(Some(Ok(Message::Text(text)))) => {
                                    println!("Received message: {}", text);

                                    // 解析 Gotify 消息
                                    if let Ok(gotify_msg) = serde_json::from_str::<GotifyMessage>(&text) {
                                        println!("📨 收到 Gotify 消息: {} (priority={})", gotify_msg.title, gotify_msg.priority);

                                        let sound_cfg = load_sound_config_from_file();
                                        let should_sound = sound_cfg.sound_enabled && gotify_msg.priority >= sound_cfg.min_priority;

                                        // 发送 macOS / 系统通知
                                        use tauri_plugin_notification::NotificationExt;

                                        let mut notification = app.notification().builder();
                                        notification = notification
                                            .title(&gotify_msg.title)
                                            .body(&gotify_msg.message);

                                        if should_sound {
                                            println!("🔔 为通知添加默认系统声音");
                                            notification = notification.sound("default");
                                        }

                                        match notification.show() {
                                            Ok(_) => println!("✅ 系统通知已生成"),
                                            Err(e) => eprintln!("❌ 发送通知失败: {:?}", e),
                                        }

                                        // 发送消息到前端主窗口
                                        if let Some(window) = app.get_webview_window("main") {
                                            let mut payload = serde_json::to_value(&gotify_msg).unwrap_or_default();
                                            if let Some(obj) = payload.as_object_mut() {
                                                obj.insert("should_play_sound".to_string(), serde_json::Value::Bool(should_sound));
                                            }
                                            match window.emit("gotify-message", &payload) {
                                                Ok(_) => println!("✅ 消息已推送到前端"),
                                                Err(e) => eprintln!("❌ 推送失败: {}", e),
                                            }
                                        } else {
                                            eprintln!("❌ 无法找到主窗口");
                                        }
                                    }
                                }
                                Ok(Some(Ok(Message::Ping(_)))) => {
                                    // 自动回复 pong（tokio-tungstenite 内部处理或此标记）
                                }
                                Ok(Some(Ok(Message::Pong(_)))) => {
                                    // 收到服务器 pong
                                }
                                Ok(Some(Ok(Message::Close(_)))) => {
                                    println!("⚠️ WebSocket 服务器主动关闭连接");
                                    break;
                                }
                                Ok(Some(Err(e))) => {
                                    eprintln!("⚠️ WebSocket 读取异常: {}", e);
                                    break;
                                }
                                Ok(None) => {
                                    println!("⚠️ WebSocket 数据流结束");
                                    break;
                                }
                                Err(_) => {
                                    eprintln!("⚠️ 45 秒内未收到响应或心跳，判定连接断开");
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ WebSocket 连接尝试失败: {}", e);
                let err_str = e.to_string();
                let friendly = if err_str.contains("os error 65") || err_str.contains("No route to host") {
                    "网络不可达，服务器无法连接".to_string()
                } else if err_str.contains("os error 61") || err_str.contains("Connection refused") {
                    "连接被拒绝，请检查服务器是否运行".to_string()
                } else if err_str.contains("os error 50") || err_str.contains("Network is down") {
                    "本机网络已断开".to_string()
                } else if err_str.contains("401") || err_str.contains("Unauthorized") {
                    "Token 认证失败，请检查客户端 Token".to_string()
                } else {
                    format!("连接失败: {}", e)
                };
                let _ = app.emit("websocket-error", &friendly);
            }
        }

        // 如果用户主动点击了停止连接，则跳出重连循环
        if !*ws_running.lock().unwrap() {
            break;
        }

        reconnect_attempt += 1;
        // 指数退避重连等待时间：2s, 4s, 8s, 16s, 最大 30s
        let delay_secs = std::cmp::min(1u64 << reconnect_attempt.min(5), 30);
        let _ = app.emit("websocket-status", "reconnecting");
        println!("⏳ {} 秒后尝试重新连接...", delay_secs);
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
    }

    let _ = app.emit("websocket-status", "disconnected");
    let mut running = ws_running.lock().unwrap();
    *running = false;
    println!("🛑 WebSocket 异步运行任务已终止");

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            config: Arc::new(Mutex::new(load_config_from_file())),
            ws_running: Arc::new(Mutex::new(false)),
            window_position: Arc::new(Mutex::new(None)),
            tray_icon: Arc::new(Mutex::new(None)),
            snap_debounce_token: Arc::new(AtomicU64::new(0)),
        })
        .setup(|app| {
            // macOS: 根据用户设置决定是否显示 Dock 图标
            #[cfg(target_os = "macos")]
            {
                let config_dir = get_config_dir();
                let dock_path = format!("{}/.gotify_show_in_dock.json", config_dir);
                let show_in_dock = std::fs::read_to_string(&dock_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<bool>(&c).ok())
                    .unwrap_or(false);

                let policy = if show_in_dock {
                    tauri::ActivationPolicy::Regular
                } else {
                    tauri::ActivationPolicy::Accessory
                };
                app.set_activation_policy(policy);
            }

            let app_handle = app.handle().clone();
            let main_window = app.get_webview_window("main").unwrap();

            // Windows: 根据用户设置决定是否在任务栏显示图标
            #[cfg(target_os = "windows")]
            {
                let config_dir = get_config_dir();
                let dock_path = format!("{}/.gotify_show_in_dock.json", config_dir);
                let show_in_dock = std::fs::read_to_string(&dock_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<bool>(&c).ok())
                    .unwrap_or(true);
                if !show_in_dock {
                    let _ = main_window.set_skip_taskbar(true);
                }
            }
            
            // 恢复窗口位置
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<AppState> = app_handle.state();
                
                // 加载保存的窗口位置
                let config_dir = get_config_dir();
                let config_path = format!("{}/.gotify_window_position.json", config_dir);
                
                // 确保窗口可见且未最小化（Windows 兼容性）
                let _ = main_window.unminimize();
                let _ = main_window.show();
                let _ = main_window.set_focus();
                
                let mut should_restore = false;
                
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(position) = serde_json::from_str::<WindowPosition>(&content) {
                        // 验证位置有效性：不能是屏幕外的极端坐标
                        let is_valid = position.x > -10000 && position.y > -10000 && 
                                      position.width > 0 && position.height > 0 &&
                                      position.width < 10000 && position.height < 10000;
                        
                        if is_valid {
                            println!("🪟 恢复窗口位置: x={}, y={}, w={}, h={}", position.x, position.y, position.width, position.height);
                            
                            // 设置窗口位置和大小 - 统一使用外部坐标
                            let _ = main_window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                                x: position.x,
                                y: position.y,
                            }));
                            let _ = main_window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                                width: position.width,
                                height: position.height,
                            }));
                            
                            // 更新状态
                            let mut window_position = state.window_position.lock().unwrap();
                            *window_position = Some(position);
                            should_restore = true;
                        } else {
                            println!("⚠️ 保存的窗口位置无效（x={}, y={}, w={}, h={}），将重置", position.x, position.y, position.width, position.height);
                            // 删除无效的保存文件
                            let _ = std::fs::remove_file(&config_path);
                        }
                    }
                } else {
                    println!("ℹ️ 未找到保存的窗口位置，使用默认位置");
                }
                
                // 如果没有成功恢复位置，则居中显示
                if !should_restore {
                    let _ = main_window.center();
                }

                // 恢复窗口置顶状态
                let always_on_top_path = format!("{}/.gotify_always_on_top.json", config_dir);
                if let Ok(content) = std::fs::read_to_string(&always_on_top_path) {
                    if let Ok(always_on_top) = serde_json::from_str::<bool>(&content) {
                        if always_on_top {
                            println!("📌 恢复窗口置顶状态: true");
                            let _ = main_window.set_always_on_top(true);
                            // macOS: 必须在主线程调用 Cocoa API
                            #[cfg(target_os = "macos")]
                            {
                                if let Ok(ns_win_ptr) = main_window.ns_window() {
                                    macos_window::schedule_set_floating(
                                        ns_win_ptr as *mut std::ffi::c_void,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }

                // 检查并恢复桌面小组件模式
                let widget_config = load_widget_config_from_file();
                if widget_config.is_widget_mode {
                    println!("🖥️ 启动恢复桌面小组件模式: true");
                    let _ = set_widget_mode(app_handle.clone(), true).await;
                }

                // 等待 1 秒后自动连接
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                let config = {
                    let cfg = state.config.lock().unwrap();
                    cfg.clone()
                };

                if let Some(config) = config {
                    println!("🚀 自动连接到 Gotify 服务器...");

                    let mut ws_running = state.ws_running.lock().unwrap();
                    if !*ws_running {
                        *ws_running = true;
                        drop(ws_running);

                        let ws_running_clone = state.ws_running.clone();
                        tokio::spawn(async move {
                            if let Err(e) = run_websocket(app_handle, config, ws_running_clone).await {
                                eprintln!("WebSocket error: {}", e);
                            }
                        });
                    }
                }
            });

            // 创建系统托盘
            use tauri::{menu::{Menu, MenuItem}};

            println!("🔧 开始创建系统托盘...");
            let toggle_item = MenuItem::with_id(app, "toggle", "显示通知", true, None::<&str>)?;
            let widget_item = MenuItem::with_id(app, "widget", "🖥️ 切换桌面小组件", true, None::<&str>)?;
            let pin_item = MenuItem::with_id(app, "pin", "窗口置顶", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &widget_item, &pin_item, &settings_item, &quit_item])?;

            // 加载托盘图标 - macOS 使用单色 Template，Windows 使用彩色图标
            #[cfg(target_os = "macos")]
            let icon_png = include_bytes!("../icons/tray-icon.png");
            #[cfg(not(target_os = "macos"))]
            let icon_png = include_bytes!("../icons/32x32.png");

            let icon_image = image::load_from_memory(icon_png)
                .map_err(|e| format!("Failed to decode tray icon: {}", e))?;
            let (icon_width, icon_height) = icon_image.dimensions();
            let icon_rgba = icon_image.into_rgba8().into_raw();
            let icon = tauri::image::Image::new_owned(icon_rgba, icon_width, icon_height);
            println!("✅ 托盘图标加载成功");

            let is_macos = cfg!(target_os = "macos");

            let tray = TrayIconBuilder::new()
                .icon(icon)
                .icon_as_template(is_macos)
                .menu(&tray_menu)
                .show_menu_on_left_click(is_macos)
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::{TrayIconEvent, MouseButton, MouseButtonState};
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
                                if window.is_focused().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.set_focus();
                                }
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "toggle" => {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.unminimize();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        "widget" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = toggle_widget_mode(app_handle).await;
                            });
                        }
                        "pin" => {
                            let _ = toggle_window_always_on_top(app.clone());
                        }
                        "settings" => {
                            println!("🖱️ 用户点击了设置菜单");
                            let _ = app.emit("show-settings", ());
                        }
                        "quit" => {
                            println!("🚪 用户点击了退出菜单");
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            println!("✅ 系统托盘创建成功");

            // 保存tray到状态
            let state: tauri::State<AppState> = app.state();
            let mut tray_icon = state.tray_icon.lock().unwrap();
            *tray_icon = Some(tray);

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Windows & 主窗口：点击右上角关闭按钮时，隐藏到托盘而不是直接退出应用
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
            tauri::WindowEvent::Focused(focused) => {
                // macOS: 小组件失去焦点时，自动沉到所有普通窗口下方，不遮挡正在操作的应用程序
                if window.label() == "main" && !focused {
                    let widget_cfg = load_widget_config_from_file();
                    if widget_cfg.is_widget_mode {
                        #[cfg(target_os = "macos")]
                        {
                            if let Ok(ns_win) = window.ns_window() {
                                macos_window::schedule_order_back(ns_win as *mut std::ffi::c_void);
                            }
                        }
                    }
                }
            }
            tauri::WindowEvent::Moved(pos) => {
                if window.label() == "main" {
                    let widget_cfg = load_widget_config_from_file();
                    if widget_cfg.is_widget_mode {
                        let mut cfg = widget_cfg;
                        cfg.widget_x = Some(pos.x);
                        cfg.widget_y = Some(pos.y);
                        let _ = save_widget_config_to_file(&cfg);
                    }
                }
                // Windows 拖动贴边：在 Moved 事件停止后 200ms 才执行磁性吸附
                // 用原子令牌实现轻量级防抖，避免拖动过程中窗口跳动
                #[cfg(target_os = "windows")]
                if window.label() == "main" {
                    let state: tauri::State<AppState> = window.state();
                    let token_arc = state.snap_debounce_token.clone();
                    // 自增令牌，旧的 spawn 任务检测到令牌变化后会自动放弃
                    let current_token = token_arc.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        // 如果令牌未变（没有新的移动事件），才执行贴边
                        if token_arc.load(AtomicOrdering::SeqCst) == current_token {
                            let state: tauri::State<AppState> = app.state();
                            let _ = save_window_position_auto(app.clone(), state).await;
                        }
                    });
                }
            }
            tauri::WindowEvent::Resized(size) => {
                if window.label() == "main" {
                    let factor = window.scale_factor().unwrap_or(1.0);
                    let logical_w = ((size.width as f64) / factor).round() as u32;
                    let logical_h = ((size.height as f64) / factor).round() as u32;

                    let widget_cfg = load_widget_config_from_file();
                    if widget_cfg.is_widget_mode {
                        if logical_w >= 200 && logical_w <= 800 && logical_h >= 140 && logical_h <= 800 {
                            let mut cfg = widget_cfg;
                            cfg.widget_width = logical_w;
                            cfg.widget_height = logical_h;
                            let _ = save_widget_config_to_file(&cfg);
                        }
                    } else {
                        if logical_w >= 320 && logical_w <= 1000 && logical_h >= 400 && logical_h <= 1000 {
                            let mut cfg = widget_cfg;
                            cfg.normal_width = logical_w;
                            cfg.normal_height = logical_h;
                            let _ = save_widget_config_to_file(&cfg);
                        }
                    }
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                println!("🎨 系统主题已更改: {:?}", theme);

                // 发送主题变化事件到前端
                let theme_str = match theme {
                    tauri::Theme::Light => "light",
                    tauri::Theme::Dark => "dark",
                    _ => "light", // 默认浅色主题
                };

                let _ = window.emit("tauri://theme-changed", serde_json::json!({
                    "theme": theme_str
                }));
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            save_config,
            save_config_to_file,
            get_config,
            test_connection,
            start_websocket,
            stop_websocket,
            is_websocket_running,
            save_window_position,
            save_window_position_auto,
            load_window_position,
            show_settings_window,
            test_settings_window,
            get_system_theme,
            get_auto_launch,
            set_auto_launch,
            get_show_in_dock,
            set_show_in_dock,
            get_sound_config,
            save_sound_config,
            get_window_always_on_top,
            set_window_always_on_top,
            toggle_window_always_on_top,
            start_dragging_window,
            snap_window_to_edge,
            get_widget_mode,
            get_widget_config,
            save_widget_position,
            set_widget_size,
            set_widget_opacity,
            set_widget_mode,
            toggle_widget_mode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn get_auto_launch() -> bool {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::*;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
            if key.get_value::<String, _>("GotifyNotifier").is_ok() {
                return true;
            }
        }
    }

    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_auto_launch.json", config_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str::<bool>(&c).ok())
        .unwrap_or(false)
}

#[tauri::command]
fn set_auto_launch(enabled: bool) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_auto_launch.json", config_dir);
    std::fs::write(&path, serde_json::to_string_pretty(&enabled).unwrap())
        .map_err(|e| format!("保存开机启动设置失败: {}", e))?;

    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| format!("获取应用路径失败: {}", e))?;
        let bundle = exe
            .ancestors()
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
            .unwrap_or(&exe);

        let bundle_path = bundle.display().to_string();

        if enabled {
            // 通过 AppleScript 添加到登录项（显示在系统设置的登录项中）
            let script = format!(
                "tell application \"System Events\" to make login item at end with properties {{path:\"{}\", hidden:false}}",
                bundle_path
            );
            let result = std::process::Command::new("osascript")
                .args(&["-e", &script])
                .output()
                .map_err(|e| format!("添加开机启动失败: {}", e))?;
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                eprintln!("⚠️ 添加登录项结果: {}", stderr);
            } else {
                println!("✅ 已添加登录项: {}", bundle_path);
            }

            // 额外创建 launchd plist 作为备份（部分 macOS 版本需要）
            let label = "com.gotify.notifier";
            let plist_path = format!(
                "{}/Library/LaunchAgents/{}.plist",
                std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
                label
            );
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>Program</key>
    <string>{}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
                label,
                bundle_path
            );
            let _ = std::fs::write(&plist_path, &plist);
        } else {
            // 从登录项移除
            let name = bundle
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let result = std::process::Command::new("osascript")
                .args(&["-e", &format!(
                    "tell application \"System Events\" to delete login item \"{}\"",
                    name
                )])
                .output()
                .map_err(|e| format!("移除开机启动失败: {}", e))?;
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                println!("ℹ️ 删除登录项（可能不存在）: {}", stderr);
            } else {
                println!("✅ 已移除登录项: {}", name);
            }

            // 同时清理 launchd plist
            let label = "com.gotify.notifier";
            let plist_path = format!(
                "{}/Library/LaunchAgents/{}.plist",
                std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
                label
            );
            let _ = std::process::Command::new("launchctl")
                .args(&["unload", &plist_path])
                .output();
            let _ = std::fs::remove_file(&plist_path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::*;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        let (key, _) = hkcu.create_subkey(key_path)
            .map_err(|e| format!("打开 Windows 注册表失败: {}", e))?;

        let app_name = "GotifyNotifier";
        if enabled {
            let exe = std::env::current_exe().map_err(|e| format!("获取当前程序路径失败: {}", e))?;
            let exe_str = exe.to_string_lossy().to_string();
            let cmd = format!("\"{}\"", exe_str);
            key.set_value(app_name, &cmd)
                .map_err(|e| format!("写入注册表开机启动失败: {}", e))?;
            println!("✅ 已写入 Windows 开机启动注册表: {}", cmd);
        } else {
            let _ = key.delete_value(app_name);
            println!("✅ 已从 Windows 开机启动注册表移除: {}", app_name);
        }
    }

    Ok(())
}

#[tauri::command]
fn get_show_in_dock() -> bool {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_show_in_dock.json", config_dir);
    let default_val = cfg!(target_os = "windows");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str::<bool>(&c).ok())
        .unwrap_or(default_val)
}

#[tauri::command]
fn set_show_in_dock(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_show_in_dock.json", config_dir);
    std::fs::write(&path, serde_json::to_string_pretty(&enabled).unwrap())
        .map_err(|e| format!("保存任务栏/Dock 设置失败: {}", e))?;

    // 运行时切换 Dock 显示 (macOS)
    #[cfg(target_os = "macos")]
    {
        let policy = if enabled {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }

    // 运行时切换任务栏图标显示 (Windows)
    #[cfg(target_os = "windows")]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_skip_taskbar(!enabled);
        }
    }

    Ok(())
}

#[tauri::command]
fn get_window_always_on_top(app: tauri::AppHandle) -> bool {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_always_on_top.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<bool>(&content) {
            return val;
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        return window.is_always_on_top().unwrap_or(false);
    }
    false
}

#[tauri::command]
fn set_window_always_on_top(app: tauri::AppHandle, always_on_top: bool) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_always_on_top(always_on_top).map_err(|e| format!("设置窗口置顶失败: {}", e))?;

        // macOS: override constrainFrameRect so window can be placed at x=0
        // Must be called on the main thread via dispatch_async_f
        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win_ptr) = window.ns_window() {
                macos_window::schedule_set_floating(
                    ns_win_ptr as *mut std::ffi::c_void,
                    always_on_top,
                );
            }
        }
    }
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_always_on_top.json", config_dir);
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&always_on_top).unwrap());
    
    // 广播事件通知前端更新状态
    let _ = app.emit("tauri://always-on-top-changed", serde_json::json!({ "always_on_top": always_on_top }));
    
    println!("📌 窗口置顶状态已设置: {}", always_on_top);
    Ok(always_on_top)
}

#[tauri::command]
fn toggle_window_always_on_top(app: tauri::AppHandle) -> Result<bool, String> {
    let current = if let Some(window) = app.get_webview_window("main") {
        window.is_always_on_top().unwrap_or(false)
    } else {
        get_window_always_on_top(app.clone())
    };
    let new_state = !current;
    set_window_always_on_top(app, new_state)
}

#[tauri::command]
fn start_dragging_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.start_dragging().map_err(|e| format!("拖动窗口失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn get_widget_mode() -> bool {
    load_widget_config_from_file().is_widget_mode
}

#[tauri::command]
fn get_widget_config() -> WidgetConfig {
    load_widget_config_from_file()
}

#[tauri::command]
async fn save_widget_position(x: i32, y: i32) -> Result<(), String> {
    let mut config = load_widget_config_from_file();
    config.widget_x = Some(x);
    config.widget_y = Some(y);
    save_widget_config_to_file(&config)
}

#[tauri::command]
async fn set_widget_mode(app: tauri::AppHandle, enable: bool) -> Result<bool, String> {
    let mut config = load_widget_config_from_file();
    let window = app.get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_string())?;

    let factor = window.scale_factor().unwrap_or(1.0);

    if enable {
        if !config.is_widget_mode {
            if let Ok(pos) = window.outer_position() {
                config.normal_x = Some(pos.x);
                config.normal_y = Some(pos.y);
            }
            if let Ok(size) = window.inner_size() {
                let w = ((size.width as f64) / factor).round() as u32;
                let h = ((size.height as f64) / factor).round() as u32;
                config.normal_width = w.clamp(320, 1000);
                config.normal_height = h.clamp(400, 1000);
            }
        }
        config.is_widget_mode = true;
        let _ = save_widget_config_to_file(&config);

        let _ = window.set_decorations(false);
        let _ = window.set_always_on_top(false);

        let target_w = if config.widget_width >= 200 && config.widget_width <= 800 {
            config.widget_width
        } else {
            340
        };
        let target_h = if config.widget_height >= 140 && config.widget_height <= 800 {
            config.widget_height
        } else {
            240
        };
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: target_w as f64,
            height: target_h as f64,
        }));

        if let (Some(wx), Some(wy)) = (config.widget_x, config.widget_y) {
            let safe_x = if wx > 10000 || wx < -2000 { 100 } else { wx };
            let safe_y = if wy > 10000 || wy < -2000 { 100 } else { wy };
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: safe_x,
                y: safe_y,
            }));
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win_ptr) = window.ns_window() {
                macos_window::schedule_set_desktop_pin(
                    ns_win_ptr as *mut std::ffi::c_void,
                    true,
                );
            }
        }
    } else {
        if config.is_widget_mode {
            if let Ok(pos) = window.outer_position() {
                config.widget_x = Some(pos.x);
                config.widget_y = Some(pos.y);
            }
            if let Ok(size) = window.inner_size() {
                let w = ((size.width as f64) / factor).round() as u32;
                let h = ((size.height as f64) / factor).round() as u32;
                config.widget_width = w.clamp(200, 800);
                config.widget_height = h.clamp(140, 800);
            }
        }
        config.is_widget_mode = false;
        let _ = save_widget_config_to_file(&config);

        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win_ptr) = window.ns_window() {
                macos_window::schedule_set_desktop_pin(
                    ns_win_ptr as *mut std::ffi::c_void,
                    false,
                );
            }
        }

        let _ = window.set_decorations(true);

        let target_w = if config.normal_width >= 320 && config.normal_width <= 1000 {
            config.normal_width
        } else {
            380
        };
        let target_h = if config.normal_height >= 400 && config.normal_height <= 1000 {
            config.normal_height
        } else {
            600
        };
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: target_w as f64,
            height: target_h as f64,
        }));

        if let (Some(nx), Some(ny)) = (config.normal_x, config.normal_y) {
            let safe_x = if nx > 10000 || nx < -2000 { 100 } else { nx };
            let safe_y = if ny > 10000 || ny < -2000 { 100 } else { ny };
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: safe_x,
                y: safe_y,
            }));
        }

        let always_on_top = get_window_always_on_top(app.clone());
        if always_on_top {
            let _ = window.set_always_on_top(true);
            #[cfg(target_os = "macos")]
            {
                if let Ok(ns_win_ptr) = window.ns_window() {
                    macos_window::schedule_set_floating(
                        ns_win_ptr as *mut std::ffi::c_void,
                        true,
                    );
                }
            }
        }

        let _ = window.show();
        let _ = window.set_focus();
    }

    let _ = app.emit("tauri://widget-mode-changed", serde_json::json!({ "is_widget_mode": enable }));
    println!("🖥️ 桌面小组件模式已切换: {}", enable);
    Ok(enable)
}

#[tauri::command]
async fn toggle_widget_mode(app: tauri::AppHandle) -> Result<bool, String> {
    let current = get_widget_mode();
    set_widget_mode(app, !current).await
}

#[tauri::command]
async fn set_widget_size(app: tauri::AppHandle, width: u32, height: u32) -> Result<WidgetConfig, String> {
    let w = width.clamp(240, 800);
    let h = height.clamp(140, 800);

    let window = app.get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_string())?;

    window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: w as f64,
        height: h as f64,
    })).map_err(|e| format!("设置尺寸失败: {}", e))?;

    let mut config = load_widget_config_from_file();
    config.widget_width = w;
    config.widget_height = h;
    save_widget_config_to_file(&config)?;

    Ok(config)
}

#[tauri::command]
async fn set_widget_opacity(opacity: f64) -> Result<f64, String> {
    let clamped = opacity.clamp(0.05, 1.0);
    let mut config = load_widget_config_from_file();
    config.opacity = clamped;
    save_widget_config_to_file(&config)?;
    Ok(clamped)
}

