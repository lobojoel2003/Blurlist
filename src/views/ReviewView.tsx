import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Expand,
  EyeOff,
  Keyboard,
  ListChecks,
  RefreshCcw,
  ScanFace,
  ShieldCheck,
  Undo2,
  UserRound,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { PhotoViewer } from '../components/PhotoViewer';
import { ScanPicker } from '../components/ScanPicker';
import { useBackend } from '../components/backend';
import { Callout, EmptyState, Modal, Ring } from '../components/ui';
import { basename, dirname, firstName, matchStrength, plural, STRENGTH_LABEL } from '../lib/format';
import {
  clampIndex,
  firstUnreviewed,
  keyToAction,
  nextUnreviewed,
  reviewStats,
  withDecision,
  type UndoEntry,
} from '../lib/review';
import { errorMessage, useApp } from '../lib/store';
import type { Decision, ReviewItem, ReviewPile, Thresholds } from '../lib/types';

const FALLBACK_THRESHOLDS: Thresholds = { review: 0.28, match: 0.42 };

export function ReviewView() {
  const backend = useBackend();
  const { scans, currentScanId, refreshScans, setView, toast, people } = useApp();
  const scan = scans.find((s) => s.id === currentScanId) ?? null;
  const [chosenPile, setPile] = useState<ReviewPile | null>(null);
  const [rematching, setRematching] = useState(false);
  const [generation, setGeneration] = useState(0);

  if (!scan) {
    return (
      <div className="page">
        <ReviewHeader />
        <div className="card">
          <EmptyState
            icon={<ScanFace size={30} />}
            title="Scan an event first"
            actions={
              <button className="btn btn-primary" onClick={() => setView('scan')}>
                Go to Scan
              </button>
            }
          >
            After a scan, every roster match and every uncertain face shows up here for a quick yes or no.
          </EmptyState>
        </div>
      </div>
    );
  }

  const c = scan.counts;
  // With no matches at all, start on the "Check these" pile.
  const pile: ReviewPile = chosenPile ?? (c.matches === 0 && c.check > 0 ? 'check' : 'matches');
  const rosterChanged = scan.rosterSize !== people.length;

  const rematch = async () => {
    setRematching(true);
    try {
      const r = await backend.rematch(scan.id);
      toast(
        r.changed > 0
          ? `${plural(r.changed, 'face')} changed and need${r.changed === 1 ? 's' : ''} review`
          : 'Re-matched with the current roster — nothing changed',
        'success',
      );
      await refreshScans();
      setGeneration((g) => g + 1);
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      setRematching(false);
    }
  };

  const tabs = (
    <>
      <div className="segmented" role="tablist">
        <button
          role="tab"
          aria-selected={pile === 'matches'}
          className={pile === 'matches' ? 'active' : ''}
          onClick={() => setPile('matches')}
        >
          <ShieldCheck size={15} /> Matches
          <span className="count">
            {c.matchesReviewed}/{c.matches}
          </span>
        </button>
        <button
          role="tab"
          aria-selected={pile === 'check'}
          className={pile === 'check' ? 'active' : ''}
          onClick={() => setPile('check')}
        >
          <ListChecks size={15} /> Check these
          <span className={`count ${c.checkPending > 0 ? 'alert' : ''}`}>{c.checkPending}</span>
        </button>
      </div>
      <span className="muted small pile-hint">
        {pile === 'matches'
          ? 'Confident roster matches. They will be blurred unless you say otherwise.'
          : 'Uncertain or tiny faces. Their photos are held back until you decide.'}
      </span>
    </>
  );

  return (
    <div className="page review-page">
      <ReviewHeader>
        <ScanPicker />
        <button
          className="btn"
          onClick={rematch}
          disabled={rematching}
          title="Compare this event's faces with the current roster again"
        >
          <RefreshCcw size={15} className={rematching ? 'spin' : ''} /> Re-match
        </button>
      </ReviewHeader>

      {rosterChanged && (
        <Callout tone="warning" icon={<RefreshCcw size={18} />}>
          The roster changed after this event was scanned ({scan.rosterSize} → {people.length} people).{' '}
          <a href="#" onClick={(e) => (e.preventDefault(), void rematch())}>
            Re-match with current roster
          </a>{' '}
          — it takes seconds and keeps your decisions.
        </Callout>
      )}

      <PileReview
        key={`${scan.id}:${pile}:${generation}`}
        scanId={scan.id}
        pile={pile}
        checkPending={c.checkPending}
        tabs={tabs}
        onGoToCheck={() => setPile('check')}
      />
    </div>
  );
}

function PileReview({
  scanId,
  pile,
  checkPending,
  tabs,
  onGoToCheck,
}: {
  scanId: number;
  pile: ReviewPile;
  checkPending: number;
  tabs: React.ReactNode;
  onGoToCheck: () => void;
}) {
  const backend = useBackend();
  const { refreshScans, setView, toast, info, settings } = useApp();
  const [items, setItems] = useState<ReviewItem[] | null>(null);
  const [index, setIndex] = useState(0);
  const [undo, setUndo] = useState<UndoEntry[]>([]);
  const [viewer, setViewer] = useState<ReviewItem | null>(null);
  const [help, setHelp] = useState(false);
  const filmstrip = useRef<HTMLDivElement>(null);
  const thresholds = settings?.thresholds ?? info?.engine?.defaultThresholds ?? FALLBACK_THRESHOLDS;

  useEffect(() => {
    let alive = true;
    backend.reviewItems(scanId, pile).then(
      (list) => {
        if (!alive) return;
        setItems(list);
        setIndex(firstUnreviewed(list));
      },
      (e) => toast(errorMessage(e), 'error'),
    );
    return () => {
      alive = false;
    };
  }, [backend, scanId, pile, toast]);

  const reload = useCallback(async () => {
    try {
      const list = await backend.reviewItems(scanId, pile);
      setItems(list);
      setIndex((i) => clampIndex(i, list.length));
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  }, [backend, scanId, pile, toast]);

  const stats = useMemo(() => reviewStats(items ?? []), [items]);
  const current = items?.[index] ?? null;

  const decide = useCallback(
    async (decision: Decision) => {
      if (!items || !current) return;
      const prev = current.face;
      setUndo((u) => [
        ...u.slice(-49),
        { faceId: prev.id, decision: prev.decision, reviewed: prev.reviewed },
      ]);
      const updated = withDecision(items, prev.id, decision);
      setItems(updated);
      const next = nextUnreviewed(updated, index);
      setIndex(next === -1 ? clampIndex(index + 1, updated.length) : next);
      try {
        await backend.setDecision(prev.id, decision);
        void refreshScans();
      } catch (e) {
        toast(errorMessage(e), 'error');
        void reload();
      }
    },
    [items, current, index, backend, refreshScans, toast, reload],
  );

  const undoLast = useCallback(async () => {
    const last = undo[undo.length - 1];
    if (!last || !items) return;
    setUndo((u) => u.slice(0, -1));
    setItems(withDecision(items, last.faceId, last.decision, last.reviewed));
    const i = items.findIndex((it) => it.face.id === last.faceId);
    if (i >= 0) setIndex(i);
    try {
      await backend.restoreDecision(last.faceId, last.decision, last.reviewed);
      void refreshScans();
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  }, [undo, items, backend, refreshScans, toast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (viewer || help) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) return;
      const action = keyToAction(e);
      if (!action || !items || items.length === 0) return;
      e.preventDefault();
      switch (action) {
        case 'blur':
          void decide('blur');
          break;
        case 'keep':
          void decide('keep');
          break;
        case 'next':
          setIndex((i) => clampIndex(i + 1, items.length));
          break;
        case 'prev':
          setIndex((i) => clampIndex(i - 1, items.length));
          break;
        case 'undo':
          void undoLast();
          break;
        case 'context':
          if (current) setViewer(current);
          break;
        case 'help':
          setHelp(true);
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [items, current, decide, undoLast, viewer, help]);

  useEffect(() => {
    filmstrip.current
      ?.querySelector<HTMLElement>(`[data-index="${index}"]`)
      ?.scrollIntoView?.({ block: 'nearest', inline: 'center', behavior: 'smooth' });
  }, [index]);

  const bulk = async (decision: Decision) => {
    try {
      const n = await backend.bulkDecision(scanId, pile, true, decision);
      toast(`${plural(n, 'face')} set to ${decision === 'blur' ? 'blur' : 'leave as is'}`, 'success');
      await Promise.all([reload(), refreshScans()]);
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const confirmAllMatches = async () => {
    if (!items) return;
    const unconfirmed = items.filter((i) => !i.face.reviewed);
    try {
      for (const it of unconfirmed) await backend.setDecision(it.face.id, 'blur');
      toast(`${plural(unconfirmed.length, 'match', 'matches')} confirmed for blurring`, 'success');
      await Promise.all([reload(), refreshScans()]);
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const done = items !== null && items.length > 0 && stats.reviewed === stats.total;

  return (
    <>
      <div className="review-toolbar">
        {tabs}
        <span className="spacer" />
        {items && items.length > 0 && (
          <>
            <Ring
              value={stats.reviewed}
              total={stats.total}
              label={`${stats.reviewed} of ${stats.total} reviewed`}
            />
            {pile === 'matches' ? (
              <button
                className="btn btn-sm"
                onClick={confirmAllMatches}
                disabled={stats.reviewed === stats.total}
              >
                <CheckCircle2 size={15} /> Confirm all
              </button>
            ) : (
              <button className="btn btn-sm" onClick={() => bulk('blur')} disabled={stats.pending === 0}>
                <EyeOff size={15} /> Blur all remaining
              </button>
            )}
          </>
        )}
        <button className="btn btn-sm btn-ghost" onClick={() => setHelp(true)} title="Keyboard shortcuts (?)">
          <Keyboard size={16} />
        </button>
      </div>

      {items === null ? (
        <div className="card review-skeleton" />
      ) : items.length === 0 ? (
        <div className="card">
          <EmptyState
            icon={pile === 'matches' ? <ShieldCheck size={30} /> : <CheckCircle2 size={30} />}
            title={pile === 'matches' ? 'No roster matches in this event' : 'Nothing to check'}
            actions={
              <button className="btn btn-primary" onClick={() => setView('export')}>
                Continue to Export <ArrowRight size={16} />
              </button>
            }
          >
            {pile === 'matches'
              ? 'None of the faces looked like anyone on the opt-out roster.'
              : 'Every face was either a confident match or clearly someone else.'}
          </EmptyState>
        </div>
      ) : (
        <>
          {done && (
            <Callout tone="success" icon={<CheckCircle2 size={18} />}>
              <div className="row">
                <span>
                  <strong>All {stats.total} reviewed.</strong>{' '}
                  {pile === 'matches' && checkPending > 0
                    ? `Next, check the ${checkPending} uncertain face${checkPending === 1 ? '' : 's'}.`
                    : 'This pile is done.'}
                </span>
                <span className="spacer" />
                {pile === 'matches' && checkPending > 0 ? (
                  <button className="btn btn-sm" onClick={onGoToCheck}>
                    Check these <ArrowRight size={14} />
                  </button>
                ) : (
                  <button className="btn btn-sm btn-primary" onClick={() => setView('export')}>
                    Continue to Export <ArrowRight size={14} />
                  </button>
                )}
              </div>
            </Callout>
          )}
          {current && (
            <CompareCard
              item={current}
              thresholds={thresholds}
              position={index + 1}
              total={items.length}
              onDecide={decide}
              onPrev={() => setIndex((i) => clampIndex(i - 1, items.length))}
              onNext={() => setIndex((i) => clampIndex(i + 1, items.length))}
              onContext={() => setViewer(current)}
              onUndo={undo.length > 0 ? undoLast : undefined}
            />
          )}
          <div className="filmstrip" ref={filmstrip} aria-label="All faces in this pile">
            {items.map((it, i) => (
              <button
                key={it.face.id}
                data-index={i}
                className={`film ${i === index ? 'current' : ''} ${it.face.reviewed ? 'reviewed' : ''}`}
                onClick={() => setIndex(i)}
                title={`${it.person?.name ?? 'Unknown'} · ${basename(it.photo.relPath)}`}
              >
                <img src={backend.mediaUrl('face', it.face.id)} alt="" loading="lazy" />
                <span className={`dot ${it.face.decision}`} />
              </button>
            ))}
          </div>
        </>
      )}

      {viewer && (
        <PhotoViewer
          photoId={viewer.photo.id}
          focusFaceId={viewer.face.id}
          onClose={() => setViewer(null)}
          onChanged={() => {
            void reload();
            void refreshScans();
          }}
        />
      )}
      {help && <ShortcutsDialog onClose={() => setHelp(false)} />}
    </>
  );
}

function ReviewHeader({ children }: { children?: React.ReactNode }) {
  return (
    <div className="page-header">
      <div className="titles">
        <span className="eyebrow">Step 3</span>
        <h1>Review</h1>
      </div>
      <div className="spacer" />
      {children}
    </div>
  );
}

function decisionLabel(item: ReviewItem): { text: string; tone: string } {
  const f = item.face;
  if (f.decision === 'pending') return { text: 'Waiting for your decision', tone: 'warning' };
  if (f.decision === 'blur')
    return { text: f.reviewed ? 'Will be blurred' : 'Will be blurred · not yet confirmed', tone: 'danger' };
  return { text: 'Left as is', tone: 'success' };
}

function CompareCard({
  item,
  thresholds,
  position,
  total,
  onDecide,
  onPrev,
  onNext,
  onContext,
  onUndo,
}: {
  item: ReviewItem;
  thresholds: Thresholds;
  position: number;
  total: number;
  onDecide: (d: Decision) => void;
  onPrev: () => void;
  onNext: () => void;
  onContext: () => void;
  onUndo?: () => void;
}) {
  const backend = useBackend();
  const { face, photo, person } = item;
  const strength = matchStrength(face.similarity, thresholds);
  const status = decisionLabel(item);
  const who = person ? firstName(person.name) : null;
  const small = face.category === 'small';
  const refs = person?.referenceIds ?? [];
  const meter = Math.max(0, Math.min(1, face.similarity ?? 0));

  return (
    <div className="card compare" aria-live="polite">
      <div className="compare-top">
        <span className="muted small">
          Face {position} of {total}
        </span>
        <span className={`badge ${status.tone}`}>{status.text}</span>
        <span className="spacer" />
        <button
          className="btn btn-sm btn-ghost btn-icon"
          onClick={onPrev}
          disabled={position === 1}
          aria-label="Previous face"
        >
          <ArrowLeft size={16} />
        </button>
        <button
          className="btn btn-sm btn-ghost btn-icon"
          onClick={onNext}
          disabled={position === total}
          aria-label="Next face"
        >
          <ArrowRight size={16} />
        </button>
      </div>

      <div className="compare-faces">
        <figure className="face-panel">
          <div className="face-frame event">
            <img src={backend.mediaUrl('face', face.id)} alt="Face found in the event photo" />
            <button className="frame-action" onClick={onContext} title="Show in the whole photo (Space)">
              <Expand size={15} /> In photo
            </button>
          </div>
          <figcaption>
            <span className="caption-kicker">In the event photo</span>
            <strong className="truncate">{basename(photo.relPath)}</strong>
            <span className="muted small truncate">{dirname(photo.relPath) || 'Event folder'}</span>
          </figcaption>
        </figure>

        <div className="compare-middle">
          {small ? (
            <span className="badge">Too small to identify</span>
          ) : (
            <>
              <div className={`strength ${strength}`}>{STRENGTH_LABEL[strength]}</div>
              <div className="meter" aria-label={`Similarity ${face.similarity?.toFixed(2) ?? 'unknown'}`}>
                <div className="meter-fill" style={{ width: `${meter * 100}%` }} />
                <i
                  className="tick review"
                  style={{ left: `${thresholds.review * 100}%` }}
                  title="Review threshold"
                />
                <i
                  className="tick match"
                  style={{ left: `${thresholds.match * 100}%` }}
                  title="Match threshold"
                />
              </div>
              <span className="subtle small mono">similarity {face.similarity?.toFixed(2) ?? '—'}</span>
            </>
          )}
        </div>

        <figure className="face-panel">
          <div className={`face-frame roster ${person ? '' : 'none'}`}>
            {person && refs[0] !== undefined ? (
              <img src={backend.mediaUrl('refface', refs[0])} alt={`Roster photo of ${person.name}`} />
            ) : (
              <div className="no-candidate">
                <UserRound size={34} />
                <span>{small ? 'This face is too small to compare reliably.' : 'No roster candidate'}</span>
              </div>
            )}
            {refs.length > 1 && (
              <span className="ref-more">
                +{refs.length - 1} photo{refs.length > 2 ? 's' : ''}
              </span>
            )}
          </div>
          <figcaption>
            <span className="caption-kicker">On the opt-out roster</span>
            <strong className="truncate">
              {person?.name ?? (small ? 'Unknown person' : 'Removed from roster')}
            </strong>
            <span className="muted small truncate">
              {person?.group || (person ? 'No group' : 'Decide whether to blur it')}
            </span>
          </figcaption>
        </figure>
      </div>

      <div className="decide">
        <button
          className={`decide-btn blur ${face.decision === 'blur' && face.reviewed ? 'chosen' : ''}`}
          onClick={() => onDecide('blur')}
        >
          <kbd>Y</kbd>
          <span>
            <strong>
              {who
                ? face.category === 'match'
                  ? `Yes, it's ${who} — blur`
                  : `Could be ${who} — blur`
                : 'Blur this face'}
            </strong>
            <small>The face is blurred in the exported copy.</small>
          </span>
        </button>
        <button
          className={`decide-btn keep ${face.decision === 'keep' ? 'chosen' : ''}`}
          onClick={() => onDecide('keep')}
        >
          <kbd>N</kbd>
          <span>
            <strong>{who ? `Not ${who} — leave it` : 'Leave as is'}</strong>
            <small>The face stays visible.</small>
          </span>
        </button>
      </div>
      <div className="compare-foot subtle small">
        <span>
          <kbd>←</kbd> <kbd>→</kbd> move
        </span>
        <span>
          <kbd>Space</kbd> whole photo
        </span>
        <span>
          <kbd>U</kbd> undo
        </span>
        {onUndo && (
          <button className="btn btn-sm btn-ghost" onClick={onUndo}>
            <Undo2 size={14} /> Undo last
          </button>
        )}
      </div>
    </div>
  );
}

function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  const rows: [string, string][] = [
    ['Y  or  B', 'Blur this face (accept the match)'],
    ['N  or  K', 'Leave this face as is (reject the match)'],
    ['→  /  ←', 'Next / previous face'],
    ['Space', 'Show the whole photo'],
    ['U  or  ⌘/Ctrl+Z', 'Undo the last decision'],
    ['?', 'Show these shortcuts'],
  ];
  return (
    <Modal title="Keyboard shortcuts" onClose={onClose} icon={<Keyboard size={22} className="subtle" />}>
      <div className="list">
        {rows.map(([k, d]) => (
          <div key={k} className="list-row">
            <span className="mono" style={{ width: 150, fontWeight: 700 }}>
              {k}
            </span>
            <span>{d}</span>
          </div>
        ))}
      </div>
    </Modal>
  );
}
