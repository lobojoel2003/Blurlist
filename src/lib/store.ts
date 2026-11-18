import { create } from 'zustand';

import { getBackend } from './api';
import type { AppInfo, Person, ScanSummary, Settings } from './types';

export type View = 'roster' | 'scan' | 'review' | 'export' | 'settings';

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'error';
  message: string;
}

interface AppState {
  view: View;
  info: AppInfo | null;
  settings: Settings | null;
  people: Person[];
  scans: ScanSummary[];
  currentScanId: number | null;
  toasts: Toast[];
  loaded: boolean;

  setView: (v: View) => void;
  selectScan: (id: number | null) => void;
  refreshInfo: () => Promise<void>;
  refreshPeople: () => Promise<void>;
  refreshScans: () => Promise<void>;
  refreshSettings: () => Promise<void>;
  saveSettings: (s: Settings) => Promise<void>;
  init: () => Promise<void>;
  toast: (message: string, kind?: Toast['kind']) => void;
  dismissToast: (id: number) => void;
}

let toastId = 1;

export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return 'Something went wrong.';
}

export const useApp = create<AppState>((set, get) => ({
  view: 'roster',
  info: null,
  settings: null,
  people: [],
  scans: [],
  currentScanId: null,
  toasts: [],
  loaded: false,

  setView: (view) => set({ view }),
  selectScan: (currentScanId) => set({ currentScanId }),

  refreshInfo: async () => {
    const b = await getBackend();
    set({ info: await b.appInfo() });
  },
  refreshPeople: async () => {
    const b = await getBackend();
    set({ people: await b.listPeople() });
  },
  refreshScans: async () => {
    const b = await getBackend();
    const scans = await b.listScans();
    const cur = get().currentScanId;
    const stillThere = cur !== null && scans.some((s) => s.id === cur);
    set({
      scans,
      currentScanId: stillThere
        ? cur
        : (scans.find((s) => s.status === 'complete')?.id ?? scans[0]?.id ?? null),
    });
  },
  refreshSettings: async () => {
    const b = await getBackend();
    set({ settings: await b.getSettings() });
  },
  saveSettings: async (s) => {
    const b = await getBackend();
    const saved = await b.saveSettings(s);
    set({ settings: saved });
  },
  init: async () => {
    const { refreshPeople, refreshScans, refreshSettings, refreshInfo } = get();
    await Promise.all([refreshPeople(), refreshScans(), refreshSettings()]);
    const { people, scans } = get();
    set({ loaded: true, view: people.length === 0 ? 'roster' : scans.length > 0 ? 'review' : 'scan' });
    // Model loading can take a moment; don't block the first paint on it.
    void refreshInfo().catch((e) => get().toast(errorMessage(e), 'error'));
  },
  toast: (message, kind = 'info') => {
    const id = toastId++;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    setTimeout(() => get().dismissToast(id), kind === 'error' ? 7000 : 3500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export function currentScan(): ScanSummary | null {
  const { scans, currentScanId } = useApp.getState();
  return scans.find((s) => s.id === currentScanId) ?? null;
}
