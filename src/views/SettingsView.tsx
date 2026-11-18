import {
  AlertTriangle,
  BadgeCheck,
  Cpu,
  FolderOpen,
  Info,
  PackagePlus,
  RotateCcw,
  ScanFace,
  ShieldAlert,
  SlidersHorizontal,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import { useBackend } from '../components/backend';
import { Callout, Modal } from '../components/ui';
import { errorMessage, useApp } from '../lib/store';
import type { ModelFamily, ModelInfo, Settings, SmallFacePolicy, Thoroughness } from '../lib/types';

export function SettingsView() {
  const backend = useBackend();
  const { settings, saveSettings, info, refreshInfo, toast } = useApp();
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [importing, setImporting] = useState<string | null>(null);

  useEffect(() => {
    backend.listModels().then(setModels, (e) => toast(errorMessage(e), 'error'));
  }, [backend, toast]);

  if (!settings) return null;
  const defaults = info?.engine?.defaultThresholds ?? { review: 0.28, match: 0.42 };
  const t = settings.thresholds ?? defaults;

  const update = async (patch: Partial<Settings>) => {
    try {
      await saveSettings({ ...settings, ...patch });
      if (patch.detectorId || patch.embedderId) {
        await refreshInfo();
        toast('Models switched. Re-scan events to use them.', 'success');
      }
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const detectors = models.filter((m) => m.kind === 'detector');
  const embedders = models.filter((m) => m.kind === 'embedder');

  const pickModel = async () => {
    const path = await backend.pickModel();
    if (path) setImporting(path);
  };

  return (
    <div className="page settings-page">
      <div className="page-header">
        <div className="titles">
          <h1>Settings</h1>
          <p>Defaults are tuned to be careful: when in doubt, Blurlist asks you instead of guessing.</p>
        </div>
      </div>

      <section className="card card-pad">
        <div className="section-title">
          <SlidersHorizontal size={18} />
          <h2>Matching</h2>
        </div>
        <div className="settings-grid">
          <div className="field">
            <label htmlFor="t-review">“Check these” from similarity {t.review.toFixed(2)}</label>
            <input
              id="t-review"
              type="range"
              min={0.1}
              max={0.6}
              step={0.01}
              value={t.review}
              onChange={(e) =>
                update({
                  thresholds: {
                    review: Number(e.target.value),
                    match: Math.max(t.match, Number(e.target.value)),
                  },
                })
              }
            />
            <span className="hint">
              Lower catches more look-alikes for you to check. Faces below this count as strangers.
            </span>
          </div>
          <div className="field">
            <label htmlFor="t-match">Confident match from similarity {t.match.toFixed(2)}</label>
            <input
              id="t-match"
              type="range"
              min={0.2}
              max={0.8}
              step={0.01}
              value={t.match}
              onChange={(e) =>
                update({
                  thresholds: {
                    review: Math.min(t.review, Number(e.target.value)),
                    match: Number(e.target.value),
                  },
                })
              }
            />
            <span className="hint">
              Above this, a face is treated as a roster match and blurred unless you reject it.
            </span>
          </div>
          <div className="field">
            <label htmlFor="min-face">Smallest face to identify: {settings.minFacePx}px</label>
            <input
              id="min-face"
              type="range"
              min={16}
              max={96}
              step={2}
              value={settings.minFacePx}
              onChange={(e) => update({ minFacePx: Number(e.target.value) })}
            />
            <span className="hint">Smaller faces can't be identified reliably.</span>
          </div>
          <div className="field">
            <span className="label">Faces too small to identify</span>
            <div className="segmented" style={{ alignSelf: 'flex-start' }}>
              {(
                [
                  ['review', 'Ask me'],
                  ['blur', 'Always blur'],
                  ['ignore', 'Leave them'],
                ] as [SmallFacePolicy, string][]
              ).map(([id, label]) => (
                <button
                  key={id}
                  className={settings.smallFacePolicy === id ? 'active' : ''}
                  onClick={() => update({ smallFacePolicy: id })}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="row" style={{ marginTop: 14 }}>
          <span className="muted small">
            Model defaults: check from {defaults.review.toFixed(2)}, match from {defaults.match.toFixed(2)}.
            Changes apply to new scans and to <strong>Re-match</strong>.
          </span>
          <span className="spacer" />
          <button
            className="btn btn-sm"
            disabled={settings.thresholds === null}
            onClick={() => update({ thresholds: null })}
          >
            <RotateCcw size={14} /> Use model defaults
          </button>
        </div>
      </section>

      <section className="card card-pad">
        <div className="section-title">
          <ScanFace size={18} />
          <h2>Finding faces</h2>
        </div>
        <div className="settings-grid">
          <div className="field">
            <span className="label">Thoroughness</span>
            <div className="segmented" style={{ alignSelf: 'flex-start' }}>
              {(
                [
                  ['fast', 'Fast'],
                  ['thorough', 'Thorough'],
                  ['maximum', 'Maximum'],
                ] as [Thoroughness, string][]
              ).map(([id, label]) => (
                <button
                  key={id}
                  className={settings.thoroughness === id ? 'active' : ''}
                  onClick={() => update({ thoroughness: id })}
                >
                  {label}
                </button>
              ))}
            </div>
            <span className="hint">
              Thorough also scans each photo in overlapping high-resolution tiles to find small faces at the
              back of group shots. Maximum is for big crowds and stage photos.
            </span>
          </div>
          <div className="field">
            <label htmlFor="det-t">Detection confidence {settings.detectionThreshold.toFixed(2)}</label>
            <input
              id="det-t"
              type="range"
              min={0.3}
              max={0.9}
              step={0.05}
              value={settings.detectionThreshold}
              onChange={(e) => update({ detectionThreshold: Number(e.target.value) })}
            />
            <span className="hint">Lower finds more partly hidden faces (and a few false ones).</span>
          </div>
          <div className="field">
            <label htmlFor="jpeg-q">Export JPEG quality {settings.jpegQuality}</label>
            <input
              id="jpeg-q"
              type="range"
              min={70}
              max={100}
              step={1}
              value={settings.jpegQuality}
              onChange={(e) => update({ jpegQuality: Number(e.target.value) })}
            />
          </div>
        </div>
      </section>

      <section className="card card-pad">
        <div className="section-title">
          <Cpu size={18} />
          <h2>Models</h2>
          <span className="spacer" />
          {info && (
            <button className="btn btn-sm" onClick={() => void backend.revealPath(info.userModelsDir)}>
              <FolderOpen size={14} /> Open models folder
            </button>
          )}
          <button className="btn btn-sm" onClick={pickModel}>
            <PackagePlus size={14} /> Import model…
          </button>
        </div>
        {info?.engineError && (
          <Callout tone="danger" icon={<AlertTriangle size={18} />}>
            {info.engineError}
          </Callout>
        )}
        {info?.engineNotice && (
          <Callout tone="warning" icon={<AlertTriangle size={18} />}>
            {info.engineNotice}
          </Callout>
        )}
        <div className="settings-grid">
          <ModelSelect
            label="Face detector"
            models={detectors}
            value={settings.detectorId}
            onChange={(id) => update({ detectorId: id })}
          />
          <ModelSelect
            label="Face recogniser"
            models={embedders}
            value={settings.embedderId}
            onChange={(id) => update({ embedderId: id })}
          />
        </div>
        <Callout icon={<Info size={18} />}>
          Blurlist ships with YuNet (MIT) and SFace (Apache-2.0). It also supports InsightFace's SCRFD and
          ArcFace models, but their published weights are licensed for{' '}
          <strong>non-commercial research only</strong>, so they are never bundled. If you are entitled to use
          them, put the files in the models folder.
        </Callout>
      </section>

      <section className="card card-pad about">
        <div className="section-title">
          <BadgeCheck size={18} />
          <h2>About Blurlist</h2>
        </div>
        {info && (
          <div className="about-grid">
            <span className="muted">Version</span>
            <span className="mono">
              {info.version} (build {info.buildNumber}, {info.gitSha})
            </span>
            <span className="muted">Built</span>
            <span className="mono">{info.buildDate}</span>
            <span className="muted">Data folder</span>
            <span className="mono truncate">{info.dataDir}</span>
            <span className="muted">Licence</span>
            <span>GNU GPL v3.0 or later</span>
          </div>
        )}
        <Callout tone="success" icon={<ShieldAlert size={18} />}>
          Blurlist has no network code and no telemetry. The roster, face crops and decisions are kept in a
          local database in the data folder. Removing an event in Scan deletes its face crops.
        </Callout>
      </section>

      {importing && (
        <ImportModelDialog path={importing} onClose={() => setImporting(null)} onDone={setModels} />
      )}
    </div>
  );
}

function ModelSelect({
  label,
  models,
  value,
  onChange,
}: {
  label: string;
  models: ModelInfo[];
  value: string;
  onChange: (id: string) => void;
}) {
  const backend = useBackend();
  const current = models.find((m) => m.id === value);
  return (
    <div className="field">
      <span className="label">{label}</span>
      <select className="input" value={value} onChange={(e) => onChange(e.target.value)}>
        {models.map((m) => (
          <option key={m.id} value={m.id} disabled={!m.path}>
            {m.name}
            {m.path ? '' : ' — not installed'}
          </option>
        ))}
      </select>
      {current && (
        <div className="row" style={{ gap: 6, flexWrap: 'wrap' }}>
          <span className={`badge ${current.permissive ? 'success' : 'warning'}`}>{current.license}</span>
          {current.bundled && <span className="badge">Bundled</span>}
          {current.sourceUrl && (
            <button
              className="btn btn-sm btn-ghost"
              onClick={() => void backend.openExternal(current.sourceUrl)}
            >
              Source
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function ImportModelDialog({
  path,
  onClose,
  onDone,
}: {
  path: string;
  onClose: () => void;
  onDone: (m: ModelInfo[]) => void;
}) {
  const backend = useBackend();
  const { toast } = useApp();
  const [name, setName] = useState(
    path
      .split(/[\\/]/)
      .pop()
      ?.replace(/\.onnx$/i, '') ?? 'Model',
  );
  const [family, setFamily] = useState<ModelFamily>('arcface');
  const [license, setLicense] = useState('');
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    try {
      onDone(await backend.importModel(path, name, family, license));
      toast('Model imported. Select it above to use it.', 'success');
      onClose();
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title="Import a model"
      subtitle={path}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" disabled={busy || !name.trim()} onClick={submit}>
            Import
          </button>
        </>
      }
    >
      <div className="field">
        <label htmlFor="m-name">Name</label>
        <input id="m-name" className="input" value={name} onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor="m-family">Model family</label>
        <select
          id="m-family"
          className="input"
          value={family}
          onChange={(e) => setFamily(e.target.value as ModelFamily)}
        >
          <option value="arcface">ArcFace-style recogniser (112×112, InsightFace layout)</option>
          <option value="sface">SFace-style recogniser (112×112, OpenCV layout)</option>
          <option value="scrfd">SCRFD detector with 5 keypoints</option>
          <option value="yunet">YuNet detector</option>
        </select>
      </div>
      <div className="field">
        <label htmlFor="m-license">Licence</label>
        <input
          id="m-license"
          className="input"
          placeholder="e.g. Apache-2.0"
          value={license}
          onChange={(e) => setLicense(e.target.value)}
        />
        <span className="hint">Check the model card. Many face models are licensed for research only.</span>
      </div>
    </Modal>
  );
}
