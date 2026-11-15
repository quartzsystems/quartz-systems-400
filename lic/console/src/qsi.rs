// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console on a partition's console port (`qsipart::console`): a
//! [`Display`] that sends the grid as cells, the attributes, the cursor
//! and the indicators, a whole frame when a peer attaches or asks and
//! only what changed afterwards; and the port's keys back as the keys
//! the keyboard path gives ([`Key`]), so nothing above the keyboard
//! knows the difference.
//!
//! The port is written without blocking, one frame a write. With no
//! peer, or a peer too slow to take a frame, nothing is kept: the next
//! draw it sees is a whole frame. [`Link`] is what the display and the
//! port's reader (`spawn_reader`, Linux) share.

use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qsipart::console::{self as wire, FromSystem, Shape, Status, attr, indicator};

use crate::display::{Display, View};
use crate::keyboard::map::Key;
use crate::scale::Cells;
use crate::screen::{Attr, Color};

/// What the display and the port's reader share.
#[derive(Debug, Default)]
pub struct Link {
    /// A peer has the port open.
    connected: AtomicBool,
    /// The next draw sends the hello and a whole frame.
    whole: AtomicBool,
    /// A frame was dropped because the port would not take it.
    stalled: AtomicBool,
}

impl Link {
    /// A link with no peer yet.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A peer attached: hello and a whole frame next.
    pub fn attached(&self) {
        self.whole.store(true, Ordering::SeqCst);
        self.connected.store(true, Ordering::SeqCst);
    }

    /// The peer went away.
    pub fn detached(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    /// The peer asked for a whole frame.
    pub fn want_whole(&self) {
        self.whole.store(true, Ordering::SeqCst);
    }

    /// Whether a peer is attached.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Whether a frame was dropped since the last call.
    pub fn take_stalled(&self) -> bool {
        self.stalled.swap(false, Ordering::SeqCst)
    }
}

/// A cell's attribute bits on the wire.
#[must_use]
pub fn attr_bits(a: Attr) -> u16 {
    let color = match a.color {
        Color::Green => attr::GREEN,
        Color::White => attr::WHITE,
        Color::Red => attr::RED,
        Color::Blue => attr::BLUE,
        Color::Turquoise => attr::TURQUOISE,
        Color::Pink => attr::PINK,
        Color::Yellow => attr::YELLOW,
    };
    [
        (a.high, attr::HIGH),
        (a.reverse, attr::REVERSE),
        (a.underline, attr::UNDERLINE),
        (a.blink, attr::BLINK),
        (a.column_sep, attr::COLUMN_SEP),
        (a.nondisplay, attr::NONDISPLAY),
        (a.continuation, attr::CONTINUATION),
        (a.double_width, attr::DOUBLE_WIDTH),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .fold(color, |b, (_, bit)| b | bit)
}

/// A key from the port as the keyboard path gives it; `None` for one a
/// later version has, or a function key past 24.
#[must_use]
pub fn key(k: wire::Key) -> Option<Key> {
    use wire::Key as W;
    Some(match k {
        W::Char(c) if !c.is_control() => Key::Char(c),
        W::Enter => Key::Enter,
        W::FieldAdvance => Key::FieldAdvance,
        W::FieldBackspace => Key::FieldBackspace,
        W::FieldExit => Key::FieldExit,
        W::Backspace => Key::Backspace,
        W::Home => Key::Home,
        W::Insert => Key::Insert,
        W::Delete => Key::Delete,
        W::Function(n) if (1..=24).contains(&n) => Key::Function(n),
        W::Attention => Key::Attention,
        W::SysReq => Key::SysReq,
        W::Reset => Key::Reset,
        W::Left => Key::Left,
        W::Right => Key::Right,
        W::Up => Key::Up,
        W::Down => Key::Down,
        W::PageUp => Key::PageUp,
        W::PageDown => Key::PageDown,
        W::Char(_) | W::Function(_) | W::Other(_) => return None,
    })
}

/// What was last sent: the peer's grid.
struct Shown {
    rows: u8,
    cols: u8,
    cells: Vec<wire::Cell>,
    status: Status,
}

/// The grid on a console port.
pub struct QsiDisplay<W: Write + Send> {
    out: W,
    link: Arc<Link>,
    shown: Option<Shown>,
    seq: u32,
    name: String,
}

impl<W: Write + Send> QsiDisplay<W> {
    /// A display writing frames to `out` (the port, opened without
    /// blocking), named `name` in the log.
    pub fn new(out: W, link: Arc<Link>, name: &str) -> Self {
        Self {
            out,
            link,
            shown: None,
            seq: 0,
            name: name.to_string(),
        }
    }

    /// Write one frame whole; false when the port did not take it.
    fn send(&mut self, m: &FromSystem) -> bool {
        let Ok(bytes) = m.encode() else {
            return false;
        };
        match self.out.write(&bytes) {
            Ok(n) if n == bytes.len() => true,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                self.link.stalled.store(true, Ordering::SeqCst);
                false
            }
            _ => false,
        }
    }

    fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }
}

fn grid(cells: &dyn Cells) -> (u8, u8, Vec<wire::Cell>) {
    let rows = cells.rows().min(usize::from(wire::MAX_ROWS));
    let cols = cells.cols().min(usize::from(wire::MAX_COLS));
    let mut v = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let cell = cells.cell(r, c);
            v.push(wire::Cell {
                ch: cell.ch,
                attr: attr_bits(cell.attr),
            });
        }
    }
    let n = |x: usize| u8::try_from(x).unwrap_or(u8::MAX);
    (n(rows), n(cols), v)
}

fn status(cells: &dyn Cells, rows: u8, cols: u8, view: &View) -> Status {
    let (r, c) = cells.cursor();
    let n = |x: usize, max: u8| {
        u8::try_from(x)
            .unwrap_or(u8::MAX)
            .min(max.saturating_sub(1))
    };
    let mut ind = 0;
    for (on, bit) in [
        (view.inhibited, indicator::INHIBITED),
        (view.insert, indicator::INSERT),
        (view.session, indicator::SESSION),
    ] {
        if on {
            ind |= bit;
        }
    }
    Status {
        row: n(r, rows),
        col: n(c, cols),
        shape: Shape::Underline,
        indicators: ind,
        src: view.src.as_ref().map(|(code, text)| wire::SrcShown {
            code: code.clone(),
            text: text.clone(),
        }),
    }
}

impl<W: Write + Send> Display for QsiDisplay<W> {
    fn draw(&mut self, cells: &dyn Cells) -> io::Result<()> {
        self.draw_view(cells, &View::default())
    }

    fn draw_view(&mut self, cells: &dyn Cells, view: &View) -> io::Result<()> {
        if !self.link.is_connected() {
            self.shown = None;
            return Ok(());
        }
        let (rows, cols, now) = grid(cells);
        let status = status(cells, rows, cols, view);
        let whole = self.link.whole.swap(false, Ordering::SeqCst);
        if whole {
            self.shown = None;
            let hello = FromSystem::Hello(wire::Hello {
                version: qsipart::VERSION,
                caps: qsipart::caps::CONSOLE_V1,
            });
            if !self.send(&hello) {
                self.link.whole.store(true, Ordering::SeqCst);
                return Ok(());
            }
        }
        let update = match &self.shown {
            Some(s) if s.rows == rows && s.cols == cols => {
                let runs = wire::runs(&s.cells, &now, cols);
                if runs.is_empty() && s.status == status {
                    return Ok(());
                }
                Some(runs)
            }
            _ => None,
        };
        let seq = self.next_seq();
        let mut sent = false;
        if let Some(runs) = update {
            let m = FromSystem::Update(wire::Update {
                seq,
                runs,
                status: status.clone(),
            });
            // A change bigger than a frame goes as a whole one.
            if m.encode().is_ok() {
                sent = self.send(&m);
                if !sent {
                    self.shown = None;
                    return Ok(());
                }
            }
        }
        if !sent {
            let m = FromSystem::Frame(wire::Frame {
                seq,
                rows,
                cols,
                cells: now.clone(),
                status: status.clone(),
            });
            if !self.send(&m) {
                self.shown = None;
                return Ok(());
            }
        }
        self.shown = Some(Shown {
            rows,
            cols,
            cells: now,
            status,
        });
        Ok(())
    }

    fn describe(&self) -> String {
        format!("console port {}", self.name)
    }
}

/// The device node's name of the virtio port called `name`
/// (`vport1p2`), from sysfs: there is no udev to make a link.
#[must_use]
pub fn find_port(sys: &Path, name: &str) -> Option<String> {
    let dir = sys.join("class/virtio-ports");
    let mut ports: Vec<String> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ports.sort();
    ports
        .into_iter()
        .find(|p| std::fs::read_to_string(dir.join(p).join("name")).is_ok_and(|n| n.trim() == name))
}

#[cfg(target_os = "linux")]
pub use self::linux::{open, spawn_reader};

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::{File, OpenOptions};
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    use qsipart::console::ToSystem;
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    use super::Link;
    use crate::service::Command;

    /// `O_NONBLOCK`, `O_NOCTTY`.
    const FLAGS: i32 = 0o4000 | 0o400;

    /// The console port: one handle to write, one to read; `None` when
    /// the hypervisor gave no port of that name.
    #[must_use]
    pub fn open(sys: &Path, dev: &Path, name: &str) -> Option<(File, File, String)> {
        let node = super::find_port(sys, name)?;
        let path = dev.join(&node);
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FLAGS)
            .open(&path)
            .ok()?;
        let r = f.try_clone().ok()?;
        Some((f, r, path.display().to_string()))
    }

    fn peer_present(f: &File) -> bool {
        let mut fds = [PollFd::new(f, PollFlags::IN)];
        let zero = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        poll(&mut fds, Some(&zero)).is_ok_and(|_| !fds[0].revents().contains(PollFlags::HUP))
    }

    /// Read the port on a thread of its own: a peer attaching or asking
    /// gets a whole frame, its keys go to the console as typed.
    pub fn spawn_reader(port: File, link: Arc<Link>, console: Sender<Command>) {
        std::thread::spawn(move || {
            let mut port = port;
            let mut frames = qsipart::Reader::new();
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let present = peer_present(&port);
                if present != link.is_connected() {
                    if present {
                        frames.reset();
                        link.attached();
                    } else {
                        link.detached();
                    }
                    if console.send(Command::Redraw).is_err() {
                        return;
                    }
                }
                if !present {
                    // A port with no peer polls as readable.
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
                let mut fds = [PollFd::new(&port, PollFlags::IN)];
                let wait = Timespec {
                    tv_sec: 0,
                    tv_nsec: 500_000_000,
                };
                if poll(&mut fds, Some(&wait)).is_err() {
                    continue;
                }
                // A frame the port would not take: the whole screen again.
                if link.take_stalled() {
                    link.want_whole();
                    std::thread::sleep(Duration::from_millis(50));
                    let _ = console.send(Command::Redraw);
                }
                if fds[0].revents().is_empty() {
                    continue;
                }
                while let Ok(n) = port.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    frames.push(&buf[..n]);
                }
                while let Some(f) = frames.next_frame() {
                    let cmd = match ToSystem::decode(&f) {
                        Ok(Some(ToSystem::Key(k))) => super::key(k).map(Command::Virtual),
                        Ok(Some(ToSystem::Refresh)) => {
                            link.want_whole();
                            Some(Command::Redraw)
                        }
                        _ => None,
                    };
                    if let Some(c) = cmd
                        && console.send(c).is_err()
                    {
                        return;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Screen;
    use qsipart::console::Grid;

    #[derive(Clone, Default)]
    struct Port(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Port {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The peer's side: every message written so far, applied.
    fn peer(port: &Port, grid: &mut Grid) -> Vec<FromSystem> {
        let bytes = std::mem::take(&mut *port.0.lock().unwrap());
        let mut r = qsipart::Reader::new();
        r.push(&bytes);
        let msgs: Vec<FromSystem> = r
            .frames()
            .iter()
            .map(|f| FromSystem::decode(f).unwrap().unwrap())
            .collect();
        for m in &msgs {
            grid.apply(m).unwrap();
        }
        msgs
    }

    #[test]
    fn whole_frame_on_attach_then_changes_and_nothing_without_a_peer() {
        let port = Port::default();
        let link = Link::new();
        let mut d = QsiDisplay::new(port.clone(), Arc::clone(&link), "test");
        let mut s = Screen::new();
        s.put(1, 1, "Sign On", Attr::WHITE);
        // Nobody watching: nothing written.
        d.draw(&s).unwrap();
        assert!(port.0.lock().unwrap().is_empty());
        link.attached();
        let view = View {
            src: Some(("C6001090".into(), "IPL complete".into())),
            ..View::default()
        };
        d.draw_view(&s, &view).unwrap();
        let mut g = Grid::default();
        let msgs = peer(&port, &mut g);
        assert!(matches!(msgs[0], FromSystem::Hello(_)));
        assert!(matches!(msgs[1], FromSystem::Frame(_)));
        assert_eq!((g.rows, g.cols), (24, 80));
        assert_eq!(g.text(), s.dump());
        assert_eq!(g.status.src.as_ref().unwrap().code, "C6001090");
        assert_eq!(g.cells[0].attr, attr::WHITE | attr::HIGH);
        // A change goes as an update; the same screen again as nothing.
        s.put(6, 53, "QSECOFR", Attr::INPUT);
        s.cursor = (5, 60);
        d.draw(&s).unwrap();
        let msgs = peer(&port, &mut g);
        assert!(matches!(&msgs[..], [FromSystem::Update(u)] if u.runs.len() == 1));
        assert_eq!(g.text(), s.dump());
        assert_eq!((g.status.row, g.status.col), (5, 60));
        assert!(g.status.src.is_none());
        d.draw(&s).unwrap();
        assert!(peer(&port, &mut g).is_empty());
        // Away and back: a whole frame, after its hello.
        link.detached();
        s.put(2, 1, "while away", Attr::GREEN);
        d.draw(&s).unwrap();
        assert!(port.0.lock().unwrap().is_empty());
        link.attached();
        d.draw(&s).unwrap();
        let mut fresh = Grid::default();
        let msgs = peer(&port, &mut fresh);
        assert!(matches!(
            msgs[..],
            [FromSystem::Hello(_), FromSystem::Frame(_)]
        ));
        assert_eq!(fresh.text(), s.dump());
    }

    #[test]
    fn every_key_maps_as_the_keyboard_path_gives_it() {
        use qsipart::console::Key as W;
        assert_eq!(key(W::Char('q')), Some(Key::Char('q')));
        assert_eq!(key(W::Function(12)), Some(Key::Function(12)));
        assert_eq!(key(W::SysReq), Some(Key::SysReq));
        assert_eq!(key(W::Attention), Some(Key::Attention));
        assert_eq!(key(W::Reset), Some(Key::Reset));
        assert_eq!(key(W::Function(25)), None);
        assert_eq!(key(W::Char('\u{7}')), None);
        assert_eq!(key(W::Other(99)), None);
    }

    #[test]
    fn attributes_are_the_screens() {
        assert_eq!(attr_bits(Attr::GREEN), 0);
        assert_eq!(
            attr_bits(Attr::INPUT_HIDDEN),
            attr::UNDERLINE | attr::NONDISPLAY
        );
        assert_eq!(attr_bits(Attr::RED), attr::RED);
    }
}
