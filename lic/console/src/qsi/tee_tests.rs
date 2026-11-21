// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hub and its viewers over loopback streams, with a console of
//! their own: a thread that edits a screen as the console does and
//! draws it to the tee and to a console port beside it.

use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qsipart::console::{self as wire, FromSystem, Grid, ToSystem};

use super::tee::{Ask, Hub, Poke, TAKEN_OVER};
use super::viewer::{self, Error, Seen, Viewer};
use super::*;
use crate::editor;
use crate::screen::Screen;

const WAIT: Duration = Duration::from_secs(5);

/// What the stand-in console is asked by the test.
enum Do {
    Show(Screen),
    Ask(Ask),
}

/// A port in memory, as a console port's peer reads it.
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

struct Rig {
    hub: Arc<Hub>,
    addr: std::net::SocketAddr,
    console: Sender<Do>,
    /// The keys the console was given.
    keys: Receiver<Key>,
    /// What a console port beside the tee was sent.
    port: Port,
}

/// The console: a screen with one input field, the tee and a console
/// port as its displays.
fn rig() -> Rig {
    let hub = Hub::new();
    let (tx, rx) = mpsc::channel::<Do>();
    let (keys_tx, keys) = mpsc::channel();
    let port = Port::default();
    let link = Link::new();
    link.attached();
    let mut beside = QsiDisplay::new(port.clone(), link, "beside");
    let mut tee = hub.display("test");
    std::thread::spawn(move || {
        let mut s = Screen::new();
        while let Ok(d) = rx.recv() {
            match d {
                Do::Show(n) => s = n,
                Do::Ask(Ask::Key(k)) => {
                    let _ = keys_tx.send(k);
                    let _ = editor::apply(&mut s, k);
                }
                Do::Ask(Ask::Redraw) => {}
            }
            let view = View::default();
            tee.draw_view(&s, &view).unwrap();
            beside.draw_view(&s, &view).unwrap();
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let poke: Poke = {
        let tx = Mutex::new(tx.clone());
        Arc::new(move |a| {
            let _ = tx.lock().unwrap().send(Do::Ask(a));
        })
    };
    let h = Arc::clone(&hub);
    std::thread::spawn(move || {
        for c in listener.incoming() {
            let (h, poke) = (Arc::clone(&h), Arc::clone(&poke));
            std::thread::spawn(move || h.serve(c.unwrap(), &poke));
        }
    });
    Rig {
        hub,
        addr,
        console: tx,
        keys,
        port,
    }
}

impl Rig {
    fn conn(&self) -> TcpStream {
        TcpStream::connect(self.addr).unwrap()
    }

    fn show(&self, s: &Screen) {
        self.console.send(Do::Show(s.clone())).unwrap();
    }

    /// The console port's grid once it shows `want` (it is drawn after
    /// the tee).
    fn beside_at(&self, want: &str) -> Grid {
        let end = std::time::Instant::now() + WAIT;
        loop {
            let g = self.beside();
            if g.text() == want {
                return g;
            }
            assert!(std::time::Instant::now() < end, "beside: {:?}", g.text());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The console port's grid, from everything it was sent.
    fn beside(&self) -> Grid {
        let bytes = self.port.0.lock().unwrap().clone();
        let mut r = qsipart::Reader::new();
        let mut g = Grid::default();
        // A piece at a time: a reader keeps only a few frames waiting.
        for piece in bytes.chunks(4096) {
            r.push(piece);
            for f in r.frames() {
                g.apply(&FromSystem::decode(&f).unwrap().unwrap()).unwrap();
            }
        }
        g
    }
}

/// Apply what the viewer sees until its grid shows `want`.
fn until(v: &mut Viewer<TcpStream>, g: &mut Grid, want: &str) -> Vec<Seen> {
    let mut seen = Vec::new();
    let end = std::time::Instant::now() + WAIT;
    while g.text() != want {
        assert!(std::time::Instant::now() < end, "viewer saw {:?}", g.text());
        let Some(s) = v.recv(Duration::from_millis(200)).unwrap() else {
            continue;
        };
        match &s {
            Seen::Frame(f) => g.apply(&FromSystem::Frame(f.clone())).unwrap(),
            Seen::Update(u) => g.apply(&FromSystem::Update(u.clone())).unwrap(),
            Seen::Ended(why) => panic!("ended: {why}"),
        }
        seen.push(s);
    }
    seen
}

fn sign_on() -> Screen {
    let mut s = Screen::new();
    s.put(1, 36, "Sign On", Attr::WHITE);
    s.put(6, 17, "User . . . . . .", Attr::GREEN);
    s.field(6, 53, 10, false);
    editor::home(&mut s);
    s
}

#[test]
fn the_viewer_sees_what_the_console_port_would() {
    let rig = rig();
    let s = sign_on();
    rig.show(&s);
    let mut v = viewer::attach(rig.conn(), false).unwrap();
    let mut grid = Grid::default();
    let seen = until(&mut v, &mut grid, &s.dump());
    assert!(matches!(seen[0], Seen::Frame(_)), "a whole frame first");
    // The same cells, attributes and status as the console port's.
    let beside = rig.beside_at(&s.dump());
    assert_eq!(grid.cells, beside.cells);
    assert_eq!(grid.status, beside.status);
    assert_eq!((grid.rows, grid.cols), (24, 80));
    // A change goes as an update.
    let mut s2 = s.clone();
    s2.put(3, 1, "changed", Attr::GREEN);
    rig.show(&s2);
    let seen = until(&mut v, &mut grid, &s2.dump());
    assert!(matches!(&seen[..], [Seen::Update(_)]));
    assert_eq!(grid.cells, rig.beside_at(&s2.dump()).cells);
    // A refresh: a whole frame again.
    v.send(&ToSystem::Refresh).unwrap();
    let frame = loop {
        if let Some(Seen::Frame(frame)) = v.recv(WAIT).unwrap() {
            break frame;
        }
    };
    assert_eq!(frame.cells, rig.beside().cells);
}

#[test]
fn keys_from_the_viewer_reach_the_console() {
    let r = rig();
    r.show(&sign_on());
    let mut v = viewer::attach(r.conn(), false).unwrap();
    let mut g = Grid::default();
    until(&mut v, &mut g, &sign_on().dump());
    for k in [wire::Key::Char('Q'), wire::Key::Char('S'), wire::Key::Function(25)] {
        v.send(&ToSystem::Key(k)).unwrap();
    }
    v.send(&ToSystem::Key(wire::Key::FieldExit)).unwrap();
    // As the keyboard path gives them; a key it has not, passed over.
    let got: Vec<Key> = (0..3).map(|_| r.keys.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(got, [Key::Char('Q'), Key::Char('S'), Key::FieldExit]);
    let mut want = sign_on();
    for k in [Key::Char('Q'), Key::Char('S'), Key::FieldExit] {
        let _ = editor::apply(&mut want, k);
    }
    until(&mut v, &mut g, &want.dump());
    assert_eq!(g.cells, r.beside_at(&want.dump()).cells);
}

#[test]
fn one_viewer_and_take_over() {
    let r = rig();
    r.show(&sign_on());
    let mut first = viewer::attach(r.conn(), false).unwrap();
    let mut g1 = Grid::default();
    until(&mut first, &mut g1, &sign_on().dump());
    // A second is refused unless it takes over.
    assert!(matches!(
        viewer::attach(r.conn(), false),
        Err(Error::Viewed)
    ));
    let mut second = viewer::attach(r.conn(), true).unwrap();
    let mut g2 = Grid::default();
    let seen = until(&mut second, &mut g2, &sign_on().dump());
    assert!(matches!(seen[0], Seen::Frame(_)));
    // The first is told and its stream ends.
    let why = loop {
        match first.recv(WAIT) {
            Ok(Some(Seen::Ended(why))) => break why,
            Ok(Some(_)) => {}
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(why, TAKEN_OVER);
    assert!(first.recv(Duration::from_millis(500)).is_err());
    // Its keys no longer count; the second's do.
    let _ = first.send(&ToSystem::Key(wire::Key::Char('X')));
    second.send(&ToSystem::Key(wire::Key::Char('Y'))).unwrap();
    assert_eq!(r.keys.recv_timeout(WAIT).unwrap(), Key::Char('Y'));
    // The second leaves: nobody views, and the next attaches freely.
    second.close();
    drop(second);
    let end = std::time::Instant::now() + WAIT;
    while r.hub.viewed() {
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut third = viewer::attach(r.conn(), false).unwrap();
    // Only the second's key was typed.
    let mut want = sign_on();
    let _ = editor::apply(&mut want, Key::Char('Y'));
    until(&mut third, &mut Grid::default(), &want.dump());
}

#[test]
fn the_screen_alone_without_a_viewer() {
    let r = rig();
    let s = sign_on();
    r.show(&s);
    let f = viewer::screen(r.conn()).unwrap();
    let mut g = Grid::default();
    g.apply(&FromSystem::Frame(f)).unwrap();
    assert_eq!(g.text(), s.dump());
    assert_eq!(g.cells, r.beside_at(&s.dump()).cells);
    assert!(!r.hub.viewed());
}

#[test]
fn a_slow_viewer_never_holds_the_console_up() {
    let r = rig();
    let mut v = viewer::attach(r.conn(), false).unwrap();
    // Whole screens of change, far more than the stream holds, while
    // the viewer reads nothing.
    let full = |c: char| {
        let mut s = Screen::new();
        for row in 1..=24 {
            s.put(row, 1, &c.to_string().repeat(80), Attr::WHITE);
        }
        s
    };
    let started = std::time::Instant::now();
    for i in 0..400 {
        r.show(&full(if i % 2 == 0 { 'A' } else { 'B' }));
    }
    let last = full('Z');
    r.show(&last);
    r.beside_at(&last.dump());
    assert!(started.elapsed() < WAIT, "the console waited");
    // The viewer catches up to the screen as it is.
    let mut g = Grid::default();
    let seen = until(&mut v, &mut g, &last.dump());
    assert!(seen.len() < 400, "nothing was dropped: {}", seen.len());
}

#[test]
fn a_zero_wait_takes_what_is_there_and_never_waits() {
    let rig = rig();
    rig.show(&sign_on());
    let mut v = viewer::attach(rig.conn(), false).unwrap();
    let mut grid = Grid::default();
    let end = std::time::Instant::now() + WAIT;
    while grid.text() != sign_on().dump() {
        assert!(std::time::Instant::now() < end, "nothing at a zero wait");
        let t = std::time::Instant::now();
        let got = v.recv(Duration::ZERO).unwrap();
        assert!(t.elapsed() < Duration::from_millis(100), "a zero wait waited");
        match got {
            Some(Seen::Frame(f)) => grid.apply(&FromSystem::Frame(f)).unwrap(),
            Some(Seen::Update(u)) => grid.apply(&FromSystem::Update(u)).unwrap(),
            Some(Seen::Ended(why)) => panic!("{why}"),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[test]
fn nothing_goes_out_with_no_viewer() {
    let hub = Hub::new();
    let mut tee = hub.display("test");
    // No viewer: the tee draws nothing and keeps nothing.
    tee.draw(&sign_on()).unwrap();
    assert!(!hub.viewed());
    assert!(tee.describe().contains("test"));
}
