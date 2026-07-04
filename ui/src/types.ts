// UI types mirroring the daemon protocol.
// Keep in sync with daemon/crates/daemon/src/protocol.rs and extension/shared/protocol.js.

export type JobStatus =
  | "queued"
  | "active"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface Segment {
  id: number;
  byteStart: number;
  byteEnd: number;
  downloaded: number;
  status: JobStatus;
}

export interface DownloadJob {
  id: string;
  url: string;
  filename: string;
  savePath: string;
  fileSize: number | null;
  downloadedBytes: number;
  status: JobStatus;
  priority: number;
  createdAt?: string;
  completedAt?: string | null;
  error?: string | null;
  segments?: Segment[];
  sourceBrowser: string;
  // Live, client-side only (populated from JOB_PROGRESS, not persisted):
  speedBps?: number;
  eta?: number;
  /** Number of parallel segments, from JOB_STARTED. */
  segmentCount?: number;
}

export interface AppSettings {
  downloadDirectory: string;
  maxConcurrentDownloads: number;
  maxSegmentsPerDownload: number;
  maxBandwidthKbps: number | null;
  autoStartOnBoot: boolean;
  minimizeToTray: boolean;
  daemonPort: number;
  promptBeforeDownload: boolean;
  /** Sort downloads into per-type subfolders (Video, Music, …). */
  autoCategorize: boolean;
  /** Watch the clipboard for copied download URLs and offer to add them. */
  clipboardWatcher: boolean;
}

export interface AddDownloadPayload {
  url: string;
  filename?: string;
  savePath?: string;
  cookies?: string;
  referrer?: string;
  headers?: Record<string, string>;
  sourceBrowser: string;
  priority?: number;
  fileSize?: number;
}

export type ClientMessage =
  | { type: "ADD_DOWNLOAD"; payload: AddDownloadPayload }
  | { type: "PAUSE"; jobId: string }
  | { type: "RESUME"; jobId: string }
  | { type: "CANCEL"; jobId: string }
  | { type: "RETRY"; jobId: string }
  | { type: "REMOVE"; jobId: string }
  | { type: "SET_PRIORITY"; jobId: string; priority: number }
  | { type: "GET_ALL_JOBS" }
  | { type: "SET_BANDWIDTH"; kbps: number | null }
  | { type: "GET_SETTINGS" }
  | { type: "UPDATE_SETTINGS"; settings: AppSettings };

export type ServerMessage =
  | { type: "JOB_ADDED"; job: DownloadJob }
  | { type: "DOWNLOAD_PROMPT"; payload: AddDownloadPayload }
  | {
      type: "JOB_STARTED";
      jobId: string;
      fileSize: number | null;
      segmentCount: number;
    }
  | {
      type: "JOB_PROGRESS";
      jobId: string;
      downloadedBytes: number;
      speedBps: number;
      eta: number;
    }
  | { type: "JOB_COMPLETED"; jobId: string; finalPath: string; totalBytes: number }
  | { type: "JOB_FAILED"; jobId: string; error: string }
  | { type: "JOB_PAUSED"; jobId: string }
  | { type: "JOB_CANCELLED"; jobId: string }
  | { type: "JOB_REMOVED"; jobId: string }
  | { type: "ALL_JOBS"; jobs: DownloadJob[] }
  | { type: "SETTINGS_UPDATED"; settings: AppSettings };

export const DEFAULT_SETTINGS: AppSettings = {
  downloadDirectory: "",
  maxConcurrentDownloads: 3,
  maxSegmentsPerDownload: 8,
  maxBandwidthKbps: null,
  autoStartOnBoot: false,
  minimizeToTray: true,
  daemonPort: 60123,
  promptBeforeDownload: true,
  autoCategorize: true,
  clipboardWatcher: false,
};
