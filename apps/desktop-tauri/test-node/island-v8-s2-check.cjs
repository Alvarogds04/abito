'use strict';
/* v8 S2 (02 compacta A): up to 3 minis, EACH with its own colour (14 Pi sessions must not be three identical greens), «+N» for the rest,
   and the «•••» state badge on the mascot while the focused session works. Real layout. */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED: no playwright — NOT a pass'); process.exit(0); }
(async () => {
  const r = await H.open({ manyPi: true, quick: true });
  try {
    await r.page.evaluate(() => window.__islandSetState('compact')); await r.page.waitForTimeout(1800);
    const m = await r.page.evaluate(() => ({
      minis: [...document.querySelectorAll('#mini-grid .mini')].filter(e => e.getBoundingClientRect().width > 0).map(e => e.querySelector('svg').dataset.color),
      more: (document.querySelector('.mini-more') || {}).textContent,
      stop: [...document.querySelectorAll('#mini-grid .mini svg')].map(s => s.style.getPropertyValue('--chosen-2').trim()),
      badge: (() => { const b = document.getElementById('compact-badge'), r = b.getBoundingClientRect(), i = document.getElementById('island').getBoundingClientRect(); return { hidden: b.hidden, shown: r.width > 0, kind: b.dataset.kind, inside: r.left >= i.left && r.right <= i.right && r.top >= i.top && r.bottom <= i.bottom, w: r.width }; })() }));
    assert.equal(m.minis.length, 3, `three minis (${m.minis})`);
    assert.equal(new Set(m.minis).size, 3, `each mini has its own colour: ${m.minis}`);
    assert.equal(m.more, '+11');
    assert.ok(m.badge.shown && m.badge.inside && m.badge.kind === 'working', `state badge visible on the mascot ${JSON.stringify(m.badge)}`);
  } finally { await r.close(); }
  console.log('island-v8-s2-check: ok');
})().catch(e => { console.error('FAIL', e.message, e.actual, e.expected); process.exit(1); });
