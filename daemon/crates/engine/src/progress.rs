//! Shared progress counter + periodic emitter (Phase 3).
//!
//! TODO:
//!   - a shared AtomicU64 of downloaded bytes per job
//!   - a 500ms ticker that computes speedBps + eta and emits JOB_PROGRESS
