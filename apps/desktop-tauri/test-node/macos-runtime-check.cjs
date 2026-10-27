'use strict';
// S1/S2/S5: exercise the public bundle generator and the rendered runtime messages.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const cp = require('node:child_process');
const crate = path.resolve(__dirname, '..');
const S = require('../frontend/settings-view.js');
const O = require('../frontend/overview-view.js');
const L = require('../packaging/lib.cjs');
const messages = {
  node_missing: ['Falta Node.js', 'Falta Node.js: instalalo desde nodejs.org.'],
  pi_missing: ['Falta Pi', 'Falta Pi: instalalo con npm install -g @earendil-works/pi-coding-agent.'],
};
for (const [code, [word, detail]] of Object.entries(messages)) {
  const state = { connection: 'no_daemon', last_error: { code, n: 1 } };
  const view = S.connectionView(state);
  assert.equal(view.word, word);
  assert.equal(view.aria, detail);
  const row = O.engineRowView({ kind: 'down' }, state);
  assert.equal(row.label, word);
  assert.ok(row.aria.includes(detail));
}
assert.equal(S.connectionView({ connection: 'no_daemon' }).word, 'Sin motor');
assert.equal(S.connectionView({ connection: 'lost', last_error: { code: 'pi_missing' } }).word, 'Motor caído');
assert.equal(S.connectionView({ connection: 'connected', last_error: { code: 'pi_missing' } }), null);
assert.equal(O.engineRowView({ kind: 'down' }, { connection: 'no_daemon' }).label, 'Sin motor');
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'abito-macos-bundle-'));
try {
  const result = cp.spawnSync(process.execPath, [path.join(crate, 'packaging/macos-config.cjs')], { cwd: dir, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, '');
  const config = JSON.parse(result.stdout);
  const expected = Object.fromEntries([
    ...L.DAEMON_FILES.map(f => [`../desktop-daemon/${f}`, `desktop-daemon/${f}`]),
    ...L.HARNESS_FILES.map(f => [`../desktop-harness/${f}`, `desktop-harness/${f}`]),
  ]);
  assert.deepEqual(config.bundle.resources, expected);
  assert.equal(config.bundle.active, true);
  assert.equal(config.bundle.macOS.minimumSystemVersion, '12.0');
  // Ad-hoc seal of the whole bundle (no Apple account): Gatekeeper reports «unidentified developer», never «damaged».
  assert.equal(config.bundle.macOS.signingIdentity, '-');
  assert.equal(config.bundle.macOS.hardenedRuntime, false);
  for (const file of Object.keys(expected)) assert.ok(fs.statSync(path.resolve(crate, file)).isFile(), file);
  assert.deepEqual(fs.readdirSync(dir), [], 'generator writes no files in its caller cwd');
} finally { fs.rmSync(dir, { recursive: true, force: true }); }
// The island is a menu-bar accessory: no Dock icon on macOS (LSUIElement), the tray keeps Mostrar/Ocultar/Salir.
const plist = fs.readFileSync(path.join(crate, 'Info.plist'), 'utf8');
assert.match(plist, /<key>LSUIElement<\/key>\s*<true\/>/);
assert.match(plist, /<key>NSMicrophoneUsageDescription<\/key>/);
console.log('macos-runtime: 5 rules passed (missing runtime UI, existing states, exact bundle resources, ad-hoc seal, no Dock icon)');
