'use strict';
const { tmpBase } = require('./platform-env.cjs');
// HU-047: the Windows install flow (audit, lock, settings, statusline side file, Pi) driven through a fake helper
// backed by a POSIX temp dir. The fake enforces the same contract the native helper does (private parent, no
// symlinks, expectation hashes) so a bypass in the Node flow shows up here without a Windows machine.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const winfs = require('../src/win-fs.cjs');
const ui = require('../src/install-ui.cjs');

const sha = b => crypto.createHash('sha256').update(b).digest('hex');
function fakeBackend(log = []) {
  const err = r => new winfs.WinFsError(r);
  const parentOk = p => { try { const st = fs.lstatSync(path.dirname(p)); return st.isDirectory() && !st.isSymbolicLink() && (st.mode & 0o022) === 0; } catch { return false; } };
  const facts = p => {
    let st = null; try { st = fs.lstatSync(p); } catch { /* absent */ }
    return { schema: 'abito-fs/1', ok: true, path: p, parent_ok: parentOk(p), exists: !!st, kind: !st ? 'none' : st.isDirectory() ? 'dir' : st.isFile() ? 'file' : 'other',
      reparse: !!st && st.isSymbolicLink(), private: !!st && !st.isSymbolicLink() && (st.mode & 0o077) === 0, nlink: st ? st.nlink : 0, size: st ? st.size : 0, mtime_ms: st ? Math.floor(st.mtimeMs) : 0, elevated: false };
  };
  const b = {
    checkPath: p => { log.push(['check', p]); return facts(p); },
    readPrivate(p, { readFile = fs.readFileSync } = {}) {
      const f = facts(p);
      if (!f.parent_ok) throw err('not_private');
      if (!f.exists) return null;
      if (f.reparse) throw err('reparse');
      if (f.kind !== 'file' || !f.private || f.nlink !== 1) throw err('not_private');
      const buf = readFile(p);
      return { buf, sha256: sha(buf), size: buf.length, mtimeMs: f.mtime_ms };
    },
    writePrivate(p, data, expect) {
      log.push(['write', p]);
      const f = facts(p);
      if (!f.parent_ok) throw err('not_private');
      if (expect === null || expect === undefined) { if (f.exists) throw err('exists'); }
      else { if (!f.exists) throw err('missing'); if (f.reparse || !f.private) throw err('not_private'); if (sha(fs.readFileSync(p)) !== expect) throw err('mismatch'); }
      const tmp = path.join(path.dirname(p), `.t${process.pid}${Math.random()}`);
      fs.writeFileSync(tmp, data, { mode: 0o600, flag: 'wx' });
      fs.renameSync(tmp, p);
      return { sha256: sha(Buffer.from(data)) };
    },
    makeDir(p) { if (!parentOk(p)) throw err('not_private'); if (fs.existsSync(p)) return { created: false }; fs.mkdirSync(p, { mode: 0o700 }); return { created: true }; },
    move(a, c) { if (fs.existsSync(c)) throw err('exists'); fs.renameSync(a, c); return { ok: true }; },
    remove(p) { const st = fs.lstatSync(p); if (st.isDirectory()) fs.rmdirSync(p); else fs.unlinkSync(p); return { ok: true }; },
  };
  return b;
}
function tmp(t) {
  const d = fs.realpathSync(fs.mkdtempSync(path.join(tmpBase(), 'abito-winfake-')));
  fs.chmodSync(d, 0o700);
  t.after(() => { winfs.useForTests(null); fs.rmSync(d, { recursive: true, force: true }); });
  return d;
}
async function run(argv, extra = {}) {
  const lines = [];
  const code = await ui.runInstallUi(argv, { out: s => lines.push(s), env: {}, ...extra });
  return { code, json: lines.length === 1 ? JSON.parse(lines[0]) : null, lines };
}
const guards = { readExe: () => '/x' };

test('Windows flow: preview then apply a Claude settings file through the helper only (audit, lock, atomic write)', async t => {
  const d = tmp(t); const log = [];
  winfs.useForTests(fakeBackend(log));
  const settings = path.join(d, 'settings.json'); const audit = path.join(d, 'audit');
  fs.writeFileSync(settings, '{\n  "a": 1\n}\n', { mode: 0o600 });
  const pv = (await run(['claude', '--settings', settings, '--json'])).json;
  assert.equal(pv.ok, true); assert.equal(pv.changed, true);
  const exe = fs.realpathSync(process.execPath);
  const ap = await run(['claude', '--settings', settings, '--json', '--apply-plan', pv.plan_hash, '--audit-dir', audit], { env: { ABITO_ISLAND_EXE: exe }, readExe: () => exe });
  assert.equal(ap.json.ok, true, JSON.stringify(ap.json));
  assert.equal(ap.json.rollback, null);
  assert.match(fs.readFileSync(settings, 'utf8'), /abito-hook/);
  assert.equal(JSON.parse(fs.readFileSync(settings, 'utf8')).a, 1);
  const auditLines = fs.readFileSync(path.join(audit, 'hook-installs.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(auditLines.map(l => l.event), ['intent', 'result']);
  assert.ok(log.some(([k, p]) => k === 'write' && p === settings), 'settings written through the helper');
  assert.ok(fs.readdirSync(d).some(n => n.includes('.abito-backup-')), 'backup exists');
  assert.ok(!fs.readdirSync(d).some(n => n.includes('abito-lock')), 'lock released');
});

test('Windows flow: a symlinked settings file or a non-private parent writes nothing and reports a fixed code', async t => {
  const d = tmp(t);
  winfs.useForTests(fakeBackend());
  const real = path.join(d, 'real.json'); fs.writeFileSync(real, '{}\n', { mode: 0o600 });
  const link = path.join(d, 'settings.json'); fs.symlinkSync(real, link);
  const r1 = await run(['claude', '--settings', link, '--json']);
  assert.equal(r1.json.ok, false); assert.equal(r1.json.code, 'unsafe_target');
  assert.equal(fs.readFileSync(real, 'utf8'), '{}\n');
  const open = path.join(d, 'open'); fs.mkdirSync(open); fs.chmodSync(open, 0o777);
  const r2 = await run(['claude', '--settings', path.join(open, 'settings.json'), '--json']);
  assert.equal(r2.json.ok, false); assert.equal(r2.json.code, 'unsafe_folder');
  assert.equal(r2.json.message, `La carpeta ${open} tiene permisos de Windows (ACL) que dan acceso a otros usuarios o no es tuya; dejala accesible solo para tu usuario (Propiedades > Seguridad) y volvé a intentar.`);
  assert.deepEqual(fs.readdirSync(open), []);
});

test('Windows flow: Pi extension installs, replaces and removes through the helper, and refuses another drive', async t => {
  const d = tmp(t); const home = path.join(d, 'home'); fs.mkdirSync(home, { mode: 0o700 });
  winfs.useForTests(fakeBackend());
  const verified = winfs.readVerified;
  // payload bytes come from the manifest-verified reader; here the fake returns the real source bytes
  winfs.readVerified = rel => fs.readFileSync(path.resolve(__dirname, '../..', rel));
  t.after(() => { winfs.readVerified = verified; });
  const piwin = require('../src/install-pi-win.cjs');
  fs.mkdirSync(path.join(home, '.pi', 'agent'), { recursive: true, mode: 0o700 });
  const plan = piwin.planPi({ home, remove: false, execPath: process.execPath, binPath: '/x' });
  assert.equal(plan.changed, true);
  let msg = '';
  assert.equal(await piwin.applyPi(plan, { backupPath: null, out: () => {}, err: s => { msg += s; } }), 0, msg);
  const target = path.join(home, '.pi', 'agent', 'extensions', 'abito-island');
  assert.deepEqual(fs.readdirSync(target).sort(), [...piwin.FILES, piwin.MANIFEST].sort());
  const again = piwin.planPi({ home, remove: false, execPath: process.execPath, binPath: '/x' });
  assert.equal(again.changed, false);
  const rm = piwin.planPi({ home, remove: true, execPath: process.execPath, binPath: '/x' });
  const bk = path.join(rm.backupRoot, 'abito-island.abito-backup-20260101T000000000Z');
  assert.equal(await piwin.applyPi(rm, { backupPath: bk, out: () => {}, err: () => {} }), 0);
  assert.equal(fs.existsSync(target), false);
});

test('Windows flow (verifier M1): the saved status-line file is read only through the helper checks; a foreign or linked one is ignored', t => {
  const d = tmp(t);
  winfs.useForTests(fakeBackend());
  const statusline = require('../src/install-statusline.cjs');
  fs.mkdirSync(path.join(d, '.abito'), { mode: 0o700 });
  const file = path.join(d, '.abito', 'statusline-previous.json');
  fs.writeFileSync(file, '{"type":"command","command":"evil"}\n', { mode: 0o666 }); // not private: the helper contract refuses it
  const settings = path.join(d, 'settings.json');
  fs.writeFileSync(settings, '{"statusLine":{"type":"command","command":"node /x/abito-statusline.cjs"}}\n', { mode: 0o600 });
  const plan = statusline.planStatusLine({ settingsPath: settings, remove: true, home: d, binPath: '/x/abito-statusline.cjs', execPath: process.execPath, uid: 'win' });
  assert.ok(!/evil/.test(plan.after), 'a non-private side file must never be restored into settings');
  fs.chmodSync(file, 0o600);
  const ok = statusline.planStatusLine({ settingsPath: settings, remove: true, home: d, binPath: '/x/abito-statusline.cjs', execPath: process.execPath, uid: 'win' });
  assert.match(ok.after, /evil/, 'a private side file is still honoured');
});

test('L6: on Windows the status-line command is double-quoted for cmd and a path with an apostrophe or quote stays safe', t => {
  const d = tmp(t);
  winfs.useForTests(fakeBackend());
  const statusline = require('../src/install-statusline.cjs');
  const settings = path.join(d, 'settings.json');
  fs.writeFileSync(settings, '{}\n', { mode: 0o600 });
  const plan = statusline.planStatusLine({ settingsPath: settings, home: d, binPath: 'C:\\Users\\O\'Neil\\Abito\\abito-statusline.cjs', execPath: 'C:\\Program Files\\nodejs\\node.exe', uid: 'win' });
  assert.match(plan.after, /"command": "\\"C:\\\\Program Files\\\\nodejs\\\\node.exe\\" \\"C:\\\\Users\\\\O'Neil\\\\Abito\\\\abito-statusline.cjs\\""/);
  assert.throws(() => statusline.planStatusLine({ settingsPath: settings, home: d, binPath: 'C:\\a"b\\abito-statusline.cjs', execPath: 'C:\\n.exe', uid: 'win' }), /refused|unsafe/);
});

test('L7: mismatch/busy/io/exists/missing helper reasons get their own Ajustes detail instead of unsafe_target', async t => {
  const d = tmp(t);
  const settings = path.join(d, 'settings.json'); fs.writeFileSync(settings, '{}\n', { mode: 0o600 });
  for (const [reason, code] of [['mismatch', 'plan_changed'], ['busy', 'apply_failed'], ['io', 'apply_failed'], ['not_private', 'unsafe_target'], ['reparse', 'unsafe_target']]) {
    const b = fakeBackend();
    b.readPrivate = () => { throw new winfs.WinFsError(reason); };
    winfs.useForTests(b);
    const r = await run(['claude', '--settings', settings, '--json']);
    assert.equal(r.json.code, code, reason);
  }
});
