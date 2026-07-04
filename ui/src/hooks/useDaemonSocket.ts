// WebSocket connection to the daemon (Phase 8).
//
// A single module-level socket is shared across the app. `sendMessage` is
// importable anywhere (no prop drilling); `useDaemonSocket` boots the
// connection once and keeps the store's `connected` flag in sync. On open we
// hydrate via GET_ALL_JOBS + GET_SETTINGS; on close we reconnect after 2s.

import { useEffect } from "react";
import { AddDownloadPayload, ClientMessage, ServerMessage } from "../types";
import {
  applyServerMessage,
  pushToast,
  sampleSpeed,
  setConnected,
  getState,
} from "../store/downloads";

const DAEMON_URL = "ws://127.0.0.1:60123";
const RECONNECT_MS = 2000;

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let speedTimer: ReturnType<typeof setInterval> | null = null;
let started = false;
/** Messages enqueued while the socket is down, flushed on (re)connect. */
const outbox: ClientMessage[] = [];

function flushOutbox() {
  if (!socket || socket.readyState !== WebSocket.OPEN) return;
  while (outbox.length) {
    socket.send(JSON.stringify(outbox.shift()));
  }
}

/** Subscribers notified when the daemon asks the UI to prompt for a download. */
type PromptHandler = (payload: AddDownloadPayload) => void;
const promptHandlers = new Set<PromptHandler>();

/** Register a DOWNLOAD_PROMPT handler (the main window opens a popup). */
export function onDownloadPrompt(cb: PromptHandler): () => void {
  promptHandlers.add(cb);
  return () => {
    promptHandlers.delete(cb);
  };
}

/** Send a message to the daemon, queueing it if the socket isn't open yet. */
export function sendMessage(msg: ClientMessage) {
  if (socket && socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(msg));
  } else {
    outbox.push(msg);
  }
}

function connect() {
  try {
    socket = new WebSocket(DAEMON_URL);
  } catch {
    scheduleReconnect();
    return;
  }

  socket.onopen = () => {
    setConnected(true);
    sendMessage({ type: "GET_ALL_JOBS" });
    sendMessage({ type: "GET_SETTINGS" });
    flushOutbox();
  };

  socket.onmessage = (ev) => {
    let msg: ServerMessage;
    try {
      msg = JSON.parse(ev.data as string) as ServerMessage;
    } catch {
      return;
    }
    applyServerMessage(msg);
    notify(msg);
    if (msg.type === "DOWNLOAD_PROMPT") {
      for (const h of promptHandlers) h(msg.payload);
    }
  };

  socket.onclose = () => {
    setConnected(false);
    scheduleReconnect();
  };

  socket.onerror = () => {
    socket?.close();
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, RECONNECT_MS);
}

/** Surface noteworthy events as toasts (the store already updated the rows). */
function notify(msg: ServerMessage) {
  switch (msg.type) {
    case "JOB_COMPLETED": {
      const job = getState().jobs.find((j) => j.id === msg.jobId);
      pushToast("success", `Completed: ${job?.filename ?? "download"}`);
      break;
    }
    case "JOB_FAILED": {
      const job = getState().jobs.find((j) => j.id === msg.jobId);
      pushToast("error", `Failed: ${job?.filename ?? "download"} — ${msg.error}`);
      break;
    }
  }
}

/**
 * Boot the shared socket once for the app's lifetime, plus a 1s timer that
 * samples aggregate speed into the rolling history for the chart.
 */
export function useDaemonSocket() {
  useEffect(() => {
    if (started) return;
    started = true;
    connect();
    speedTimer = setInterval(sampleSpeed, 1000);
    // The socket is intentionally long-lived; no teardown on unmount (the app
    // root never unmounts), but guard against double-start under StrictMode.
    return () => {
      if (speedTimer) clearInterval(speedTimer);
      speedTimer = null;
      started = false;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      socket?.close();
      socket = null;
    };
  }, []);
}
