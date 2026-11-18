// Pure helpers behind the review screen, kept separate so they are easy to test.

import type { Decision, ReviewItem } from './types';

export interface UndoEntry {
  faceId: number;
  decision: Decision;
  reviewed: boolean;
}

export interface ReviewStats {
  total: number;
  reviewed: number;
  pending: number;
  blur: number;
  keep: number;
}

export function reviewStats(items: ReviewItem[]): ReviewStats {
  let reviewed = 0;
  let pending = 0;
  let blur = 0;
  let keep = 0;
  for (const it of items) {
    if (it.face.reviewed) reviewed++;
    if (it.face.decision === 'pending') pending++;
    if (it.face.decision === 'blur') blur++;
    if (it.face.decision === 'keep') keep++;
  }
  return { total: items.length, reviewed, pending, blur, keep };
}

/** Index of the next item still needing attention after `from`, wrapping around. */
export function nextUnreviewed(items: ReviewItem[], from: number): number {
  const n = items.length;
  for (let step = 1; step <= n; step++) {
    const i = (from + step) % n;
    if (!items[i]!.face.reviewed) return i;
  }
  return -1;
}

/** First item that still needs attention, or 0. */
export function firstUnreviewed(items: ReviewItem[]): number {
  const i = items.findIndex((it) => !it.face.reviewed);
  return i === -1 ? 0 : i;
}

export function clampIndex(i: number, len: number): number {
  if (len === 0) return 0;
  return Math.max(0, Math.min(len - 1, i));
}

/** Apply a decision locally (optimistic update). */
export function withDecision(
  items: ReviewItem[],
  faceId: number,
  decision: Decision,
  reviewed = true,
): ReviewItem[] {
  return items.map((it) =>
    it.face.id === faceId
      ? { ...it, face: { ...it.face, decision, reviewed: decision === 'pending' ? false : reviewed } }
      : it,
  );
}

/** Keyboard shortcuts on the review screen. */
export type ReviewAction = 'blur' | 'keep' | 'next' | 'prev' | 'undo' | 'context' | 'help';

export function keyToAction(
  e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>,
): ReviewAction | null {
  const k = e.key.toLowerCase();
  if ((e.metaKey || e.ctrlKey) && k === 'z') return 'undo';
  if (e.metaKey || e.ctrlKey || e.altKey) return null;
  switch (k) {
    case 'y':
    case 'b':
      return 'blur';
    case 'n':
    case 'k':
      return 'keep';
    case 'arrowright':
    case 'j':
      return 'next';
    case 'arrowleft':
    case 'h':
      return 'prev';
    case 'u':
    case 'backspace':
      return 'undo';
    case ' ':
    case 'p':
      return 'context';
    case '?':
      return 'help';
    default:
      return null;
  }
}
