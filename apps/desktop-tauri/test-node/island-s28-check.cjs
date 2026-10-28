'use strict';
/* S28: the expanded island is 720x184 (was 640x160), its background is slightly see-through (Ghostty-like) while text, controls and the
   mascot stay opaque, and single-line texts fit on one line. Real css/scripts in Chromium. Run: node test-node/island-s28-check.cjs */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-s28-check: playwright is not installed here — NOT a pass'); process.exit(0); }

const ONE_LINE = ['.col-status span', '.centre-state', '.centre-steps li', '.centre-empty-title', '.centre-empty-sub', '#centre-cta', '.card-left > .who', '.pill-choice span:not(.ico)', '#blobs-footer span', '#island-header .tab'];
const alpha = c => { const m = /rgba?\(([^)]+)\)/.exec(c); if (!m) return 1; const p = m[1].split(/[ ,\/]+/).filter(Boolean); return p.length > 3 ? +p[3] : 1; };

(async () => {
  for (const theme of ['dark', 'light']) {
    // S28 is the BASE island (720x184). Since abito-avisos-ux S1 (human: «que salga abajo como sale los avisos de sin cuenta», commit
    // edb3bd76) retained notices add the 700x32 Avisos strip (+36 => 220), so the base size is measured with no notices.
    const r = await H.open({ github: true, notices: 0, theme, engine: 'ok' });
    try {
      const isl = (await r.measure('#island'))[0];
      assert.equal(Math.round(isl.w), 720, `${theme}: island width`);
      assert.equal(Math.round(isl.h), 184, `${theme}: island height`);
      assert.equal(Math.round(isl.left), 16, `${theme}: centred in the 752 window`);
      const a = await r.page.evaluate(() => {
        const bg = getComputedStyle(document.getElementById("island")).backgroundColor;
        const op = ['#island', '#clay', '.col-status', '.pill-choice', '#island-header'].map(s => { const e = document.querySelector(s); return e ? [s, getComputedStyle(e).opacity] : null; }).filter(Boolean);
        return { bg, op };
      });
      const al = alpha(a.bg);
      assert.ok(al < 0.8 && al >= 0.6, `${theme}: island background alpha in [0.6,0.8) (S29: visibly translucent), got ${a.bg}`);
      for (const [s, o] of a.op) if (s !== '#island-header' || +o) assert.equal(+o, 1, `${theme}: ${s} stays fully opaque (opacity ${o})`);
      const bad = await r.page.evaluate(sels => sels.flatMap(sel => [...document.querySelectorAll(sel)].filter(el => {
        const b = el.getBoundingClientRect(), s = getComputedStyle(el); return s.display !== 'none' && s.visibility !== 'hidden' && b.width > 0 && el.scrollWidth > el.clientWidth + 1;
      }).map(el => `${sel} «${el.textContent.trim().slice(0, 30)}» ${el.scrollWidth}>${el.clientWidth}`)), ONE_LINE);
      assert.deepEqual(bad, [], `${theme}: texts overflow their line: ${bad.join(' | ')}`);
      const below = (await r.measure('.card-left'))[0];
      assert.ok(below && below.bottom <= 174.5 && below.h >= 131, `${theme}: card row is 132 high (y42..174), got ${JSON.stringify(below && [below.top, below.bottom])}`);
    } finally { await r.close(); }
  }
  {
    const r = await H.open({ github: true, notices: 2, theme: 'dark', engine: 'ok' });
    try {
      const isl = (await r.measure('#island'))[0];
      assert.equal(Math.round(isl.w), 720, 'with notices: width unchanged');
      assert.equal(Math.round(isl.h), 220, 'with notices: the Avisos strip adds 36px (abito-avisos-ux S1)');
    } finally { await r.close(); }
  }
  console.log('island-s28-check: ok');
})().catch(e => { console.error('FAIL', e.message); process.exit(1); });
