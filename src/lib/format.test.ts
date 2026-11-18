import { describe, expect, it } from 'vitest';

import { basename, dirname, firstName, formatDuration, initials, matchStrength, plural } from './format';

describe('format helpers', () => {
  it('pluralises', () => {
    expect(plural(1, 'photo')).toBe('1 photo');
    expect(plural(3, 'photo')).toBe('3 photos');
    expect(plural(2, 'person', 'people')).toBe('2 people');
  });

  it('describes match strength relative to the thresholds', () => {
    const t = { review: 0.3, match: 0.45 };
    expect(matchStrength(0.7, t)).toBe('very-strong');
    expect(matchStrength(0.5, t)).toBe('strong');
    expect(matchStrength(0.35, t)).toBe('possible');
    expect(matchStrength(0.1, t)).toBe('weak');
    expect(matchStrength(null, t)).toBe('weak');
  });

  it('handles paths on every platform', () => {
    expect(basename('/a/b/c.jpg')).toBe('c.jpg');
    expect(basename('C:\\Photos\\Day 1')).toBe('Day 1');
    expect(dirname('day1/sub/x.jpg')).toBe('day1/sub');
    expect(dirname('x.jpg')).toBe('');
  });

  it('formats names and durations', () => {
    expect(firstName('  Ava  Thompson ')).toBe('Ava');
    expect(initials('ava thompson lee')).toBe('AT');
    expect(formatDuration(4200)).toBe('4s');
    expect(formatDuration(125000)).toBe('2m 5s');
  });
});
