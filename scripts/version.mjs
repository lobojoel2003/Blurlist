#!/usr/bin/env node
// Single source of truth for the release version.
//
//   node scripts/version.mjs check            # all manifests agree
//   node scripts/version.mjs check v1.2.3     # ...and match this tag
//   node scripts/version.mjs set 1.2.3        # bump everything at once
//   node scripts/version.mjs get              # print the version
//   node scripts/version.mjs notes 1.2.3      # print that CHANGELOG section
//
// The version lives in package.json, the Cargo workspace and Cargo.lock;
// src-tauri/tauri.conf.json reads it from package.json.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const write = (p, s) => writeFileSync(join(root, p), s);
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$/;

function versions() {
  const pkg = JSON.parse(read('package.json')).version;
  const lockPkg = JSON.parse(read('package-lock.json')).version;
  const cargo = read('Cargo.toml').match(/\[workspace\.package\][^[]*?\nversion = "([^"]+)"/)?.[1];
  const lock = read('Cargo.lock');
  const lockOf = (name) => lock.match(new RegExp(`name = "${name}"\\nversion = "([^"]+)"`))?.[1];
  const tauri = JSON.parse(read('src-tauri/tauri.conf.json')).version;
  return {
    'package.json': pkg,
    'package-lock.json': lockPkg,
    'Cargo.toml [workspace.package]': cargo,
    'Cargo.lock blurlist-core': lockOf('blurlist-core'),
    'Cargo.lock blurlist-app': lockOf('blurlist-app'),
    'src-tauri/tauri.conf.json': tauri === '../package.json' ? pkg : tauri,
  };
}

function check(tag) {
  const v = versions();
  const values = new Set(Object.values(v));
  let ok = values.size === 1 && !values.has(undefined);
  for (const [file, ver] of Object.entries(v))
    console.log(`${ver === v['package.json'] ? '✓' : '✗'} ${file}: ${ver}`);
  const version = v['package.json'];
  if (!SEMVER.test(version ?? '')) {
    console.error(`✗ "${version}" is not a semantic version`);
    ok = false;
  }
  if (tag) {
    const expected = `v${version}`;
    if (tag.replace(/^refs\/tags\//, '') !== expected) {
      console.error(`✗ tag ${tag} does not match version ${expected}`);
      ok = false;
    } else console.log(`✓ tag ${tag}`);
  }
  if (!ok) process.exit(1);
  console.log(`\nVersion ${version} is consistent.`);
}

function set(next) {
  if (!SEMVER.test(next)) {
    console.error(`"${next}" is not a semantic version (expected e.g. 1.4.0)`);
    process.exit(1);
  }
  const pkg = JSON.parse(read('package.json'));
  pkg.version = next;
  write('package.json', `${JSON.stringify(pkg, null, 2)}\n`);
  const lockJson = JSON.parse(read('package-lock.json'));
  lockJson.version = next;
  if (lockJson.packages?.['']) lockJson.packages[''].version = next;
  write('package-lock.json', `${JSON.stringify(lockJson, null, 2)}\n`);
  write(
    'Cargo.toml',
    read('Cargo.toml').replace(/(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, `$1${next}$2`),
  );
  let lock = read('Cargo.lock');
  for (const name of ['blurlist-core', 'blurlist-app']) {
    lock = lock.replace(new RegExp(`(name = "${name}"\\nversion = ")[^"]+(")`), `$1${next}$2`);
  }
  write('Cargo.lock', lock);
  console.log(
    `Version set to ${next}. Add a "## [${next}]" section to CHANGELOG.md, commit, then tag v${next}.`,
  );
}

function notes(version) {
  const log = read('CHANGELOG.md');
  const start = log.indexOf(`## [${version}]`);
  if (start === -1) {
    console.error(`CHANGELOG.md has no section for ${version}`);
    process.exit(1);
  }
  const rest = log.slice(start).split('\n').slice(1);
  const end = rest.findIndex((l) => /^## \[/.test(l) || /^\[[^\]]+\]: /.test(l));
  console.log(
    rest
      .slice(0, end === -1 ? undefined : end)
      .join('\n')
      .trim(),
  );
}

const [cmd, arg] = process.argv.slice(2);
switch (cmd) {
  case 'check':
    check(arg);
    break;
  case 'set':
    set(arg ?? '');
    break;
  case 'get':
    console.log(versions()['package.json']);
    break;
  case 'notes':
    notes(arg ?? versions()['package.json']);
    break;
  default:
    console.log('usage: version.mjs check [tag] | set <x.y.z> | get | notes [x.y.z]');
    process.exit(cmd ? 1 : 0);
}
