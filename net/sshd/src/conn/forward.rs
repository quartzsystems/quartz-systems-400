// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP forwarding: off unless `AllowTcpForwarding` allows it; remote
//! forwards (`tcpip-forward`) also pass the `ADDTCPPORT` restriction for
//! the user. [`Forwarder`] makes the connections and listeners;
//! [`TcpForwarder`] is the one over the kernel's sockets.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// One forwarded connection's two halves and the far address.
pub struct Duplex {
    /// Bytes from the far end.
    pub read: Box<dyn Read + Send>,
    /// Bytes to the far end.
    pub write: Box<dyn Write + Send>,
    /// The far end's address.
    pub peer: (String, u16),
}

/// A listener for a remote forward.
pub trait ForwardListener: Send {
    /// The port bound (the one chosen, for port 0).
    fn port(&self) -> u16;
    /// The next connection; `None` once `stop` is set or on failure.
    fn accept(&self, stop: &AtomicBool) -> Option<Duplex>;
}

/// Makes forwarded connections.
pub trait Forwarder: Send + Sync {
    /// `direct-tcpip`: connect to `host:port`.
    fn connect(&self, host: &str, port: u16) -> io::Result<Duplex>;
    /// `tcpip-forward`: listen on `host:port`.
    fn listen(&self, host: &str, port: u16) -> io::Result<Box<dyn ForwardListener>>;
}

/// Forwarding over the kernel's TCP.
#[derive(Debug, Default, Clone, Copy)]
pub struct TcpForwarder;

fn duplex(s: TcpStream) -> io::Result<Duplex> {
    let peer = s.peer_addr()?;
    let r = s.try_clone()?;
    Ok(Duplex {
        read: Box::new(r),
        write: Box::new(HalfClose(s)),
        peer: (peer.ip().to_string(), peer.port()),
    })
}

/// A TCP write half that sends FIN when dropped.
struct HalfClose(TcpStream);

impl Write for HalfClose {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Drop for HalfClose {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Write);
    }
}

struct TcpForwardListener(TcpListener);

impl ForwardListener for TcpForwardListener {
    fn port(&self) -> u16 {
        self.0.local_addr().map_or(0, |a| a.port())
    }

    fn accept(&self, stop: &AtomicBool) -> Option<Duplex> {
        while !stop.load(Ordering::SeqCst) {
            match self.0.accept() {
                Ok((s, _)) => {
                    let _ = s.set_nonblocking(false);
                    return duplex(s).ok();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Forwarder for TcpForwarder {
    fn connect(&self, host: &str, port: u16) -> io::Result<Duplex> {
        let addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        duplex(TcpStream::connect_timeout(&addr, Duration::from_secs(10))?)
    }

    fn listen(&self, host: &str, port: u16) -> io::Result<Box<dyn ForwardListener>> {
        let host = match host {
            "" | "*" | "0.0.0.0" => "0.0.0.0",
            "localhost" => "127.0.0.1",
            h => h,
        };
        let l = TcpListener::bind((host, port))?;
        l.set_nonblocking(true)?;
        Ok(Box::new(TcpForwardListener(l)))
    }
}
