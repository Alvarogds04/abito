# Paused features (this version: Chat and Agentes)

Human, 26-oct: «añadir todo el resto de funciones que tenemos como no disponibles y en pausa, excepto el chat y los agentes que es
lo que iremos tocando en esta version y el resto lo trabajaremos en otras versiones mas a futuro», with the chosen option
«Visibles en gris y apagadas».

| Feature | Ajustes | Island | Inside |
| --- | --- | --- | --- |
| Dictado + Flow pages (Notetaker, Estadísticas, Diccionario, Fragmentos, Estilos, Ajustes de dictado) | grey, «En pausa» | no dictation pill | engine and `CTRL+ALT+v` portal key never start |
| OK Abito | grey, «En pausa» | — | microphone listener never starts |
| Integraciones (GitHub, n8n, Vercel, Stripe, Resend, Notion, Cal.com) | grey, «En pausa» | no rows, no cards | no poller threads, no network |
| Spotify | (a service pill) | no card | `spotify_action` answers `paused` |
| Teléfono Android | grey, «En pausa» | — | the link server never ran in production |

Active: Chat, Agentes (sessions, Sileo questions/permissions/notices, recap, plan usage), General, Atajos, Privacidad.

**Stored preferences are never rewritten.** A paused feature with its preference «on» simply does not start.

## Turning a feature back on (next version)

Remove it from the three lists — they are pinned against each other by `test-node/paused-features-check.cjs`:

1. `apps/desktop-tauri/src/paused.rs` → `paused()`
2. `apps/desktop-tauri/frontend/paused.js` → `PAUSED`
3. `apps/desktop-tauri/frontend/vendor/coucou/hub/shell.ts` → `ACTIVE_PAGES` (then `node scripts/build-coucou.cjs --hub-only`)

The previous behaviour of each island module stays tested through its `wantedIgnoringPause` export.
