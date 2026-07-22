import React, { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/api/dialog";
import "./App.css";

interface AppItem {
  id: string;
  name: string;
  path: string;
  icon?: string;
  app_type: string;
  category: string;
  order: number;
}

interface RunningApp {
  id: string;
  title: string;
  hwnd: string;
  path?: string;
  icon?: string;
}

interface RadialSelection {
  ring: "outer" | "inner" | null;
  index: number;
}

function RadialMenu() {
  const [visible, setVisible] = useState(false);
  const [fadingOut, setFadingOut] = useState(false);
  const [shortcuts, setShortcuts] = useState<AppItem[]>([]);
  const [runningApps, setRunningApps] = useState<RunningApp[]>([]);
  const [selection, setSelection] = useState<RadialSelection>({ ring: null, index: -1 });
  const [theme, setTheme] = useState("black");
  
  const visibleRef = useRef(false);
  const shortcutsRef = useRef<AppItem[]>([]);
  const runningAppsRef = useRef<RunningApp[]>([]);
  const selectionRef = useRef<RadialSelection>({ ring: null, index: -1 });

  useEffect(() => {
    visibleRef.current = visible && !fadingOut;
  }, [visible, fadingOut]);

  useEffect(() => {
    shortcutsRef.current = shortcuts;
  }, [shortcuts]);

  useEffect(() => {
    runningAppsRef.current = runningApps;
  }, [runningApps]);

  useEffect(() => {
    selectionRef.current = selection;
  }, [selection]);

  const loadThemeAndShortcuts = async () => {
    try {
      const config = await invoke<any>("get_config");
      if (config && config.theme) {
        setTheme(config.theme.toLowerCase() === "white" ? "white" : "black");
      }
      const appList = await invoke<AppItem[]>("get_apps");
      setShortcuts(appList || []);
    } catch (e) {
      console.error("Error loading shortcuts:", e);
    }
  };

  const loadRunningAppsAsync = async () => {
    try {
      const running = await invoke<RunningApp[]>("get_running_apps");
      setRunningApps(running || []);
    } catch (e) {
      console.error("Error loading running apps:", e);
    }
  };

  useEffect(() => {
    loadThemeAndShortcuts();

    let unlistenShow: () => void;
    let unlistenHide: () => void;

    const setupListeners = async () => {
      const showPromise = listen("show-radial", () => {
        // INSTANT APPEARANCE: Show UI immediately without waiting!
        setFadingOut(false);
        setVisible(true);

        // Load shortcuts & running apps in background
        loadThemeAndShortcuts();
        loadRunningAppsAsync();
      });

      const hidePromise = listen("hide-radial", () => {
        setFadingOut(true);
        setTimeout(() => {
          setVisible(false);
          setFadingOut(false);
          setSelection({ ring: null, index: -1 });
        }, 200);
      });

      unlistenShow = await showPromise;
      unlistenHide = await hidePromise;
    };

    setupListeners();

    return () => {
      if (unlistenShow) unlistenShow();
      if (unlistenHide) unlistenHide();
    };
  }, []);

  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!visibleRef.current) return;

    const center = { x: window.innerWidth / 2, y: window.innerHeight / 2 };
    const dx = e.clientX - center.x;
    const dy = e.clientY - center.y;
    const dist = Math.sqrt(dx * dx + dy * dy);

    // Center Deadzone: < 65px
    if (dist < 65) {
      setSelection({ ring: null, index: -1 });
      return;
    }

    let angle = Math.atan2(dy, dx);
    if (angle < 0) angle += Math.PI * 2;

    const currentShortcuts = shortcutsRef.current;
    const currentRunning = runningAppsRef.current;

    // Inner Ring: Distance between 65px and 175px (Live Running Taskbar Apps)
    if (dist >= 65 && dist < 175) {
      if (currentRunning.length === 0) {
        setSelection({ ring: null, index: -1 });
        return;
      }
      const angleStep = (Math.PI * 2) / currentRunning.length;
      let idx = Math.floor((angle + angleStep / 2) / angleStep) % currentRunning.length;
      setSelection({ ring: "inner", index: idx });
      return;
    }

    // Outer Ring: Distance >= 175px (Configured App Shortcuts)
    if (dist >= 175) {
      if (currentShortcuts.length === 0) {
        setSelection({ ring: null, index: -1 });
        return;
      }
      const angleStep = (Math.PI * 2) / currentShortcuts.length;
      let idx = Math.floor((angle + angleStep / 2) / angleStep) % currentShortcuts.length;
      setSelection({ ring: "outer", index: idx });
      return;
    }
  }, []);

  const handleLaunch = useCallback(async (_e: MouseEvent) => {
    if (!visibleRef.current) return;

    const { ring, index } = selectionRef.current;

    if (ring === "inner" && index >= 0 && runningAppsRef.current[index]) {
      const app = runningAppsRef.current[index];
      await invoke("focus_running_app", { hwndStr: app.hwnd, hwnd_str: app.hwnd });
      await invoke("hide_radial");
    } else if (ring === "outer" && index >= 0 && shortcutsRef.current[index]) {
      const app = shortcutsRef.current[index];
      await invoke("launch_app", { path: app.path });
      await invoke("hide_radial");
    } else {
      // Empty space / deadzone clicked
      await invoke("hide_radial");
    }
  }, []);

  useEffect(() => {
    if (!visible) return;
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleLaunch);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleLaunch);
    };
  }, [visible, handleMouseMove, handleLaunch]);

  if (!visible && !fadingOut) return null;

  const outerRadius = 220;
  const innerRadius = 130;

  const handleBackdropClick = async (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      await invoke("hide_radial");
    }
  };

  return (
    <div
      className={`arc-reactor-overlay theme-${theme} ${fadingOut ? "fade-out" : "fade-in"}`}
      onClick={handleBackdropClick}
    >
      {shortcuts.length === 0 && runningApps.length === 0 ? (
        <div className="no-apps-message" onClick={handleBackdropClick}>
          <div className="arc-core-empty"></div>
          <p>No shortcuts or running apps found!</p>
        </div>
      ) : (
        <div className="arc-reactor-container">
          {/* Dual Ring Visual Boundaries */}
          <div className="arc-ring arc-outer"></div>
          <div className="arc-ring arc-inner"></div>
          <div className="arc-core-center" onClick={handleBackdropClick}></div>

          {/* Outer Ring: Configured App Shortcuts */}
          {shortcuts.map((app, i) => {
            const angleStep = (Math.PI * 2) / shortcuts.length;
            const angle = i * angleStep;
            const x = Math.cos(angle) * outerRadius;
            const y = Math.sin(angle) * outerRadius;
            const isSelected = selection.ring === "outer" && selection.index === i;

            return (
              <div
                key={`shortcut-${app.id}`}
                className={`arc-app-node outer-node ${isSelected ? "selected" : ""}`}
                style={{
                  left: x,
                  top: y,
                  transform: `translate(-50%, -50%) scale(${isSelected ? 1.25 : 1})`,
                }}
              >
                <div className="app-icon-container">
                  {app.icon ? (
                    <img src={app.icon} alt={app.name} className="app-icon-img" />
                  ) : (
                    <div className="app-icon-placeholder">{app.name.charAt(0)}</div>
                  )}
                </div>
                {isSelected && <div className="app-name-label">{app.name}</div>}
              </div>
            );
          })}

          {/* Inner Ring: Live Open Taskbar Apps */}
          {runningApps.map((run, i) => {
            const angleStep = (Math.PI * 2) / runningApps.length;
            const angle = i * angleStep;
            const x = Math.cos(angle) * innerRadius;
            const y = Math.sin(angle) * innerRadius;
            const isSelected = selection.ring === "inner" && selection.index === i;

            return (
              <div
                key={`running-${run.id}`}
                className={`arc-app-node inner-node running-app ${isSelected ? "selected" : ""}`}
                style={{
                  left: x,
                  top: y,
                  transform: `translate(-50%, -50%) scale(${isSelected ? 1.25 : 1})`,
                }}
              >
                <div className="app-icon-container">
                  {run.icon ? (
                    <img src={run.icon} alt={run.title} className="app-icon-img" />
                  ) : (
                    <div className="app-icon-placeholder">{run.title.charAt(0)}</div>
                  )}
                  <div className="running-indicator-dot" title="Active Taskbar App"></div>
                </div>
                {isSelected && <div className="app-name-label">🟢 {run.title}</div>}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function HotkeyInput({ value, onChange, placeholder = "" }: { value: string; onChange: (val: string) => void; placeholder?: string }) {
  const [recording, setRecording] = useState(false);
  const [tempKeys, setTempKeys] = useState<string[]>([]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    e.preventDefault();
    const keys = [];
    if (e.ctrlKey) keys.push("Ctrl");
    if (e.altKey) keys.push("Alt");
    if (e.shiftKey) keys.push("Shift");
    if (e.metaKey) keys.push("Win");

    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      setTempKeys(keys);
      return;
    }

    if (e.key === "Backspace" && keys.length === 0) {
      onChange("");
      setRecording(false);
      setTempKeys([]);
      return;
    }

    let key = e.key;
    if (key === " ") key = "Space";
    else if (key.length === 1) key = key.toUpperCase();

    const keyMap: Record<string, string> = {
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Escape: "Esc",
      Delete: "Del",
      Insert: "Ins",
      PageUp: "PageUp",
      PageDown: "PageDown",
    };

    if (keyMap[key]) key = keyMap[key];

    keys.push(key);
    const finalHotkey = keys.join("+");
    onChange(finalHotkey);
    setRecording(false);
    setTempKeys([]);
  };

  const handleKeyUp = (e: React.KeyboardEvent) => {
    e.preventDefault();
    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      const keys = [];
      if (e.ctrlKey) keys.push("Ctrl");
      if (e.altKey) keys.push("Alt");
      if (e.shiftKey) keys.push("Shift");
      if (e.metaKey) keys.push("Win");
      setTempKeys(keys);
    }
  };

  let displayValue = value;
  if (recording) {
    displayValue = tempKeys.length > 0 ? tempKeys.join("+") + "+" : "Press shortcut...";
  } else if (!value && placeholder) {
    displayValue = "";
  }

  return (
    <input
      type="text"
      value={displayValue}
      placeholder={placeholder}
      onFocus={() => setRecording(true)}
      onBlur={() => {
        setRecording(false);
        setTempKeys([]);
      }}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      readOnly
      className="hotkey-input"
    />
  );
}

function Settings() {
  const [config, setConfig] = useState<any>(null);
  const [apps, setApps] = useState<AppItem[]>([]);
  const [activeTab, setActiveTab] = useState("general");

  useEffect(() => {
    loadConfig();
    loadApps();
  }, []);

  const loadConfig = async () => {
    const cfg = await invoke("get_config");
    setConfig(cfg);
  };

  const loadApps = async () => {
    const appList = await invoke<AppItem[]>("get_apps");
    setApps(appList);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    const files = Array.from(e.dataTransfer.files);

    for (const file of files) {
      const fileWithPath = file as { path?: string };
      if (fileWithPath.path && (fileWithPath.path.endsWith(".exe") || fileWithPath.path.endsWith(".lnk"))) {
        const newApp: AppItem = {
          id: crypto.randomUUID(),
          name: file.name.replace(".exe", "").replace(".lnk", ""),
          path: fileWithPath.path,
          icon: undefined,
          app_type: fileWithPath.path.endsWith(".lnk") ? "lnk" : "exe",
          category: "general",
          order: apps.length,
        };
        await invoke("add_app", { app: newApp });
        await loadApps();
      }
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
  };

  const handleBrowse = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "Applications & Shortcuts",
            extensions: ["exe", "lnk"],
          },
        ],
      });

      if (selected) {
        const files = Array.isArray(selected) ? selected : [selected];
        for (const path of files) {
          const name = path.split("\\").pop()?.replace(".exe", "").replace(".lnk", "") || "App";
          const newApp: AppItem = {
            id: crypto.randomUUID(),
            name: name,
            path: path,
            icon: undefined,
            app_type: "exe",
            category: "general",
            order: apps.length,
          };
          await invoke("add_app", { app: newApp });
        }
        await loadApps();
      }
    } catch (e) {
      console.error(e);
    }
  };

  if (!config) return <div className="loading">Loading...</div>;

  const currentTheme = config.theme ? config.theme.toLowerCase() : "black";

  return (
    <div className={`settings-container theme-${currentTheme}`}>
      <div className="sidebar">
        <button className={`tab ${activeTab === "general" ? "active" : ""}`} onClick={() => setActiveTab("general")}>
          ⚙️ General
        </button>
        <button className={`tab ${activeTab === "apps" ? "active" : ""}`} onClick={() => setActiveTab("apps")}>
          📱 Apps
        </button>
      </div>

      <div className="content">
        {activeTab === "general" && (
          <div className="section">
            <h2>General Settings</h2>

            <div className="setting">
              <label>Theme Style</label>
              <div className="theme-selector-group">
                <button
                  className={`theme-btn ${currentTheme === "black" ? "selected" : ""}`}
                  onClick={async () => {
                    const newConfig = { ...config, theme: "black" };
                    await invoke("update_config", { config: newConfig });
                    await loadConfig();
                  }}
                >
                  ⬛ Black Minimal
                </button>
                <button
                  className={`theme-btn ${currentTheme === "white" ? "selected" : ""}`}
                  onClick={async () => {
                    const newConfig = { ...config, theme: "white" };
                    await invoke("update_config", { config: newConfig });
                    await loadConfig();
                  }}
                >
                  ⬜ White Minimal
                </button>
              </div>
            </div>

            <div className="setting">
              <label>Radial Menu Hotkey</label>
              <HotkeyInput
                value={config.hotkey}
                onChange={async (val) => {
                  if (!val) return;
                  const newConfig = { ...config, hotkey: val };
                  try {
                    await invoke("update_config", { config: newConfig });
                    await loadConfig();
                  } catch (e) {
                    console.error("Failed to update hotkey:", e);
                  }
                }}
              />
            </div>

            <div className="setting">
              <label>Taskbar Toggle Hotkey</label>
              <HotkeyInput
                placeholder="Leave empty to disable"
                value={config.taskbar_hotkey || ""}
                onChange={async (val) => {
                  const newConfig = { ...config, taskbar_hotkey: val };
                  try {
                    await invoke("update_config", { config: newConfig });
                    await loadConfig();
                  } catch (e) {
                    console.error("Failed to update hotkey:", e);
                  }
                }}
              />
            </div>

            <div className="setting">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={config.startup}
                  onChange={async (e) => {
                    const newConfig = { ...config, startup: e.target.checked };
                    await invoke("update_config", { config: newConfig });
                    await loadConfig();
                  }}
                />
                Start with Windows
              </label>
            </div>
          </div>
        )}

        {activeTab === "apps" && (
          <div className="section dropzone" onDragOver={handleDragOver} onDrop={handleDrop}>
            <h2>Applications</h2>
            <div className="drop-area">
              📂 Drag & Drop .exe files here
              <br />
              <br />
              <button onClick={handleBrowse} className="browse-btn">
                Browse for App...
              </button>
            </div>
            <div className="apps-list">
              {apps.map((app) => (
                <div key={app.id} className="app-item">
                  <span>{app.name}</span>
                  <button
                    onClick={async () => {
                      try {
                        await invoke("remove_app", { appId: app.id, app_id: app.id });
                        await loadApps();
                      } catch (e) {
                        console.error("Failed to remove app:", e);
                      }
                    }}
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function App() {
  const isSettings = window.location.search.includes("settings=true");

  return <div className="app">{isSettings ? <Settings /> : <RadialMenu />}</div>;
}

export default App;