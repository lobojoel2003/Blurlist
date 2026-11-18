// In-memory demo backend with illustrated people. Used when Blurlist's UI runs
// in a normal browser: development, README screenshots and component tests.

import type { Backend, Events, Unlisten } from '../api';
import type {
  BBox,
  Decision,
  ExportReport,
  FaceCategory,
  FaceRecord,
  ImportReport,
  MediaKind,
  ModelInfo,
  Person,
  Pile,
  PhotoRecord,
  ReviewItem,
  ReviewPile,
  ScanSummary,
  Settings,
} from '../types';
import {
  portraitSvg,
  randomAvatar,
  rng,
  sceneCropSvg,
  sceneFaceBox,
  sceneSvg,
  svgUrl,
  type AvatarParams,
  type Expression,
  type Scene,
  type ScenePerson,
} from './avatars';

interface DemoPerson {
  person: Person;
  avatar: AvatarParams;
}

interface DemoPhoto {
  record: PhotoRecord;
  scene: Scene | null;
}

interface DemoFace {
  record: FaceRecord;
  scene: Scene;
}

interface DemoScan {
  summary: Omit<ScanSummary, 'counts'>;
  photos: DemoPhoto[];
  faces: DemoFace[];
}

const ROSTER: [string, string][] = [
  ['Ava Thompson', 'Maple Cabin'],
  ['Leo Martínez', 'Maple Cabin'],
  ['Maya Chen', 'Birch Cabin'],
  ['Noah Williams', 'Birch Cabin'],
  ['Zara Ahmed', 'Cedar Cabin'],
  ['Eli Johnson', 'Cedar Cabin'],
  ['Sofia Rossi', 'Day campers'],
  ['Kai Nakamura', 'Day campers'],
];

const DEFAULT_SETTINGS: Settings = {
  detectorId: 'yunet-2023mar',
  embedderId: 'sface-2021dec',
  thresholds: null,
  minFacePx: 36,
  smallFacePolicy: 'review',
  thoroughness: 'thorough',
  detectionThreshold: 0.5,
  blurStyle: 'blur',
  jpegQuality: 92,
};

const MODELS: ModelInfo[] = [
  {
    id: 'yunet-2023mar',
    name: 'YuNet face detector (2023mar)',
    family: 'yunet',
    kind: 'detector',
    file: 'face_detection_yunet_2023mar.onnx',
    license: 'MIT',
    licenseUrl: 'https://github.com/opencv/opencv_zoo/blob/main/models/face_detection_yunet/LICENSE',
    sourceUrl: 'https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet',
    permissive: true,
    bundled: true,
    path: '/Applications/Blurlist.app/Contents/Resources/models/face_detection_yunet_2023mar.onnx',
  },
  {
    id: 'sface-2021dec',
    name: 'SFace recogniser (2021dec)',
    family: 'sface',
    kind: 'embedder',
    file: 'face_recognition_sface_2021dec.onnx',
    license: 'Apache-2.0',
    licenseUrl: 'https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/LICENSE',
    sourceUrl: 'https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface',
    permissive: true,
    bundled: true,
    path: '/Applications/Blurlist.app/Contents/Resources/models/face_recognition_sface_2021dec.onnx',
  },
  ...(
    [
      ['scrfd-10g', 'SCRFD-10G detector (InsightFace buffalo_l)', 'scrfd', 'detector', 'det_10g.onnx'],
      ['scrfd-500m', 'SCRFD-500M detector (InsightFace buffalo_sc)', 'scrfd', 'detector', 'det_500m.onnx'],
      [
        'arcface-w600k-r50',
        'ArcFace R50 WebFace600K (InsightFace buffalo_l)',
        'arcface',
        'embedder',
        'w600k_r50.onnx',
      ],
      [
        'arcface-w600k-mbf',
        'ArcFace MobileFaceNet WebFace600K (InsightFace buffalo_sc)',
        'arcface',
        'embedder',
        'w600k_mbf.onnx',
      ],
    ] as const
  ).map(([id, name, family, kind, file]) => ({
    id,
    name,
    family,
    kind,
    file,
    license: 'Non-commercial research only',
    licenseUrl: 'https://github.com/deepinsight/insightface#license',
    sourceUrl: 'https://github.com/deepinsight/insightface/tree/master/model_zoo',
    permissive: false,
    bundled: false,
  })),
];

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => new Date().toISOString();

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

export function createDemoBackend(opts: { latency?: number } = {}): Backend {
  const latency = opts.latency ?? 120;
  let nextId = 1;
  const id = () => nextId++;
  const people: DemoPerson[] = [];
  const refAvatars = new Map<number, AvatarParams>();
  const scans: DemoScan[] = [];
  let settings: Settings = { ...DEFAULT_SETTINGS };
  const listeners = new Map<keyof Events, Set<(p: never) => void>>();
  let cancelled = false;

  const emit = <K extends keyof Events>(event: K, payload: Events[K]) => {
    listeners.get(event)?.forEach((cb) => cb(payload as never));
  };

  function addPerson(name: string, group: string, avatar: AvatarParams, notes = ''): DemoPerson {
    const p: DemoPerson = {
      person: { id: id(), name, group, notes, createdAt: now(), references: [] },
      avatar,
    };
    const rid = id();
    refAvatars.set(rid, avatar);
    p.person.references.push({
      id: rid,
      personId: p.person.id,
      sourceName: `${name}.jpg`,
      detScore: 0.93,
      facePx: 214,
      createdAt: now(),
    });
    people.push(p);
    return p;
  }

  ROSTER.forEach(([name, group], i) => addPerson(name, group, randomAvatar(100 + i)));

  function generateScan(
    name: string,
    folder: string,
    seed: number,
    photoCount: number,
    reviewedShare: number,
  ) {
    const r = rng(seed);
    const scanId = id();
    const folders = ['morning', 'relay-races', 'picnic', 'awards'];
    const scan: DemoScan = {
      summary: {
        id: scanId,
        name,
        inputDir: folder,
        createdAt: now(),
        finishedAt: now(),
        status: 'complete',
        detectorId: settings.detectorId,
        embedderId: settings.embedderId,
        rosterSize: people.length,
      },
      photos: [],
      faces: [],
    };
    const W = 1600;
    const H = 1067;
    for (let i = 0; i < photoCount; i++) {
      const n = 2 + Math.floor(r() * 4);
      const members: { avatar: AvatarParams; rosterId: number | null }[] = [];
      const rosterRoll = r();
      const rosterCount = rosterRoll < 0.12 ? 2 : rosterRoll < 0.55 ? 1 : 0;
      const shuffled = [...people].sort(() => r() - 0.5);
      for (let k = 0; k < rosterCount && k < shuffled.length; k++) {
        members.push({ avatar: shuffled[k]!.avatar, rosterId: shuffled[k]!.person.id });
      }
      while (members.length < n)
        members.push({ avatar: randomAvatar(500 + Math.floor(r() * 40)), rosterId: null });
      members.sort(() => r() - 0.5);

      const scenePeople: (ScenePerson & { rosterId: number | null })[] = [];
      const front = members.slice(0, Math.min(members.length, 3));
      const back = members.slice(3);
      const expressions: Expression[] = ['smile', 'grin', 'open'];
      front.forEach((m, k) => {
        const s = 1.75 + r() * 0.35;
        scenePeople.push({
          params: m.avatar,
          rosterId: m.rosterId,
          scale: s,
          x: (W / (front.length + 1)) * (k + 1) - 100 * s + (r() - 0.5) * 80,
          y: H - 240 * s + 30,
          expression: expressions[Math.floor(r() * 3)]!,
        });
      });
      back.forEach((m, k) => {
        const s = 0.95 + r() * 0.25;
        scenePeople.push({
          params: m.avatar,
          rosterId: m.rosterId,
          scale: s,
          x: (W / (back.length + 1)) * (k + 1) - 100 * s + (r() - 0.5) * 140,
          y: H * 0.5 - 40 * s,
          expression: expressions[Math.floor(r() * 3)]!,
        });
      });
      if (r() < 0.25) {
        scenePeople.push({
          params: randomAvatar(900 + i),
          rosterId: null,
          scale: 0.32,
          x: W * (0.1 + r() * 0.8),
          y: H * 0.47,
          expression: 'smile',
        });
      }
      const scene: Scene = { width: W, height: H, people: scenePeople, palette: i % 4 };
      const photoId = id();
      scan.photos.push({
        record: {
          id: photoId,
          scanId,
          relPath: `${folders[i % folders.length]}/IMG_${2040 + i}.jpg`,
          width: W,
          height: H,
          status: 'ok',
          error: null,
          fileSize: 4_200_000 + Math.floor(r() * 2_000_000),
          modified: 1_700_000_000,
        },
        scene,
      });
      for (const sp of scenePeople) {
        const box = sceneFaceBox(sp);
        let category: FaceCategory = 'none';
        let personId: number | null = null;
        let similarity = 0.05 + r() * 0.18;
        if (Math.min(box.w, box.h) < settings.minFacePx) {
          category = 'small';
        } else if (sp.rosterId !== null) {
          personId = sp.rosterId;
          if (r() < 0.18) {
            category = 'uncertain';
            similarity = 0.3 + r() * 0.1;
          } else {
            category = 'match';
            similarity = 0.48 + r() * 0.34;
          }
        } else if (r() < 0.07) {
          category = 'uncertain';
          personId = people[Math.floor(r() * people.length)]!.person.id;
          similarity = 0.29 + r() * 0.08;
        }
        const pile: Pile = category === 'match' ? 'matches' : category === 'none' ? 'none' : 'check';
        let decision: Decision = category === 'match' ? 'blur' : category === 'none' ? 'keep' : 'pending';
        let reviewed = false;
        if (r() < reviewedShare && pile !== 'none') {
          reviewed = true;
          decision = category === 'match' ? (r() < 0.93 ? 'blur' : 'keep') : r() < 0.6 ? 'blur' : 'keep';
        }
        scan.faces.push({
          record: {
            id: id(),
            photoId,
            bbox: box,
            landmarks: null,
            detScore: 0.82 + r() * 0.15,
            category,
            pile,
            personId,
            similarity: category === 'small' ? null : similarity,
            decision,
            reviewed,
          },
          scene,
        });
      }
    }
    const unsupported = ['videos/relay-finish.MOV', 'awards/IMG_2101.HEIC'];
    for (const relPath of unsupported) {
      scan.photos.push({
        record: {
          id: id(),
          scanId,
          relPath,
          width: 0,
          height: 0,
          status: 'unsupported',
          error: 'this file type cannot be checked by Blurlist',
          fileSize: 0,
          modified: 0,
        },
        scene: null,
      });
    }
    scans.unshift(scan);
    return scan;
  }

  const art = generateScan('Art Show — March', '/Users/organizer/Pictures/Art Show', 77, 14, 1);
  art.summary.createdAt = '2026-03-14T16:02:00Z';
  art.summary.finishedAt = '2026-03-14T16:04:10Z';
  generateScan('Spring Field Day 2026', '/Users/organizer/Pictures/Spring Field Day 2026', 2026, 36, 0.35);

  const findScan = (scanId: number) => {
    const s = scans.find((x) => x.summary.id === scanId);
    if (!s) throw new Error(`scan ${scanId} not found`);
    return s;
  };

  function summarize(s: DemoScan): ScanSummary {
    const f = s.faces.map((x) => x.record);
    const ok = s.photos.filter((p) => p.record.status === 'ok').length;
    const held = new Set(f.filter((x) => x.decision === 'pending').map((x) => x.photoId)).size;
    return {
      ...s.summary,
      counts: {
        photos: s.photos.length,
        photosOk: ok,
        unsupported: s.photos.filter((p) => p.record.status === 'unsupported').length,
        errors: s.photos.filter((p) => p.record.status === 'error').length,
        faces: f.length,
        matches: f.filter((x) => x.pile === 'matches').length,
        matchesReviewed: f.filter((x) => x.pile === 'matches' && x.reviewed).length,
        check: f.filter((x) => x.pile === 'check').length,
        checkPending: f.filter((x) => x.pile === 'check' && x.decision === 'pending').length,
        manual: f.filter((x) => x.category === 'manual').length,
        toBlur: f.filter((x) => x.decision === 'blur').length,
        pending: f.filter((x) => x.decision === 'pending').length,
        photosHeld: held,
      },
    };
  }

  const allFaces = () => scans.flatMap((s) => s.faces);
  const findFace = (faceId: number) => {
    const f = allFaces().find((x) => x.record.id === faceId);
    if (!f) throw new Error(`face ${faceId} not found`);
    return f;
  };
  const findPhoto = (photoId: number) => {
    for (const s of scans) {
      const p = s.photos.find((x) => x.record.id === photoId);
      if (p) return p;
    }
    throw new Error(`photo ${photoId} not found`);
  };

  const wait = () => delay(latency);

  const backend: Backend = {
    isDemo: true,
    async appInfo() {
      await wait();
      return {
        version: __APP_VERSION__,
        buildNumber: 'demo',
        gitSha: 'demo',
        buildDate: new Date().toISOString().slice(0, 10),
        dataDir: '~/Library/Application Support/io.github.lobojoel2003.blurlist',
        userModelsDir: '~/Library/Application Support/io.github.lobojoel2003.blurlist/models',
        engine: {
          detector: MODELS[0]!,
          embedder: MODELS[1]!,
          defaultThresholds: { review: 0.28, match: 0.42 },
        },
        engineError: null,
        engineNotice: null,
        busy: null,
      };
    },
    async listModels() {
      await wait();
      return MODELS;
    },
    async importModel() {
      await wait();
      throw new Error('Importing models is not available in the browser demo.');
    },
    async getSettings() {
      return { ...settings };
    },
    async saveSettings(s) {
      await wait();
      settings = { ...s };
      return { ...settings };
    },
    async revealPath() {},
    async openExternal(url) {
      window.open(url, '_blank', 'noopener');
    },
    async cancelTask() {
      cancelled = true;
    },

    async listPeople() {
      await wait();
      return people
        .map((p) => structuredClone(p.person))
        .sort((a, b) => a.group.localeCompare(b.group) || a.name.localeCompare(b.name));
    },
    async createPerson(name, group, notes) {
      await wait();
      if (!name.trim()) throw new Error('a name is required');
      const p: DemoPerson = {
        person: { id: id(), name: name.trim(), group: group.trim(), notes, createdAt: now(), references: [] },
        avatar: randomAvatar(hash(name)),
      };
      people.push(p);
      return p.person.id;
    },
    async updatePerson(pid, name, group, notes) {
      await wait();
      const p = people.find((x) => x.person.id === pid);
      if (!p) throw new Error('person not found');
      Object.assign(p.person, { name, group, notes });
    },
    async deletePerson(pid) {
      await wait();
      const i = people.findIndex((x) => x.person.id === pid);
      if (i >= 0) people.splice(i, 1);
    },
    async deleteReference(rid) {
      await wait();
      for (const p of people) p.person.references = p.person.references.filter((r) => r.id !== rid);
    },
    async inspectPhoto(path) {
      await delay(latency * 3);
      const seed = hash(path);
      const group = /group|team|class/i.test(path);
      const avatars = group ? [seed, seed + 1, seed + 2] : [seed];
      return {
        width: 1200,
        height: 1500,
        suggested: group ? null : 0,
        faces: avatars.map((s, index) => ({
          index,
          bbox: { x: 300 + index * 200, y: 300, w: 220 - index * 30, h: 260 - index * 30 },
          score: 0.94 - index * 0.04,
          facePx: 220 - index * 30,
          warnings:
            index === 2 ? ['The face is small in this photo; a closer photo will match more reliably.'] : [],
          preview: svgUrl(portraitSvg(randomAvatar(s))),
        })),
      };
    },
    async addReference(personId, path) {
      await delay(latency * 2);
      const p = people.find((x) => x.person.id === personId);
      if (!p) throw new Error('person not found');
      const rid = id();
      if (p.person.references.length === 0) p.avatar = randomAvatar(hash(path));
      refAvatars.set(rid, p.avatar);
      p.person.references.push({
        id: rid,
        personId,
        sourceName: path.split(/[\\/]/).pop() ?? 'photo.jpg',
        detScore: 0.92,
        facePx: 220,
        createdAt: now(),
      });
      return rid;
    },
    async importRoster(folder) {
      const names: [string, string][] = [
        ['Amara Okafor', 'Willow Cabin'],
        ['Jonah Becker', 'Willow Cabin'],
        ['Priya Patel', 'Willow Cabin'],
      ];
      const total = names.length + 2;
      const report: ImportReport = { peopleCreated: 0, photosAdded: 0, entries: [] };
      for (let i = 0; i < names.length; i++) {
        emit('import-progress', { done: i, total });
        await delay(latency * 2);
        const [name, group] = names[i]!;
        const p = addPerson(name, group, randomAvatar(hash(name)));
        report.peopleCreated++;
        report.photosAdded++;
        report.entries.push({
          file: `${name}.jpg`,
          path: `${folder}/${group}/${name}.jpg`,
          name,
          group,
          status: 'added',
          personId: p.person.id,
          created: true,
        });
      }
      report.entries.push({
        file: 'Willow Cabin group.jpg',
        path: `${folder}/Willow Cabin group.jpg`,
        name: 'Willow Cabin group',
        group: '',
        status: 'needsAttention',
        reason: '6 faces were found; add this photo by hand and pick one',
      });
      report.entries.push({
        file: 'Theo Grant.jpg',
        path: `${folder}/Willow Cabin/Theo Grant.jpg`,
        name: 'Theo Grant',
        group: 'Willow Cabin',
        status: 'needsAttention',
        reason: 'no face was found',
      });
      emit('import-progress', { done: total, total });
      return report;
    },

    async startScan(name, folder) {
      cancelled = false;
      const total = 24;
      const placeholderId = nextId;
      emit('scan-progress', {
        scanId: placeholderId,
        phase: 'preparing',
        total: 0,
        done: 0,
        faces: 0,
        matches: 0,
        check: 0,
        current: null,
      });
      let faces = 0;
      let matches = 0;
      let check = 0;
      for (let i = 1; i <= total; i++) {
        if (cancelled) break;
        await delay(Math.max(30, latency));
        faces += 3;
        if (i % 3 === 0) matches++;
        if (i % 7 === 0) check++;
        emit('scan-progress', {
          scanId: placeholderId,
          phase: 'scanning',
          total,
          done: i,
          faces,
          matches,
          check,
          current: `morning/IMG_${3000 + i}.jpg`,
        });
      }
      const s = generateScan(
        name || folder.split(/[\\/]/).pop() || 'Event',
        folder,
        hash(name + folder),
        total,
        0,
      );
      if (cancelled) s.summary.status = 'cancelled';
      emit('scan-progress', {
        scanId: s.summary.id,
        phase: 'finished',
        total,
        done: total,
        faces,
        matches,
        check,
        current: null,
      });
      return s.summary.id;
    },
    async listScans() {
      await wait();
      return scans.map(summarize);
    },
    async getScan(scanId) {
      await wait();
      return summarize(findScan(scanId));
    },
    async deleteScan(scanId) {
      await wait();
      const i = scans.findIndex((s) => s.summary.id === scanId);
      if (i >= 0) scans.splice(i, 1);
    },
    async rematch(scanId) {
      await delay(latency * 3);
      const s = findScan(scanId);
      s.summary.rosterSize = people.length;
      return { facesChecked: s.faces.length, changed: 0 };
    },

    async reviewItems(scanId, pile: ReviewPile) {
      await wait();
      const s = findScan(scanId);
      return s.faces
        .filter((f) => f.record.pile === pile)
        .map((f): ReviewItem => {
          const photo = s.photos.find((p) => p.record.id === f.record.photoId)!;
          const person = people.find((p) => p.person.id === f.record.personId);
          return {
            face: { ...f.record },
            photo: { ...photo.record },
            person: person
              ? {
                  id: person.person.id,
                  name: person.person.name,
                  group: person.person.group,
                  referenceIds: person.person.references.map((r) => r.id),
                }
              : null,
          };
        })
        .sort((a, b) => a.photo.relPath.localeCompare(b.photo.relPath) || a.face.bbox.x - b.face.bbox.x);
    },
    async setDecision(faceId, decision) {
      const f = findFace(faceId);
      f.record.decision = decision;
      f.record.reviewed = decision !== 'pending';
      return { ...f.record };
    },
    async restoreDecision(faceId, decision, reviewed) {
      const f = findFace(faceId);
      f.record.decision = decision;
      f.record.reviewed = reviewed && decision !== 'pending';
      return { ...f.record };
    },
    async bulkDecision(scanId, pile, onlyPending, decision) {
      await wait();
      let n = 0;
      for (const f of findScan(scanId).faces) {
        if (f.record.pile === pile && (!onlyPending || f.record.decision === 'pending')) {
          f.record.decision = decision;
          f.record.reviewed = true;
          n++;
        }
      }
      return n;
    },
    async photoDetail(photoId) {
      await wait();
      const p = findPhoto(photoId);
      return {
        photo: { ...p.record },
        faces: allFaces()
          .filter((f) => f.record.photoId === photoId)
          .map((f) => ({
            ...f.record,
            personName: people.find((x) => x.person.id === f.record.personId)?.person.name ?? null,
          })),
      };
    },
    async addManualRegion(photoId, bbox: BBox) {
      await wait();
      const p = findPhoto(photoId);
      const s = findScan(p.record.scanId);
      const fid = id();
      s.faces.push({
        record: {
          id: fid,
          photoId,
          bbox,
          landmarks: null,
          detScore: null,
          category: 'manual',
          pile: 'none',
          personId: null,
          similarity: null,
          decision: 'blur',
          reviewed: true,
        },
        scene: p.scene!,
      });
      return fid;
    },
    async deleteManualRegion(faceId) {
      await wait();
      for (const s of scans)
        s.faces = s.faces.filter((f) => !(f.record.id === faceId && f.record.category === 'manual'));
    },

    async exportScan(scanId, destination) {
      cancelled = false;
      const s = findScan(scanId);
      const sum = summarize(s);
      const held = new Set(
        s.faces.filter((f) => f.record.decision === 'pending').map((f) => f.record.photoId),
      );
      const ok = s.photos.filter((p) => p.record.status === 'ok' && !held.has(p.record.id));
      for (let i = 1; i <= ok.length; i++) {
        if (cancelled) throw new Error('the operation was cancelled');
        await delay(Math.max(20, latency / 3));
        emit('export-progress', { total: ok.length, done: i, current: ok[i - 1]!.record.relPath });
      }
      const blurredPhotos = new Set(
        s.faces
          .filter((f) => f.record.decision === 'blur' && !held.has(f.record.photoId))
          .map((f) => f.record.photoId),
      );
      const report: ExportReport = {
        outputDir: `${destination}/${s.summary.name} (blurred ${new Date().toISOString().slice(0, 10)})`,
        exported: ok.length,
        photosBlurred: blurredPhotos.size,
        facesBlurred: s.faces.filter((f) => f.record.decision === 'blur' && !held.has(f.record.photoId))
          .length,
        heldBack: s.photos
          .filter((p) => held.has(p.record.id))
          .map((p) => ({ relPath: p.record.relPath, reason: 'a face still needs a decision' })),
        failed: [],
        notSupported: s.photos.filter((p) => p.record.status === 'unsupported').map((p) => p.record.relPath),
        durationMs: 1800 + sum.counts.photosOk * 40,
      };
      return report;
    },

    mediaUrl(kind: MediaKind, mid: number) {
      if (kind === 'ref' || kind === 'refface') {
        const a = refAvatars.get(mid);
        return a ? svgUrl(portraitSvg(a)) : '';
      }
      if (kind === 'face') {
        const f = allFaces().find((x) => x.record.id === mid);
        return f ? svgUrl(sceneCropSvg(f.scene, f.record.bbox)) : '';
      }
      try {
        const p = findPhoto(mid);
        return p.scene ? svgUrl(sceneSvg(p.scene)) : '';
      } catch {
        return '';
      }
    },
    async pickFolder(title) {
      await delay(80);
      if (/export|save/i.test(title)) return '/Users/organizer/Desktop/Ready to upload';
      if (/roster/i.test(title)) return '/Users/organizer/Pictures/Opt-out roster';
      return '/Users/organizer/Pictures/Summer Camp Week 1';
    },
    async pickImage() {
      await delay(80);
      return '/Users/organizer/Pictures/Opt-out roster/New camper.jpg';
    },
    async pickModel() {
      return null;
    },
    async listen(event, cb): Promise<Unlisten> {
      const set = listeners.get(event) ?? new Set();
      set.add(cb as (p: never) => void);
      listeners.set(event, set);
      return () => set.delete(cb as (p: never) => void);
    },
  };
  return backend;
}
