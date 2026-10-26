'use strict';
// mirrors apps/desktop-pi/config.cjs (own copy; no cross-app require). Both read the island's `<runtime>/endpoint.json`
// handoff with the same checks; this copy has no question config, and an absent ABITO_DESKTOP_SOCKET with no valid
// handoff yields {endpoint: undefined}: callers treat it as unavailable and never scan.
// ABITO_PROJECT_ROOT is intentionally NOT read here.
const fs = require('node:fs');
const path = require('node:path');

function normalized(value) {
  return typeof value === 'string' && value !== '/' && path.isAbsolute(value) &&
    path.normalize(value) === value && !value.endsWith('/') && !value.includes('\0');
}
function endpointShape(endpoint, runtime) {
  return normalized(endpoint) && Buffer.byteLength(endpoint) <= 107 &&
    path.dirname(path.dirname(endpoint)) === runtime && path.basename(endpoint) === 's' &&
    /^p-[a-zA-Z0-9]{6}$/.test(path.basename(path.dirname(endpoint)));
}
// Default runtime: ABITO_SOCKET_DIR, else $XDG_RUNTIME_DIR/abito-island, else /run/user/<uid>/abito-island.
function defaultRuntime(env, owner) {
  if (env.ABITO_SOCKET_DIR !== undefined) return env.ABITO_SOCKET_DIR;
  const xdg = env.XDG_RUNTIME_DIR;
  if (normalized(xdg)) return `${xdg}/abito-island`;
  return `/run/user/${owner}/abito-island`;
}
const MAX_HANDOFF = 4096;
function readProcState(pid) {
  let fd;
  try {
    fd = fs.openSync(`/proc/${pid}/stat`, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    const buffer = Buffer.alloc(512);
    const n = fs.readSync(fd, buffer, 0, buffer.length, 0);
    const stat = buffer.toString('utf8', 0, n);
    const close = stat.lastIndexOf(')');
    if (close < 0) return undefined;
    return /^\s+([A-Za-z])(?:\s|$)/.exec(stat.slice(close + 1))?.[1];
  } catch { return undefined; } finally {
    if (fd !== undefined) try { fs.closeSync(fd); } catch { /* ignore */ }
  }
}
// Field 22 of /proc/<pid>/stat (process start time in clock ticks) as a decimal string. `comm` may contain spaces and
// parentheses, so the fields are parsed after the LAST ')'. `undefined` when unreadable. Mirrors the island's `proc_start`.
function readProcStart(pid) {
  let fd;
  try {
    fd = fs.openSync(`/proc/${pid}/stat`, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    const buffer = Buffer.alloc(1024);
    const n = fs.readSync(fd, buffer, 0, buffer.length, 0);
    const stat = buffer.toString('utf8', 0, n);
    const close = stat.lastIndexOf(')');
    if (close < 0) return undefined;
    const ticks = stat.slice(close + 2).split(' ')[22 - 3];
    return typeof ticks === 'string' && /^[0-9]+$/.test(ticks) ? ticks : undefined;
  } catch { return undefined; } finally {
    if (fd !== undefined) try { fs.closeSync(fd); } catch { /* ignore */ }
  }
}
// Stable handoff `<runtime>/endpoint.json`: a pointer, not authority. Any failure => undefined (silent).
function readHandoff(runtime, owner, procExists, procState, procStart) {
  let fd;
  try {
    fd = fs.openSync(`${runtime}/endpoint.json`, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
    const st = fs.fstatSync(fd);
    if (!st.isFile() || st.uid !== owner || st.size > MAX_HANDOFF ||
        (st.mode & 0o7777) !== 0o600 || st.nlink !== 1) return undefined;
    const buffer = Buffer.alloc(MAX_HANDOFF + 1);
    const n = fs.readSync(fd, buffer, 0, MAX_HANDOFF + 1, 0);
    if (n > MAX_HANDOFF) return undefined;
    const text = buffer.toString('utf8', 0, n);
    const value = JSON.parse(text);
    if (value === null || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const canonical = JSON.stringify(value);
    if (text !== canonical && text !== `${canonical}\n`) return undefined;
    // closed key set: v, endpoint, pid and, optionally, procStart (the island's identity of the pid)
    const keys = Object.keys(value).sort().join();
    const hasStart = keys === 'endpoint,pid,procStart,v';
    if (keys !== 'endpoint,pid,v' && !hasStart) return undefined;
    if (value.v !== 1 || !Number.isSafeInteger(value.pid) || value.pid < 1 || value.pid > 2147483647) return undefined;
    if (typeof value.endpoint !== 'string' || !endpointShape(value.endpoint, runtime)) return undefined;
    // Load lazily to avoid config.cjs <-> security.cjs initialization cycles.
    require('./security.cjs').verifyEndpoint(value.endpoint, runtime);
    if (!procExists(value.pid)) return undefined;
    const state = procState(value.pid);
    if (typeof state !== 'string' || !/^[A-Za-z]$/.test(state) || state === 'Z' || state === 'X') return undefined;
    // Present = the pid is an IDENTITY: a recycled pid has another start time. Absent keeps the old behaviour
    // (existence only). A claimed value that is malformed, or a start time we cannot read, fails closed.
    if (hasStart) {
      const claimed = value.procStart;
      if (typeof claimed !== 'string' || !/^[0-9]+$/.test(claimed)) return undefined;
      if (procStart(value.pid) !== claimed) return undefined;
    }
    return value.endpoint;
  } catch { return undefined; } finally {
    if (fd !== undefined) try { fs.closeSync(fd); } catch { /* ignore */ }
  }
}
// Resolving configuration never creates a directory or connects a socket.
function resolveConfig(env, owner, options = {}) {
  if (!Number.isInteger(owner) || owner < 0) throw new Error('Linux uid required');
  const runtime = defaultRuntime(env, owner);
  if (!normalized(runtime)) throw new Error('Invalid ABITO_SOCKET_DIR');
  const explicit = env.ABITO_DESKTOP_SOCKET;
  if (explicit !== undefined) {
    if (!endpointShape(explicit, runtime)) throw new Error('Unexpected endpoint');
    return { runtime, endpoint: explicit, source: 'env' };
  }
  const procExists = options.procExists ?? (pid => fs.existsSync(`/proc/${pid}`));
  const procState = options.procState ?? readProcState;
  const procStart = options.procStart ?? readProcStart;
  const endpoint = readHandoff(runtime, owner, procExists, procState, procStart);
  return { runtime, endpoint, source: endpoint === undefined ? undefined : 'handoff' };
}
module.exports = { resolveConfig, endpointShape };
