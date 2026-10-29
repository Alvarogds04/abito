'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { resolveQuestionConfig } = require('../config.cjs');
const { normalizeRequest } = require('../questions.cjs');
const { verifyEndpoint } = require('../security.cjs');
const { registerAdapter } = require('../adapter.cjs');
const { fakeReceiver, harness } = require('./fake-receiver.cjs');
const choice = { kind: 'question', text: 'Choose', options: [
  { value: 'one', label: 'One', description: '' }, { value: 'two', label: 'Two', description: '' }], allowFreeText: false };

test('configuration rejects invalid opt-ins and timeout outside bounds', () => {
  assert.deepEqual(resolveQuestionConfig({}), { questionLevel: 0, questionTimeoutMs: 300000 });
  assert.equal(resolveQuestionConfig({ ABITO_QUESTIONS: '1', ABITO_QUESTION_TIMEOUT_MS: '30000' }).questionLevel, 1);
  for (const value of ['true', '', '4']) assert.throws(() => resolveQuestionConfig({ ABITO_QUESTIONS: value }));
  for (const value of ['0', '29999', '900001', '1e5', '', 'NaN']) {
    assert.throws(() => resolveQuestionConfig({ ABITO_QUESTION_TIMEOUT_MS: value }));
  }
});
test('choice/text-only/mixed legal; approval exactly two unique tokens, no free text', () => {
  assert.doesNotThrow(() => normalizeRequest(choice));
  assert.doesNotThrow(() => normalizeRequest({ ...choice, allowFreeText: true }));
  assert.doesNotThrow(() => normalizeRequest({ ...choice, options: [], allowFreeText: true }));
  assert.throws(() => normalizeRequest({ ...choice, options: [] }));
  assert.throws(() => normalizeRequest({ ...choice, options: [choice.options[0]] }));
  assert.throws(() => normalizeRequest({ ...choice, options: [choice.options[0], choice.options[0]] }));
  assert.throws(() => normalizeRequest({ ...choice, kind: 'approval', allowFreeText: true }));
  assert.throws(() => normalizeRequest({ ...choice, kind: 'approval', options: [...choice.options,
    { value: 'always', label: 'Always', description: '' }] }));
  assert.throws(() => normalizeRequest({ ...choice, default: 'one' }));
});

test('source symlink is refused even when target is a private socket', async t => {
  const receiver = await fakeReceiver(t);
  if (process.platform === 'win32') {
    // Windows twin: an endpoint that is not directly `<runtime>\\endpoint.json` (a nested/redirected path) is refused.
    assert.throws(() => verifyEndpoint(path.win32.join(receiver.runtime, 'sub', 'endpoint.json'), receiver.runtime));
    return;
  }
  const dir = path.join(receiver.runtime, 'p-symlnk');
  fs.symlinkSync(path.dirname(receiver.endpoint), dir);
  assert.throws(() => verifyEndpoint(path.join(dir, 's'), receiver.runtime));
});

test('missing endpoint or unfinished handshake fails immediately, no question timeout waiting', async t => {
  const receiver = await fakeReceiver(t);
  for (const endpoint of [undefined, path.join(receiver.runtime, 'p-absent', 's'), receiver.endpoint]) {
    const { controller, handlers, ctx } = harness(registerAdapter, { runtime: receiver.runtime,
      endpoint, questionLevel: 1 });
    t.after(() => handlers.get('session_shutdown')());
    handlers.get('session_start')({}, ctx);
    const before = Date.now();
    assert.equal((await controller.ask(choice, ctx)).status, 'unavailable');
    assert.ok(Date.now() - before < 250);
  }
});

test('drop just after opt-in reports unsupported root and never downgrades or reconnects silently', async t => {
  const receiver = await fakeReceiver(t);
  let report;
  const reported = new Promise(resolve => { report = resolve; });
  const { controller, handlers, ctx } = harness(registerAdapter, { ...receiver, questionLevel: 1,
    onTransportError: report });
  t.after(() => handlers.get('session_shutdown')());
  const subscribed = receiver.next('questions_hello'); handlers.get('session_start')({}, ctx);
  await subscribed; for (const c of receiver.clients) c.destroy();
  assert.deepEqual(await reported, { status: 'unsupported_receiver', runtime: receiver.runtime });
  assert.deepEqual(await controller.ask(choice, ctx), { status: 'unsupported_receiver', runtime: receiver.runtime });
  await new Promise(resolve => setTimeout(resolve, 60));
  assert.equal(receiver.frames.filter(f => f.type === 'hello').length, 1);
});

test('multibyte answer within 2000 characters can exceed 4096 BYTES and is rejected', async t => {
  const receiver = await fakeReceiver(t, { questions: true });
  const { controller, handlers, ctx } = harness(registerAdapter, { ...receiver, questionLevel: 1 });
  t.after(() => handlers.get('session_shutdown')());
  handlers.get('session_start')({}, ctx); await receiver.next('heartbeat');
  const frame = receiver.next('question');
  const result = controller.ask({ ...choice, options: [], allowFreeText: true }, ctx);
  const q = await frame;
  const value = { v: 1, type: 'answer', questionVersion: 1, project: q.project, sessionId: q.sessionId,
    at: Date.now(), id: q.id, source: 'human', answer: { type: 'text', text: '界'.repeat(2000) } };
  assert.equal([...value.answer.text].length, 2000);
  assert.ok(Buffer.byteLength(JSON.stringify(value)) > 4096);
  receiver.send(value);
  assert.equal((await result).status, 'protocol_error');
});
