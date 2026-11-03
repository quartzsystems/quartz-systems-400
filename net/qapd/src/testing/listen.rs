// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's listener, as `QAPD` listens: one port on loopback,
//! a connection job for each connection. While the server is ended
//! ([`System::end`] with [`Ending::Server`]) the port is closed and
//! connects are refused; [`System::restart`] opens it again on the same
//! port, as `STRTCPSVR` does.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(doc)]
use crate::host::Ending;
use crate::{Closed, Config};

use super::{System, lock, next_job, serve_socket};

/// How often the accept loop looks for a connection.
const POLL: Duration = Duration::from_millis(5);

/// What the system and the accept loop share.
pub(super) struct Port {
    addr: SocketAddr,
    /// The socket; `None` while the server is ended.
    sock: Mutex<Option<TcpListener>>,
    stop: AtomicBool,
}

impl Port {
    /// Close the port: connects are refused from now on.
    pub(super) fn close(&self) {
        lock(&self.sock).take();
    }

    /// Open the port again at its address.
    pub(super) fn open(&self) -> io::Result<()> {
        let mut s = lock(&self.sock);
        if s.is_none() {
            let l = TcpListener::bind(self.addr)?;
            l.set_nonblocking(true)?;
            *s = Some(l);
        }
        Ok(())
    }
}

/// A listener of the test system on loopback. Dropping it closes the
/// port; connections already served run on.
pub struct Listener {
    port: Arc<Port>,
    closes: Mutex<Receiver<(u32, Closed)>>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Listen for `system` on a free loopback port, closed at once when
    /// the server is ended.
    pub(super) fn start(system: &Arc<System>, cfg: Config) -> io::Result<Self> {
        let l = TcpListener::bind("127.0.0.1:0")?;
        l.set_nonblocking(true)?;
        let port = Arc::new(Port {
            addr: l.local_addr()?,
            sock: Mutex::new(Some(l)),
            stop: AtomicBool::new(false),
        });
        lock(&system.ports).push(Arc::downgrade(&port));
        if system.server_ending.load(Ordering::SeqCst) {
            port.close();
        }
        let (tx, rx) = mpsc::channel();
        let (p, sys) = (Arc::clone(&port), Arc::clone(system));
        let thread = thread::spawn(move || accept(&p, &sys, cfg, &tx));
        Ok(Self {
            port,
            closes: Mutex::new(rx),
            thread: Some(thread),
        })
    }

    /// The address clients connect to.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.port.addr
    }

    /// Whether the port takes connections now.
    #[must_use]
    pub fn accepting(&self) -> bool {
        lock(&self.port.sock).is_some()
    }

    /// Connect a client.
    ///
    /// # Errors
    /// Refused while the server is ended.
    pub fn connect(&self) -> io::Result<TcpStream> {
        TcpStream::connect(self.port.addr)
    }

    /// The next connection to close, its job and why, waiting at most
    /// `timeout`.
    pub fn next_close(&self, timeout: Duration) -> Option<(u32, Closed)> {
        lock(&self.closes).recv_timeout(timeout).ok()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.port.stop.store(true, Ordering::SeqCst);
        self.port.close();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Accept until stopped; each connection gets a job of its own.
fn accept(port: &Port, sys: &Arc<System>, cfg: Config, closes: &Sender<(u32, Closed)>) {
    while !port.stop.load(Ordering::SeqCst) {
        let got = lock(&port.sock).as_ref().map(TcpListener::accept);
        match got {
            Some(Ok((s, peer))) => {
                let (sys, tx) = (Arc::clone(sys), closes.clone());
                thread::spawn(move || {
                    let job = next_job();
                    let why = match s.set_nonblocking(false) {
                        Ok(()) => serve_socket(cfg, &sys, job, s, peer),
                        Err(e) => Closed::Broken(format!("blocking: {e}")),
                    };
                    let _ = tx.send((job, why));
                });
            }
            Some(Err(e)) if e.kind() != io::ErrorKind::WouldBlock => {
                tracing::warn!("test listener on {}: {e}", port.addr);
                thread::sleep(POLL);
            }
            _ => thread::sleep(POLL),
        }
    }
}
