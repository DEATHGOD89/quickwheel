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
struct UserConfig {
    startup: bool,
    theme: String,
    hotkey: String,
    wheel_size: u32,
    animations: bool,
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
            
            // ✅ FIX: Load saved hotkey from config - NO HARDCODED OVERRIDE
            let config_manager = APP_STATE.config.lock().unwrap();
            let config = config_manager.load();
            let saved_hotkey = config.hotkey.clone();
            
            println!("📋 Loaded saved hotkey: '{}'", saved_hotkey);
            
            // Initialize Hotkey System
            let _ = hotkey::init_hotkey_system(window.clone(), app.handle());
            
            // ✅ Register the SAVED hotkey, not hardcoded
            if let Err(e) = hotkey::GlobalHotkey::register(&saved_hotkey) {
                let _ = logger::log_error(&format!("Failed to register hotkey: {}", e));
                // Fallback to default if saved one fails
                if let Err(e2) = hotkey::GlobalHotkey::register("Alt+Space") {
                    let _ = logger::log_error(&format!("Failed to register fallback: {}", e2));
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
        if let Some(b64) = extract_icon_base64(&app.path) {
            app.icon = Some(format!("data:image/png;base64,{}", b64));
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
    // Unregister old hotkey
    hotkey::GlobalHotkey::unregister_all();
    
    // Try to register new hotkey
    if let Err(e) = hotkey::GlobalHotkey::register(&config.hotkey) {
        let _ = logger::log_error(&format!("Failed to register new hotkey: {}", e));
        // Don't save if registration fails
        return Err(format!("Invalid hotkey: {}", e));
    }
    
    // Save only if registration succeeded
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