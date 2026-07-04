//! Events emitted by the download engine.
//!
//! The engine is decoupled from the daemon's wire protocol: it emits these
//! plain events over a channel, and the daemon translates them into
//! `ServerMessage`s and DB updates (see `daemon/src/bridge.rs`).

use std::path::PathBuf;
use udm_storage::models::Segment;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Download has begun. `total` is the Content-Length if known; `segments`
    /// is the (possibly empty) segment plan — persisted so a crash can resume.
    Started {
        job_id: Uuid,
        total: Option<u64>,
        segments: Vec<Segment>,
    },
    /// Periodic progress tick (~every 500ms).
    Progress {
        job_id: Uuid,
        downloaded: u64,
        speed_bps: u64,
        eta_secs: u64,
    },
    /// Download was paused (cancelled cooperatively). `segments` carry the
    /// per-segment byte counts so it can be resumed later.
    Paused {
        job_id: Uuid,
        segments: Vec<Segment>,
    },
    /// Download finished and the file was moved into place.
    Completed {
        job_id: Uuid,
        final_path: PathBuf,
        total: u64,
    },
    /// Download failed.
    Failed { job_id: Uuid, error: String },
}
