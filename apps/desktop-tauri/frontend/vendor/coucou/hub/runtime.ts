// Abito-owned lifecycle and trusted-gesture adapter for the literal MIT hub controls.
import { h } from '../views/dom.js';
import { button, label, card, row } from './ui.js';
export function trusted<T extends HTMLElement>(el: T): T {
  for (const node of [el, ...el.querySelectorAll('button')]) for (const type of ['click', 'change', 'input', 'submit', 'keydown']) {
    node.addEventListener(type, e => { if (e.isTrusted !== true) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  }
  return el;
}
export function action(text: string, fn: () => void, disabled = false): HTMLButtonElement {
  const b = trusted(button(text, fn)); b.type = 'button'; b.disabled = disabled; return b;
}
export const field = (id: string, title: string, type = 'text', value = '') => h('input', { id, class: 'hub-input', type, value, 'aria-label': title, autocomplete: type === 'password' ? 'new-password' : 'off' }) as HTMLInputElement;
export const note = (text: string) => h('p', { class: 'hub-note', role: 'status', text });
// A section with no children yet must not draw an empty card: several pages create the section first and append their own cards or
// rows afterwards (outside it), and the empty box above them was found by eye on Pills, Integraciones, Chat and Conexiones (#503).
export const section = (title: string, ...children: HTMLElement[]) =>
  children.some(Boolean) ? h('section', {}, label(title), card(...children)) : h('section', {}, label(title));
export const status = (v: any) => row(v?.word || v?.say || 'Leyendo…', v?.note || null);
// Keep the legacy editor/search caret when a changed backend frame really needs a paint.
export function preserveFocus(root: HTMLElement): () => void {
  const active = document.activeElement as HTMLInputElement | null;
  const fields = () => [...root.querySelectorAll('input'), ...root.querySelectorAll('textarea'), ...root.querySelectorAll('select')];
  if (!active?.id || !fields().includes(active)) return () => {};
  const start = active.selectionStart, end = active.selectionEnd;
  return () => { const next = fields().find(n => n.id === active.id) as HTMLInputElement | undefined; next?.focus(); if (typeof start === 'number') try { next?.setSelectionRange(start, end); } catch { /* Not a text field. */ } };
}
export function session(invoke: any, listen?: any, timers: any = globalThis) {
  let active = true;
  const pending = new Set<Promise<any>>(), cleanups: (() => void)[] = [];
  const root = h('div', {});
  const message = note('');
  const track = (p: Promise<any>) => { pending.add(p); p.finally(() => pending.delete(p)).catch(() => {}); return p; };
  const call = (cmd: string, payload: any) => active ? track(Promise.resolve().then(() => { if (!active) throw new Error('Página cerrada'); return invoke(cmd, { payload }); })) : Promise.reject(new Error('Página cerrada'));
  const work = (fn: () => Promise<any>, done: (value: any) => void = () => {}, failed?: (error: any) => void) => track((async () => {
    try { const v = await fn(); if (active) done(v); }
    catch (error) { if (active) { message.textContent = 'No se pudo completar. No se anuncia ningún cambio como guardado.'; failed?.(error); } }
  })());
  const watch = (name: string, fn: (p: any) => void) => { if (!listen) return; work(async () => {
    const off = await listen(name, (e: any) => { if (active && e?.payload) fn(e.payload); });
    if (active) cleanups.push(off); else off();
  }); };
  const later = (fn: () => void, ms: number) => { const id = timers.setTimeout(() => { if (active) fn(); }, ms); cleanups.push(() => timers.clearTimeout(id)); return id; };
  const dispose = () => { active = false; for (const off of cleanups.splice(0)) off(); for (const input of root.querySelectorAll('input')) if ((input as HTMLInputElement).type === 'password') (input as HTMLInputElement).value = ''; };
  return { root, message, call, work, watch, later, alive: () => active, finish: () => Object.assign(root, { dispose, settled: async () => { while (pending.size) await Promise.allSettled([...pending]); } }) };
}
