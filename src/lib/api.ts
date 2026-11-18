// One typed interface to the backend. Inside the desktop app it calls the
// Rust commands; in a plain browser (development, screenshots, tests) it
// falls back to an in-memory demo with illustrated people.

import type {
  AppInfo,
  BBox,
  CountProgress,
  Decision,
  ExportProgress,
  ExportReport,
  FaceRecord,
  ImportReport,
  Inspection,
  MediaKind,
  ModelFamily,
  ModelInfo,
  Person,
  PhotoDetail,
  RematchReport,
  ReviewItem,
  ReviewPile,
  ScanProgress,
  ScanSummary,
  Settings,
} from './types';

export interface Events {
  'scan-progress': ScanProgress;
  'export-progress': ExportProgress;
  'import-progress': CountProgress;
}

export type Unlisten = () => void;

export interface Backend {
  readonly isDemo: boolean;
  appInfo(): Promise<AppInfo>;
  listModels(): Promise<ModelInfo[]>;
  importModel(path: string, name: string, family: ModelFamily, license: string): Promise<ModelInfo[]>;
  getSettings(): Promise<Settings>;
  saveSettings(settings: Settings): Promise<Settings>;
  revealPath(path: string): Promise<void>;
  openExternal(url: string): Promise<void>;
  cancelTask(): Promise<void>;

  listPeople(): Promise<Person[]>;
  createPerson(name: string, group: string, notes: string): Promise<number>;
  updatePerson(id: number, name: string, group: string, notes: string): Promise<void>;
  deletePerson(id: number): Promise<void>;
  deleteReference(id: number): Promise<void>;
  inspectPhoto(path: string): Promise<Inspection>;
  addReference(personId: number, path: string, faceIndex: number | null): Promise<number>;
  importRoster(folder: string): Promise<ImportReport>;

  startScan(name: string, folder: string): Promise<number>;
  listScans(): Promise<ScanSummary[]>;
  getScan(id: number): Promise<ScanSummary>;
  deleteScan(id: number): Promise<void>;
  rematch(scanId: number): Promise<RematchReport>;

  reviewItems(scanId: number, pile: ReviewPile): Promise<ReviewItem[]>;
  setDecision(faceId: number, decision: Decision): Promise<FaceRecord>;
  restoreDecision(faceId: number, decision: Decision, reviewed: boolean): Promise<FaceRecord>;
  bulkDecision(scanId: number, pile: ReviewPile, onlyPending: boolean, decision: Decision): Promise<number>;
  photoDetail(photoId: number): Promise<PhotoDetail>;
  addManualRegion(photoId: number, bbox: BBox): Promise<number>;
  deleteManualRegion(faceId: number): Promise<void>;

  exportScan(scanId: number, destination: string): Promise<ExportReport>;

  mediaUrl(kind: MediaKind, id: number): string;
  pickFolder(title: string): Promise<string | null>;
  pickImage(title: string): Promise<string | null>;
  pickModel(): Promise<string | null>;
  listen<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void): Promise<Unlisten>;
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function createTauriBackend(): Promise<Backend> {
  const [{ invoke, convertFileSrc }, { listen }, dialog] = await Promise.all([
    import('@tauri-apps/api/core'),
    import('@tauri-apps/api/event'),
    import('@tauri-apps/plugin-dialog'),
  ]);
  const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);
  const one = (r: string | string[] | null) => (Array.isArray(r) ? (r[0] ?? null) : r);
  return {
    isDemo: false,
    appInfo: () => call('app_info'),
    listModels: () => call('list_models'),
    importModel: (path, name, family, license) => call('import_model', { path, name, family, license }),
    getSettings: () => call('get_settings'),
    saveSettings: (settings) => call('save_settings', { settings }),
    revealPath: (path) => call('reveal_path', { path }),
    openExternal: (url) => call('open_external', { url }),
    cancelTask: () => call('cancel_task'),
    listPeople: () => call('list_people'),
    createPerson: (name, group, notes) => call('create_person', { name, group, notes }),
    updatePerson: (id, name, group, notes) => call('update_person', { id, name, group, notes }),
    deletePerson: (id) => call('delete_person', { id }),
    deleteReference: (id) => call('delete_reference', { id }),
    inspectPhoto: (path) => call('inspect_photo', { path }),
    addReference: (personId, path, faceIndex) => call('add_reference', { personId, path, faceIndex }),
    importRoster: (folder) => call('import_roster', { folder }),
    startScan: (name, folder) => call('start_scan', { name, folder }),
    listScans: () => call('list_scans'),
    getScan: (id) => call('get_scan', { id }),
    deleteScan: (id) => call('delete_scan', { id }),
    rematch: (scanId) => call('rematch', { scanId }),
    reviewItems: (scanId, pile) => call('review_items', { scanId, pile }),
    setDecision: (faceId, decision) => call('set_decision', { faceId, decision }),
    restoreDecision: (faceId, decision, reviewed) => call('restore_decision', { faceId, decision, reviewed }),
    bulkDecision: (scanId, pile, onlyPending, decision) =>
      call('bulk_decision', { scanId, pile, onlyPending, decision }),
    photoDetail: (photoId) => call('photo_detail', { photoId }),
    addManualRegion: (photoId, bbox) => call('add_manual_region', { photoId, bbox }),
    deleteManualRegion: (faceId) => call('delete_manual_region', { faceId }),
    exportScan: (scanId, destination) => call('export_scan', { scanId, destination }),
    mediaUrl: (kind, id) => convertFileSrc(`${kind}/${id}`, 'media'),
    pickFolder: async (title) => one(await dialog.open({ directory: true, multiple: false, title })),
    pickImage: async (title) =>
      one(
        await dialog.open({
          multiple: false,
          title,
          filters: [{ name: 'Photos', extensions: ['jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff', 'bmp'] }],
        }),
      ),
    pickModel: async () =>
      one(
        await dialog.open({
          multiple: false,
          title: 'Choose an ONNX model',
          filters: [{ name: 'ONNX model', extensions: ['onnx'] }],
        }),
      ),
    listen: async (event, cb) => listen(event, (e) => cb(e.payload as never)),
  };
}

let backendPromise: Promise<Backend> | null = null;

/** The active backend (Tauri in the app, demo in a browser). */
export function getBackend(): Promise<Backend> {
  if (!backendPromise) {
    backendPromise = isTauri()
      ? createTauriBackend()
      : import('./demo/backend').then((m) => m.createDemoBackend());
  }
  return backendPromise;
}

/** For tests: replace the backend. */
export function setBackend(b: Backend): void {
  backendPromise = Promise.resolve(b);
}
