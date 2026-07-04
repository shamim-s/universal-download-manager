// Shared message shapes between extensions and the daemon.
// Keep in sync with daemon/crates/daemon/src/protocol.rs and ui/src/types.ts.

export const ClientMessageType = {
  ADD_DOWNLOAD: "ADD_DOWNLOAD",
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
  ALL_JOBS: "ALL_JOBS",
  SETTINGS_UPDATED: "SETTINGS_UPDATED",
};

export const DAEMON_URL = "ws://127.0.0.1:60123";
