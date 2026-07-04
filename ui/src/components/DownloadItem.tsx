// Single download row (Phase 8): filename, segmented progress bar, size/speed/
// ETA, and contextual controls that send ClientMessages over the socket.

import { DownloadJob } from "../types";
import { sendMessage } from "../hooks/useDaemonSocket";
import { pushToast } from "../store/downloads";
import { formatBytes, formatEta, formatSpeed, progressPct } from "../utils/format";

const STATUS_LABEL: Record<DownloadJob["status"], string> = {
  queued: "Queued",
  active: "Downloading",
  paused: "Paused",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

async function reveal(path: string) {
  try {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.revealItemInDir(path);
  } catch {
    pushToast("error", "Couldn't open the file location.");
  }
}

async function openFile(path: string) {
  try {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.openPath(path);
  } catch {
    pushToast("error", "Couldn't open the file.");
  }
}

export default function DownloadItem({ job }: { job: DownloadJob }) {
  const pct = progressPct(job.downloadedBytes, job.fileSize);
  const indeterminate = job.status === "active" && pct === null;

  return (
    <div className={`dl-item dl-item--${job.status}`}>
      <div className="dl-item__main">
        <div className="dl-item__top">
          <span className="dl-item__name" title={job.url}>
            {job.filename}
          </span>
          <span className={`badge badge--${job.status}`}>{STATUS_LABEL[job.status]}</span>
        </div>

        <div className={`progress ${indeterminate ? "progress--indeterminate" : ""}`}>
          {renderBar(job, pct)}
        </div>

        <div className="dl-item__meta">
          <span>
            {formatBytes(job.downloadedBytes)}
            {job.fileSize ? ` / ${formatBytes(job.fileSize)}` : ""}
            {pct !== null ? ` · ${pct.toFixed(1)}%` : ""}
          </span>
          {job.status === "active" && (
            <>
              <span className="dl-item__speed">{formatSpeed(job.speedBps)}</span>
              <span>ETA {formatEta(job.eta)}</span>
            </>
          )}
          {job.status === "failed" && job.error && (
            <span className="dl-item__error" title={job.error}>
              {job.error}
            </span>
          )}
        </div>
      </div>

      <div className="dl-item__actions">{renderActions(job, () => reveal(job.savePath), () => openFile(job.savePath))}</div>
    </div>
  );
}

/** Segment-aware progress bar; falls back to a single fill when no segments. */
function renderBar(job: DownloadJob, pct: number | null) {
  const segs = job.segments;
  if (segs && segs.length > 1 && job.fileSize) {
    const total = job.fileSize;
    return (
      <div className="progress__segments">
        {segs.map((s) => {
          const span = s.byteEnd - s.byteStart + 1;
          const filled = span > 0 ? Math.min(100, (s.downloaded / span) * 100) : 0;
          const widthPct = (span / total) * 100;
          return (
            <div className="progress__seg" key={s.id} style={{ width: `${widthPct}%` }}>
              <div className="progress__seg-fill" style={{ width: `${filled}%` }} />
            </div>
          );
        })}
      </div>
    );
  }
  return <div className="progress__fill" style={{ width: pct === null ? "100%" : `${pct}%` }} />;
}

function renderActions(job: DownloadJob, onReveal: () => void, onOpen: () => void) {
  const send = sendMessage;
  const btns: React.ReactNode[] = [];

  if (job.status === "active" || job.status === "queued") {
    btns.push(
      <button key="pause" className="icon-btn" title="Pause" onClick={() => send({ type: "PAUSE", jobId: job.id })}>
        ❚❚
      </button>,
    );
  }
  if (job.status === "paused") {
    btns.push(
      <button key="resume" className="icon-btn" title="Resume" onClick={() => send({ type: "RESUME", jobId: job.id })}>
        ▶
      </button>,
    );
  }
  if (job.status === "failed" || job.status === "cancelled") {
    btns.push(
      <button key="retry" className="icon-btn" title="Retry" onClick={() => send({ type: "RETRY", jobId: job.id })}>
        ↻
      </button>,
    );
  }
  if (job.status === "completed") {
    btns.push(
      <button key="open" className="icon-btn" title="Open file" onClick={onOpen}>
        ↗
      </button>,
      <button key="folder" className="icon-btn" title="Show in folder" onClick={onReveal}>
        ⌖
      </button>,
    );
  }
  if (job.status === "active" || job.status === "queued" || job.status === "paused") {
    btns.push(
      <button key="cancel" className="icon-btn" title="Cancel" onClick={() => send({ type: "CANCEL", jobId: job.id })}>
        ✕
      </button>,
    );
  }
  btns.push(
    <button
      key="remove"
      className="icon-btn icon-btn--danger"
      title="Remove from list"
      onClick={() => send({ type: "REMOVE", jobId: job.id })}
    >
      🗑
    </button>,
  );
  return btns;
}
