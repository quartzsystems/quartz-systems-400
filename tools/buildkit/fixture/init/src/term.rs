// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The web product's terminal page on the look crate (`termlook`,
//! handoff 23): QSDEMO's menu as its console sends it on a partition's
//! console port (`qsipart` frames written by the console's own
//! display), the frames applied to a peer's grid, the grid drawn as
//! the framebuffer draws it. The page carries the picture, the keypad
//! (posted without script) and the script's element; a posted key and
//! a socket's key message decode to the console's keys. This module
//! takes nothing of the fixture, so the golden picture is made from it
//! too (`docs/BUILDKIT.md` section 18).

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use console::Screen;
use console::display::{Display, View};
use console::qsi::{Link, QsiDisplay};
use qsipart::console::{FromSystem, Grid, Key};
use termlook::exchange::{Input, keys};
use termlook::look::render::Scheme;
use termlook::screen::grid;

/// The name the status row shows.
pub const SYSTEM: &str = "QSDEMO";

/// The console's screen the page shows.
pub fn screen() -> Screen {
    scrkit::ui::menu(
        "QSDEMO Service Tools",
        SYSTEM,
        &[(1, "Work with the protected set"), (2, "Work with routes")],
        "F3=Exit   F12=Cancel",
    )
}

#[derive(Clone, Default)]
struct Port(Arc<Mutex<Vec<u8>>>);

impl Write for Port {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("port"))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The bytes the console port carries for `s` to a peer that just
/// attached: the hello and a whole frame.
pub fn frames(s: &Screen) -> Result<Vec<u8>, String> {
    let port = Port::default();
    let link = Link::new();
    link.attached();
    let mut d = QsiDisplay::new(port.clone(), Arc::clone(&link), "web");
    d.draw_view(s, &View::default())
        .map_err(|e| format!("console port: {e}"))?;
    let bytes = port.0.lock().map_err(|_| "port")?.clone();
    Ok(bytes)
}

/// A peer's grid from the port's bytes.
pub fn grid_of(bytes: &[u8]) -> Result<Grid, String> {
    let mut r = qsipart::Reader::new();
    r.push(bytes);
    let mut g = Grid::default();
    for f in r.frames() {
        let m = FromSystem::decode(&f)
            .map_err(|e| format!("frame: {e}"))?
            .ok_or("a message the peer does not know")?;
        g.apply(&m).map_err(|e| format!("grid: {e}"))?;
    }
    if g.ready() {
        Ok(g)
    } else {
        Err("no whole frame came".into())
    }
}

/// The grid with the page's state.
pub fn view(g: Grid) -> grid::View {
    grid::View {
        grid: g,
        system: SYSTEM.into(),
        connected: true,
        tls: true,
        scheme: Scheme::Console,
    }
}

/// The picture the page serves: the screen sent, received and drawn.
pub fn png() -> Result<Vec<u8>, String> {
    Ok(view(grid_of(&frames(&screen())?)?).png(1))
}

/// The console key a posted keypad label is.
pub fn posted(label: &str) -> Option<Key> {
    keys::keypad(label).and_then(keys::console_key)
}

/// The console keys a socket's message is.
pub fn from_socket(msg: &str) -> Vec<Key> {
    Input::parse(msg).map(|i| i.console_keys()).unwrap_or_default()
}

/// The page's body: the picture the script replaces, the keypad as a
/// form, the script's element.
pub fn body(url: &dyn Fn(&str) -> String, csrf_field: &str, csrf: &str) -> String {
    let (w, h) = termlook::screen::size(24, 80, 1);
    let mut s = format!(
        "<img id=\"term\" alt=\"Console\" src=\"{}\" width=\"{w}\" height=\"{h}\">\n<form method=\"post\" action=\"{}\">\n<input type=\"hidden\" name=\"{csrf_field}\" value=\"{csrf}\">\n",
        url("/terminal/screen.png"),
        url("/terminal/key"),
    );
    for (label, _) in keys::KEYPAD {
        s.push_str(&format!(
            "<input type=\"submit\" name=\"k\" value=\"{label}\">\n"
        ));
    }
    s.push_str(&format!(
        "</form>\n<script src=\"{}\" data-socket=\"{}\" data-image=\"term\" data-scale=\"1\"></script>\n",
        url("/terminal/script.js"),
        url("/terminal/ws"),
    ));
    s
}

/// The script as served.
pub fn script() -> String {
    termlook::script::text(keys::KEYPAD)
}
