'use strict';
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { EventEmitter } = require('node:events');

// A real AF_UNIX socket, strictly below this worktree; not the installed app.
// `runtime` (optional) binds a SECOND receiver under an existing private root: the handoff test needs the same
// runtime to publish a different endpoint, exactly as an island restart does.
async function fakeReceiver(t, { questions = false, busyReason = false, structured = false, runtime: parent } = {}) {
  const runtime = parent ?? fs.mkdtempSync(path.join(path.resolve(__dirname, '..'), '.test-'));
  fs.chmodSync(runtime, 0o700);
  const directory = fs.mkdtempSync(path.join(runtime, 'p-'));
  fs.chmodSync(directory, 0o700);
  const endpoint = path.join(directory, 's');
  const capabilities = new Map();
  const clients = new Set(), frames = [], rawFrames = [], events = new EventEmitter();
  const server = net.createServer(socket => {
    clients.add(socket);
    socket.on('error', () => {});
    socket.on('close', () => { clients.delete(socket); capabilities.delete(socket); });
    let bytes = '';
    socket.on('data', chunk => {
      bytes += chunk.toString('utf8');
      let end;
      while ((end = bytes.indexOf('\n')) >= 0) {
        const raw = bytes.slice(0, end); rawFrames.push(raw + '\n');
        const frame = JSON.parse(raw); bytes = bytes.slice(end + 1);
        frames.push(frame); events.emit('frame', frame);
        if (frame.type === 'hello') {
          socket.write('{"v":1,"type":"ready"}\n');

        }
        if (frame.type === 'questions_hello' && questions) {
          capabilities.set(socket, busyReason && frame.busyReason === 1);
          socket.write(JSON.stringify({ v: 1, type: 'questions_ready', questionVersion: 1,
            ...(busyReason && frame.busyReason === 1 ? { busyReason: 1 } : {}),
            ...(structured && frame.structured === 1 ? { structured: 1 } : {}) }) + '\n');
        }
      }
    });
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(endpoint, resolve); });
  fs.chmodSync(endpoint, 0o600);
  let closed;
  // Idempotent stop: models an island restart (socket closes) without waiting for the test to end.
  const close = () => closed ??= (async () => {
    for (const client of clients) client.destroy();
    await new Promise(resolve => server.close(resolve));
  })();
  t.after(async () => {
    await close();
    fs.rmSync(runtime, { recursive: true, force: true });
  });
  function next(type) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { events.off('frame', handler); reject(new Error('No frame: ' + type)); }, 1500);
      function handler(frame) {
        if (frame.type !== type) return;
        clearTimeout(timer); events.off('frame', handler); resolve(frame);
      }
      events.on('frame', handler);
    });
  }
  return { runtime, endpoint, frames, rawFrames, clients, next, close,
    send: value => { for (const c of clients) c.write(JSON.stringify(value) + '\n'); },
    replyBusy: (q, code) => {
      const replies = [];
      for (const c of clients) {
        const reply = { v: 1, type: 'question_closed', questionVersion: 1, project: q.project,
          sessionId: q.sessionId, at: Date.now(), id: q.id, source: 'system', reason: 'busy',
          ...(capabilities.get(c) ? { busyReason: code } : {}) };
        const raw = JSON.stringify(reply) + '\n'; replies.push(raw); c.write(raw);
      }
      return replies;
    } };
}
function harness(registerAdapter, options) {
  const handlers = new Map();
  const pi = { on: (name, handler) => handlers.set(name, handler) };
  const controller = registerAdapter(pi, { retryMs: 20, heartbeatMs: 30, ...options });
  const ctx = { cwd: path.resolve(__dirname, '..'), sessionManager: { getSessionId: () => 'test-session' } };
  return { pi, handlers, controller, ctx };
}
module.exports = { fakeReceiver, harness };
