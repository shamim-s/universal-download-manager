# UDM — Architecture Reference

Design reference for the Universal Download Manager. The build order lives in
[`BUILD_PHASES.md`](BUILD_PHASES.md); this document is the *what* and *why*.

## 1. System Overview

Two parts on the user's machine:

- **Browser extensions** (Chrome / Firefox / Edge) intercept downloads and forward them.
- **A native daemon + desktop UI** that accelerates, resumes, queues, and organizes downloads.

```
Chrome / Firefox / Edge extensions
        │  (WebSocket ws://127.0.0.1:60123)
        ▼
   UDM Daemon (Tokio)
   ├── Download Engine (multi-segment, resume)
   ├── Queue & Scheduler (priority, concurrency, throttle)
   └── Storage (SQLite)
        │  (Tauri IPC / WebSocket)
        ▼
   UDM Desktop UI (Tauri + React)
```

**Pattern:** event-driven microkernel. The daemon is the always-on kernel; extensions are
untrusted clients (validate everything); the UI is an optional observer (daemon runs headless).

## 2. Communication

- **Extensions ↔ Daemon:** WebSocket on `127.0.0.1:60123` (chosen over Native Messaging for
  cross-browser portability and easier debugging).
- **UI ↔ Daemon:** Tauri IPC for local control + the same WebSocket for live events.
- **REST API (optional):** `/api/jobs`, `/api/settings`, `/api/stats` for CLI/integrations.

### Key flows

1. **New download:** extension intercepts `downloads.onCreated` → cancels native download →
   sends `ADD_DOWNLOAD` (url, cookies, referrer) → daemon validates, enqueues, ACKs `JOB_ADDED`,
   streams `JOB_PROGRESS`.
2. **Resume after crash:** daemon reads `jobs.db`, finds Active/Paused jobs, inspects `.part`
   files, resumes from the last valid byte range, emits `RESUMED`.
3. **Throttle:** UI sends `SET_BANDWIDTH` → daemon reconfigures the shared `TokenBucket`;
   workers await tokens before reading.

## 3. Data Models (see `daemon/crates/storage/src/models.rs`)

- `DownloadJob` — id, url, filename, save_path, file_size, downloaded_bytes, status, priority,
  timestamps, referrer, cookies, user_agent, headers, `Vec<Segment>`, checksum, source_browser, tags.
- `JobStatus` — Queued | Active | Paused | Completed | Failed | Cancelled.
- `Segment` — id, byte_start, byte_end, downloaded, temp_file, status.
- `Checksum` — algorithm, expected, verified.
- `AppSettings` — download_directory, max_concurrent_downloads (3), max_segments_per_download (8),
  max_bandwidth_kbps, auto_start_on_boot, minimize_to_tray, daemon_port (60123), file_type_rules,
  schedule window.

## 4. Protocol (see `daemon/crates/daemon/src/protocol.rs`)

`#[serde(tag = "type")]` enums mirrored in `extension/shared/protocol.js` and `ui/src/types.ts`.

**Client → Daemon:** `ADD_DOWNLOAD`, `PAUSE`, `RESUME`, `CANCEL`, `SET_PRIORITY`,
`GET_ALL_JOBS`, `SET_BANDWIDTH`.

**Daemon → Clients:** `JOB_ADDED`, `JOB_PROGRESS` (downloadedBytes, speedBps, eta),
`JOB_COMPLETED` (finalPath), `JOB_FAILED` (error), `JOB_PAUSED`, `ALL_JOBS`, `SETTINGS_UPDATED`.

## 5. Download Engine

1. **HEAD probe:** read `Content-Length` + `Accept-Ranges: bytes`. No ranges / unknown size →
   single-stream fallback.
2. **Chunking:** `num = min(max_segments, ceil(size / MIN_CHUNK_SIZE))`, `MIN_CHUNK_SIZE = 1 MiB`.
3. **Parallel segments:** one task per range with `Range: bytes=start-end` → `name.part.N`.
4. **Progress:** shared `AtomicU64`; emitter ticks every 500ms.
5. **Assembly:** concatenate parts in order → final file; verify checksum if provided; cleanup.
6. **Pause/Resume:** persist per-segment `downloaded`; resume re-issues `Range` from `start+downloaded`.

**Bandwidth:** a shared `TokenBucket` (capacity = bytes/s) refilled on a timer; `consume(bytes)`
awaits available tokens.

## 6. Browser Extension

- **MV3 (Chrome/Edge):** `service-worker.js` holds the WebSocket + reconnect loop; intercepts
  `downloads.onCreated`, cancels native, forwards `ADD_DOWNLOAD` with cookies/referrer.
- **MV2 (Firefox):** `background.scripts`, `browser.*` namespace (or webextension-polyfill).
- Cookies are gathered via `chrome.cookies.getAll({url})` and sent only over loopback.

## 7. Desktop UI

Tauri (system WebView → small bundle) + React + TypeScript + Zustand. `useDaemonSocket` manages
the WS connection and dispatches events into the store. Components: `DownloadList`,
`DownloadItem`, `Toolbar`, `SpeedGraph` (recharts), `Settings`, `AddURLModal`. System tray with
show / pause-all / quit and an active-count badge.

## 8. Security

| Threat                          | Mitigation                                              |
|---------------------------------|--------------------------------------------------------|
| Any local app connects          | Bind WS to `127.0.0.1`; optional bearer token in keychain |
| Malicious URL from extension    | Validate URL format; strip unsafe protocols            |
| MITM on download                | Verify checksum when provided                          |
| Cookie theft                    | Loopback only; never logged                            |
| Path traversal in `save_path`   | Normalize; reject paths outside allowed dirs           |
| Privilege escalation            | Run as user; never root                                |

## 9. Testing

Unit (range split, token bucket, DB round-trip) · Integration (local `axum` server: single,
multi, resume, no-Range fallback, checksum) · Extension (Playwright + headless Chrome) ·
Load (50 concurrent downloads).

## 10. Distribution

Tauri bundles per-OS installers (.dmg / .msi / .AppImage / .deb), code-signed, with
`tauri-plugin-updater`. Extensions go to Chrome Web Store, Firefox AMO, and Edge Add-ons.
CI (GitHub Actions) builds + tests daemon and UI on a 3-OS matrix and zips the extensions.
