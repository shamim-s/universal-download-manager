// The small "New Download" popup window (IDM-style). Two phases:
//   1. prompt      — filename, size, save folder (native Browse), Start/Cancel
//   2. downloading — live progress (%, speed, ETA, bytes, segments) + controls
//
// Rendered as a standalone window (see main.tsx routing). It claims its intent
// from the Rust `take_intent` command, opens its own daemon socket to send the
// confirmed ADD_DOWNLOAD, and tracks the resulting job from the shared store.

import { useEffect, useRef, useState } from "react";
import "../App.css";
import "./NewDownload.css";
import { AddDownloadPayload } from "../types";
import { useDaemonSocket, sendMessage } from "../hooks/useDaemonSocket";
import { getState, useStore } from "../store/downloads";
import { formatBytes, formatEta, formatSpeed, progressPct } from "../utils/format";

function basename(path?: string): string {
  if (!path) return "";
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || "";
}

function inferName(intent: AddDownloadPayload): string {
  if (intent.filename) return basename(intent.filename) || intent.filename;
  const clean = intent.url.split(/[?#]/)[0];
  return basename(clean) || "download";
}

function joinPath(dir: string, name: string): string {
  if (!dir) return name;
  const sep = dir.includes("/") && !dir.includes("\\") ? "/" : "\\";
  return `${dir.replace(/[\\/]+$/, "")}${sep}${name}`;
}

async function closeWindow() {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } catch {
    window.close();
  }
}

async function reveal(path: string) {
  try {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.revealItemInDir(path);
  } catch {
    /* not in Tauri */
  }
}

async function openFile(path: string) {
  try {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.openPath(path);
  } catch {
    /* not in Tauri */
  }
}

export default function NewDownload({ intentId }: { intentId: string }) {
  useDaemonSocket();
  const { jobs, settings, connected } = useStore();

  const [intent, setIntent] = useState<AddDownloadPayload | null>(null);
  const [missing, setMissing] = useState(false);
  const [name, setName] = useState("");
  const [folder, setFolder] = useState("");
  const folderEdited = useRef(false);

  const [phase, setPhase] = useState<"prompt" | "downloading">("prompt");
  const [jobId, setJobId] = useState<string | null>(null);
  const knownIds = useRef<Set<string>>(new Set());
  const pendingUrl = useRef<string | null>(null);

  // Claim the stashed intent once.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const got = await invoke<AddDownloadPayload | null>("take_intent", { id: intentId });
        if (cancelled) return;
        if (got) {
          setIntent(got);
          setName(inferName(got));
        } else {
          setMissing(true);
        }
      } catch {
        if (!cancelled) setMissing(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [intentId]);

  // Default the folder to the configured download dir until the user edits it.
  useEffect(() => {
    if (!folderEdited.current && settings.downloadDirectory) {
      setFolder(settings.downloadDirectory);
    }
  }, [settings.downloadDirectory]);

  // Capture the job created by our ADD_DOWNLOAD (the first new one with our URL).
  useEffect(() => {
    if (jobId || phase !== "downloading" || !pendingUrl.current) return;
    const match = jobs.find((j) => !knownIds.current.has(j.id) && j.url === pendingUrl.current);
    if (match) setJobId(match.id);
  }, [jobs, jobId, phase]);

  async function browse() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true, defaultPath: folder || undefined });
      if (typeof picked === "string") {
        folderEdited.current = true;
        setFolder(picked);
      }
    } catch {
      /* not in Tauri */
    }
  }

  function start() {
    if (!intent) return;
    const savePath = joinPath(folder, name.trim() || inferName(intent));
    knownIds.current = new Set(getState().jobs.map((j) => j.id));
    pendingUrl.current = intent.url;
    sendMessage({
      type: "ADD_DOWNLOAD",
      payload: { ...intent, filename: name.trim() || undefined, savePath },
    });
    setPhase("downloading");
  }

  if (missing) {
    return (
      <div className="nd">
        <p className="nd__missing">This download prompt has expired.</p>
        <div className="nd__actions">
          <button className="btn btn--primary" onClick={closeWindow}>
            Close
          </button>
        </div>
      </div>
    );
  }

  if (!intent) {
    return (
      <div className="nd">
        <p className="nd__missing">Loading…</p>
      </div>
    );
  }

  const job = jobId ? jobs.find((j) => j.id === jobId) : undefined;

  // --- Prompt phase ---------------------------------------------------------
  if (phase === "prompt") {
    return (
      <div className="nd">
        <h1 className="nd__title">New Download</h1>

        <label className="field">
          <span className="field__label">File name</span>
          <input
            className="field__input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            spellCheck={false}
          />
        </label>

        <div className="nd__row">
          <span className="nd__url" title={intent.url}>
            {intent.url}
          </span>
          <span className="nd__size">
            {intent.fileSize ? formatBytes(intent.fileSize) : "Size unknown"}
          </span>
        </div>

        <label className="field">
          <span className="field__label">Save to</span>
          <div className="nd__folder">
            <input
              className="field__input"
              value={folder}
              placeholder="Choose a folder…"
              onChange={(e) => {
                folderEdited.current = true;
                setFolder(e.target.value);
              }}
              spellCheck={false}
            />
            <button className="btn btn--ghost" onClick={browse}>
              Browse…
            </button>
          </div>
        </label>

        {!connected && <p className="nd__warn">Daemon offline — reconnecting…</p>}

        <div className="nd__actions">
          <button className="btn btn--ghost" onClick={closeWindow}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!name.trim() || !folder.trim()}
            onClick={start}
          >
            Start Download
          </button>
        </div>
      </div>
    );
  }

  // --- Downloading phase ----------------------------------------------------
  const size = job?.fileSize ?? intent.fileSize ?? null;
  const done = job?.downloadedBytes ?? 0;
  const pct = progressPct(done, size);
  const status = job?.status ?? "queued";
  const segCount = job?.segmentCount ?? 0;

  return (
    <div className="nd">
      <h1 className="nd__title nd__title--sm" title={intent.url}>
        {name}
      </h1>

      <div className={`progress ${pct === null && status === "active" ? "progress--indeterminate" : ""}`}>
        <div className="progress__fill" style={{ width: pct === null ? "100%" : `${pct}%` }} />
      </div>

      <div className="nd__stats">
        <span>
          {formatBytes(done)}
          {size ? ` / ${formatBytes(size)}` : ""}
          {pct !== null ? ` · ${pct.toFixed(1)}%` : ""}
        </span>
        {status === "active" && (
          <>
            <span>{formatSpeed(job?.speedBps)}</span>
            <span>ETA {formatEta(job?.eta)}</span>
            {segCount > 0 && <span>{segCount} segments</span>}
          </>
        )}
      </div>

      {status === "failed" && job?.error && <p className="nd__warn">{job.error}</p>}
      {status === "completed" && <p className="nd__ok">Saved to {job?.savePath}</p>}

      <div className="nd__actions">
        {status === "active" && jobId && (
          <button className="btn btn--ghost" onClick={() => sendMessage({ type: "PAUSE", jobId })}>
            Pause
          </button>
        )}
        {status === "paused" && jobId && (
          <button className="btn btn--ghost" onClick={() => sendMessage({ type: "RESUME", jobId })}>
            Resume
          </button>
        )}
        {(status === "active" || status === "queued" || status === "paused") && jobId && (
          <button
            className="btn btn--ghost"
            onClick={() => {
              sendMessage({ type: "CANCEL", jobId });
              closeWindow();
            }}
          >
            Cancel
          </button>
        )}
        {status === "completed" && (
          <>
            <button className="btn btn--ghost" onClick={() => job && reveal(job.savePath)}>
              Show in folder
            </button>
            <button className="btn btn--primary" onClick={() => job && openFile(job.savePath)}>
              Open
            </button>
          </>
        )}
        {(status === "completed" || status === "failed") && (
          <button className="btn btn--ghost" onClick={closeWindow}>
            Close
          </button>
        )}
      </div>
    </div>
  );
}
