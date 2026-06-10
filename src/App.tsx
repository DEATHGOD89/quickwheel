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

function RadialMenu() {
  const [visible, setVisible] = useState(false);
  const [fadingOut, setFadingOut] = useState(false);
  const [apps, setApps] = useState<AppItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const visibleRef = useRef(false);
  
  useEffect(() => {
    visibleRef.current = visible && !fadingOut;
  }, [visible, fadingOut]);
  
  useEffect(() => {
    loadApps();
    
    let unlistenShow: () => void;
    let unlistenHide: () => void;
    
    const setupListeners = async () => {
      const showPromise = listen("show-radial", async () => {
        await loadApps();
        setFadingOut(false);
        setVisible(true);
      });
      
      const hidePromise = listen("hide-radial", () => {
        setFadingOut(true);
        setTimeout(() => {
          setVisible(false);
          setFadingOut(false);
          setSelectedIndex(-1);
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
  
  const loadApps = async () => {
    const appList = await invoke<AppItem[]>("get_apps");
    setApps(appList);
  };
  
  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!visibleRef.current || apps.length === 0) return;
    
    const center = { x: window.innerWidth / 2, y: window.innerHeight / 2 };
    const dx = e.clientX - center.x;
    const dy = e.clientY - center.y;
    const dist = Math.sqrt(dx * dx + dy * dy);
    
    if (dist < 60) {
      setSelectedIndex(-1);
      return;
    }
    
    let angle = Math.atan2(dy, dx);
    if (angle < 0) angle += Math.PI * 2;
    
    const angleStep = (Math.PI * 2) / apps.length;
    let index = Math.floor((angle + angleStep / 2) / angleStep);
    index = index % apps.length;
    
    setSelectedIndex(index);
  }, [apps]);
  
  const handleLaunch = useCallback(async (_e: MouseEvent) => {
    if (!visibleRef.current) return;
    
    if (selectedIndex >= 0 && apps[selectedIndex]) {
      const app = apps[selectedIndex];
      await invoke("launch_app", { path: app.path });
      await invoke("hide_radial");
    } else {
      await invoke("hide_radial");
    }
  }, [selectedIndex, apps]);
  
  useEffect(() => {
    if (!visible) return;
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleLaunch);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleLaunch);
    };
  }, [visible, handleMouseMove, handleLaunch]);
  
  if (!visible && !fadingOut) return null;
  
  const radius = 150;

  return (
    <div className={`arc-reactor-overlay ${fadingOut ? "fade-out" : "fade-in"}`}>
      {apps.length === 0 ? (
        <div className="no-apps-message">
          <div className="arc-core-empty"></div>
          <p>No apps added! Right-click tray -&gt; Settings</p>
        </div>
      ) : (
        <div className="arc-reactor-container">
          <div className="arc-ring arc-outer"></div>
          <div className="arc-ring arc-inner"></div>
          <div className="arc-core-center"></div>
          
          {apps.map((app, i) => {
            const angleStep = (Math.PI * 2) / apps.length;
            const angle = i * angleStep;
            
            const x = Math.cos(angle) * radius;
            const y = Math.sin(angle) * radius;
            const isSelected = i === selectedIndex;
            
            return (
              <div
                key={app.id}
                className={`arc-app-node ${isSelected ? "selected" : ""}`}
                style={{
                  left: x,
                  top: y,
                  transform: 'translate(-50%, -50%) scale(' + (isSelected ? 1.2 : 1) + ')'
                }}
              >
                <div className="app-icon-container">
                  {app.icon ? (
                    <img src={app.icon} alt={app.name} className="app-icon-img" />
                  ) : (
                    <div className="app-icon-placeholder">{app.name.charAt(0)}</div>
                  )}
                </div>
                {isSelected && (
                  <div className="app-name-label">{app.name}</div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
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
        filters: [{
          name: 'Applications & Shortcuts',
          extensions: ['exe', 'lnk']
        }]
      });
      
      if (selected) {
        const files = Array.isArray(selected) ? selected : [selected];
        for (const path of files) {
          const name = path.split('\\').pop()?.replace(".exe", "").replace(".lnk", "") || "App";
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

  return (
    <div className="settings-container">
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
              <label>Hotkey</label>
              <input
                type="text"
                value={config.hotkey}
                onChange={async (e) => {
                  const newConfig = { ...config, hotkey: e.target.value };
                  await invoke("update_config", { config: newConfig });
                  await loadConfig();
                }}
              />
            </div>
            <div className="setting">
              <label>
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
              <br/><br/>
              <button onClick={handleBrowse} style={{ padding: '8px 16px', background: '#0d7377', color: 'white', border: 'none', borderRadius: '4px', cursor: 'pointer' }}>
                Browse for App...
              </button>
            </div>
            <div className="apps-list">
              {apps.map((app) => (
                <div key={app.id} className="app-item">
                  <span>{app.name}</span>
                  <button onClick={async () => {
                    try {
                      await invoke("remove_app", { appId: app.id, app_id: app.id });
                      await loadApps();
                    } catch (e) {
                      console.error("Failed to remove app:", e);
                    }
                  }}>✕</button>
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

  return (
    <div className="app">
      {isSettings ? <Settings /> : <RadialMenu />}
    </div>
  );
}

export default App;