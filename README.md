# Universal Download Manager (UDM)

A two-part download manager:

- **Browser extensions** (Chrome / Firefox / Edge) that intercept browser downloads.
- **A native daemon + desktop UI** (Rust + Tauri/React) that accelerates, resumes, queues, and organizes downloads.

## Repository Layout

```
udm-downloader/
├── README.md                 # this file
├── docs/                     # architecture + phased build plan
│   ├── ARCHITECTURE.md       # full system design reference
│   └── BUILD_PHASES.md       # phased 0 → 100% implementation plan  ← START HERE
├── daemon/                   # Rust workspace: daemon + engine + storage
├── extension/                # browser extensions (chrome / firefox / edge / shared)
└── ui/                       # Tauri + React desktop app
```

## Where To Start

1. Read **[docs/BUILD_PHASES.md](docs/BUILD_PHASES.md)** — the step-by-step 0→100% plan.
2. Read **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — the full design reference.
3. Begin with **Phase 0** (toolchain + scaffold verification).

## Quick Start (after Phase 0)

```bash
# Daemon
cd daemon && cargo run -p udm-daemon

# UI (separate terminal)
cd ui && npm install && npm run tauri dev

# Chrome extension: load extension/chrome as an unpacked extension
```

## Tech Stack

| Layer      | Technology                              |
|------------|-----------------------------------------|
| Daemon     | Rust + Tokio, `reqwest`, `tokio-tungstenite` |
| Storage    | SQLite via `rusqlite`                    |
| UI         | Tauri + React + TypeScript + Zustand    |
| Extensions | Vanilla JS (MV3 / MV2)                   |

See `docs/` for details.
