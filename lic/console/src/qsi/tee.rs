// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console for one viewer beside its own displays. [`Hub`] holds
//! the viewer; [`Tee`], one more display of the console, draws the grid
//! to it as the console port's frames ([`QsiDisplay`]). A viewer reaches
//! the hub over a local stream ([`Conn`]; the socket's path is the
//! caller's, [`listen`]) and asks first, in a frame of `qsipart`'s:
//!
//! - [`kind::ATTACH`] (one byte, 1 to take over): answered
//!   [`kind::ACCEPTED`] and then the console port's stream both ways
//!   (hello, a whole frame, updates; keys and refreshes back), or
//!   [`kind::REFUSED`] (one byte, [`why`]). A viewer taken over gets
//!   [`kind::ENDED`] (its reason as text) and the stream closes.
//! - [`kind::SCREEN`]: answered with one whole frame, then closed.
//!
//! With no viewer nothing is sent or kept. A viewer too slow to take
//! its frames never holds the console up: a frame its queue has no room
//! for is dropped and the next one is whole.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use qsipart::console::{self as wire, FromSystem, ToSystem};
use qsipart::frame::{self, Frame as RawFrame, Reader};

use super::{Link, QsiDisplay, grid, status};
use crate::display::{Display, View};
use crate::keyboard::map::Key;
use crate::scale::Cells;

/// The stream's own frame kinds, beside the console port's.
pub mod kind {
    /// A viewer asks to attach; the body is one byte, 1 to take over.
    pub const ATTACH: u16 = 0x0501;
    /// A viewer asks for the screen alone.
    pub const SCREEN: u16 = 0x0502;
    /// The attach is accepted: the console port's stream follows.
    pub const ACCEPTED: u16 = 0x0511;
    /// The attach is refused; the body is one byte ([`super::why`]).
    pub const REFUSED: u16 = 0x0512;
    /// The viewer is detached; the body is the reason as text.
    pub const ENDED: u16 = 0x0513;
}

/// Why an attach was refused.
pub mod why {
    /// Another viewer has the console.
    pub const VIEWED: u8 = 1;
}

/// What a viewer taken over is told.
pub const TAKEN_OVER: &str = "Another viewer took the console over.";

/// Frames queued for a viewer before one is dropped.
const QUEUE: usize = 8;
/// How long a viewer has to ask, and an asker to be answered.
const ASK_WAIT: Duration = Duration::from_secs(10);
/// How often a viewer's reader looks up from its stream.
const POLL: Duration = Duration::from_millis(250);
/// How long a viewer that is leaving has to take what is queued for it.
const WRITE_WAIT: Duration = Duration::from_secs(5);
/// How long a screen waits for the console's next draw.
const SCREEN_WAIT: Duration = Duration::from_secs(2);

/// What the hub asks of the console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A key from the viewer, as the keyboard path gives it.
    Key(Key),
    /// Draw again (a viewer attached, asked, or lost a frame).
    Redraw,
}

/// How the hub reaches the console (the caller's command channel).
pub type Poke = Arc<dyn Fn(Ask) + Send + Sync>;

/// A local stream to or from a viewer.
pub trait Conn: Read + Write + Send + Sized + 'static {
    /// A second handle to the same stream.
    ///
    /// # Errors
    /// The system would not make one.
    fn try_clone(&self) -> io::Result<Self>;
    /// How long a read waits.
    ///
    /// # Errors
    /// The stream refused it.
    fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()>;
    /// How long a write waits.
    ///
    /// # Errors
    /// The stream refused it.
    fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()>;
    /// Close both ways.
    fn close(&self);
}

impl Conn for TcpStream {
    fn try_clone(&self) -> io::Result<Self> {
        TcpStream::try_clone(self)
    }
    fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, d)
    }
    fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        TcpStream::set_write_timeout(self, d)
    }
    fn close(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

#[cfg(unix)]
impl Conn for std::os::unix::net::UnixStream {
    fn try_clone(&self) -> io::Result<Self> {
        Self::try_clone(self)
    }
    fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        Self::set_read_timeout(self, d)
    }
    fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        Self::set_write_timeout(self, d)
    }
    fn close(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A read that waited its time out.
pub(crate) fn timed_out(e: &io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// The next frame from `conn` by `deadline`; `None` when none came.
///
/// # Errors
/// The stream ended or failed.
pub(crate) fn read_frame<C: Conn>(
    conn: &mut C,
    frames: &mut Reader,
    deadline: Instant,
) -> io::Result<Option<RawFrame>> {
    let mut buf = [0u8; 4096];
    loop {
        if let Some(f) = frames.next_frame() {
            return Ok(Some(f));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        conn.set_read_timeout(Some(left.max(Duration::from_millis(1))))?;
        match conn.read(&mut buf) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => frames.push(&buf[..n]),
            Err(e) if timed_out(&e) => {}
            Err(e) => return Err(e),
        }
    }
}

/// The viewer that has the console.
struct Current {
    id: u64,
    /// Its frames, to its writer.
    tx: SyncSender<Vec<u8>>,
    /// Why it was ended, for its writer to say.
    end: Arc<Mutex<Option<String>>>,
}

/// The one viewer and what the console's draws owe it.
pub struct Hub {
    link: Arc<Link>,
    current: Mutex<Option<Current>>,
    next: AtomicU64,
    /// Screens asked for, answered at the next draw.
    snaps: Mutex<Vec<Sender<wire::Frame>>>,
}

impl Hub {
    /// A hub with no viewer.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            link: Link::new(),
            current: Mutex::new(None),
            next: AtomicU64::new(1),
            snaps: Mutex::new(Vec::new()),
        })
    }

    /// The display to give the console, named `name` in its log.
    #[must_use]
    pub fn display(self: &Arc<Self>, name: &str) -> Tee {
        Tee {
            inner: QsiDisplay::new(Sink(Arc::clone(self)), Arc::clone(&self.link), name),
            hub: Arc::clone(self),
            name: name.to_string(),
        }
    }

    /// Whether a viewer has the console.
    #[must_use]
    pub fn viewed(&self) -> bool {
        lock(&self.current).is_some()
    }

    fn is_current(&self, id: u64) -> bool {
        lock(&self.current).as_ref().is_some_and(|c| c.id == id)
    }

    /// Serve one stream until it ends, on the caller's thread.
    pub fn serve<C: Conn>(self: &Arc<Self>, mut conn: C, poke: &Poke) {
        let mut frames = Reader::new();
        let deadline = Instant::now() + ASK_WAIT;
        let Ok(Some(ask)) = read_frame(&mut conn, &mut frames, deadline) else {
            return conn.close();
        };
        match ask.kind {
            kind::SCREEN => self.screen(conn, poke),
            kind::ATTACH => self.attach(conn, frames, ask.body.first() == Some(&1), poke),
            _ => conn.close(),
        }
    }

    fn screen<C: Conn>(&self, mut conn: C, poke: &Poke) {
        let (tx, rx) = mpsc::channel();
        lock(&self.snaps).push(tx);
        poke(Ask::Redraw);
        if let Ok(f) = rx.recv_timeout(SCREEN_WAIT)
            && let Ok(bytes) = FromSystem::Frame(f).encode()
        {
            let _ = conn.set_write_timeout(Some(WRITE_WAIT));
            let _ = conn.write_all(&bytes);
        }
        conn.close();
    }

    fn attach<C: Conn>(self: &Arc<Self>, mut conn: C, frames: Reader, take_over: bool, poke: &Poke) {
        let Ok(writer) = conn.try_clone() else {
            return conn.close();
        };
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let end = Arc::new(Mutex::new(None));
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        {
            let mut cur = lock(&self.current);
            if cur.is_some() && !take_over {
                drop(cur);
                if let Ok(b) = frame::encode(kind::REFUSED, &[why::VIEWED]) {
                    let _ = conn.set_write_timeout(Some(WRITE_WAIT));
                    let _ = conn.write_all(&b);
                }
                return conn.close();
            }
            if let Ok(b) = frame::encode(kind::ACCEPTED, &[]) {
                let _ = tx.try_send(b);
            }
            // The one displaced is told why once its queue is out.
            if let Some(old) = cur.take() {
                *lock(&old.end) = Some(TAKEN_OVER.to_string());
            }
            *cur = Some(Current {
                id,
                tx,
                end: Arc::clone(&end),
            });
            self.link.attached();
        }
        let (done_tx, done) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            write_out(writer, &rx, &end);
            drop(done_tx);
        });
        poke(Ask::Redraw);
        self.read_keys(&mut conn, frames, id, poke);
        let mut cur = lock(&self.current);
        if cur.as_ref().is_some_and(|c| c.id == id) {
            *cur = None;
            self.link.detached();
        }
        drop(cur);
        // The writer has its queue and the reason to say first; one the
        // viewer will not take is cut short.
        let _ = done.recv_timeout(WRITE_WAIT);
        conn.close();
    }

    /// The viewer's keys to the console, while it has it.
    fn read_keys<C: Conn>(&self, conn: &mut C, mut frames: Reader, id: u64, poke: &Poke) {
        let mut buf = vec![0u8; 4096];
        let _ = conn.set_read_timeout(Some(POLL));
        loop {
            if !self.is_current(id) {
                return;
            }
            // A frame it had no room for: the whole screen again.
            if self.link.take_stalled() {
                self.link.want_whole();
                poke(Ask::Redraw);
            }
            match conn.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => frames.push(&buf[..n]),
                Err(e) if timed_out(&e) => continue,
                Err(_) => return,
            }
            while let Some(f) = frames.next_frame() {
                if !self.is_current(id) {
                    return;
                }
                match ToSystem::decode(&f) {
                    Ok(Some(ToSystem::Key(k))) => {
                        if let Some(k) = super::key(k) {
                            poke(Ask::Key(k));
                        }
                    }
                    Ok(Some(ToSystem::Refresh)) => {
                        self.link.want_whole();
                        poke(Ask::Redraw);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// A viewer's writer: its queue to its stream, then why it ended. It
/// may wait on a slow viewer as long as it likes: the console does not.
fn write_out<C: Conn>(mut conn: C, rx: &Receiver<Vec<u8>>, end: &Mutex<Option<String>>) {
    while let Ok(bytes) = rx.recv() {
        if conn.write_all(&bytes).is_err() {
            return conn.close();
        }
    }
    if let Some(why) = lock(end).take()
        && let Ok(b) = frame::encode(kind::ENDED, why.as_bytes())
    {
        let _ = conn.write_all(&b);
    }
    conn.close();
}

/// The display's port: the current viewer's queue, never waited on.
struct Sink(Arc<Hub>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let cur = lock(&self.0.current);
        let Some(c) = cur.as_ref() else {
            return Err(ErrorKind::NotConnected.into());
        };
        match c.tx.try_send(buf.to_vec()) {
            Ok(()) => Ok(buf.len()),
            Err(TrySendError::Full(_)) => Err(ErrorKind::WouldBlock.into()),
            Err(TrySendError::Disconnected(_)) => Err(ErrorKind::BrokenPipe.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The console's display for its viewer.
pub struct Tee {
    inner: QsiDisplay<Sink>,
    hub: Arc<Hub>,
    name: String,
}

impl Display for Tee {
    fn draw(&mut self, cells: &dyn Cells) -> io::Result<()> {
        self.draw_view(cells, &View::default())
    }

    fn draw_view(&mut self, cells: &dyn Cells, view: &View) -> io::Result<()> {
        let waiting = std::mem::take(&mut *lock(&self.hub.snaps));
        if !waiting.is_empty() {
            let (rows, cols, now) = grid(cells);
            let status = status(cells, rows, cols, view);
            let f = wire::Frame {
                seq: 0,
                rows,
                cols,
                cells: now,
                status,
            };
            for w in waiting {
                let _ = w.send(f.clone());
            }
        }
        self.inner.draw_view(cells, view)
    }

    fn describe(&self) -> String {
        format!("console viewer at {}", self.name)
    }
}

#[cfg(unix)]
pub use self::unix::listen;

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::sync::Arc;

    use super::{Hub, Poke};

    /// Serve the hub at `path` (a stale socket removed, the owner's
    /// alone), a thread a connection.
    ///
    /// # Errors
    /// The socket could not be made.
    pub fn listen(hub: &Arc<Hub>, path: &Path, poke: Poke) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let hub = Arc::clone(hub);
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                };
                let hub = Arc::clone(&hub);
                let poke = Arc::clone(&poke);
                std::thread::spawn(move || hub.serve(conn, &poke));
            }
        });
        Ok(())
    }
}
