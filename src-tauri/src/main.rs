// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn check_and_focus_existing_instance() -> bool {
    use std::ffi::CString;
    extern "system" {
        fn CreateMutexA(lpMutexAttributes: *mut std::ffi::c_void, bInitialOwner: i32, lpName: *const i8) -> isize;
        fn GetLastError() -> u32;
        fn EnumWindows(lpEnumFunc: unsafe extern "system" fn(isize, isize) -> i32, lParam: isize) -> i32;
        fn GetCurrentProcessId() -> u32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;

    let mutex_name = CString::new("Local\\GotifyNotifierSingleInstanceMutex").unwrap();
    unsafe {
        let handle = CreateMutexA(std::ptr::null_mut(), 1, mutex_name.as_ptr());
        if handle != 0 && GetLastError() == ERROR_ALREADY_EXISTS {
            let my_pid = GetCurrentProcessId();

            unsafe extern "system" fn enum_callback(hwnd: isize, lparam: isize) -> i32 {
                extern "system" {
                    fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
                    fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
                    fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
                    fn SetForegroundWindow(hWnd: isize) -> i32;
                }
                let my_pid = lparam as u32;
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid != my_pid && pid != 0 {
                    let mut buf = [0u16; 256];
                    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
                    if len > 0 {
                        let title = String::from_utf16_lossy(&buf[..len as usize]);
                        if title.contains("Gotify") {
                            ShowWindow(hwnd, 9); // SW_RESTORE
                            ShowWindow(hwnd, 5); // SW_SHOW
                            SetForegroundWindow(hwnd);
                            return 0; // stop enumerating
                        }
                    }
                }
                1 // continue
            }

            EnumWindows(enum_callback, my_pid as isize);
            return false;
        }
    }
    true
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        if !check_and_focus_existing_instance() {
            return;
        }
    }

    gotify_notifier_lib::run();
}
