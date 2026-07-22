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
use std::collections::HashMap;

static ICON_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn get_cached_icon(path: &str) -> Option<String> {
    {
        let cache = ICON_CACHE.lock().unwrap();
        if let Some(icon) = cache.get(path) {
            return Some(icon.clone());
        }
    }
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
struct UserConfig {
    startup: bool,
    theme: String,
    hotkey: String,
    wheel_size: u32,
    animations: bool,
    #[serde(default)]
    taskbar_hotkey: String,
    apps: Vec<AppItem>,
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
            apps: vec![],
        }
    }
}

struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    fn new() -> Self {
        let proj_dirs = ProjectDirs::from("com", "quickwheel", "QuickWheel")
            .expect("Failed to get project directories");
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir).expect("Failed to create config directory");
        let config_path = config_dir.join("config.json");
        ConfigManager { config_path }
    }
    
    fn load(&self) -> UserConfig {
        if !self.config_path.exists() {
            return UserConfig::default();
        }
        let contents = fs::read_to_string(&self.config_path).unwrap_or_default();
        serde_json::from_str(&contents).unwrap_or_default()
    }
    
    fn save(&self, config: &UserConfig) -> Result<(), String> {
        let contents = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
        
        let mut temp_path = self.config_path.clone();
        temp_path.set_extension("tmp");
        
        fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
        fs::rename(&temp_path, &self.config_path).map_err(|e| e.to_string())?;
        
        // Handle Auto-Launch
        if let Ok(current_exe) = std::env::current_exe() {
            let app_name = "QuickWheel";
            let auto = auto_launch::AutoLaunchBuilder::new()
                .set_app_name(app_name)
                .set_app_path(current_exe.to_str().unwrap())
                .set_use_launch_agent(true)
                .build()
                .unwrap();
                
            if config.startup {
                let _ = auto.enable();
            } else {
                let _ = auto.disable();
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

fn main() {
    logger::setup_panic_hook();

    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let settings = CustomMenuItem::new("settings".to_string(), "Settings");
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
                        if let Some(window) = app.get_window("settings") {
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        }
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
            show_radial,
            hide_radial
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
    
    let safe_path = path.replace("'", "''");
    
    let script = format!(
        "$path = '{}'; \
        [System.Reflection.Assembly]::LoadWithPartialName('System.Drawing') | Out-Null; \
        $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($path); \
        if ($icon -eq $null) {{ exit 1; }} \
        $ms = New-Object System.IO.MemoryStream; \
        $icon.ToBitmap().Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); \
        $base64 = [Convert]::ToBase64String($ms.ToArray()); \
        Write-Output $base64;",
        safe_path
    );
    
    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command", &script])
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
fn add_app(mut app: AppItem) -> Result<(), String> {
    if (app.app_type == "exe" || app.app_type == "lnk") && app.icon.is_none() {
        if let Some(b64) = get_cached_icon(&app.path) {
            app.icon = Some(b64);
        }
    }
    let config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    config.apps.push(app);
    config_manager.save(&config)
}

#[tauri::command]
fn remove_app(app_id: String) -> Result<(), String> {
    let config_manager = APP_STATE.config.lock().unwrap();
    let mut config = config_manager.load();
    config.apps.retain(|a| a.id != app_id);
    config_manager.save(&config)
}

#[tauri::command]
fn get_config() -> Result<UserConfig, String> {
    Ok(APP_STATE.config.lock().unwrap().load())
}

#[tauri::command]
fn update_config(config: UserConfig) -> Result<(), String> {
    hotkey::GlobalHotkey::unregister_all();
    
    if let Err(e) = hotkey::GlobalHotkey::register(&config.hotkey, hotkey::HotkeyAction::RadialMenu) {
        let _ = logger::log_error(&format!("Failed to register new hotkey: {}", e));
        return Err(format!("Invalid hotkey: {}", e));
    }
    
    if !config.taskbar_hotkey.is_empty() {
        if let Err(e) = hotkey::GlobalHotkey::register(&config.taskbar_hotkey, hotkey::HotkeyAction::ToggleTaskbar) {
            let _ = logger::log_error(&format!("Failed to register taskbar hotkey: {}", e));
            return Err(format!("Invalid taskbar hotkey: {}", e));
        }
    }
    
    APP_STATE.config.lock().unwrap().save(&config)
}

#[tauri::command]
fn launch_app(path: String) -> Result<(), String> {
    let path_path = std::path::Path::new(&path);
    if !path_path.exists() || !path_path.is_file() {
        return Err("Invalid executable path".to_string());
    }
    
    if path.to_lowercase().ends_with(".lnk") {
        Command::new("cmd")
            .args(&["/C", "start", "", &path])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        Command::new(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn show_radial(window: tauri::Window) {
    window.emit_all("show-radial", ()).unwrap();
}

#[tauri::command]
fn hide_radial(window: tauri::Window) {
    let _ = window.emit_all("hide-radial", ());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let _ = window.hide();
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
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    Ok(())
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
}