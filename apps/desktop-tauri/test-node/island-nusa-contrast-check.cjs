'use strict';
/* HU-063 T5 (human, 28-oct: «corregir el contraste del texto chico de las tarjetas en take-over a WCAG AA sobre negro»):
   every VISIBLE text node inside the take-over card surfaces (#question, #permission, #notices, #engine-band, the Nusa header strip)
   must reach contrast >= 4.5:1 (>= 3:1 for large text, >= 24px or >= 18.66px bold) against the notch's own black. Real layout, Playwright.
   The effective colour is the computed colour composited with its ancestors' opacity over #000 (the shell is opaque black, S1). */
const assert = require('node:assert/strict');
const H = require('./island-geometry-harness.cjs');
if (!H.findPlaywright()) { console.log('SKIPPED island-nusa-contrast-check: no playwright — NOT a pass'); process.exit(0); }
const SURFACES = '#question, #permission, #notices, #engine-band, #nusa-head';
const CHAT_SURFACES = '#chat, #island-header';
// comment 8 (human, 29-oct): the chat view sits on the Nusa black too; rows the real chat.js builds are injected so bubbles / notes / errors are measured as well
const CHAT_ROWS = () => { const log = document.getElementById('chat-log'); log.textContent = '';
  for (const [cls, text] of [['user', 'Hola Abito, ¿qué hay pendiente hoy?'], ['assistant', 'Tenés dos avisos y una pregunta esperando respuesta.'], ['note', 'Conversación nueva'], ['error', 'No se pudo enviar el mensaje']]) { const li = document.createElement('li'); li.className = cls; li.textContent = text; log.append(li); } };
async function measure(r, surfaces = SURFACES) {
  return r.page.evaluate(sel => {
    const parse = c => { const m = /rgba?\(([^)]+)\)/.exec(c); if (!m) return null; const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
    const lum = ({ r, g, b }) => { const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
    const out = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = n.nodeValue.trim(); if (!text) continue;
      const el = n.parentElement; if (!el || !el.closest(sel) || el.closest('[hidden], [inert], .sr-only')) continue;
      const s = getComputedStyle(el), b = el.getBoundingClientRect();
      if (s.display === 'none' || s.visibility === 'hidden' || b.width < 1 || b.height < 1) continue;
      if (el.closest('button:disabled, [aria-disabled="true"]')) continue; // inactive controls are exempt from WCAG 1.4.3
      const fg = parse(s.color); if (!fg) continue;
      // Real compositing, outermost first, starting from the notch black: each element's own fill and its ::after fill (the buttons' visible
      // face) go over the backdrop, then the element's opacity blends that group back toward the backdrop; the text is composited the same way.
      const chain = []; for (let e = el; e; e = e.parentElement) chain.unshift(e);
      const over = (top, base) => top && top.a > 0 ? { r: top.r * top.a + base.r * (1 - top.a), g: top.g * top.a + base.g * (1 - top.a), b: top.b * top.a + base.b * (1 - top.a) } : base;
      const lerp = (from, to, o) => ({ r: from.r + (to.r - from.r) * o, g: from.g + (to.g - from.g) * o, b: from.b + (to.b - from.b) * o });
      let bg = { r: 0, g: 0, b: 0 }, tc = null;
      for (const e of chain) {
        const cs = getComputedStyle(e), o = +cs.opacity, before = bg;
        let group = bg;
        if (e.closest(sel)) { group = over(parse(cs.backgroundColor), group); const af = getComputedStyle(e, '::after'); if (af.content !== 'none' && af.position === 'absolute') group = over(parse(af.backgroundColor), group); }
        if (e === el) tc = lerp(before, over(fg, group), o);
        bg = lerp(before, group, o);
      }
      const L1 = lum(tc), L2 = lum(bg), ratio = (Math.max(L1, L2) + 0.05) / (Math.min(L1, L2) + 0.05);
      const px = parseFloat(s.fontSize), bold = +s.fontWeight >= 700, large = px >= 24 || (px >= 18.66 && bold);
      out.push({ text: text.slice(0, 28), ratio: Math.round(ratio * 100) / 100, need: large ? 3 : 4.5, px });
    }
    return out;
  }, surfaces);
}
(async () => {
  const scenarios = [
    ['permission', { question: 'approvalPi' }],
    ['question', { question: 'plain' }],
    ['question multi', { question: 'multi', windowCap: 340 }],
    ['question form', { question: 'form', windowCap: 340 }],
    ['question multi picked', { question: 'multi', windowCap: 340, pick: true }], // «Hecho» enabled: its face must pass too
    ['question form picked', { question: 'form', windowCap: 340, pick: true }], // «Siguiente» enabled
    ['finished', { finishedCard: 'pi', sessions: [], quick: true }],
    ['engine band', { engine: 'down' }],
  ];
  const bad = [];
  for (const theme of ['dark', 'light']) for (const [name, sc] of [['chat', { chat: true }]]) {
    const r = await H.open({ ...sc, theme });
    try {
      await r.page.waitForTimeout(900); await r.page.evaluate(CHAT_ROWS); await r.page.waitForTimeout(300);
      const m = await measure(r, CHAT_SURFACES); assert.ok(m.length > 4, `${name}/${theme}: chat text measured (${m.length})`);
      for (const x of m) if (x.ratio < x.need) bad.push(`${name}/${theme}: «${x.text}» ${x.ratio}:1 < ${x.need}:1 (${x.px}px)`);
      if (process.env.NUSA_SHOTS) await r.page.screenshot({ path: `/tmp/nusa-contrast-${name.replace(/ /g, '-')}-${theme}.png` });
    } finally { await r.close(); }
  }
  for (const theme of ['dark', 'light']) for (const [name, sc] of scenarios) {
    const r = await H.open({ ...sc, theme });
    try { await r.page.waitForTimeout(700); if (sc.pick) { await r.page.click('.q-option'); await r.page.waitForTimeout(400); } const m = await measure(r); assert.ok(m.length > 0, `${name}/${theme}: some text measured`); for (const x of m) if (x.ratio < x.need) bad.push(`${name}/${theme}: «${x.text}» ${x.ratio}:1 < ${x.need}:1 (${x.px}px)`);
      if (process.env.NUSA_SHOTS) await r.page.screenshot({ path: `/tmp/nusa-contrast-${name.replace(/ /g, '-')}-${theme}.png` }); }
    finally { await r.close(); }
  }
  // HU-063: question option descriptions (.q-desc) against the REAL painted background: walk up to the first opaque ancestor, composite alpha over it
  for (const theme of ['dark', 'light']) {
    const r = await H.open({ question: 'plain', described: true, theme });
    try {
      await r.page.waitForTimeout(700);
      const m = await r.page.evaluate(() => {
        const parse = c => { const x = /rgba?\(([^)]+)\)/.exec(c); if (!x) return null; const p = x[1].split(/[ ,/]+/).filter(Boolean).map(Number); return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
        const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
        const lum = ({ r, g, b }) => 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
        return [...document.querySelectorAll('.q-desc')].filter(e => e.getBoundingClientRect().width > 0).map(el => {
          const layers = []; let opaque = null;
          for (let e = el; e && !opaque; e = e.parentElement) { const c = parse(getComputedStyle(e).backgroundColor); if (c && c.a > 0) { if (c.a >= 1) opaque = c; else layers.unshift(c); } }
          const own = parse(getComputedStyle(document.getElementById('question')).backgroundColor); if (!own || own.a < 1) return { text: el.textContent.slice(0, 28), none: true }; // the question card itself must paint an opaque fill (what island-s6 measures)
          if (!opaque) return { text: el.textContent.slice(0, 28), none: true };
          let bg = opaque; for (const c of layers) bg = { r: c.r * c.a + bg.r * (1 - c.a), g: c.g * c.a + bg.g * (1 - c.a), b: c.b * c.a + bg.b * (1 - c.a) };
          const fg = parse(getComputedStyle(el).color), t = { r: fg.r * fg.a + bg.r * (1 - fg.a), g: fg.g * fg.a + bg.g * (1 - fg.a), b: fg.b * fg.a + bg.b * (1 - fg.a) };
          const a = lum(t), b = lum(bg); return { text: el.textContent.slice(0, 28), ratio: Math.round((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) * 100) / 100 };
        });
      });
      assert.ok(m.length >= 2, `option descriptions/${theme}: descriptions measured (${m.length})`);
      for (const x of m) if (x.none) bad.push(`option description/${theme}: «${x.text}» #question / ancestor chain has no opaque background`); else if (x.ratio < 4.5) bad.push(`option description/${theme}: «${x.text}» ${x.ratio}:1 < 4.5:1`);
    } finally { await r.close(); }
  }
  assert.deepEqual(bad, [], `contrast below WCAG AA on black:\n${bad.join('\n')}`);
  console.log('island-nusa-contrast-check: ok');
})().catch(e => { console.error('FAIL', e.message); process.exit(1); });
