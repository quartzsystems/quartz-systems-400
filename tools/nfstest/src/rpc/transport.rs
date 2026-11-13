// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection: TCP with record marking (RFC 5531 section 11), or a
//! UDP socket where a datagram is one message.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// The transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    /// TCP, record marked.
    Tcp,
    /// UDP.
    Udp,
}

impl Proto {
    /// The IP protocol number portmap uses.
    pub fn number(self) -> u32 {
        match self {
            Proto::Tcp => 6,
            Proto::Udp => 17,
        }
    }

    /// `tcp` or `udp`.
    pub fn name(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

/// The largest record this client takes (the server sends at most 1 MiB of data).
pub const MAX_REPLY: usize = 4 << 20;

/// A socket to one server address.
#[derive(Debug)]
pub struct Conn {
    /// Where.
    pub addr: SocketAddr,
    /// How.
    pub proto: Proto,
    /// The wait for one reply.
    pub timeout: Duration,
    tcp: Option<TcpStream>,
    udp: Option<UdpSocket>,
}

impl Conn {
    /// A connection, opened on first use.
    pub fn new(addr: SocketAddr, proto: Proto, timeout: Duration) -> Conn {
        Conn {
            addr,
            proto,
            timeout,
            tcp: None,
            udp: None,
        }
    }

    fn tcp(&mut self) -> io::Result<&mut TcpStream> {
        if self.tcp.is_none() {
            let s = TcpStream::connect_timeout(&self.addr, self.timeout)?;
            s.set_read_timeout(Some(self.timeout))?;
            s.set_write_timeout(Some(self.timeout))?;
            s.set_nodelay(true)?;
            self.tcp = Some(s);
        }
        Ok(self.tcp.as_mut().expect("just set"))
    }

    fn udp(&mut self) -> io::Result<&UdpSocket> {
        if self.udp.is_none() {
            let local: SocketAddr = if self.addr.is_ipv4() {
                "0.0.0.0:0".parse().expect("literal")
            } else {
                "[::]:0".parse().expect("literal")
            };
            let s = UdpSocket::bind(local)?;
            s.connect(self.addr)?;
            s.set_read_timeout(Some(self.timeout))?;
            self.udp = Some(s);
        }
        Ok(self.udp.as_ref().expect("just set"))
    }

    /// Drops the TCP connection (the next send opens a new one).
    pub fn reset(&mut self) {
        self.tcp = None;
    }

    /// Sends one message (one record over TCP, one datagram over UDP).
    pub fn send(&mut self, msg: &[u8]) -> io::Result<()> {
        match self.proto {
            Proto::Tcp => {
                let len = u32::try_from(msg.len())
                    .ok()
                    .filter(|l| *l < 0x8000_0000)
                    .ok_or_else(|| io::Error::other("record too long"))?;
                let mut rec = Vec::with_capacity(msg.len() + 4);
                rec.extend_from_slice(&(len | 0x8000_0000).to_be_bytes());
                rec.extend_from_slice(msg);
                let r = self.tcp()?.write_all(&rec);
                if r.is_err() {
                    self.reset();
                }
                r
            }
            Proto::Udp => self.udp()?.send(msg).map(|_| ()),
        }
    }

    /// Sends raw bytes on the TCP stream, as they are (no record mark).
    pub fn send_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        let r = self.tcp()?.write_all(bytes);
        if r.is_err() {
            self.reset();
        }
        r
    }

    /// Waits for the next message.
    pub fn recv(&mut self) -> io::Result<Vec<u8>> {
        match self.proto {
            Proto::Tcp => {
                let r = read_record(self.tcp()?);
                if r.is_err() {
                    self.reset();
                }
                r
            }
            Proto::Udp => {
                let mut buf = vec![0u8; 65_536];
                let n = self.udp()?.recv(&mut buf)?;
                buf.truncate(n);
                Ok(buf)
            }
        }
    }
}

/// Reads one record: fragments up to the last one.
pub fn read_record(s: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut h = [0u8; 4];
        s.read_exact(&mut h)?;
        let h = u32::from_be_bytes(h);
        let len = (h & 0x7FFF_FFFF) as usize;
        if out.len() + len > MAX_REPLY {
            return Err(io::Error::other("reply record too long"));
        }
        let at = out.len();
        out.resize(at + len, 0);
        s.read_exact(&mut out[at..])?;
        if h & 0x8000_0000 != 0 {
            return Ok(out);
        }
    }
}
