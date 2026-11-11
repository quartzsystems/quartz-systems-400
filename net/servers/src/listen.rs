// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The listener a server job runs: bind (waiting out a port a server
//! that just ended still holds), accept on a thread until stopped, a
//! TLS handshake when the port is secure, then one thread per
//! connection running the server's [`Service`].

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

/// How long a TLS handshake may take.
const HANDSHAKE: Duration = Duration::from_secs(10);
/// How long the accept loop waits after an accept that failed.
const RETRY: Duration = Duration::from_millis(50);
/// How long `stop` waits for its wake-up connection.
const WAKE: Duration = Duration::from_secs(2);

/// One accepted connection, plain or TLS.
pub enum Conn {
    /// Plain TCP.
    Plain(TcpStream),
    /// TLS over TCP, handshake done.
    Tls(Box<TlsStream<TcpStream>>),
}

impl Conn {
    /// The socket under it.
    pub fn tcp(&self) -> &TcpStream {
        match self {
            Conn::Plain(s) => s,
            Conn::Tls(t) => t.get_ref(),
        }
    }

    /// Whether it is TLS.
    pub fn secure(&self) -> bool {
        matches!(self, Conn::Tls(_))
    }

    /// Send TLS's `close_notify` (when TLS) and shut the socket.
    pub fn close(&mut self) {
        if let Conn::Tls(t) = self {
            let _ = t.close();
        }
        let _ = self.tcp().shutdown(std::net::Shutdown::Both);
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(t) => t.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(t) => t.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(t) => t.flush(),
        }
    }
}

/// What a server does with a connection; called on the connection's
/// own thread.
pub trait Service: Send + Sync + 'static {
    /// Serve `conn` from `peer` until done.
    fn serve(&self, conn: Conn, peer: SocketAddr);
}

/// Bind `port` on every IPv4 address, trying for a few seconds while a
/// server that just ended still holds it.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    bind_at(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)), 20)
}

/// Bind `addr`, `tries` times half a second apart.
pub fn bind_at(addr: SocketAddr, tries: u32) -> io::Result<TcpListener> {
    let mut last = None;
    for i in 0..tries.max(1) {
        match TcpListener::bind(addr) {
            Ok(l) => return Ok(l),
            Err(e) => {
                last = Some(e);
                if i + 1 < tries {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    Err(last.expect("tried"))
}

/// A running accept loop.
pub struct Running {
    stop: Arc<AtomicBool>,
    addr: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    /// The address it listens on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop accepting; connections being served run on.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            // The loop blocks in accept: a connection of our own wakes it.
            let mut to = self.addr;
            if to.ip().is_unspecified() {
                to.set_ip(match to {
                    SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
                    SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
                });
            }
            if TcpStream::connect_timeout(&to, WAKE).is_ok() {
                let _ = t.join();
            }
            // Not woken (the port is filtered from itself): the thread
            // ends at the next connection, which it drops.
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.halt();
    }
}

/// Accept on `listener` until stopped, TLS first when `tls` is given.
pub fn serve(
    listener: TcpListener,
    tls: Option<Arc<ServerConfig>>,
    service: Arc<dyn Service>,
) -> io::Result<Running> {
    // Blocking: a connection is taken the moment it arrives.
    listener.set_nonblocking(false)?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((s, peer)) => {
                    if flag.load(Ordering::Relaxed) {
                        // Stopped: this is the wake-up, or a client too late.
                        break;
                    }
                    let tls = tls.clone();
                    let service = Arc::clone(&service);
                    std::thread::spawn(move || {
                        if let Some(c) = open(s, tls, peer) {
                            service.serve(c, peer);
                        }
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    tracing::warn!("accept on {addr}: {e}");
                    std::thread::sleep(RETRY);
                }
            }
        }
    });
    Ok(Running {
        stop,
        addr,
        thread: Some(thread),
    })
}

/// The connection ready to serve: blocking, the handshake done.
fn open(s: TcpStream, tls: Option<Arc<ServerConfig>>, peer: SocketAddr) -> Option<Conn> {
    s.set_nonblocking(false).ok()?;
    let _ = s.set_nodelay(true);
    match tls {
        None => Some(Conn::Plain(s)),
        Some(cfg) => {
            s.set_read_timeout(Some(HANDSHAKE)).ok()?;
            match TlsStream::accept(cfg, s) {
                Ok(t) => {
                    let _ = t.get_ref().set_read_timeout(None);
                    Some(Conn::Tls(Box::new(t)))
                }
                Err(e) => {
                    tracing::info!("TLS handshake with {peer}: {e}");
                    None
                }
            }
        }
    }
}
