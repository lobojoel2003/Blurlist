import {
  AlertTriangle,
  Camera,
  CheckCircle2,
  FolderInput,
  ImagePlus,
  Loader2,
  Search,
  Trash2,
  UserPlus,
  Users,
  XCircle,
} from 'lucide-react';
import { useMemo, useState } from 'react';

import { useBackend } from '../components/backend';
import { Callout, EmptyState, Modal, Progress } from '../components/ui';
import { plural } from '../lib/format';
import { errorMessage, useApp } from '../lib/store';
import type { ImportReport, Inspection, Person } from '../lib/types';

export function RosterView() {
  const backend = useBackend();
  const { people, refreshPeople, toast, scans } = useApp();
  const [query, setQuery] = useState('');
  const [group, setGroup] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<Person | null>(null);
  const [importing, setImporting] = useState<{ done: number; total: number } | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);

  const groups = useMemo(
    () => [...new Set(people.map((p) => p.group).filter(Boolean))].sort((a, b) => a.localeCompare(b)),
    [people],
  );
  const filtered = people.filter(
    (p) =>
      (group === null || p.group === group) &&
      (query === '' || `${p.name} ${p.group} ${p.notes}`.toLowerCase().includes(query.toLowerCase())),
  );

  const importFolder = async () => {
    const folder = await backend.pickFolder('Choose the roster folder');
    if (!folder) return;
    setImporting({ done: 0, total: 0 });
    const un = await backend.listen('import-progress', (p) => setImporting(p));
    try {
      const r = await backend.importRoster(folder);
      setReport(r);
      await refreshPeople();
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      un();
      setImporting(null);
    }
  };

  const staleScans = scans.filter((s) => s.status === 'complete' && s.rosterSize !== people.length).length;

  return (
    <div className="page">
      <div className="page-header">
        <div className="titles">
          <span className="eyebrow">Step 1</span>
          <h1>Opt-out roster</h1>
          <p>
            Everyone whose family has opted out of photos, with one clear reference photo each. Blurlist looks
            for these faces in every event photo.
          </p>
        </div>
        <div className="spacer" />
        <button className="btn" onClick={importFolder} disabled={!!importing}>
          <FolderInput size={16} /> Import folder
        </button>
        <button className="btn btn-primary" onClick={() => setAdding(true)}>
          <UserPlus size={16} /> Add person
        </button>
      </div>

      {importing && (
        <div className="card card-pad col">
          <div className="row">
            <Loader2 size={16} className="spin" />
            <strong>Importing roster photos…</strong>
            <span className="spacer" />
            <span className="muted small">
              {importing.done} / {importing.total || '…'}
            </span>
          </div>
          <Progress value={importing.done} total={importing.total} indeterminate={importing.total === 0} />
        </div>
      )}

      {staleScans > 0 && (
        <Callout tone="warning" icon={<AlertTriangle size={18} />}>
          The roster changed since {plural(staleScans, 'event was', 'events were')} scanned. Open the event in
          Review and choose <strong>Re-match with current roster</strong> — it takes seconds and keeps your
          decisions.
        </Callout>
      )}

      {people.length === 0 ? (
        <div className="card">
          <EmptyState
            icon={<Users size={30} />}
            title="Start with the opt-out list"
            actions={
              <>
                <button className="btn" onClick={importFolder}>
                  <FolderInput size={16} /> Import a folder
                </button>
                <button className="btn btn-primary" onClick={() => setAdding(true)}>
                  <UserPlus size={16} /> Add the first person
                </button>
              </>
            }
          >
            Add people one at a time, or import a folder of photos named after each person — for example{' '}
            <code>Maple Cabin/Ava Thompson.jpg</code>. Sub-folders become groups.
          </EmptyState>
        </div>
      ) : (
        <>
          <div className="row" style={{ flexWrap: 'wrap' }}>
            <div className="search">
              <Search size={16} />
              <input
                className="input"
                placeholder="Search names, groups, notes"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
            {groups.length > 0 && (
              <div className="chips">
                <button className={`chip ${group === null ? 'active' : ''}`} onClick={() => setGroup(null)}>
                  All · {people.length}
                </button>
                {groups.map((g) => (
                  <button
                    key={g}
                    className={`chip ${group === g ? 'active' : ''}`}
                    onClick={() => setGroup(g)}
                  >
                    {g} · {people.filter((p) => p.group === g).length}
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="people-grid">
            {filtered.map((p) => (
              <PersonCard key={p.id} person={p} onOpen={() => setEditing(p)} />
            ))}
            {filtered.length === 0 && <p className="muted">No one matches “{query}”.</p>}
          </div>
        </>
      )}

      {adding && <AddPersonDialog onClose={() => setAdding(false)} />}
      {editing && (
        <PersonDialog
          key={editing.id}
          person={people.find((p) => p.id === editing.id) ?? editing}
          onClose={() => setEditing(null)}
        />
      )}
      {report && <ImportReportDialog report={report} onClose={() => setReport(null)} />}
    </div>
  );
}

function PersonCard({ person, onOpen }: { person: Person; onOpen: () => void }) {
  const backend = useBackend();
  const ref = person.references[0];
  const weak =
    person.references.length > 0 && person.references.every((r) => r.facePx < 80 || r.detScore < 0.7);
  return (
    <button className="person-card card" onClick={onOpen}>
      <div className="person-photo">
        {ref ? (
          <img
            src={backend.mediaUrl('ref', ref.id)}
            alt={`Reference photo of ${person.name}`}
            loading="lazy"
          />
        ) : (
          <div className="no-photo">
            <Camera size={22} />
            <span>No photo yet</span>
          </div>
        )}
        {person.references.length > 1 && <span className="photo-count">+{person.references.length - 1}</span>}
      </div>
      <div className="person-meta">
        <strong className="truncate">{person.name}</strong>
        <span className="muted small truncate">{person.group || 'No group'}</span>
        {(person.references.length === 0 || weak) && (
          <span className="badge warning" style={{ alignSelf: 'flex-start', marginTop: 4 }}>
            <AlertTriangle size={12} />{' '}
            {person.references.length === 0 ? 'Needs a photo' : 'Low-quality photo'}
          </span>
        )}
      </div>
    </button>
  );
}

/** Choose a photo, pick the right face, check quality. */
function FacePicker({
  inspection,
  selected,
  onSelect,
}: {
  inspection: Inspection;
  selected: number | null;
  onSelect: (i: number) => void;
}) {
  if (inspection.faces.length === 0) {
    return (
      <Callout tone="danger" icon={<XCircle size={18} />}>
        No face was found in this photo. Choose a photo where the face is clearly visible and looking roughly
        toward the camera.
      </Callout>
    );
  }
  const chosen = inspection.faces.find((f) => f.index === selected);
  return (
    <div className="col" style={{ gap: 12 }}>
      {inspection.faces.length > 1 && (
        <p className="muted small">
          {inspection.faces.length} faces found. {inspection.suggested === null ? 'Choose' : 'Confirm'} which
          one is this person:
        </p>
      )}
      <div className="face-choices">
        {inspection.faces.map((f) => (
          <button
            key={f.index}
            className={`face-choice ${selected === f.index ? 'selected' : ''}`}
            onClick={() => onSelect(f.index)}
            aria-pressed={selected === f.index}
          >
            <img src={f.preview} alt={`Face ${f.index + 1}`} />
            {selected === f.index && <CheckCircle2 className="tick" size={20} />}
          </button>
        ))}
      </div>
      {chosen?.warnings.map((w) => (
        <Callout key={w} tone="warning" icon={<AlertTriangle size={18} />}>
          {w}
        </Callout>
      ))}
    </div>
  );
}

function usePhotoInspection() {
  const backend = useBackend();
  const { toast } = useApp();
  const [path, setPath] = useState<string | null>(null);
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const choose = async () => {
    const p = await backend.pickImage('Choose a reference photo');
    if (!p) return;
    setBusy(true);
    setPath(p);
    setInspection(null);
    try {
      const insp = await backend.inspectPhoto(p);
      setInspection(insp);
      setSelected(insp.suggested);
    } catch (e) {
      toast(errorMessage(e), 'error');
      setPath(null);
    } finally {
      setBusy(false);
    }
  };
  const reset = () => {
    setPath(null);
    setInspection(null);
    setSelected(null);
  };
  return { path, inspection, selected, setSelected, busy, choose, reset };
}

function AddPersonDialog({ onClose }: { onClose: () => void }) {
  const backend = useBackend();
  const { refreshPeople, toast, people } = useApp();
  const [name, setName] = useState('');
  const [group, setGroup] = useState('');
  const [notes, setNotes] = useState('');
  const [saving, setSaving] = useState(false);
  const photo = usePhotoInspection();
  const groups = [...new Set(people.map((p) => p.group).filter(Boolean))];

  const canSave = name.trim() !== '' && photo.path !== null && photo.selected !== null && !saving;
  const save = async () => {
    if (!canSave || !photo.path) return;
    setSaving(true);
    let id: number | null = null;
    try {
      id = await backend.createPerson(name, group, notes);
      await backend.addReference(id, photo.path, photo.selected);
      await refreshPeople();
      toast(`${name.trim()} was added to the roster`, 'success');
      onClose();
    } catch (e) {
      if (id !== null) await backend.deletePerson(id).catch(() => undefined);
      toast(errorMessage(e), 'error');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title="Add to the opt-out roster"
      subtitle="One clear, recent, front-facing photo works best."
      onClose={onClose}
      icon={<UserPlus size={22} className="subtle" />}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={save} disabled={!canSave}>
            {saving ? <Loader2 size={16} className="spin" /> : <UserPlus size={16} />} Add person
          </button>
        </>
      }
    >
      <div className="field">
        <label htmlFor="p-name">Name</label>
        <input id="p-name" className="input" value={name} onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="row" style={{ alignItems: 'flex-start' }}>
        <div className="field" style={{ flex: 1 }}>
          <label htmlFor="p-group">Group (optional)</label>
          <input
            id="p-group"
            className="input"
            list="groups"
            placeholder="Class, cabin, team…"
            value={group}
            onChange={(e) => setGroup(e.target.value)}
          />
          <datalist id="groups">
            {groups.map((g) => (
              <option key={g} value={g} />
            ))}
          </datalist>
        </div>
        <div className="field" style={{ flex: 1 }}>
          <label htmlFor="p-notes">Notes (optional)</label>
          <input id="p-notes" className="input" value={notes} onChange={(e) => setNotes(e.target.value)} />
        </div>
      </div>
      <div className="field">
        <span className="label">Reference photo</span>
        {photo.inspection ? (
          <>
            <FacePicker
              inspection={photo.inspection}
              selected={photo.selected}
              onSelect={photo.setSelected}
            />
            <button className="btn btn-sm" style={{ alignSelf: 'flex-start' }} onClick={photo.choose}>
              <ImagePlus size={15} /> Choose a different photo
            </button>
          </>
        ) : (
          <button className="drop-zone" onClick={photo.choose} disabled={photo.busy}>
            {photo.busy ? <Loader2 className="spin" size={22} /> : <ImagePlus size={22} />}
            <strong>{photo.busy ? 'Finding faces…' : 'Choose a photo'}</strong>
            <span className="muted small">JPEG, PNG, WebP, TIFF or BMP. It stays on this computer.</span>
          </button>
        )}
      </div>
    </Modal>
  );
}

function PersonDialog({ person, onClose }: { person: Person; onClose: () => void }) {
  const backend = useBackend();
  const { refreshPeople, toast } = useApp();
  const [name, setName] = useState(person.name);
  const [group, setGroup] = useState(person.group);
  const [notes, setNotes] = useState(person.notes);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const photo = usePhotoInspection();

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      await refreshPeople();
      if (ok) toast(ok, 'success');
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const dirty = name !== person.name || group !== person.group || notes !== person.notes;

  return (
    <Modal
      title={person.name}
      subtitle={person.group || 'No group'}
      onClose={onClose}
      size="medium"
      footer={
        <>
          {confirmDelete ? (
            <button
              className="btn btn-danger"
              onClick={() =>
                run(() => backend.deletePerson(person.id), `${person.name} was removed from the roster`).then(
                  onClose,
                )
              }
            >
              <Trash2 size={16} /> Yes, remove {person.name}
            </button>
          ) : (
            <button className="btn btn-ghost" onClick={() => setConfirmDelete(true)}>
              <Trash2 size={16} /> Remove from roster
            </button>
          )}
          <span className="spacer" />
          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button
            className="btn btn-primary"
            disabled={!dirty || !name.trim()}
            onClick={() => run(() => backend.updatePerson(person.id, name, group, notes), 'Saved')}
          >
            Save changes
          </button>
        </>
      }
    >
      <div className="row" style={{ alignItems: 'flex-start' }}>
        <div className="field" style={{ flex: 2 }}>
          <label htmlFor="e-name">Name</label>
          <input id="e-name" className="input" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="field" style={{ flex: 1 }}>
          <label htmlFor="e-group">Group</label>
          <input id="e-group" className="input" value={group} onChange={(e) => setGroup(e.target.value)} />
        </div>
      </div>
      <div className="field">
        <label htmlFor="e-notes">Notes</label>
        <input id="e-notes" className="input" value={notes} onChange={(e) => setNotes(e.target.value)} />
      </div>
      <div className="field">
        <span className="label">Reference photos</span>
        <span className="hint">
          More than one photo (different days, with and without glasses) improves matching.
        </span>
        <div className="ref-strip">
          {person.references.map((r) => (
            <div key={r.id} className="ref-thumb">
              <img src={backend.mediaUrl('ref', r.id)} alt={r.sourceName} />
              <button
                className="btn btn-sm btn-icon remove"
                title="Remove this photo"
                onClick={() => run(() => backend.deleteReference(r.id))}
              >
                <Trash2 size={14} />
              </button>
              <span className="small subtle truncate">{r.sourceName}</span>
            </div>
          ))}
          {!photo.inspection && (
            <button className="ref-add" onClick={photo.choose} disabled={photo.busy}>
              {photo.busy ? <Loader2 className="spin" size={20} /> : <ImagePlus size={20} />}
              <span className="small">Add photo</span>
            </button>
          )}
        </div>
        {photo.inspection && (
          <div className="card card-pad col" style={{ boxShadow: 'none' }}>
            <FacePicker
              inspection={photo.inspection}
              selected={photo.selected}
              onSelect={photo.setSelected}
            />
            <div className="row">
              <button className="btn btn-sm" onClick={photo.reset}>
                Cancel
              </button>
              <button
                className="btn btn-sm btn-primary"
                disabled={photo.selected === null}
                onClick={() =>
                  run(async () => {
                    await backend.addReference(person.id, photo.path!, photo.selected);
                    photo.reset();
                  }, 'Photo added')
                }
              >
                Add this face
              </button>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}

function ImportReportDialog({ report, onClose }: { report: ImportReport; onClose: () => void }) {
  const attention = report.entries.filter((e) => e.status !== 'added');
  return (
    <Modal
      title="Roster import finished"
      subtitle={`${plural(report.peopleCreated, 'person', 'people')} added · ${plural(report.photosAdded, 'reference photo')} stored`}
      onClose={onClose}
      icon={<CheckCircle2 size={22} style={{ color: 'var(--success)' }} />}
      footer={
        <button className="btn btn-primary" onClick={onClose} data-autofocus>
          Done
        </button>
      }
    >
      {attention.length > 0 ? (
        <>
          <Callout tone="warning" icon={<AlertTriangle size={18} />}>
            {plural(attention.length, 'photo')} could not be added automatically. Add{' '}
            {attention.length === 1 ? 'it' : 'them'} by hand with <strong>Add person</strong> so nobody is
            missed.
          </Callout>
          <div className="list">
            {attention.map((e) => (
              <div key={e.path} className="list-row">
                <XCircle size={16} style={{ color: 'var(--warning)', flexShrink: 0 }} />
                <div className="col" style={{ gap: 0, minWidth: 0 }}>
                  <strong className="truncate">{e.file}</strong>
                  <span className="muted small">
                    {e.status === 'failed' ? e.error : e.status === 'needsAttention' ? e.reason : ''}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </>
      ) : (
        <Callout tone="success" icon={<CheckCircle2 size={18} />}>
          Every photo was added.
        </Callout>
      )}
    </Modal>
  );
}
