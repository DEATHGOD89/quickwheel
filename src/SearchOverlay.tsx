import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { appWindow, PhysicalPosition } from "@tauri-apps/api/window";

export interface BrowserInfo {
  name: string;
  exe_path: string;
  icon?: string;
}

export function DesktopSearchBarWidget() {
  const [query, setQuery] = useState("");
  const [browsers, setBrowsers] = useState<BrowserInfo[]>([
    { name: "Default Browser", exe_path: "default" }
  ]);
  const [selectedBrowserIndex, setSelectedBrowserIndex] = useState(0);
  const [theme, setTheme] = useState("black");
  const [isLocked, setIsLocked] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [showBrowsers, setShowBrowsers] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let unlistenMove: (() => void) | undefined;

    const loadData = async () => {
      try {
        const config = await invoke<any>("get_config");
        if (config && config.theme) {
          setTheme(config.theme.toLowerCase() === "white" ? "white" : "black");
        }

        // Restore saved position if available
        if (config && config.searchbar_x !== undefined && config.searchbar_y !== undefined && config.searchbar_x !== null && config.searchbar_y !== null) {
          await appWindow.setPosition(new PhysicalPosition(config.searchbar_x, config.searchbar_y));
        }

        const list = await invoke<BrowserInfo[]>("get_browsers");
        if (list && list.length > 0) {
          setBrowsers(list);
          if (config && config.default_browser) {
            const idx = list.findIndex(b => b.exe_path === config.default_browser);
            if (idx >= 0) {
              setSelectedBrowserIndex(idx);
            }
          }
        }
      } catch (e) {
        console.error("Error initializing desktop search bar:", e);
      }
    };

    loadData();

    // Listen for window move to auto-save position with 500ms debounce
    let moveTimeout: any = null;
    appWindow.onMoved(({ payload: pos }) => {
      if (moveTimeout) clearTimeout(moveTimeout);
      moveTimeout = setTimeout(() => {
        invoke("save_window_position", {
          label: "searchbar",
          x: pos.x,
          y: pos.y
        }).catch((err) => console.error("Error saving searchbar position:", err));
      }, 500);
    }).then((unlisten) => {
      unlistenMove = unlisten;
    });

    return () => {
      if (moveTimeout) clearTimeout(moveTimeout);
      if (unlistenMove) unlistenMove();
    };
  }, []);

  const toggleAlwaysOnTop = async () => {
    const nextState = !alwaysOnTop;
    setAlwaysOnTop(nextState);
    try {
      await appWindow.setAlwaysOnTop(nextState);
    } catch (e) {
      console.error(e);
    }
  };

  const toggleShowBrowsers = () => {
    setShowBrowsers(!showBrowsers);
  };

  const handleStartDrag = (e: React.MouseEvent) => {
    if (e.button === 0 && !isLocked) {
      const target = e.target as HTMLElement;
      if (target.tagName !== "INPUT" && target.tagName !== "BUTTON" && !target.closest("button")) {
        appWindow.startDragging();
      }
    }
  };

  const handleSearchSubmit = async () => {
    if (!query.trim()) return;
    const currentBrowser = browsers[selectedBrowserIndex] || browsers[0];
    try {
      await invoke("search_web", {
        query: query.trim(),
        browserPath: currentBrowser.exe_path
      });
      setQuery("");
    } catch (e) {
      console.error("Error searching web:", e);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleSearchSubmit();
    }
  };

  return (
    <div 
      className={`desktop-search-widget-container theme-${theme}`}
      onMouseDown={handleStartDrag}
    >
      <div 
        className="glass-search-box"
        onMouseDown={handleStartDrag}
        data-tauri-drag-region={!isLocked ? "true" : undefined}
        title={!isLocked ? "Drag to move search bar anywhere on desktop" : "Position locked"}
      >
        {!isLocked && (
          <div className="drag-handle-grip" onMouseDown={handleStartDrag} data-tauri-drag-region title="Drag to move">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
              <circle cx="8" cy="6" r="1.5" />
              <circle cx="16" cy="6" r="1.5" />
              <circle cx="8" cy="12" r="1.5" />
              <circle cx="16" cy="12" r="1.5" />
              <circle cx="8" cy="18" r="1.5" />
              <circle cx="16" cy="18" r="1.5" />
            </svg>
          </div>
        )}

        <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="11" cy="11" r="8" />
          <path d="M21 21l-4.35-4.35" />
        </svg>

        <input
          ref={inputRef}
          type="text"
          className="search-input"
          placeholder="Search web directly from desktop..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
        />

        {/* Action Controls */}
        <div className="widget-controls">
          <button 
            className={`widget-control-btn ${showBrowsers ? "active" : ""}`}
            onClick={toggleShowBrowsers}
            title={showBrowsers ? "Hide Browser Selector" : "Choose Target Browser"}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="2" y1="12" x2="22" y2="12" />
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
          </button>
          <button 
            className={`widget-control-btn ${alwaysOnTop ? "active" : ""}`}
            onClick={toggleAlwaysOnTop}
            title={alwaysOnTop ? "Pinned Above All Windows" : "Desktop Wallpaper Mode"}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="17" x2="12" y2="22" />
              <path d="M5 17h14l-1.7-6.8a2 2 0 0 1-.3-.9V4a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v5.3a2 2 0 0 1-.3.9L5 17z" />
            </svg>
          </button>
          <button 
            className={`widget-control-btn ${isLocked ? "active" : ""}`}
            onClick={() => setIsLocked(!isLocked)}
            title={isLocked ? "Locked in Place (Click to Unlock & Move)" : "Unlocked (Drag to Move)"}
          >
            {isLocked ? (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0 1 10 0v4" />
              </svg>
            ) : (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0 1 9.9-1" />
              </svg>
            )}
          </button>
        </div>
      </div>

      {/* Browser Selector Slider Bar */}
      {showBrowsers && (
        <div className="browser-slider-bar">
          {browsers.map((browser, index) => {
            const isSelected = index === selectedBrowserIndex;
            return (
              <button
                key={browser.exe_path + index}
                className={`browser-chip ${isSelected ? "active" : ""}`}
                onClick={() => setSelectedBrowserIndex(index)}
                title={`Search in ${browser.name}`}
              >
                {browser.icon ? (
                  <img src={browser.icon} alt="" className="browser-chip-icon" />
                ) : (
                  <svg className="browser-chip-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="2" y1="12" x2="22" y2="12" />
                    <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                  </svg>
                )}
                <span>{browser.name}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

interface SearchOverlayProps {
  theme: string;
  onSwitchToRadial: () => void;
  onClose: () => void;
}

export function SearchOverlay({ theme, onSwitchToRadial, onClose }: SearchOverlayProps) {
  const [query, setQuery] = useState("");
  const [browsers, setBrowsers] = useState<BrowserInfo[]>([
    { name: "Default Browser", exe_path: "default" }
  ]);
  const [selectedBrowserIndex, setSelectedBrowserIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setTimeout(() => {
      inputRef.current?.focus();
    }, 50);

    const loadBrowsers = async () => {
      try {
        const list = await invoke<BrowserInfo[]>("get_browsers");
        if (list && list.length > 0) {
          setBrowsers(list);
          const config = await invoke<any>("get_config");
          if (config && config.default_browser) {
            const idx = list.findIndex(b => b.exe_path === config.default_browser);
            if (idx >= 0) {
              setSelectedBrowserIndex(idx);
            }
          }
        }
      } catch (e) {
        console.error("Error loading browsers:", e);
      }
    };

    loadBrowsers();
  }, []);

  const handleSearchSubmit = async () => {
    if (!query.trim()) return;
    const currentBrowser = browsers[selectedBrowserIndex] || browsers[0];
    try {
      await invoke("search_web", {
        query: query.trim(),
        browserPath: currentBrowser.exe_path
      });
      await invoke("hide_radial");
    } catch (e) {
      console.error("Error searching web:", e);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleSearchSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Tab") {
      e.preventDefault();
      onSwitchToRadial();
    } else if (e.altKey && (e.key === "ArrowRight" || e.key === "ArrowDown")) {
      e.preventDefault();
      setSelectedBrowserIndex((prev) => (prev + 1) % browsers.length);
    } else if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowUp")) {
      e.preventDefault();
      setSelectedBrowserIndex((prev) => (prev - 1 + browsers.length) % browsers.length);
    }
  };

  return (
    <div className={`search-mode-container theme-${theme}`} onClick={(e) => e.stopPropagation()}>
      <div className="glass-search-box">
        <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="11" cy="11" r="8" />
          <path d="M21 21l-4.35-4.35" />
        </svg>

        <input
          ref={inputRef}
          type="text"
          className="search-input"
          placeholder="Search Google or type a web address..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
        />

        <div 
          className="mode-switch-badge" 
          onClick={onSwitchToRadial}
          title="Press Tab to switch to Radial Menu"
        >
          <span>Wheel</span>
          <kbd style={{ fontSize: '10px', background: 'rgba(255,255,255,0.2)', padding: '2px 5px', borderRadius: '4px' }}>Tab</kbd>
        </div>
      </div>

      <div className="browser-slider-bar">
        {browsers.map((browser, index) => {
          const isSelected = index === selectedBrowserIndex;
          return (
            <button
              key={browser.exe_path + index}
              className={`browser-chip ${isSelected ? "active" : ""}`}
              onClick={() => setSelectedBrowserIndex(index)}
              title={`Search in ${browser.name}`}
            >
              {browser.icon ? (
                <img src={browser.icon} alt="" className="browser-chip-icon" />
              ) : (
                <svg className="browser-chip-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="2" y1="12" x2="22" y2="12" />
                  <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1 4-10z" />
                </svg>
              )}
              <span>{browser.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
