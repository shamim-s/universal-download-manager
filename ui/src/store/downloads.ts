// Dependency-free reactive store for download state (Phase 8).
//
// A tiny external store (subscribe / getSnapshot) consumed via React's
// `useSyncExternalStore`. We avoid pulling in Zustand to keep the bundle lean;
// the surface is the same: a single immutable state object replaced on each
// change, plus action functions that the socket layer and components call.

import { useSyncExternalStore } from "react";
import {
  AppSettings,
  DEFAULT_SETTINGS,
  DownloadJob,
  JobStatus,
  ServerMessage,
} from "../types";

export type Filter = "all" | "active" | "queued" | "completed" | "failed";

export type ToastKind = "info" | "success" | "error";
export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

export interface State {
  connected: boolean;
  jobs: DownloadJob[];
  settings: AppSettings;
  filter: Filter;
  search: string;
  /** Aggregate active speed samples (bytes/s), newest last, capped at 60. */
  speedHistory: number[];
  toasts: Toast[];
}

const SPEED_HISTORY_LEN = 60;

let state: State = {
  connected: false,
  jobs: [],
  settings: DEFAULT_SETTINGS,
  filter: "all",
  search: "",
  speedHistory: new Array(SPEED_HISTORY_LEN).fill(0),
  toasts: [],
};

const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function setState(patch: Partial<State>) {
  state = { ...state, ...patch };
  emit();
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getState(): State {
  return state;
}

/** Subscribe a component to the whole store; re-renders on any change. */
export function useStore(): State {
  return useSyncExternalStore(subscribe, getState, getState);
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

export function setConnected(connected: boolean) {
  setState({ connected });
}

export function setFilter(filter: Filter) {
  setState({ filter });
}

export function setSearch(search: string) {
  setState({ search });
}

export function setSettings(settings: AppSettings) {
  setState({ settings });
}

let toastSeq = 0;
export function pushToast(kind: ToastKind, message: string) {
  const id = ++toastSeq;
  setState({ toasts: [...state.toasts, { id, kind, message }] });
  // Auto-dismiss after 4s.
  setTimeout(() => dismissToast(id), 4000);
}

export function dismissToast(id: number) {
  setState({ toasts: state.toasts.filter((t) => t.id !== id) });
}

/** Sample the current aggregate active speed into the rolling history. */
export function sampleSpeed() {
  const total = state.jobs
    .filter((j) => j.status === "active")
    .reduce((sum, j) => sum + (j.speedBps ?? 0), 0);
  const next = [...state.speedHistory.slice(1), total];
  setState({ speedHistory: next });
}

function upsertJob(job: DownloadJob) {
  const idx = state.jobs.findIndex((j) => j.id === job.id);
  if (idx === -1) {
    setState({ jobs: [job, ...state.jobs] });
  } else {
    const jobs = state.jobs.slice();
    // Preserve live-only fields if the incoming snapshot omits them.
    jobs[idx] = { ...jobs[idx], ...job };
    setState({ jobs });
  }
}

function patchJob(id: string, patch: Partial<DownloadJob>) {
  const idx = state.jobs.findIndex((j) => j.id === id);
  if (idx === -1) return;
  const jobs = state.jobs.slice();
  jobs[idx] = { ...jobs[idx], ...patch };
  setState({ jobs });
}

function setStatus(id: string, status: JobStatus, patch: Partial<DownloadJob> = {}) {
  patchJob(id, { status, speedBps: 0, eta: undefined, ...patch });
}

/** Apply one inbound server message to the store. */
export function applyServerMessage(msg: ServerMessage) {
  switch (msg.type) {
    case "ALL_JOBS":
      setState({ jobs: msg.jobs });
      break;
    case "JOB_ADDED":
      upsertJob(msg.job);
      break;
    case "JOB_STARTED":
      patchJob(msg.jobId, {
        fileSize: msg.fileSize,
        segmentCount: msg.segmentCount,
        status: "active",
      });
      break;
    case "DOWNLOAD_PROMPT":
      // Handled by the socket layer (opens the New Download window); no state.
      break;
    case "JOB_PROGRESS":
      patchJob(msg.jobId, {
        downloadedBytes: msg.downloadedBytes,
        speedBps: msg.speedBps,
        eta: msg.eta,
        status: "active",
      });
      break;
    case "JOB_PAUSED":
      setStatus(msg.jobId, "paused");
      break;
    case "JOB_CANCELLED":
      setStatus(msg.jobId, "cancelled");
      break;
    case "JOB_COMPLETED": {
      // Snap to the exact final size — the last JOB_PROGRESS tick can lag.
      const prev = state.jobs.find((j) => j.id === msg.jobId);
      setStatus(msg.jobId, "completed", {
        completedAt: new Date().toISOString(),
        downloadedBytes: msg.totalBytes,
        fileSize: prev?.fileSize ?? msg.totalBytes,
        savePath: msg.finalPath,
      });
      break;
    }
    case "JOB_FAILED":
      setStatus(msg.jobId, "failed", { error: msg.error });
      break;
    case "JOB_REMOVED":
      setState({ jobs: state.jobs.filter((j) => j.id !== msg.jobId) });
      break;
    case "SETTINGS_UPDATED":
      setState({ settings: msg.settings });
      break;
  }
}
