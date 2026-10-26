//! Active-window context (#487, NotchBuddy feature 8).
//!
//! Adapted from Coucou NotchBuddy/Sources/App/WindowContextCapture.swift @71b773f,
//! Copyright (c) 2026 Louis Raillé, MIT. Rewritten for Linux: no browser URL in v1 (#483 S3).
//!
//! NotchBuddy attaches `Context — App: <app>, Window: <title>` to the FIRST chat message,
//! taken from the last app that was active before the island (`lastExternalApp`). The island
//! is itself the focused window when the chat opens, so the reader returns the most recent
//! window that is NOT Abito.
//!
//! Sources, best first:
//! 1. Abito's own GNOME Shell extension (`gnome-extension/abito-context@abito`) on the session
//!    bus: `org.abito.Context.GetRecentWindows() -> a(sss)` (app id, app name, title), most
//!    recent first.
//! 2. X11/EWMH (`_NET_CLIENT_LIST_STACKING`, topmost first) over the existing x11rb; on GNOME
//!    Wayland this only sees XWayland windows, never guessed.
//!
//! Automatic capture runs only on chat open with the switch on. Explicit character drags use
//! a separate point query, including while the switch is off. No browser URL or page-chosen window.
use std::time::Duration;
use crate::characterwin::Rect;

// Adapted from Coucou NotchBuddy/Sources/App/IslandWindowController.swift @71b773f, Copyright (c) 2026 Louis Raillé, MIT.
/// Topmost-first hit test. The input and rectangles must use the same coordinate space.
pub fn topmost_at(point: (f64, f64), rows: &[(Candidate, Rect)]) -> Option<(ActiveWindow, Rect)> {
    rows.iter().find_map(|(c, r)| {
        if !r.contains(point) { return None; }
        pick_external(std::slice::from_ref(c)).map(|w| (w, *r))
    })
}

pub const POINT_METHOD: &str = "GetWindowAt";
type PointRow = (String, String, String, i32, i32, i32, i32);

fn point_rows(rows: Vec<PointRow>) -> Vec<(Candidate, Rect)> {
    rows.into_iter().map(|(app_id, app_name, title, x, y, w, h)| (
        Candidate { app_id, app_name, title },
        Rect { x: x as f64, y: y as f64, w: w as f64, h: h as f64 },
    )).collect()
}

/// A window as either source reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub app_id: String,
    pub app_name: String,
    pub title: String,
}

/// What the chat attaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWindow {
    pub app: String,
    pub title: String,
}

pub const BUS_NAME: &str = "org.abito.Context";
pub const OBJECT_PATH: &str = "/org/abito/Context";
pub const INTERFACE: &str = "org.abito.Context";
pub const METHOD: &str = "GetRecentWindows";
/// The extension answers from memory; anything slower is treated as absent.
pub const DBUS_DEADLINE: Duration = Duration::from_millis(800);

/// True for Abito's own windows (app id `com.abitog.AbitoTauri`, WM_CLASS, desktop file).
pub fn is_abito(c: &Candidate) -> bool {
    c.app_id.to_lowercase().contains("abito") || c.app_name.to_lowercase().contains("abito")
}

/// Most recent window that is not Abito and has an app name (NotchBuddy: `guard let appName`).
pub fn pick_external(recent: &[Candidate]) -> Option<ActiveWindow> {
    recent
        .iter()
        .find(|c| !is_abito(c) && !c.app_name.trim().is_empty())
        .map(|c| ActiveWindow { app: c.app_name.trim().to_string(), title: c.title.trim().to_string() })
}

/// The exact text NotchBuddy sends to the model (ClaudeService.swift: first message only).
pub fn format_context(w: &ActiveWindow) -> String {
    format!("Context — App: {}, Window: {}", w.app, w.title)
}

/// Build candidates from the extension's `a(sss)` reply.
pub fn from_reply(rows: Vec<(String, String, String)>) -> Vec<Candidate> {
    rows.into_iter().map(|(app_id, app_name, title)| Candidate { app_id, app_name, title }).collect()
}

/// Build candidates from X data: `(instance, class, title)` per window, TOPMOST LAST
/// (the EWMH stacking order); returned most recent first.
pub fn from_stacking(windows: Vec<(String, String, String)>) -> Vec<Candidate> {
    windows
        .into_iter()
        .rev()
        .map(|(instance, class, title)| {
            let app_name = if class.is_empty() { instance.clone() } else { class.clone() };
            Candidate { app_id: format!("{instance} {class}"), app_name, title }
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub mod dbus {
    use super::*;
    use zbus::blocking::{Connection, Proxy};

    fn connect(bus_address: Option<&str>) -> zbus::Result<Connection> {
        match bus_address {
            Some(addr) => zbus::blocking::connection::Builder::address(addr)?.build(),
            None => Connection::session(),
        }
    }

    /// Presence only: never activate the extension or ask for any window data.
    pub fn available(bus_address: Option<&str>) -> bool {
        let addr = bus_address.map(str::to_owned);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let answer = (|| -> Option<bool> {
                let conn = connect(addr.as_deref()).ok()?;
                let bus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
                bus.name_has_owner(zbus::names::BusName::try_from(BUS_NAME).ok()?).ok()
            })().unwrap_or(false);
            let _ = tx.send(answer);
        });
        rx.recv_timeout(DBUS_DEADLINE).unwrap_or(false)
    }

    fn ask(bus_address: Option<&str>) -> Option<Vec<Candidate>> {
        let conn = connect(bus_address).ok()?;
        // Only call a bus name somebody owns: never trigger activation of anything.
        let dbus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
        let name = zbus::names::BusName::try_from(BUS_NAME).ok()?;
        if !dbus.name_has_owner(name).ok()? {
            return None;
        }
        let proxy = Proxy::new(&conn, BUS_NAME, OBJECT_PATH, INTERFACE).ok()?;
        let rows: Vec<(String, String, String)> = proxy.call(METHOD, &()).ok()?;
        Some(from_reply(rows))
    }

    /// An empty reply is authoritative (no visible target); None permits the X11 fallback.
    pub fn window_at(point: (i32, i32)) -> Option<Vec<(Candidate, Rect)>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let answer = (|| {
                let conn = connect(None).ok()?;
                let bus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
                if !bus.name_has_owner(zbus::names::BusName::try_from(BUS_NAME).ok()?).ok()? { return None; }
                let proxy = Proxy::new(&conn, BUS_NAME, OBJECT_PATH, INTERFACE).ok()?;
                let rows: Vec<PointRow> = proxy.call(POINT_METHOD, &point).ok()?;
                (rows.len() <= 1).then(|| point_rows(rows))
            })();
            let _ = tx.send(answer);
        });
        rx.recv_timeout(DBUS_DEADLINE).ok().flatten()
    }

    /// Recent windows from the extension; `None` when it is absent, slow or malformed.
    /// `bus_address` None = the user's session bus; Some(addr) = a private bus (tests).
    pub fn recent_windows(bus_address: Option<&str>) -> Option<Vec<Candidate>> {
        let addr = bus_address.map(str::to_owned);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ask(addr.as_deref()));
        });
        rx.recv_timeout(DBUS_DEADLINE).ok().flatten()
    }
}

#[cfg(target_os = "linux")]
pub mod x11 {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    use x11rb::rust_connection::RustConnection;

    fn intern(conn: &RustConnection, name: &[u8]) -> Option<u32> {
        conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom)
    }

    fn prop(conn: &RustConnection, window: u32, atom: u32, ty: u32) -> Vec<u8> {
        conn.get_property(false, window, atom, ty, 0, u32::MAX)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.value)
            .unwrap_or_default()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// Visible EWMH clients, topmost first, with root-relative physical geometry.
    pub fn windows_with_rects() -> Option<Vec<(Candidate, Rect)>> {
        use x11rb::protocol::xproto::MapState;
        let (conn, screen) = std::panic::catch_unwind(|| RustConnection::connect(None)).ok()?.ok()?;
        let root = conn.setup().roots[screen].root;
        let list = intern(&conn, b"_NET_CLIENT_LIST_STACKING")?;
        let ids: Vec<u32> = conn.get_property(false, root, list, AtomEnum::WINDOW, 0, u32::MAX)
            .ok()?.reply().ok()?.value32()?.collect();
        let utf8 = intern(&conn, b"UTF8_STRING")?;
        let name = intern(&conn, b"_NET_WM_NAME")?;
        let state = intern(&conn, b"_NET_WM_STATE")?;
        let hidden = intern(&conn, b"_NET_WM_STATE_HIDDEN")?;
        let skip = intern(&conn, b"_NET_WM_STATE_SKIP_TASKBAR")?;
        let mut rows = Vec::new();
        for id in ids.into_iter().rev() {
            let Some(attrs) = conn.get_window_attributes(id).ok().and_then(|c| c.reply().ok()) else { continue };
            if attrs.map_state != MapState::VIEWABLE { continue; }
            let states = prop(&conn, id, state, AtomEnum::ATOM.into());
            if states.chunks_exact(4).any(|b| {
                let a = u32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
                a == hidden || a == skip
            }) { continue; }
            let Some(g) = conn.get_geometry(id).ok().and_then(|c| c.reply().ok()) else { continue };
            let Some(p) = conn.translate_coordinates(id, root, 0, 0).ok().and_then(|c| c.reply().ok()) else { continue };
            let class = prop(&conn, id, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into());
            let mut parts = class.split(|b| *b == 0);
            let instance = text(parts.next().unwrap_or(&[]));
            let class = text(parts.next().unwrap_or(&[]));
            let mut title = prop(&conn, id, name, utf8);
            if title.is_empty() { title = prop(&conn, id, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()); }
            let mut candidates = from_stacking(vec![(instance, class, text(&title))]);
            rows.push((candidates.remove(0), Rect { x: p.dst_x as f64, y: p.dst_y as f64, w: g.width as f64, h: g.height as f64 }));
        }
        Some(rows)
    }

    /// Windows of the display, most recent (topmost) first. `None` when X is unreachable.
    pub fn recent_windows(display: Option<&str>) -> Option<Vec<Candidate>> {
        let owned = display.map(str::to_owned);
        let (conn, screen) = std::panic::catch_unwind(move || RustConnection::connect(owned.as_deref()))
            .ok()?
            .ok()?;
        let root = conn.setup().roots[screen].root;
        let list_atom = intern(&conn, b"_NET_CLIENT_LIST_STACKING")?;
        let ids: Vec<u32> = conn
            .get_property(false, root, list_atom, AtomEnum::WINDOW, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()?
            .value32()
            .map(|it| it.collect())
            .unwrap_or_default();
        let utf8 = intern(&conn, b"UTF8_STRING")?;
        let net_name = intern(&conn, b"_NET_WM_NAME")?;
        let rows = ids
            .into_iter()
            .map(|id| {
                let class = prop(&conn, id, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into());
                let mut parts = class.split(|b| *b == 0);
                let instance = text(parts.next().unwrap_or(&[]));
                let class = text(parts.next().unwrap_or(&[]));
                let mut title = prop(&conn, id, net_name, utf8);
                if title.is_empty() {
                    title = prop(&conn, id, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into());
                }
                (instance, class, text(&title))
            })
            .collect();
        Some(from_stacking(rows))
    }
}

/// The context to attach, or `None`. Extension first, X11 fallback.
#[cfg(not(target_os = "linux"))]
pub fn capture(_bus_address: Option<&str>, _display: Option<&str>) -> Option<ActiveWindow> { None }

#[cfg(not(target_os = "linux"))]
pub fn capture_at(_point: (f64, f64), _scale: f64) -> Option<(ActiveWindow, Rect)> { None }

#[cfg(target_os = "linux")]
pub fn capture(bus_address: Option<&str>, display: Option<&str>) -> Option<ActiveWindow> {
    if let Some(found) = dbus::recent_windows(bus_address).as_deref().and_then(pick_external) {
        return Some(found);
    }
    x11::recent_windows(display).as_deref().and_then(pick_external)
}

/// Explicit drag capture: physical pointer -> GNOME stage logical px using the display under it.
/// Extension geometry is converted back to physical for the input-free highlight window.
#[cfg(target_os = "linux")]
pub fn capture_at(point: (f64, f64), scale: f64) -> Option<(ActiveWindow, Rect)> {
    let k = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let logical = ((point.0 / k).floor(), (point.1 / k).floor());
    if let Some(rows) = dbus::window_at((logical.0 as i32, logical.1 as i32)) {
        return topmost_at(logical, &rows).map(|(w, r)| (w, Rect { x: r.x * k, y: r.y * k, w: r.w * k, h: r.h * k }));
    }
    topmost_at(point, &x11::windows_with_rects()?)
}

/// The chat's attach point (NotchBuddy `promptContext`): captured once when the chat opens and
/// nothing is held; offered ONLY for the first message of a conversation (`conversationMessages
/// .isEmpty`), where the caller prefixes that user message with it. Pure state; the caller
/// supplies the read, so nothing here touches D-Bus or X11.
#[derive(Debug, Default)]
pub struct ChatContext {
    captured: Option<String>,
    dragged: Option<String>,
}

impl ChatContext {
    /// Explicit human intent is independent of the automatic-capture switch.
    pub fn attach_dragged(&mut self, text: String) { self.dragged = Some(text); }
    pub fn first_message_consumed(&mut self) { self.dragged = None; }
    pub fn new_conversation(&mut self) { self.dragged = None; }

    /// Chat opened. Switch off: forget automatic context and read NOTHING. Preserve explicit drag.
    pub fn on_chat_open(&mut self, enabled: bool, read: impl FnOnce() -> Option<ActiveWindow>) {
        if !enabled {
            self.captured = None;
            return;
        }
        if self.captured.is_none() {
            self.captured = read().map(|w| format_context(&w));
        }
    }

    /// First message only: explicit drag wins regardless of the automatic-capture switch.
    pub fn for_turn(&self, enabled: bool, first: bool) -> Option<String> {
        if !first { return None; }
        self.dragged.clone().or_else(|| if enabled { self.captured.clone() } else { None })
    }
}

#[cfg(test)]
#[path = "contextwin_tests.rs"]
mod tests;
