// Mirrors of the Rust types serialised by the Tauri commands (camelCase).

export type FaceCategory = 'match' | 'uncertain' | 'small' | 'none' | 'manual';
export type Decision = 'pending' | 'blur' | 'keep';
export type Pile = 'matches' | 'check' | 'none';
export type ReviewPile = Exclude<Pile, 'none'>;
export type SmallFacePolicy = 'review' | 'blur' | 'ignore';
export type Thoroughness = 'fast' | 'thorough' | 'maximum';
export type BlurStyle = 'blur' | 'pixelate' | 'solid';
export type ModelFamily = 'yunet' | 'scrfd' | 'sface' | 'arcface';
export type ModelKind = 'detector' | 'embedder';
export type ScanStatus = 'running' | 'complete' | 'cancelled' | 'failed';
export type PhotoStatus = 'ok' | 'unsupported' | 'error';

export interface BBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Thresholds {
  review: number;
  match: number;
}

export interface Settings {
  detectorId: string;
  embedderId: string;
  thresholds: Thresholds | null;
  minFacePx: number;
  smallFacePolicy: SmallFacePolicy;
  thoroughness: Thoroughness;
  detectionThreshold: number;
  blurStyle: BlurStyle;
  jpegQuality: number;
}

export interface ModelInfo {
  id: string;
  name: string;
  family: ModelFamily;
  kind: ModelKind;
  file: string;
  license: string;
  licenseUrl: string;
  sourceUrl: string;
  permissive: boolean;
  bundled: boolean;
  path?: string;
}

export interface ReferenceSummary {
  id: number;
  personId: number;
  sourceName: string;
  detScore: number;
  facePx: number;
  createdAt: string;
}

export interface Person {
  id: number;
  name: string;
  group: string;
  notes: string;
  createdAt: string;
  references: ReferenceSummary[];
}

export interface ScanCounts {
  photos: number;
  photosOk: number;
  unsupported: number;
  errors: number;
  faces: number;
  matches: number;
  matchesReviewed: number;
  check: number;
  checkPending: number;
  manual: number;
  toBlur: number;
  pending: number;
  photosHeld: number;
}

export interface ScanSummary {
  id: number;
  name: string;
  inputDir: string;
  createdAt: string;
  finishedAt: string | null;
  status: ScanStatus;
  detectorId: string;
  embedderId: string;
  rosterSize: number;
  counts: ScanCounts;
}

export interface PhotoRecord {
  id: number;
  scanId: number;
  relPath: string;
  width: number;
  height: number;
  status: PhotoStatus;
  error: string | null;
  fileSize: number;
  modified: number;
}

export interface FaceRecord {
  id: number;
  photoId: number;
  bbox: BBox;
  landmarks: Point[] | null;
  detScore: number | null;
  category: FaceCategory;
  pile: Pile;
  personId: number | null;
  similarity: number | null;
  decision: Decision;
  reviewed: boolean;
}

export interface PersonBrief {
  id: number;
  name: string;
  group: string;
  referenceIds: number[];
}

export interface ReviewItem {
  face: FaceRecord;
  photo: PhotoRecord;
  person: PersonBrief | null;
}

export interface FaceWithPerson extends FaceRecord {
  personName: string | null;
}

export interface PhotoDetail {
  photo: PhotoRecord;
  faces: FaceWithPerson[];
}

export interface ScanProgress {
  scanId: number;
  phase: 'preparing' | 'scanning' | 'finished';
  total: number;
  done: number;
  faces: number;
  matches: number;
  check: number;
  current: string | null;
}

export interface CountProgress {
  done: number;
  total: number;
}

export interface ExportProgress {
  total: number;
  done: number;
  current: string | null;
}

export interface SkippedPhoto {
  relPath: string;
  reason: string;
}

export interface ExportReport {
  outputDir: string;
  exported: number;
  photosBlurred: number;
  facesBlurred: number;
  heldBack: SkippedPhoto[];
  failed: SkippedPhoto[];
  notSupported: string[];
  durationMs: number;
}

export type ImportEntry = {
  file: string;
  path: string;
  name: string;
  group: string;
} & (
  | { status: 'added'; personId: number; created: boolean }
  | { status: 'needsAttention'; reason: string }
  | { status: 'failed'; error: string }
);

export interface ImportReport {
  peopleCreated: number;
  photosAdded: number;
  entries: ImportEntry[];
}

export interface Candidate {
  index: number;
  bbox: BBox;
  score: number;
  facePx: number;
  warnings: string[];
  preview: string;
}

export interface Inspection {
  width: number;
  height: number;
  suggested: number | null;
  faces: Candidate[];
}

export interface EngineInfo {
  detector: ModelInfo;
  embedder: ModelInfo;
  defaultThresholds: Thresholds;
}

export interface AppInfo {
  version: string;
  buildNumber: string;
  gitSha: string;
  buildDate: string;
  dataDir: string;
  userModelsDir: string;
  engine: EngineInfo | null;
  engineError: string | null;
  engineNotice: string | null;
  busy: string | null;
}

export interface RematchReport {
  facesChecked: number;
  changed: number;
}

export type MediaKind = 'ref' | 'refface' | 'face' | 'thumb' | 'photo';
