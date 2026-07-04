//! Per-segment ranged download (Phase 4).

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use udm_storage::models::DownloadJob;

use crate::throttle::TokenBucket;

/// Download the byte range `[start+already, end]` into `part_path`.
///
/// Returns `Ok(true)` when the range completed, `Ok(false)` when cancelled
/// (paused). Both `global` and `seg` counters are incremented as bytes arrive.
#[allow(clippy::too_many_arguments)]
pub async fn download_segment(
    job: &DownloadJob,
    client: &reqwest::Client,
    start: u64,
    end: u64,
    already: u64,
    part_path: &Path,
    global: Arc<AtomicU64>,
    seg: Arc<AtomicU64>,
    cancel: CancellationToken,
    limiter: Arc<TokenBucket>,
) -> Result<bool> {
    let from = start + already;
    if from > end {
        return Ok(true); // segment already complete
    }

    let mut req = client
        .get(&job.url)
        .header("User-Agent", &job.user_agent)
        .header("Range", format!("bytes={from}-{end}"));
    if let Some(referrer) = &job.referrer {
        req = req.header("Referer", referrer);
    }
    if let Some(cookies) = &job.cookies {
        req = req.header("Cookie", cookies);
    }
    for (k, v) in &job.headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let mut resp = req
        .send()
        .await
        .with_context(|| format!("range request {from}-{end} for {}", job.url))?
        .error_for_status()
        .context("range request returned an error status")?;

    // Append when resuming (part file already holds `already` bytes), else create.
    let mut file = if already > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .await
            .with_context(|| format!("opening {} for append", part_path.display()))?
    } else {
        tokio::fs::File::create(part_path)
            .await
            .with_context(|| format!("creating {}", part_path.display()))?
    };

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                file.flush().await?;
                return Ok(false);
            }
            chunk = resp.chunk() => {
                match chunk.context("reading range body")? {
                    Some(bytes) => {
                        let n = bytes.len() as u64;
                        limiter.consume(n).await;
                        file.write_all(&bytes).await.context("writing part file")?;
                        global.fetch_add(n, Ordering::Relaxed);
                        seg.fetch_add(n, Ordering::Relaxed);
                    }
                    None => break,
                }
            }
        }
    }
    file.flush().await?;
    Ok(true)
}
