'use strict';
/* S29: the shell is rounded on ALL four corners (Sileo's pill family, 16px+), flush to the top edge, background alpha < 0.8 (dark and light),
   text/controls/mascot opaque and >= 4.5:1 over BOTH a busy dark and a busy light backdrop, nothing clipped by the radius.
   S30: no coloured outline on the blob rows; the selected row is still distinguishable; keyboard focus keeps a ring. Real layout. */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED: no playwright — NOT a pass'); process.exit(0); }
const parse = c => c.match(/[\d.]+/g).map(Number);
const lum = ([r, g, b]) => { const v = [r, g, b].map(x => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }); return 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]; };
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const over = (fg, a, bg) => fg.map((c, i) => Math.round(c * a + bg[i] * (1 - a)));
(async () => {
  for (const theme of ['dark', 'light']) {
    const r = await H.open({ github: true, theme, quick: true });
    try {
      const m = await r.page.evaluate(() => { const i = document.getElementById('island'), c = getComputedStyle(i), b = i.getBoundingClientRect();
        const ink = getComputedStyle(document.querySelector('.centre-name') || document.body).color, rows = [...document.querySelectorAll('.pill-choice')].filter(e => e.getBoundingClientRect().width > 0).map(e => { const s = getComputedStyle(e); return { bw: s.borderTopWidth, bc: s.borderTopColor, pressed: e.getAttribute('aria-pressed'), bg: s.backgroundColor, text: e.textContent.trim().slice(0, 12), mark: !!e.querySelector('.blob-mark') || /◂/.test(e.textContent) }; });
        const clay = getComputedStyle(document.getElementById('clay')).opacity;
        return { radii: [c.borderTopLeftRadius, c.borderTopRightRadius, c.borderBottomRightRadius, c.borderBottomLeftRadius].map(parseFloat), top: b.top, bg: c.backgroundColor, ink, rows, clay, w: b.width, h: b.height }; });
      assert.ok(m.radii.every(x => x >= 16), `${theme}: all four corners rounded >= 16px (Sileo family): ${m.radii}`);
      assert.equal(Math.round(m.top), 0, `${theme}: the shell is flush to the top edge (${m.top})`);
      const a = parse(m.bg)[3] ?? 1; assert.ok(a < 0.8 && a >= 0.6, `${theme}: shell alpha in [0.6,0.8): ${m.bg}`);
      assert.equal(+m.clay, 1, `${theme}: mascot opaque`);
      const shell = parse(m.bg).slice(0, 3), ink = parse(m.ink).slice(0, 3);
      for (const [name, bd] of [['busy dark', [20, 20, 24]], ['busy light', [245, 245, 235]], ['mid', [128, 128, 128]]]) { const eff = over(shell, a, bd); assert.ok(ratio(ink, eff) >= 4.5, `${theme}: text contrast over ${name}: ${ratio(ink, eff).toFixed(2)}`); }
      // S30
      for (const row of m.rows) assert.ok(row.bw === '0px' || /rgba\(0, 0, 0, 0\)|transparent/.test(row.bc), `${theme}: no coloured outline on «${row.text}» (${row.bw} ${row.bc})`);
      const sel = m.rows.filter(x => x.pressed === 'true' || x.mark); assert.ok(sel.length >= 1, `${theme}: a selected row exists`);
      const others = m.rows.filter(x => x.pressed !== 'true' && !x.mark);
      assert.ok(sel.some(x => x.mark || !others.some(o => o.bg === x.bg)), `${theme}: the selected row is distinguishable (mark or background)`);
      const ring = await r.page.evaluate(() => { const e = document.querySelector('.pill-choice'); e.focus({ focusVisible: true }); const s = getComputedStyle(e); return [s.outlineStyle, s.outlineWidth]; });
      assert.ok(ring[0] !== 'none' && parseFloat(ring[1]) >= 2, `${theme}: keyboard focus ring kept ${ring}`);
      const clip = await r.page.evaluate(() => { const i = document.getElementById('island').getBoundingClientRect(), R = 20; const out = []; const list = document.getElementById('pill-choices').getBoundingClientRect(); for (const e of document.querySelectorAll('#island-header .tab .ico, #island-header .hdr-action .ico, .pill-choice, .card-left, #overview-view')) { const b = e.getBoundingClientRect(); if (!b.width) continue; if (e.classList.contains('pill-choice') && (b.top < list.top - 0.5 || b.bottom > list.bottom + 0.5)) continue; const corners = [[b.left, b.top], [b.right, b.top], [b.left, b.bottom], [b.right, b.bottom]]; for (const [x, y] of corners) { const dx = Math.min(x - i.left, i.right - x), dy = Math.min(y - i.top, i.bottom - y); if (dx < R && dy < R && Math.hypot(R - dx, R - dy) > R + 0.5) out.push((e.id || e.className) + '@' + Math.round(x) + ',' + Math.round(y)); } } return out; });
      assert.deepEqual(clip, [], `${theme}: nothing is cut by the corner radius: ${clip}`);
    } finally { await r.close(); }
  }
  console.log('island-s29-check: ok');
})().catch(e => { console.error('FAIL', e.message, e.actual, e.expected); process.exit(1); });
