// Opens the small "New Download" popup window when the daemon asks the UI to
// prompt (DOWNLOAD_PROMPT). Called only from the main window. The intent payload
// is stashed in the Rust backend (keyed by id) so a large cookie header never
// has to ride through the window URL; the popup claims it via `take_intent`.

import { AddDownloadPayload } from "./types";
import { pushToast } from "./store/downloads";

let seq = 0;

export async function openNewDownloadPrompt(payload: AddDownloadPayload): Promise<void> {
  try {
    const [{ WebviewWindow }, { invoke }] = await Promise.all([
      import("@tauri-apps/api/webviewWindow"),
      import("@tauri-apps/api/core"),
    ]);
    const id = `${Date.now()}-${seq++}`;
    await invoke("stash_intent", { id, intent: payload });

    const w = new WebviewWindow(`new-download-${id}`, {
      url: `index.html?nd=${id}`,
      title: "New Download",
      width: 480,
      height: 300,
      resizable: false,
      center: true,
      alwaysOnTop: true,
      focus: true,
    });
    w.once("tauri://error", (e) => {
      console.error("New Download window failed:", e);
      pushToast("error", `New Download window failed: ${JSON.stringify(e.payload ?? e)}`);
    });
  } catch (e) {
    // Not running under Tauri (e.g. plain `vite` preview) — nothing to open.
    console.error("openNewDownloadPrompt:", e);
    pushToast("error", `Couldn't open New Download prompt: ${String(e)}`);
  }
}
