import type { Thresholds } from './types';

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

export type Strength = 'very-strong' | 'strong' | 'possible' | 'weak';

/** How a similarity score compares with the active thresholds. */
export function matchStrength(similarity: number | null, t: Thresholds): Strength {
  if (similarity === null) return 'weak';
  if (similarity >= t.match + 0.15) return 'very-strong';
  if (similarity >= t.match) return 'strong';
  if (similarity >= t.review) return 'possible';
  return 'weak';
}

export const STRENGTH_LABEL: Record<Strength, string> = {
  'very-strong': 'Very strong match',
  strong: 'Strong match',
  possible: 'Possible match',
  weak: 'Weak resemblance',
};

/** Position of a similarity on a 0..1 meter, stretched around the thresholds. */
export function meterPosition(similarity: number | null): number {
  if (similarity === null) return 0;
  return Math.max(0, Math.min(1, similarity));
}

export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

export function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function dirname(relPath: string): string {
  const parts = relPath.split('/');
  parts.pop();
  return parts.join('/');
}

export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${s % 60}s`;
}

export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toUpperCase() ?? '')
    .join('');
}
