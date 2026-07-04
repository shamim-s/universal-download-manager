//! UDM download engine.
//!
//! Phase 3: `downloader::stream_download` (single segment) + `EngineEvent`.
//! Phase 4: `chunker` + `segment` + `assembler` (multi-segment, resume).

pub mod assembler;
pub mod chunker;
pub mod downloader;
pub mod event;
pub mod file_manager;
pub mod progress;
pub mod segment;
pub mod throttle;

pub use downloader::download;
pub use event::EngineEvent;
pub use throttle::TokenBucket;

/// Minimum chunk size; avoids tiny inefficient range requests.
pub const MIN_CHUNK_SIZE: u64 = 1024 * 1024; // 1 MiB
