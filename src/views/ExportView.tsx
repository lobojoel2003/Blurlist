import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Download,
  FileX2,
  Folder,
  FolderOpen,
  Loader2,
  MapPinOff,
  ShieldCheck,
  Tags,
  UserX,
} from 'lucide-react';
import { useState } from 'react';

import { ScanPicker } from '../components/ScanPicker';
import { useBackend } from '../components/backend';
import { Callout, EmptyState, Progress, Stat } from '../components/ui';
import { basename, formatDuration, plural } from '../lib/format';
import { errorMessage, useApp } from '../lib/store';
import type { BlurStyle, ExportProgress, ExportReport } from '../lib/types';

const STYLES: { id: BlurStyle; label: string }[] = [
  { id: 'blur', label: 'Soft blur' },
  { id: 'pixelate', label: 'Pixelate' },
  { id: 'solid', label: 'Solid fill' },
];

export function ExportView() {
  const backend = useBackend();
  const { scans, currentScanId, setView, settings, saveSettings, toast, refreshScans } = useApp();
  const scan = scans.find((s) => s.id === currentScanId) ?? null;
  const [dest, setDest] = useState<string | null>(null);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<{ scanId: number; report: ExportReport } | null>(null);
  const report = result && result.scanId === currentScanId ? result.report : null;

  if (!scan) {
    return (
      <div className="page">
        <ExportHeader />
        <div className="card">
          <EmptyState
            icon={<Download size={28} />}
            title="Nothing to export yet"
            actions={
              <button className="btn btn-primary" onClick={() => setView('scan')}>
                Scan an event
              </button>
            }
          >
            Scan an event folder and review its matches, then export a copy that is safe to upload.
          </EmptyState>
        </div>
      </div>
    );
  }

  const c = scan.counts;
  const ready = c.photosOk - c.photosHeld;
  const unconfirmed = c.matches - c.matchesReviewed;

  const choose = async () => {
    const d = await backend.pickFolder('Choose where to save the export');
    if (d) setDest(d);
  };

  const run = async () => {
    if (!dest) return;
    setRunning(true);
    setResult(null);
    setProgress(null);
    const un = await backend.listen('export-progress', setProgress);
    try {
      const r = await backend.exportScan(scan.id, dest);
      setResult({ scanId: scan.id, report: r });
      void refreshScans();
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      un();
      setRunning(false);
    }
  };

  return (
    <div className="page">
      <ExportHeader>
        <ScanPicker />
      </ExportHeader>

      {scan.status !== 'complete' ? (
        <Callout tone="danger" icon={<AlertTriangle size={18} />}>
          This scan did not finish, so some photos were never checked. Scan the folder again before exporting.
        </Callout>
      ) : (
        <>
          <div className="stats">
            <Stat value={ready} label="photos ready to export" tone="success" />
            <Stat value={c.toBlur} label={`faces will be blurred`} />
            <Stat
              value={c.photosHeld}
              label="photos held back"
              tone={c.photosHeld > 0 ? 'warning' : undefined}
            />
            <Stat
              value={c.unsupported + c.errors}
              label="files not checked"
              tone={c.unsupported + c.errors > 0 ? 'danger' : undefined}
            />
          </div>

          {c.photosHeld > 0 && (
            <Callout tone="warning" icon={<AlertTriangle size={18} />}>
              <div className="row">
                <span>
                  <strong>{plural(c.photosHeld, 'photo')} will be held back</strong> because{' '}
                  {plural(c.checkPending, 'face')} in the “Check these” pile still need a decision. Blurlist
                  never guesses.
                </span>
                <span className="spacer" />
                <button className="btn btn-sm" onClick={() => setView('review')}>
                  Resolve <ArrowRight size={14} />
                </button>
              </div>
            </Callout>
          )}
          {unconfirmed > 0 && (
            <Callout icon={<ShieldCheck size={18} />}>
              {plural(unconfirmed, 'roster match', 'roster matches')} {unconfirmed === 1 ? 'has' : 'have'} not
              been confirmed one by one. To be safe they will be blurred.
            </Callout>
          )}

          <div className="export-grid">
            <div className="card card-pad col" style={{ gap: 18 }}>
              <h2>Export a copy that is safe to upload</h2>
              <button className={`folder-pick ${dest ? 'chosen' : ''}`} onClick={choose} disabled={running}>
                {dest ? <FolderOpen size={24} /> : <Folder size={24} />}
                <div className="col" style={{ gap: 2, minWidth: 0, alignItems: 'flex-start' }}>
                  <strong>{dest ? basename(dest) : 'Choose where to save it'}</strong>
                  <span className="muted small truncate" style={{ maxWidth: 460 }}>
                    {dest
                      ? `A new folder “${scan.name} (blurred …)” is created here.`
                      : 'Pick a folder outside the event folder, e.g. your Desktop.'}
                  </span>
                </div>
                <span className="spacer" />
                <span className="btn btn-sm">{dest ? 'Change' : 'Browse…'}</span>
              </button>
              {settings && (
                <div className="field">
                  <span className="label">How faces are hidden</span>
                  <div className="segmented" style={{ alignSelf: 'flex-start' }}>
                    {STYLES.map((s) => (
                      <button
                        key={s.id}
                        className={settings.blurStyle === s.id ? 'active' : ''}
                        onClick={() => void saveSettings({ ...settings, blurStyle: s.id })}
                      >
                        <span className={`style-swatch ${s.id}`} /> {s.label}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {running ? (
                <div className="col">
                  <div className="row">
                    <Loader2 size={16} className="spin" />
                    <strong>Exporting {progress ? `${progress.done} of ${progress.total}` : '…'}</strong>
                    <span className="spacer" />
                    <button className="btn btn-sm" onClick={() => backend.cancelTask()}>
                      Cancel
                    </button>
                  </div>
                  <Progress
                    value={progress?.done ?? 0}
                    total={progress?.total ?? 0}
                    indeterminate={!progress}
                  />
                </div>
              ) : (
                <button className="btn btn-primary btn-lg" disabled={!dest || ready === 0} onClick={run}>
                  <Download size={18} /> Export {plural(ready, 'photo')}
                </button>
              )}
            </div>
            <div className="card card-pad col privacy-card">
              <h3>Every exported photo is</h3>
              <div className="check-row">
                <ShieldCheck size={18} /> Blurred where you decided, with an irreversible blur
              </div>
              <div className="check-row">
                <MapPinOff size={18} /> Stripped of GPS location, camera, date and all other metadata
              </div>
              <div className="check-row">
                <Tags size={18} /> Free of embedded thumbnails that could show the original face
              </div>
              <div className="check-row">
                <UserX size={18} /> Written without any names or lists of who was blurred
              </div>
              <div className="check-row">
                <FolderOpen size={18} /> Saved as a new copy — your originals are never changed
              </div>
            </div>
          </div>

          {report && <ExportResult report={report} />}
        </>
      )}
    </div>
  );
}

function ExportHeader({ children }: { children?: React.ReactNode }) {
  return (
    <div className="page-header">
      <div className="titles">
        <span className="eyebrow">Step 4</span>
        <h1>Export</h1>
      </div>
      <div className="spacer" />
      {children}
    </div>
  );
}

function ExportResult({ report }: { report: ExportReport }) {
  const backend = useBackend();
  const problems = report.heldBack.length + report.failed.length + report.notSupported.length;
  return (
    <div className="card card-pad col" style={{ gap: 14 }}>
      <div className="row">
        <CheckCircle2 size={22} style={{ color: 'var(--success)' }} />
        <h2>
          {plural(report.exported, 'photo')} exported · {plural(report.facesBlurred, 'face')} blurred
        </h2>
        <span className="spacer" />
        <span className="subtle small">{formatDuration(report.durationMs)}</span>
        <button className="btn btn-primary" onClick={() => void backend.revealPath(report.outputDir)}>
          <FolderOpen size={16} /> Open folder
        </button>
      </div>
      <span className="mono muted truncate">{report.outputDir}</span>
      {problems > 0 && (
        <div className="list result-list">
          {report.heldBack.map((h) => (
            <div key={`h-${h.relPath}`} className="list-row">
              <AlertTriangle size={16} style={{ color: 'var(--warning)' }} />
              <strong className="truncate">{h.relPath}</strong>
              <span className="muted small">held back — {h.reason}</span>
            </div>
          ))}
          {report.failed.map((h) => (
            <div key={`f-${h.relPath}`} className="list-row">
              <FileX2 size={16} style={{ color: 'var(--danger)' }} />
              <strong className="truncate">{h.relPath}</strong>
              <span className="muted small">not exported — {h.reason}</span>
            </div>
          ))}
          {report.notSupported.map((p) => (
            <div key={`u-${p}`} className="list-row">
              <FileX2 size={16} style={{ color: 'var(--danger)' }} />
              <strong className="truncate">{p}</strong>
              <span className="muted small">not checked — unsupported file type; it was not copied</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
