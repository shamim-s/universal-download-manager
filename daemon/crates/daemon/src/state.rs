//! Shared application state (Phases 2–5): DB handle, event broadcaster, HTTP
//! client, engine + scheduler channels, bandwidth limiter, and control maps.
//!
//! The SQLite `Connection` is `Send` but not `Sync`, so it lives behind a
//! `std::sync::Mutex`. Handlers lock it only briefly and never hold the guard
//! across an `.await`, keeping connection futures `Send`.

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use udm_engine::EngineEvent;

use crate::protocol::ServerMessage;
use crate::queue::scheduler::SchedulerCmd;
use crate::queue::throttle::TokenBucket;

/// Capacity of the per-process event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 256;

pub struct AppState {
    pub db: Mutex<Connection>,
    /// Fan-out of server events to every connected client (extensions + UI).
    pub events: broadcast::Sender<ServerMessage>,
    /// Shared HTTP client (connection pooling) for the engine.
    pub http: reqwest::Client,
    /// Producer side of the engine event channel; the consumer lives in `bridge`.
    pub engine_tx: mpsc::UnboundedSender<EngineEvent>,
    /// Commands to the scheduler task (`queue::scheduler`).
    pub sched_tx: mpsc::UnboundedSender<SchedulerCmd>,
    /// Shared bandwidth limiter for all active downloads.
    pub limiter: Arc<TokenBucket>,
    /// Cancellation tokens for currently-running downloads, keyed by job id.
    pub active: Mutex<HashMap<Uuid, CancellationToken>>,
    /// Jobs whose cancellation should be treated as Cancelled (not Paused).
    pub cancelling: Mutex<HashSet<Uuid>>,
    /// Optional loopback bearer token; when set, clients must present it on the
    /// WebSocket handshake. `None` disables auth (loopback-only access).
    pub auth_token: Option<String>,
}

impl AppState {
    pub fn new(
        conn: Connection,
        engine_tx: mpsc::UnboundedSender<EngineEvent>,
        sched_tx: mpsc::UnboundedSender<SchedulerCmd>,
        limiter: Arc<TokenBucket>,
        auth_token: Option<String>,
    ) -> Self {
        let (events, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            db: Mutex::new(conn),
            events,
            http: reqwest::Client::new(),
            engine_tx,
            sched_tx,
            limiter,
            active: Mutex::new(HashMap::new()),
            cancelling: Mutex::new(HashSet::new()),
            auth_token,
        }
    }

    /// Broadcast an event to all connected clients. Errors only when there are
    /// no subscribers, which is fine to ignore.
    pub fn broadcast(&self, msg: ServerMessage) {
        let _ = self.events.send(msg);
    }
}
