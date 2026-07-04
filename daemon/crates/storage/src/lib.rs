//! UDM storage crate — SQLite persistence for jobs and settings.
//!
//! Phase 1 target (see docs/BUILD_PHASES.md). Implement `db` + `models` here.

pub mod db;
pub mod models;

pub use models::{AppSettings, Checksum, DownloadJob, JobStatus, Segment};
