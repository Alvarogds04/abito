'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { verifyEndpoint } = require('../src/security.cjs');
const { fakeReceiver } = require('./fake-receiver.cjs');

test('accepts a private socket and returns dev/ino', async t => {
  const r = await fakeReceiver(t);
  const id = verifyEndpoint(r.endpoint, r.runtime);
  assert.ok(Number.isInteger(id.ino) && id.dev !== undefined);
});
test('rejects wrong socket mode, wrong dir mode and runtime mismatch', async t => {
  const r = await fakeReceiver(t);
  fs.chmodSync(r.endpoint, 0o666);
  assert.throws(() => verifyEndpoint(r.endpoint, r.runtime));
  fs.chmodSync(r.endpoint, 0o600);
  fs.chmodSync(path.dirname(r.endpoint), 0o755);
  assert.throws(() => verifyEndpoint(r.endpoint, r.runtime));
  fs.chmodSync(path.dirname(r.endpoint), 0o700);
  assert.throws(() => verifyEndpoint(r.endpoint, r.runtime + '-other'));
  verifyEndpoint(r.endpoint, r.runtime);
});
test('rejects symlinked endpoint, symlinked directory and regular files', async t => {
  const r = await fakeReceiver(t);
  const dir = path.dirname(r.endpoint);
  const link = path.join(r.runtime, 'p-link12');
  fs.mkdirSync(link, { mode: 0o700 }); fs.chmodSync(link, 0o700);
  fs.symlinkSync(r.endpoint, path.join(link, 's'));
  assert.throws(() => verifyEndpoint(path.join(link, 's'), r.runtime));
  const other = path.join(r.runtime, 'p-dirlnk');
  fs.symlinkSync(dir, other);
  assert.throws(() => verifyEndpoint(path.join(other, 's'), r.runtime));
  const file = path.join(r.runtime, 'p-file12');
  fs.mkdirSync(file, { mode: 0o700 }); fs.chmodSync(file, 0o700);
  fs.writeFileSync(path.join(file, 's'), '', { mode: 0o600 });
  assert.throws(() => verifyEndpoint(path.join(file, 's'), r.runtime));
});
test('rejects entries owned by someone else (simulated uid mismatch)', async t => {
  const r = await fakeReceiver(t);
  const real = process.getuid;
  process.getuid = () => real.call(process) + 12345;
  t.after(() => { process.getuid = real; });
  assert.throws(() => verifyEndpoint(r.endpoint, r.runtime));
});
