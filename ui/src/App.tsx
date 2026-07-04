// UDM app root (Phase 8). Boots the daemon socket and composes the UI:
// toolbar + speed graph header, the filtered download list, settings drawer,
// and toast notifications.

import { useEffect, useState } from "react";
import "./App.css";
import { useDaemonSocket, sendMessage, onDownloadPrompt } from "./hooks/useDaemonSocket";
import { useClipboardWatcher } from "./hooks/useClipboardWatcher";
import { openNewDownloadPrompt } from "./newDownloadWindow";
import { getState, useStore } from "./store/downloads";
import Toolbar from "./components/Toolbar";
import SpeedGraph from "./components/SpeedGraph";
import DownloadList from "./components/DownloadList";
import Settings from "./components/Settings";
import Onboarding from "./components/Onboarding";
import Toasts from "./components/Toasts";

export default function App() {
  useDaemonSocket();
  useClipboardWatcher();
  const { connected } = useStore();
  const [showSettings, setShowSettings] = useState(false);
  // Show onboarding automatically on the very first launch.
  const [showOnboarding, setShowOnboarding] = useState(
    () => !localStorage.getItem("udm.onboarded"),
  );

  // When the daemon relays an intercepted browser download, pop the small
  // "New Download" window (path picker + Start). Main window only.
  useEffect(() => onDownloadPrompt(openNewDownloadPrompt), []);

  // Bridge the tray's "Pause all" menu item to the daemon. No-op outside Tauri.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen("tray-pause-all", () => {
          for (const j of getState().jobs) {
            if (j.status === "active") sendMessage({ type: "PAUSE", jobId: j.id });
          }
        }),
      )
      .then((fn) => (unlisten = fn))
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  return (
    <div className="app">
      <Toolbar
        onOpenSettings={() => setShowSettings(true)}
        onOpenSetup={() => setShowOnboarding(true)}
      />

      {!connected && (
        <div className="banner banner--warn">
          Daemon offline — start the UDM daemon to manage downloads. Reconnecting…
        </div>
      )}

      <main className="app__body">
        <aside className="app__side">
          <SpeedGraph />
        </aside>
        <section className="app__list">
          <DownloadList />
        </section>
      </main>

      {showSettings && <Settings onClose={() => setShowSettings(false)} />}
      {showOnboarding && <Onboarding onClose={() => setShowOnboarding(false)} />}
      <Toasts />
    </div>
  );
}
