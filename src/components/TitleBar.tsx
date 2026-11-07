import { useEffect, useState, useCallback } from "react";
import { Minus, X, Square, Copy } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMac } from "../lib/platform";

export const TITLEBAR_HEIGHT = 32;

// Room for the native traffic lights. tauri.macos.conf.json places them.
const TRAFFIC_LIGHTS_WIDTH = 80;

function useWindowFocus() {
  const [isFocused, setIsFocused] = useState(true);

  useEffect(() => {
    const win = getCurrentWindow();
    win
      .isFocused()
      .then(setIsFocused)
      .catch(() => {});
    const unlisten = win.onFocusChanged(({ payload }) => {
      setIsFocused(payload);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  return isFocused;
}

function useWindowControls() {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    win
      .isMaximized()
      .then(setIsMaximized)
      .catch(() => {});
    const unlisten = win.onResized(() => {
      win
        .isMaximized()
        .then(setIsMaximized)
        .catch(() => {});
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const minimize = useCallback(() => {
    getCurrentWindow()
      .minimize()
      .catch(() => {});
  }, []);

  // Explicit branch dodges wry#622 — toggleMaximize is flaky on frameless windows
  const toggleMaximize = useCallback(async () => {
    const win = getCurrentWindow();
    try {
      if (await win.isMaximized()) {
        await win.unmaximize();
      } else {
        await win.maximize();
      }
    } catch {
      /* ignore */
    }
  }, []);

  const close = useCallback(() => {
    getCurrentWindow()
      .close()
      .catch(() => {});
  }, []);

  return { isMaximized, minimize, toggleMaximize, close };
}

function Wordmark({ isFocused }: { isFocused: boolean }) {
  return (
    <>
      <img
        src="/sone-icon.png"
        alt=""
        width={16}
        height={16}
        draggable={false}
        className={`pointer-events-none transition-opacity ${
          isFocused ? "opacity-100" : "opacity-50"
        }`}
      />
      <span
        className={`pointer-events-none text-[11px] font-semibold tracking-wider text-th-text-primary transition-opacity ${
          isFocused ? "opacity-100" : "opacity-50"
        }`}
      >
        SONE
      </span>
      <span
        className={`w-1.5 h-1.5 rounded-full bg-th-accent shrink-0 pointer-events-none transition-opacity ${
          isFocused ? "opacity-100" : "opacity-50"
        }`}
      />
    </>
  );
}

const BAR_CLASS =
  "flex items-center bg-th-surface border-b border-th-border-subtle select-none shrink-0";

// macOS: the native traffic lights sit on the left of this strip, and the
// wordmark is centered like a native window title.
function MacTitleBar() {
  const isFocused = useWindowFocus();

  return (
    <div
      data-tauri-drag-region
      className={BAR_CLASS}
      style={{ height: TITLEBAR_HEIGHT }}
    >
      <div
        data-tauri-drag-region
        className="h-full shrink-0"
        style={{ width: TRAFFIC_LIGHTS_WIDTH }}
      />
      <div
        data-tauri-drag-region
        className="flex-1 flex items-center justify-center gap-2 h-full"
      >
        <Wordmark isFocused={isFocused} />
      </div>
      <div
        data-tauri-drag-region
        className="h-full shrink-0"
        style={{ width: TRAFFIC_LIGHTS_WIDTH }}
      />
    </div>
  );
}

function DefaultTitleBar() {
  const isFocused = useWindowFocus();
  const { isMaximized, minimize, toggleMaximize, close } = useWindowControls();

  // Double-click maximize is handled by Tauri's drag-region script
  return (
    <div
      data-tauri-drag-region
      className={`${BAR_CLASS} justify-between`}
      style={{ height: TITLEBAR_HEIGHT }}
    >
      {/* Left zone: app icon + wordmark + accent indicator (also draggable) */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 pl-3 pr-4 h-full"
      >
        <Wordmark isFocused={isFocused} />
      </div>

      {/* Middle zone (flex grows to fill — draggable) */}
      <div data-tauri-drag-region className="flex-1 h-full" />

      {/* Right zone: window controls */}
      <div
        className={`flex items-center gap-1 pr-2 h-full transition-opacity ${
          isFocused ? "opacity-100" : "opacity-60"
        }`}
      >
        <button
          onClick={minimize}
          aria-label="Minimize"
          className="w-7 h-7 rounded-full flex items-center justify-center text-th-text-muted hover:bg-th-button-hover hover:text-th-text-primary transition-colors"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={toggleMaximize}
          aria-label={isMaximized ? "Restore" : "Maximize"}
          className="w-7 h-7 rounded-full flex items-center justify-center text-th-text-muted hover:bg-th-button-hover hover:text-th-text-primary transition-colors"
        >
          {isMaximized ? <Copy size={12} /> : <Square size={12} />}
        </button>
        <button
          onClick={close}
          aria-label="Close"
          className="w-7 h-7 rounded-full flex items-center justify-center text-th-text-muted hover:bg-red-500/80 hover:text-white transition-colors"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}

export default function TitleBar() {
  return isMac ? <MacTitleBar /> : <DefaultTitleBar />;
}
