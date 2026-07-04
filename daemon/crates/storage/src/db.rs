//! SQLite access layer (Phase 1, docs/BUILD_PHASES.md).
//!
//! Jobs persist across restarts so the daemon can resume Active/Paused downloads.
//! Complex fields (headers, segments, checksum, tags) are stored as JSON text columns.

use crate::models::{AppSettings, DownloadJob, JobStatus};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const SCHEMA: &str = include_str!("schema.sql");

/// Open (or create) the database and ensure the schema exists.
pub fn init(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA).context("applying schema")?;
    Ok(conn)
}

/// Open an in-memory database with the schema applied (used by tests).
pub fn init_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

/// Insert a new job, or replace an existing one with the same id.
pub fn insert_job(conn: &Connection, job: &DownloadJob) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO jobs (
            id, url, filename, save_path, file_size, downloaded, status, priority,
            created_at, completed_at, error, referrer, cookies, user_agent,
            headers, segments, checksum, source_browser, tags
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
        )",
        params![
            job.id.to_string(),
            job.url,
            job.filename,
            job.save_path.to_string_lossy(),
            job.file_size.map(|v| v as i64),
            job.downloaded_bytes as i64,
            job.status.as_str(),
            job.priority as i64,
            job.created_at.to_rfc3339(),
            job.completed_at.map(|t| t.to_rfc3339()),
            job.error,
            job.referrer,
            job.cookies,
            job.user_agent,
            serde_json::to_string(&job.headers)?,
            serde_json::to_string(&job.segments)?,
            match &job.checksum {
                Some(c) => Some(serde_json::to_string(c)?),
                None => None,
            },
            job.source_browser,
            serde_json::to_string(&job.tags)?,
        ],
    )?;
    Ok(())
}

/// Atomically update a job's downloaded byte count and status.
pub fn update_job_progress(
    conn: &Connection,
    id: &Uuid,
    downloaded_bytes: u64,
    status: JobStatus,
) -> Result<()> {
    let rows = conn.execute(
        "UPDATE jobs SET downloaded = ?2, status = ?3 WHERE id = ?1",
        params![id.to_string(), downloaded_bytes as i64, status.as_str()],
    )?;
    if rows == 0 {
        return Err(anyhow!("no job with id {id}"));
    }
    Ok(())
}

/// Set a job's status, optionally recording an error and/or completion time.
pub fn set_status(
    conn: &Connection,
    id: &Uuid,
    status: JobStatus,
    error: Option<&str>,
    completed_at: Option<DateTime<Utc>>,
) -> Result<()> {
    let rows = conn.execute(
        "UPDATE jobs SET status = ?2, error = ?3, completed_at = ?4 WHERE id = ?1",
        params![
            id.to_string(),
            status.as_str(),
            error,
            completed_at.map(|t| t.to_rfc3339()),
        ],
    )?;
    if rows == 0 {
        return Err(anyhow!("no job with id {id}"));
    }
    Ok(())
}

/// Fetch a single job by id, if present.
pub fn get_job(conn: &Connection, id: &Uuid) -> Result<Option<DownloadJob>> {
    conn.query_row(
        "SELECT * FROM jobs WHERE id = ?1",
        params![id.to_string()],
        row_to_job,
    )
    .optional()
    .map_err(Into::into)
    .and_then(|opt| opt.transpose())
}

/// All jobs, newest first.
pub fn get_all_jobs(conn: &Connection) -> Result<Vec<DownloadJob>> {
    query_jobs(
        conn,
        "SELECT * FROM jobs ORDER BY created_at DESC",
        params![],
    )
}

/// Jobs that should be resumed/processed on startup (Queued, Active, Paused).
pub fn get_pending_jobs(conn: &Connection) -> Result<Vec<DownloadJob>> {
    query_jobs(
        conn,
        "SELECT * FROM jobs WHERE status IN ('queued','active','paused') ORDER BY priority DESC, created_at ASC",
        params![],
    )
}

/// Delete a job row; returns true if a row was removed.
pub fn delete_job(conn: &Connection, id: &Uuid) -> Result<bool> {
    let rows = conn.execute("DELETE FROM jobs WHERE id = ?1", params![id.to_string()])?;
    Ok(rows > 0)
}

fn query_jobs(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<DownloadJob>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, row_to_job)?;
    let mut jobs = Vec::new();
    for r in rows {
        jobs.push(r??);
    }
    Ok(jobs)
}

/// Map a SQLite row into a `DownloadJob`. The inner `Result` carries
/// JSON/UUID/date parse errors so they surface instead of panicking.
fn row_to_job(row: &Row) -> rusqlite::Result<Result<DownloadJob>> {
    // Pull raw columns first (rusqlite errors propagate via `?`).
    let id_s: String = row.get("id")?;
    let url: String = row.get("url")?;
    let filename: String = row.get("filename")?;
    let save_path_s: String = row.get("save_path")?;
    let file_size: Option<i64> = row.get("file_size")?;
    let downloaded: i64 = row.get("downloaded")?;
    let status_s: String = row.get("status")?;
    let priority: i64 = row.get("priority")?;
    let created_s: String = row.get("created_at")?;
    let completed_s: Option<String> = row.get("completed_at")?;
    let error: Option<String> = row.get("error")?;
    let referrer: Option<String> = row.get("referrer")?;
    let cookies: Option<String> = row.get("cookies")?;
    let user_agent: String = row.get("user_agent")?;
    let headers_s: String = row.get("headers")?;
    let segments_s: String = row.get("segments")?;
    let checksum_s: Option<String> = row.get("checksum")?;
    let source_browser: String = row.get("source_browser")?;
    let tags_s: String = row.get("tags")?;

    // Parsing that can fail for non-SQLite reasons is collected in this closure.
    let build = || -> Result<DownloadJob> {
        Ok(DownloadJob {
            id: Uuid::parse_str(&id_s).context("parsing job id")?,
            url,
            filename,
            save_path: PathBuf::from(save_path_s),
            file_size: file_size.map(|v| v as u64),
            downloaded_bytes: downloaded as u64,
            status: JobStatus::from_db(&status_s)
                .ok_or_else(|| anyhow!("unknown status '{status_s}'"))?,
            priority: priority as u8,
            created_at: parse_dt(&created_s)?,
            completed_at: match completed_s {
                Some(s) => Some(parse_dt(&s)?),
                None => None,
            },
            error,
            referrer,
            cookies,
            user_agent,
            headers: serde_json::from_str(&headers_s).context("parsing headers")?,
            segments: serde_json::from_str(&segments_s).context("parsing segments")?,
            checksum: match checksum_s {
                Some(s) => Some(serde_json::from_str(&s).context("parsing checksum")?),
                None => None,
            },
            source_browser,
            tags: serde_json::from_str(&tags_s).context("parsing tags")?,
        })
    };
    Ok(build())
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("parsing datetime '{s}'"))?
        .with_timezone(&Utc))
}

// ---------------------------------------------------------------------------
// Settings (key -> JSON value)
// ---------------------------------------------------------------------------

/// Persist the whole settings blob under a single key.
pub fn save_settings(conn: &Connection, settings: &AppSettings) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('app', ?1)",
        params![serde_json::to_string(settings)?],
    )?;
    Ok(())
}

/// Load settings, falling back to defaults when nothing is stored yet.
pub fn load_settings(conn: &Connection) -> Result<AppSettings> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| {
            r.get(0)
        })
        .optional()?;
    match raw {
        Some(s) => {
            let mut settings: AppSettings = serde_json::from_str(&s).context("parsing settings")?;
            // Normalize a missing/relative save dir (e.g. an empty string or a
            // legacy ".") to an absolute Downloads path so downloads never land
            // in the daemon's working directory.
            if settings.download_directory.as_os_str().is_empty()
                || settings.download_directory == std::path::Path::new(".")
                || settings.download_directory.is_relative()
            {
                settings.download_directory = crate::models::default_download_dir();
            }
            Ok(settings)
        }
        None => Ok(AppSettings::default()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Checksum, Segment};
    use std::collections::HashMap;

    fn sample_job() -> DownloadJob {
        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "*/*".to_string());
        DownloadJob {
            id: Uuid::new_v4(),
            url: "https://example.com/file.zip".into(),
            filename: "file.zip".into(),
            save_path: PathBuf::from("C:/dl/file.zip"),
            file_size: Some(10_485_760),
            downloaded_bytes: 0,
            status: JobStatus::Queued,
            priority: 200,
            created_at: Utc::now(),
            completed_at: None,
            error: None,
            referrer: Some("https://example.com".into()),
            cookies: Some("session=abc".into()),
            user_agent: "UDM/0.1".into(),
            headers,
            segments: vec![Segment {
                id: 0,
                byte_start: 0,
                byte_end: 5_242_879,
                downloaded: 0,
                temp_file: PathBuf::from("C:/dl/file.zip.part.0"),
                status: JobStatus::Queued,
            }],
            checksum: Some(Checksum {
                algorithm: "sha256".into(),
                expected: "deadbeef".into(),
                verified: None,
            }),
            source_browser: "chrome".into(),
            tags: vec!["archive".into()],
        }
    }

    #[test]
    fn job_round_trips() {
        let conn = init_in_memory().unwrap();
        let job = sample_job();
        insert_job(&conn, &job).unwrap();

        let loaded = get_job(&conn, &job.id).unwrap().expect("job present");
        assert_eq!(loaded, job);

        let all = get_all_jobs(&conn).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn progress_update_is_persisted() {
        let conn = init_in_memory().unwrap();
        let job = sample_job();
        insert_job(&conn, &job).unwrap();

        update_job_progress(&conn, &job.id, 4096, JobStatus::Active).unwrap();
        let loaded = get_job(&conn, &job.id).unwrap().unwrap();
        assert_eq!(loaded.downloaded_bytes, 4096);
        assert_eq!(loaded.status, JobStatus::Active);
    }

    #[test]
    fn set_status_records_error_and_completion() {
        let conn = init_in_memory().unwrap();
        let job = sample_job();
        insert_job(&conn, &job).unwrap();

        let now = Utc::now();
        set_status(&conn, &job.id, JobStatus::Failed, Some("boom"), Some(now)).unwrap();
        let loaded = get_job(&conn, &job.id).unwrap().unwrap();
        assert_eq!(loaded.status, JobStatus::Failed);
        assert_eq!(loaded.error.as_deref(), Some("boom"));
        assert_eq!(loaded.completed_at, Some(now));
    }

    #[test]
    fn pending_excludes_terminal_states() {
        let conn = init_in_memory().unwrap();
        let mut active = sample_job();
        active.status = JobStatus::Active;
        let mut done = sample_job();
        done.status = JobStatus::Completed;
        insert_job(&conn, &active).unwrap();
        insert_job(&conn, &done).unwrap();

        let pending = get_pending_jobs(&conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, active.id);
    }

    #[test]
    fn delete_removes_job() {
        let conn = init_in_memory().unwrap();
        let job = sample_job();
        insert_job(&conn, &job).unwrap();
        assert!(delete_job(&conn, &job.id).unwrap());
        assert!(get_job(&conn, &job.id).unwrap().is_none());
    }

    #[test]
    fn settings_round_trip_and_default() {
        let conn = init_in_memory().unwrap();
        // Defaults when nothing stored.
        assert_eq!(load_settings(&conn).unwrap(), AppSettings::default());

        let s = AppSettings {
            max_concurrent_downloads: 7,
            max_bandwidth_kbps: Some(500),
            ..AppSettings::default()
        };
        save_settings(&conn, &s).unwrap();
        assert_eq!(load_settings(&conn).unwrap(), s);
    }

    #[test]
    fn db_file_is_created_and_reused() {
        let dir = std::env::temp_dir().join(format!("udm-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jobs.db");

        let job = sample_job();
        {
            let conn = init(&path).unwrap();
            insert_job(&conn, &job).unwrap();
        }
        assert!(path.exists(), "db file should exist after first run");
        {
            // Reopen the same file: data survives.
            let conn = init(&path).unwrap();
            assert!(get_job(&conn, &job.id).unwrap().is_some());
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
