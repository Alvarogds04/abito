'use strict';
/* abito-pausa S1-S2 (human: «Visibles en gris y apagadas»): this version works on Chat and Agentes. The other features are paused:
   grey in Ajustes (hub/shell.ts PAUSED_PAGES) and switched off here on the island and in Rust (src/paused.rs). Stored preferences are
   never rewritten, so the next version turns a feature back on by removing it from PAUSED. Classic script (window.AbitoPaused). */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.AbitoPaused = Object.freeze(api);
})(typeof self !== 'undefined' ? self : this, () => {
  const PAUSED = Object.freeze(['dictation', 'voice', 'integrations', 'spotify', 'phone']);
  const paused = feature => PAUSED.includes(feature);
  return { PAUSED, paused, LABEL: 'En pausa · próxima versión' };
});
