//! Top-level download orchestration (Phase 3 + 4).
//!
//! `download` probes the URL and branches:
//!   - multi-segment (parallel ranged tasks) when the server supports
//!     `Accept-Ranges: bytes` and the size warrants more than one segment;
//!   - single-stream fallback otherwise.
//!
//! Both paths are cancellable (pause) and emit `EngineEvent`s.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use udm_storage::models::{DownloadJob, JobStatus, Segment};

use crate::chunker;
use crate::event::EngineEvent;
use crate::segment::download_segment;
use crate::throttle::TokenBucket;
use crate::{assembler, file_manager};

/// How often progress events are emitted.
const TICK: Duration = Duration::from_millis(500);

/// Download `job`, choosing multi-segment or single-stream automatically.
///
/// If `job.segments` is non-empty the job is treated as a resume and the probe
/// is skipped. Returns `Ok(())` on success or pause (events convey which); the
/// caller emits `Failed` on `Err`.
pub async fn download(
    job: DownloadJob,
    client: reqwest::Client,
    events: UnboundedSender<EngineEvent>,
    cancel: CancellationToken,
    max_segments: u8,
    limiter: Arc<TokenBucket>,
) -> Result<()> {
    // Resume: segments already planned (and partially downloaded).
    if !job.segments.is_empty() {
        let total = job
            .segments
            .iter()
            .map(|s| s.byte_end + 1)
            .max()
            .unwrap_or(0);
        return multi_segment(Arc::new(job), client, events, cancel, total, limiter).await;
    }

    // Fresh: probe for size + range support.
    let (total, ranges_ok) = probe(&client, &job).await;
    let seg_count = match total {
        Some(t) => chunker::calculate_segments(t, max_segments),
        None => 1,
    };

    if !ranges_ok || total.is_none() || seg_count <= 1 {
        tracing::debug!(
            "single-stream download (ranges_ok={ranges_ok}, total={total:?}, segs={seg_count})"
        );
        return single_stream(job, client, events, cancel, limiter).await;
    }

    // Plan segments and run multi-segment.
    let total = total.unwrap();
    let ranges = chunker::split_ranges(total, seg_count);
    let mut job = job;
    job.segments = ranges
        .iter()
        .enumerate()
        .map(|(i, (start, end))| Segment {
            id: i as u8,
            byte_start: *start,
            byte_end: *end,
            downloaded: 0,
            temp_file: file_manager::segment_part_path(&job.save_path, i as u8),
            status: JobStatus::Active,
        })
        .collect();
    multi_segment(Arc::new(job), client, events, cancel, total, limiter).await
}

/// HEAD probe -> (Content-Length, supports byte ranges).
async fn probe(client: &reqwest::Client, job: &DownloadJob) -> (Option<u64>, bool) {
    let mut req = client.head(&job.url).header("User-Agent", &job.user_agent);
    if let Some(referrer) = &job.referrer {
        req = req.header("Referer", referrer);
    }
    if let Some(cookies) = &job.cookies {
        req = req.header("Cookie", cookies);
    }
    match req.send().await.and_then(|r| r.error_for_status()) {
        Ok(resp) => {
            // NOTE: `content_length()` returns Some(0) for HEAD (empty body),
            // so read the Content-Length header directly.
            let total = resp.content_length().filter(|&n| n > 0).or_else(|| {
                resp.headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
            });
            let ranges_ok = resp
                .headers()
                .get(ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.eq_ignore_ascii_case("bytes"))
                .unwrap_or(false);
            (total, ranges_ok)
        }
        Err(e) => {
            tracing::debug!("HEAD probe failed ({e}); falling back to single stream");
            (None, false)
        }
    }
}

async fn multi_segment(
    job: Arc<DownloadJob>,
    client: reqwest::Client,
    events: UnboundedSender<EngineEvent>,
    cancel: CancellationToken,
    total: u64,
    limiter: Arc<TokenBucket>,
) -> Result<()> {
    let job_id = job.id;
    let segments = job.segments.clone();
    let initial: u64 = segments.iter().map(|s| s.downloaded).sum();

    let global = Arc::new(AtomicU64::new(initial));
    let seg_counters: Vec<Arc<AtomicU64>> = segments
        .iter()
        .map(|s| Arc::new(AtomicU64::new(s.downloaded)))
        .collect();

    let _ = events.send(EngineEvent::Started {
        job_id,
        total: Some(total),
        segments: segments.clone(),
    });

    let ticker = spawn_ticker(job_id, Some(total), Arc::clone(&global), events.clone());

    // One task per incomplete segment.
    let mut handles = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        if seg.byte_start + seg.downloaded > seg.byte_end {
            continue; // already complete
        }
        let job = Arc::clone(&job);
        let client = client.clone();
        let part = seg.temp_file.clone();
        let (start, end, already) = (seg.byte_start, seg.byte_end, seg.downloaded);
        let g = Arc::clone(&global);
        let sc = Arc::clone(&seg_counters[i]);
        let c = cancel.clone();
        let lim = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            download_segment(&job, &client, start, end, already, &part, g, sc, c, lim).await
        }));
    }

    let mut completed_all = true;
    let mut first_err: Option<anyhow::Error> = None;
    for h in handles {
        match h.await {
            Ok(Ok(true)) => {}
            Ok(Ok(false)) => completed_all = false, // paused
            Ok(Err(e)) => {
                completed_all = false;
                first_err.get_or_insert(e);
            }
            Err(e) => {
                completed_all = false;
                first_err.get_or_insert_with(|| anyhow::anyhow!("segment task panicked: {e}"));
            }
        }
    }
    ticker.abort();

    // A real error (not a cancellation) aborts the whole job.
    if let Some(e) = first_err {
        if !cancel.is_cancelled() {
            return Err(e);
        }
    }

    if cancel.is_cancelled() || !completed_all {
        let _ = events.send(EngineEvent::Paused {
            job_id,
            segments: snapshot_segments(&segments, &seg_counters),
        });
        return Ok(());
    }

    let parts: Vec<_> = segments.iter().map(|s| s.temp_file.clone()).collect();
    assembler::assemble_parts(&parts, &job.save_path)
        .await
        .context("assembling segments")?;

    let _ = events.send(EngineEvent::Completed {
        job_id,
        final_path: job.save_path.clone(),
        total: global.load(Ordering::Relaxed),
    });
    Ok(())
}

/// Single HTTP stream into `<save>.part`, then atomic move. Cancellable.
async fn single_stream(
    job: DownloadJob,
    client: reqwest::Client,
    events: UnboundedSender<EngineEvent>,
    cancel: CancellationToken,
    limiter: Arc<TokenBucket>,
) -> Result<()> {
    let job_id = job.id;

    let mut req = client.get(&job.url).header("User-Agent", &job.user_agent);
    if let Some(referrer) = &job.referrer {
        req = req.header("Referer", referrer);
    }
    if let Some(cookies) = &job.cookies {
        req = req.header("Cookie", cookies);
    }
    for (k, v) in &job.headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req
        .send()
        .await
        .with_context(|| format!("requesting {}", job.url))?
        .error_for_status()
        .context("server returned an error status")?;
    let total = resp.content_length();

    let _ = events.send(EngineEvent::Started {
        job_id,
        total,
        segments: Vec::new(),
    });

    let downloaded = Arc::new(AtomicU64::new(0));
    let ticker = spawn_ticker(job_id, total, Arc::clone(&downloaded), events.clone());

    let part = file_manager::part_path(&job.save_path);
    file_manager::ensure_parent_dir(&part).await?;
    let paused = stream_to_file(resp, &part, &downloaded, &cancel, &limiter).await;
    ticker.abort();
    let paused = paused?;

    if paused {
        // No reliable resume for plain streams; restart from scratch next time.
        let _ = events.send(EngineEvent::Paused {
            job_id,
            segments: Vec::new(),
        });
        return Ok(());
    }

    file_manager::finalize(&part, &job.save_path)
        .await
        .context("moving completed file into place")?;
    let _ = events.send(EngineEvent::Completed {
        job_id,
        final_path: job.save_path.clone(),
        total: downloaded.load(Ordering::Relaxed),
    });
    Ok(())
}

/// Returns `Ok(true)` if cancelled (paused), `Ok(false)` if the stream finished.
async fn stream_to_file(
    mut resp: reqwest::Response,
    part: &Path,
    downloaded: &AtomicU64,
    cancel: &CancellationToken,
    limiter: &TokenBucket,
) -> Result<bool> {
    let mut file = tokio::fs::File::create(part)
        .await
        .with_context(|| format!("creating {}", part.display()))?;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                file.flush().await?;
                return Ok(true);
            }
            chunk = resp.chunk() => {
                match chunk.context("reading response body")? {
                    Some(bytes) => {
                        limiter.consume(bytes.len() as u64).await;
                        file.write_all(&bytes).await.context("writing part file")?;
                        downloaded.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                    None => break,
                }
            }
        }
    }
    file.flush().await?;
    Ok(false)
}

/// Snapshot live per-segment counters back into `Segment` records.
fn snapshot_segments(segments: &[Segment], counters: &[Arc<AtomicU64>]) -> Vec<Segment> {
    segments
        .iter()
        .zip(counters)
        .map(|(s, c)| Segment {
            downloaded: c.load(Ordering::Relaxed),
            ..s.clone()
        })
        .collect()
}

/// Spawn a task that emits a `Progress` event every `TICK` until aborted.
fn spawn_ticker(
    job_id: uuid::Uuid,
    total: Option<u64>,
    downloaded: Arc<AtomicU64>,
    events: UnboundedSender<EngineEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        let mut last: u64 = 0;
        loop {
            interval.tick().await;
            let now = downloaded.load(Ordering::Relaxed);
            let speed_bps = now.saturating_sub(last) * (1000 / TICK.as_millis() as u64);
            last = now;
            let eta_secs = match total {
                Some(t) if speed_bps > 0 => t.saturating_sub(now) / speed_bps,
                _ => 0,
            };
            if events
                .send(EngineEvent::Progress {
                    job_id,
                    downloaded: now,
                    speed_bps,
                    eta_secs,
                })
                .is_err()
            {
                break;
            }
        }
    })
}
