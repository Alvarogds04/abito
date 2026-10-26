'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { extractPlan, writeChannel, readPrevious, channelPath, previousPath, runPrevious } = require('../src/statusline.cjs');

const NOW = 1_800_000_000_000;
const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'abito-sl-'));
const BIN = path.resolve(__dirname, '../bin/abito-statusline.cjs');

test('only the two windows survive; every other field of the status JSON is dropped', () => {
  const plan = extractPlan({
    cwd: '/secret/project', model: { id: 'x' }, transcript_path: '/t', session_id: 's',
    rate_limits: { five_hour: { used_percentage: 12.5, resets_at: 1_800_003_600, junk: 1 }, seven_day: { used_percentage: 40, resets_at: 1_800_500_000 }, extra: 'x' },
  }, NOW);
  assert.deepEqual(plan, { v: 1, at: NOW, five_hour: { used_percentage: 12.5, resets_at: 1_800_003_600 }, seven_day: { used_percentage: 40, resets_at: 1_800_500_000 } });
});

test('no limits, bad numbers or ms-in-disguise epochs give null', () => {
  for (const input of [null, 1, {}, { rate_limits: null }, { rate_limits: {} },
    { rate_limits: { five_hour: { used_percentage: 'a', resets_at: 1 } } },
    { rate_limits: { five_hour: { used_percentage: -1, resets_at: 1_800_003_600 } } },
    { rate_limits: { five_hour: { used_percentage: 201, resets_at: 1_800_003_600 } } },
    { rate_limits: { five_hour: { used_percentage: 5, resets_at: 1_800_003_600_000 } } }]) {
    assert.equal(extractPlan(input, NOW), null, JSON.stringify(input));
  }
});

test('the channel is written 0600 atomically and holds the plan only', () => {
  const rt = tmp();
  const plan = extractPlan({ rate_limits: { five_hour: { used_percentage: 1, resets_at: 1_800_003_600 } } }, NOW);
  assert.equal(writeChannel(rt, plan), true);
  const file = channelPath(rt);
  assert.equal(fs.statSync(file).mode & 0o777, 0o600);
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), plan);
  assert.deepEqual(fs.readdirSync(rt), ['plan-claude.json']);
});

test('a symlink at the channel path is replaced, never written through', () => {
  const rt = tmp();
  const victim = path.join(tmp(), 'victim');
  fs.writeFileSync(victim, 'keep');
  fs.symlinkSync(victim, channelPath(rt));
  const plan = extractPlan({ rate_limits: { five_hour: { used_percentage: 1, resets_at: 1_800_003_600 } } }, NOW);
  assert.equal(writeChannel(rt, plan), true);
  assert.equal(fs.readFileSync(victim, 'utf8'), 'keep');
  assert.equal(fs.lstatSync(channelPath(rt)).isFile(), true);
});

test('an unusable runtime directory fails quietly', () => {
  assert.equal(writeChannel('/nonexistent-abito-dir/x', { v: 1 }), false);
});

test('the previous status line is read only when well formed', () => {
  const home = tmp();
  assert.equal(readPrevious(home), null);
  fs.mkdirSync(path.dirname(previousPath(home)), { recursive: true });
  fs.writeFileSync(previousPath(home), JSON.stringify({ type: 'command', command: 'echo hi' }));
  assert.equal(readPrevious(home), 'echo hi');
  fs.writeFileSync(previousPath(home), JSON.stringify({ command: '  ' }));
  assert.equal(readPrevious(home), null);
  fs.writeFileSync(previousPath(home), 'not json');
  assert.equal(readPrevious(home), null);
});

test('the previous command gets the same stdin and its output is kept, capped and time-boxed', () => {
  assert.equal(runPrevious('cat', Buffer.from('hello'), 3000).toString(), 'hello');
  assert.equal(runPrevious('sleep 5; echo late', Buffer.from(''), 300), null);
  assert.ok(runPrevious('head -c 200000 /dev/zero', Buffer.from(''), 3000).length <= 64 * 1024);
});

test('the script prints the previous line untouched and writes the channel', () => {
  const rt = tmp();
  const home = tmp();
  fs.mkdirSync(path.dirname(previousPath(home)), { recursive: true });
  fs.writeFileSync(previousPath(home), JSON.stringify({ type: 'command', command: 'printf "[%s]" "$(head -c 5)"' }));
  const at = Math.floor(Date.now() / 1000);
  const input = JSON.stringify({ cwd: '/p', hello: 'world', rate_limits: { five_hour: { used_percentage: 3, resets_at: at + 3600 } } });
  const r = spawnSync(process.execPath, [BIN], { input, env: { ...process.env, ABITO_SOCKET_DIR: rt, HOME: home }, encoding: 'utf8' });
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '[{"cwd]');
  const written = JSON.parse(fs.readFileSync(channelPath(rt), 'utf8'));
  assert.equal(written.five_hour.used_percentage, 3);
  assert.equal(JSON.stringify(written).includes('/p'), false);
});

test('garbage input or no runtime never fails the status line', () => {
  const r = spawnSync(process.execPath, [BIN], { input: 'not json', env: { PATH: process.env.PATH, HOME: tmp(), ABITO_SOCKET_DIR: '/nonexistent-abito-dir/x' }, encoding: 'utf8' });
  assert.equal(r.status, 0);
  assert.equal(r.stdout, '');
});
