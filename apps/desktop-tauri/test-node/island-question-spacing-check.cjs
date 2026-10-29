'use strict';
// Regression for the human's crowded cards: computed geometry and real edge hits (playwright).
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-question-spacing-check: no Playwright — NOT a pass'); process.exit(0); }
(async () => {
  let passed = 0, failed = 0;
  const test = async (name, fn) => { try { await fn(); passed++; console.log('PASS ' + name); } catch (e) { failed++; console.error('FAIL ' + name + ': ' + e.message); } };
  const r = await H.open({ question: 'plain', described: true, manyPi: true, windowCap: 340, quick: true });
  const styles = sel => r.page.$eval(sel, el => { const s = getComputedStyle(el), a = getComputedStyle(el, '::after'); return { gap: s.gap, rowGap: s.rowGap, padding: s.padding, minHeight: s.minHeight, minWidth: s.minWidth, margin: s.margin, inset: a.inset, background: a.backgroundColor }; });
  try {
    await test('option edges ±1px belong to the correct row (including scrolled rows)', async () => {
      const hits = await r.hitEdges('.q-option');
      assert.ok(hits.length >= 36, 'all rows sampled');
      assert.deepEqual(hits.filter(h => !h.ok), []);
    });
    await test('44px non-overlapping option targets, 36px visible pills and 6px gaps', async () => {
      const s = await styles('.q-option');
      assert.equal(s.margin, '0px'); assert.equal(s.minHeight, '44px'); assert.equal(s.inset, '4px 0px');
      assert.equal((await styles('#question-options')).gap, '6px');
    });
    await test('card has 8px row gaps and 8/16/18/116px breathing room', async () => {
      const s = await styles('#question'); assert.equal(s.rowGap, '8px'); assert.equal(s.padding, '8px 16px 18px 116px');
    });
    await test('Omitir ghost is visible, 44px high and clear of the fold hit box', async () => {
      await r.page.$eval('#question', el => { el.scrollTop = 0; });
      const s = await styles('#question-skip'); assert.equal(s.minHeight, '44px'); assert.notEqual(s.background, 'rgba(0, 0, 0, 0)');
      const [skip] = await r.measure('#question-skip'), [fold] = await r.measure('#question-fold');
      assert.ok(skip.right <= fold.left || skip.bottom <= fold.top || fold.bottom <= skip.top, JSON.stringify({ skip, fold }));
    });
    await test('focus shell is opaque so terminal titles cannot bleed behind the top bar', async () => {
      const colors = await r.page.evaluate(() => {
        const root = document.documentElement, previous = root.dataset.theme;
        const colors = ['light', 'dark'].map(theme => { root.dataset.theme = theme; return getComputedStyle(document.getElementById('island')).backgroundColor; });
        if (previous === undefined) delete root.dataset.theme; else root.dataset.theme = previous;
        return colors;
      });
      for (const color of colors) assert.match(color, /^rgb\(/, 'opaque theme background, not translucent glass: ' + color);
    });
    await test('focus blobs have a 12px gutter, even before MutationObserver sync', async () => {
      // Mutate and measure synchronously: observer callbacks cannot supply focus layout.
      const b = await r.page.evaluate(() => {
        const c = document.getElementById('capsule'); c.dataset.layout = 'normal';
        const q = document.getElementById('question').getBoundingClientRect(), b = document.getElementById('blobs').getBoundingClientRect();
        return { gap: b.left - q.right, width: b.width, names: getComputedStyle(document.querySelector('.pill-choice span:not(.ico)')).display };
      });
      assert.ok(b.gap >= 12, JSON.stringify(b)); assert.equal(b.width, 68); assert.equal(b.names, 'none');
    });
    await test('long card text wraps anywhere without nowrap or ellipsis; capped body scrolls', async () => {
      const results = await r.page.evaluate(() => {
        const q = document.getElementById('question');
        // DOM-only fixture: exercise the existing fitCard resize path, no protocol/copy changes.
        for (const sel of ['#question-body', '.q-label', '.q-desc']) for (const el of q.querySelectorAll(sel)) el.textContent = '/very-long-path'.repeat(70);
        return [...q.querySelectorAll('#question-body, .q-label, .q-desc')].map(el => { const s = getComputedStyle(el); return { whiteSpace: s.whiteSpace, wrap: s.overflowWrap, ellipsis: s.textOverflow }; });
      });
      for (const s of results) { assert.notEqual(s.whiteSpace, 'nowrap'); assert.equal(s.wrap, 'anywhere'); assert.notEqual(s.ellipsis, 'ellipsis'); }
      await r.page.waitForTimeout(1000);
      const b = await r.page.$eval('#question', el => ({ overflow: getComputedStyle(el).overflowY, scroll: el.scrollHeight, client: el.clientHeight, width: el.clientWidth, sw: el.scrollWidth, top: el.getBoundingClientRect().top, bottom: el.getBoundingClientRect().bottom }));
      assert.equal(b.overflow, 'auto'); assert.ok(b.scroll > b.client); assert.ok(b.sw <= b.width, JSON.stringify(b)); assert.ok(b.top >= 42 && b.bottom <= 332, JSON.stringify(b));
    });
  } finally { await r.close(); }
  {
    const m = await H.open({ question: 'multi', quick: true });
    try {
      await test('multi-select controls stay clickable after keyboard toggles', async () => {
        const rows = (await m.measure('.q-option')).filter(b => b.shown);
        await m.page.mouse.click(rows[0].left + rows[0].w / 2, rows[0].top + rows[0].h / 2);
        await m.page.mouse.click(rows[2].left + rows[2].w / 2, rows[2].top + rows[2].h / 2);
        await m.page.focus('.q-option[role="checkbox"]'); await m.page.keyboard.press('Space');
        const blocked = (await m.hit('.q-option, #question-primary')).filter(h => !h.ok);
        assert.deepEqual(blocked, [], JSON.stringify(blocked));
      });
    } finally { await m.close(); }
  }
  for (const question of ['approvalPi', 'form']) {
    const a = await H.open({ question, quick: true, windowCap: 340 });
    try {
      await test(question === 'form' ? 'question nav targets are at least 44px (touch target)' : 'approval targets are non-overlapping, at least 44x96px with 36px visible fill', async () => {
        const s = await a.page.$eval(question === 'form' ? '#question-nav button' : '.q-approval-btn', el => { const s = getComputedStyle(el); return { h: s.minHeight, w: s.minWidth, margin: s.margin, inset: getComputedStyle(el, '::after').inset }; });
        // Touch target: 44px since 0d2784ff (T4/S5 «a touch target like every other control on the question card»); it was 36px in the vendored island.css.
        assert.equal(s.h, '44px');
        if (question !== 'form') { assert.equal(s.w, '96px'); assert.equal(s.margin, '0px'); assert.equal(s.inset, '4px 0px'); }
      });
      if (question !== 'form') await test('legacy permission command also wraps without ellipsis', async () => {
        const s = await a.page.$eval('#permission #operation', el => { const s = getComputedStyle(el); return { whiteSpace: s.whiteSpace, wrap: s.overflowWrap, ellipsis: s.textOverflow }; });
        assert.notEqual(s.whiteSpace, 'nowrap'); assert.equal(s.wrap, 'anywhere'); assert.notEqual(s.ellipsis, 'ellipsis');
      });
      if (question !== 'form') await test('approval path/command text wraps anywhere', async () => {
        const s = await a.page.$eval('#question-approval .q-cmd', el => { el.textContent = '/long-path'.repeat(160); const s = getComputedStyle(el); return { whiteSpace: s.whiteSpace, wrap: s.overflowWrap, width: el.clientWidth, scroll: el.scrollWidth }; });
        assert.notEqual(s.whiteSpace, 'nowrap'); assert.equal(s.wrap, 'anywhere'); assert.ok(s.scroll <= s.width);
      });
    } finally { await a.close(); }
  }
  console.log(`island-question-spacing-check: ${passed} passed, ${failed} failed`);
  if (failed) process.exitCode = 1;
})().catch(e => { console.error(e); process.exitCode = 1; });
