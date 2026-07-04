//! Bridge between the download engine and the rest of the daemon (Phases 3–5).
//!
//! - `start_download` runs an engine download (called by the scheduler), tracked
//!   by a cancellation token; on finish it frees the scheduler slot.
//! - `pause` / `cancel` signal a running download.
//! - `consume_events` drains `EngineEvent`s, persists state, and broadcasts.

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use udm_engine::EngineEvent;
use udm_storage::db;
use udm_storage::models::{DownloadJob, JobStatus, Segment};

use crate::protocol::ServerMessage;
use crate::queue::scheduler::SchedulerCmd;
use crate::state::AppState;

/// Default maximum parallel segments per download.
const MAX_SEGMENTS: u8 = 8;

/// Backoff between automatic retries of transient failures.
const RETRY_BACKOFF: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
];

/// Run `job` now (the scheduler decides when this is called). Frees the slot
/// when the download ends, so the next queued job can start.
///
/// Transient failures (connection drops, timeouts, 5xx) are retried up to
/// `RETRY_BACKOFF.len()` times with backoff before the job is marked Failed.
pub fn start_download(state: Arc<AppState>, job: DownloadJob) {
    let job_id = job.id;
    let token = CancellationToken::new();
    state.active.lock().unwrap().insert(job_id, token.clone());

    let tx = state.engine_tx.clone();
    let client = state.http.clone();
    let limiter = Arc::clone(&state.limiter);
    let sched = state.sched_tx.clone();
    let st = Arc::clone(&state);
    tokio::spawn(async move {
        let mut job = job;
        let mut attempt = 0usize;
        loop {
            reconcile_segments_from_disk(&mut job);
            let res = udm_engine::download(
                job.clone(),
                client.clone(),
                tx.clone(),
                token.clone(),
                MAX_SEGMENTS,
                Arc::clone(&limiter),
            )
            .await;
            match res {
                Ok(()) => break,
                Err(e) => {
                    let transient = is_transient(&e);
                    if token.is_cancelled() || !transient || attempt >= RETRY_BACKOFF.len() {
                        tracing::warn!("download {job_id} failed: {e:#}");
                        let _ = tx.send(EngineEvent::Failed {
                            job_id,
                            error: format!("{e:#}"),
                        });
                        break;
                    }
                    let wait = RETRY_BACKOFF[attempt];
                    attempt += 1;
                    tracing::info!(
                        "download {job_id} hit a transient error (attempt {attempt}/{}), \
                         retrying in {wait:?}: {e:#}",
                        RETRY_BACKOFF.len()
                    );
                    tokio::select! {
                        _ = token.cancelled() => {
                            // Paused/cancelled while waiting: the Paused event
                            // path below settles the final status.
                            let _ = tx.send(EngineEvent::Paused { job_id, segments: job.segments.clone() });
                            break;
                        }
                        _ = tokio::time::sleep(wait) => {}
                    }
                    // Pick up the segment plan persisted at Started so the
                    // retry resumes instead of starting over.
                    if let Some(fresh) =
                        with_db_ret(&st, |conn| db::get_job(conn, &job_id)).flatten()
                    {
                        job = fresh;
                    }
                }
            }
        }
        st.active.lock().unwrap().remove(&job_id);
        let _ = sched.send(SchedulerCmd::SlotFreed(job_id));
    });
}

/// Trust the bytes actually on disk over possibly-stale DB counters: set each
/// segment's `downloaded` to its part file's length (capped to the segment
/// size, truncating any overshoot). Prevents corrupt resumes after a crash and
/// lets retries keep valid partial data.
fn reconcile_segments_from_disk(job: &mut DownloadJob) {
    for seg in &mut job.segments {
        let seg_len = seg.byte_end - seg.byte_start + 1;
        let on_disk = std::fs::metadata(&seg.temp_file)
            .map(|m| m.len())
            .unwrap_or(0);
        if on_disk > seg_len {
            // Shouldn't happen; trim so assembly can't include stray bytes.
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&seg.temp_file) {
                let _ = f.set_len(seg_len);
            }
        }
        seg.downloaded = on_disk.min(seg_len);
    }
    if !job.segments.is_empty() {
        job.downloaded_bytes = job.segments.iter().map(|s| s.downloaded).sum();
    }
}

/// Heuristic: is this failure worth retrying automatically? Network-level
/// errors and 5xx responses are; 4xx client errors are not.
fn is_transient(e: &anyhow::Error) -> bool {
    let msg = format!("{e:#}").to_ascii_lowercase();
    if msg.contains("client error") {
        return false;
    }
    msg.contains("server error")
        || msg.contains("error sending request")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("reading response body")
        || msg.contains("reading range body")
        || msg.contains("incomplete message")
}

/// Signal a running download to pause. No-op if it isn't active.
pub fn pause(state: &AppState, job_id: &Uuid) {
    if let Some(token) = state.active.lock().unwrap().get(job_id) {
        tracing::info!("pausing job {job_id}");
        token.cancel();
    }
}

/// Cancel a download: if running, stop it and treat the resulting stop as a
/// cancellation (not a pause). Pending-queue removal is handled by the caller.
pub fn cancel(state: &Arc<AppState>, job_id: &Uuid) {
    let was_running = {
        let active = state.active.lock().unwrap();
        active.contains_key(job_id)
    };
    if was_running {
        state.cancelling.lock().unwrap().insert(*job_id);
        if let Some(token) = state.active.lock().unwrap().get(job_id) {
            token.cancel();
        }
    } else {
        // Not running: mark Cancelled directly and clean any leftover parts.
        finish_cancel(state, job_id);
    }
}

/// Remove a job entirely: stop it if running, delete its DB row and any part
/// files, then broadcast `JobRemoved` so every client drops the row.
pub fn remove(state: &Arc<AppState>, job_id: &Uuid) {
    // Stop a running download first (treat as a cancellation, no pause-resume).
    let was_running = state.active.lock().unwrap().contains_key(job_id);
    if was_running {
        state.cancelling.lock().unwrap().insert(*job_id);
        if let Some(token) = state.active.lock().unwrap().get(job_id) {
            token.cancel();
        }
    }
    let parts = with_db_ret(state, |conn| {
        let job = db::get_job(conn, job_id)?;
        db::delete_job(conn, job_id)?;
        Ok(job.map(part_files))
    })
    .flatten()
    .unwrap_or_default();
    for p in parts {
        let _ = std::fs::remove_file(p);
    }
    state.broadcast(ServerMessage::JobRemoved {
        job_id: job_id.to_string(),
    });
}

/// Long-running consumer: maps engine events to DB writes + client broadcasts.
pub async fn consume_events(state: Arc<AppState>, mut rx: mpsc::UnboundedReceiver<EngineEvent>) {
    while let Some(evt) = rx.recv().await {
        match evt {
            EngineEvent::Started {
                job_id,
                total,
                segments,
            } => {
                tracing::info!(
                    "download {job_id} started (total: {total:?}, segments: {})",
                    segments.len()
                );
                persist_job_start(&state, &job_id, total, &segments);
                // JobAdded reached the UI before the size/segments were known;
                // announce them now so progress bars render accurately.
                state.broadcast(ServerMessage::JobStarted {
                    job_id: job_id.to_string(),
                    file_size: total,
                    segment_count: segments.len() as u32,
                });
            }
            EngineEvent::Progress {
                job_id,
                downloaded,
                speed_bps,
                eta_secs,
            } => {
                with_db(&state, |conn| {
                    db::update_job_progress(conn, &job_id, downloaded, JobStatus::Active)
                });
                state.broadcast(ServerMessage::JobProgress {
                    job_id: job_id.to_string(),
                    downloaded_bytes: downloaded,
                    speed_bps,
                    eta: eta_secs,
                });
            }
            EngineEvent::Paused { job_id, segments } => {
                // Was this stop actually a cancellation?
                if state.cancelling.lock().unwrap().remove(&job_id) {
                    tracing::info!("download {job_id} cancelled");
                    finish_cancel(&state, &job_id);
                } else {
                    tracing::info!("download {job_id} paused");
                    persist_job_pause(&state, &job_id, &segments);
                    state.broadcast(ServerMessage::JobPaused {
                        job_id: job_id.to_string(),
                    });
                }
            }
            EngineEvent::Completed {
                job_id,
                final_path,
                total,
            } => {
                tracing::info!("download {job_id} completed -> {}", final_path.display());
                with_db(&state, |conn| {
                    db::update_job_progress(conn, &job_id, total, JobStatus::Completed)?;
                    db::set_status(conn, &job_id, JobStatus::Completed, None, Some(Utc::now()))
                });
                state.broadcast(ServerMessage::JobCompleted {
                    job_id: job_id.to_string(),
                    final_path: final_path.to_string_lossy().to_string(),
                    total_bytes: total,
                });
            }
            EngineEvent::Failed { job_id, error } => {
                with_db(&state, |conn| {
                    db::set_status(conn, &job_id, JobStatus::Failed, Some(&error), None)
                });
                state.broadcast(ServerMessage::JobFailed {
                    job_id: job_id.to_string(),
                    error,
                });
            }
        }
    }
    tracing::info!("engine event channel closed");
}

/// Mark a job Cancelled, delete any leftover part files, and broadcast.
fn finish_cancel(state: &AppState, job_id: &Uuid) {
    let parts = with_db_ret(state, |conn| {
        let job = db::get_job(conn, job_id)?;
        if job.is_some() {
            db::set_status(conn, job_id, JobStatus::Cancelled, None, None)?;
        }
        Ok(job.map(part_files))
    })
    .flatten()
    .unwrap_or_default();
    for p in parts {
        let _ = std::fs::remove_file(p);
    }
    state.broadcast(ServerMessage::JobCancelled {
        job_id: job_id.to_string(),
    });
}

/// All on-disk part files for a job (segment parts + single-stream part).
fn part_files(job: DownloadJob) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = job.segments.iter().map(|s| s.temp_file.clone()).collect();
    v.push(udm_engine::file_manager::part_path(&job.save_path));
    v
}

/// Persist the segment plan + file size and mark the job Active.
fn persist_job_start(state: &AppState, job_id: &Uuid, total: Option<u64>, segments: &[Segment]) {
    with_db(state, |conn| {
        if let Some(mut job) = db::get_job(conn, job_id)? {
            job.file_size = total.or(job.file_size);
            job.segments = segments.to_vec();
            job.status = JobStatus::Active;
            db::insert_job(conn, &job)?;
        }
        Ok(())
    });
}

/// Persist per-segment progress and mark the job Paused.
fn persist_job_pause(state: &AppState, job_id: &Uuid, segments: &[Segment]) {
    with_db(state, |conn| {
        if let Some(mut job) = db::get_job(conn, job_id)? {
            job.downloaded_bytes = segments.iter().map(|s| s.downloaded).sum();
            job.segments = segments.to_vec();
            job.status = JobStatus::Paused;
            db::insert_job(conn, &job)?;
        }
        Ok(())
    });
}

/// Run a DB op under the lock, logging errors. Closure must not `.await`.
fn with_db<F>(state: &AppState, f: F)
where
    F: FnOnce(&rusqlite::Connection) -> anyhow::Result<()>,
{
    let _ = with_db_ret(state, |c| f(c).map(|_| ()));
}

fn with_db_ret<F, T>(state: &AppState, f: F) -> Option<T>
where
    F: FnOnce(&rusqlite::Connection) -> anyhow::Result<T>,
{
    match state.db.lock() {
        Ok(conn) => match f(&conn) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("db update failed: {e:#}");
                None
            }
        },
        Err(e) => {
            tracing::error!("db mutex poisoned: {e}");
            None
        }
    }
}
