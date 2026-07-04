//! Merge `.part.N` files into the final output (Phase 4).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;

use crate::file_manager;

/// Concatenate the given part files (in order) into `final_path`, then delete
/// the parts. Caller guarantees `parts` are ordered by segment id.
pub async fn assemble_parts(parts: &[PathBuf], final_path: &Path) -> Result<()> {
    file_manager::ensure_parent_dir(final_path).await?;
    let mut out = tokio::fs::File::create(final_path)
        .await
        .with_context(|| format!("creating {}", final_path.display()))?;

    for p in parts {
        let mut input = tokio::fs::File::open(p)
            .await
            .with_context(|| format!("opening part {}", p.display()))?;
        tokio::io::copy(&mut input, &mut out)
            .await
            .with_context(|| format!("appending {}", p.display()))?;
    }
    out.flush().await?;
    drop(out);

    for p in parts {
        tokio::fs::remove_file(p).await.ok();
    }
    Ok(())
}
