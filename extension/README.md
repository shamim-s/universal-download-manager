# UDM Browser Extensions

Intercepts browser downloads and forwards them to the UDM daemon (`ws://127.0.0.1:60123`).

## Chrome / Edge (MV3)

### Load (development)
1. Start the UDM daemon (`cargo run -p udm-daemon`).
2. Open `chrome://extensions` (or `edge://extensions`), enable **Developer mode**.
3. **Load unpacked** → select `extension/chrome`.

### Structure
```
chrome/
├── manifest.json                 # MV3, module service worker
├── lib/udm-core.js               # pure logic (interception decision, payload) — unit-tested
├── background/service-worker.js  # WS connect/reconnect, intercept downloads, context menu
└── popup/                        # live status (opens its own socket to the daemon)
```

### Behaviour
- Intercepts `http(s)`/`ftp` downloads only while **connected** to the daemon — if the daemon
  is down, downloads fall through to the browser (never lost). Reconnects every 3 s.
- On a download: cancels + erases the browser entry, gathers cookies, forwards
  `ADD_DOWNLOAD` with `url`/`filename`/`referrer`/`cookies`.
- Right-click a link → **Download with UDM**.

## Tests
```bash
# Unit tests (interception decision, cookie/payload building) — pure, no browser:
node --test extension/test/core.test.mjs

# Contract e2e (needs the daemon + a file server running):
node extension/scripts/e2e.mjs <savePath>
```

## Manual verification (DoD)
1. Daemon running; extension loaded; open the popup → shows **Connected**.
2. Click a normal http(s) download (e.g. a sample file). The browser's download is
   cancelled and the file appears in UDM (popup shows it active, then complete).
3. Download from a site requiring login → succeeds (cookies/referrer forwarded).
4. Quit the daemon → popup shows "UDM app not running"; restart it → reconnects within ~3 s.

## Firefox
See `firefox/` (Phase 7) — MV2 with the `browser.*` namespace; same `lib/udm-core.js`.
