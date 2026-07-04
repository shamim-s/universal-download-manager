// Add URL modal (Phase 8): paste one or more URLs (one per line), optionally
// override filename/priority, validate, and dispatch ADD_DOWNLOAD per URL.

import { useState } from "react";
import { sendMessage } from "../hooks/useDaemonSocket";
import { pushToast } from "../store/downloads";

function isValidUrl(raw: string): boolean {
  try {
    const u = new URL(raw);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

/** Non-empty lines of the textarea, trimmed. */
function parseUrls(raw: string): string[] {
  return raw
    .split(/\s+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export default function AddURLModal({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [filename, setFilename] = useState("");
  const [priority, setPriority] = useState(128);

  const urls = parseUrls(text);
  const invalid = urls.filter((u) => !isValidUrl(u));
  const valid = urls.length > 0 && invalid.length === 0;
  const batch = urls.length > 1;

  function submit() {
    if (!valid) {
      pushToast("error", invalid.length ? `Invalid URL: ${invalid[0]}` : "Enter a valid http(s) URL.");
      return;
    }
    for (const url of urls) {
      sendMessage({
        type: "ADD_DOWNLOAD",
        payload: {
          url,
          // A custom filename only makes sense for a single download.
          filename: batch ? undefined : filename.trim() || undefined,
          sourceBrowser: "udm-ui",
          priority,
        },
      });
    }
    pushToast("info", batch ? `${urls.length} downloads queued.` : "Download queued.");
    onClose();
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2 className="modal__title">Add download{batch ? `s (${urls.length})` : ""}</h2>

        <label className="field">
          <span className="field__label">URL{batch ? "s" : ""} — one per line for a batch</span>
          <textarea
            className="field__input field__input--multi"
            placeholder={"https://example.com/file.zip\nhttps://example.com/file2.zip"}
            value={text}
            autoFocus
            rows={3}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && valid) submit();
            }}
          />
        </label>

        {!batch && (
          <label className="field">
            <span className="field__label">Filename (optional)</span>
            <input
              className="field__input"
              placeholder="Inferred from URL"
              value={filename}
              onChange={(e) => setFilename(e.target.value)}
            />
          </label>
        )}

        <label className="field">
          <span className="field__label">
            Priority <strong>{priority}</strong>
          </span>
          <input
            type="range"
            min={0}
            max={255}
            value={priority}
            onChange={(e) => setPriority(Number(e.target.value))}
          />
        </label>

        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn--primary" disabled={!valid} onClick={submit}>
            {batch ? `Download ${urls.length} files` : "Download"}
          </button>
        </div>
      </div>
    </div>
  );
}
