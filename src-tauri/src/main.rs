#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::{Manager, SystemTray, SystemTrayEvent, CustomMenuItem, SystemTrayMenu, WindowEvent, SystemTrayMenuItem};
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
mod hotkey;
mod logger;
use once_cell::sync::Lazy;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

use windows::Win32::Foundation::{HWND, LPARAM, BOOL, CloseHandle};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowTextLengthW, IsWindowVisible, GetWindowLongW,
    GWL_STYLE, GWL_EXSTYLE, WS_EX_TOOLWINDOW, GetWindow, GW_OWNER,
    SetForegroundWindow, ShowWindow, SW_RESTORE, GetWindowThreadProcessId
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_FORMAT
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER, KEY_READ,
    RegOpenKeyExW, RegEnumKeyExW, RegQueryValueExW, RegCloseKey,
};
use std::collections::HashMap;

static ICON_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static BROWSER_CACHE: Lazy<Mutex<Option<Vec<BrowserInfo>>>> = Lazy::new(|| Mutex::new(None));
pub static RADIAL_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        
        result.push(CHARSET[(b0 >> 2) as usize] as char);
        result.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(b2 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn extract_icon_native(path: &str) -> Option<String> {
    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut shfi = windows::Win32::UI::Shell::SHFILEINFOW::default();
    
    let res = unsafe {
        windows::Win32::UI::Shell::SHGetFileInfoW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<windows::Win32::UI::Shell::SHFILEINFOW>() as u32,
            windows::Win32::UI::Shell::SHGFI_ICON | windows::Win32::UI::Shell::SHGFI_LARGEICON,
        )
    };
    
    if res == 0 || shfi.hIcon.0 == 0 {
        return None;
    }
    
    let hicon = shfi.hIcon;
    let mut icon_info = windows::Win32::UI::WindowsAndMessaging::ICONINFO::default();
    let success = unsafe { windows::Win32::UI::WindowsAndMessaging::GetIconInfo(hicon, &mut icon_info) };
    if success.is_err() {
        unsafe { let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon); }
        return None;
    }

    let hbm_color = icon_info.hbmColor;
    let mut bmp = windows::Win32::Graphics::Gdi::BITMAP::default();
    let get_bmp = unsafe {
        windows::Win32::Graphics::Gdi::GetObjectW(
            windows::Win32::Graphics::Gdi::HGDIOBJ(hbm_color.0),
            std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut std::ffi::c_void),
        )
    };

    if get_bmp == 0 {
        unsafe {
            if icon_info.hbmColor.0 != 0 { let _ = windows::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmColor); }
            if icon_info.hbmMask.0 != 0 { let _ = windows::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmMask); }
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
        }
        return None;
    }

    let width = bmp.bmWidth;
    let height = bmp.bmHeight;
    let image_size = (width * height * 4) as usize;
    let mut pixels: Vec<u8> = vec![0; image_size];

    let mut bmi = windows::Win32::Graphics::Gdi::BITMAPINFO {
        bmiHeader: windows::Win32::Graphics::Gdi::BITMAPINFOHEADER {
            biSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: windows::Win32::Graphics::Gdi::BI_RGB.0,
            biSizeImage: image_size as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
    };

    let hdc = unsafe { windows::Win32::Graphics::Gdi::CreateCompatibleDC(None) };
    let dib_res = unsafe {
        windows::Win32::Graphics::Gdi::GetDIBits(
            hdc,
            hbm_color,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            windows::Win32::Graphics::Gdi::DIB_RGB_COLORS,
        )
    };

    unsafe {
        let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
        if icon_info.hbmColor.0 != 0 { let _ = windows::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmColor); }
        if icon_info.hbmMask.0 != 0 { let _ = windows::Win32::Graphics::Gdi::DeleteObject(icon_info.hbmMask); }
        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
    }

    if dib_res == 0 {
        return None;
    }

    // Convert to 32-bit BMP with alpha channel
    let mut bmp_file = Vec::with_capacity(54 + image_size);
    // BITMAPFILEHEADER (14 bytes)
    bmp_file.extend_from_slice(b"BM");
    let file_size = (54 + image_size) as u32;
    bmp_file.extend_from_slice(&file_size.to_le_bytes());
    bmp_file.extend_from_slice(&0u32.to_le_bytes()); // reserved
    bmp_file.extend_from_slice(&54u32.to_le_bytes()); // offset to pixel data
    
    // BITMAPINFOHEADER (40 bytes)
    bmp_file.extend_from_slice(&40u32.to_le_bytes());
    bmp_file.extend_from_slice(&(width as i32).to_le_bytes());
    bmp_file.extend_from_slice(&(-(height as i32)).to_le_bytes()); // top-down
    bmp_file.extend_from_slice(&1u16.to_le_bytes()); // planes
    bmp_file.extend_from_slice(&32u16.to_le_bytes()); // bit count
    bmp_file.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB)
    bmp_file.extend_from_slice(&(image_size as u32).to_le_bytes());
    bmp_file.extend_from_slice(&0u32.to_le_bytes());
    bmp_file.extend_from_slice(&0u32.to_le_bytes());
    bmp_file.extend_from_slice(&0u32.to_le_bytes());
    bmp_file.extend_from_slice(&0u32.to_le_bytes());

    // Check if alpha channel has non-zero values
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    if !has_alpha {
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
    }

    bmp_file.extend_from_slice(&pixels);

    let b64 = base64_encode(&bmp_file);
    Some(format!("data:image/bmp;base64,{}", b64))
}

fn get_cached_icon(path: &str) -> Option<String> {
    {
        let cache = ICON_CACHE.lock().unwrap();
        if let Some(icon) = cache.get(path) {
            return Some(icon.clone());
        }
    }

    // 1. Fast Native Win32 extraction (< 0.1ms)
    if let Some(icon_data) = extract_icon_native(path) {
        let mut cache = ICON_CACHE.lock().unwrap();
        cache.insert(path.to_string(), icon_data.clone());
        return Some(icon_data);
    }

    // 2. Fallback asynchronous background extraction
    let path_clone = path.to_string();
    std::thread::spawn(move || {
        if let Some(icon) = extract_icon_base64(&path_clone) {
            let icon_data = format!("data:image/png;base64,{}", icon);
            let mut cache = ICON_CACHE.lock().unwrap();
            cache.insert(path_clone, icon_data);
        }
    });
    None
}

fn extract_icon_sync(path: &str) -> Option<String> {
    // 1. Try Fast Native Win32 extraction first
    if let Some(icon_data) = extract_icon_native(path) {
        ICON_CACHE.lock().unwrap().insert(path.to_string(), icon_data.clone());
        return Some(icon_data);
    }

    // 2. Fallback to PowerShell
    let icon = extract_icon_base64(path)?;
    let icon_data = format!("data:image/png;base64,{}", icon);
    ICON_CACHE.lock().unwrap().insert(path.to_string(), icon_data.clone());
    Some(icon_data)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppItem {
    id: String,
    name: String,
    path: String,
    icon: Option<String>,
    app_type: String,
    category: String,
    order: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RunningApp {
    id: String,
    title: String,
    hwnd: String,
    path: Option<String>,
    icon: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct BrowserInfo {
    name: String,
    exe_path: String,
    icon: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserConfig {
    startup: bool,
    theme: String,
    hotkey: String,
    wheel_size: u32,
    animations: bool,
    #[serde(default)]
    taskbar_hotkey: String,
    #[serde(default = "default_browser_val")]
    default_browser: String,
    #[serde(default = "default_true")]
    enable_searchbar: bool,
    #[serde(default = "default_true")]
    enable_clock: bool,
    #[serde(default)]
    searchbar_x: Option<i32>,
    #[serde(default)]
    searchbar_y: Option<i32>,
    #[serde(default)]
    clock_x: Option<i32>,
    #[serde(default)]
    clock_y: Option<i32>,
    #[serde(default)]
    clock_font: Option<String>,
    apps: Vec<AppItem>,
}

fn default_browser_val() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for UserConfig {
    fn default() -> Self {
        UserConfig {
            startup: true,
            theme: "dark".to_string(),
            hotkey: "Alt+Space".to_string(),
            wheel_size: 350,
            animations: true,
            taskbar_hotkey: "".to_string(),
            default_browser: "default".to_string(),
            enable_searchbar: true,
            enable_clock: true,
            searchbar_x: None,
            searchbar_y: None,
            clock_x: None,
            clock_y: None,
            clock_font: None,
            apps: vec![],
        }
    }
}

struct ConfigManager {
    config_path: PathBuf,
    cached: UserConfig,
}

impl ConfigManager {
    fn new() -> Self {
        let proj_dirs = ProjectDirs::from("com", "quickwheel", "QuickWheel")
            .expect("Failed to get project directories");
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir).expect("Failed to create config directory");
        let config_path = config_dir.join("config.json");
        let cached = if config_path.exists() {
            let contents = fs::read_to_string(&config_path).unwrap_or_default();
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            UserConfig::default()
        };
        ConfigManager { config_path, cached }
    }
    
    fn load(&self) -> UserConfig {
        self.cached.clone()
    }
    
    fn save_config_only(&mut self, config: &UserConfig) -> Result<(), String> {
        let contents = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
        let mut temp_path = self.config_path.clone();
        temp_path.set_extension("tmp");
        fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
        fs::rename(&temp_path, &self.config_path).map_err(|e| e.to_string())?;
        self.cached = config.clone();
        Ok(())
    }
    
    fn save(&mut self, config: &UserConfig) -> Result<(), String> {
        let old_startup = self.cached.startup;
        self.save_config_only(config)?;
        
        // Handle Auto-Launch only if startup setting actually changed
        if old_startup != config.startup {
            if let Ok(current_exe) = std::env::current_exe() {
                let app_name = "QuickWheel";
                if let Ok(auto) = auto_launch::AutoLaunchBuilder::new()
                    .set_app_name(app_name)
                    .set_app_path(current_exe.to_str().unwrap())
                    .set_use_launch_agent(true)
                    .build() {
                    if config.startup {
                        let _ = auto.enable();
                    } else {
                        let _ = auto.disable();
                    }
                }
            }
        }
        
        Ok(())
    }
}

struct AppState {
    config: Mutex<ConfigManager>,
}

static APP_STATE: Lazy<AppState> = Lazy::new(|| AppState {
    config: Mutex::new(ConfigManager::new()),
});

static SINGLETON_HANDLE: Mutex<Option<windows::Win32::Foundation::HANDLE>> = Mutex::new(None);

fn main() {
    // RAM Optimization 1: Enforce single shared Chromium Renderer & memory caps
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--renderer-process-limit=1 --process-per-site --disable-gpu-shader-disk-cache --js-flags=\"--max-old-space-size=64\""
    );

    logger::setup_panic_hook();

    // H1: Single-instance guard (Win32 Named Mutex in Local namespace)
    let mutex_name: Vec<u16> = "Local\\QuickWheel_SingleInstance_Mutex\0".encode_utf16().collect();
    let mutex_handle = unsafe {
        windows::Win32::System::Threading::CreateMutexW(
            None,
            true,
            windows::core::PCWSTR(mutex_name.as_ptr()),
        )
    };
    
    if let Ok(handle) = mutex_handle {
        if handle.0 == 0 {
            println!("Failed to acquire mutex. Exiting.");
            return;
        }
        if let Err(e) = unsafe { windows::Win32::Foundation::GetLastError() } {
            if e.code() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS.to_hresult() {
                println!("Another instance of QuickWheel is already running. Exiting.");
                return;
            }
        }
        *SINGLETON_HANDLE.lock().unwrap() = Some(handle);
    }

    let settings = CustomMenuItem::new("settings".to_string(), "Settings");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let tray_menu = SystemTrayMenu::new()
        .add_item(settings)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(quit);

    let tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            
            let config_manager = APP_STATE.config.lock().unwrap();
            let config = config_manager.load();
            let saved_hotkey = config.hotkey.clone();
            
            println!("📋 Loaded saved hotkey: '{}'", saved_hotkey);
            
            let _ = hotkey::init_hotkey_system(window.clone(), app.handle());
            
            if let Err(e) = hotkey::GlobalHotkey::register(&saved_hotkey, hotkey::HotkeyAction::RadialMenu) {
                let _ = logger::log_error(&format!("Failed to register hotkey: {}", e));
                if let Err(e2) = hotkey::GlobalHotkey::register("Alt+Space", hotkey::HotkeyAction::RadialMenu) {
                    let _ = logger::log_error(&format!("Failed to register fallback: {}", e2));
                }
            }
            
            let taskbar_hotkey = config.taskbar_hotkey.clone();
            if !taskbar_hotkey.is_empty() {
                if let Err(e) = hotkey::GlobalHotkey::register(&taskbar_hotkey, hotkey::HotkeyAction::ToggleTaskbar) {
                    let _ = logger::log_error(&format!("Failed to register taskbar hotkey: {}", e));
                }
            }
            
            if config.enable_searchbar {
                if let Some(searchbar) = app.get_window("searchbar") {
                    let _ = searchbar.show();
                }
            }
            if config.enable_clock {
                if let Some(clock) = app.get_window("clock") {
                    let _ = clock.show();
                }
            }

            // Performance 2: Window Pre-Warming (Pre-load webview GPU buffer for instant < 10ms Alt+Space response)
            let _ = window.show();
            let _ = window.hide();

            Ok(())
        })
        .on_window_event(|event| match event.event() {
            WindowEvent::CloseRequested { api, .. } => {
                event.window().hide().unwrap();
                api.prevent_close();
            }
            _ => {}
        })
        .system_tray(tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => {
                match id.as_str() {
                    "quit" => std::process::exit(0),
                    "settings" => {
                        let _ = open_settings(app.clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_apps,
            add_app,
            remove_app,
            update_config,
            get_config,
            launch_app,
            get_running_apps,
            focus_running_app,
            hide_radial,
            get_browsers,
            search_web,
            save_window_position,
            save_clock_font,
            open_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn save_window_position(label: String, x: i32, y: i32) -> Result<(), String> {
    let mut config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    if label == "searchbar" {
        config.searchbar_x = Some(x);
        config.searchbar_y = Some(y);
    } else if label == "clock" {
        config.clock_x = Some(x);
        config.clock_y = Some(y);
    }
    config_manager.save_config_only(&config)
}

#[tauri::command]
fn save_clock_font(font: String) -> Result<(), String> {
    const VALID_FONTS: &[&str] = &["modern", "system", "tech", "serif", "digital"];
    if !VALID_FONTS.contains(&font.as_str()) {
        return Err("Invalid clock font".to_string());
    }
    let mut config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    config.clock_font = Some(font);
    config_manager.save_config_only(&config)
}

#[tauri::command]
fn get_apps() -> Result<Vec<AppItem>, String> {
    let config = APP_STATE.config.lock().unwrap().load();
    Ok(config.apps)
}

fn extract_icon_base64(path: &str) -> Option<String> {
    let lower_path = path.to_lowercase();
    if !lower_path.ends_with(".exe") && !lower_path.ends_with(".lnk") {
        return None;
    }
    
    let script = r#"
        $path = $env:TARGET_PATH;
        [System.Reflection.Assembly]::LoadWithPartialName('System.Drawing') | Out-Null;
        $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($path);
        if ($icon -eq $null) { exit 1; }
        $ms = New-Object System.IO.MemoryStream;
        $icon.ToBitmap().Save($ms, [System.Drawing.Imaging.ImageFormat]::Png);
        $base64 = [Convert]::ToBase64String($ms.ToArray());
        Write-Output $base64;
    "#;
    
    let output = Command::new("powershell")
        .env("TARGET_PATH", path)
        .args(&["-NoProfile", "-Command", script])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
        
    if output.status.success() {
        let b64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !b64.is_empty() {
            return Some(b64);
        }
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        let _ = logger::log_error(&format!("Icon extraction failed for {}: {}", path, err));
    }
    None
}

#[tauri::command]
fn add_app(app: tauri::AppHandle, mut app_item: AppItem) -> Result<(), String> {
    if (app_item.app_type == "exe" || app_item.app_type == "lnk") && app_item.icon.is_none() {
        if let Some(b64) = extract_icon_sync(&app_item.path) {
            app_item.icon = Some(b64);
        }
    }
    let mut config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    config.apps.push(app_item);
    let res = config_manager.save(&config);
    let _ = app.emit_all("refresh-shortcuts", ());
    res
}

#[tauri::command]
fn remove_app(app: tauri::AppHandle, app_id: String) -> Result<(), String> {
    let mut config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    config.apps.retain(|a| a.id != app_id);
    let res = config_manager.save(&config);
    let _ = app.emit_all("refresh-shortcuts", ());
    res
}

#[tauri::command]
fn get_config() -> Result<UserConfig, String> {
    Ok(APP_STATE.config.lock().unwrap().load())
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = tauri::WindowBuilder::new(
            &app,
            "settings",
            tauri::WindowUrl::App("index.html?settings=true".into()),
        )
        .title("QuickWheel Settings")
        .inner_size(1000.0, 700.0)
        .resizable(true)
        .center()
        .decorations(true)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn update_config(app: tauri::AppHandle, config: UserConfig) -> Result<(), String> {
    let old_config = APP_STATE.config.lock().unwrap().load();
    hotkey::GlobalHotkey::unregister_all();
    
    if let Err(e) = hotkey::GlobalHotkey::register(&config.hotkey, hotkey::HotkeyAction::RadialMenu) {
        let _ = logger::log_error(&format!("Failed to register new hotkey: {}", e));
        // Restore both old hotkeys
        let _ = hotkey::GlobalHotkey::register(&old_config.hotkey, hotkey::HotkeyAction::RadialMenu);
        if !old_config.taskbar_hotkey.is_empty() {
            let _ = hotkey::GlobalHotkey::register(&old_config.taskbar_hotkey, hotkey::HotkeyAction::ToggleTaskbar);
        }
        return Err(format!("Invalid hotkey: {}", e));
    }
    
    if !config.taskbar_hotkey.is_empty() {
        if let Err(e) = hotkey::GlobalHotkey::register(&config.taskbar_hotkey, hotkey::HotkeyAction::ToggleTaskbar) {
            let _ = logger::log_error(&format!("Failed to register taskbar hotkey: {}", e));
            // Restore both old hotkeys
            hotkey::GlobalHotkey::unregister_all();
            let _ = hotkey::GlobalHotkey::register(&old_config.hotkey, hotkey::HotkeyAction::RadialMenu);
            if !old_config.taskbar_hotkey.is_empty() {
                let _ = hotkey::GlobalHotkey::register(&old_config.taskbar_hotkey, hotkey::HotkeyAction::ToggleTaskbar);
            }
            return Err(format!("Invalid taskbar hotkey: {}", e));
        }
    }
    
    // Dynamic widget visibility based on user setting
    if config.enable_searchbar {
        if let Some(w) = app.get_window("searchbar") {
            let _ = w.show();
        }
    } else if let Some(w) = app.get_window("searchbar") {
        let _ = w.hide();
    }

    if config.enable_clock {
        if let Some(w) = app.get_window("clock") {
            let _ = w.show();
        }
    } else if let Some(w) = app.get_window("clock") {
        let _ = w.hide();
    }

    let res = APP_STATE.config.lock().unwrap().save(&config);
    let _ = app.emit_all("refresh-shortcuts", ());
    res
}

#[tauri::command]
fn launch_app(path: String) -> Result<(), String> {
    let path_path = std::path::Path::new(&path);
    if !path_path.exists() || !path_path.is_file() {
        return Err("Invalid executable path".to_string());
    }
    
    if path.to_lowercase().ends_with(".lnk") {
        Command::new("cmd")
            .args(&["/C", "start", "", &format!("\"{}\"", path)])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        Command::new(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn hide_radial(window: tauri::Window) {
    let current_gen = RADIAL_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    let _ = window.emit("hide-radial", ());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if RADIAL_GENERATION.load(std::sync::atomic::Ordering::SeqCst) == current_gen {
            let _ = window.hide();
            // Performance 3: Working Set RAM Trimming (Flushes unused pages to system cache when idle)
            unsafe {
                let proc = windows::Win32::System::Threading::GetCurrentProcess();
                let _ = windows::Win32::System::Threading::SetProcessWorkingSetSize(proc, usize::MAX, usize::MAX);
            }
        }
    });
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let running_apps = &mut *(lparam.0 as *mut Vec<RunningApp>);
    
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    let _style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return BOOL(1);
    }
    if GetWindow(hwnd, GW_OWNER).0 != 0 {
        return BOOL(1);
    }

    // Windows 10/11 DWM Cloaked filtering (skips hidden background UWP apps)
    let mut cloaked: u32 = 0;
    let res = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut _,
        std::mem::size_of::<u32>() as u32,
    );
    if res.is_ok() && cloaked != 0 {
        return BOOL(1);
    }

    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return BOOL(1);
    }

    let mut buf = vec![0u16; (len + 1) as usize];
    let actual_len = GetWindowTextW(hwnd, &mut buf);
    if actual_len == 0 {
        return BOOL(1);
    }

    let title = String::from_utf16_lossy(&buf[..actual_len as usize]);
    let title_trimmed = title.trim();

    if title_trimmed.is_empty() 
        || title_trimmed == "Program Manager" 
        || title_trimmed.starts_with("QuickWheel") 
        || title_trimmed == "Windows Shell Experience Host"
        || title_trimmed == "Settings"
    {
        return BOOL(1);
    }

    let mut process_id = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    
    let mut exe_path: Option<String> = None;
    if process_id != 0 {
        if let Ok(process_handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
            let mut path_buf = vec![0u16; 1024];
            let mut size = path_buf.len() as u32;
            if QueryFullProcessImageNameW(
                process_handle, 
                PROCESS_NAME_FORMAT(0), 
                windows::core::PWSTR(path_buf.as_mut_ptr()), 
                &mut size
            ).is_ok() {
                let path_str = String::from_utf16_lossy(&path_buf[..size as usize]);
                exe_path = Some(path_str);
            }
            let _ = CloseHandle(process_handle);
        }
    }

    // Deduplicate: If an app with the same title or path is already listed, skip duplicate handles
    if running_apps.iter().any(|a| a.title == title_trimmed || (exe_path.is_some() && a.path == exe_path)) {
        return BOOL(1);
    }

    let mut icon_b64: Option<String> = None;
    if let Some(ref path) = exe_path {
        icon_b64 = get_cached_icon(path);
    }

    running_apps.push(RunningApp {
        id: format!("running-{}", hwnd.0),
        title: title_trimmed.to_string(),
        hwnd: hwnd.0.to_string(),
        path: exe_path,
        icon: icon_b64,
    });

    BOOL(1)
}

#[tauri::command]
fn get_running_apps() -> Result<Vec<RunningApp>, String> {
    let mut running_apps: Vec<RunningApp> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut running_apps as *mut _ as isize),
        );
    }
    Ok(running_apps)
}

#[tauri::command]
fn focus_running_app(hwnd_str: String) -> Result<(), String> {
    let hwnd_isize = hwnd_str.parse::<isize>().map_err(|e| e.to_string())?;
    let hwnd = HWND(hwnd_isize);
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            let cur_thread = windows::Win32::System::Threading::GetCurrentThreadId();
            let target_thread = windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, None);
            
            let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(windows::Win32::UI::WindowsAndMessaging::ASFW_ANY);
            let _ = windows::Win32::System::Threading::AttachThreadInput(cur_thread, target_thread, true);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
            let _ = windows::Win32::System::Threading::AttachThreadInput(cur_thread, target_thread, false);
        }
    }
    Ok(())
}

fn reg_get_string(hkey: HKEY, subkey: Option<&str>, value_name: Option<&str>) -> Option<String> {
    unsafe {
        let target_key = if let Some(sub) = subkey {
            let wide_sub: Vec<u16> = sub.encode_utf16().chain(std::iter::once(0)).collect();
            let mut opened = HKEY::default();
            if RegOpenKeyExW(hkey, windows::core::PCWSTR(wide_sub.as_ptr()), 0, KEY_READ, &mut opened).is_err() {
                return None;
            }
            opened
        } else {
            hkey
        };

        let wide_val: Option<Vec<u16>> = value_name.map(|v| v.encode_utf16().chain(std::iter::once(0)).collect());
        let val_ptr = wide_val.as_ref().map(|v| windows::core::PCWSTR(v.as_ptr())).unwrap_or(windows::core::PCWSTR::null());

        let mut data_len = 0u32;
        if RegQueryValueExW(target_key, val_ptr, None, None, None, Some(&mut data_len)).is_err() || data_len == 0 {
            if subkey.is_some() {
                let _ = RegCloseKey(target_key);
            }
            return None;
        }

        let mut buf = vec![0u8; data_len as usize + 2];
        let res = RegQueryValueExW(target_key, val_ptr, None, None, Some(buf.as_mut_ptr()), Some(&mut data_len));
        if subkey.is_some() {
            let _ = RegCloseKey(target_key);
        }

        if res.is_ok() && data_len >= 2 {
            let u16_slice = std::slice::from_raw_parts(buf.as_ptr() as *const u16, (data_len as usize) / 2);
            let s = String::from_utf16_lossy(u16_slice);
            return Some(s.trim_matches('\0').trim().to_string());
        }

        None
    }
}

fn reg_enum_subkeys(root: HKEY, subkey: &str) -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let wide_sub: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut opened = HKEY::default();
        if RegOpenKeyExW(root, windows::core::PCWSTR(wide_sub.as_ptr()), 0, KEY_READ, &mut opened).is_err() {
            return names;
        }

        let mut index = 0u32;
        let mut name_buf = vec![0u16; 256];
        loop {
            let mut name_len = name_buf.len() as u32;
            if RegEnumKeyExW(
                opened,
                index,
                windows::core::PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                windows::core::PWSTR::null(),
                None,
                None,
            ).is_err() {
                break;
            }
            let subkey_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            names.push(subkey_name);
            index += 1;
        }

        let _ = RegCloseKey(opened);
    }
    names
}

#[tauri::command]
fn get_browsers() -> Result<Vec<BrowserInfo>, String> {
    {
        let cache = BROWSER_CACHE.lock().unwrap();
        if let Some(list) = cache.as_ref() {
            return Ok(list
                .iter()
                .map(|b| {
                    let mut resolved = b.clone();
                    if resolved.icon.is_none() && resolved.exe_path != "default" {
                        resolved.icon = get_cached_icon(&resolved.exe_path);
                    }
                    resolved
                })
                .collect());
        }
    }

    let mut browsers: Vec<BrowserInfo> = vec![
        BrowserInfo {
            name: "Default Browser".to_string(),
            exe_path: "default".to_string(),
            icon: None,
        }
    ];

    let roots = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE];
    let mut seen_exes = std::collections::HashSet::new();

    for root in roots {
        let subkeys = reg_enum_subkeys(root, "SOFTWARE\\Clients\\StartMenuInternet");
        for sub in subkeys {
            let base_path = format!("SOFTWARE\\Clients\\StartMenuInternet\\{}", sub);
            let display_name = reg_get_string(root, Some(&base_path), None)
                .unwrap_or_else(|| sub.clone());
            let cmd_path = format!("{}\\shell\\open\\command", base_path);
            if let Some(cmd_val) = reg_get_string(root, Some(&cmd_path), None) {
                let trimmed = cmd_val.trim();
                let exe = if trimmed.starts_with('"') {
                    trimmed.split('"').nth(1).unwrap_or("").to_string()
                } else {
                    trimmed.split_whitespace().next().unwrap_or("").to_string()
                };

                if !exe.is_empty() && std::path::Path::new(&exe).exists() {
                    let exe_lower = exe.to_lowercase();
                    if !seen_exes.contains(&exe_lower) {
                        seen_exes.insert(exe_lower);
                        let icon = get_cached_icon(&exe);
                        browsers.push(BrowserInfo {
                            name: display_name,
                            exe_path: exe,
                            icon,
                        });
                    }
                }
            }
        }
    }

    *BROWSER_CACHE.lock().unwrap() = Some(browsers.clone());
    Ok(browsers)
}

#[tauri::command]
fn search_web(query: String, browser_path: Option<String>) -> Result<(), String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let encoded_query = utf8_percent_encode(trimmed);
    let search_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://www.google.com/search?q={}", encoded_query)
    };

    if let Some(ref path) = browser_path {
        if !path.is_empty() && path != "default" && std::path::Path::new(path).exists() {
            Command::new(path)
                .arg(&search_url)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    Command::new("cmd")
        .args(&["/C", "start", "", &search_url])
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn utf8_percent_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[test]
    fn test_user_config_default_and_serialize() {
        let config = UserConfig::default();
        assert_eq!(config.startup, true);
        assert_eq!(config.theme, "dark");
        assert_eq!(config.hotkey, "Alt+Space");

        let json = serde_json::to_string(&config).unwrap();
        let restored: UserConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.hotkey, "Alt+Space");
    }

    #[test]
    fn test_icon_cache_concurrency() {
        use std::thread;

        let mut handles = vec![];
        for i in 0..10 {
            let handle = thread::spawn(move || {
                let cache_key = format!("c:\\test_path_{}.exe", i % 3);
                let mut cache = ICON_CACHE.lock().unwrap();
                cache.insert(cache_key.clone(), "data:image/png;base64,dummy".to_string());
                assert!(cache.contains_key(&cache_key));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_get_running_apps_execution() {
        let res = get_running_apps();
        assert!(res.is_ok());
    }

    #[test]
    fn test_utf8_percent_encode() {
        assert_eq!(utf8_percent_encode("hello world"), "hello+world");
        assert_eq!(utf8_percent_encode("rust & tauri"), "rust+%26+tauri");
    }

    #[test]
    fn test_get_browsers_native() {
        let res = get_browsers();
        assert!(res.is_ok(), "get_browsers should succeed");
        let browsers = res.unwrap();
        assert!(!browsers.is_empty(), "Should have at least the Default Browser entry");
        assert_eq!(browsers[0].name, "Default Browser");
        assert_eq!(browsers[0].exe_path, "default");
        // Verify no empty paths in discovered browsers
        for b in &browsers {
            assert!(!b.name.is_empty(), "Browser name must not be empty");
            assert!(!b.exe_path.is_empty(), "Browser exe_path must not be empty");
        }
    }

    #[test]
    fn test_config_manager_caching() {
        let temp_dir = std::env::temp_dir().join(format!("qw_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("config.json");
        
        let mut cm = ConfigManager {
            config_path: config_path.clone(),
            cached: UserConfig::default(),
        };

        // Cache read should match default
        let loaded = cm.load();
        assert_eq!(loaded.hotkey, "Alt+Space");

        // Save new config and verify in-memory cache is updated without re-reading
        let mut updated = loaded.clone();
        updated.hotkey = "Ctrl+Shift+Z".to_string();
        let save_res = cm.save_config_only(&updated);
        assert!(save_res.is_ok());

        assert_eq!(cm.load().hotkey, "Ctrl+Shift+Z");
        assert!(config_path.exists());
        
        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_radial_generation_atomic() {
        use std::sync::atomic::Ordering;
        let gen1 = RADIAL_GENERATION.load(Ordering::SeqCst);
        let gen2 = RADIAL_GENERATION.fetch_add(1, Ordering::SeqCst);
        assert_eq!(gen2, gen1);
        assert_eq!(RADIAL_GENERATION.load(Ordering::SeqCst), gen1 + 1);
    }
}