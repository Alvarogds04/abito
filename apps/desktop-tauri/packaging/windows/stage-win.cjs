#!/usr/bin/env node
'use strict';
/* U4 receipt, windows flavor (format-A spirit, decision B: NSIS-direct).
   Pins the hashes + provenance of what `xbuild.sh package` produced in the
   gitignored dist/. `--check` re-hashes against the stored receipt.
   Nothing here installs anything or launches the binary. */
const fs = require('node:fs');
const path = require('node:path');
const L = require('../lib.cjs');

const crate = path.resolve(__dirname, '../..');
const dist = path.join(crate, 'dist');
const args = process.argv.slice(2);

const cfg = L.checkConfig(crate); // identifier/version/csp gate shared with linux
const r6 = L.checkR6(crate);
L.checkFrontend(crate);

const exe = path.join(dist, 'abito-island.exe');
const setup = path.join(dist, `abito-island-${cfg.version}-win-x64-setup.exe`);
const receiptPath = path.join(dist, 'receipt-windows.json');
for (const f of [exe, setup]) {
  if (!fs.existsSync(f)) { console.error(`missing artifact: ${f} (run: sh packaging/windows/xbuild.sh package)`); process.exit(1); }
}
const files = { 'abito-island.exe': L.sha256File(exe), [`abito-island-${cfg.version}-win-x64-setup.exe`]: L.sha256File(setup) };

if (args.includes('--check')) {
  const rec = JSON.parse(fs.readFileSync(receiptPath, 'utf8'));
  const problems = [];
  for (const [name, hash] of Object.entries(files)) {
    if (rec.files[name] !== hash) problems.push(`hash mismatch: ${name}`);
  }
  if (problems.length) { console.error('FAIL:\n - ' + problems.join('\n - ')); process.exit(1); }
  console.log('OK: windows artifacts match the receipt');
  process.exit(0);
}

const prov = L.provenance(crate);
const receipt = {
  identifier: L.IDENTIFIER,
  version: cfg.version,
  format: 'A user-local (windows, cross container)',
  toolchain: { image: 'abito-xwin-build:latest (rust 1.98 + cargo-xwin + system NSIS)', target: 'x86_64-pc-windows-msvc' },
  r6_sha256: r6,
  provenance: { head: prov.head, clean: prov.clean, dirty_entries: prov.dirty.length, ...(prov.clean ? {} : { dirty: prov.dirty }) },
  files,
  inputs: {
    'Cargo.lock': L.sha256File(path.join(crate, 'Cargo.lock')),
    'tauri.conf.json': L.sha256File(path.join(crate, 'tauri.conf.json')),
    'capabilities/main.json': L.sha256File(path.join(crate, 'capabilities', 'main.json')),
    'packaging/windows/Dockerfile': L.sha256File(path.join(crate, 'packaging', 'windows', 'Dockerfile')),
    'packaging/windows/abito.nsi': L.sha256File(path.join(crate, 'packaging', 'windows', 'abito.nsi')),
    frontend: Object.fromEntries(L.FRONTEND_FILES.map(f => [f, L.sha256File(path.join(crate, 'frontend', f))])),
  },
  csp: cfg.csp,
  build: {
    profile: 'release', features: 'tauri/custom-protocol (no probe)',
    paths: 'stable by construction: the container pins the crate to /abito/src, the registry to /usr/local/cargo and one image toolchain — no remap flags needed, but the checkout path is therefore NOT part of the hash (unlike the linux build.cjs which remaps to the same /abito/src)',
    subsystem: 'windows_subsystem=windows in release (no console); debug keeps the console for U2 VM logs',
    signing: 'NONE - unsigned; the VM checklist validates the SmartScreen path explicitly',
  },
  note: 'Staged only (dist/, gitignored). Nothing installed, nothing launched. dirty_entries is the honest count at build time; a final receipt from a clean checkout must show clean:true. Measured: the cargo exe is byte-identical across rebuilds of one commit (6c2e44b9 on two runs), but makensis embeds a build timestamp in the installer PE header, so the SETUP hash is per packaging run — the receipt and its dist/ files are a matched pair; regenerating the installer invalidates --check by design.',
};
fs.writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n');
console.log(`receipt: ${receiptPath}`);
for (const [name, hash] of Object.entries(files)) console.log(`${name} sha256 ${hash}`);
