# Probar Abito en tu Mac (versión de prueba 0.1.1)

Abito es una isla chiquita arriba de la pantalla que muestra qué están haciendo tus agentes (**Claude Code**, **Codex** y **Pi**) y te deja contestarles sin ir a la terminal.

Necesitás una Mac con **Apple Silicon (M1 o más nuevo)** y **macOS 12 o superior**. En una Mac con Intel no abre.

## 1. Antes de instalar: Node.js y Pi

Abito usa Node.js y Pi para manejar los agentes. Si ya los tenés, pasá al punto 2.

1. **Node.js:** instalalo desde https://nodejs.org (la versión LTS) o con `brew install node`.
2. **Pi:** en la Terminal, ejecutá `npm install -g @earendil-works/pi-coding-agent`.

Si falta alguno, Abito igual abre y los avisos funcionan, pero en **Ajustes → Motor** te va a decir «Falta Node.js» o «Falta Pi» y cómo instalarlo.

## 2. Bajar e instalar

1. Desde https://github.com/Alvarogds04/abito/releases bajá `Abito-macos-arm64.dmg`.
2. Abrí el `.dmg` y arrastrá **abito-island** a **Aplicaciones**.

## 3. Abrirla la primera vez

La app **no está notarizada por Apple** (eso cuesta 99 USD al año), así que macOS la frena la primera vez. Es normal.

1. Abrila desde **Aplicaciones**. macOS va a decir que no se puede verificar el desarrollador: tocá **Listo** (o **Cancelar**).
2. Abrí **Ajustes del Sistema → Privacidad y seguridad** y bajá hasta el aviso sobre «abito-island».
3. Tocá **Abrir igualmente** y confirmá con tu contraseña o Touch ID.

A partir de ahí abre normalmente.

**Si macOS dice que la app «está dañada»:** abrí la Terminal y ejecutá

```sh
xattr -dr com.apple.quarantine /Applications/abito-island.app
```

y volvé a abrirla. (No debería pasar con esta versión, pero si pasa, avisame).

## 4. Dónde está

- La **isla** aparece arriba, en el centro de la pantalla.
- **No hay ícono en el Dock**: es a propósito. Para mostrarla, ocultarla o salir, usá el ícono de Abito en la **barra de menú** (arriba a la derecha).

## 5. Conectar tus agentes

- **Claude Code y Codex:** en **Ajustes → Agentes**, activalos. Abito instala sus hooks.
- **Pi:** cargá la extensión de Abito (instrucciones en el README del repo).

Después, abrí una sesión de tu agente: cuando te pida permiso o te haga una pregunta, aparece en la isla.

## 6. Permisos de macOS

- **Micrófono:** solo si usás el dictado (está en pausa en esta versión).
- **Accesibilidad / Automatización:** solo si macOS la pide para un atajo. Si no la pide, no hace falta.

## 7. Checklist (marcá: anda / no anda / no aplica)

- [ ] Abre (después de «Abrir igualmente») y se ve la isla arriba.
- [ ] No aparece en el Dock y sí en la barra de menú; desde ahí se puede mostrar, ocultar y salir.
- [ ] **Ajustes → Motor** dice que está conectado (o, si falta Node.js o Pi, dice cuál y cómo instalarlo).
- [ ] **Ajustes → Agentes:** activar Claude Code o Codex.
- [ ] Una pregunta o pedido de permiso de tu agente aparece en la isla y lo podés contestar desde ahí.
- [ ] Cerrar y volver a abrir: se conservan los ajustes.
- [ ] Desinstalar: arrastrar la app de Aplicaciones a la Papelera.

## 8. Qué mandarme

- **Capturas** de lo que falle y de cualquier cartel de macOS.
- El reporte de la app: **Ajustes → «Reportar un problema»**.
- Tu modelo de Mac y versión de macOS.

¡Gracias por probarla!
