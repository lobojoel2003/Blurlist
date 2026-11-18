import { AlertCircle, CheckCircle2, Info, X } from 'lucide-react';

import { useApp } from '../lib/store';

export function Toasts() {
  const { toasts, dismissToast } = useApp();
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`} role={t.kind === 'error' ? 'alert' : 'status'}>
          {t.kind === 'error' ? (
            <AlertCircle size={18} />
          ) : t.kind === 'success' ? (
            <CheckCircle2 size={18} />
          ) : (
            <Info size={18} />
          )}
          <span style={{ flex: 1 }}>{t.message}</span>
          <button
            className="btn btn-ghost btn-icon btn-sm"
            style={{ color: 'inherit' }}
            onClick={() => dismissToast(t.id)}
            aria-label="Dismiss"
          >
            <X size={15} />
          </button>
        </div>
      ))}
    </div>
  );
}
