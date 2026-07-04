// Transient notifications (Phase 8). Auto-dismiss is handled by the store;
// clicking a toast removes it early.

import { dismissToast, useStore } from "../store/downloads";

export default function Toasts() {
  const { toasts } = useStore();
  if (toasts.length === 0) return null;
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast--${t.kind}`} onClick={() => dismissToast(t.id)}>
          {t.message}
        </div>
      ))}
    </div>
  );
}
