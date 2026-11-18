import { CalendarDays } from 'lucide-react';

import { formatDate } from '../lib/format';
import { useApp } from '../lib/store';

/** Selects which scanned event the Review and Export screens work on. */
export function ScanPicker() {
  const { scans, currentScanId, selectScan } = useApp();
  if (scans.length === 0) return null;
  return (
    <label className="row" style={{ gap: 8 }}>
      <CalendarDays size={16} className="subtle" />
      <span className="sr-only">Event</span>
      <select
        className="input"
        value={currentScanId ?? ''}
        onChange={(e) => selectScan(Number(e.target.value))}
        style={{ minWidth: 240 }}
      >
        {scans.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name} · {formatDate(s.createdAt)}
            {s.status !== 'complete' ? ` (${s.status})` : ''}
          </option>
        ))}
      </select>
    </label>
  );
}
