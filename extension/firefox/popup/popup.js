// UDM popup (Phase 6): opens a short-lived socket to the daemon and shows live
// status. Decoupled from the service worker so it works even if the worker slept.

import { DAEMON_URL, ClientMessageType, ServerMessageType, summarize }
  from "../lib/udm-core.js";

const statusEl = document.getElementById("status");
const countEl = document.getElementById("active-count");
const speedEl = document.getElementById("speed");
const jobsEl = document.getElementById("jobs");

const jobs = {}; // id -> { filename, downloadedBytes, fileSize, speedBps, status }

function fmtSpeed(bps) {
  if (bps > 1024 * 1024) return (bps / 1024 / 1024).toFixed(1) + " MB/s";
  return Math.round(bps / 1024) + " KB/s";
}

function render() {
  const { activeCount, totalSpeed } = summarize(jobs);
  countEl.textContent = activeCount;
  speedEl.textContent = fmtSpeed(totalSpeed);
  jobsEl.innerHTML = "";
  for (const j of Object.values(jobs)) {
    if (j.status !== "active" && j.status !== "queued") continue;
    const li = document.createElement("li");
    const pct = j.fileSize ? Math.round((j.downloadedBytes / j.fileSize) * 100) : 0;
    li.textContent = `${j.filename || "download"} — ${j.status} ${pct ? pct + "%" : ""}`;
    jobsEl.appendChild(li);
  }
}

const ws = new WebSocket(DAEMON_URL);
ws.onopen = () => {
  statusEl.textContent = "Connected";
  statusEl.className = "status ok";
  ws.send(JSON.stringify({ type: ClientMessageType.GET_ALL_JOBS }));
};
ws.onclose = () => {
  statusEl.textContent = "UDM app not running";
  statusEl.className = "status err";
};
ws.onmessage = (ev) => {
  let msg;
  try { msg = JSON.parse(ev.data); } catch { return; }
  switch (msg.type) {
    case ServerMessageType.ALL_JOBS:
      for (const j of msg.jobs) jobs[j.id] = j;
      break;
    case ServerMessageType.JOB_ADDED:
      jobs[msg.job.id] = msg.job;
      break;
    case ServerMessageType.JOB_PROGRESS: {
      const j = jobs[msg.jobId] || (jobs[msg.jobId] = { id: msg.jobId });
      j.downloadedBytes = msg.downloadedBytes;
      j.speedBps = msg.speedBps;
      j.status = "active";
      break;
    }
    case ServerMessageType.JOB_COMPLETED:
      if (jobs[msg.jobId]) jobs[msg.jobId].status = "completed";
      break;
    case ServerMessageType.JOB_PAUSED:
      if (jobs[msg.jobId]) jobs[msg.jobId].status = "paused";
      break;
    case ServerMessageType.JOB_FAILED:
      if (jobs[msg.jobId]) jobs[msg.jobId].status = "failed";
      break;
  }
  render();
};
