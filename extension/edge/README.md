# Edge Extension

Edge is Chromium-based and supports Manifest V3, so the **Chrome build runs unmodified**.

## Load (development)
1. Start the UDM daemon.
2. Open `edge://extensions`, enable **Developer mode**.
3. **Load unpacked** → select `extension/chrome`.

The MV3 manifest, module service worker, `downloads`/`cookies` APIs, and `ws://127.0.0.1`
loopback connection all behave identically to Chrome.

## Store packaging
For the Edge Add-ons store, zip the contents of `extension/chrome/` and submit. Keep this
directory only for Edge-specific store metadata (listing copy, screenshots) if needed — there
are no Edge-specific code files.
