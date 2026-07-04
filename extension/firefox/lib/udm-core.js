// UDM extension core — pure logic, no browser APIs, so it can be unit-tested
// under Node. Keep this file identical across browser builds (chrome/firefox).

export const DAEMON_URL = "ws://127.0.0.1:60123";
export const RECONNECT_MS = 3000;

export const ClientMessageType = {
  ADD_DOWNLOAD: "ADD_DOWNLOAD",
  PROMPT_DOWNLOAD: "PROMPT_DOWNLOAD",
  PAUSE: "PAUSE",
  RESUME: "RESUME",
  CANCEL: "CANCEL",
  SET_PRIORITY: "SET_PRIORITY",
  GET_ALL_JOBS: "GET_ALL_JOBS",
  SET_BANDWIDTH: "SET_BANDWIDTH",
};

export const ServerMessageType = {
  JOB_ADDED: "JOB_ADDED",
  JOB_PROGRESS: "JOB_PROGRESS",
  JOB_COMPLETED: "JOB_COMPLETED",
  JOB_FAILED: "JOB_FAILED",
  JOB_PAUSED: "JOB_PAUSED",
  JOB_CANCELLED: "JOB_CANCELLED",
  ALL_JOBS: "ALL_JOBS",
};

// Schemes the daemon can fetch by URL. blob:/data:/file: cannot be re-fetched,
// so we leave those to the browser.
const FETCHABLE = /^(https?|ftp):/i;

/** Decide whether UDM should take over this browser download. */
export function shouldIntercept(item) {
  const url = item?.finalUrl || item?.url;
  if (!url) return false;
  return FETCHABLE.test(url);
}

/** Format chrome.cookies.getAll() output into a Cookie header value. */
export function formatCookies(cookies) {
  if (!Array.isArray(cookies) || cookies.length === 0) return undefined;
  return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
}

/** Basename of a download path (handles both / and \ separators). */
export function basename(path) {
  if (!path) return undefined;
  const parts = path.split(/[\\/]/);
  const last = parts[parts.length - 1];
  return last || undefined;
}

/** Build the download payload the daemon expects (shared by add + prompt). */
export function buildPayload(item, cookies, sourceBrowser, fileSize) {
  const url = item.finalUrl || item.url;
  const payload = { url, sourceBrowser };
  const name = basename(item.filename);
  if (name) payload.filename = name;
  if (item.referrer) payload.referrer = item.referrer;
  const cookieHeader = typeof cookies === "string" ? cookies : formatCookies(cookies);
  if (cookieHeader) payload.cookies = cookieHeader;
  if (typeof fileSize === "number" && fileSize > 0) payload.fileSize = fileSize;
  return payload;
}

/** Build the ADD_DOWNLOAD message (a confirmed download that queues at once). */
export function buildAddDownload(item, cookies, sourceBrowser) {
  return { type: ClientMessageType.ADD_DOWNLOAD, payload: buildPayload(item, cookies, sourceBrowser) };
}

/**
 * Build the PROMPT_DOWNLOAD message: hands an intercepted download to the app,
 * which opens the "New Download" window (or auto-starts if the prompt is off).
 */
export function buildPromptDownload(item, cookies, sourceBrowser, fileSize) {
  return {
    type: ClientMessageType.PROMPT_DOWNLOAD,
    payload: buildPayload(item, cookies, sourceBrowser, fileSize),
  };
}

/** Aggregate live status for the popup from a jobs map. */
export function summarize(jobs) {
  const values = Object.values(jobs || {});
  const active = values.filter((j) => j.status === "active");
  const totalSpeed = active.reduce((s, j) => s + (j.speedBps || 0), 0);
  return { activeCount: active.length, totalSpeed, totalJobs: values.length };
}
