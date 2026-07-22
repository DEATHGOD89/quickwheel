use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey,
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, MOD_NOREPEAT,
    VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
    VK_F9, VK_F10, VK_F11, VK_F12
};
use windows::Win32::Foundation::{HWND, LRESULT, WPARAM, LPARAM, POINT};
use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, SetWindowLongW, GWLP_WNDPROC, GWL_WNDPROC, CallWindowProcW, WM_HOTKEY, WNDPROC, GetCursorPos, FindWindowW, ShowWindow, IsWindowVisible, SW_HIDE, SW_SHOW};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicIsize, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tauri::Manager;

static HOTKEY_ID_COUNTER: AtomicU32 = AtomicU32::new(0xC000);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static ACTIVE_HOTKEYS: Lazy<Mutex<HashMap<u32, GlobalHotkey>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static ORIGINAL_WNDPROC: Lazy<Mutex<isize>> = Lazy::new(|| Mutex::new(0));
static APP_HANDLE: Lazy<Mutex<Option<tauri::AppHandle>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotkeyAction {
    RadialMenu,
    ToggleTaskbar,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalHotkey {
    pub id: u32,
    pub action: HotkeyAction,
}

impl GlobalHotkey {
    pub fn register(hotkey_str: &str, action: HotkeyAction) -> Result<Self, String> {
        let (modifiers, vk_code) = Self::parse_hotkey_string(hotkey_str)?;
        let id = HOTKEY_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        let hwnd_isize = MAIN_HWND.load(Ordering::SeqCst);
        let hwnd = if hwnd_isize == 0 { HWND(0) } else { HWND(hwnd_isize) };
        
        unsafe {
            let result = RegisterHotKey(
                hwnd,
                id as i32,
                windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(modifiers),
                vk_code,
            );
            
            if result.is_ok() {
                let hotkey = GlobalHotkey { id, action };
                ACTIVE_HOTKEYS.lock().unwrap().insert(id, hotkey);
                
                // Verify registration
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok(hotkey)
            } else {
                Err(format!("Failed to register hotkey: {}", hotkey_str))
            }
        }
    }
    
    pub fn unregister_all() {
        let hwnd_isize = MAIN_HWND.load(Ordering::SeqCst);
        let hwnd = if hwnd_isize == 0 { HWND(0) } else { HWND(hwnd_isize) };
        let mut active = ACTIVE_HOTKEYS.lock().unwrap();
        for (_, hotkey) in active.iter() {
            unsafe {
                let _ = UnregisterHotKey(hwnd, hotkey.id as i32);
            }
        }
        active.clear();
    }
    
    fn parse_hotkey_string(hotkey: &str) -> Result<(u32, u32), String> {
        let mut modifiers = 0u32;
        let mut key_code = 0u32;
        let parts: Vec<&str> = hotkey.split('+').collect();
        
        for part in parts {
            match part.trim().to_lowercase().as_str() {
                "alt" => modifiers |= MOD_ALT.0,
                "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "win" | "windows" => modifiers |= MOD_WIN.0,
                "f1" => key_code = VK_F1.0 as u32,
                "f2" => key_code = VK_F2.0 as u32,
                "f3" => key_code = VK_F3.0 as u32,
                "f4" => key_code = VK_F4.0 as u32,
                "f5" => key_code = VK_F5.0 as u32,
                "f6" => key_code = VK_F6.0 as u32,
                "f7" => key_code = VK_F7.0 as u32,
                "f8" => key_code = VK_F8.0 as u32,
                "f9" => key_code = VK_F9.0 as u32,
                "f10" => key_code = VK_F10.0 as u32,
                "f11" => key_code = VK_F11.0 as u32,
                "f12" => key_code = VK_F12.0 as u32,
                "space" => key_code = 0x20,
                "enter" | "return" => key_code = 0x0D,
                "tab" => key_code = 0x09,
                "esc" | "escape" => key_code = 0x1B,
                "delete" | "del" => key_code = 0x2E,
                "insert" | "ins" => key_code = 0x2D,
                "home" => key_code = 0x24,
                "end" => key_code = 0x23,
                "pageup" | "pgup" => key_code = 0x21,
                "pagedown" | "pgdn" => key_code = 0x22,
                "up" => key_code = 0x26,
                "down" => key_code = 0x28,
                "left" => key_code = 0x25,
                "right" => key_code = 0x27,
                "-" | "minus" => key_code = 0xBD,
                "+" | "plus" | "=" => key_code = 0xBB,
                ";" | "semicolon" => key_code = 0xBA,
                "/" | "slash" => key_code = 0xBF,
                "`" | "backtick" | "~" | "tilde" => key_code = 0xC0,
                "[" | "bracketleft" => key_code = 0xDB,
                "\\" | "backslash" => key_code = 0xDC,
                "]" | "bracketright" => key_code = 0xDD,
                "'" | "quote" | "\"" => key_code = 0xDE,
                "," | "comma" | "<" => key_code = 0xBC,
                "." | "period" | ">" => key_code = 0xBE,
                s if s.len() == 1 => {
                    let c = s.chars().next().unwrap();
                    if c.is_ascii_alphabetic() || c.is_ascii_digit() {
                        key_code = c.to_ascii_uppercase() as u32;
                    } else {
                        return Err(format!("Invalid key: {}", s));
                    }
                }
                _ => return Err(format!("Unknown modifier or key: {}", part)),
            }
        }
        
        if key_code == 0 {
            return Err("No valid key specified".to_string());
        }
        
        modifiers |= MOD_NOREPEAT.0;
        Ok((modifiers, key_code))
    }
}

pub fn init_hotkey_system(window: tauri::Window, app_handle: tauri::AppHandle) -> Result<(), String> {
    *APP_HANDLE.lock().unwrap() = Some(app_handle);
    let hwnd = window.hwnd().unwrap().0;
    MAIN_HWND.store(hwnd, Ordering::SeqCst);
    let prev: isize = if cfg!(target_pointer_width = "64") {
        unsafe { SetWindowLongPtrW(HWND(hwnd), GWLP_WNDPROC, custom_wndproc as *const () as isize) }
    } else {
        unsafe { SetWindowLongW(HWND(hwnd), GWL_WNDPROC, custom_wndproc as *const () as i32) as isize }
    };
    *ORIGINAL_WNDPROC.lock().unwrap() = prev;
    Ok(())
}

unsafe extern "system" fn custom_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_HOTKEY {
        let hotkey_id = wparam.0 as u32;
        let action = {
            let active = ACTIVE_HOTKEYS.lock().unwrap();
            active.get(&hotkey_id).map(|h| h.action)
        };
        
        if let Some(action) = action {
            match action {
                HotkeyAction::RadialMenu => {
                    let guard = APP_HANDLE.lock().unwrap();
                    if let Some(app_handle) = guard.as_ref() {
                        if let Some(window) = app_handle.get_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        
                        let mut point = POINT::default();
                        unsafe {
                            let _ = GetCursorPos(&mut point);
                        }
                        let _ = app_handle.emit_all("show-radial", vec![point.x, point.y]);
                    }
                }
                HotkeyAction::ToggleTaskbar => {
                    toggle_taskbar();
                }
            }
        }
        return LRESULT(0);
    }
    
    let orig = *ORIGINAL_WNDPROC.lock().unwrap();
    if orig != 0 {
        CallWindowProcW(
            std::mem::transmute::<isize, WNDPROC>(orig),
            hwnd, msg, wparam, lparam,
        )
    } else {
        CallWindowProcW(None, hwnd, msg, wparam, lparam)
    }
}

fn toggle_taskbar() {
    unsafe {
        let class_name: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
        let hwnd = FindWindowW(
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR(std::ptr::null())
        );
        
        if hwnd.0 != 0 {
            if IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_string() {
        let (mods, key) = GlobalHotkey::parse_hotkey_string("Alt+Space").unwrap();
        assert_eq!(mods, MOD_ALT.0 | MOD_NOREPEAT.0);
        assert_eq!(key, 0x20);

        let (mods, key) = GlobalHotkey::parse_hotkey_string("Ctrl+Shift+A").unwrap();
        assert_eq!(mods, MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0);
        assert_eq!(key, 'A' as u32);
    }
}
