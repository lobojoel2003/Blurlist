#!/usr/bin/env node
// Regenerate the README images from the browser demo (illustrated people only).
//
//   node scripts/screenshots.mjs
//
// Needs Playwright with Chromium available to `require('playwright')`.

import { mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createServer } from 'vite';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(root, 'docs', 'images');
const require = createRequire(import.meta.url);
let chromium;
try {
  ({ chromium } = require('playwright'));
} catch {
  console.error('Playwright is not installed. Run `npm i -D playwright && npx playwright install chromium`.');
  process.exit(1);
}

await mkdir(out, { recursive: true });
const server = await createServer({ root, server: { port: 0, strictPort: false }, logLevel: 'error' });
await server.listen();
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1360, height: 860 }, deviceScaleFactor: 2 });
page.on('pageerror', (e) => console.error('page error:', e));
const wait = (ms) => page.waitForTimeout(ms);
const shot = async (name, opts = {}) => {
  await page.screenshot({ path: join(out, `${name}.png`), ...opts });
  console.log(`✓ docs/images/${name}.png`);
};
const open = async (view) => {
  await page.goto(`${base}/?view=${view}`);
  await page.waitForSelector('.main .page');
  await wait(900);
};

try {
  await page.goto(`${base}/scripts/readme/hero.html`);
  await page.waitForSelector('body[data-ready="1"]');
  await wait(800);
  await page.locator('#hero').screenshot({ path: join(out, 'hero.png') });
  console.log('✓ docs/images/hero.png');

  await open('review');
  await page.waitForSelector('.compare');
  await shot('review');

  // Show the whole-photo viewer on a busy photo with faces in several states.
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press('Space');
    await page.waitForSelector('.viewer-stage img');
    await wait(500);
    const boxes = await page.locator('.face-box').count();
    const kinds = await page.locator('.face-box.keep').count();
    if (boxes >= 4 && kinds >= 1) break;
    await page.keyboard.press('Escape');
    await wait(150);
    await page.keyboard.press('ArrowRight');
    await wait(150);
  }
  await wait(500);
  await shot('photo-viewer');
  await page.keyboard.press('Escape');
  await open('review');
  await page.waitForSelector('.compare');

  await page.getByRole('tab', { name: /check these/i }).click();
  await page.waitForSelector('.compare');
  await wait(600);
  await shot('check-these');

  await open('roster');
  await shot('roster');

  await open('scan');
  await page.getByText('Choose the event folder').click();
  await wait(300);
  await page.getByRole('button', { name: /start scan/i }).click();
  await wait(1600);
  await shot('scan');

  await open('export');
  await page.getByText('Choose where to save it').click();
  await wait(400);
  await shot('export');

  await open('settings');
  await shot('settings');
} finally {
  await browser.close();
  await server.close();
}
