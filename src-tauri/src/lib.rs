use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

    // 记录所有由本程序纳管的桌面小组件窗口指针（支持通知窗口、应用盒窗口等）
    static MANAGED_WINS: [AtomicUsize; 4] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];

    fn register_managed_win(win: *mut c_void) {
        let w = win as usize;
        if MANAGED_WINS.iter().any(|a| a.load(Ordering::Relaxed) == w) {
            return;
        }
        for a in &MANAGED_WINS {
            if a.compare_exchange(0, w, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                break;
            }
        }
    }

    fn unregister_managed_win(win: *mut c_void) {
        let w = win as usize;
        for a in &MANAGED_WINS {
            let _ = a.compare_exchange(w, 0, Ordering::Relaxed, Ordering::Relaxed);
        }
    }

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
        let w = _self as usize;
        if !MANAGED_WINS.iter().any(|a| a.load(Ordering::Relaxed) == w) {
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

    unsafe extern "C" fn yes_imp(_this: *mut c_void, _cmd: *mut c_void) -> bool {
        true
    }

    unsafe extern "C" fn yes_first_mouse_imp(_this: *mut c_void, _cmd: *mut c_void, _event: *mut c_void) -> bool {
        true
    }

    unsafe fn setup_accepts_first_mouse(win: *mut c_void, cls: *mut c_void) {
        let first_mouse_sel = sel_registerName(b"acceptsFirstMouse:\0".as_ptr() as _);
        let bool_event_types = b"c@:@\0".as_ptr() as *const c_char;
        let yes_fn: unsafe extern "C" fn() = std::mem::transmute(yes_first_mouse_imp as *const ());

        if !cls.is_null() {
            if !class_addMethod(cls, first_mouse_sel, yes_fn, bool_event_types) {
                let m = class_getInstanceMethod(cls, first_mouse_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }
        }

        if !win.is_null() {
            let cv_sel = sel_registerName(b"contentView\0".as_ptr() as _);
            let f_cv: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let cv = f_cv(win, cv_sel);
            if !cv.is_null() {
                let cv_cls = object_getClass(cv);
                if !class_addMethod(cv_cls, first_mouse_sel, yes_fn, bool_event_types) {
                    let m = class_getInstanceMethod(cv_cls, first_mouse_sel);
                    if !m.is_null() { method_setImplementation(m, yes_fn); }
                }
            }
        }

        let wk_content_cls = objc_getClass(b"WKContentView\0".as_ptr() as _);
        if !wk_content_cls.is_null() {
            if !class_addMethod(wk_content_cls, first_mouse_sel, yes_fn, bool_event_types) {
                let m = class_getInstanceMethod(wk_content_cls, first_mouse_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }
        }

        let wk_web_cls = objc_getClass(b"WKWebView\0".as_ptr() as _);
        if !wk_web_cls.is_null() {
            if !class_addMethod(wk_web_cls, first_mouse_sel, yes_fn, bool_event_types) {
                let m = class_getInstanceMethod(wk_web_cls, first_mouse_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }
        }
    }

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
            register_managed_win(win);

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

            // 允许无边框小组件成为 Key Window / Main Window，确保点击与交互顺畅
            let key_sel = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as _);
            let main_sel = sel_registerName(b"canBecomeMainWindow\0".as_ptr() as _);
            let bool_types = b"c@:\0".as_ptr() as *const c_char;
            let yes_fn: unsafe extern "C" fn() = std::mem::transmute(yes_imp as *const ());
            if !class_addMethod(cls, key_sel, yes_fn, bool_types) {
                let m = class_getInstanceMethod(cls, key_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }
            if !class_addMethod(cls, main_sel, yes_fn, bool_types) {
                let m = class_getInstanceMethod(cls, main_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }

            // 允许接收初次鼠标点击，空白处点击也能直接响应并激活
            setup_accepts_first_mouse(win, cls);

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

            // NSFloatingWindowLevel = 3 (float above all apps)
            msg_isize(win, level_sel, 3);
            // NSWindowCollectionBehaviorCanJoinAllApplications (262144) = Stage Manager ignores management
            // NSWindowCollectionBehaviorStationary (16)                 = don't move when spaces change
            // NSWindowCollectionBehaviorCanJoinAllSpaces (1)
            // NSWindowCollectionBehaviorFullScreenAuxiliary (256)
            // NSWindowCollectionBehaviorIgnoresCycle (64)
            msg_usize(win, behavior_sel, 262144 | 256 | 64 | 16 | 1);

            let button_sel = sel_registerName(b"standardWindowButton:\0".as_ptr() as _);
            let hidden_sel = sel_registerName(b"setHidden:\0".as_ptr() as _);
            let f_btn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let f_hide: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            for btn_type in 0..=2 {
                let btn = f_btn(win, button_sel, btn_type);
                if !btn.is_null() {
                    f_hide(btn, hidden_sel, 1);
                }
            }

            let inval_sel = sel_registerName(b"invalidateShadow\0".as_ptr() as _);
            let f_inval: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_inval(win, inval_sel);

            let order_front_sel = sel_registerName(b"orderFront:\0".as_ptr() as _);
            let f_order: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_order(win, order_front_sel, std::ptr::null_mut());
        } else {
            unregister_managed_win(win);
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
            register_managed_win(win);
            let cls = object_getClass(win);
            let added = class_addMethod(cls, constrain_sel, imp_fn, types);
            if !added {
                let m = class_getInstanceMethod(cls, constrain_sel);
                if !m.is_null() {
                    method_setImplementation(m, imp_fn);
                }
            }

            // 允许无边框小组件成为 Key Window / Main Window，确保点击与交互顺畅
            let key_sel = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as _);
            let main_sel = sel_registerName(b"canBecomeMainWindow\0".as_ptr() as _);
            let bool_types = b"c@:\0".as_ptr() as *const c_char;
            let yes_fn: unsafe extern "C" fn() = std::mem::transmute(yes_imp as *const ());
            if !class_addMethod(cls, key_sel, yes_fn, bool_types) {
                let m = class_getInstanceMethod(cls, key_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }
            if !class_addMethod(cls, main_sel, yes_fn, bool_types) {
                let m = class_getInstanceMethod(cls, main_sel);
                if !m.is_null() { method_setImplementation(m, yes_fn); }
            }

            // 允许接收初次鼠标点击，空白处点击也能直接响应并激活
            setup_accepts_first_mouse(win, cls);

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

            // 桌面小组件窗口层级设置为 -1 (即 kCGNormalWindowLevel - 1)：
            // 1. 普通应用窗口和 macOS 台前调度 (Stage Manager) 的缩略图堆叠卡片都在 Level 0 或更高。
            //    小组件处于 Level -1，意味着台前调度的卡片和所有普通窗口永远天然位于小组件上方，
            //    绝不会被小组件遮挡！
            // 2. Finder (访达) 的桌面全屏壁纸处于 kCGDesktopWindowLevel (-2147483623)，
            //    Level -1 远高于壁纸层，因此鼠标点击、拖拽操作绝不会被桌面背景拦截，
            //    小组件内的所有按钮和卡片均能流畅点击交互！
            msg_isize(win, level_sel, -1);

            // 1: canJoinAllSpaces (所有桌面空间可见)
            // 16: stationary (切换桌面空间时不移动)
            // 64: ignoresCycle (Cmd+Tab 轮巡不选中)
            // 256: fullScreenAuxiliary (全屏时作为辅助窗口)
            // 262144: canJoinAllApplications (Stage Manager 识别为桌面组件，不作为独立舞台应用管理或遮挡卡片)
            msg_usize(win, behavior_sel, 1 | 16 | 64 | 256 | 262144);

            // 隐藏红黄绿按钮（macOS 交通灯按钮），保持 FullSizeContentView 完整不被破坏
            let button_sel = sel_registerName(b"standardWindowButton:\0".as_ptr() as _);
            let hidden_sel = sel_registerName(b"setHidden:\0".as_ptr() as _);
            let f_btn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let f_hide: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            for btn_type in 0..=2 {
                let btn = f_btn(win, button_sel, btn_type);
                if !btn.is_null() {
                    f_hide(btn, hidden_sel, 1); // hidden = YES
                }
            }

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
            unregister_managed_win(win);

            // 恢复红黄绿按钮显示（始终保持在窗口内部左上角，绝不脱离或漂出窗口）
            let button_sel = sel_registerName(b"standardWindowButton:\0".as_ptr() as _);
            let hidden_sel = sel_registerName(b"setHidden:\0".as_ptr() as _);
            let f_btn: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let f_hide: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            for btn_type in 0..=2 {
                let btn = f_btn(win, button_sel, btn_type);
                if !btn.is_null() {
                    f_hide(btn, hidden_sel, 0); // hidden = NO
                }
            }

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


    unsafe extern "C" fn do_elevate_to_front(ctx: *mut c_void) {
        let win = ctx;
        if win.is_null() { return; }

        let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
        // 临时提升层级到 3 (NSFloatingWindowLevel)，确保立即越过任何遮挡的常规应用窗口
        msg_isize(win, level_sel, 3);

        let order_front_sel = sel_registerName(b"orderFrontRegardless\0".as_ptr() as _);
        let f_front: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        f_front(win, order_front_sel);

        let make_key_sel = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as _);
        let f_key: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        f_key(win, make_key_sel, std::ptr::null_mut());

        let ns_app_cls = objc_getClass(b"NSApplication\0".as_ptr() as _);
        if !ns_app_cls.is_null() {
            let shared_sel = sel_registerName(b"sharedApplication\0".as_ptr() as _);
            let f_shared: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let app = f_shared(ns_app_cls, shared_sel);
            if !app.is_null() {
                let act_sel = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as _);
                msg_usize(app, act_sel, 1);
            }
        }
    }

    /// 当小组件被点击获得焦点时，临时提升到最前台让用户交互
    pub fn schedule_elevate_to_front(ns_window: *mut c_void) {
        if ns_window.is_null() { return; }
        unsafe {
            let ns_thread_cls = objc_getClass(b"NSThread\0".as_ptr() as _);
            let is_main_sel = sel_registerName(b"isMainThread\0".as_ptr() as _);
            let f_is_main: unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool =
                std::mem::transmute(objc_msgSend as *const ());
            if !ns_thread_cls.is_null() && f_is_main(ns_thread_cls, is_main_sel) {
                do_elevate_to_front(ns_window);
            } else {
                dispatch_async_f(&_dispatch_main_q, ns_window, do_elevate_to_front);
            }
        }
    }

    unsafe extern "C" fn do_sink_to_desktop(ctx: *mut c_void) {
        let win = ctx;
        if win.is_null() { return; }

        let level_sel = sel_registerName(b"setLevel:\0".as_ptr() as _);
        // 恢复沉降到桌面 -1 层级 (处于所有应用与台前调度下方)
        msg_isize(win, level_sel, -1);

        let order_back_sel = sel_registerName(b"orderBack:\0".as_ptr() as _);
        let f_order: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        f_order(win, order_back_sel, std::ptr::null_mut());
    }

    /// 当小组件失去焦点时，恢复沉降回桌面底层
    pub fn schedule_sink_to_desktop(ns_window: *mut c_void) {
        if ns_window.is_null() { return; }
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ns_window, do_sink_to_desktop);
        }
    }

    pub fn schedule_set_desktop_pin(ns_window: *mut c_void, enable: bool) {
        if ns_window.is_null() { return; }
        let ctx = Box::into_raw(Box::new(DesktopPinCtx { ns_window, enable }));
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ctx as *mut c_void, do_set_desktop_pin);
        }
    }

    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct NSPoint {
        pub x: f64,
        pub y: f64,
    }

    /// 获取鼠标相对于该 NSWindow 的客户区坐标（左上角为 0,0）。
    /// 无论窗口是否为 Key/Active Window，该方法都能瞬时返回鼠标相对坐标。
    pub fn get_window_client_mouse_location(ns_window: *mut c_void) -> Option<(f64, f64)> {
        if ns_window.is_null() { return None; }
        unsafe {
            let mouse_sel = sel_registerName(b"mouseLocationOutsideOfEventStream\0".as_ptr() as _);
            let frame_sel = sel_registerName(b"frame\0".as_ptr() as _);
            let f_mouse: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSPoint =
                std::mem::transmute(objc_msgSend as *const ());
            let f_frame: unsafe extern "C" fn(*mut c_void, *mut c_void) -> CGRect =
                std::mem::transmute(objc_msgSend as *const ());

            let pt = f_mouse(ns_window, mouse_sel);
            let frame = f_frame(ns_window, frame_sel);

            let width = frame.size.width;
            let height = frame.size.height;

            let client_x = pt.x;
            let client_y = height - pt.y;

            if client_x >= 0.0 && client_x <= width && client_y >= 0.0 && client_y <= height {
                Some((client_x, client_y))
            } else {
                None
            }
        }
    }

    unsafe extern "C" fn do_enable_mouse_tracking(ctx: *mut c_void) {
        let win = ctx;
        if win.is_null() { return; }
        let accepts_sel = sel_registerName(b"setAcceptsMouseMovedEvents:\0".as_ptr() as _);
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        f(win, accepts_sel, 1);
    }

    pub fn schedule_enable_mouse_tracking(ns_window: *mut c_void) {
        if ns_window.is_null() { return; }
        unsafe {
            dispatch_async_f(&_dispatch_main_q, ns_window, do_enable_mouse_tracking);
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
mod macos_window {
    use std::ffi::c_void;
    pub fn get_window_client_mouse_location(_ns_window: *mut c_void) -> Option<(f64, f64)> {
        None
    }
    pub fn schedule_enable_mouse_tracking(_ns_window: *mut c_void) {}
}

#[cfg(target_os = "windows")]
mod windows_window {
    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[link(name = "dwmapi")]
    #[link(name = "gdi32")]
    #[link(name = "user32")]
    #[allow(dead_code)]
    extern "system" {
        fn GetWindowLongPtrW(hWnd: isize, nIndex: i32) -> isize;
        fn SetWindowLongPtrW(hWnd: isize, nIndex: i32, dwNewLong: isize) -> isize;
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
        fn ScreenToClient(hWnd: isize, lpPoint: *mut POINT) -> i32;
        fn GetClientRect(hWnd: isize, lpRect: *mut RECT) -> i32;
        fn IsWindow(hWnd: isize) -> i32;
        fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
        fn SetWindowRgn(hWnd: isize, hRgn: isize, bRedraw: i32) -> i32;
        fn DwmSetWindowAttribute(
            hwnd: isize,
            dwAttribute: u32,
            pvAttribute: *const std::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
    }

    /// 获取鼠标相对于该 HWND 的客户区坐标（左上角为 0,0）。
    /// 无论窗口是否为激活窗口，该方法都能瞬时返回鼠标相对逻辑坐标。
    pub fn get_window_client_mouse_location(hwnd: isize, scale_factor: f64) -> Option<(f64, f64)> {
        if hwnd == 0 {
            return None;
        }
        unsafe {
            if IsWindow(hwnd) == 0 {
                return None;
            }
            let mut pt = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            if ScreenToClient(hwnd, &mut pt) == 0 {
                return None;
            }
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetClientRect(hwnd, &mut rect) == 0 {
                return None;
            }

            if pt.x >= rect.left && pt.x <= rect.right && pt.y >= rect.top && pt.y <= rect.bottom {
                let factor = if scale_factor > 0.0 { scale_factor } else { 1.0 };
                let client_x = (pt.x as f64) / factor;
                let client_y = (pt.y as f64) / factor;
                Some((client_x, client_y))
            } else {
                None
            }
        }
    }

    /// 为 Windows 小组件模式配置纯净无边框窗口环境（清除任何系统区域剪裁，移除边框高光与系统圆角）
    pub fn configure_widget_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            if IsWindow(hwnd) == 0 {
                return;
            }

            // 彻底清除物理裁切区域，避免 Win32 判定为复合异形窗口导致非客户区标题栏/按钮泄漏
            SetWindowRgn(hwnd, 0, 1);

            // 1. 禁用 Windows 11 DWM 系统边框高光
            let border_color: u32 = 0xFFFFFFFE; // DWMWA_COLOR_NONE
            let _ = DwmSetWindowAttribute(hwnd, 34 /* DWMWA_BORDER_COLOR */, &border_color as *const _ as _, 4);

            // 2. 禁用 Windows 11 DWM 默认圆角，由 Webview clip-path 统一精确掌控 16px 圆角
            let corner_pref: u32 = 1; // DWMWCP_DONOTROUND
            let _ = DwmSetWindowAttribute(hwnd, 33 /* DWMWA_WINDOW_CORNER_PREFERENCE */, &corner_pref as *const _ as _, 4);

            // 3. 禁用 DWM 默认背景材质 (DWMSBT_NONE)，保持纯透明 WebView2 通道
            let backdrop_type: u32 = 1; // DWMSBT_NONE
            let _ = DwmSetWindowAttribute(hwnd, 38 /* DWMWA_SYSTEMBACKDROP_TYPE */, &backdrop_type as *const _ as _, 4);
        }
    }

    /// 在 Windows 下设置小组件桌面层级 (常驻底层壁纸 vs 悬浮最顶层)
    pub fn set_desktop_pin(hwnd: isize, pin: bool) {
        if hwnd == 0 {
            return;
        }
        const HWND_TOPMOST: isize = -1;
        const HWND_BOTTOM: isize = 1;
        const HWND_NOTOPMOST: isize = -2;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOACTIVATE: u32 = 0x0010;
        const SWP_SHOWWINDOW: u32 = 0x0040;
        const GWL_EXSTYLE: i32 = -20;
        const WS_EX_TOPMOST: isize = 0x00000008;

        unsafe {
            if IsWindow(hwnd) == 0 {
                return;
            }
            if pin {
                // 底层常驻桌面模式：取消置顶样式，并沉入底层壁纸
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if (ex_style & WS_EX_TOPMOST) != 0 {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !WS_EX_TOPMOST);
                }
                SetWindowPos(
                    hwnd,
                    HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                SetWindowPos(
                    hwnd,
                    HWND_BOTTOM,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            } else {
                // 悬浮置顶模式：注入 WS_EX_TOPMOST 并提升到系统最顶层 (永远在所有应用上方)
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if (ex_style & WS_EX_TOPMOST) == 0 {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOPMOST);
                }
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    /// 当小组件被点击获得焦点时，临时提升层级到顶层让用户交互
    pub fn bring_to_front(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        const HWND_TOP: isize = 0;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_SHOWWINDOW: u32 = 0x0040;
        unsafe {
            if IsWindow(hwnd) == 0 {
                return;
            }
            SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }
}

static FOCUS_EPOCH: AtomicU64 = AtomicU64::new(0);

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

#[tauri::command]
fn get_unread_effect() -> String {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_unread_effect.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(effect) = serde_json::from_str::<String>(&content) {
            return effect;
        }
    }
    "aurora".to_string()
}

#[tauri::command]
fn save_unread_effect(app: tauri::AppHandle, effect: String) -> Result<(), String> {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_unread_effect.json", config_dir);
    let _ = std::fs::write(&config_path, serde_json::to_string(&effect).unwrap());
    let _ = app.emit("tauri://unread-effect-changed", serde_json::json!({ "effect": effect }));
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnreadBounceConfig {
    pub enabled: bool,
    pub amplitude: u32,
}

impl Default for UnreadBounceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            amplitude: 6,
        }
    }
}

#[tauri::command]
fn get_unread_bounce() -> UnreadBounceConfig {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_unread_bounce.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(cfg) = serde_json::from_str::<UnreadBounceConfig>(&content) {
            return cfg;
        }
    }
    UnreadBounceConfig::default()
}

#[tauri::command]
fn save_unread_bounce(app: tauri::AppHandle, enabled: bool, amplitude: u32) -> Result<UnreadBounceConfig, String> {
    let clamped_amp = amplitude.clamp(2, 24);
    let cfg = UnreadBounceConfig { enabled, amplitude: clamped_amp };
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_unread_bounce.json", config_dir);
    let _ = std::fs::write(&config_path, serde_json::to_string_pretty(&cfg).unwrap());
    let _ = app.emit("tauri://unread-bounce-changed", serde_json::json!(cfg));
    println!("🦘 已保存未读弹跳设置: enabled={}, amp={}px", enabled, clamped_amp);
    Ok(cfg)
}


fn default_widget_opacity() -> f64 {
    0.70
}

fn default_floating_opacity() -> f64 {
    0.92
}

fn default_desktop_opacity() -> f64 {
    0.45
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WidgetConfig {
    pub widget_x: Option<i32>,
    pub widget_y: Option<i32>,
    pub widget_width: u32,
    pub widget_height: u32,
    #[serde(default = "default_widget_opacity")]
    pub opacity: f64,
    #[serde(default = "default_floating_opacity")]
    pub floating_opacity: f64,
    #[serde(default = "default_desktop_opacity")]
    pub desktop_opacity: f64,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        WidgetConfig {
            widget_x: None,
            widget_y: None,
            widget_width: 340,
            widget_height: 240,
            opacity: 0.70,
            floating_opacity: 0.92,
            desktop_opacity: 0.45,
        }
    }
}

fn load_widget_config_from_file() -> WidgetConfig {
    let config_dir = get_config_dir();
    let config_path = format!("{}/.gotify_widget_config.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(mut config) = serde_json::from_str::<WidgetConfig>(&content) {
            // 防御性过滤
            if config.widget_width < 200 || config.widget_width > 800 {
                config.widget_width = 340;
            }
            if config.widget_height < 140 || config.widget_height > 800 {
                config.widget_height = 240;
            }
            if config.floating_opacity < 0.1 || config.floating_opacity > 1.0 {
                config.floating_opacity = 0.92;
            }
            if config.desktop_opacity < 0.05 || config.desktop_opacity > 1.0 {
                config.desktop_opacity = 0.45;
            }
            if let Some(x) = config.widget_x {
                if x > 10000 || x < -5000 { config.widget_x = None; }
            }
            if let Some(y) = config.widget_y {
                if y > 10000 || y < -5000 { config.widget_y = None; }
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
    ws_status: Arc<Mutex<String>>,
    tray_icon: Arc<Mutex<Option<TrayIcon>>>,
    pin_menu_item: Arc<Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>>,
    toggle_menu_item: Arc<Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>>,
    dock_menu_item: Arc<Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>>,
    /// 启动期间抑制位置与尺寸保存，防止过渡过程中的中间事件污染持久化坐标
    is_suppressing_save: Arc<AtomicBool>,
    /// 小组件移动防抖持久化令牌
    widget_move_debounce_token: Arc<AtomicU64>,
}

fn emit_status(app: &tauri::AppHandle, ws_status: &Arc<Mutex<String>>, status: &str) {
    if let Ok(mut lock) = ws_status.lock() {
        *lock = status.to_string();
    }
    let _ = app.emit("websocket-status", status);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("websocket-status", status);
    }
}

fn get_dock_menu_text(show_in_dock: bool) -> &'static str {
    if cfg!(target_os = "macos") {
        if show_in_dock {
            "🍎 隐藏 Dock 图标"
        } else {
            "🍎 显示 Dock 图标"
        }
    } else {
        if show_in_dock {
            "🪟 隐藏任务栏图标"
        } else {
            "🪟 显示任务栏图标"
        }
    }
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
    let ws_status_clone = state.ws_status.clone();
    
    tokio::spawn(async move {
        if let Err(e) = run_websocket(app, config, ws_running_clone, ws_status_clone).await {
            eprintln!("WebSocket error: {}", e);
        }
    });

    Ok("WebSocket connection started".to_string())
}

#[tauri::command]
async fn stop_websocket(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let mut ws_running = state.ws_running.lock().unwrap();
    *ws_running = false;
    emit_status(&app, &state.ws_status, "disconnected");
    Ok("WebSocket connection stopped".to_string())
}

#[tauri::command]
async fn is_websocket_running(state: State<'_, AppState>) -> Result<bool, String> {
    let ws_running = state.ws_running.lock().unwrap();
    Ok(*ws_running)
}

#[tauri::command]
async fn get_connection_status(state: State<'_, AppState>) -> Result<String, String> {
    let status = state.ws_status.lock().unwrap();
    Ok(status.clone())
}

#[tauri::command]
async fn snap_window_to_edge(
    app: tauri::AppHandle,
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
        let mut cfg = load_widget_config_from_file();
        cfg.widget_x = Some(position.x);
        cfg.widget_y = Some(position.y);
        let _ = save_widget_config_to_file(&cfg);
    }
    Ok(())
}



async fn run_websocket(
    app: tauri::AppHandle,
    config: ConnectionConfig,
    ws_running: Arc<Mutex<bool>>,
    ws_status: Arc<Mutex<String>>,
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
            emit_status(&app, &ws_status, "reconnecting");
        } else {
            println!("🔌 正在建立 WebSocket 连接...");
            emit_status(&app, &ws_status, "connecting");
        }

        match connect_async(url.clone()).await {
            Ok((ws_stream, _)) => {
                println!("✅ WebSocket 连接建立成功");
                reconnect_attempt = 0;
                emit_status(&app, &ws_status, "connected");

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
        emit_status(&app, &ws_status, "reconnecting");
        println!("⏳ {} 秒后尝试重新连接...", delay_secs);
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
    }

    emit_status(&app, &ws_status, "disconnected");
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
            ws_status: Arc::new(Mutex::new("connecting".to_string())),
            tray_icon: Arc::new(Mutex::new(None)),
            pin_menu_item: Arc::new(Mutex::new(None)),
            toggle_menu_item: Arc::new(Mutex::new(None)),
            dock_menu_item: Arc::new(Mutex::new(None)),
            is_suppressing_save: Arc::new(AtomicBool::new(true)),
            widget_move_debounce_token: Arc::new(AtomicU64::new(0)),
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

            #[cfg(target_os = "macos")]
            {
                if let Ok(ns_win_ptr) = main_window.ns_window() {
                    macos_window::schedule_enable_mouse_tracking(ns_win_ptr as *mut std::ffi::c_void);
                }
            }

            // 启动后台无焦鼠标移动监测任务：在应用未被激活/无焦点的情况下，鼠标滑过卡片依然能够瞬时检测并标记已阅
            let hover_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_client_pos: Option<(i32, i32)> = None;
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    if let Some(win) = hover_app.get_webview_window("main") {
                        if let Ok(is_visible) = win.is_visible() {
                            if !is_visible {
                                continue;
                            }
                        } else {
                            continue;
                        }

                        #[cfg(target_os = "macos")]
                        {
                            if let Ok(ns_win_ptr) = win.ns_window() {
                                if let Some((cx, cy)) = macos_window::get_window_client_mouse_location(ns_win_ptr as *mut std::ffi::c_void) {
                                    let rounded = (cx.round() as i32, cy.round() as i32);
                                    if last_client_pos != Some(rounded) {
                                        last_client_pos = Some(rounded);
                                        let _ = win.emit("native-mouse-hover", serde_json::json!({ "x": cx, "y": cy }));
                                    }
                                } else if last_client_pos.is_some() {
                                    last_client_pos = None;
                                }
                            }
                        }

                        #[cfg(target_os = "windows")]
                        {
                            if let Ok(hwnd) = win.hwnd() {
                                let scale_factor = win.scale_factor().unwrap_or(1.0);
                                if let Some((cx, cy)) = windows_window::get_window_client_mouse_location(hwnd.0 as isize, scale_factor) {
                                    let rounded = (cx.round() as i32, cy.round() as i32);
                                    if last_client_pos != Some(rounded) {
                                        last_client_pos = Some(rounded);
                                        let _ = win.emit("native-mouse-hover", serde_json::json!({ "x": cx, "y": cy }));
                                    }
                                } else if last_client_pos.is_some() {
                                    last_client_pos = None;
                                }
                            }
                        }
                    }
                }
            });

            // Windows: 根据用户设置决定是否在任务栏显示图标
            #[cfg(target_os = "windows")]
            {
                let config_dir = get_config_dir();
                let dock_path = format!("{}/.gotify_show_in_dock.json", config_dir);
                let show_in_dock = std::fs::read_to_string(&dock_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<bool>(&c).ok())
                    .unwrap_or(false);
                if !show_in_dock {
                    let _ = main_window.set_skip_taskbar(true);
                }
            }
            
            // 恢复桌面小组件位置与大小
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<AppState> = app_handle.state();
                let widget_config = load_widget_config_from_file();
                let always_on_top = get_window_always_on_top(app_handle.clone());

                let target_w = widget_config.widget_width.clamp(200, 800);
                let target_h = widget_config.widget_height.clamp(140, 800);
                let _ = main_window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: target_w as f64,
                    height: target_h as f64,
                }));

                if let (Some(wx), Some(wy)) = (widget_config.widget_x, widget_config.widget_y) {
                    let safe_x = if wx > 10000 || wx < -5000 { 100 } else { wx };
                    let safe_y = if wy > 10000 || wy < -5000 { 100 } else { wy };
                    let _ = main_window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                        x: safe_x,
                        y: safe_y,
                    }));
                } else {
                    let _ = main_window.center();
                }

                #[cfg(target_os = "macos")]
                {
                    if let Ok(ns_win_ptr) = main_window.ns_window() {
                        if always_on_top {
                            let _ = main_window.set_always_on_top(true);
                            macos_window::schedule_set_floating(ns_win_ptr as *mut std::ffi::c_void, true);
                        } else {
                            let _ = main_window.set_always_on_top(false);
                            macos_window::schedule_set_desktop_pin(ns_win_ptr as *mut std::ffi::c_void, true);
                        }
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    if let Ok(hwnd) = main_window.hwnd() {
                        windows_window::configure_widget_window(hwnd.0 as isize);
                        if always_on_top {
                            let _ = main_window.set_always_on_top(true);
                            windows_window::set_desktop_pin(hwnd.0 as isize, false);
                        } else {
                            let _ = main_window.set_always_on_top(false);
                            windows_window::set_desktop_pin(hwnd.0 as isize, true);
                        }
                    }
                }

                let _ = main_window.unminimize();
                let _ = main_window.show();
                let _ = main_window.set_focus();

                #[cfg(target_os = "windows")]
                {
                    if let Ok(hwnd) = main_window.hwnd() {
                        if always_on_top {
                            windows_window::set_desktop_pin(hwnd.0 as isize, false);
                        }
                    }
                }

                // 启动与初始布局完成后，延时解除事件抑制
                let suppress_flag = state.is_suppressing_save.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    suppress_flag.store(false, Ordering::SeqCst);
                    println!("🔓 小组件初始化就绪，已解除位置与尺寸保存抑制");
                });

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
                        let ws_status_clone = state.ws_status.clone();
                        tokio::spawn(async move {
                            if let Err(e) = run_websocket(app_handle, config, ws_running_clone, ws_status_clone).await {
                                eprintln!("WebSocket error: {}", e);
                            }
                        });
                    }
                }
            });

            // 若应用盒此前处于开启状态，启动时自动恢复显示桌面应用盒
            let shelf_cfg = load_app_shelf_config_from_file();
            if shelf_cfg.is_open {
                let shelf_app = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    let _ = create_app_shelf_window(&shelf_app);
                });
            }

            // 创建系统托盘
            use tauri::{menu::{Menu, MenuItem}};

            println!("🔧 开始创建系统托盘...");
            let is_pinned = get_window_always_on_top(app.handle().clone());
            let initial_pin_text = if is_pinned {
                "🖥️ 常驻桌面 (底层)"
            } else {
                "📌 悬浮置顶 (前台)"
            };

            let show_in_dock = get_show_in_dock();
            let initial_dock_text = get_dock_menu_text(show_in_dock);

            let toggle_item = MenuItem::with_id(app, "toggle", "隐藏通知小组件", true, None::<&str>)?;
            let shelf_item = MenuItem::with_id(app, "shelf", "🗂️ 桌面应用盒", true, None::<&str>)?;
            let pin_item = MenuItem::with_id(app, "pin", initial_pin_text, true, None::<&str>)?;
            let dock_item = MenuItem::with_id(app, "dock", initial_dock_text, true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &shelf_item, &pin_item, &dock_item, &settings_item, &quit_item])?;

            // 保存菜单项到状态供实时双向同步
            let state: tauri::State<AppState> = app.state();
            *state.pin_menu_item.lock().unwrap() = Some(pin_item.clone());
            *state.toggle_menu_item.lock().unwrap() = Some(toggle_item.clone());
            *state.dock_menu_item.lock().unwrap() = Some(dock_item.clone());

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
                            let state: tauri::State<AppState> = app.state();
                            if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
                                if window.is_focused().unwrap_or(false) {
                                    let _ = window.hide();
                                    if let Ok(guard) = state.toggle_menu_item.lock() {
                                        if let Some(ref item) = *guard {
                                            let _ = item.set_text("显示小组件");
                                        }
                                    }
                                } else {
                                    let _ = window.set_focus();
                                }
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                                if let Ok(guard) = state.toggle_menu_item.lock() {
                                    if let Some(ref item) = *guard {
                                        let _ = item.set_text("隐藏小组件");
                                    }
                                }
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "toggle" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let state: tauri::State<AppState> = app.state();
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                    if let Ok(guard) = state.toggle_menu_item.lock() {
                                        if let Some(ref item) = *guard {
                                            let _ = item.set_text("显示小组件");
                                        }
                                    }
                                } else {
                                    let _ = window.show();
                                    let _ = window.unminimize();
                                    let _ = window.set_focus();
                                    if let Ok(guard) = state.toggle_menu_item.lock() {
                                        if let Some(ref item) = *guard {
                                            let _ = item.set_text("隐藏小组件");
                                        }
                                    }
                                }
                            }
                        }
                        "shelf" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = toggle_app_shelf_window(app_handle).await;
                            });
                        }
                        "pin" => {
                            let _ = toggle_window_always_on_top(app.clone());
                        }
                        "dock" => {
                            let current = get_show_in_dock();
                            let _ = set_show_in_dock(app.clone(), !current);
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
                // Windows & 主窗口：点击关闭或快捷键时，隐藏到托盘而不是直接退出应用
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                } else if window.label() == "app_shelf" {
                    let _ = window.hide();
                    let mut cfg = load_app_shelf_config_from_file();
                    cfg.is_open = false;
                    let _ = save_app_shelf_config_to_file(&cfg);
                    api.prevent_close();
                }
            }
            tauri::WindowEvent::Focused(focused) => {
                let current_epoch = FOCUS_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
                if *focused {
                    // 当桌面小组件被点击获得焦点时，临时提升层级到前台 (Level 3)，让用户能够看清并无阻碍操作
                    #[cfg(target_os = "macos")]
                    {
                        if let Ok(ns_win) = window.ns_window() {
                            macos_window::schedule_elevate_to_front(ns_win as *mut std::ffi::c_void);
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        if let Ok(hwnd) = window.hwnd() {
                            windows_window::bring_to_front(hwnd.0 as isize);
                        }
                    }
                } else {
                    // 小组件失去焦点时，延迟 6 秒才沉降回底层桌面，确保用户点击后有充足的 5~6 秒时间查看并交互
                    let win_clone = window.clone();
                    let always_on_top = if window.label() == "main" {
                        get_window_always_on_top(window.app_handle().clone())
                    } else if window.label() == "app_shelf" {
                        load_app_shelf_config_from_file().always_on_top
                    } else {
                        false
                    };

                    if !always_on_top {
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_millis(6000)).await;
                            // 如果 6 秒内有任何新的焦点或激活事件发生，立即取消本次沉降
                            if FOCUS_EPOCH.load(Ordering::SeqCst) != current_epoch {
                                return;
                            }
                            // 如果 6 秒后窗口依然具有焦点，不沉降
                            if let Ok(is_focused) = win_clone.is_focused() {
                                if is_focused {
                                    return;
                                }
                            }
                            #[cfg(target_os = "macos")]
                            {
                                if let Ok(ns_win) = win_clone.ns_window() {
                                    macos_window::schedule_sink_to_desktop(ns_win as *mut std::ffi::c_void);
                                }
                            }

                            #[cfg(target_os = "windows")]
                            {
                                if let Ok(hwnd) = win_clone.hwnd() {
                                    windows_window::set_desktop_pin(hwnd.0 as isize, true);
                                }
                            }
                        });
                    }
                }
            }
            tauri::WindowEvent::Moved(pos) => {
                if window.label() == "main" {
                    let state: tauri::State<AppState> = window.state();
                    if !state.is_suppressing_save.load(Ordering::SeqCst) {
                        let token_arc = state.widget_move_debounce_token.clone();
                        let current_token = token_arc.fetch_add(1, Ordering::SeqCst) + 1;
                        let target_x = pos.x;
                        let target_y = pos.y;
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                            if token_arc.load(Ordering::SeqCst) == current_token {
                                let mut cfg = load_widget_config_from_file();
                                cfg.widget_x = Some(target_x);
                                cfg.widget_y = Some(target_y);
                                let _ = save_widget_config_to_file(&cfg);
                                println!("💾 小组件位置已防抖保存: x={}, y={}", target_x, target_y);
                            }
                        });
                    }
                } else if window.label() == "app_shelf" {
                    let mut cfg = load_app_shelf_config_from_file();
                    cfg.window_x = Some(pos.x);
                    cfg.window_y = Some(pos.y);
                    let _ = save_app_shelf_config_to_file(&cfg);
                }
            }
            tauri::WindowEvent::Resized(size) => {
                if window.label() == "main" {
                    let state: tauri::State<AppState> = window.state();
                    if state.is_suppressing_save.load(Ordering::SeqCst) {
                        return;
                    }

                    let factor = window.scale_factor().unwrap_or(1.0);
                    let logical_w = ((size.width as f64) / factor).round() as u32;
                    let logical_h = ((size.height as f64) / factor).round() as u32;

                    if logical_w >= 200 && logical_w <= 800 && logical_h >= 140 && logical_h <= 800 {
                        let mut cfg = load_widget_config_from_file();
                        cfg.widget_width = logical_w;
                        cfg.widget_height = logical_h;
                        let _ = save_widget_config_to_file(&cfg);
                    }
                } else if window.label() == "app_shelf" {
                    let factor = window.scale_factor().unwrap_or(1.0);
                    let logical_w = ((size.width as f64) / factor).round() as u32;
                    let logical_h = ((size.height as f64) / factor).round() as u32;

                    if logical_w >= 180 && logical_h >= 120 {
                        let mut cfg = load_app_shelf_config_from_file();
                        cfg.window_width = logical_w;
                        cfg.window_height = logical_h;
                        let _ = save_app_shelf_config_to_file(&cfg);
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
            tauri::WindowEvent::DragDrop(drag_event) => {
                if window.label() == "app_shelf" {
                    match drag_event {
                        tauri::DragDropEvent::Drop { paths, .. } => {
                            let str_paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                            if let Ok(updated) = add_apps_to_shelf(str_paths) {
                                let _ = window.emit("app-shelf-updated", updated);
                            }
                        }
                        _ => {}
                    }
                }
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
            get_connection_status,
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
            get_widget_config,
            save_widget_position,
            set_widget_size,
            set_widget_opacity,
            set_widget_mode_opacity,
            get_unread_effect,
            save_unread_effect,
            get_unread_bounce,
            save_unread_bounce,
            toggle_app_shelf_window,
            get_app_shelf_items,
            add_apps_to_shelf,
            remove_app_from_shelf,
            open_app_item,
            show_in_finder,
            pick_and_add_app,
            get_app_shelf_config,
            save_app_shelf_config,
            set_shelf_always_on_top,
            set_shelf_opacity,
            set_shelf_mode_opacity,
            activate_widget_window,
            set_app_shelf_hide_header,
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
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str::<bool>(&c).ok())
        .unwrap_or(false)
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

    // 实时更新托盘菜单快捷开关文字
    let state: tauri::State<AppState> = app.state();
    if let Ok(guard) = state.dock_menu_item.lock() {
        if let Some(ref item) = *guard {
            let text = get_dock_menu_text(enabled);
            let _ = item.set_text(text);
        }
    }

    // 广播事件通知前端设置页面同步刷新开关
    let _ = app.emit("tauri://show-in-dock-changed", serde_json::json!({ "enabled": enabled }));

    println!("🪟 任务栏/Dock 状态已更新: enabled={}", enabled);
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

        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win_ptr) = window.ns_window() {
                if always_on_top {
                    macos_window::schedule_set_floating(
                        ns_win_ptr as *mut std::ffi::c_void,
                        true,
                    );
                } else {
                    macos_window::schedule_set_desktop_pin(
                        ns_win_ptr as *mut std::ffi::c_void,
                        true,
                    );
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd) = window.hwnd() {
                if always_on_top {
                    windows_window::set_desktop_pin(hwnd.0 as isize, false);
                } else {
                    windows_window::set_desktop_pin(hwnd.0 as isize, true);
                }
            }
        }
    }
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_always_on_top.json", config_dir);
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&always_on_top).unwrap());

    // 实时同步托盘菜单置顶状态项文本
    let state: tauri::State<AppState> = app.state();
    if let Ok(guard) = state.pin_menu_item.lock() {
        if let Some(ref item) = *guard {
            let text = if always_on_top {
                "🖥️ 常驻桌面 (底层)"
            } else {
                "📌 悬浮置顶 (前台)"
            };
            let _ = item.set_text(text);
        }
    }

    // 广播事件通知前端更新状态
    let _ = app.emit("tauri://always-on-top-changed", serde_json::json!({ "always_on_top": always_on_top }));

    println!("📌 窗口置顶/常驻桌面状态已设置: always_on_top={}", always_on_top);
    Ok(always_on_top)
}

#[tauri::command]
fn toggle_window_always_on_top(app: tauri::AppHandle) -> Result<bool, String> {
    let current = get_window_always_on_top(app.clone());
    let new_state = !current;
    set_window_always_on_top(app, new_state)
}

#[tauri::command]
fn start_dragging_window(window: tauri::WebviewWindow) -> Result<(), String> {
    FOCUS_EPOCH.fetch_add(1, Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    {
        if let Ok(ns_win) = window.ns_window() {
            macos_window::schedule_elevate_to_front(ns_win as *mut std::ffi::c_void);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = window.hwnd() {
            windows_window::bring_to_front(hwnd.0 as isize);
        }
    }
    let _ = window.set_focus();

    let res = window.start_dragging().map_err(|e| format!("拖动窗口失败: {}", e));

    #[cfg(target_os = "macos")]
    {
        if let Ok(ns_win) = window.ns_window() {
            macos_window::schedule_elevate_to_front(ns_win as *mut std::ffi::c_void);
        }
    }
    let _ = window.set_focus();

    res
}

#[tauri::command]
fn activate_widget_window(window: tauri::WebviewWindow) -> Result<(), String> {
    FOCUS_EPOCH.fetch_add(1, Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    {
        if let Ok(ns_win) = window.ns_window() {
            macos_window::schedule_elevate_to_front(ns_win as *mut std::ffi::c_void);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = window.hwnd() {
            windows_window::bring_to_front(hwnd.0 as isize);
        }
    }
    let _ = window.set_focus();
    Ok(())
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
async fn set_widget_size(app: tauri::AppHandle, width: u32, height: u32) -> Result<WidgetConfig, String> {
    let w = width.clamp(200, 800);
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

#[tauri::command]
async fn set_widget_mode_opacity(mode: String, opacity: f64) -> Result<f64, String> {
    let clamped = opacity.clamp(0.05, 1.0);
    let mut config = load_widget_config_from_file();
    if mode == "floating" {
        config.floating_opacity = clamped;
    } else {
        config.desktop_opacity = clamped;
    }
    config.opacity = clamped;
    save_widget_config_to_file(&config)?;
    println!("💾 已保存 {} 模式透明度: {:.2}", mode, clamped);
    Ok(clamped)
}

// ─────────────────────────────────────────────────────────────────────────────
// 桌面应用快捷收纳盒 (Desktop App Shelf)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
extern "C" {
    fn macos_get_display_name(file_path: *const std::ffi::c_char) -> *mut std::ffi::c_char;
    fn macos_get_icon_png(file_path: *const std::ffi::c_char, target_size: i32, out_len: *mut i32) -> *mut u8;
    fn macos_pick_app_file() -> *mut std::ffi::c_char;
    fn free(ptr: *mut std::ffi::c_void);
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
unsafe fn macos_get_display_name(_file_path: *const std::ffi::c_char) -> *mut std::ffi::c_char { std::ptr::null_mut() }
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
unsafe fn macos_get_icon_png(_file_path: *const std::ffi::c_char, _target_size: i32, _out_len: *mut i32) -> *mut u8 { std::ptr::null_mut() }
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
unsafe fn macos_pick_app_file() -> *mut std::ffi::c_char { std::ptr::null_mut() }
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
unsafe fn free(_ptr: *mut std::ffi::c_void) {}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppShelfItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub icon_data: String,
    pub added_at: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppShelfConfig {
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: u32,
    pub window_height: u32,
    pub opacity: f64,
    pub floating_opacity: f64,
    pub desktop_opacity: f64,
    pub always_on_top: bool,
    pub is_open: bool,
    #[serde(default)]
    pub hide_header: bool,
}

impl Default for AppShelfConfig {
    fn default() -> Self {
        AppShelfConfig {
            window_x: None,
            window_y: None,
            window_width: 320,
            window_height: 240,
            opacity: 0.35,
            floating_opacity: 0.92,
            desktop_opacity: 0.35,
            always_on_top: false,
            is_open: true,
            hide_header: false,
        }
    }
}

fn load_app_shelf_items_from_file() -> Vec<AppShelfItem> {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_app_shelf.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(items) = serde_json::from_str::<Vec<AppShelfItem>>(&content) {
            return items;
        }
    }
    Vec::new()
}

fn save_app_shelf_items_to_file(items: &[AppShelfItem]) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_app_shelf.json", config_dir);
    let json = serde_json::to_string_pretty(items)
        .map_err(|e| format!("序列化应用盒失败: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("写入应用盒配置失败: {}", e))
}

fn load_app_shelf_config_from_file() -> AppShelfConfig {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_app_shelf_config.json", config_dir);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<AppShelfConfig>(&content) {
            return cfg;
        }
    }
    AppShelfConfig::default()
}

fn save_app_shelf_config_to_file(config: &AppShelfConfig) -> Result<(), String> {
    let config_dir = get_config_dir();
    let path = format!("{}/.gotify_app_shelf_config.json", config_dir);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化应用盒窗口配置失败: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("写入应用盒窗口配置失败: {}", e))
}

fn get_app_display_name(path: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(c_path) = std::ffi::CString::new(path) {
            unsafe {
                let name_ptr = macos_get_display_name(c_path.as_ptr());
                if !name_ptr.is_null() {
                    let name = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().to_string();
                    free(name_ptr as *mut std::ffi::c_void);
                    if !name.trim().is_empty() {
                        return name;
                    }
                }
            }
        }
    }

    let p = std::path::Path::new(path);
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn get_app_icon_data_url(path: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(c_path) = std::ffi::CString::new(path) {
            let mut out_len: i32 = 0;
            unsafe {
                let png_ptr = macos_get_icon_png(c_path.as_ptr(), 128, &mut out_len);
                if !png_ptr.is_null() && out_len > 0 {
                    let slice = std::slice::from_raw_parts(png_ptr, out_len as usize);
                    use base64::Engine;
                    let b64 = base64::prelude::BASE64_STANDARD.encode(slice);
                    free(png_ptr as *mut std::ffi::c_void);
                    return format!("data:image/png;base64,{}", b64);
                }
            }
        }
    }

    String::new()
}

pub fn create_app_shelf_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window("app_shelf") {
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return Ok(existing);
    }

    let cfg = load_app_shelf_config_from_file();
    let width = cfg.window_width.max(200) as f64;
    let height = cfg.window_height.max(140) as f64;

    let win = tauri::WebviewWindowBuilder::new(
        app,
        "app_shelf",
        tauri::WebviewUrl::App("shelf.html".into()),
    )
    .title("桌面应用盒")
    .inner_size(width, height)
    .min_inner_size(200.0, 140.0)
    .resizable(true)
    .maximizable(false)
    .fullscreen(false)
    .decorations(false)
    .always_on_top(cfg.always_on_top)
    .transparent(true)
    .shadow(false)
    .build()
    .map_err(|e| format!("创建应用盒窗口失败: {}", e))?;

    if let (Some(x), Some(y)) = (cfg.window_x, cfg.window_y) {
        let safe_x = if x > 10000 || x < -5000 { 100 } else { x };
        let safe_y = if y > 10000 || y < -5000 { 100 } else { y };
        let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: safe_x, y: safe_y }));
    } else {
        let _ = win.center();
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(ns_win_ptr) = win.ns_window() {
            if cfg.always_on_top {
                macos_window::schedule_set_floating(ns_win_ptr as *mut std::ffi::c_void, true);
            } else {
                macos_window::schedule_set_desktop_pin(ns_win_ptr as *mut std::ffi::c_void, true);
            }
        }
    }

    Ok(win)
}

#[tauri::command]
async fn toggle_app_shelf_window(app: tauri::AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("app_shelf") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            let mut cfg = load_app_shelf_config_from_file();
            cfg.is_open = false;
            let _ = save_app_shelf_config_to_file(&cfg);
            return Ok(false);
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            let mut cfg = load_app_shelf_config_from_file();
            cfg.is_open = true;
            let _ = save_app_shelf_config_to_file(&cfg);
            return Ok(true);
        }
    }

    create_app_shelf_window(&app)?;
    let mut cfg = load_app_shelf_config_from_file();
    cfg.is_open = true;
    let _ = save_app_shelf_config_to_file(&cfg);
    Ok(true)
}

#[tauri::command]
fn get_app_shelf_items() -> Result<Vec<AppShelfItem>, String> {
    Ok(load_app_shelf_items_from_file())
}

#[tauri::command]
fn add_apps_to_shelf(paths: Vec<String>) -> Result<Vec<AppShelfItem>, String> {
    let mut items = load_app_shelf_items_from_file();
    for raw_path in paths {
        let path = raw_path.trim().to_string();
        if path.is_empty() { continue; }
        let p = std::path::Path::new(&path);
        if !p.exists() { continue; }

        if items.iter().any(|it| it.path == path) {
            continue;
        }

        let name = get_app_display_name(&path);
        let icon_data = get_app_icon_data_url(&path);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let id = format!("{}_{}", now.as_millis(), items.len());

        items.push(AppShelfItem {
            id,
            name,
            path,
            icon_data,
            added_at: now.as_secs(),
        });
    }

    save_app_shelf_items_to_file(&items)?;
    Ok(items)
}

#[tauri::command]
fn remove_app_from_shelf(id: String) -> Result<Vec<AppShelfItem>, String> {
    let mut items = load_app_shelf_items_from_file();
    items.retain(|it| it.id != id);
    save_app_shelf_items_to_file(&items)?;
    Ok(items)
}

#[tauri::command]
fn open_app_item(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开应用失败: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/C", "start", "", &path])
            .spawn()
            .map_err(|e| format!("打开应用失败: {}", e))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开应用失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn show_in_finder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(&["-R", &path])
            .spawn()
            .map_err(|e| format!("在访达中显示失败: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .args(&[format!("/select,{}", path)])
            .spawn()
            .map_err(|e| format!("在资源管理器中显示失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn pick_and_add_app() -> Result<Vec<AppShelfItem>, String> {
    #[cfg(target_os = "macos")]
    {
        unsafe {
            let picked_ptr = macos_pick_app_file();
            if !picked_ptr.is_null() {
                let c_str = std::ffi::CStr::from_ptr(picked_ptr);
                let path = c_str.to_string_lossy().to_string();
                free(picked_ptr as *mut std::ffi::c_void);
                return add_apps_to_shelf(vec![path]);
            }
        }
    }
    Ok(load_app_shelf_items_from_file())
}

#[tauri::command]
fn get_app_shelf_config() -> Result<AppShelfConfig, String> {
    Ok(load_app_shelf_config_from_file())
}

#[tauri::command]
fn save_app_shelf_config(config: AppShelfConfig) -> Result<(), String> {
    save_app_shelf_config_to_file(&config)
}

#[tauri::command]
fn set_shelf_always_on_top(app: tauri::AppHandle, always_on_top: bool) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("app_shelf") {
        let _ = window.set_always_on_top(always_on_top);

        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win_ptr) = window.ns_window() {
                if always_on_top {
                    macos_window::schedule_set_floating(ns_win_ptr as *mut std::ffi::c_void, true);
                } else {
                    macos_window::schedule_set_desktop_pin(ns_win_ptr as *mut std::ffi::c_void, true);
                }
            }
        }

        let mut cfg = load_app_shelf_config_from_file();
        cfg.always_on_top = always_on_top;
        let _ = save_app_shelf_config_to_file(&cfg);
        return Ok(always_on_top);
    }
    Ok(always_on_top)
}

#[tauri::command]
async fn set_shelf_opacity(opacity: f64) -> Result<f64, String> {
    let clamped = opacity.clamp(0.05, 1.0);
    let mut config = load_app_shelf_config_from_file();
    config.opacity = clamped;
    save_app_shelf_config_to_file(&config)?;
    Ok(clamped)
}

#[tauri::command]
async fn set_shelf_mode_opacity(mode: String, opacity: f64) -> Result<f64, String> {
    let clamped = opacity.clamp(0.05, 1.0);
    let mut config = load_app_shelf_config_from_file();
    if mode == "floating" {
        config.floating_opacity = clamped;
    } else {
        config.desktop_opacity = clamped;
    }
    config.opacity = clamped;
    save_app_shelf_config_to_file(&config)?;
    Ok(clamped)
}

#[tauri::command]
fn set_app_shelf_hide_header(hide: bool) -> Result<bool, String> {
    let mut cfg = load_app_shelf_config_from_file();
    cfg.hide_header = hide;
    save_app_shelf_config_to_file(&cfg)?;
    Ok(hide)
}

