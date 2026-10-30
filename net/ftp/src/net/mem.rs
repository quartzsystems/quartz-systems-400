// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Connections in memory: the replies a session sent, what it wrote on
//! each data connection, and the bytes each upload delivers.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::rc::Rc;

use super::{DataConn, Net};
use crate::reply::Reply;

/// What one data connection carried out of the server.
type Sent = Rc<RefCell<Vec<Vec<u8>>>>;

/// The in-memory connections.
#[allow(clippy::struct_excessive_bools)] // a test double's switches
pub struct MemNet {
    /// Every reply sent, in order.
    pub replies: Vec<Reply>,
    /// What each data connection sent to the client, in order.
    pub sent: Sent,
    /// Payloads the next uploads deliver.
    pub uploads: VecDeque<Vec<u8>>,
    /// Whether a data connection is set up.
    pub ready: bool,
    /// Data connections fail to open.
    pub refuse_data: bool,
    /// TLS can be negotiated.
    pub tls: bool,
    /// The control connection is TLS.
    pub secure: bool,
    /// `PROT P`.
    pub private: bool,
    /// The client's address.
    pub peer: SocketAddr,
    /// The server's address.
    pub local: SocketAddr,
    /// The passive port handed out next.
    pub next_port: u16,
    /// Where active connections go.
    pub active_to: Option<SocketAddr>,
}

impl Default for MemNet {
    fn default() -> Self {
        Self {
            replies: Vec::new(),
            sent: Rc::new(RefCell::new(Vec::new())),
            uploads: VecDeque::new(),
            ready: false,
            refuse_data: false,
            tls: false,
            secure: false,
            private: false,
            peer: SocketAddr::from(([10, 0, 2, 2], 40_000)),
            local: SocketAddr::from(([10, 0, 2, 15], 21)),
            next_port: 50_000,
            active_to: None,
        }
    }
}

impl MemNet {
    /// The replies sent since the last call.
    pub fn take_replies(&mut self) -> Vec<Reply> {
        std::mem::take(&mut self.replies)
    }

    /// The data sent since the last call, one entry per connection.
    pub fn take_sent(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.sent.borrow_mut())
    }
}

struct MemData {
    input: io::Cursor<Vec<u8>>,
    output: Vec<u8>,
    sent: Sent,
    done: bool,
}

impl Read for MemData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for MemData {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl DataConn for MemData {
    fn finish(&mut self) -> io::Result<()> {
        if !self.done {
            self.done = true;
            self.sent.borrow_mut().push(std::mem::take(&mut self.output));
        }
        Ok(())
    }
}

impl Drop for MemData {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

impl Net for MemNet {
    fn send(&mut self, r: &Reply) -> io::Result<()> {
        self.replies.push(r.clone());
        Ok(())
    }

    fn passive(&mut self, _extended: bool) -> io::Result<SocketAddr> {
        self.ready = true;
        let p = self.next_port;
        self.next_port = self.next_port.wrapping_add(1).max(50_000);
        Ok(SocketAddr::new(self.local.ip(), p))
    }

    fn active(&mut self, to: SocketAddr) -> io::Result<()> {
        self.ready = true;
        self.active_to = Some(to);
        Ok(())
    }

    fn data(&mut self) -> io::Result<Box<dyn DataConn>> {
        if self.refuse_data {
            return Err(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"));
        }
        self.ready = false;
        let input = self.uploads.pop_front().unwrap_or_default();
        Ok(Box::new(MemData {
            input: io::Cursor::new(input),
            output: Vec::new(),
            sent: Rc::clone(&self.sent),
            done: false,
        }))
    }

    fn tls_available(&self) -> bool {
        self.tls
    }

    fn start_tls(&mut self) -> io::Result<()> {
        self.secure = true;
        Ok(())
    }

    fn end_tls(&mut self) -> io::Result<()> {
        self.secure = false;
        Ok(())
    }

    fn secure(&self) -> bool {
        self.secure
    }

    fn set_private(&mut self, private: bool) {
        self.private = private;
    }

    fn aborted(&mut self) -> bool {
        false
    }

    fn peer(&self) -> SocketAddr {
        self.peer
    }

    fn local(&self) -> SocketAddr {
        self.local
    }
}
