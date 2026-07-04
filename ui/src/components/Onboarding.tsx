// First-run onboarding (Phase 9 roadmap): live connection diagnostics + the
// extension setup steps with one-click store links. Shown automatically on the
// first launch (gated by a localStorage flag) and reopenable from the toolbar.

import { useStore } from "../store/downloads";
import { pushToast } from "../store/downloads";

const CHROME_STORE = "https://chromewebstore.google.com/";
const EDGE_STORE = "https://microsoftedge.microsoft.com/addons/";
const FIREFOX_AMO = "https://addons.mozilla.org/firefox/";

async function openUrl(url: string) {
  try {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    pushToast("success", `Copied: ${text}`);
  } catch {
    pushToast("error", "Couldn't copy to clipboard.");
  }
}

export default function Onboarding({ onClose }: { onClose: () => void }) {
  const { connected, jobs } = useStore();
  const active = jobs.filter((j) => j.status === "active").length;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
        <h2 className="modal__title">Welcome to UDM 👋</h2>

        {/* Connection diagnostics */}
        <div className="diag">
          <div className="diag__row">
            <span className={`conn__dot ${connected ? "conn__dot--up" : ""}`} />
            <span className="diag__label">Daemon</span>
            <span className="diag__value">
              {connected ? "Connected" : "Starting… (bundled, launches with the app)"}
            </span>
          </div>
          <div className="diag__row">
            <span className="diag__spacer" />
            <span className="diag__label">Endpoint</span>
            <span className="diag__value">ws://127.0.0.1:60123</span>
          </div>
          <div className="diag__row">
            <span className="diag__spacer" />
            <span className="diag__label">Active</span>
            <span className="diag__value">{active} download(s)</span>
          </div>
        </div>

        {/* Extension setup */}
        <h3 className="onb__h3">Capture browser downloads</h3>
        <p className="onb__p">
          Install the UDM browser extension so clicked downloads are handed to UDM
          automatically. Pick the path that fits:
        </p>

        <ol className="onb__steps">
          <li>
            <strong>From a store</strong> (recommended — one click, no warnings):
            <div className="onb__btns">
              <button className="btn btn--ghost" onClick={() => openUrl(CHROME_STORE)}>
                Chrome Web Store
              </button>
              <button className="btn btn--ghost" onClick={() => openUrl(EDGE_STORE)}>
                Edge Add-ons
              </button>
              <button className="btn btn--ghost" onClick={() => openUrl(FIREFOX_AMO)}>
                Firefox AMO
              </button>
            </div>
          </li>
          <li>
            <strong>Load unpacked</strong> (dev): open{" "}
            <code className="onb__code" onClick={() => copy("chrome://extensions")}>
              chrome://extensions
            </code>{" "}
            (click to copy) → enable <em>Developer mode</em> → <em>Load unpacked</em> →
            select the <code>extension/chrome</code> folder. Firefox: use{" "}
            <code className="onb__code" onClick={() => copy("about:debugging#/runtime/this-firefox")}>
              about:debugging
            </code>{" "}
            → <em>Load Temporary Add-on</em>.
          </li>
        </ol>

        <p className="onb__note">
          Browsers don't allow apps to silently enable extensions — that's a security rule.
          See <code>docs/INSTALL_GUIDE.md</code> for the enterprise force-install option.
        </p>

        <div className="modal__actions">
          <button
            className="btn btn--primary"
            onClick={() => {
              localStorage.setItem("udm.onboarded", "1");
              onClose();
            }}
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}
