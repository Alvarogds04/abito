// mirrors apps/desktop-pi/test/fake-receiver.cjs (own copy; no cross-app require)
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { EventEmitter } = require('node:events');

// A real AF_UNIX socket, strictly below this worktree; not the installed app.
async function fakeReceiver(t, { questions = false, busyReason = false, structured = false } = {}) {
  const runtime = fs.mkdtempSync(path.join(path.resolve(__dirname, '..'), '.test-'));
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
  t.after(async () => {
    for (const client of clients) client.destroy();
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(runtime, { recursive: true });
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
  // Like next(), but also sees a frame that already arrived (the producer's last
  // frame can land before the test registers its listener).
  function seen(type) {
    const existing = frames.find(f => f.type === type);
    return existing ? Promise.resolve(existing) : next(type);
  }
  return { runtime, endpoint, frames, rawFrames, clients, next, seen,
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
module.exports = { fakeReceiver };
