#!/usr/bin/env node
// Download the models Blurlist bundles and verify their SHA-256 checksums.
//
//   node scripts/fetch-models.mjs            # download missing models into ./models
//   node scripts/fetch-models.mjs --verify   # only check what is already there
//
// Behind an HTTPS proxy, run with NODE_USE_ENV_PROXY=1 so Node's fetch uses it.

import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const modelsDir = join(root, 'models');
const manifest = JSON.parse(await readFile(join(modelsDir, 'manifest.json'), 'utf8'));
const verifyOnly = process.argv.includes('--verify');

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');
const mb = (n) => `${(n / 1024 / 1024).toFixed(1)} MB`;

async function download(url, attempts = 4) {
  let lastError;
  for (let i = 1; i <= attempts; i++) {
    try {
      const res = await fetch(url, { redirect: 'follow' });
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
      return Buffer.from(await res.arrayBuffer());
    } catch (err) {
      lastError = err;
      if (i < attempts) {
        const wait = 2 ** i * 1000;
        console.warn(`  attempt ${i} failed (${err.message}); retrying in ${wait / 1000}s`);
        await new Promise((r) => setTimeout(r, wait));
      }
    }
  }
  throw lastError;
}

let failed = false;
await mkdir(modelsDir, { recursive: true });
for (const m of manifest.models) {
  const dest = join(modelsDir, m.file);
  if (existsSync(dest)) {
    const digest = sha256(await readFile(dest));
    if (digest === m.sha256) {
      console.log(`✓ ${m.file} (${m.license}) already present`);
      continue;
    }
    console.warn(
      `✗ ${m.file} has the wrong checksum (${digest}); ${verifyOnly ? 'failing' : 're-downloading'}`,
    );
    if (verifyOnly) {
      failed = true;
      continue;
    }
  } else if (verifyOnly) {
    console.error(`✗ ${m.file} is missing`);
    failed = true;
    continue;
  }

  console.log(`↓ ${m.file} (${mb(m.bytes)}, ${m.license})`);
  const data = await download(m.url);
  const digest = sha256(data);
  if (digest !== m.sha256) {
    console.error(`✗ ${m.file}: checksum mismatch\n  expected ${m.sha256}\n  got      ${digest}`);
    failed = true;
    continue;
  }
  const tmp = `${dest}.part`;
  await writeFile(tmp, data);
  await rm(dest, { force: true });
  await rename(tmp, dest);
  console.log(`✓ ${m.file} verified`);
}

if (failed) {
  console.error('\nSome models could not be fetched or verified.');
  process.exit(1);
}
console.log(`\nModels are ready in ${modelsDir}`);
