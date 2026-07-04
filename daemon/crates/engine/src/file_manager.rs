//! Part-file paths, temp dirs, and the final move (Phase 3).

use anyhow::Result;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Path of the in-progress single-stream download: `<save_path>.part`.
pub fn part_path(save_path: &Path) -> PathBuf {
    let mut s: OsString = save_path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Path of one segment's part file: `<save_path>.part.<id>`.
pub fn segment_part_path(save_path: &Path, id: u8) -> PathBuf {
    let mut s: OsString = save_path.as_os_str().to_owned();
    s.push(format!(".part.{id}"));
    PathBuf::from(s)
}

/// Ensure the parent directory of `path` exists.
pub async fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    Ok(())
}

/// First path in the sequence `name.ext`, `name (1).ext`, `name (2).ext`, …
/// for which `taken` returns false. Used to avoid silently overwriting an
/// existing file (or colliding with another queued job's target).
pub fn unique_path(path: &Path, taken: impl Fn(&Path) -> bool) -> PathBuf {
    if !taken(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for n in 1u32.. {
        let name = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(name);
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("u32 range exhausted finding a unique filename")
}

/// Atomically move the completed `.part` file to its final name,
/// replacing any existing file at the destination.
pub async fn finalize(part: &Path, final_path: &Path) -> Result<()> {
    if tokio::fs::try_exists(final_path).await.unwrap_or(false) {
        tokio::fs::remove_file(final_path).await.ok();
    }
    tokio::fs::rename(part, final_path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("C:/dl/file.zip")),
            PathBuf::from("C:/dl/file.zip.part")
        );
    }

    #[test]
    fn unique_path_bumps_until_free() {
        // Nothing taken: unchanged.
        let p = Path::new("C:/dl/file.zip");
        assert_eq!(unique_path(p, |_| false), PathBuf::from("C:/dl/file.zip"));
        // First two names taken: lands on " (2)".
        let taken = |c: &Path| {
            let s = c.to_string_lossy().replace('\\', "/");
            s == "C:/dl/file.zip" || s == "C:/dl/file (1).zip"
        };
        assert_eq!(unique_path(p, taken), PathBuf::from("C:/dl/file (2).zip"));
        // No extension.
        let q = Path::new("C:/dl/download");
        let taken_q = |c: &Path| c.to_string_lossy().replace('\\', "/") == "C:/dl/download";
        assert_eq!(unique_path(q, taken_q), PathBuf::from("C:/dl/download (1)"));
    }
}
