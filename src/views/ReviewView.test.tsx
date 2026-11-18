import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { BackendProvider } from '../components/backend';
import { setBackend } from '../lib/api';
import { createDemoBackend } from '../lib/demo/backend';
import { useApp } from '../lib/store';
import { ReviewView } from './ReviewView';

describe('ReviewView', () => {
  let backend: ReturnType<typeof createDemoBackend>;

  beforeEach(async () => {
    backend = createDemoBackend({ latency: 0 });
    setBackend(backend);
    useApp.setState({ scans: [], people: [], currentScanId: null, toasts: [] });
    await act(() => useApp.getState().init());
  });

  it('shows a match side by side with the roster photo and accepts it with one key', async () => {
    render(
      <BackendProvider value={backend}>
        <ReviewView />
      </BackendProvider>,
    );
    await screen.findByText(/in the event photo/i);
    expect(screen.getByText(/on the opt-out roster/i)).toBeInTheDocument();
    const before = (await backend.reviewItems(useApp.getState().currentScanId!, 'matches')).filter(
      (i) => i.face.reviewed,
    ).length;

    fireEvent.keyDown(window, { key: 'y' });

    await waitFor(async () => {
      const after = (await backend.reviewItems(useApp.getState().currentScanId!, 'matches')).filter(
        (i) => i.face.reviewed,
      ).length;
      expect(after).toBe(before + 1);
    });
  });

  it('rejects with N and can undo', async () => {
    render(
      <BackendProvider value={backend}>
        <ReviewView />
      </BackendProvider>,
    );
    await screen.findByText(/in the event photo/i);
    const scanId = useApp.getState().currentScanId!;
    const keptBefore = (await backend.reviewItems(scanId, 'matches')).filter(
      (i) => i.face.decision === 'keep',
    ).length;

    fireEvent.keyDown(window, { key: 'n' });
    await waitFor(async () => {
      const kept = (await backend.reviewItems(scanId, 'matches')).filter(
        (i) => i.face.decision === 'keep',
      ).length;
      expect(kept).toBe(keptBefore + 1);
    });

    fireEvent.keyDown(window, { key: 'u' });
    await waitFor(async () => {
      const kept = (await backend.reviewItems(scanId, 'matches')).filter(
        (i) => i.face.decision === 'keep',
      ).length;
      expect(kept).toBe(keptBefore);
    });
  });

  it('switches to the "Check these" pile', async () => {
    render(
      <BackendProvider value={backend}>
        <ReviewView />
      </BackendProvider>,
    );
    fireEvent.click(await screen.findByRole('tab', { name: /check these/i }));
    expect(await screen.findByText(/held back until you decide/i)).toBeInTheDocument();
  });
});
