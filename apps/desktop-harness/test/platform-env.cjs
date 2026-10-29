'use strict';
// Environment values that point a hook at "no island" in a shape the platform's config resolver accepts.
// Linux/macOS: a Unix runtime directory and socket path. Windows: only an absolute `...\endpoint.json` metadata path is
// accepted (config.cjs resolveConfigWindows), anything else is refused as "Unexpected endpoint" before the client runs.
const win = process.platform === 'win32';
// Both variables, pointing at a runtime that does not exist.
const offlineEnv = name => win
  ? { ABITO_DESKTOP_SOCKET: `C:\\${name}\\endpoint.json` }
  : { ABITO_SOCKET_DIR: `/${name}`, ABITO_DESKTOP_SOCKET: `/${name}/p-abcdef/s` };
// Runtime directory only (the endpoint is then discovered, and is absent).
const offlineDir = name => win ? { ABITO_DESKTOP_SOCKET: `C:\\${name}\\endpoint.json` } : { ABITO_SOCKET_DIR: `/${name}` };
// An explicit endpoint override that exists nowhere (the tests inject a stub client, so it is never connected).
const stubEndpoint = (name = 'nope') => ({ ABITO_DESKTOP_SOCKET: win ? `C:\\${name}\\endpoint.json` : `/${name}` });
// Tests of a Unix-only MECHANISM (mode bits, uid, /run/user handoff, /bin/sh). `twin` names the test that asserts the same
// behaviour on Windows, or the registered gap; the skip reason is printed by CI so nothing is skipped silently.
const unixOnly = twin => ({ skip: win && `Unix mechanism; Windows: ${twin}` });
const unixTest = (base, twin) => (name, fn) => base(name, unixOnly(twin), fn);
// Child-process hook tests: on Windows preload pipe-preload.cjs and tell it which pipe to use (see that file).
const hookNodeArgs = win ? ['-r', require('node:path').join(__dirname, 'pipe-preload.cjs')] : [];
const pipeEnv = receiver => (receiver.pipe ? { ABITO_TEST_PIPE: receiver.pipe } : {});
module.exports = { hookNodeArgs, pipeEnv, unixOnly, unixTest, offlineEnv, offlineDir, stubEndpoint };
