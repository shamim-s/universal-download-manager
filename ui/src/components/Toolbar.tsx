// Toolbar (Phase 8): brand + connection status, search, aggregate stats, and
// the primary actions (Add URL, Pause all, Settings). Hosts the AddURLModal.

import { useState } from "react";
import { sendMessage } from "../hooks/useDaemonSocket";
import { setSearch, useStore } from "../store/downloads";
import { formatSpeed } from "../utils/format";
import AddURLModal from "./AddURLModal";

export default function Toolbar({
  onOpenSettings,
  onOpenSetup,
}: {
  onOpenSettings: () => void;
  onOpenSetup: () => void;
}) {
  const { connected, jobs, search } = useStore();
  const [showAdd, setShowAdd] = useState(false);

  const active = jobs.filter((j) => j.status === "active");
  const totalSpeed = active.reduce((s, j) => s + (j.speedBps ?? 0), 0);

  function pauseAll() {
    for (const j of active) sendMessage({ type: "PAUSE", jobId: j.id });
  }

  return (
    <header className="toolbar">
      <div className="toolbar__brand">
        <span className="toolbar__logo">⬇</span>
        <span className="toolbar__title">UDM</span>
        <span className={`conn ${connected ? "conn--up" : "conn--down"}`} title={connected ? "Connected to daemon" : "Daemon offline"}>
          <span className="conn__dot" />
          {connected ? "Connected" : "Offline"}
        </span>
      </div>

      <div className="toolbar__stats">
        <span className="stat">
          <strong>{active.length}</strong> active
        </span>
        <span className="stat">
          <strong>{formatSpeed(totalSpeed)}</strong>
        </span>
      </div>

      <div className="toolbar__actions">
        <input
          className="toolbar__search"
          placeholder="Search downloads…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <button className="btn btn--ghost" onClick={pauseAll} disabled={active.length === 0}>
          Pause all
        </button>
        <button className="btn btn--ghost" onClick={onOpenSetup} title="Setup & help">
          ?
        </button>
        <button className="btn btn--ghost" onClick={onOpenSettings} title="Settings">
          ⚙
        </button>
        <button className="btn btn--primary" onClick={() => setShowAdd(true)}>
          + Add URL
        </button>
      </div>

      {showAdd && <AddURLModal onClose={() => setShowAdd(false)} />}
    </header>
  );
}
