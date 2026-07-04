// Real-time aggregate speed chart (Phase 8): a lightweight SVG area chart over
// the last 60 one-second samples. No charting dependency — just an SVG path.

import { useStore } from "../store/downloads";
import { formatSpeed } from "../utils/format";

const W = 280;
const H = 56;

export default function SpeedGraph() {
  const { speedHistory } = useStore();
  const n = speedHistory.length;
  const peak = Math.max(1, ...speedHistory);
  const current = speedHistory[n - 1] ?? 0;

  const stepX = W / Math.max(1, n - 1);
  const points = speedHistory.map((v, i) => {
    const x = i * stepX;
    const y = H - (v / peak) * (H - 4) - 2;
    return [x, y] as const;
  });

  const line = points
    .map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`)
    .join(" ");
  const area = `${line} L${W},${H} L0,${H} Z`;

  return (
    <div className="speed-graph">
      <div className="speed-graph__head">
        <span className="speed-graph__label">Total speed</span>
        <span className="speed-graph__value">{formatSpeed(current)}</span>
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="speed-graph__svg">
        <defs>
          <linearGradient id="speedFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.45" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path d={area} fill="url(#speedFill)" />
        <path d={line} fill="none" stroke="var(--accent)" strokeWidth="1.5" />
      </svg>
      <div className="speed-graph__peak">peak {formatSpeed(peak)}</div>
    </div>
  );
}
