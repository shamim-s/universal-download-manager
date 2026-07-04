// Clipboard watcher: when enabled in settings, poll the clipboard and offer to
// download any copied URL that looks like a file link (known extension).
// Mirrors daemon/crates/daemon/src/categorize.rs plus a few common binaries.

import { useEffect, useRef } from "react";
import { useStore } from "../store/downloads";
import { openNewDownloadPrompt } from "../newDownloadWindow";

const FILE_EXTENSIONS = new Set([
  // video
  "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "3gp",
  // music
  "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus",
  // images
  "jpg", "jpeg", "png", "gif", "bmp", "webp", "svg", "tiff", "ico", "heic",
  // documents
  "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "md", "epub", "odt", "rtf",
  // archives
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst",
  // programs / other binaries
  "exe", "msi", "apk", "deb", "rpm", "appimage", "iso", "jar", "dmg", "pkg", "bin",
]);

/** Is this clipboard text a single http(s) URL pointing at a file-ish path? */
export function looksLikeDownloadUrl(text: string): boolean {
  const t = text.trim();
  if (!t || /\s/.test(t) || t.length > 2048) return false;
  let u: URL;
  try {
    u = new URL(t);
  } catch {
    return false;
  }
  if (u.protocol !== "http:" && u.protocol !== "https:") return false;
  const last = u.pathname.split("/").filter(Boolean).pop() ?? "";
  const ext = last.includes(".") ? last.split(".").pop()!.toLowerCase() : "";
  return FILE_EXTENSIONS.has(ext);
}

const POLL_MS = 1500;

export function useClipboardWatcher() {
  const enabled = useStore().settings.clipboardWatcher;
  // Baseline whatever is on the clipboard when the watcher turns on, so we
  // only react to *new* copies; also dedupe repeat reads of the same text.
  const lastSeen = useRef<string | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let stop = false;
    let timer: number | undefined;
    let first = true;

    async function tick() {
      try {
        const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
        const text = (await readText()) ?? "";
        if (stop) return;
        if (first) {
          first = false;
          lastSeen.current = text;
        } else if (text && text !== lastSeen.current) {
          lastSeen.current = text;
          if (looksLikeDownloadUrl(text)) {
            void openNewDownloadPrompt({ url: text.trim(), sourceBrowser: "clipboard" });
          }
        }
      } catch {
        // Not running under Tauri (plain vite preview) or clipboard busy — skip.
      }
      if (!stop) timer = window.setTimeout(tick, POLL_MS);
    }

    void tick();
    return () => {
      stop = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [enabled]);
}
