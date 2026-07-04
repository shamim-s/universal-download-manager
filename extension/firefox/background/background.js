// UDM Firefox background (Phase 7) — MV2 persistent background page.
// Logic mirrors the Chrome service worker but uses the promise-based `browser.*`
// namespace and reuses the same lib/udm-core.js. `api` falls back to `chrome`
// so the file is portable.

import {
  DAEMON_URL,
  RECONNECT_MS,
  ServerMessageType,
  shouldIntercept,
  buildPromptDownload,
} from "../lib/udm-core.js";

const api = globalThis.browser ?? globalThis.chrome;

let socket = null;
let connected = false;

function connect() {
  try {
    socket = new WebSocket(DAEMON_URL);
  } catch {
    scheduleReconnect();
    return;
  }
  socket.onopen = () => {
    connected = true;
    console.log("[UDM] connected to daemon");
    setBadge("");
  };
  socket.onmessage = (ev) => handleServerMessage(ev.data);
  socket.onclose = () => {
    connected = false;
    setBadge("!");
    scheduleReconnect();
  };
  socket.onerror = () => {
    try { socket.close(); } catch {}
  };
}

function scheduleReconnect() {
  setTimeout(connect, RECONNECT_MS);
}

function handleServerMessage(data) {
  let msg;
  try { msg = JSON.parse(data); } catch { return; }
  if (msg.type === ServerMessageType.JOB_COMPLETED) {
    notify("Download complete", msg.finalPath || "");
  } else if (msg.type === ServerMessageType.JOB_FAILED) {
    notify("Download failed", msg.error || "");
  }
}

api.downloads.onCreated.addListener(async (item) => {
  if (!connected) return;
  if (!shouldIntercept(item)) return;
  try {
    await api.downloads.cancel(item.id);
    await api.downloads.erase({ id: item.id });
  } catch (e) {
    console.warn("[UDM] could not cancel download", e);
    return;
  }
  const url = item.finalUrl || item.url;
  let cookies = [];
  try { cookies = await api.cookies.getAll({ url }); } catch {}
  // Hand off to the app's New Download prompt (path + Start).
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize;
  send(buildPromptDownload(item, cookies, "firefox", size));
});

api.runtime.onInstalled.addListener(() => {
  api.contextMenus.create({
    id: "udm-download-link",
    title: "Download with UDM",
    contexts: ["link"],
  });
});

api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "udm-download-link" || !info.linkUrl) return;
  if (!connected) {
    notify("UDM offline", "Start the UDM app and try again.");
    return;
  }
  let cookies = [];
  try { cookies = await api.cookies.getAll({ url: info.linkUrl }); } catch {}
  send(buildPromptDownload({ url: info.linkUrl, referrer: info.pageUrl }, cookies, "firefox"));
});

function send(msg) {
  if (socket && socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(msg));
  }
}

function notify(title, message) {
  api.notifications.create({
    type: "basic",
    iconUrl: api.runtime.getURL("icons/icon48.png"),
    title,
    message: String(message).slice(0, 200),
  });
}

function setBadge(text) {
  // MV2 uses browserAction; guard in case it's unavailable.
  const action = api.browserAction ?? api.action;
  if (!action) return;
  action.setBadgeText({ text });
  if (text && action.setBadgeBackgroundColor) {
    action.setBadgeBackgroundColor({ color: "#d33" });
  }
}

connect();
