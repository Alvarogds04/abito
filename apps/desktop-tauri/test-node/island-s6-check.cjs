'use strict';
/* S6 (06 pregunta simple, B): options with a title (13px) and a grey one-line description (11px, >= 4.5:1), whole rows, nothing overflows,
   every option clickable, and the card fits the island. Real layout, dark and light. */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-s6-check: no playwright — NOT a pass'); process.exit(0); }
const lum = c => { const v = c.match(/[\d.]+/g).slice(0, 3).map(Number).map(x => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }); return 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]; };
const toRgb = c => { const n = c.match(/[\d.]+/g).map(Number); return /^color\(srgb/.test(c) ? `rgb(${n.slice(0, 3).map(x => Math.round(x * 255)).join(',')})` : c; };
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
(async () => {
  for (const theme of ['dark', 'light']) {
    const r = await H.open({ question: 'plain', described: true, theme });
    try {
      const m = await r.page.evaluate(() => {
        const l = getComputedStyle(document.querySelector('.q-label')), d = document.querySelector('.q-desc');
        const isl = document.getElementById('island').getBoundingClientRect(), q = document.getElementById('question').getBoundingClientRect();
        const opts = [...document.querySelectorAll('.q-option')].filter(e => e.getBoundingClientRect().width > 0).map(e => { const b = e.getBoundingClientRect(); return [b.top, b.bottom]; });
        return { label: [l.fontSize, l.fontWeight], desc: d && getComputedStyle(d).fontSize, descColor: d && getComputedStyle(d).color, bg: getComputedStyle(document.getElementById('question')).backgroundColor, descs: document.querySelectorAll('.q-desc').length,
          descOverflow: [...document.querySelectorAll('.q-desc')].some(e => e.scrollWidth > e.clientWidth + 1 || e.getBoundingClientRect().height > 16), inside: q.bottom <= isl.bottom + 0.5, opts };
      });
      assert.equal(m.label[0], '13px', `${theme}: option title 13px`);
      assert.equal(m.desc, '11px', `${theme}: description 11px`);
      assert.ok(m.descs >= 2, `${theme}: descriptions drawn`);
      assert.equal(m.descOverflow, false, `${theme}: each description is ONE line`);
      assert.ok(m.inside, `${theme}: the card stays inside the island`);
      const card = theme === 'dark' ? 'rgb(20,21,24)' : 'rgb(247,242,231)';
      assert.ok(ratio(m.descColor, m.bg.startsWith('rgba(0, 0, 0, 0)') ? card : toRgb(m.bg)) >= 4.5, `${theme}: description contrast ${m.descColor} on ${m.bg}`);
      const blocked = (await r.hit('.q-option')).filter(h => !h.ok);
      assert.deepEqual(blocked, [], `${theme}: options receive their own click`);
    } finally { await r.close(); }
  }
  console.log('island-s6-check: ok');
})().catch(e => { console.error('FAIL', e.message); process.exit(1); });
