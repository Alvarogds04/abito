'use strict';
// D1: an idle session's expanded overview shows «En reposo» exactly once (centre card OR status column, never both). (real layout, playwright)
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-idle-word-check: no Playwright — NOT a pass'); process.exit(0); }
(async () => {
  let passed = 0, failed = 0;
  const test = async (name, fn) => { try { await fn(); passed++; console.log('PASS ' + name); } catch (e) { failed++; console.error('FAIL ' + name + ': ' + e.message); } };
  const visibleWords = r => r.page.evaluate(() => [...document.querySelectorAll('#overview-view *')].filter(el => {
    if (el.children.length || (el.textContent || '').trim() !== 'En reposo') return false;
    const b = el.getBoundingClientRect(), s = getComputedStyle(el);
    return b.width > 0 && b.height > 0 && s.display !== 'none' && s.visibility !== 'hidden' && +s.opacity > 0;
  }).map(el => el.className || el.parentElement.className));
  const cases = [
    ['idle session', { sessions: [{ id: 's-0000000c', kind: 'pi', cwd: 'repo', state: 'sleeping', origin: null }], quick: true }, 1],
  ];
  for (const [name, scenario, want] of cases) {
    const r = await H.open(scenario);
    try { await test(`${name}: «En reposo» appears ${want} time(s) in the expanded overview`, async () => assert.equal((await visibleWords(r)).length, want, JSON.stringify(await visibleWords(r)))); }
    finally { await r.close(); }
  }
  console.log(`island-idle-word-check: ${passed} passed, ${failed} failed`);
  if (failed) process.exitCode = 1;
})().catch(e => { console.error(e); process.exitCode = 1; });
