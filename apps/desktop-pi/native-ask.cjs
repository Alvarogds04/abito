'use strict';
// Native ask interception (path B: gentle-pi is NOT touched). Pi's own `ask_user_choice` and `ask_user_question`
// are answered from Abito OR the terminal; the first answer wins and the other side is withdrawn. The `tool_call`
// hook BLOCKS the native call with the exact text the native tool would have returned, so the model sees a tool
// error carrying the right words (a blocked call cannot be rewritten into a success through the public API).
// Anything uncertain is NOT intercepted: the native tool then runs untouched.
//
// A request is a list of ITEMS (one for ask_user_choice / a plain question, one to four for a questionnaire).
// Abito answers the whole request at once; the terminal asks the items in order. Partial answers are never mixed:
// whoever finishes first provides ALL the answers, the other side is discarded.
const { normalizeRequest } = require('./questions.cjs');

const CHOICE = 'ask_user_choice', QUESTIONNAIRE = 'ask_user_question';
const OTHER_ROW = 'Other…', CUSTOM_ROW = 'Type something.', DONE_ROW = 'Done';

// SECURITY: consent / authorization prompts must never be answerable from Abito. Deliberately broad: a false
// positive only means the native terminal dialog runs as today.
const SENSITIVE = /consent|authori[sz]|autoriz|grant|conced|declin|rechaz|permission|permiso|token|binding|envelope|lineage|provider|credential|secret|password/i;
const isSensitive = value => typeof value === 'string' && SENSITIVE.test(value);
// Values that read as plain words (`yes-once`, `main`, `dev_2`); opaque ids, hex, JSON or digit runs do not.
const PLAIN_VALUE = /^[A-Za-z][A-Za-z0-9]*(?:[-_ .][A-Za-z0-9]+)*$/;
const plainValue = v => typeof v === 'string' && v.length <= 32 && PLAIN_VALUE.test(v) &&
  !/\d{4,}/.test(v) && !/^[0-9a-f]{12,}$/i.test(v) && !isSensitive(v);

const isObject = v => v !== null && typeof v === 'object' && !Array.isArray(v);
const str = v => (typeof v === 'string' ? v : null);
const rowOf = c => (c.description ? `${c.label} — ${c.description}` : c.label);

// ---- what is asked --------------------------------------------------------------------------------------
// plan = { items, form, simple, scan, render(answers), cancelled }
//  item = { title, question, header, choices[{label, description, value?}], multi, free, freeRow }
//  answers[i] = {kind:'option',index} | {kind:'options',indexes,text?} | {kind:'custom',text}
function planChoice(input) {
  if (!isObject(input) || !Array.isArray(input.options)) return null;
  const question = str(input.question);
  if (question === null) return null;
  const choices = [];
  for (const o of input.options) {
    if (!isObject(o) || str(o.label) === null || str(o.description) === null || !plainValue(o.value)) return null;
    choices.push({ label: o.label, description: o.description, value: o.value });
  }
  const item = { title: question, question, header: '', choices, multi: false, free: input.allowCustomResponse === true, freeRow: OTHER_ROW };
  return {
    items: [item], form: false, simple: true,
    scan: [question, ...choices.flatMap(c => [c.label, c.description])],
    render: ([a]) => (a.kind === 'custom' ? `User responded: ${a.text}`
      : `User selected: ${a.index + 1}. ${choices[a.index].label} (value: ${choices[a.index].value})`),
    cancelled: 'User cancelled the choice',
  };
}

function planQuestionnaire(input) {
  if (!isObject(input) || !Array.isArray(input.questions) || input.questions.length < 1 || input.questions.length > 4) return null;
  const items = [], scan = [];
  for (const q of input.questions) {
    if (!isObject(q) || str(q.question) === null || str(q.header) === null || [...q.header].length > 32 || !Array.isArray(q.options)) return null;
    const choices = [];
    for (const o of q.options) {
      if (!isObject(o) || str(o.label) === null) return null;
      choices.push({ label: o.label, description: str(o.description) ?? '' });
    }
    items.push({ title: `${q.header}: ${q.question}`, question: q.question, header: q.header, choices, multi: q.multiSelect === true, free: true, freeRow: CUSTOM_ROW });
    scan.push(q.question, q.header, ...q.options.flatMap(o => [o.label, o.description, o.preview].filter(v => typeof v === 'string')));
  }
  const body = (item, a) => {
    if (a.kind === 'option') return item.choices[a.index].label;
    if (a.kind === 'custom') return `(custom) ${a.text}`;
    const picked = a.indexes.map(i => item.choices[i].label);
    if (a.kind === 'options') return a.text === undefined ? `selected: ${picked.join(', ')}`
      : picked.length ? `(custom) ${a.text} — selected: ${picked.join(', ')}` : `(custom) ${a.text}`;
    return `(custom) ${a.text}`;
  };
  return {
    items, form: items.length > 1, simple: items.length === 1 && !items[0].multi, scan,
    render: answers => answers.map((a, i) => `${i + 1}. ${items[i].question} — ${body(items[i], a)}`).join('\n'),
    cancelled: 'User cancelled the questionnaire',
  };
}

// ---- telling the human what happened -----------------------------------------------------------------------
// Pi paints a blocked call with gentle-pi's own renderResult, which prints «Cancelled» when the result carries no
// selection (a block yields `details: {}`), and the public API cannot change that row: `tool_result` does not run
// for blocked calls and the live row is drawn from `tool_execution_end`. So the human is told who answered and what
// with ONE info notification (a dim line under the row). Display only: never persisted, never seen by the model.
const NOTE_MAX = 200;
const clip = text => (text.length <= NOTE_MAX ? text : `${text.slice(0, NOTE_MAX - 1)}…`);
function summarize(plan, answers) {
  const one = (item, a) => {
    if (a.kind === 'option') return item.choices[a.index].label;
    if (a.kind === 'custom') return a.text;
    return [...a.indexes.map(i => item.choices[i].label), ...(a.text === undefined ? [] : [a.text])].join(', ');
  };
  if (plan.items.length === 1) return one(plan.items[0], answers[0]);
  return answers.map((a, i) => `${plan.items[i].header || i + 1}: ${one(plan.items[i], a)}`).join(' · ');
}
function tell(ctx, text) {
  try { if (typeof ctx?.ui?.notify === 'function') ctx.ui.notify(clip(text), 'info'); } catch { /* display only */ }
}

// ---- the Abito side ---------------------------------------------------------------------------------------
// Synthetic values o1..o4, never the originals; Abito's own text/size rules decide eligibility.
const synth = choices => choices.map((c, i) => ({ value: `o${i + 1}`, label: c.label, description: c.description }));
function abitoRequest(plan, structured) {
  let request;
  if (plan.simple) {
    const [item] = plan.items;
    request = { kind: 'question', text: item.question, allowFreeText: item.free, options: synth(item.choices) };
  } else {
    if (!structured) return null;
    if (plan.form) {
      request = { kind: 'question', text: plan.items.length === 1 ? plan.items[0].question : plan.items.map(i => i.header).join(' · '), options: [], allowFreeText: false,
        questions: plan.items.map(i => ({ header: i.header, text: i.question, options: synth(i.choices), multiSelect: i.multi, allowFreeText: i.free })) };
    } else {
      const [item] = plan.items;
      request = { kind: 'question', text: item.question, allowFreeText: item.free, options: synth(item.choices), multiSelect: true };
    }
  }
  try { normalizeRequest(request, undefined, structured); } catch { return null; }
  return request;
}

const indexOfSynth = (v, n) => { const m = typeof v === 'string' ? /^o([1-4])$/.exec(v) : null; return m && Number(m[1]) <= n ? Number(m[1]) - 1 : -1; };
// One Abito answer for one item, checked against what was asked. null = does not fit.
function decodeItem(item, a) {
  if (!isObject(a)) return null;
  if (item.multi) {
    if (a.type !== 'options' || !Array.isArray(a.values)) return null;
    const indexes = a.values.map(v => indexOfSynth(v, item.choices.length));
    if (indexes.some(i => i < 0) || new Set(indexes).size !== indexes.length) return null;
    if (a.text !== undefined && !(item.free && typeof a.text === 'string' && a.text.trim())) return null;
    if (!indexes.length && a.text === undefined) return null;
    return { kind: 'options', indexes: [...indexes].sort((x, y) => x - y), ...(a.text === undefined ? {} : { text: a.text }) };
  }
  if (a.type === 'option') { const i = indexOfSynth(a.value, item.choices.length); return i < 0 ? null : { kind: 'option', index: i }; }
  if (a.type === 'text' && item.free && typeof a.text === 'string' && a.text.trim()) return { kind: 'custom', text: a.text };
  return null;
}
function decodeAbito(plan, a) {
  if (plan.form) {
    if (!isObject(a) || a.type !== 'form' || !Array.isArray(a.answers) || a.answers.length !== plan.items.length) return null;
    const out = plan.items.map((item, i) => decodeItem(item, a.answers[i]));
    return out.every(Boolean) ? out : null;
  }
  const one = decodeItem(plan.items[0], a);
  return one ? [one] : null;
}

// ---- the terminal side ------------------------------------------------------------------------------------
// One item through ctx.ui.select / ctx.ui.input. undefined = cancelled; the shared signal aborts every dialog.
async function askItem(ctx, item, signal) {
  const display = item.choices.map(rowOf);
  if (!item.multi) {
    const picked = await ctx.ui.select(item.title, item.free ? [...display, item.freeRow] : display, { signal });
    if (picked === undefined || signal.aborted) return undefined;
    if (item.free && picked === item.freeRow) {
      const text = await ctx.ui.input(item.title, 'Type your response', { signal });
      return text === undefined || signal.aborted ? undefined : { kind: 'custom', text };
    }
    const index = display.indexOf(picked);
    return index < 0 ? undefined : { kind: 'option', index };
  }
  // multiSelect: toggle rounds, like gentle-pi's own RPC dialogs; «Done» (or every row on) commits.
  const toggled = display.map(() => false);
  const cap = Math.max(32, display.length + 4);
  for (let round = 0; round < cap; round++) {
    const rows = display.map((row, i) => `${toggled[i] ? '[x]' : '[ ]'} ${row}`);
    const picked = await ctx.ui.select(item.title, [...rows, ...(item.free ? [item.freeRow] : []), DONE_ROW], { signal });
    if (picked === undefined || signal.aborted) return undefined;
    const indexes = () => toggled.flatMap((on, i) => (on ? [i] : []));
    if (picked === DONE_ROW) return { kind: 'options', indexes: indexes() };
    if (item.free && picked === item.freeRow) {
      const text = await ctx.ui.input(item.title, 'Type your response', { signal });
      return text === undefined || signal.aborted ? undefined : { kind: 'options', indexes: indexes(), text };
    }
    const at = rows.indexOf(picked);
    if (at < 0) return undefined; // unrecognised answer: never commit a partial state
    toggled[at] = !toggled[at];
    if (toggled.every(Boolean)) return { kind: 'options', indexes: indexes() };
  }
  return undefined;
}

// One race. Resolves with {kind:'done', answers} | {kind:'cancelled'} | {kind:'fallthrough'}.
// Abito failing (busy, expired, disconnected...) never answers and never ends the race: the terminal dialogs keep
// waiting indefinitely. Only a human decision from either side decides, and always for ALL the items at once.
function race({ controller, ctx, plan, request }) {
  return new Promise(resolve => {
    let decided = false;
    const abito = new AbortController(), terminal = new AbortController();
    const finish = (selection, winner) => {
      if (decided) return;
      decided = true;
      ctx.signal?.removeEventListener('abort', onRunAborted);
      if (winner !== 'abito') abito.abort();
      if (winner !== 'terminal') terminal.abort();
      resolve({ ...selection, by: winner });
    };
    const onRunAborted = () => finish({ kind: 'cancelled' });
    if (ctx.signal?.aborted) { resolve({ kind: 'cancelled' }); return; }
    ctx.signal?.addEventListener('abort', onRunAborted, { once: true });

    Promise.resolve(controller.ask(request, ctx, abito.signal)).then(result => {
      if (decided || !result) return;
      if (result.status === 'human_cancelled') { finish({ kind: 'cancelled' }, 'abito'); return; }
      if (result.status !== 'answered') return; // failure: the terminal keeps waiting
      const answers = decodeAbito(plan, result.answer);
      if (answers) finish({ kind: 'done', answers }, 'abito');
    }, () => {});

    (async () => {
      const answers = [];
      for (const item of plan.items) {
        const a = await askItem(ctx, item, terminal.signal);
        if (decided || terminal.signal.aborted) return;
        if (a === undefined) { finish({ kind: 'cancelled' }, 'terminal'); return; }
        answers.push(a);
      }
      finish({ kind: 'done', answers }, 'terminal');
    })().catch(() => finish({ kind: 'fallthrough' }, 'terminal')); // the terminal cannot ask: let the native tool run
  });
}

/**
 * @param {{ controller?: any, env?: Record<string, string | undefined> }} [options]
 * @returns {(event: any, ctx: any) => Promise<{ block: true, reason: string } | undefined>}
 */
function createNativeAskHandler({ controller, env = process.env } = {}) {
  return async function onToolCall(event, ctx) {
    try {
      if (env.ABITO_NATIVE_ASK === '0' || !event) return undefined;
      const plan = event.toolName === CHOICE ? planChoice(event.input)
        : event.toolName === QUESTIONNAIRE ? planQuestionnaire(event.input) : null;
      if (!plan || plan.scan.some(isSensitive)) return undefined;
      for (const item of plan.items) {
        if (new Set(item.choices.map(rowOf)).size !== item.choices.length) return undefined;
        if (item.free && [item.freeRow, DONE_ROW].some(r => item.choices.some(c => rowOf(c) === r))) return undefined;
      }
      if (ctx?.hasUI === false || typeof ctx?.ui?.select !== 'function' || typeof ctx.ui.input !== 'function') return undefined;
      if (!controller || typeof controller.canAsk !== 'function' || !controller.canAsk()) return undefined;
      const request = abitoRequest(plan, typeof controller.structured === 'function' && controller.structured() === true);
      if (!request) return undefined;
      const selection = await race({ controller, ctx, plan, request });
      if (selection.kind === 'fallthrough') return undefined;
      const where = selection.by === 'abito' ? 'Abito' : selection.by === 'terminal' ? 'la terminal' : null;
      if (where && selection.kind === 'done') tell(ctx, `Respondido en ${where}: ${summarize(plan, selection.answers)}`);
      else if (where) tell(ctx, `Cancelado en ${where}`);
      return { block: true, reason: selection.kind === 'done' ? plan.render(selection.answers) : plan.cancelled };
    } catch { return undefined; }
  };
}

module.exports = { createNativeAskHandler, isSensitive, planChoice, planQuestionnaire };
