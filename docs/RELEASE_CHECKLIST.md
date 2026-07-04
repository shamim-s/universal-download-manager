# UDM — Release Checklist (Phase 9)

The steps to cut a signed, store-ready release. Items marked **(manual)** require
credentials or a developer account and can't be automated in CI without secrets.

## 1. Pre-flight

- [ ] All tests green: `cd daemon && cargo test --workspace` and `cd ui && npm run build`.
- [ ] `cargo fmt --check` and `cargo clippy -- -D warnings` clean.
- [ ] Extension `lib/udm-core.js` in sync across browsers: `node extension/scripts/sync-lib.mjs --check`.
- [ ] Bump versions in lockstep:
  - `daemon/Cargo.toml` (`workspace.package.version`)
  - `ui/package.json`, `ui/src-tauri/Cargo.toml`, `ui/src-tauri/tauri.conf.json` (`version`)
  - extension manifests (`extension/*/manifest.json`)
- [ ] Update `CHANGELOG`/release notes.
- [ ] Daemon port 60123 documented and user-configurable in settings.

## 2. Security checklist (ARCHITECTURE §11)

- [x] WebSocket binds to `127.0.0.1` only (loopback) — `daemon/src/main.rs` `BIND_ADDR`.
- [x] URLs validated: only `http`/`https` with a host are accepted — `security::validate::validate_url`.
- [x] Filenames sanitized to a safe basename (no `../`, separators, control chars) — `security::validate::safe_filename`.
- [x] Explicit save paths can't escape the configured download directory — `security::validate::is_within`.
- [x] Cookies forwarded as request headers only, transmitted over loopback, and **never logged**.
- [x] Optional loopback bearer token (`UDM_AUTH_TOKEN`) — `security::auth`; clients pass `?token=`.
- [ ] **(manual)** Store the token in the OS keychain instead of an env var for production installs.
- [x] Daemon runs as the invoking user (never elevated/root).
- [ ] Privacy policy: no telemetry without consent; crash reporter opt-in only.
- [ ] Uninstaller removes app, daemon, and (optionally) part files.

## 3. Build installers

Run on each target OS (or via the CI matrix). Tauri emits to
`ui/src-tauri/target/release/bundle/`.

```bash
cd ui && npm ci && npm run tauri build
# Or per-target:
cargo tauri build --target aarch64-apple-darwin     # macOS .dmg
cargo tauri build --target x86_64-pc-windows-msvc    # Windows .msi / .exe
cargo tauri build --target x86_64-unknown-linux-gnu  # Linux .AppImage / .deb
```

| OS      | Artifacts                          |
|---------|------------------------------------|
| Windows | `.msi` (WiX) and `.exe` (NSIS)     |
| macOS   | `.dmg` and `.app`                  |
| Linux   | `.AppImage` and `.deb`             |

## 4. Code signing **(manual)**

- [ ] **Windows:** Authenticode/EV cert — `signtool` or `bundle.windows.certificateThumbprint`.
- [ ] **macOS:** Apple Developer ID Application cert + notarization (`xcrun notarytool`);
      set `APPLE_CERTIFICATE`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.
- [ ] **Linux:** AppImage/`.deb` distributed unsigned or with a detached GPG signature.

## 5. Auto-update **(manual — requires keys)**

The plugin + `check_for_updates` command are **already wired** (`src-tauri/src/lib.rs`,
surfaced by Settings → "Check for updates"). It's inert until you add keys + config — no
code change needed to activate:

1. Generate a key pair: `npm run tauri signer generate -- -w ~/.tauri/udm.key`.
2. Add the **public** key to `tauri.conf.json` under `plugins.updater.pubkey`, set
   `bundle.createUpdaterArtifacts: true`, and add an `endpoints` URL pointing at the
   release manifest (e.g. GitHub Releases `latest.json`).
3. Build with `TAURI_SIGNING_PRIVATE_KEY` (+ password) set so update artifacts are signed.
4. Publish `latest.json` + signed bundles to the release endpoint.
- [ ] Auto-updater tested on macOS / Windows / Linux.

## 6. Extension store submissions **(manual)**

- [ ] Chrome Web Store — upload `chrome-extension.zip` (CI artifact) + screenshots + privacy policy.
- [ ] Firefox AMO — upload `firefox-extension.zip` (MV2); submit for review.
- [ ] Edge Add-ons — reuse the Chrome MV3 zip.

## 7. Publish

- [ ] Tag the release (`vX.Y.Z`) and push — the CI `release` job attaches installers
      and extension zips as artifacts.
- [ ] Verify a clean-machine install of each installer launches and connects to the daemon.
- [ ] Verify auto-update from the previous version (once §5 is configured).
- [ ] CI green on all three OSes.
