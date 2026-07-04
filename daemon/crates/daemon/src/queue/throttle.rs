//! Bandwidth limiting (Phase 5).
//!
//! The token-bucket implementation lives in the engine crate (it is consumed by
//! the download tasks). The daemon holds a shared `Arc<TokenBucket>` in
//! `AppState` and reconfigures it via the `SET_BANDWIDTH` message.

pub use udm_engine::TokenBucket;
