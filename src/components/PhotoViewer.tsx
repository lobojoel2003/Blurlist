import { Eye, EyeOff, Loader2, PenSquare, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';

import { basename, dirname } from '../lib/format';
import { errorMessage, useApp } from '../lib/store';
import type { BBox, FaceWithPerson, PhotoDetail } from '../lib/types';
import { useBackend } from './backend';
import { Modal } from './ui';

/** Matches the export's blur region (see blur.rs). */
const EXPAND_X = 1.6;
const EXPAND_Y = 1.75;

function faceLabel(f: FaceWithPerson): string {
  switch (f.category) {
    case 'match':
      return f.personName ?? 'Roster match';
    case 'uncertain':
      return f.personName ? `Maybe ${f.personName}` : 'Possible match';
    case 'small':
      return 'Too small to identify';
    case 'manual':
      return 'Hand-drawn';
    default:
      return 'Not on the roster';
  }
}

/** The whole photo with every face outlined; click faces to change them, drag to add a region. */
export function PhotoViewer({
  photoId,
  focusFaceId,
  onClose,
  onChanged,
}: {
  photoId: number;
  focusFaceId?: number;
  onClose: () => void;
  onChanged: () => void;
}) {
  const backend = useBackend();
  const { toast } = useApp();
  const [detail, setDetail] = useState<PhotoDetail | null>(null);
  const [preview, setPreview] = useState(false);
  const [drawing, setDrawing] = useState(false);
  const [draft, setDraft] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const [loaded, setLoaded] = useState(false);
  const stage = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    try {
      setDetail(await backend.photoDetail(photoId));
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  }, [backend, photoId, toast]);

  useEffect(() => {
    let alive = true;
    backend.photoDetail(photoId).then(
      (d) => alive && setDetail(d),
      (e) => toast(errorMessage(e), 'error'),
    );
    return () => {
      alive = false;
    };
  }, [backend, photoId, toast]);

  const toggle = async (f: FaceWithPerson) => {
    if (f.category === 'manual') return;
    const next = f.decision === 'blur' ? 'keep' : 'blur';
    try {
      await backend.setDecision(f.id, next);
      await load();
      onChanged();
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const removeManual = async (f: FaceWithPerson) => {
    try {
      await backend.deleteManualRegion(f.id);
      await load();
      onChanged();
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const rel = (e: MouseEvent) => {
    const r = stage.current!.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)),
      y: Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)),
    };
  };

  const finishDraw = async () => {
    if (!draft || !detail) return;
    const { width: W, height: H } = detail.photo;
    const box: BBox = {
      x: Math.min(draft.x0, draft.x1) * W,
      y: Math.min(draft.y0, draft.y1) * H,
      w: Math.abs(draft.x1 - draft.x0) * W,
      h: Math.abs(draft.y1 - draft.y0) * H,
    };
    setDraft(null);
    if (box.w < 6 || box.h < 6) return;
    try {
      // Like detected faces, the blur extends a little beyond the drawn box.
      await backend.addManualRegion(photoId, box);
      await load();
      onChanged();
      toast('Blur region added', 'success');
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const photo = detail?.photo;
  const pct = (v: number, total: number) => `${(v / total) * 100}%`;

  return (
    <Modal
      size="wide"
      title={photo ? basename(photo.relPath) : 'Photo'}
      subtitle={photo ? dirname(photo.relPath) || 'Event folder' : undefined}
      onClose={onClose}
      footer={
        <div className="row" style={{ width: '100%' }}>
          <div className="legend">
            <span>
              <i style={{ background: 'var(--blur)' }} /> Will be blurred
            </span>
            <span>
              <i style={{ background: 'var(--keep)' }} /> Left as is
            </span>
            <span>
              <i style={{ background: 'var(--pending)' }} /> Waiting for you
            </span>
          </div>
          <span className="spacer" />
          <span className="muted small">Click a face to switch between blur and keep.</span>
          <button
            className={`btn btn-sm ${preview ? 'btn-primary' : ''}`}
            onClick={() => setPreview((p) => !p)}
          >
            {preview ? <EyeOff size={15} /> : <Eye size={15} />} Preview blur
          </button>
          <button
            className={`btn btn-sm ${drawing ? 'btn-primary' : ''}`}
            onClick={() => setDrawing((d) => !d)}
          >
            <PenSquare size={15} /> {drawing ? 'Drag over the area…' : 'Add blur region'}
          </button>
        </div>
      }
    >
      <div className="viewer">
        {!photo ? (
          <Loader2 className="spin" />
        ) : (
          <div
            ref={stage}
            className={`viewer-stage ${drawing ? 'drawing' : ''}`}
            onMouseDown={(e) => {
              if (!drawing) return;
              const p = rel(e);
              setDraft({ x0: p.x, y0: p.y, x1: p.x, y1: p.y });
            }}
            onMouseMove={(e) => {
              if (!draft) return;
              const p = rel(e);
              setDraft({ ...draft, x1: p.x, y1: p.y });
            }}
            onMouseUp={() => void finishDraw()}
          >
            <img
              src={backend.mediaUrl('photo', photo.id)}
              alt={photo.relPath}
              draggable={false}
              onLoad={() => setLoaded(true)}
            />
            {!loaded && (
              <div className="viewer-loading">
                <Loader2 className="spin" />
              </div>
            )}
            {detail.faces.map((f) => {
              const b = f.bbox;
              const ex = { w: b.w * EXPAND_X, h: b.h * EXPAND_Y };
              const cx = b.x + b.w / 2;
              const cy = b.y + b.h / 2;
              return (
                <div key={f.id}>
                  {preview && f.decision === 'blur' && (
                    <div
                      className="blur-preview"
                      style={{
                        left: pct(cx - ex.w / 2, photo.width),
                        top: pct(cy - ex.h / 2, photo.height),
                        width: pct(ex.w, photo.width),
                        height: pct(ex.h, photo.height),
                      }}
                    />
                  )}
                  <button
                    className={`face-box ${f.decision} ${f.id === focusFaceId ? 'focus' : ''} ${preview ? 'quiet' : ''}`}
                    style={{
                      left: pct(b.x, photo.width),
                      top: pct(b.y, photo.height),
                      width: pct(b.w, photo.width),
                      height: pct(b.h, photo.height),
                    }}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!drawing) void toggle(f);
                    }}
                    title={faceLabel(f)}
                  >
                    {!preview && (
                      <span className="face-tag">
                        {faceLabel(f)}
                        {f.category === 'manual' && (
                          <Trash2
                            size={12}
                            onClick={(e) => {
                              e.stopPropagation();
                              void removeManual(f);
                            }}
                          />
                        )}
                      </span>
                    )}
                  </button>
                </div>
              );
            })}
            {draft && (
              <div
                className="draft-box"
                style={{
                  left: `${Math.min(draft.x0, draft.x1) * 100}%`,
                  top: `${Math.min(draft.y0, draft.y1) * 100}%`,
                  width: `${Math.abs(draft.x1 - draft.x0) * 100}%`,
                  height: `${Math.abs(draft.y1 - draft.y0) * 100}%`,
                }}
              />
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
