import { describe, expect, it } from 'vitest';

import {
  clampIndex,
  firstUnreviewed,
  keyToAction,
  nextUnreviewed,
  reviewStats,
  withDecision,
} from './review';
import type { Decision, ReviewItem } from './types';

function item(id: number, decision: Decision, reviewed: boolean): ReviewItem {
  return {
    face: {
      id,
      photoId: 1,
      bbox: { x: 0, y: 0, w: 10, h: 10 },
      landmarks: null,
      detScore: 0.9,
      category: 'match',
      pile: 'matches',
      personId: 1,
      similarity: 0.6,
      decision,
      reviewed,
    },
    photo: {
      id: 1,
      scanId: 1,
      relPath: 'a.jpg',
      width: 100,
      height: 100,
      status: 'ok',
      error: null,
      fileSize: 1,
      modified: 1,
    },
    person: null,
  };
}

const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...mods,
});

describe('keyboard shortcuts', () => {
  it('maps one key each to accept and reject', () => {
    expect(keyToAction(key('y'))).toBe('blur');
    expect(keyToAction(key('Y'))).toBe('blur');
    expect(keyToAction(key('n'))).toBe('keep');
    expect(keyToAction(key('ArrowRight'))).toBe('next');
    expect(keyToAction(key('ArrowLeft'))).toBe('prev');
    expect(keyToAction(key(' '))).toBe('context');
    expect(keyToAction(key('?'))).toBe('help');
    expect(keyToAction(key('u'))).toBe('undo');
  });

  it('supports Ctrl/Cmd+Z and ignores other modified keys', () => {
    expect(keyToAction(key('z', { ctrlKey: true }))).toBe('undo');
    expect(keyToAction(key('z', { metaKey: true }))).toBe('undo');
    expect(keyToAction(key('y', { ctrlKey: true }))).toBeNull();
    expect(keyToAction(key('q'))).toBeNull();
  });
});

describe('review navigation', () => {
  const items = [
    item(1, 'blur', true),
    item(2, 'blur', false),
    item(3, 'keep', true),
    item(4, 'pending', false),
  ];

  it('finds the next face that still needs attention, wrapping around', () => {
    expect(nextUnreviewed(items, 0)).toBe(1);
    expect(nextUnreviewed(items, 1)).toBe(3);
    expect(nextUnreviewed(items, 3)).toBe(1);
    expect(
      nextUnreviewed(
        items.map((i) => ({ ...i, face: { ...i.face, reviewed: true } })),
        0,
      ),
    ).toBe(-1);
  });

  it('starts at the first unreviewed face', () => {
    expect(firstUnreviewed(items)).toBe(1);
    expect(firstUnreviewed([])).toBe(0);
  });

  it('clamps indexes', () => {
    expect(clampIndex(-1, 3)).toBe(0);
    expect(clampIndex(5, 3)).toBe(2);
    expect(clampIndex(2, 0)).toBe(0);
  });

  it('applies decisions immutably and counts them', () => {
    const next = withDecision(items, 4, 'blur');
    expect(items[3]!.face.decision).toBe('pending');
    expect(next[3]!.face).toMatchObject({ decision: 'blur', reviewed: true });
    expect(withDecision(next, 4, 'pending')[3]!.face.reviewed).toBe(false);
    expect(reviewStats(next)).toEqual({ total: 4, reviewed: 3, pending: 0, blur: 3, keep: 1 });
  });
});
