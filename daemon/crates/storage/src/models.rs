//! Data models shared across the daemon. See docs/ARCHITECTURE.md §3.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub id: Uuid,
    pub url: String,
    pub filename: String,
    pub save_path: PathBuf,
    pub file_size: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: JobStatus,
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub referrer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: String,
    pub headers: HashMap<String, String>,
    pub segments: Vec<Segment>,
    pub checksum: Option<Checksum>,
    pub source_browser: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Active,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    /// Stable lowercase string used in the `status` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Active => "active",
            JobStatus::Paused => "paused",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    /// Parse the value stored in the `status` column.
    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => JobStatus::Queued,
            "active" => JobStatus::Active,
            "paused" => JobStatus::Paused,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: u8,
    pub byte_start: u64,
    pub byte_end: u64,
    pub downloaded: u64,
    pub temp_file: PathBuf,
    pub status: JobStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: String,
    pub expected: String,
    pub verified: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub download_directory: PathBuf,
    pub max_concurrent_downloads: u8,
    pub max_segments_per_download: u8,
    pub max_bandwidth_kbps: Option<u32>,
    pub auto_start_on_boot: bool,
    pub minimize_to_tray: bool,
    pub daemon_port: u16,
    /// When true (IDM-style default), an intercepted browser download opens the
    /// "New Download" prompt instead of starting silently to the default dir.
    #[serde(default = "default_true")]
    pub prompt_before_download: bool,
    /// Sort downloads into per-type subfolders (Video, Music, Documents, …)
    /// of the download directory. Only applies when the client didn't pick an
    /// explicit save location.
    #[serde(default = "default_true")]
    pub auto_categorize: bool,
    /// UI-side: watch the clipboard for copied download URLs and offer to add
    /// them. Stored here so every client stays in sync.
    #[serde(default)]
    pub clipboard_watcher: bool,
}

fn default_true() -> bool {
    true
}

/// The user's Downloads folder, falling back to the home dir, then the current
/// directory. Used as the default save location so downloads never land in the
/// daemon's working directory (Program Files on install, `src-tauri` in dev).
pub fn default_download_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    match home {
        Some(h) => {
            let dl = h.join("Downloads");
            if dl.is_dir() {
                dl
            } else {
                h
            }
        }
        None => PathBuf::from("."),
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_directory: default_download_dir(),
            max_concurrent_downloads: 3,
            max_segments_per_download: 8,
            max_bandwidth_kbps: None,
            auto_start_on_boot: false,
            minimize_to_tray: true,
            daemon_port: 60123,
            prompt_before_download: true,
            auto_categorize: true,
            clipboard_watcher: false,
        }
    }
}
