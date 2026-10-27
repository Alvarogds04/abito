'use strict';
// HU-047: Windows flavour of the default Pi extension install. Same closed payload and manifest ownership as
// install-pi.cjs; every check and write goes through the native ACL helper (win-fs.cjs). Cross-volume publication
// is refused, never copied. Nothing here runs on Linux/macOS.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const winfs = require('./win-fs.cjs');
const core = require('./install-claude.cjs');

const MANIFEST = '.abito-manifest.json';
const SCHEMA = 'abito-pi-extension/1';
const FILES = Object.freeze(['adapter.cjs', 'ask-tool.cjs', 'config.cjs', 'index.ts', 'native-ask.cjs', 'questions.cjs', 'security.cjs', 'windows-transport.cjs']);
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
const refuse = () => core.InstallErrorRefuse('Pi extension ownership or path could not be verified');
const listing = files => JSON.stringify(files);
const sourceRel = name => `desktop-pi/${name === 'index.ts' ? 'pi-extension.ts' : name}`;
const drive = p => p.slice(0, 2).toLowerCase();
const rand = () => crypto.randomBytes(6).toString('hex');

function dirFact(w, dir) {
  const f = w.checkPath(dir);
  if (!f.exists) return null;
  if (f.reparse || f.kind !== 'dir' || !f.private) throw refuse();
  return f;
}
function parents(w, home, create = false) {
  const h = w.checkPath(path.join(home, '.pi'));
  if (!h.parent_ok) throw refuse();
  let dir = home, missing = false;
  for (const part of ['.pi', 'agent', 'extensions']) {
    dir = path.join(dir, part);
    if (missing && !create) continue;
    if (dirFact(w, dir) === null) { if (create) w.makeDir(dir); else missing = true; }
  }
  return dir;
}
function owned(w, target) {
  if (dirFact(w, target) === null) return null;
  if (listing(fs.readdirSync(target).sort()) !== listing([...FILES, MANIFEST].sort())) throw refuse();
  const rawFile = w.readPrivate(path.join(target, MANIFEST));
  if (!rawFile || rawFile.size > 16384) throw refuse();
  const raw = rawFile.buf;
  let manifest;
  try { manifest = JSON.parse(raw.toString('utf8')); } catch { throw refuse(); }
  if (![JSON.stringify(manifest), JSON.stringify(manifest) + '\n'].includes(raw.toString('utf8'))) throw refuse();
  if (!manifest || Object.keys(manifest).sort().join(',') !== 'files,schema' || manifest.schema !== SCHEMA || !manifest.files || listing(Object.keys(manifest.files).sort()) !== listing(FILES)) throw refuse();
  for (const name of FILES) {
    const got = w.readPrivate(path.join(target, name));
    if (!got || !/^[a-f0-9]{64}$/.test(manifest.files[name]) || got.sha256 !== manifest.files[name]) throw refuse();
  }
  return { files: manifest.files, signature: hash(Buffer.concat([raw, Buffer.from(listing(manifest.files))])) };
}
function payload() {
  const data = {}, files = {};
  for (const name of FILES) {
    data[name] = winfs.readVerified(sourceRel(name));
    files[name] = hash(data[name]);
  }
  data[MANIFEST] = Buffer.from(JSON.stringify({ schema: SCHEMA, files }) + '\n');
  return { data, files };
}
function backupRoot(w, home, create = false) {
  const root = path.join(home, '.pi', 'agent', 'abito-backups');
  if (dirFact(w, root) === null) {
    if (!create) return root;
    w.makeDir(root);
  }
  return root;
}
function cleanStage(w, dir) {
  if (!dir) return;
  try {
    for (const name of [...FILES, MANIFEST]) { try { if (w.checkPath(path.join(dir, name)).exists) w.remove(path.join(dir, name)); } catch { /* own missing file */ } }
    w.remove(dir);
  } catch { /* leave evidence, never recurse */ }
}
function pruneBackups(w, root, keep) {
  for (const name of fs.readdirSync(root)) {
    if (!/^abito-island\.abito-backup-[0-9TZ]+(?:-[1-9])?$/.test(name)) continue;
    const dir = path.join(root, name);
    if (dir === keep) continue;
    try { if (owned(w, dir)) cleanStage(w, dir); } catch { /* preserve foreign/modified backup */ }
  }
}
function planPi({ home, remove, execPath, binPath }) {
  const w = winfs.active();
  const parent = parents(w, home);
  const target = path.join(parent, 'abito-island');
  const root = backupRoot(w, home);
  const current = owned(w, target), next = payload();
  const before = current ? JSON.stringify(current.files, null, 2) : '';
  const after = remove ? '' : JSON.stringify(next.files, null, 2);
  const changed = remove ? !!current : before !== after;
  return { settingsPath: target, home, uid: 'win', remove, execPath, binPath, timeoutSec: null,
    backupRoot: root, backupTarget: path.join(root, 'abito-island'),
    lockTarget: path.join(home, '.abito-pi-extension'), exists: !!current,
    sig: current ? { sha256: current.signature, size: Buffer.byteLength(before) } : null,
    before, after, changed, data: next.data, files: next.files,
    actions: [{ event: 'extension', action: !changed ? 'unchanged' : remove ? 'remove' : current ? 'replace' : 'add' }] };
}
function same(w, plan) {
  parents(w, plan.home);
  const now = owned(w, plan.settingsPath);
  return (now?.signature || '') === (plan.sig?.sha256 || '');
}
async function applyPi(plan, { backupPath, out, err }) {
  const w = winfs.active();
  let stage = null, saved = false, published = false;
  try {
    if (!same(w, plan)) { err('changed since the preview'); return 70; }
    const parent = parents(w, plan.home, true);
    const root = backupRoot(w, plan.home, true);
    // Never copy across volumes: a cross-volume publication is refused outright.
    if (root !== plan.backupRoot || drive(root) !== drive(parent) || drive(backupPath || root) !== drive(parent)) throw refuse();
    if (!plan.remove) {
      stage = path.join(root, `.abito-island-stage-${rand()}`);
      w.makeDir(stage);
      for (const [name, data] of Object.entries(plan.data)) w.writePrivate(path.join(stage, name), data, null);
      if (listing(owned(w, stage)?.files) !== listing(plan.files)) throw refuse();
    }
    if (!same(w, plan)) { err('changed since the preview'); return 70; }
    if (plan.exists) { w.move(plan.settingsPath, backupPath); saved = true; }
    if (!plan.remove) {
      w.move(stage, plan.settingsPath); stage = null; published = true;
      if (listing(owned(w, plan.settingsPath)?.files) !== listing(plan.files)) throw refuse();
    }
    if (saved) pruneBackups(w, root, backupPath);
    out('Done. Pi extension updated.\n');
    return 0;
  } catch {
    const verificationFailed = published;
    try {
      if (published) { w.move(plan.settingsPath, path.join(plan.backupRoot, `.abito-island-quarantine-${rand()}`)); published = false; }
      if (saved) { w.move(backupPath, plan.settingsPath); saved = false; }
    } catch { err('could not restore'); return 70; }
    err(verificationFailed ? 'verification failed; previous state restored' : 'could not apply Pi extension');
    return 70;
  } finally { cleanStage(w, stage); }
}
module.exports = { planPi, applyPi, FILES, MANIFEST, SCHEMA };
