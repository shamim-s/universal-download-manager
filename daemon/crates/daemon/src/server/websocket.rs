//! WebSocket server (Phase 2). Binds loopback only — see docs/ARCHITECTURE.md §8.
//!
//! Each connection runs a `select!` loop that simultaneously:
//!   - reads `ClientMessage`s from the socket and dispatches them, and
//!   - forwards broadcast `ServerMessage` events to the client.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use udm_storage::db;
use udm_storage::models::{DownloadJob, JobStatus};

use crate::protocol::{AddDownloadPayload, ClientMessage, ServerMessage};
use crate::queue::scheduler::SchedulerCmd;
use crate::state::AppState;

/// Accept connections forever on `addr` (e.g. "127.0.0.1:60123").
pub async fn run(state: Arc<AppState>, addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("WebSocket server listening on ws://{addr}");
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_conn(state, stream, peer).await {
                tracing::warn!("connection {peer} ended with error: {e}");
            }
        });
    }
}

// The auth callback's `Result<Response, ErrorResponse>` is dictated by the
// tungstenite handshake API; its `Err` variant is unavoidably large.
#[allow(clippy::result_large_err)]
async fn handle_conn(state: Arc<AppState>, stream: TcpStream, peer: SocketAddr) -> Result<()> {
    let ws = match &state.auth_token {
        Some(expected) => {
            let expected = expected.clone();
            let callback = move |req: &Request,
                                 resp: Response|
                  -> std::result::Result<Response, ErrorResponse> {
                let got = crate::security::auth::token_in_uri(&req.uri().to_string());
                if crate::security::auth::token_matches(&expected, got.as_deref()) {
                    Ok(resp)
                } else {
                    let mut err = ErrorResponse::new(Some("unauthorized".to_string()));
                    *err.status_mut() = StatusCode::UNAUTHORIZED;
                    Err(err)
                }
            };
            match tokio_tungstenite::accept_hdr_async(stream, callback).await {
                Ok(ws) => ws,
                Err(e) => {
                    tracing::warn!("rejected unauthorized client {peer}: {e}");
                    return Ok(());
                }
            }
        }
        None => tokio_tungstenite::accept_async(stream).await?,
    };
    tracing::info!("client connected: {peer}");
    let (mut write, mut read) = ws.split();
    let mut events = state.events.subscribe();

    loop {
        tokio::select! {
            // Inbound message from this client.
            incoming = read.next() => {
                match incoming {
                    Some(Ok(Message::Text(txt))) => {
                        if let Err(e) = handle_text(&state, &mut write, &txt).await {
                            tracing::warn!("handler error for {peer}: {e}");
                        }
                    }
                    Some(Ok(Message::Ping(p))) => write.send(Message::Pong(p)).await?,
                    Some(Ok(Message::Close(frame))) => {
                        // Complete the close handshake by echoing the frame back.
                        let _ = write.send(Message::Close(frame)).await;
                        break;
                    }
                    None => break,
                    Some(Ok(_)) => {} // ignore Binary/Pong/Frame
                    Some(Err(e)) => return Err(e.into()),
                }
            }
            // Outbound broadcast event -> this client.
            evt = events.recv() => {
                match evt {
                    Ok(msg) => write.send(Message::Text(serde_json::to_string(&msg)?)).await?,
                    Err(RecvError::Lagged(n)) => tracing::warn!("{peer} lagged, dropped {n} events"),
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }

    tracing::info!("client disconnected: {peer}");
    Ok(())
}

/// Parse and dispatch one text frame.
async fn handle_text<S>(state: &Arc<AppState>, write: &mut S, txt: &str) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let msg: ClientMessage = match serde_json::from_str(txt) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("ignoring malformed message: {e}");
            return Ok(());
        }
    };

    match msg {
        ClientMessage::GetAllJobs => {
            let jobs = {
                let conn = state.db.lock().unwrap();
                db::get_all_jobs(&conn)?
            };
            let values = jobs
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()?;
            let reply = ServerMessage::AllJobs { jobs: values };
            send(write, &reply).await?;
        }

        ClientMessage::AddDownload { payload } => {
            queue_payload(state, payload)?;
        }

        ClientMessage::PromptDownload { payload } => {
            // Reject anything that isn't a well-formed http(s) URL up front.
            if let Err(e) = crate::security::validate::validate_url(&payload.url) {
                tracing::warn!("rejecting PROMPT_DOWNLOAD: {e}");
                return Ok(());
            }
            let ask = {
                let conn = state.db.lock().unwrap();
                db::load_settings(&conn)
                    .map(|s| s.prompt_before_download)
                    .unwrap_or(true)
            };
            if ask {
                // Hand off to the UI, which opens the New Download window.
                state.broadcast(ServerMessage::DownloadPrompt {
                    payload: serde_json::to_value(&payload)?,
                });
            } else {
                // Prompt disabled: behave like the old auto-start flow.
                queue_payload(state, payload)?;
            }
        }

        ClientMessage::Pause { job_id } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                crate::bridge::pause(state, &id);
            }
        }

        ClientMessage::Resume { job_id } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                let job = {
                    let conn = state.db.lock().unwrap();
                    db::get_job(&conn, &id)?
                };
                match job {
                    Some(job) => {
                        tracing::info!("resuming job {id}");
                        let _ = state.sched_tx.send(SchedulerCmd::Enqueue(Box::new(job)));
                    }
                    None => tracing::warn!("resume: no job {id}"),
                }
            }
        }

        ClientMessage::Retry { job_id } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                let job = {
                    let conn = state.db.lock().unwrap();
                    match db::get_job(&conn, &id)? {
                        Some(mut job) => {
                            job.status = JobStatus::Queued;
                            job.error = None;
                            db::insert_job(&conn, &job)?;
                            Some(job)
                        }
                        None => None,
                    }
                };
                match job {
                    Some(job) => {
                        tracing::info!("retrying job {id}");
                        // Full row re-broadcast so clients move it back to Queued.
                        state.broadcast(ServerMessage::JobAdded {
                            job: serde_json::to_value(&job)?,
                        });
                        let _ = state.sched_tx.send(SchedulerCmd::Enqueue(Box::new(job)));
                    }
                    None => tracing::warn!("retry: no job {id}"),
                }
            }
        }

        ClientMessage::Cancel { job_id } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                // Drop it from the pending queue if it hasn't started.
                let _ = state.sched_tx.send(SchedulerCmd::RemovePending(id));
                crate::bridge::cancel(state, &id);
            }
        }

        ClientMessage::Remove { job_id } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                // Make sure it's out of the pending queue, then delete it.
                let _ = state.sched_tx.send(SchedulerCmd::RemovePending(id));
                crate::bridge::remove(state, &id);
            }
        }

        ClientMessage::SetPriority { job_id, priority } => {
            if let Ok(id) = Uuid::parse_str(&job_id) {
                let _ = state.sched_tx.send(SchedulerCmd::SetPriority(id, priority));
            }
        }

        ClientMessage::SetBandwidth { kbps } => {
            let bps = kbps.map(|k| k as u64 * 1024).unwrap_or(0); // None/0 = unlimited
            tracing::info!("bandwidth limit set to {bps} B/s (0 = unlimited)");
            state.limiter.set_rate(bps);
        }

        ClientMessage::GetSettings => {
            let settings = {
                let conn = state.db.lock().unwrap();
                db::load_settings(&conn).unwrap_or_default()
            };
            let reply = ServerMessage::SettingsUpdated {
                settings: serde_json::to_value(&settings)?,
            };
            send(write, &reply).await?;
        }

        ClientMessage::UpdateSettings { settings } => {
            tracing::info!(
                "settings updated: concurrency={}, bandwidth={:?} kbps, dir={}",
                settings.max_concurrent_downloads,
                settings.max_bandwidth_kbps,
                settings.download_directory.display()
            );
            {
                let conn = state.db.lock().unwrap();
                db::save_settings(&conn, &settings)?;
            }
            // Apply live: bandwidth limiter + scheduler concurrency.
            let bps = settings
                .max_bandwidth_kbps
                .map(|k| k as u64 * 1024)
                .unwrap_or(0);
            state.limiter.set_rate(bps);
            let _ = state.sched_tx.send(SchedulerCmd::SetMaxConcurrent(
                settings.max_concurrent_downloads,
            ));
            // Echo the persisted settings to every client so they stay in sync.
            state.broadcast(ServerMessage::SettingsUpdated {
                settings: serde_json::to_value(&settings)?,
            });
        }
    }
    Ok(())
}

async fn send<S>(write: &mut S, msg: &ServerMessage) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    write
        .send(Message::Text(serde_json::to_string(msg)?))
        .await
        .map_err(Into::into)
}

/// Validate, persist, announce, and enqueue a download from a payload. Shared by
/// `AddDownload` (UI-confirmed) and the auto-start branch of `PromptDownload`.
fn queue_payload(state: &Arc<AppState>, payload: AddDownloadPayload) -> Result<()> {
    // Reject anything that isn't a well-formed http(s) URL (untrusted input).
    if let Err(e) = crate::security::validate::validate_url(&payload.url) {
        tracing::warn!("rejecting download: {e}");
        return Ok(());
    }
    // Resolve a default save directory from settings when the client didn't pin
    // an explicit path.
    let settings = {
        let conn = state.db.lock().unwrap();
        db::load_settings(&conn).unwrap_or_default()
    };
    let mut job = build_job(
        payload,
        Some(settings.download_directory.as_path()),
        settings.auto_categorize,
    );
    // Never silently overwrite: bump to "name (1).ext" if the target exists on
    // disk or another live job is already downloading to it.
    {
        let conn = state.db.lock().unwrap();
        let taken: std::collections::HashSet<std::path::PathBuf> = db::get_all_jobs(&conn)?
            .into_iter()
            .filter(|j| {
                // Only in-flight jobs reserve a name; finished ones are covered
                // by the on-disk check (and shouldn't block re-downloads after
                // the file was deleted).
                matches!(
                    j.status,
                    JobStatus::Queued | JobStatus::Active | JobStatus::Paused
                )
            })
            .map(|j| j.save_path)
            .collect();
        job.save_path = udm_engine::file_manager::unique_path(&job.save_path, |p| {
            p.exists() || taken.contains(p)
        });
        job.filename = job
            .save_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(job.filename);
        db::insert_job(&conn, &job)?;
    }
    tracing::info!(
        "queued job {} ({}) -> {}",
        job.id,
        job.url,
        job.save_path.display()
    );
    // Announce to everyone (including this client), then hand to scheduler.
    state.broadcast(ServerMessage::JobAdded {
        job: serde_json::to_value(&job)?,
    });
    let _ = state.sched_tx.send(SchedulerCmd::Enqueue(Box::new(job)));
    Ok(())
}

/// Turn an inbound `ADD_DOWNLOAD` payload into a freshly-queued job.
///
/// `default_dir` is the configured download directory; it's used as the parent
/// for the inferred filename when the client doesn't pin an explicit save path.
/// With `auto_categorize` on, defaulted paths get a per-type subfolder
/// (Video, Music, …) between the directory and the filename.
fn build_job(
    payload: AddDownloadPayload,
    default_dir: Option<&std::path::Path>,
    auto_categorize: bool,
) -> DownloadJob {
    use crate::security::validate;
    use std::path::PathBuf;

    // Sanitize the (untrusted) filename down to a safe basename.
    let filename = validate::safe_filename(
        &payload
            .filename
            .clone()
            .unwrap_or_else(|| infer_filename(&payload.url)),
    );
    let configured_dir = default_dir.filter(|d| !d.as_os_str().is_empty());
    let default_join = |name: &str| {
        let category = auto_categorize
            .then(|| crate::categorize::category_for(name))
            .flatten();
        match (configured_dir, category) {
            (Some(dir), Some(cat)) => dir.join(cat).join(name),
            (Some(dir), None) => dir.join(name),
            (None, _) => PathBuf::from(name),
        }
    };
    let save_path = match payload.save_path.as_deref() {
        // Explicit path from the UI's folder picker. Honor any absolute folder
        // the user chose (a download manager isn't jailed to one directory), but
        // reject path traversal and re-sanitize the basename so a crafted value
        // can't write outside the chosen folder.
        Some(p) => {
            let candidate = PathBuf::from(p);
            let has_traversal = candidate
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
            if candidate.is_absolute() && !has_traversal {
                let name = candidate
                    .file_name()
                    .map(|n| validate::safe_filename(&n.to_string_lossy()))
                    .unwrap_or_else(|| filename.clone());
                match candidate.parent() {
                    Some(parent) => parent.join(name),
                    None => candidate,
                }
            } else {
                tracing::warn!("save path '{p}' is relative or escapes; using default dir");
                default_join(&filename)
            }
        }
        None => default_join(&filename),
    };

    // The on-disk name is the basename we actually resolved above.
    let filename = save_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(filename);

    DownloadJob {
        id: Uuid::new_v4(),
        url: payload.url,
        filename,
        save_path,
        file_size: payload.file_size,
        downloaded_bytes: 0,
        status: JobStatus::Queued,
        priority: payload.priority.unwrap_or(128),
        created_at: Utc::now(),
        completed_at: None,
        error: None,
        referrer: payload.referrer,
        cookies: payload.cookies,
        user_agent: "UDM/0.1".to_string(),
        headers: payload.headers.unwrap_or_default(),
        segments: Vec::new(),
        checksum: None,
        source_browser: payload.source_browser,
        tags: Vec::new(),
    }
}

/// Best-effort filename from the URL's last path segment.
fn infer_filename(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .find(|s| !s.is_empty())
        .filter(|s| s.contains('.'))
        .unwrap_or("download")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_filename_from_url() {
        assert_eq!(infer_filename("https://x.com/a/b/file.zip"), "file.zip");
        assert_eq!(infer_filename("https://x.com/file.iso?token=1"), "file.iso");
        assert_eq!(infer_filename("https://x.com/no-extension/"), "download");
    }

    #[test]
    fn build_job_defaults_to_queued() {
        let payload = AddDownloadPayload {
            url: "https://x.com/f.bin".into(),
            filename: None,
            save_path: None,
            cookies: None,
            referrer: None,
            headers: None,
            source_browser: "chrome".into(),
            priority: None,
            file_size: None,
        };
        let job = build_job(payload, None, false);
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.filename, "f.bin");
        assert_eq!(job.downloaded_bytes, 0);
    }

    #[test]
    fn build_job_joins_default_dir() {
        let payload = AddDownloadPayload {
            url: "https://x.com/f.bin".into(),
            filename: None,
            save_path: None,
            cookies: None,
            referrer: None,
            headers: None,
            source_browser: "chrome".into(),
            priority: None,
            file_size: None,
        };
        let dir = std::path::Path::new("C:/Downloads");
        let job = build_job(payload, Some(dir), false);
        assert_eq!(job.save_path, dir.join("f.bin"));
    }

    #[test]
    fn build_job_rejects_traversal_filename() {
        let payload = AddDownloadPayload {
            url: "https://x.com/a/b.bin".into(),
            filename: Some("../../etc/passwd".into()),
            save_path: None,
            cookies: None,
            referrer: None,
            headers: None,
            source_browser: "chrome".into(),
            priority: None,
            file_size: None,
        };
        let dir = std::path::Path::new("C:/Downloads");
        let job = build_job(payload, Some(dir), false);
        // The malicious filename is reduced to its basename, kept inside the dir.
        assert_eq!(job.filename, "passwd");
        assert_eq!(job.save_path, dir.join("passwd"));
    }

    #[test]
    fn build_job_rejects_escaping_save_path() {
        let payload = AddDownloadPayload {
            url: "https://x.com/f.bin".into(),
            filename: None,
            save_path: Some("C:/Downloads/../Windows/System32/evil.bin".into()),
            cookies: None,
            referrer: None,
            headers: None,
            source_browser: "chrome".into(),
            priority: None,
            file_size: None,
        };
        let dir = std::path::Path::new("C:/Downloads");
        let job = build_job(payload, Some(dir), false);
        // Escaping path is discarded in favor of the configured directory.
        assert_eq!(job.save_path, dir.join("f.bin"));
    }

    #[test]
    fn build_job_honors_absolute_picked_folder() {
        // The UI's folder picker returns an absolute path outside the default
        // download dir; a download manager must honor it (not jail to one dir).
        let payload = AddDownloadPayload {
            url: "https://x.com/f.bin".into(),
            filename: Some("movie.mp4".into()),
            save_path: Some("D:/Media/movie.mp4".into()),
            cookies: None,
            referrer: None,
            headers: None,
            source_browser: "chrome".into(),
            priority: None,
            file_size: Some(1234),
        };
        let dir = std::path::Path::new("C:/Downloads");
        let job = build_job(payload, Some(dir), false);
        assert_eq!(job.save_path, std::path::Path::new("D:/Media/movie.mp4"));
        assert_eq!(job.filename, "movie.mp4");
        assert_eq!(job.file_size, Some(1234));
    }
}
