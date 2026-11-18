import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Folder,
  FolderOpen,
  Images,
  Loader2,
  ScanFace,
  Trash2,
  Users,
} from 'lucide-react';
import { useState } from 'react';

import { useBackend } from '../components/backend';
import { Callout, EmptyState, Progress, Stat } from '../components/ui';
import { basename, formatDate, plural } from '../lib/format';
import { errorMessage, useApp } from '../lib/store';
import type { ScanProgress, ScanSummary } from '../lib/types';

export function ScanView() {
  const backend = useBackend();
  const { people, scans, refreshScans, selectScan, setView, toast } = useApp();
  const [folder, setFolder] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState<ScanSummary | null>(null);

  const choose = async () => {
    const f = await backend.pickFolder('Choose the event folder');
    if (!f) return;
    setFolder(f);
    setName(basename(f));
    setFinished(null);
  };

  const start = async () => {
    if (!folder) return;
    setRunning(true);
    setFinished(null);
    setProgress(null);
    const un = await backend.listen('scan-progress', setProgress);
    try {
      const id = await backend.startScan(name, folder);
      await refreshScans();
      selectScan(id);
      setFinished(await backend.getScan(id));
      setFolder(null);
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      un();
      setRunning(false);
    }
  };

  const remove = async (s: ScanSummary) => {
    try {
      await backend.deleteScan(s.id);
      await refreshScans();
      toast(`Removed “${s.name}” and its face crops from Blurlist. The photos themselves were not touched.`);
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  return (
    <div className="page">
      <div className="page-header">
        <div className="titles">
          <span className="eyebrow">Step 2</span>
          <h1>Scan an event folder</h1>
          <p>
            Blurlist checks every photo in the folder and its sub-folders, finds every face and compares it
            with the roster. Your originals are only read, never changed.
          </p>
        </div>
      </div>

      {people.length === 0 && (
        <Callout tone="warning" icon={<Users size={18} />}>
          The roster is empty, so nobody can be matched yet.{' '}
          <a href="#" onClick={(e) => (e.preventDefault(), setView('roster'))}>
            Add the opt-out list first
          </a>
          .
        </Callout>
      )}

      <div className="card card-pad scan-setup">
        {running ? (
          <ScanRunning progress={progress} onCancel={() => backend.cancelTask()} />
        ) : finished ? (
          <ScanFinished
            scan={finished}
            onReview={() => setView('review')}
            onAnother={() => setFinished(null)}
          />
        ) : (
          <div className="col" style={{ gap: 18 }}>
            <button className={`folder-pick ${folder ? 'chosen' : ''}`} onClick={choose}>
              {folder ? <FolderOpen size={26} /> : <Folder size={26} />}
              <div className="col" style={{ gap: 2, minWidth: 0, alignItems: 'flex-start' }}>
                <strong>{folder ? basename(folder) : 'Choose the event folder'}</strong>
                <span className="muted small truncate" style={{ maxWidth: 640 }}>
                  {folder ?? 'For example the folder you copied from the camera or phone.'}
                </span>
              </div>
              <span className="spacer" />
              <span className="btn btn-sm">{folder ? 'Change' : 'Browse…'}</span>
            </button>
            <div className="row" style={{ alignItems: 'flex-end' }}>
              <div className="field" style={{ flex: 1 }}>
                <label htmlFor="scan-name">Event name</label>
                <input
                  id="scan-name"
                  className="input"
                  placeholder="Spring Field Day"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </div>
              <button className="btn btn-primary btn-lg" disabled={!folder} onClick={start}>
                <ScanFace size={18} /> Start scan
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="col" style={{ gap: 12 }}>
        <h2>Scanned events</h2>
        {scans.length === 0 ? (
          <div className="card">
            <EmptyState icon={<Images size={28} />} title="No events yet">
              Scans you run appear here, so you can come back to review or export them later.
            </EmptyState>
          </div>
        ) : (
          <div className="card">
            <div className="scan-list">
              {scans.map((s) => (
                <div key={s.id} className="scan-row">
                  <div className="scan-icon">
                    <Images size={18} />
                  </div>
                  <div className="col" style={{ gap: 0, minWidth: 0 }}>
                    <strong className="truncate">{s.name}</strong>
                    <span className="muted small truncate">
                      {formatDate(s.createdAt)} · {plural(s.counts.photosOk, 'photo')} ·{' '}
                      {plural(s.counts.faces, 'face')}
                    </span>
                  </div>
                  <span className="spacer" />
                  <ScanBadges scan={s} />
                  <button
                    className="btn btn-sm"
                    onClick={() => {
                      selectScan(s.id);
                      setView('review');
                    }}
                  >
                    Open <ArrowRight size={14} />
                  </button>
                  <button
                    className="btn btn-sm btn-ghost btn-icon"
                    title="Remove this scan from Blurlist"
                    onClick={() => remove(s)}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ScanBadges({ scan }: { scan: ScanSummary }) {
  if (scan.status !== 'complete') {
    return (
      <span className="badge danger">{scan.status === 'cancelled' ? 'Cancelled' : 'Did not finish'}</span>
    );
  }
  const c = scan.counts;
  return (
    <div className="row" style={{ gap: 6 }}>
      <span className="badge accent">{plural(c.matches, 'match', 'matches')}</span>
      {c.checkPending > 0 ? (
        <span className="badge warning">{c.checkPending} to check</span>
      ) : (
        <span className="badge success">
          <CheckCircle2 size={12} /> Reviewed
        </span>
      )}
    </div>
  );
}

function ScanRunning({ progress, onCancel }: { progress: ScanProgress | null; onCancel: () => void }) {
  const p = progress;
  const preparing = !p || p.phase === 'preparing';
  return (
    <div className="col" style={{ gap: 16 }}>
      <div className="row">
        <Loader2 size={20} className="spin" style={{ color: 'var(--accent)' }} />
        <h2>{preparing ? 'Looking through the folder…' : `Checking photo ${p.done} of ${p.total}`}</h2>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onCancel}>
          Cancel
        </button>
      </div>
      <Progress value={p?.done ?? 0} total={p?.total ?? 0} indeterminate={preparing} />
      <div className="row muted small" style={{ gap: 18 }}>
        <span>{plural(p?.faces ?? 0, 'face')} found</span>
        <span>{plural(p?.matches ?? 0, 'roster match', 'roster matches')}</span>
        <span>{p?.check ?? 0} to check</span>
        <span className="spacer" />
        <span className="truncate mono" style={{ maxWidth: 360 }}>
          {p?.current ?? ''}
        </span>
      </div>
    </div>
  );
}

function ScanFinished({
  scan,
  onReview,
  onAnother,
}: {
  scan: ScanSummary;
  onReview: () => void;
  onAnother: () => void;
}) {
  const c = scan.counts;
  return (
    <div className="col" style={{ gap: 16 }}>
      <div className="row">
        <CheckCircle2 size={22} style={{ color: 'var(--success)' }} />
        <h2>{scan.status === 'complete' ? `“${scan.name}” is scanned` : `The scan was ${scan.status}`}</h2>
      </div>
      <div className="stats">
        <Stat value={c.photosOk} label="photos checked" />
        <Stat value={c.faces} label="faces found" />
        <Stat value={c.matches} label="roster matches" />
        <Stat
          value={c.checkPending}
          label="to check by hand"
          tone={c.checkPending > 0 ? 'warning' : undefined}
        />
      </div>
      {(c.unsupported > 0 || c.errors > 0) && (
        <Callout tone="warning" icon={<AlertTriangle size={18} />}>
          {c.unsupported > 0 && (
            <>{plural(c.unsupported, 'file')} (such as HEIC photos or videos) could not be checked. </>
          )}
          {c.errors > 0 && <>{plural(c.errors, 'file')} could not be read. </>}
          They are listed on the Export screen and will not be exported.
        </Callout>
      )}
      <div className="row">
        <button className="btn btn-primary btn-lg" onClick={onReview}>
          Review matches <ArrowRight size={18} />
        </button>
        <button className="btn btn-lg" onClick={onAnother}>
          Scan another folder
        </button>
      </div>
    </div>
  );
}
