# UDM — Phased Build Plan (0 → 100%)

This is the **execution checklist** for building the Universal Download Manager from an
empty repo to a shippable product. Each phase has: a goal, prerequisites, the exact files
you touch, concrete tasks, and a **Definition of Done (DoD)** you can verify before moving on.

> Reference design lives in [`ARCHITECTURE.md`](ARCHITECTURE.md). This document is the *order
> of operations*; the architecture doc is the *what* and *why*.

## Progress Tracker

| Phase | Name                                   | Output                                  | %    |
|-------|----------------------------------------|-----------------------------------------|------|
| 0 ✅  | Toolchain & Scaffold                   | Repo builds, empty crates compile       | 10%  |
| 1 ✅  | Storage Layer                          | SQLite jobs/settings persistence        | 20%  |
| 2 ✅  | WebSocket Server + Protocol            | Daemon accepts clients, echoes messages | 30%  |
| 3 ✅  | Single-Segment Download Engine         | One URL downloads end-to-end            | 45%  |
| 4 ✅  | Multi-Segment + Pause/Resume           | Parallel chunks, resumable              | 60%  |
| 5 ✅  | Queue, Scheduler & Throttle            | Concurrency, priority, bandwidth cap    | 70%  |
| 6 ✅  | Chrome Extension (MV3)                 | Browser downloads intercepted           | 80%  |
| 7 ✅  | Firefox + Edge Extensions              | Cross-browser interception              | 85%  |
| 8 ✅  | Tauri Desktop UI                       | Live list, progress, controls           | 95%  |
| 9 ◑  | Polish, Security, Packaging            | Security + CI done; signing/stores manual | 100% |

---

## Phase 0 — Toolchain & Scaffold (→ 10%)

**Goal:** every directory compiles/builds with no logic yet.

**Prerequisites — install:**
- Rust stable (`rustup`, `cargo`) ≥ 1.77
- Node.js LTS (≥ 20) + npm
- Tauri prerequisites for your OS (WebView2 on Windows is preinstalled on Win 11)
- A WebSocket test client: `cargo install websocat` (or use `wscat`)

**Files in play:**
```
daemon/Cargo.toml                         # workspace
daemon/crates/daemon/Cargo.toml + src/main.rs
daemon/crates/engine/Cargo.toml + src/lib.rs
daemon/crates/storage/Cargo.toml + src/lib.rs
ui/                                       # `npm create tauri-app`
extension/chrome/manifest.json
```

**Tasks:**
1. `cd daemon && cargo build` — workspace with three crates compiles.
2. Scaffold the UI: `cd ui && npm create tauri-app@latest .` (React + TS template), then
   `npm install && npm run tauri build` once to confirm the toolchain.
3. Load `extension/chrome` as an unpacked extension in `chrome://extensions` — it loads
   with no errors (no behavior yet).

**DoD:**
- [x] `cargo build` succeeds in `daemon/` (all 3 crates; `rusqlite` C build OK).
- [x] `cargo test -p udm-engine` passes (chunker tests).
- [x] UI scaffolded (Tauri + React-TS); `npm run build` and `src-tauri` `cargo build` both succeed.
- [x] `npm run tauri dev` opens the app window.
- [x] Chrome extension assets complete (manifest valid, icons present) — loads unpacked clean.

**Environment notes (this machine):**
- Rust 1.96.0 (msvc) installed at `%USERPROFILE%\.cargo\bin` — **not on default PATH**; prepend it per shell:
  `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.
- MSVC C++ Build Tools (toolset 14.44, Windows SDK 10.0.26100) installed into Build Tools 2022.
- Tauri crate renamed from the scaffold default `ui_gen` → `udm-ui` (productName "UDM").

---

## Phase 1 — Storage Layer (→ 20%)

**Goal:** durable persistence of jobs and settings; survives restart.

**Files:**
```
daemon/crates/storage/src/lib.rs
daemon/crates/storage/src/db.rs
daemon/crates/storage/src/models.rs        # DownloadJob, JobStatus, Segment, Settings
daemon/crates/storage/src/schema.sql
```

**Tasks:**
1. Add deps: `rusqlite` (bundled), `serde`, `serde_json`, `uuid`, `chrono`.
2. Define `DownloadJob`, `JobStatus`, `Segment`, `Checksum`, `AppSettings` (see ARCHITECTURE §5).
3. Implement:
   - `db::init(path)` — runs `schema.sql`, creates `jobs` + `settings` tables.
   - `db::insert_job`, `db::update_job_progress`, `db::set_status`.
   - `db::get_all_jobs`, `db::get_pending_jobs` (status in Active/Paused/Queued).
   - `settings::load` / `settings::save` (key-value JSON).
4. Serialize complex fields (`segments`, `headers`, `tags`, `checksum`) as JSON text columns.

**DoD:**
- [x] Unit test: insert a job, reload it, fields round-trip (full `PartialEq`).
- [x] Unit test: update progress is atomic and reflected on reload.
- [x] DB file is created on first run and reused on the next.
- [x] Bonus: `set_status`, `get_pending_jobs` (excludes terminal states), `delete_job`,
      and settings round-trip/default — 7 tests passing.

**Implementation notes:**
- `db.rs` API: `init`/`init_in_memory`, `insert_job`, `update_job_progress`, `set_status`,
  `get_job`, `get_all_jobs`, `get_pending_jobs`, `delete_job`, `save_settings`, `load_settings`.
- Complex fields (`headers`, `segments`, `checksum`, `tags`) stored as JSON text columns;
  `u64` ↔ `i64` cast for SQLite; timestamps as RFC3339; `JobStatus` ↔ lowercase string.
- WAL journal mode enabled. Models gained `PartialEq` for round-trip assertions and
  `JobStatus::as_str`/`from_db` helpers.

---

## Phase 2 — WebSocket Server + Protocol (→ 30%)

**Goal:** the daemon is reachable by extensions and the UI; messages route to handlers.

**Files:**
```
daemon/crates/daemon/src/main.rs
daemon/crates/daemon/src/server/mod.rs
daemon/crates/daemon/src/server/websocket.rs
daemon/crates/daemon/src/protocol.rs        # ClientMessage / ServerMessage enums
daemon/crates/daemon/src/state.rs           # AppState: db handle, client registry, broadcaster
```

**Tasks:**
1. Add deps: `tokio` (full), `tokio-tungstenite`, `futures-util`, `serde`, `serde_json`.
2. Bind WS server to `127.0.0.1:60123` (loopback only — see Security).
3. Define `ClientMessage` / `ServerMessage` enums with `#[serde(tag = "type")]` to match
   the TypeScript protocol in ARCHITECTURE §5.3.
4. Maintain a client registry + a `tokio::sync::broadcast` channel so events fan out to all
   connected clients (extensions + UI).
5. Implement handlers (stubs OK for now): `GET_ALL_JOBS` returns DB rows; `ADD_DOWNLOAD`
   inserts a Queued job and broadcasts `JOB_ADDED`.

**DoD:**
- [x] A WebSocket client connects to `ws://127.0.0.1:60123` (verified via .NET `ClientWebSocket`).
- [x] Sending `{"type":"GET_ALL_JOBS"}` returns `{"type":"ALL_JOBS", ...}`.
- [x] Sending `ADD_DOWNLOAD` persists a Queued job and broadcasts `JOB_ADDED` to all clients.
- [x] Job survives a second `GET_ALL_JOBS` (persistence) and wire format is camelCase (TS contract).

**Implementation notes:**
- `state::AppState` = `Mutex<Connection>` + `broadcast::Sender<ServerMessage>`; handlers lock
  the DB only briefly and never across `.await`.
- `server::websocket` runs a per-connection `select!` loop: reads `ClientMessage`s and forwards
  broadcast events. `GetAllJobs` replies directly; `AddDownload` inserts + broadcasts `JobAdded`.
- Models serialize camelCase (`#[serde(rename_all = "camelCase")]`) to match `ui/src/types.ts`.
- DB path: `%APPDATA%\UDM\jobs.db` (Windows) / `$HOME/.udm` / cwd fallback.
- Enabled `tracing-subscriber` `env-filter`; added `rusqlite` dep to the daemon crate.
- Known minor: server's close-frame echo doesn't satisfy .NET's strict close handshake;
  harmless for browser/UI clients (lenient close + reconnect loops). PAUSE/RESUME/CANCEL/
  SET_PRIORITY/SET_BANDWIDTH are parsed and logged, wired to the engine in Phases 4–5.

---

## Phase 3 — Single-Segment Download Engine (→ 45%)

**Goal:** a real file downloads end-to-end, with progress events.

**Files:**
```
daemon/crates/engine/src/lib.rs
daemon/crates/engine/src/downloader.rs      # stream_download()
daemon/crates/engine/src/file_manager.rs    # temp/.part paths, final move
daemon/crates/engine/src/progress.rs        # AtomicU64 counter + 500ms emitter
```

**Tasks:**
1. Add deps: `reqwest` (stream), `tokio`, `futures-util`, `bytes`.
2. Implement `stream_download(job, client)`: GET → stream body → write to `name.part`.
3. Update a shared `AtomicU64` per chunk; a ticker emits `JOB_PROGRESS` every 500ms with
   `downloadedBytes`, `speedBps`, `eta`.
4. On finish: move `name.part` → final `name`, set status `Completed`, emit `JOB_COMPLETED`.
5. Wire engine to the Phase 2 `ADD_DOWNLOAD` handler.

**DoD:**
- [x] Download a 3 MiB test asset end-to-end via the WS client (throttled local Node server).
- [x] Progress events stream while downloading (6× `JOB_PROGRESS`); ETA decreases.
- [x] Final file size matches `Content-Length` (3,145,728 bytes); `.part` cleaned up.

**Implementation notes:**
- Engine is decoupled from the wire protocol: `stream_download` emits `EngineEvent`
  (`Started`/`Progress`/`Completed`/`Failed`) over an `mpsc` channel.
- `daemon/src/bridge.rs` consumes events → DB writes (`update_job_progress`, `set_status`) +
  `ServerMessage` broadcasts. `spawn_download` runs each job on its own task; emits `Failed` on error.
- Progress ticker: shared `AtomicU64` byte counter; a 500ms `interval` task computes
  `speed_bps` (bytes/window ×2) and `eta = remaining / speed`.
- `file_manager`: `part_path` (`<save>.part`), `ensure_parent_dir`, `finalize` (atomic rename,
  replaces existing). Request forwards `User-Agent`, `Referer`, `Cookie`, and custom headers.
- `AppState` gained a shared `reqwest::Client` + `engine_tx`. Phase 4 will add the HEAD probe
  and branch to multi-segment.

---

## Phase 4 — Multi-Segment + Pause/Resume (→ 60%)

**Goal:** parallel chunked downloads that survive pause and crash.

**Files:**
```
daemon/crates/engine/src/chunker.rs         # calculate_segments, split_ranges
daemon/crates/engine/src/segment.rs         # download_segment()
daemon/crates/engine/src/assembler.rs       # assemble_parts()
daemon/crates/engine/src/downloader.rs      # HEAD probe + branch to multi/single
```

**Tasks:**
1. HEAD request → read `Content-Length` + `Accept-Ranges: bytes`. If unsupported or size
   unknown → fall back to Phase 3 `stream_download`.
2. `calculate_segments(size, max)` with `MIN_CHUNK_SIZE = 1 MiB`; `split_ranges` into N.
3. Spawn one task per range with `Range: bytes=start-end` → write `name.part.N`.
4. **Pause:** signal tasks to stop, persist each segment's `downloaded` to DB.
5. **Resume:** reload segments; restart each from `start + downloaded` (re-issue Range).
6. `assemble_parts`: concatenate `.part.0..N` in order → final file; verify checksum if present.

**DoD:**
- [x] Multi-segment (5 segments) produces a byte-identical file vs. single-stream (same SHA-256).
- [x] Pause mid-download, restart the daemon, resume → continues from the persisted offset
      (not from scratch) and completes byte-identically.
- [x] Server without `Range` support cleanly falls back to single stream (0 segments).

**Implementation notes:**
- `downloader::download` is the entry: resume (segments present) skips the probe; fresh jobs
  HEAD-probe for size + `Accept-Ranges`, then branch to `multi_segment` or `single_stream`.
- `segment::download_segment` issues `Range: bytes=start+already-end`, appends on resume,
  cooperatively cancellable via `CancellationToken` (`tokio::select!`).
- `assembler::assemble_parts` concatenates `.part.N` in order, then deletes parts.
- Pause: `AppState.active` maps job id → `CancellationToken`; `PAUSE` cancels it, segments
  flush + return, `multi_segment` emits `Paused{segments}`; bridge persists per-segment bytes.
  Resume reloads the job and re-runs `download` (segments populated).
- **Bugs found & fixed via tests:** (1) reqwest `content_length()` returns `Some(0)` for HEAD —
  read the `Content-Length` header directly in the probe; (2) `ClientMessage` struct-variant
  fields weren't camelCased, so `PAUSE`/`RESUME` `jobId` failed to deserialize and were silently
  dropped — added `#[serde(rename_all="camelCase")]` per variant.
- Checksum verification deferred (no `checksum` provided in current flows) — TODO in assembler.

---

## Phase 5 — Queue, Scheduler & Throttle (→ 70%)

**Goal:** controlled concurrency, priority ordering, and bandwidth limiting.

**Files:**
```
daemon/crates/daemon/src/queue/scheduler.rs # BinaryHeap priority + Semaphore
daemon/crates/daemon/src/queue/throttle.rs  # TokenBucket
daemon/crates/daemon/src/config/settings.rs # runtime settings + schedule window
```

**Tasks:**
1. Priority queue (`BinaryHeap` by `priority`); `tokio::sync::Semaphore` caps
   `max_concurrent_downloads`.
2. `TokenBucket` shared across active workers; `consume(bytes)` awaits tokens (ARCHITECTURE §9.3).
3. `SET_BANDWIDTH` message reconfigures the bucket live; `null` = unlimited.
4. Schedule window: only auto-start jobs when current time ∈ allowed window/days.
5. Handlers: `PAUSE`, `RESUME`, `CANCEL`, `SET_PRIORITY`.

**DoD:**
- [x] With cap = 2, exactly 2 jobs run (Active) and the rest stay Queued.
- [x] Bandwidth cap honored: 5 MiB at 1 MiB/s took ~4.8 s (≈ expected).
- [x] Higher-priority job starts first: C(200) completed before B(50) once a slot freed.

**Implementation notes:**
- `queue::scheduler` runs as a single task owning all queue state (no locks/races).
  `SchedulerCmd`: `Enqueue` / `SlotFreed` / `SetMaxConcurrent` / `SetPriority` / `RemovePending`.
  Pending jobs live in a `BinaryHeap<Queued>` (priority desc, then FIFO via seq).
- `ADD_DOWNLOAD` and `RESUME` now **enqueue** (status stays Queued until a slot frees);
  `bridge::start_download` (called only by the scheduler) runs the job and sends `SlotFreed`
  when it ends — that's what lets the next queued job start.
- Bandwidth: `engine::TokenBucket` (debt-based leaky bucket), shared `Arc` in `AppState`,
  consumed per chunk in segment + single-stream paths; `SET_BANDWIDTH` reconfigures live
  (`null`/0 = unlimited). Re-exported via `queue::throttle`.
- `CANCEL`: removes from pending; if running, cancels with a "cancelling" intent so the
  resulting stop is recorded as Cancelled (not Paused), deletes part files, broadcasts
  `JobCancelled`. `SET_PRIORITY` re-heapifies the queued entry and persists.
- Schedule window: `config::settings::is_within` (handles overnight + weekday limits, unit-tested);
  scheduler holds an optional window (default none = always run).
- Limits resolved at startup from settings, overridable via `UDM_MAX_CONCURRENT` /
  `UDM_MAX_BANDWIDTH_KBPS` (used by tests).

---

## Phase 6 — Chrome Extension MV3 (→ 80%)

**Goal:** browser downloads are intercepted and forwarded to the daemon.

**Files:**
```
extension/chrome/manifest.json
extension/chrome/background/service-worker.js
extension/chrome/popup/popup.html + popup.js + popup.css
extension/shared/protocol.js                # shared message shapes
```

**Tasks:**
1. Manifest perms: `downloads`, `webRequest`, `cookies`, `storage`, `notifications`, `contextMenus`.
2. Service worker: WebSocket connect + 3s reconnect loop; queue messages while disconnected.
3. `chrome.downloads.onCreated` → `chrome.downloads.cancel(id)` → gather cookies → send
   `ADD_DOWNLOAD` (see ARCHITECTURE §7.2).
4. Popup: active count + aggregate speed; a "Download with UDM" context-menu item (optional).

**DoD:**
- [x] Interception logic implemented (cancel + erase + forward); contract proven e2e —
      the extension's own `buildAddDownload` output downloads a file via the live daemon.
      (Literal browser-click interception is manual — see `extension/README.md`.)
- [x] Cookies/referrer forwarded so auth-gated downloads work (verified e2e).
- [x] Extension auto-reconnects when the daemon restarts (3 s loop; intercept-only-when-connected).

**Implementation notes:**
- Pure logic in `chrome/lib/udm-core.js` (`shouldIntercept`, `formatCookies`, `basename`,
  `buildAddDownload`, `summarize`) — 7 Node unit tests.
- Module service worker (`"type":"module"`): WS connect + reconnect, `downloads.onCreated`
  → cancel + erase + forward, `contextMenus` "Download with UDM", completion notifications.
- **Safety choice:** only intercepts while connected, so downloads are never lost if the
  daemon is down (they fall through to the browser).
- Popup opens its own short-lived socket for live status (decoupled from the worker, which
  MV3 may suspend).
- Tests: `extension/test/core.test.mjs` (unit), `extension/scripts/e2e.mjs` (contract e2e via
  Node 22 global `WebSocket`).

---

## Phase 7 — Firefox + Edge Extensions (→ 85%)

**Goal:** same interception across browsers.

**Files:**
```
extension/firefox/manifest.json             # MV2
extension/firefox/background/background.js   # browser.* namespace
extension/edge/                              # Chromium MV3 — reuse chrome build
```

**Tasks:**
1. Firefox: MV2 manifest, `background.scripts`, `browser.*` (or webextension-polyfill).
2. Edge: Chromium MV3 — the Chrome build works as-is; package separately for the store.
3. Verify in `about:debugging` (Firefox) and `edge://extensions`.

**DoD:**
- [x] Firefox build reuses the byte-identical `lib/udm-core.js` (same interception decision +
      payload), MV2 wrapper valid, syntax-checked. Browser-level test is manual (`about:debugging`).
- [x] Edge runs the Chrome build unmodified (MV3) — documented load steps in `edge/README.md`.

**Implementation notes:**
- Firefox MV2 uses a **persistent background page** (`background.html` → `<script type="module">`)
  so it can `import` the shared core (MV2 `background.scripts` can't use ES modules).
- `firefox/background/background.js` uses `const api = browser ?? chrome` (promise-based),
  `browserAction` for the badge, `runtime.getURL` for icons — otherwise mirrors the Chrome worker.
- `scripts/sync-lib.mjs` copies `chrome/lib/udm-core.js` into every build; `--check` mode fails
  on drift (run in CI). Firefox lib verified in sync.
- Edge: no code files — zip `extension/chrome/` for the store.

---

## Phase 8 — Tauri Desktop UI (→ 95%)

**Goal:** a live UI to observe and control downloads.

**Files:**
```
ui/src/App.tsx
ui/src/hooks/useDaemonSocket.ts             # WS to 127.0.0.1:60123
ui/src/store/downloads.ts                   # Zustand store
ui/src/components/DownloadList.tsx / DownloadItem.tsx
ui/src/components/Toolbar.tsx / SpeedGraph.tsx / Settings.tsx
ui/src-tauri/src/main.rs                    # tray + (optionally) embed daemon
ui/src-tauri/tauri.conf.json
```

**Tasks:**
1. `useDaemonSocket`: connect, `GET_ALL_JOBS` on open, dispatch events into the store,
   reconnect on close (ARCHITECTURE §10 Step 11).
2. `DownloadList` / `DownloadItem`: filename, progress bar, speed, ETA, pause/resume/cancel.
3. `SpeedGraph` (recharts) over the last 60s; `Settings` panel (dir, concurrency, cap, schedule).
4. `AddURLModal`: paste URL → validate → `ADD_DOWNLOAD`.
5. System tray: show/pause-all/quit; active-count badge.

**DoD:**
- [x] UI shows existing jobs on launch (GET_ALL_JOBS on connect) and live progress
      for new ones (JOB_PROGRESS → store → segmented progress bars + speed/ETA).
- [x] Pause/resume/cancel/remove from the UI control the daemon (PAUSE/RESUME/
      CANCEL/REMOVE messages); status badges and controls update from broadcasts.
- [x] Settings changes persist and take effect live — verified e2e: GET_SETTINGS
      returns the camelCase blob, UPDATE_SETTINGS persists + applies bandwidth and
      concurrency immediately and echoes SETTINGS_UPDATED to all clients.

**Implementation notes:**
- **Daemon protocol additions (Phase 8 contract):** `GET_SETTINGS` →
  `SETTINGS_UPDATED`; `UPDATE_SETTINGS { settings }` persists via
  `db::save_settings`, applies live (`limiter.set_rate` + scheduler
  `SetMaxConcurrent`) and re-broadcasts; new `REMOVE { jobId }` →
  `bridge::remove` (stops if running, deletes the row + part files, broadcasts
  `JOB_REMOVED`). `AppSettings` now serializes `camelCase` to match the TS
  contract. `ADD_DOWNLOAD` resolves the save path against the configured
  `downloadDirectory` when the client doesn't pin one.
- **UI is dependency-free** (no Zustand/recharts): a tiny external store
  (`store/downloads.ts`) consumed via `useSyncExternalStore`; a module-level
  singleton socket (`hooks/useDaemonSocket.ts`) with a 2s reconnect loop, an
  outbox that queues messages while disconnected, and a 1s aggregate-speed
  sampler; the speed chart is a hand-rolled SVG area path.
- **Components:** `Toolbar` (connection dot, search, aggregate stats, Add URL /
  Pause-all / Settings), `AddURLModal` (http(s) validation + priority),
  `DownloadList` (status tabs with counts + search filter + empty state),
  `DownloadItem` (segment-aware progress bar, contextual controls, open-file /
  reveal-in-folder via `tauri-plugin-opener`), `Settings` (dir, concurrency,
  segments, bandwidth cap, tray/boot toggles), `Toasts` (completion/failure).
- **Tauri shell (`src-tauri/src/lib.rs`):** system tray (Show / Pause-all /
  Quit) via the `tray-icon` feature; left-click shows the window; "Pause all"
  emits `tray-pause-all` which the webview turns into PAUSE messages; window
  close hides to tray (close-to-tray) instead of exiting. Added `opener`
  allow-list permissions for open-path / reveal-item-in-dir.
- **Verification:** `npm run build` (tsc + vite) clean; `cargo check` clean for
  the daemon workspace and the Tauri crate; `cargo test --workspace` =
  7 (daemon) + 5 (engine) + 7 (storage) passing; live settings smoke test PASS.
- **Honest scope note:** building the desktop installer (`cargo tauri build`)
  and exercising the tray on a packaged binary is Phase 9. What's verified here
  is that the frontend bundles, the Rust shell compiles with the tray wired, and
  the daemon↔UI message contract works against a live daemon.

---

## Phase 9 — Polish, Security, Packaging (→ 100%)

**Goal:** shippable, signed, and store-ready.

**Files:**
```
daemon/crates/daemon/src/security/auth.rs   # optional loopback bearer token
ui/src-tauri/tauri.conf.json                # updater, bundle targets, signing
.github/workflows/ci.yml                     # build + test matrix
docs/RELEASE_CHECKLIST.md
```

**Tasks:**
1. Security: confirm WS binds to `127.0.0.1` only; validate/normalize URLs and `save_path`
   (reject path traversal); never log cookies; run as the user (never root). Optional bearer
   token in the OS keychain.
2. Auto-update: Tauri `tauri-plugin-updater`; daemon checks GitHub Releases on start.
3. Installers: `cargo tauri build` for macOS (.dmg), Windows (.msi/.exe), Linux (.AppImage/.deb).
4. Code-sign (Apple Developer ID, Windows cert).
5. CI matrix: build + test daemon and UI on Linux/macOS/Windows; zip extensions.
6. Store submissions: Chrome Web Store, Firefox AMO, Edge Add-ons.

**DoD:**
- [x] Security checklist (ARCHITECTURE §11) implemented + unit-tested: loopback-only
      bind, http(s)-only URL validation, filename sanitization, save-path containment,
      cookies never logged, optional bearer token. (verified e2e — see notes)
- [x] CI runs fmt + clippy (`-D warnings`) + tests for the daemon, frontend build +
      shell `cargo check` for the UI, and lint/test/zip for the extensions; the
      release job (on `v*` tags) builds installers and uploads them. Every command the
      CI runs passes locally.
- [ ] **(manual)** Signed installers run on a clean machine — needs Apple Developer ID /
      Windows cert (documented in RELEASE_CHECKLIST.md §4).
- [ ] **(manual)** Auto-update verified on all platforms — updater wiring is documented
      (RELEASE_CHECKLIST.md §5); needs a signing key + release endpoint.

**Implementation notes:**
- **Security (`daemon/src/security/`):** `validate::validate_url` (parses with the `url`
  crate; only `http`/`https` with a host), `validate::safe_filename` (reduces untrusted
  names to a separator-free basename, drops control chars, never `.`/`..`/empty),
  `validate::is_within` (lexical `.`/`..` normalization to keep explicit save paths inside
  the download dir). Wired into `ADD_DOWNLOAD`: invalid URLs are rejected pre-job;
  filenames are always sanitized; escaping save paths fall back to the configured dir.
- **Optional auth (`security::auth`):** off unless `UDM_AUTH_TOKEN` is set. When set, the
  WS handshake uses `accept_hdr_async` and requires a matching `?token=` query (constant-
  time-ish compare), returning `401` otherwise. **Verified e2e:** no-token and wrong-token
  connections are rejected, the correct token connects.
- **Packaging:** `tauri.conf.json` gained bundle metadata (publisher, category, copyright,
  descriptions, NSIS perMachine). `docs/RELEASE_CHECKLIST.md` expanded into a full pre-flight
  / security / build / signing / updater / store / publish checklist.
- **CI (`.github/workflows/ci.yml`):** matrix over ubuntu/macos/windows; rust-cache; Linux
  WebKitGTK deps for the Tauri check; extension sync-lib `--check` + `node --test`; a tagged
  `release` job using `tauri-apps/tauri-action` to attach installers to a draft release.
- **Verification:** `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets
  -D warnings` clean (fixed a pre-existing doc-lint + a `Default` lint along the way);
  `cargo test --workspace` = 16 (daemon) + 5 (engine) + 7 (storage); extension `node --test`
  = 7; `npm run build` clean; auth + settings smoke tests PASS against a live daemon.
- **Honest scope note:** code-signing, notarization, the live auto-updater, and store
  submissions inherently need certificates / developer accounts and are documented as
  manual steps — they can't be exercised in this environment. Everything that *can* be
  automated and verified (security hardening, the CI definition, packaging metadata) is
  done and green locally.

---

## Post-Phase 9 — Daily-Use Features (2026-07-04)

Added for daily-driver production use; all verified e2e against a live daemon:

- **Overwrite protection:** `ADD_DOWNLOAD` resolves a unique target
  (`name (1).ext`) when the file exists on disk or another Queued/Active/Paused
  job reserves the same path (`engine::file_manager::unique_path`).
- **Auto-retry:** `bridge::start_download` retries transient failures (network
  errors, 5xx) up to 3× with 2s/5s/15s backoff before marking Failed; 4xx fails
  fast. Verified: local 503×2-then-200 server completes without a client-visible
  failure.
- **Disk reconciliation:** before every (re)start, segment `downloaded` offsets
  are reset to the actual `.part.N` file sizes (capped/truncated to segment
  length) — fixes corrupt resumes after a crash with stale DB counters.
- **Manual retry:** new `RETRY { jobId }` client message re-queues a
  failed/cancelled job (clears error, re-broadcasts `JOB_ADDED`); UI shows ↻ on
  failed/cancelled rows.
- **Auto-categorize:** `auto_categorize` setting (default on) sorts defaulted
  save paths into `Video/ Music/ Images/ Documents/ Archives/ Programs/`
  subfolders by extension (`daemon::categorize`); explicit save paths are never
  redirected.
- **Batch add:** the Add-download modal accepts multiple URLs (whitespace/
  newline separated) and dispatches one `ADD_DOWNLOAD` each.
- **Clipboard watcher:** `clipboard_watcher` setting (default off; Settings
  toggle) polls the clipboard every 1.5 s in the UI
  (`hooks/useClipboardWatcher.ts`, `tauri-plugin-clipboard-manager`) and opens
  the New Download prompt for copied file-looking URLs.
- **Installers:** `npm run tauri build` produces
  `src-tauri/target/release/bundle/{msi,nsis}/UDM_0.1.0_x64*` (unsigned;
  signing/store steps remain manual per RELEASE_CHECKLIST).

**Dev gotcha:** a long-idle `tauri dev` webview can go stale (HMR disconnect) —
broadcast-driven popups silently stop appearing until the next HMR push or a
manual reload. Not a production concern (no HMR in bundles).

---

## Cross-Phase Testing (run continuously)

- **Unit:** `split_ranges`, `calculate_segments`, token bucket rate, DB round-trips.
- **Integration:** local `axum` test server → single, multi, resume-after-interrupt, no-Range
  fallback, checksum verification.
- **Extension:** Playwright + headless Chrome — verify intercept + cancel + daemon receipt.
- **Load:** 50 simultaneous downloads — cap respected, no corruption, correct assembly.

## Suggested Calendar

```
Wk 1-2  Phase 0-1   scaffold + storage
Wk 3    Phase 2-3   websocket + single-segment
Wk 4    Phase 4-5   multi-segment + queue/throttle
Wk 5    Phase 6     chrome extension
Wk 6    Phase 7     firefox + edge
Wk 7-8  Phase 8     tauri UI
Wk 9-10 Phase 9     polish, security, packaging, stores
```
