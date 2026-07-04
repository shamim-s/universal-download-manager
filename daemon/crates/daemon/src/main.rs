//! UDM daemon entry point.
//!
//! Phase 2: open the database and run the WebSocket server on 127.0.0.1:60123.

mod bridge;
mod categorize;
mod protocol;
mod security;
mod server;
mod state;
mod config {
    pub mod settings;
}
mod queue {
    pub mod scheduler;
    pub mod throttle;
}

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::queue::throttle::TokenBucket;
use crate::state::AppState;

/// Loopback bind address for extensions and the UI.
const BIND_ADDR: &str = "127.0.0.1:60123";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path = data_dir()?.join("jobs.db");
    tracing::info!("opening database at {}", db_path.display());
    let conn = udm_storage::db::init(&db_path).context("initializing database")?;

    let pending = udm_storage::db::get_pending_jobs(&conn)?.len();
    tracing::info!("loaded {pending} pending job(s) from previous session");

    // Resolve runtime limits (env overrides settings, for testing/headless use).
    let settings = udm_storage::db::load_settings(&conn).unwrap_or_default();
    let max_concurrent = env_u64("UDM_MAX_CONCURRENT")
        .map(|n| n as u8)
        .unwrap_or(settings.max_concurrent_downloads);
    let init_bps = env_u64("UDM_MAX_BANDWIDTH_KBPS")
        .map(|k| k * 1024)
        .or_else(|| settings.max_bandwidth_kbps.map(|k| k as u64 * 1024))
        .unwrap_or(0);
    tracing::info!("max concurrent: {max_concurrent}, bandwidth: {init_bps} B/s (0=unlimited)");

    let limiter = Arc::new(TokenBucket::new(init_bps));

    // Optional loopback bearer token (off unless UDM_AUTH_TOKEN is set).
    let auth_token = security::auth::token_from_env();
    if auth_token.is_some() {
        tracing::info!("loopback bearer token required (UDM_AUTH_TOKEN set)");
    }

    // Channels: engine events -> bridge consumer; scheduler commands -> scheduler.
    let (engine_tx, engine_rx) = tokio::sync::mpsc::unbounded_channel();
    let (sched_tx, sched_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = Arc::new(AppState::new(
        conn, engine_tx, sched_tx, limiter, auth_token,
    ));

    tokio::spawn(bridge::consume_events(Arc::clone(&state), engine_rx));
    tokio::spawn(queue::scheduler::run(
        Arc::clone(&state),
        sched_rx,
        max_concurrent,
    ));

    server::websocket::run(state, BIND_ADDR).await
}

/// Parse an unsigned integer from an environment variable, if present and valid.
fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

/// Per-user data directory for the daemon (created if missing).
/// Windows: `%APPDATA%\UDM`; otherwise `$HOME/.udm`; fallback: current dir.
fn data_dir() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".udm")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = if base.ends_with(".udm") {
        base
    } else {
        base.join("UDM")
    };
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating data dir {}", dir.display()))?;
    Ok(dir)
}
