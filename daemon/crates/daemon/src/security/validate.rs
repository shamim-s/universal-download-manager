//! Input validation & normalization (Phase 9 security, ARCHITECTURE §11).
//!
//! All of these treat their input as untrusted (a browser extension forwards
//! values influenced by web content). The goals:
//!   - only ever fetch `http`/`https` URLs;
//!   - never let a crafted filename escape the download directory via path
//!     traversal (`../`, absolute paths, embedded separators).

use anyhow::{anyhow, bail, Result};
use std::path::{Component, Path, PathBuf};

/// Validate a download URL: it must parse and use http/https with a real host.
pub fn validate_url(raw: &str) -> Result<()> {
    let url = url::Url::parse(raw).map_err(|e| anyhow!("invalid URL: {e}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => bail!("unsupported URL scheme '{other}' (only http/https allowed)"),
    }
    if url.host_str().is_none_or(|h| h.is_empty()) {
        bail!("URL has no host");
    }
    Ok(())
}

/// Reduce a client-supplied filename to a safe basename: strips any directory
/// components and parent refs, drops control characters, and never returns an
/// empty / `.` / `..` name.
pub fn safe_filename(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches(|c: char| c.is_control());
    match base {
        "" | "." | ".." => "download".to_string(),
        other => other.to_string(),
    }
}

/// Lexically normalize a path, resolving `.`/`..` without touching the FS.
/// (Filesystem `canonicalize` requires the path to exist; we need to validate
/// *before* creating anything.)
// Retained (with tests) as a reusable containment helper; `build_job` now
// rejects traversal directly rather than jailing to the configured dir.
#[allow(dead_code)]
pub fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Is `path` contained within `base` after lexical normalization? Used to make
/// sure an explicit save path can't escape the configured download directory.
#[allow(dead_code)]
pub fn is_within(base: &Path, path: &Path) -> bool {
    let nb = lexical_normalize(base);
    let np = lexical_normalize(path);
    np.starts_with(&nb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https() {
        assert!(validate_url("https://example.com/file.zip").is_ok());
        assert!(validate_url("http://example.com/a/b.iso?x=1").is_ok());
    }

    #[test]
    fn rejects_bad_schemes_and_hosts() {
        assert!(validate_url("ftp://example.com/x").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("not a url").is_err());
    }

    #[test]
    fn filename_strips_traversal() {
        assert_eq!(safe_filename("../../etc/passwd"), "passwd");
        assert_eq!(safe_filename("..\\..\\windows\\system32\\x.dll"), "x.dll");
        assert_eq!(safe_filename("a/b/c/file.zip"), "file.zip");
        assert_eq!(safe_filename("clean.bin"), "clean.bin");
    }

    #[test]
    fn filename_rejects_degenerate() {
        assert_eq!(safe_filename(""), "download");
        assert_eq!(safe_filename("."), "download");
        assert_eq!(safe_filename(".."), "download");
        assert_eq!(safe_filename("   "), "download");
    }

    #[test]
    fn containment_detects_escape() {
        let base = Path::new("/downloads");
        assert!(is_within(base, Path::new("/downloads/sub/file.zip")));
        assert!(is_within(base, Path::new("/downloads/./a/../b.zip")));
        assert!(!is_within(base, Path::new("/downloads/../etc/passwd")));
        assert!(!is_within(base, Path::new("/elsewhere/file.zip")));
    }
}
