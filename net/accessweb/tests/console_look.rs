// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The two ways into the terminal's look draw one picture (handoff 23,
//! docs/ACCESSWEB.md "The look for a product outside QS/400"): the
//! shipped sign-on display (`QDSIGNON`) as one 5250 data stream, drawn
//! once as this product's session draws it and once as the console
//! shows it on a partition's console port: through the console's
//! terminal, sent as `qsipart` frames, kept as a peer's grid.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use console::display::{Display, View as ConsoleView};
use console::qsi::{Link, QsiDisplay};
use console::terminal::ConsoleTerminal;
use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, NoRefs, Source, compile_dspf};
use qsipart::console::{FromSystem, Grid};
use termlook::look::render::{Keyboard, Scheme};
use termlook::screen::{grid, s5250};
use tn5250::ds::attr;
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Read, ScreenModel};
use tn5250::term::Terminal;

const SYSTEM: &str = "QSHQ400";

/// The SIGNON format written as `QTVTELNET` writes it.
fn sign_on() -> Record {
    let (src, diags) = Source::from_text(dspf::sys::QDSIGNON);
    assert!(diags.is_empty(), "{diags:?}");
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    let df = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let rec = df.record("SIGNON").expect("format").clone();
    let values = [
        ("QSYSNAME", SYSTEM),
        ("SBSNAME", "QINTER"),
        ("DEVNAME", "QPADEV0003"),
    ];
    let mut out = Vec::new();
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        let v = values
            .iter()
            .find(|(n, _)| f.name.as_deref() == Some(*n))
            .map_or("", |(_, v)| v);
        let mut b = ccsid::to_ebcdic(&format!("{v:<w$}", w = bf.len as usize)).unwrap();
        b.truncate(bf.len as usize);
        out.extend(b);
    }
    let mut rt = Runtime::new(df, false);
    let mut m = ScreenModel::new(false);
    rt.write(&mut m, &Env::default(), "SIGNON", &out, &[])
        .unwrap();
    let o = m.render(Read::MdtFields);
    Record::new(o.opcode, o.data)
}

/// As the session draws it: the terminal's buffer as a 5250 screen
/// (what `work/qsjob`'s session job publishes).
fn as_the_session(rec: &Record) -> s5250::View {
    let mut t = Terminal::new(Capabilities {
        model: Model::M3179,
        gui: false,
        color: true,
    });
    t.apply(rec).unwrap();
    let mut s =
        s5250::Screen::from_buffer(t.rows(), t.cols(), t.cells(), attr::is_attr, ccsid::char_of);
    s.cursor = t.cursor();
    s.keyboard = if t.is_locked() {
        Keyboard::System
    } else {
        Keyboard::Open
    };
    s5250::View {
        screen: s,
        system: SYSTEM.into(),
        connected: true,
        tls: true,
        scheme: Scheme::Console,
    }
}

#[derive(Clone, Default)]
struct Port(Arc<Mutex<Vec<u8>>>);

impl Write for Port {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// As the console shows it on its port: the frames a peer receives,
/// applied to its grid.
fn as_the_console(rec: &Record) -> grid::View {
    let mut term = ConsoleTerminal::new();
    term.apply(rec);
    let port = Port::default();
    let link = Link::new();
    link.attached();
    let mut d = QsiDisplay::new(port.clone(), Arc::clone(&link), "test");
    let view = ConsoleView {
        inhibited: term.locked(),
        session: true,
        ..ConsoleView::default()
    };
    d.draw_view(&term.screen(), &view).unwrap();
    let mut r = qsipart::Reader::new();
    r.push(&port.0.lock().unwrap());
    let mut g = Grid::default();
    for f in r.frames() {
        g.apply(&FromSystem::decode(&f).unwrap().unwrap()).unwrap();
    }
    assert!(g.ready());
    grid::View {
        grid: g,
        system: SYSTEM.into(),
        connected: true,
        tls: true,
        scheme: Scheme::Console,
    }
}

#[test]
fn the_sign_on_draws_alike_from_both_ways_in() {
    let rec = sign_on();
    let session = as_the_session(&rec);
    let console = as_the_console(&rec);
    let (a, b) = (session.shown(), console.shown());
    assert_eq!(a.text_rows(), b.text_rows());
    assert_eq!(a.status, b.status);
    // The console marks white high intensity, the session's byte does
    // not: the same colour either way.
    let (pa, pb) = (a.picture(1), b.picture(1));
    for (i, (x, y)) in pa.px.chunks(3).zip(pb.px.chunks(3)).enumerate() {
        assert_eq!(x, y, "pixel {} of row {}", i % pa.w, i / pa.w);
    }
    for scale in [1, 2] {
        assert!(session.png(scale) == console.png(scale), "scale {scale}");
    }
    let off = |v: &mut grid::View| v.connected = false;
    let mut gone = console.clone();
    off(&mut gone);
    let mut last = session.clone();
    last.connected = false;
    assert!(last.png(1) == gone.png(1), "screen off");
}

/// QS/MC's prototype picture `shared/img/5250-24x80.png` is this
/// product's Work with Active Jobs picture with another user name in
/// two rows: with `QSMC_IMG` (its path) and `QSMC_USER` (that name,
/// eight characters) the fixture screen with the name in place draws
/// it byte for byte.
#[test]
fn the_management_console_picture() {
    let (Some(img), Some(user)) = (
        std::env::var_os("QSMC_IMG"),
        std::env::var("QSMC_USER").ok(),
    ) else {
        return;
    };
    let want = std::fs::read(img).unwrap();
    let mut s = accessweb::fixture::session5250::wrkactjob(24, 80);
    let text = accessweb::fixture::session5250::WRKACTJOB.replace("TESTUSER", &user);
    s.cells = s5250::Screen::from_text(&text, 24, 80, s.cursor).cells;
    let v = s5250::View {
        screen: s,
        system: SYSTEM.into(),
        connected: true,
        tls: true,
        scheme: Scheme::Console,
    };
    assert!(v.png(1) == want, "differs from QS/MC's picture");
}
