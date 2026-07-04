// Main download list (Phase 8). Applies the active filter + search, then maps
// matching jobs to <DownloadItem /> rows. Shows tab counts and an empty state.

import { useMemo } from "react";
import { DownloadJob } from "../types";
import { Filter, setFilter, useStore } from "../store/downloads";
import DownloadItem from "./DownloadItem";

const TABS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "active", label: "Active" },
  { key: "queued", label: "Queued" },
  { key: "completed", label: "Completed" },
  { key: "failed", label: "Failed" },
];

function matchesFilter(job: DownloadJob, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return job.status === "active" || job.status === "paused";
    case "queued":
      return job.status === "queued";
    case "completed":
      return job.status === "completed";
    case "failed":
      return job.status === "failed" || job.status === "cancelled";
  }
}

export default function DownloadList() {
  const { jobs, filter, search } = useStore();

  const counts = useMemo(() => {
    const c: Record<Filter, number> = { all: 0, active: 0, queued: 0, completed: 0, failed: 0 };
    for (const t of TABS) c[t.key] = jobs.filter((j) => matchesFilter(j, t.key)).length;
    return c;
  }, [jobs]);

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase();
    return jobs.filter(
      (j) =>
        matchesFilter(j, filter) &&
        (q === "" || j.filename.toLowerCase().includes(q) || j.url.toLowerCase().includes(q)),
    );
  }, [jobs, filter, search]);

  return (
    <div className="dl-list">
      <div className="dl-list__tabs">
        {TABS.map((t) => (
          <button
            key={t.key}
            className={`tab ${filter === t.key ? "tab--active" : ""}`}
            onClick={() => setFilter(t.key)}
          >
            {t.label}
            <span className="tab__count">{counts[t.key]}</span>
          </button>
        ))}
      </div>

      <div className="dl-list__rows">
        {visible.length === 0 ? (
          <div className="dl-list__empty">
            {jobs.length === 0
              ? "No downloads yet. Click “Add URL” or send one from your browser."
              : "Nothing matches this filter."}
          </div>
        ) : (
          visible.map((job) => <DownloadItem key={job.id} job={job} />)
        )}
      </div>
    </div>
  );
}
