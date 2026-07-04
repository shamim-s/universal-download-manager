// Settings panel (Phase 8): download dir, concurrency, segments, bandwidth cap,
// and behavior toggles. Persists via UPDATE_SETTINGS; the daemon applies the
// bandwidth + concurrency changes live and echoes SETTINGS_UPDATED back.

import { useEffect, useState } from "react";
import { AppSettings } from "../types";
import { sendMessage } from "../hooks/useDaemonSocket";
import { pushToast, useStore } from "../store/downloads";

/** Call a Tauri command; throws (caught by callers) when not running in Tauri. */
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export default function Settings({ onClose }: { onClose: () => void }) {
  const { settings } = useStore();
  const [form, setForm] = useState<AppSettings>(settings);
  // Keep the form in sync if the daemon pushes fresh settings while open.
  useEffect(() => setForm(settings), [settings]);

  // Autostart reflects the real OS state (not the daemon blob). null = unknown/dev.
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  useEffect(() => {
    tauriInvoke<boolean>("get_autostart")
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);

  async function toggleAutostart(enable: boolean) {
    try {
      await tauriInvoke("set_autostart", { enable });
      setAutostart(enable);
      pushToast("success", enable ? "UDM will start with Windows." : "Autostart disabled.");
    } catch {
      pushToast("error", "Autostart is only available in the installed app.");
    }
  }

  async function checkUpdates() {
    setCheckingUpdate(true);
    try {
      const msg = await tauriInvoke<string>("check_for_updates");
      pushToast("info", msg);
    } catch (e) {
      pushToast("error", String(e));
    } finally {
      setCheckingUpdate(false);
    }
  }

  const capped = form.maxBandwidthKbps != null;

  function set<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }

  function save() {
    sendMessage({ type: "UPDATE_SETTINGS", settings: form });
    pushToast("success", "Settings saved.");
    onClose();
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
        <h2 className="modal__title">Settings</h2>

        <label className="field">
          <span className="field__label">Download directory</span>
          <input
            className="field__input"
            placeholder="e.g. C:\\Users\\you\\Downloads"
            value={form.downloadDirectory}
            onChange={(e) => set("downloadDirectory", e.target.value)}
          />
        </label>

        <label className="field">
          <span className="field__label">
            Max concurrent downloads <strong>{form.maxConcurrentDownloads}</strong>
          </span>
          <input
            type="range"
            min={1}
            max={16}
            value={form.maxConcurrentDownloads}
            onChange={(e) => set("maxConcurrentDownloads", Number(e.target.value))}
          />
        </label>

        <label className="field">
          <span className="field__label">
            Segments per download <strong>{form.maxSegmentsPerDownload}</strong>
          </span>
          <input
            type="range"
            min={1}
            max={16}
            value={form.maxSegmentsPerDownload}
            onChange={(e) => set("maxSegmentsPerDownload", Number(e.target.value))}
          />
        </label>

        <div className="field">
          <span className="field__label">Bandwidth limit</span>
          <div className="field__row">
            <label className="check">
              <input
                type="checkbox"
                checked={capped}
                onChange={(e) => set("maxBandwidthKbps", e.target.checked ? 1024 : null)}
              />
              Limit speed
            </label>
            <input
              className="field__input field__input--num"
              type="number"
              min={1}
              disabled={!capped}
              value={form.maxBandwidthKbps ?? ""}
              onChange={(e) => set("maxBandwidthKbps", Number(e.target.value) || 1)}
            />
            <span className="field__suffix">KB/s</span>
          </div>
        </div>

        <label className="check check--block">
          <input
            type="checkbox"
            checked={form.promptBeforeDownload}
            onChange={(e) => set("promptBeforeDownload", e.target.checked)}
          />
          Ask where to save each download (show the New Download prompt)
        </label>

        <label className="check check--block">
          <input
            type="checkbox"
            checked={form.autoCategorize}
            onChange={(e) => set("autoCategorize", e.target.checked)}
          />
          Sort downloads into folders by type (Video, Music, Documents, …)
        </label>

        <label className="check check--block">
          <input
            type="checkbox"
            checked={form.clipboardWatcher}
            onChange={(e) => set("clipboardWatcher", e.target.checked)}
          />
          Watch clipboard for download links
        </label>

        <label className="check check--block">
          <input
            type="checkbox"
            checked={form.minimizeToTray}
            onChange={(e) => set("minimizeToTray", e.target.checked)}
          />
          Minimize to system tray
        </label>

        <label className="check check--block">
          <input
            type="checkbox"
            checked={!!autostart}
            onChange={(e) => toggleAutostart(e.target.checked)}
          />
          Start UDM when Windows starts
        </label>

        <div className="field">
          <span className="field__label">Updates</span>
          <div className="field__row">
            <button className="btn btn--ghost" disabled={checkingUpdate} onClick={checkUpdates}>
              {checkingUpdate ? "Checking…" : "Check for updates"}
            </button>
            <span className="field__suffix">current v0.1.0</span>
          </div>
        </div>

        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn--primary" onClick={save}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
