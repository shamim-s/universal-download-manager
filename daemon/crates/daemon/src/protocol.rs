//! WebSocket message protocol (Phase 2). Mirrors extension/shared/protocol.js
//! and ui/src/types.ts. See docs/ARCHITECTURE.md §4.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use udm_storage::models::AppSettings;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMessage {
    AddDownload {
        payload: AddDownloadPayload,
    },
    /// An intercepted browser download awaiting user confirmation. The daemon
    /// either opens the "New Download" prompt (default) or, when the
    /// `prompt_before_download` setting is off, queues it like `AddDownload`.
    PromptDownload {
        payload: AddDownloadPayload,
    },
    #[serde(rename_all = "camelCase")]
    Pause {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Resume {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Cancel {
        job_id: String,
    },
    /// Re-queue a failed job (clears the error; resumes from any valid parts).
    #[serde(rename_all = "camelCase")]
    Retry {
        job_id: String,
    },
    /// Remove a job from the list entirely (deletes the row + any part files).
    #[serde(rename_all = "camelCase")]
    Remove {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    SetPriority {
        job_id: String,
        priority: u8,
    },
    GetAllJobs,
    SetBandwidth {
        kbps: Option<u32>,
    },
    /// Ask the daemon to reply with the current persisted settings.
    GetSettings,
    /// Persist new settings and apply them live (concurrency, bandwidth, dir).
    UpdateSettings {
        settings: AppSettings,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDownloadPayload {
    pub url: String,
    pub filename: Option<String>,
    pub save_path: Option<String>,
    pub cookies: Option<String>,
    pub referrer: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub source_browser: String,
    /// Optional priority 0 (low) – 255 (high); defaults to 128.
    pub priority: Option<u8>,
    /// Optional content length known up-front (e.g. from the browser), shown in
    /// the New Download prompt before the transfer starts.
    #[serde(default)]
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    JobAdded {
        job: serde_json::Value,
    },
    /// Ask the UI to open the "New Download" prompt for an intercepted download.
    /// `payload` is a serialized `AddDownloadPayload`.
    DownloadPrompt {
        payload: serde_json::Value,
    },
    /// Emitted once a download begins and its size + segment plan are known, so
    /// clients can render an accurate progress bar (JobAdded arrives before the
    /// size is discovered).
    #[serde(rename_all = "camelCase")]
    JobStarted {
        job_id: String,
        file_size: Option<u64>,
        segment_count: u32,
    },
    #[serde(rename_all = "camelCase")]
    JobProgress {
        job_id: String,
        downloaded_bytes: u64,
        speed_bps: u64,
        eta: u64,
    },
    #[serde(rename_all = "camelCase")]
    JobCompleted {
        job_id: String,
        final_path: String,
        /// Final byte count, so clients can show an exact 100% (the last
        /// JOB_PROGRESS tick can be up to 500 ms stale).
        total_bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    JobFailed {
        job_id: String,
        error: String,
    },
    #[serde(rename_all = "camelCase")]
    JobPaused {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    JobCancelled {
        job_id: String,
    },
    /// A job was deleted from the list entirely (UI should drop the row).
    #[serde(rename_all = "camelCase")]
    JobRemoved {
        job_id: String,
    },
    AllJobs {
        jobs: Vec<serde_json::Value>,
    },
    SettingsUpdated {
        settings: serde_json::Value,
    },
}
