import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { appWindow, PhysicalPosition } from "@tauri-apps/api/window";

export type ClockFont = "system" | "modern" | "tech" | "serif" | "digital";

export function DesktopClockWidget() {
  const [clockState, setClockState] = useState({ timeMain: "", ampm: "", dateStr: "" });
  const [fontFamily, setFontFamily] = useState<ClockFont>("modern");
  const [theme, setTheme] = useState("black");
  const [isLocked, setIsLocked] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    let unlistenMove: (() => void) | undefined;

    const updateTime = () => {
      const now = new Date();
      let hours = now.getHours();
      const minutes = String(now.getMinutes()).padStart(2, "0");
      const seconds = String(now.getSeconds()).padStart(2, "0");
      const period = hours >= 12 ? "PM" : "AM";
      hours = hours % 12;
      hours = hours ? hours : 12;
      const formattedHours = String(hours).padStart(2, "0");

      setClockState({
        timeMain: `${formattedHours}:${minutes}:${seconds}`,
        ampm: period,
        dateStr: now.toLocaleDateString([], {
          weekday: "short",
          day: "numeric",
          month: "short",
          year: "numeric"
        })
      });
    };

    updateTime();
    const interval = setInterval(updateTime, 1000);

    const loadData = async () => {
      try {
        const config = await invoke<any>("get_config");
        if (config && config.theme) {
          setTheme(config.theme.toLowerCase() === "white" ? "white" : "black");
        }
        if (config && config.clock_font) {
          setFontFamily(config.clock_font as ClockFont);
        }

        // Restore saved clock position if available
        if (config && config.clock_x !== undefined && config.clock_y !== undefined && config.clock_x !== null && config.clock_y !== null) {
          await appWindow.setPosition(new PhysicalPosition(config.clock_x, config.clock_y));
        }
      } catch (e) {
        console.error("Error loading clock config:", e);
      }
    };

    loadData();

    // Listen for window move to auto-save position with 500ms debounce
    let moveTimeout: any = null;
    appWindow.onMoved(({ payload: pos }) => {
      if (moveTimeout) clearTimeout(moveTimeout);
      moveTimeout = setTimeout(() => {
        invoke("save_window_position", {
          label: "clock",
          x: pos.x,
          y: pos.y
        }).catch((err) => console.error("Error saving clock position:", err));
      }, 500);
    }).then((unlisten) => {
      unlistenMove = unlisten;
    });

    return () => {
      clearInterval(interval);
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

  const toggleShowSettings = () => {
    setShowSettings(!showSettings);
  };

  const handleStartDrag = (e: React.MouseEvent) => {
    if (e.button === 0 && !isLocked) {
      const target = e.target as HTMLElement;
      if (target.tagName !== "BUTTON" && !target.closest("button")) {
        appWindow.startDragging();
      }
    }
  };

  return (
    <div 
      className={`desktop-clock-widget-container theme-${theme}`}
      onMouseDown={handleStartDrag}
    >
      <div
        className={`glass-clock-box font-${fontFamily}`}
        onMouseDown={handleStartDrag}
        data-tauri-drag-region={!isLocked ? "true" : undefined}
        title={!isLocked ? "Drag to move clock anywhere on desktop" : "Position locked"}
      >
        {!isLocked && (
          <div className="drag-handle-grip" onMouseDown={handleStartDrag} data-tauri-drag-region title="Drag to move">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
              <circle cx="8" cy="6" r="1.5" />
              <circle cx="16" cy="6" r="1.5" />
              <circle cx="8" cy="12" r="1.5" />
              <circle cx="16" cy="12" r="1.5" />
              <circle cx="8" cy="18" r="1.5" />
              <circle cx="16" cy="18" r="1.5" />
            </svg>
          </div>
        )}

        <div className="clock-time-date-group" onMouseDown={handleStartDrag} data-tauri-drag-region={!isLocked ? "true" : undefined}>
          <div className="clock-time-display" onMouseDown={handleStartDrag} data-tauri-drag-region={!isLocked ? "true" : undefined}>
            <span className="clock-digits">{clockState.timeMain || "12:00:00"}</span>
            <span className="clock-ampm">{clockState.ampm || "PM"}</span>
          </div>
          <div className="clock-date-display" onMouseDown={handleStartDrag} data-tauri-drag-region={!isLocked ? "true" : undefined}>
            {clockState.dateStr || "Thu, 30 Jul 2026"}
          </div>
        </div>

        {/* Action Controls & Font Choice Selector */}
        <div className="widget-controls">
          <button
            className={`widget-control-btn ${showSettings ? "active" : ""}`}
            onClick={toggleShowSettings}
            title="Font Style Choice"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="4 7 4 4 20 4 20 7" />
              <line x1="9" y1="20" x2="15" y2="20" />
              <line x1="12" y1="4" x2="12" y2="20" />
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

      {/* Font Choice Selector Bar */}
      {showSettings && (
        <div className="font-selector-bar">
          {(["modern", "system", "tech", "serif", "digital"] as ClockFont[]).map((f) => (
            <button
              key={f}
              className={`font-chip ${fontFamily === f ? "active" : ""}`}
              onClick={async () => {
                setFontFamily(f);
                toggleShowSettings();
                try {
                  await invoke("save_clock_font", { font: f });
                } catch (e) {
                  console.error("Error saving font:", e);
                }
              }}
            >
              {f.toUpperCase()}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
