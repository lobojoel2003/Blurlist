import { describe, expect, it } from 'vitest';

import { createDemoBackend } from './backend';

describe('demo backend', () => {
  it('holds back photos until uncertain faces are resolved', async () => {
    const b = createDemoBackend({ latency: 0 });
    const scans = await b.listScans();
    const scan = scans.find((s) => s.name.startsWith('Spring'))!;
    expect(scan.counts.checkPending).toBeGreaterThan(0);
    expect(scan.counts.photosHeld).toBeGreaterThan(0);

    const check = await b.reviewItems(scan.id, 'check');
    expect(check.every((i) => i.face.pile === 'check')).toBe(true);
    const n = await b.bulkDecision(scan.id, 'check', true, 'keep');
    expect(n).toBe(scan.counts.checkPending);

    const after = await b.getScan(scan.id);
    expect(after.counts.photosHeld).toBe(0);
    const report = await b.exportScan(scan.id, '/tmp/out');
    expect(report.heldBack).toHaveLength(0);
    expect(report.exported).toBe(after.counts.photosOk);
    expect(report.notSupported.length).toBe(after.counts.unsupported);
  });

  it('serves illustrated media for every face', async () => {
    const b = createDemoBackend({ latency: 0 });
    const [scan] = await b.listScans();
    const items = await b.reviewItems(scan!.id, 'matches');
    expect(items.length).toBeGreaterThan(0);
    expect(b.mediaUrl('face', items[0]!.face.id)).toMatch(/^data:image\/svg\+xml/);
    expect(b.mediaUrl('refface', items[0]!.person!.referenceIds[0]!)).toMatch(/^data:image\/svg\+xml/);
  });

  it('reports scan progress events', async () => {
    const b = createDemoBackend({ latency: 0 });
    const seen: string[] = [];
    const un = await b.listen('scan-progress', (p) => seen.push(p.phase));
    const id = await b.startScan('Test event', '/photos/test');
    un();
    expect(seen[0]).toBe('preparing');
    expect(seen.at(-1)).toBe('finished');
    expect((await b.getScan(id)).name).toBe('Test event');
  });
});
