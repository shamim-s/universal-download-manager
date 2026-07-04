// UDM Chrome service worker (Phase 6).
// Connects to the daemon over WebSocket, intercepts eligible browser downloads,
// and forwards them. Only intercepts while connected so downloads are never lost
// if the daemon is down.

import {
  DAEMON_URL,
  RECONNECT_MS,
  ClientMessageType,
  ServerMessageType,
  shouldIntercept,
  buildPromptDownload,
} from "../lib/udm-core.js";

let socket = null;
let connected = false;

function connect() {
  try {
    socket = new WebSocket(DAEMON_URL);
  } catch (e) {
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
  // The popup opens its own socket for live status; the worker just tracks
  // completions for notifications.
  if (msg.type === ServerMessageType.JOB_COMPLETED) {
    notify("Download complete", msg.finalPath || "");
  } else if (msg.type === ServerMessageType.JOB_FAILED) {
    notify("Download failed", msg.error || "");
  }
}

// --- Download interception ---------------------------------------------------

chrome.downloads.onCreated.addListener(async (item) => {
  if (!connected) return;            // let the browser handle it if we're offline
  if (!shouldIntercept(item)) return;

  // Take over: cancel the browser's download and remove the shelf entry.
  try {
    await chrome.downloads.cancel(item.id);
    await chrome.downloads.erase({ id: item.id });
  } catch (e) {
    // If we couldn't cancel (already finished, etc.), don't double-download.
    console.warn("[UDM] could not cancel download", e);
    return;
  }

  const url = item.finalUrl || item.url;
  let cookies;
  try {
    cookies = await chrome.cookies.getAll({ url });
  } catch {
    cookies = [];
  }

  // Hand off to the app, which shows the New Download prompt (path + Start).
  // totalBytes is often known at creation; pass it so the prompt can show size.
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize;
  send(buildPromptDownload(item, cookies, "chrome", size));
});

// --- Context menu: "Download with UDM" --------------------------------------

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "udm-download-link",
    title: "Download with UDM",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "udm-download-link" || !info.linkUrl) return;
  if (!connected) {
    notify("UDM offline", "Start the UDM app and try again.");
    return;
  }
  let cookies = [];
  try { cookies = await chrome.cookies.getAll({ url: info.linkUrl }); } catch {}
  send(buildPromptDownload({ url: info.linkUrl, referrer: info.pageUrl }, cookies, "chrome"));
});

// --- Helpers -----------------------------------------------------------------

function send(msg) {
  if (socket && socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(msg));
  }
}

function notify(title, message) {
  chrome.notifications.create({
    type: "basic",
    iconUrl: "../icons/icon48.png",
    title,
    message: String(message).slice(0, 200),
  });
}

function setBadge(text) {
  chrome.action.setBadgeText({ text });
  if (text) chrome.action.setBadgeBackgroundColor({ color: "#d33" });
}

void ClientMessageType; // referenced for parity with other clients
connect();
