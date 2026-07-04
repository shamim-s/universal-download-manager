# UDM — Install & Use as a Native Desktop App

A complete, honest walkthrough: build the installer, install it, run it, and get the
browser extension working. Read the **"How the pieces fit"** section first — it
explains why a couple of things can't be fully automatic.

---

## 0. How the pieces fit (read this first)

UDM is **three** programs, not one:

| Piece            | What it is                          | Where it lives                                  |
|------------------|-------------------------------------|-------------------------------------------------|
| **Daemon**       | Background download engine          | `daemon/` → `udm-daemon.exe`                     |
| **Desktop UI**   | The Tauri app window (what you see) | `ui/` → `UDM.exe` / `UDM_x.y.z_x64.msi`          |
| **Extension**    | Browser hook that captures clicks   | `extension/chrome`, `extension/firefox`          |

They talk over a **loopback WebSocket** (`ws://127.0.0.1:60123`). So:

- The **daemon now ships inside the app as a Tauri sidecar** — UDM launches it on
  startup and stops it on quit, so a normal install "just works" (no separate process to
  manage). You only run the daemon by hand for headless/dev use (Section 4).
- The **extension** can't be silently force-enabled by an installer — browsers forbid
  that. You either load it manually (Section 5), publish it to a store, or use an
  enterprise policy (Section 6). This is a browser security rule, not a UDM limitation.

> **The honest summary of your goal:** "install → browser opens → extension auto-enables"
> is ~80% automatable. The app *can* open a setup page on first run (Section 7), and on a
> machine you control you *can* force-install the extension via registry policy
> (Section 6). Fully silent auto-enable on an arbitrary user's machine requires
> publishing to the Chrome Web Store / Edge Add-ons / Firefox AMO.

---

## 1. Prerequisites (build machine)

- **Rust** ≥ 1.77 (`rustup`) — on this machine it's at `%USERPROFILE%\.cargo\bin`, not on
  PATH, so prepend it per shell:
  ```powershell
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
  ```
- **MSVC C++ Build Tools** (already installed here) — needed for `rusqlite`'s bundled SQLite.
- **Node.js ≥ 20 + npm** (already on PATH).
- **WebView2** runtime — preinstalled on Windows 11. Tauri uses it for the UI.

Tauri auto-downloads the **WiX** (`.msi`) and **NSIS** (`.exe`) bundlers on first build.

---

## 2. Build the daemon

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cd D:\Personal\udm-downloader\daemon
cargo build --release
# → daemon\target\release\udm-daemon.exe
```

---

## 3. Build the desktop installer

The app bundles the daemon as a sidecar, so build the daemon (Section 2) **first**, then
copy it into place with `prepare:sidecar` before bundling:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cd D:\Personal\udm-downloader\ui
npm install
npm run prepare:sidecar   # copies udm-daemon.exe → src-tauri/binaries/udm-daemon-<triple>.exe
npm run tauri build
```

Artifacts land in (sizes from a verified local build):

```
ui\src-tauri\target\release\
  ├─ udm-ui.exe                               # the raw app (run without installing)  ~11 MB
  ├─ udm-daemon.exe                           # the bundled sidecar daemon            ~6 MB
  └─ bundle\
       ├─ msi\UDM_0.1.0_x64_en-US.msi         # Windows Installer                     ~6.4 MB
       └─ nsis\UDM_0.1.0_x64-setup.exe        # NSIS setup                            ~4.2 MB
```

Double-click either installer to install UDM into Program Files + Start Menu.
(For a signed installer that won't trip SmartScreen, see `RELEASE_CHECKLIST.md §4`.)

---

## 4. Running the daemon

**Normally you don't have to** — the installed app starts the bundled daemon sidecar
automatically and shuts it down on quit. The header dot turns green ("Connected") within
a second of launch.

Run it by hand only for headless use or daemon-only development:
```powershell
D:\Personal\udm-downloader\daemon\target\release\udm-daemon.exe
```
> If a standalone daemon is already bound to `:60123`, the app's sidecar detects the port
> is taken, exits, and the UI connects to the existing one. No conflict.

---

## 5. Install the browser extension — manual (works today, no publishing)

### Chrome / Edge (Chromium, MV3)
1. Go to `chrome://extensions` (or `edge://extensions`).
2. Toggle **Developer mode** (top-right) ON.
3. Click **Load unpacked** → select `D:\Personal\udm-downloader\extension\chrome`.
4. The "Universal Download Manager" extension appears and is **enabled**. Pin it for the popup.

> Note: unpacked extensions require Developer Mode to stay on. Chrome may nag on each
> launch. To avoid that, publish to the store (Section 6) or use the policy install.

### Firefox (MV2)
1. Go to `about:debugging#/runtime/this-firefox`.
2. **Load Temporary Add-on…** → pick any file inside `extension\firefox` (e.g. `manifest.json`).
3. It loads with ID `udm@udm.app`. **Temporary** add-ons unload when Firefox restarts.
   For a permanent install you must sign the `.xpi` via Firefox AMO.

After loading, click a download link in the browser — with the daemon running, the
extension cancels the browser download and hands it to UDM.

---

## 6. Auto-install + auto-enable the extension (no manual clicks)

This is the only way to get a download manager extension to install **and stay enabled
silently**. Two real options:

### Option A — Publish to the stores (best for end users)
Submit the zips (CI produces `chrome-extension.zip` / `firefox-extension.zip`):
- **Chrome Web Store** & **Edge Add-ons** — Chromium MV3 build.
- **Firefox AMO** — MV2 build (also gives you a signed, permanent `.xpi`).

Then your installer/setup page just deep-links to the store listing; the user clicks
"Add" once. This is the trusted, no-warnings path.

### Option B — Windows enterprise policy force-install (machines you control)
Chrome/Edge honor a registry **force-list** that installs + enables + locks an extension.
Requirements: a **fixed extension ID** and a hosted **update URL** (the store, or a
self-hosted `update.xml` + packed `.crx`).

1. Give the Chrome extension a stable ID by adding a `key` to its `manifest.json`
   (generate with `openssl`/`crx3`), or just publish it (the store assigns the ID).
2. Add the registry force-list (example for Edge; Chrome uses
   `HKLM\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist`):
   ```powershell
   # value = "<EXTENSION_ID>;<UPDATE_URL>"
   New-Item -Path "HKLM:\SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallForcelist" -Force
   Set-ItemProperty -Path "HKLM:\SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallForcelist" `
     -Name "1" -Value "<EXTENSION_ID>;https://edge.microsoft.com/extensionwebstorebase/v1/crx"
   ```
3. Restart the browser — the extension installs and **cannot be disabled by the user**.

> Why not just register an unpacked path? Chrome/Edge dropped silent unpacked
> force-install years ago (malware abuse). Force-install needs a hosted, signed package.
> Firefox's equivalent is `ExtensionSettings` policy with `installation_mode: force_installed`
> pointing at a signed `.xpi` URL.

---

## 7. The one-click app roadmap

1. ✅ **Daemon bundled as a Tauri sidecar** (implemented) — `tauri.conf.json` `externalBin`
   + `tauri-plugin-shell`; `lib.rs` spawns it on `setup` and kills it on exit. No separate
   process to manage; "Daemon offline" should never show on a clean launch. The binary is
   staged by `npm run prepare:sidecar` (see Section 3).

2. ✅ **First-run onboarding** (implemented) — `ui/src/components/Onboarding.tsx` shows
   automatically on first launch (gated by a `localStorage` flag, reopenable from the
   toolbar **?** button). It includes live connection diagnostics (daemon status,
   endpoint, active count) and the extension setup steps with one-click store-link
   buttons (Chrome Web Store / Edge Add-ons / Firefox AMO) plus click-to-copy
   `chrome://extensions` / `about:debugging`. This is the realistic, policy-free version
   of "guide the user to enable the extension" on first run.

3. ✅ **Start with Windows** (implemented) — `tauri-plugin-autostart` wired via the
   `set_autostart`/`get_autostart` commands; the **Settings → "Start UDM when Windows
   starts"** toggle drives the real OS login entry.

4. ✅ **Auto-update wiring** (implemented, inert until configured) —
   `tauri-plugin-updater` + a `check_for_updates` command behind **Settings → "Check for
   updates"**. It reports "not configured yet" until you generate a key and add
   `plugins.updater` (endpoints + pubkey) per RELEASE_CHECKLIST §5 — then it downloads and
   installs signed updates with no code change.

5. ⏭ **Store publishing** — submit the extension zips so it installs with one trusted
   click + accept/cancel prompt (Section 6A).

---

## 8. Daily use

1. Daemon running (sidecar, Startup shortcut, or manual).
2. Open **UDM** — header shows **Connected**.
3. Either click **+ Add URL** in the app, or just download normally in the browser with
   the extension enabled — it's captured automatically.
4. Pause/resume/cancel/remove per row; tune concurrency + bandwidth in **⚙ Settings**;
   closing the window hides UDM to the system tray (right-click tray → Quit to exit).

---

## 9. Troubleshooting

| Symptom                                  | Fix                                                                 |
|------------------------------------------|---------------------------------------------------------------------|
| UI says "Daemon offline"                 | Start `udm-daemon.exe` (Section 4); check nothing else uses :60123. |
| New daemon exits immediately (code 101)  | A stale daemon holds the port. Kill it: `Stop-Process -Name udm-daemon -Force`. |
| Browser downloads aren't captured        | Extension disabled, Developer Mode off, or daemon down.            |
| SmartScreen warning on the installer     | Installer is unsigned — see `RELEASE_CHECKLIST.md §4` for signing.  |
| Want a token-locked daemon               | Run with `UDM_AUTH_TOKEN=...`; clients use `ws://127.0.0.1:60123/?token=...`. |
