// HU-046 (human, verbatim option chosen: «Botón «Instalar Pi» (recomendado)»): the control that appears wherever the page learns that
import { h, clear } from "../views/dom.js";
import { action, trusted } from "./runtime.js";
const PI_COMMAND = "npm install -g @earendil-works/pi-coding-agent";
const PLAN = `Se va a ejecutar: ${PI_COMMAND}, con el Node.js de tu equipo. No pide permisos de administrador. Tarda unos minutos y se corta a los 5.`;
function missingPiece(state) {
  const code = state?.connection === "no_daemon" ? state?.last_error?.code : null;
  return code === "pi_missing" ? "pi" : code === "node_missing" ? "node" : null;
}
function piInstallControl({ call, watch, work, onDone }) {
  const box = h("div", { id: "pi-install", class: "pi-install", role: "group", "aria-label": "Instalar Pi" });
  let missing = null, phase = "idle", say = "", confirming = false, finished = false, already = false, reason = "";
  const apply = (view) => {
    if (!view || typeof view !== "object") return;
    const next = ["idle", "running", "done", "failed"].includes(view.phase) ? view.phase : null;
    if (!next) return;
    phase = next;
    confirming = false;
    say = typeof view.say === "string" ? view.say : "";
    already = view.already === true;
    reason = typeof view.reason === "string" ? view.reason : "";
    if (view.reason === "node_appeared" && missing === "node") missing = "pi";
    if (next === "done" && !finished) {
      finished = true;
      onDone?.();
    }
    paint();
  };
  const run = () => {
    if (phase === "running") return;
    phase = "running";
    confirming = false;
    say = "";
    paint();
    work(() => call("pi_install_action", { action: "run" }), apply, () => {
      phase = "failed";
      say = "No se pudo iniciar la instalación de Pi.";
      paint();
    });
  };
  const openNode = () => {
    work(() => call("pi_install_action", { action: "open_node" }), apply, () => {
      phase = "failed";
      say = "No se pudo abrir nodejs.org. Abrilo en tu navegador.";
      paint();
    });
  };
  function paint() {
    clear(box);
    box.hidden = !missing && phase !== "running" && phase !== "done";
    if (box.hidden) return;
    if (phase === "running") {
      box.append(h("span", { class: "agent-progress", role: "status" }, h("span", { class: "hub-progress indeterminate", "aria-hidden": "true" }, h("i")), h("span", { class: "hub-note", text: "Instalando Pi… no cierres Abito." })));
      return;
    }
    if (phase === "done") {
      box.append(h("p", { class: "hub-note", role: "status", text: already ? "Pi ya estaba instalado; el chat de Abito está listo." : "Pi quedó instalado y el chat de Abito arrancó." }));
      return;
    }
    if (phase === "failed" && say) box.append(h("p", { class: "hub-note notice err", role: "alert", text: say }));
    if (missing === "node") {
      box.append(
        h("p", { class: "hub-note", text: "El chat de Abito funciona con Pi, y Pi necesita Node.js, que no está instalado." }),
        action("Abrir nodejs.org", openNode)
      );
      return;
    }
    box.append(h("p", { class: "hub-note", text: `El chat de Abito funciona con Pi, que no está instalado. Instalalo con «Instalar Pi» o desde una terminal: ${PI_COMMAND}` }));
    if (!confirming) {
      box.append(action(phase === "failed" && reason !== "node_appeared" ? "Reintentar" : "Instalar Pi", () => {
        confirming = true;
        paint();
      }));
      return;
    }
    box.append(
      h("p", { class: "hub-note", id: "pi-install-plan", role: "status", text: PLAN }),
      action("Instalar ahora", run),
      action("Cancelar", () => {
        confirming = false;
        paint();
      })
    );
  }
  watch("pi-install:state", apply);
  work(() => call("pi_install_action", { action: "status" }), (v) => {
    if (v?.phase === "running") apply(v);
  }, () => {
  });
  paint();
  return { element: trusted(box), update(state) {
    const m = missingPiece(state);
    if (m !== missing) {
      missing = m;
      if (!m && phase === "failed") phase = "idle";
      paint();
    }
  } };
}
export {
  PI_COMMAND,
  missingPiece,
  piInstallControl
};
