'use strict';
/* S5 (05 vacía, A): empty island = «Todo tranquilo» + one primary «Preguntar a Abito», Abito asleep; text fits on one line. Real layout. */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-s5-check: no playwright — NOT a pass'); process.exit(0); }
(async () => {
  const r = await H.open({ sessions: [], engine: 'account' });
  try {
    const t = await r.page.evaluate(() => ({ title: document.querySelector('.centre-empty-title').textContent, sub: document.querySelector('.centre-empty-sub').textContent, cta: document.getElementById('centre-cta').textContent.trim(),
      fits: [...document.querySelectorAll('.centre-empty-title, .centre-empty-sub, #centre-cta')].every(e => e.scrollWidth <= e.clientWidth + 1) }));
    assert.deepEqual([t.title, t.cta, t.fits], ['Todo tranquilo', 'Preguntar a Abito', true]);
    assert.ok(t.sub.length <= 34, 'one short line: ' + t.sub);
    const hit = (await r.hit('#centre-cta'))[0]; assert.ok(hit && hit.ok, 'the CTA receives its click');
    const primaries = await r.page.evaluate(() => [...document.querySelectorAll('#centre-empty .btn.primary')].filter(e => e.getBoundingClientRect().width > 0).length);
    assert.equal(primaries, 1, 'a single primary action');
    const st = await r.page.evaluate(() => window.__clayState && window.__clayState());
    if (st !== undefined) assert.equal(st, 'sleeping');
  } finally { await r.close(); }
  console.log('island-s5-check: ok');
})().catch(e => { console.error('FAIL', e.message); process.exit(1); });
