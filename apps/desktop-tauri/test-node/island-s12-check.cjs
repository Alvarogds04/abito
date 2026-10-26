'use strict';
/* S12 (12 «Ver todas», B): the full list is a GRID of coloured mascots; every cell >= 44x44, names on one line, whole rows, no overlap,
   inside the card; a click focuses that session and closes the grid. Real layout. */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-s12-check: no playwright — NOT a pass'); process.exit(0); }
(async () => {
  {
    // v8: «Todas» is anchored to the blobs card (bottom of that column, per the MD), not loose under the mascot; long names show whole in the island tip
    const r = await H.open({ manyPi: true });
    try {
      const sa = (await r.measure('#blobs-links .see-all'))[0], bl = (await r.measure('#blobs'))[0], list = (await r.measure('#pill-choices'))[0];
      assert.ok(sa.shown, '«Todas» is shown');
      assert.ok(sa.left >= bl.left - 0.5 && sa.right <= bl.right + 0.5 && sa.bottom <= bl.bottom + 0.5 && sa.top >= list.bottom - 0.5, `«Todas» sits at the bottom of the blobs card, under the list [${Math.round(sa.left)},${Math.round(sa.top)},${Math.round(sa.right)},${Math.round(sa.bottom)}] in blobs [${Math.round(bl.left)},${Math.round(bl.top)},${Math.round(bl.right)},${Math.round(bl.bottom)}]`);
      const rows = (await r.measure('.pill-choice')).filter(b => b.shown).filter(b => b.top >= list.top - 0.5 && b.bottom <= list.bottom + 0.5);
      assert.ok(rows.length >= 3, `still 3 whole blob rows (${rows.length})`);
      assert.deepEqual((await r.hit('.see-all')).filter(h => !h.ok), [], '«Todas» gets its click');
    } finally { await r.close(); }
  }
  {
    const long = 'isla-expandida-con-un-nombre-muy-largo';
    const r = await H.open({ sessions: [{ id: 's-00000001', kind: 'pi', cwd: long, state: 'running', origin: null }, { id: 's-00000002', kind: 'pi', cwd: 'settings-pantalla-ajustes-generales', state: 'sleeping', origin: null }] });
    try {
      const sa = (await r.measure('#blobs-links .see-all'))[0]; await r.page.mouse.click(sa.left + sa.w / 2, sa.top + sa.h / 2); await r.page.waitForTimeout(700);
      const c = (await r.measure('.sessions-all-pick')).filter(b => b.shown)[0];
      await r.page.mouse.move(c.left + c.w / 2, c.top + c.h / 2); await r.page.waitForTimeout(700);
      const tip = await r.page.evaluate(() => { const t = document.getElementById('island-tip'), b = t.getBoundingClientRect(), i = document.getElementById('island').getBoundingClientRect(); return { shown: !t.hidden && b.width > 0, text: t.textContent, inside: b.left >= i.left - 0.5 && b.right <= i.right + 0.5 && b.bottom <= i.bottom + 0.5, z: getComputedStyle(t).zIndex }; });
      assert.ok(tip.shown && tip.text.includes(long) && tip.inside, `the tip shows the full name inside the island: ${JSON.stringify(tip)}`);
    } finally { await r.close(); }
  }
  const r = await H.open({});
  try {
    const sa = (await r.measure('#blobs-links .see-all'))[0]; assert.ok(sa && sa.shown, '«Todas» link is drawn');
    await r.page.mouse.click(sa.left + sa.w / 2, sa.top + sa.h / 2); await r.page.waitForTimeout(900);
    const cells = (await r.measure('.sessions-all-pick')).filter(b => b.shown);
    assert.ok(cells.length >= 5, `a cell per session (${cells.length})`);
    assert.ok(cells.every(b => b.w >= 44 && b.h >= 44), `cells >= 44x44: ${cells.map(b => Math.round(b.w) + 'x' + Math.round(b.h))}`);
    const list = (await r.measure('.sessions-all-list'))[0];
    const cols = new Set(cells.map(b => Math.round(b.left))).size; assert.ok(cols >= 2, `a grid, not a column (${cols} columns)`);
    const o = [];
    for (let i = 0; i < cells.length; i++) for (let j = i + 1; j < cells.length; j++) { const a = cells[i], b = cells[j]; if (Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1) o.push(i + 'x' + j); }
    assert.deepEqual(o, [], 'cells do not overlap');
    const vis = cells.filter(b => b.top >= list.top - 0.5 && b.bottom <= list.bottom + 0.5); assert.ok(vis.length >= 4, `at least 4 whole cells visible (${vis.length})`);
    const partial = cells.filter(b => { const v = Math.min(b.bottom, list.bottom) - Math.max(b.top, list.top); return v > 0.5 && v < b.h - 0.5; }).length; assert.equal(partial, 0, 'no cell is cut by the bottom');
    const m = await r.page.evaluate(() => ({ svgs: document.querySelectorAll('.sessions-all-pick .sa-mascot svg').length, names: [...document.querySelectorAll('.sessions-all-pick .sa-cwd')].every(e => e.scrollWidth <= e.clientWidth + 1 || getComputedStyle(e).textOverflow === 'ellipsis') }));
    assert.ok(m.svgs >= 5, 'a mascot per cell'); assert.ok(m.names);
    assert.deepEqual((await r.hit('.sessions-all-pick')).filter(h => !h.ok), [], 'cells receive their own click');
    const c = cells[1]; await r.page.mouse.click(c.left + c.w / 2, c.top + c.h / 2); await r.page.waitForTimeout(600);
    assert.equal(await r.page.evaluate(() => document.querySelector('.sessions-all').hidden), true, 'choosing a session closes the grid');
  } finally { await r.close(); }
  console.log('island-s12-check: ok');
})().catch(e => { console.error('FAIL', e.message, e.actual, e.expected); process.exit(1); });
