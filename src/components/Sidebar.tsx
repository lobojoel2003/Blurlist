import { Download, ScanFace, Settings as SettingsIcon, ShieldCheck, Users, ListChecks } from 'lucide-react';
import type { ReactNode } from 'react';

import { useApp, type View } from '../lib/store';

interface Item {
  view: View;
  step?: number;
  label: string;
  icon: ReactNode;
  count?: number;
  alert?: boolean;
}

export function Sidebar() {
  const { view, setView, people, scans, currentScanId, info } = useApp();
  const scan = scans.find((s) => s.id === currentScanId);
  const toReview = scan ? scan.counts.checkPending + (scan.counts.matches - scan.counts.matchesReviewed) : 0;
  const items: Item[] = [
    { view: 'roster', step: 1, label: 'Opt-out roster', icon: <Users size={17} />, count: people.length },
    { view: 'scan', step: 2, label: 'Scan event', icon: <ScanFace size={17} /> },
    {
      view: 'review',
      step: 3,
      label: 'Review',
      icon: <ListChecks size={17} />,
      count: scan ? toReview : undefined,
      alert: !!scan && scan.counts.checkPending > 0,
    },
    { view: 'export', step: 4, label: 'Export', icon: <Download size={17} /> },
  ];
  return (
    <aside className="sidebar">
      <div className="brand">
        <img src="/favicon.svg" alt="" />
        <div>
          <strong>Blurlist</strong>
          <span>Photo consent, handled</span>
        </div>
      </div>
      <div className="nav-label">Workflow</div>
      {items.map((it) => (
        <button
          key={it.view}
          className={`nav-item ${view === it.view ? 'active' : ''}`}
          onClick={() => setView(it.view)}
          aria-current={view === it.view ? 'page' : undefined}
        >
          <span className="nav-step">{it.step}</span>
          {it.label}
          {it.count !== undefined && it.count > 0 && (
            <span className={`nav-count ${it.alert ? 'alert' : ''}`}>{it.count}</span>
          )}
        </button>
      ))}
      <div className="spacer" />
      <button
        className={`nav-item ${view === 'settings' ? 'active' : ''}`}
        onClick={() => setView('settings')}
      >
        <SettingsIcon size={17} style={{ marginLeft: 3, marginRight: 2 }} />
        Settings
      </button>
      <div className="privacy-pill">
        <ShieldCheck size={18} style={{ flexShrink: 0 }} />
        <div>
          <strong>Works offline</strong>
          <span>Photos and faces never leave this computer.</span>
        </div>
      </div>
      <div className="sidebar-footer">{info ? `v${info.version} · build ${info.buildNumber}` : ' '}</div>
    </aside>
  );
}
