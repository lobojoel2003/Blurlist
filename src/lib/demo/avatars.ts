// Procedural flat illustrations of people, used by the demo backend so the
// app can be shown (README screenshots, tests) without any real faces.

export interface AvatarParams {
  skin: string;
  hair: string;
  hairStyle: 'short' | 'long' | 'curly' | 'bun' | 'buzz' | 'bob' | 'puffs';
  shirt: string;
  glasses: boolean;
  freckles: boolean;
}

export const SKINS = ['#F8D5B8', '#F1C09A', '#E0A479', '#C68457', '#9C6640', '#6E4529'];
export const HAIRS = ['#2B1B12', '#4A2E1C', '#7A4A24', '#C98B3F', '#E5C07B', '#1C1C1C', '#A0522D'];
export const SHIRTS = [
  '#4F7CFF',
  '#FF6B6B',
  '#2BB673',
  '#FFB020',
  '#9B6BFF',
  '#14B8C4',
  '#F06292',
  '#5C6BC0',
];
const STYLES: AvatarParams['hairStyle'][] = ['short', 'long', 'curly', 'bun', 'buzz', 'bob', 'puffs'];

/** Deterministic PRNG (mulberry32). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pick = <T>(r: () => number, arr: readonly T[]): T => arr[Math.floor(r() * arr.length)] as T;

export function randomAvatar(seed: number): AvatarParams {
  const r = rng(seed * 7919 + 13);
  return {
    skin: pick(r, SKINS),
    hair: pick(r, HAIRS),
    hairStyle: pick(r, STYLES),
    shirt: pick(r, SHIRTS),
    glasses: r() < 0.22,
    freckles: r() < 0.25,
  };
}

/** The detected face box inside the 200x240 avatar drawing. */
export const AVATAR_FACE = { x: 54, y: 58, w: 92, h: 106 };

function shade(hex: string, amt: number): string {
  const n = parseInt(hex.slice(1), 16);
  const c = (v: number) => Math.max(0, Math.min(255, Math.round(v + amt * 255)));
  const r = c((n >> 16) & 255);
  const g = c((n >> 8) & 255);
  const b = c(n & 255);
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
}

function hairBack(p: AvatarParams): string {
  switch (p.hairStyle) {
    case 'long':
      return `<path d="M50 104 C46 56 78 44 100 44 C124 44 156 56 150 104 L156 186 C140 196 60 196 44 186 Z" fill="${p.hair}"/>`;
    case 'bob':
      return `<path d="M50 104 C46 56 78 44 100 44 C124 44 156 56 150 104 L152 152 C140 160 60 160 48 152 Z" fill="${p.hair}"/>`;
    case 'curly': {
      let s = '';
      for (let i = 0; i < 11; i++) {
        const a = Math.PI * (0.95 + (i / 10) * 1.1);
        s += `<circle cx="${100 + Math.cos(a) * 50}" cy="${106 + Math.sin(a) * 54}" r="17" fill="${p.hair}"/>`;
      }
      return s;
    }
    case 'puffs':
      return `<circle cx="52" cy="70" r="22" fill="${p.hair}"/><circle cx="148" cy="70" r="22" fill="${p.hair}"/>`;
    case 'bun':
      return `<circle cx="100" cy="44" r="20" fill="${p.hair}"/>`;
    default:
      return '';
  }
}

function hairFront(p: AvatarParams): string {
  switch (p.hairStyle) {
    case 'buzz':
      return `<path d="M57 100 C56 66 78 56 100 56 C122 56 144 66 143 100 C134 82 118 76 100 76 C82 76 66 82 57 100 Z" fill="${p.hair}" opacity="0.82"/>`;
    case 'curly':
      return `<path d="M56 104 C54 62 80 52 100 52 C124 52 148 62 144 104 C136 86 120 80 100 80 C80 80 64 86 56 104 Z" fill="${p.hair}"/>`;
    default:
      return `<path d="M55 106 C52 60 80 50 100 50 C124 50 150 62 145 106 C138 82 122 76 108 78 C96 88 74 88 55 106 Z" fill="${p.hair}"/>`;
  }
}

export type Expression = 'smile' | 'grin' | 'open';

/** The person, drawn in a 200x240 box, without a background. */
export function avatarGroup(p: AvatarParams, expression: Expression = 'smile'): string {
  const dark = shade(p.skin, -0.12);
  const mouth =
    expression === 'grin'
      ? `<path d="M86 132 Q100 150 114 132 Z" fill="#7A2E2E"/><path d="M89 133 L111 133 L109 137 L91 137 Z" fill="#fff"/>`
      : expression === 'open'
        ? `<ellipse cx="100" cy="138" rx="8" ry="7" fill="#7A2E2E"/>`
        : `<path d="M87 132 Q100 145 113 132" stroke="#7A3B2E" stroke-width="3.6" fill="none" stroke-linecap="round"/>`;
  const glasses = p.glasses
    ? `<g fill="none" stroke="#2F3542" stroke-width="3"><rect x="70" y="101" width="25" height="19" rx="8"/><rect x="105" y="101" width="25" height="19" rx="8"/><path d="M95 109 L105 109"/></g>`
    : '';
  const freckles = p.freckles
    ? `<g fill="${shade(p.skin, -0.25)}" opacity="0.55"><circle cx="78" cy="124" r="1.6"/><circle cx="84" cy="127" r="1.6"/><circle cx="80" cy="130" r="1.6"/><circle cx="122" cy="124" r="1.6"/><circle cx="116" cy="127" r="1.6"/><circle cx="120" cy="130" r="1.6"/></g>`
    : '';
  return `
  ${hairBack(p)}
  <path d="M18 240 C18 200 52 180 100 180 C148 180 182 200 182 240 Z" fill="${p.shirt}"/>
  <path d="M80 182 Q100 204 120 182" fill="${shade(p.shirt, -0.12)}"/>
  <rect x="86" y="146" width="28" height="44" rx="12" fill="${dark}"/>
  <circle cx="55" cy="114" r="10" fill="${dark}"/><circle cx="145" cy="114" r="10" fill="${dark}"/>
  <ellipse cx="100" cy="110" rx="45" ry="52" fill="${p.skin}"/>
  ${hairFront(p)}
  <path d="M77 98 Q85 93 92 97" stroke="${shade(p.hair, -0.05)}" stroke-width="3" fill="none" stroke-linecap="round"/>
  <path d="M108 97 Q115 93 123 98" stroke="${shade(p.hair, -0.05)}" stroke-width="3" fill="none" stroke-linecap="round"/>
  <circle cx="83" cy="111" r="4.6" fill="#2B2B2B"/><circle cx="117" cy="111" r="4.6" fill="#2B2B2B"/>
  <circle cx="84.6" cy="109.4" r="1.4" fill="#fff"/><circle cx="118.6" cy="109.4" r="1.4" fill="#fff"/>
  <path d="M100 114 Q97 122 101 124" stroke="${shade(p.skin, -0.22)}" stroke-width="2.4" fill="none" stroke-linecap="round"/>
  <circle cx="74" cy="127" r="7" fill="#FF7A7A" opacity="0.22"/><circle cx="126" cy="127" r="7" fill="#FF7A7A" opacity="0.22"/>
  ${freckles}${mouth}${glasses}`;
}

export function svgUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** A studio-style reference portrait, cropped around the face. */
export function portraitSvg(p: AvatarParams, bg = '#E9EEF8'): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 20 180 200" width="360" height="400"><rect x="0" y="0" width="200" height="240" fill="${bg}"/>${avatarGroup(p, 'smile')}</svg>`;
}

export interface ScenePerson {
  params: AvatarParams;
  x: number;
  y: number;
  scale: number;
  expression: Expression;
}

export interface Scene {
  width: number;
  height: number;
  people: ScenePerson[];
  palette: number;
}

const BACKDROPS = [
  { sky: ['#BFE3FF', '#EAF6FF'], ground: ['#9AD88F', '#6DBE6F'] },
  { sky: ['#FFE2C2', '#FFF4E6'], ground: ['#C9E39A', '#97C66B'] },
  { sky: ['#CFE0FF', '#F3F7FF'], ground: ['#E7D3A8', '#D2B57F'] },
  { sky: ['#D8F3F0', '#F2FBFA'], ground: ['#7FC8A9', '#5BAF8A'] },
];

export function sceneBody(scene: Scene): string {
  const b = BACKDROPS[scene.palette % BACKDROPS.length]!;
  const { width: W, height: H } = scene;
  const horizon = H * 0.52;
  const r = rng(scene.palette * 101 + scene.people.length);
  let trees = '';
  for (let i = 0; i < 7; i++) {
    const tx = r() * W;
    const ts = 60 + r() * 70;
    trees += `<rect x="${tx - 8}" y="${horizon - ts * 0.5}" width="16" height="${ts * 0.6}" fill="#8B6B4A" opacity="0.7"/><circle cx="${tx}" cy="${horizon - ts * 0.75}" r="${ts * 0.55}" fill="#5FAE6A" opacity="0.75"/>`;
  }
  let bunting = '';
  for (let i = 0; i < 18; i++) {
    const bx = (W / 18) * i;
    const by = 40 + Math.sin((i / 17) * Math.PI) * 60;
    const color = SHIRTS[i % SHIRTS.length];
    bunting += `<path d="M${bx} ${by} L${bx + W / 18} ${by + 4} L${bx + W / 36} ${by + 46} Z" fill="${color}" opacity="0.9"/>`;
  }
  const people = [...scene.people]
    .sort((a, c) => a.y - c.y)
    .map(
      (p) =>
        `<g transform="translate(${p.x} ${p.y}) scale(${p.scale})">${avatarGroup(p.params, p.expression)}</g>`,
    )
    .join('');
  return `
  <defs>
    <linearGradient id="sky${scene.palette}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${b.sky[0]}"/><stop offset="1" stop-color="${b.sky[1]}"/></linearGradient>
    <linearGradient id="gr${scene.palette}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${b.ground[0]}"/><stop offset="1" stop-color="${b.ground[1]}"/></linearGradient>
  </defs>
  <rect width="${W}" height="${horizon}" fill="url(#sky${scene.palette})"/>
  <circle cx="${W * 0.85}" cy="${H * 0.14}" r="${H * 0.07}" fill="#FFF3B0"/>
  <ellipse cx="${W * 0.22}" cy="${H * 0.16}" rx="90" ry="26" fill="#fff" opacity="0.85"/>
  <ellipse cx="${W * 0.52}" cy="${H * 0.1}" rx="70" ry="20" fill="#fff" opacity="0.7"/>
  ${trees}
  <rect y="${horizon}" width="${W}" height="${H - horizon}" fill="url(#gr${scene.palette})"/>
  ${bunting}
  ${people}`;
}

export function sceneSvg(scene: Scene): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${scene.width} ${scene.height}" width="${scene.width}" height="${scene.height}">${sceneBody(scene)}</svg>`;
}

/** The face box of a scene person, in scene pixels. */
export function sceneFaceBox(p: ScenePerson): { x: number; y: number; w: number; h: number } {
  return {
    x: p.x + AVATAR_FACE.x * p.scale,
    y: p.y + AVATAR_FACE.y * p.scale,
    w: AVATAR_FACE.w * p.scale,
    h: AVATAR_FACE.h * p.scale,
  };
}

/** A square crop of the scene around a face (like Blurlist's review crops). */
export function sceneCropSvg(scene: Scene, box: { x: number; y: number; w: number; h: number }): string {
  const side = Math.max(box.w, box.h) * 1.9;
  const cx = box.x + box.w / 2;
  const cy = box.y + box.h / 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${cx - side / 2} ${cy - side / 2} ${side} ${side}" width="320" height="320">${sceneBody(scene)}</svg>`;
}
