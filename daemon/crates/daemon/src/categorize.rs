//! File-type → subfolder mapping for the auto-categorize setting.
//!
//! When `AppSettings::auto_categorize` is on and the client didn't pick an
//! explicit save location, downloads are sorted into a per-type subfolder of
//! the download directory (IDM-style). Unknown types stay in the root.

/// Subfolder name for `filename`, based on its extension. `None` = keep the
/// file in the download directory root.
pub fn category_for(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit_once('.')?.1.to_ascii_lowercase();
    let cat = match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "ts"
        | "3gp" => "Video",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "Music",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" | "heic" => {
            "Images"
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "md"
        | "epub" | "odt" | "rtf" => "Documents",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" => "Archives",
        "exe" | "msi" | "apk" | "deb" | "rpm" | "appimage" | "iso" | "jar" => "Programs",
        _ => return None,
    };
    Some(cat)
}

#[cfg(test)]
mod tests {
    use super::category_for;

    #[test]
    fn maps_known_extensions() {
        assert_eq!(category_for("movie.MKV"), Some("Video"));
        assert_eq!(category_for("song.mp3"), Some("Music"));
        assert_eq!(category_for("report.pdf"), Some("Documents"));
        assert_eq!(category_for("bundle.tar.gz"), Some("Archives"));
        assert_eq!(category_for("setup.exe"), Some("Programs"));
    }

    #[test]
    fn unknown_or_missing_extension_stays_in_root() {
        assert_eq!(category_for("file.xyz"), None);
        assert_eq!(category_for("download"), None);
    }
}
