'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { resolveConfig } = require('../config.cjs');

test('the default root is the Tauri island\'s own leaf; the Electron production and Preview roots stay explicit', () => {
  // the island binds `$XDG_RUNTIME_DIR/abito-island` (src/transport.rs `default_runtime`): a producer with no
  // override must look THERE, not in the Electron app's `abito-desktop` leaf
  // uid 4321 does not exist here and the process probes are injected: the default runtime is computed WITHOUT
  // reading the live installed island's real endpoint.json (a live one may exist on this machine)
  const offline = { procExists: () => false };
  assert.equal(resolveConfig({}, 4321, offline).runtime, '/run/user/4321/abito-island');
  assert.equal(resolveConfig({ XDG_RUNTIME_DIR: '/run/user/1000' }, 1000).runtime, '/run/user/1000/abito-island');
  assert.equal(resolveConfig({ XDG_RUNTIME_DIR: '/tmp/x-run' }, 1000).runtime, '/tmp/x-run/abito-island', 'XDG_RUNTIME_DIR wins over the uid guess');
  for (const bad of ['', 'relative', '/a/../b', '/a//b', '/a/', '/']) {
    assert.equal(resolveConfig({ XDG_RUNTIME_DIR: bad }, 1000).runtime, '/run/user/1000/abito-island', `an unusable XDG_RUNTIME_DIR ${JSON.stringify(bad)} falls back to the uid path`);
  }
  assert.equal(resolveConfig({ ABITO_SOCKET_DIR: '/run/user/1000/abito-desktop' }, 1000).runtime, '/run/user/1000/abito-desktop', 'the Electron production root is still reachable, explicitly');
  assert.equal(resolveConfig({ ABITO_SOCKET_DIR: '/run/user/1000/abito-desktop-preview' }, 1000).runtime,
    '/run/user/1000/abito-desktop-preview');
  assert.equal(resolveConfig({ ABITO_SOCKET_DIR: '/home/u/new-root' }, 1000).runtime, '/home/u/new-root');
  // with no handoff published (or no island running) there is no connection
  assert.deepEqual(resolveConfig({}, 4321, offline), { runtime: '/run/user/4321/abito-island', endpoint: undefined, source: undefined });
});

test('override is exact, normalized, never inferred from arbitrary endpoint', () => {
  for (const value of ['', 'relative', '/a/../b', '/a//b', '/a/', '/']) {
    assert.throws(() => resolveConfig({ ABITO_SOCKET_DIR: value }, 1000));
  }
  assert.throws(() => resolveConfig({ ABITO_DESKTOP_SOCKET: '/home/u/root/p-abc123/s' }, 1000));
  assert.deepEqual(resolveConfig({ ABITO_SOCKET_DIR: '/home/u/root',
    ABITO_DESKTOP_SOCKET: '/home/u/root/p-abc123/s' }, 1000),
    { runtime: '/home/u/root', endpoint: '/home/u/root/p-abc123/s', source: 'env' });
  for (const endpoint of ['/home/u/root/p-abc123/no', '/home/u/root/p-a/s', '/home/u/else/p-abc123/s']) {
    assert.throws(() => resolveConfig({ ABITO_SOCKET_DIR: '/home/u/root', ABITO_DESKTOP_SOCKET: endpoint }, 1000));
  }
});
