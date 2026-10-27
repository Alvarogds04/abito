//! #315 — tray state derivation and glyph icons (F098/GD011).
//!
//! The tray icon reflects the app with four states, per
//! `docs/desktop-visual-language.md` §10.3 — four glyphs distinguished by
//! SHAPE, not only by color:
//! - `idle` (Listo, `--go` #bdee63): clean ring,
//! - `working` (En curso, `--island-live` #63b3ff): dashed ring,
//! - `needs-attention` (Espera tu sí, `--sun` #ffd23f): ring + badge dot,
//! - `disconnected` (No disponible, `--faint` #6f6d66): dimmed ring + slash.
//!
//! Everything here is pure and headless-testable; `trayhost` wires it to the
//! real TrayIcon. Non-Unix builds run the same pure code (tests cover it) and
//! get an honest no-op tray host.

use serde::{Deserialize, Serialize};

/// The tray state vocabulary, exactly the four names of §10.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayState {
    Idle,
    Working,
    NeedsAttention,
    Disconnected,
}

impl TrayState {
    // The state-name vocabulary is part of the #315 contract and asserted by
    // tests; production reads the enum directly, so keep this method for the
    // contract check without a dead-code warning in release builds.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn as_str(self) -> &'static str {
        match self {
            TrayState::Idle => "idle",
            TrayState::Working => "working",
            TrayState::NeedsAttention => "needs-attention",
            TrayState::Disconnected => "disconnected",
        }
    }

    /// Stable index for the atomic last-state cache (never a guess: the
    /// unknown value fails closed to [`TrayState::Disconnected`], the honest
    /// "we cannot tell" glyph).
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => TrayState::Idle,
            1 => TrayState::Working,
            2 => TrayState::NeedsAttention,
            _ => TrayState::Disconnected,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn to_index(self) -> u8 {
        match self {
            TrayState::Idle => 0,
            TrayState::Working => 1,
            TrayState::NeedsAttention => 2,
            TrayState::Disconnected => 3,
        }
    }

    /// Human tooltip line, the island's established Spanish copy.
    pub fn label(self) -> &'static str {
        match self {
            TrayState::Idle => "Abito — Listo",
            TrayState::Working => "Abito — En curso",
            TrayState::NeedsAttention => "Abito — Espera tu sí",
            TrayState::Disconnected => "Abito — No disponible",
        }
    }
}

/// The tray state derivation (§10.3 priority):
/// 1. `needs-attention` — a pending approval card or a pending question asks
///    for the human's decision, over everything else;
/// 2. `disconnected` — the agent connection is anything but
///    `connected`/`working` (no_daemon, not_connected, outdated, error,
///    unavailable…), or the local transport failed: unknown maps here too
///    (fail closed, never a guess);
/// 3. `working` — connected and a turn is in flight;
/// 4. `idle` — connected and nothing pending.
pub fn derive(
    connection: &str,
    card_pending: bool,
    question_pending: bool,
    transport_unavailable: bool,
) -> TrayState {
    // 1. a decision the human owes wins over every machine state
    if card_pending || question_pending {
        return TrayState::NeedsAttention;
    }
    // 2. only these two names are a live connection; everything else (and an
    //    unknown name) fails closed to the honest unavailable glyph
    let live = matches!(connection, "connected" | "working");
    if !live || transport_unavailable {
        return TrayState::Disconnected;
    }
    // 3/4. connected and nothing pending: busy or ready
    if connection == "working" {
        TrayState::Working
    } else {
        TrayState::Idle
    }
}

/// A 32×32 RGBA glyph, drawn by pure pixel rules (see module docs). The four
/// variants differ by SHAPE, not only by color: continuous ring, dashed ring,
/// ring + badge dot, dimmed ring + diagonal slash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn glyph_rgba(state: TrayState) -> Glyph {
    const SIZE: u32 = 32;
    const CENTER: f64 = 16.0;
    const RING_R: f64 = 11.0;
    const STROKE: f64 = 1.4; // half-width: the ring band is ~2.8 px

    // §1.1 tokens: --go, --island-live, --sun, --faint
    let (r, g, b, dim) = match state {
        TrayState::Idle => (0xbd, 0xee, 0x63, 255u8),
        TrayState::Working => (0x63, 0xb3, 0xff, 255),
        TrayState::NeedsAttention => (0xff, 0xd2, 0x3f, 255),
        TrayState::Disconnected => (0x6f, 0x6d, 0x66, 150), // dimmed ring
    };

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mut put = |x: u32, y: u32, a: u8| {
        if x < SIZE && y < SIZE {
            let i = ((y * SIZE + x) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = a;
        }
    };

    let dist = |x: f64, y: f64| ((x - CENTER).powi(2) + (y - CENTER).powi(2)).sqrt();
    // dashed working ring: 8 segments, every other 45° arc transparent
    let dashed = state == TrayState::Working;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let d = dist(fx, fy);
            if (d - RING_R).abs() <= STROKE {
                if dashed {
                    // angle from the center, 0 at 12 o'clock, clockwise
                    let ang = (fx - CENTER).atan2(CENTER - fy); // -π..π
                    let deg = ang.to_degrees();
                    let seg = ((deg + 360.0) % 360.0 / 45.0).floor() as i32;
                    if seg % 2 == 1 {
                        continue; // gap: shape carries the state, not the color
                    }
                }
                put(x, y, dim);
            }
        }
    }

    if state == TrayState::NeedsAttention {
        // badge dot, top-right and OFF the ring stroke (per §10.3: insignia)
        let (bx, by, br) = (CENTER + 7.0, CENTER - 7.0, 3.4);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                if (fx - bx).powi(2) + (fy - by).powi(2) <= br * br {
                    put(x, y, 255);
                }
            }
        }
    }

    if state == TrayState::Disconnected {
        // diagonal slash (bottom-left -> top-right), over the dimmed ring
        let slash = |px: f64, py: f64| {
            // distance to the line through (7,25)-(25,7): x + y = 32
            let d = (px + py - 32.0).abs() / std::f64::consts::SQRT_2;
            let inside = (6.0..=26.0).contains(&px) && (6.0..=26.0).contains(&py);
            d <= 1.6 && inside
        };
        for y in 0..SIZE {
            for x in 0..SIZE {
                if slash(x as f64 + 0.5, y as f64 + 0.5) {
                    put(x, y, 255); // the slash itself stays crisp
                }
            }
        }
    }

    Glyph {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// One entry of the tray menu: a plain item, a check item (autostart) or a separator. Pure data: the same spec builds the real menu on
/// every platform and is tested natively everywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(&'static str, &'static str),
    Check(&'static str, &'static str),
    Separator,
}

/// HU-048: the tray menu. Identical on every platform except for «Arrancar al iniciar sesión», which is offered only where the autostart
/// toggle really works (`autostart_supported`: a desktop entry on Linux/macOS today). Windows has no Run-key implementation, so the item
/// and its separator are omitted there rather than shown and failing. «Salir» is always last.
pub fn menu_spec(autostart_supported: bool) -> Vec<MenuEntry> {
    let mut menu = vec![MenuEntry::Item("tray-show", "Mostrar Abito"), MenuEntry::Item("tray-hide", "Ocultar"), MenuEntry::Separator];
    if autostart_supported {
        menu.push(MenuEntry::Check("tray-autostart", "Arrancar al iniciar sesión"));
        menu.push(MenuEntry::Separator);
    }
    menu.push(MenuEntry::Item("tray-quit", "Salir"));
    menu
}

#[cfg(test)]
#[path = "tray_tests.rs"]
mod tests;
