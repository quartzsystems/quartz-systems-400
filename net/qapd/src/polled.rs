// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A TCP stream for [`crate::serve`] without a read timeout. On Windows
//! a read that times out (`SO_RCVTIMEO`) can fail with
//! `ERROR_IO_PENDING` (os error 997) instead of `WSAETIMEDOUT`, and
//! leaves the socket in a state Windows calls indeterminate: the
//! connection ended under its requests. A non-blocking read cannot.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;
use std::time::Duration;

use crate::Transport;

/// How long a read waits when nothing has come.
const WAIT: Duration = Duration::from_millis(2);

/// A non-blocking TCP stream: a read waits a moment for data, a write
/// waits for room.
pub struct Polled(TcpStream);

impl Polled {
    /// Over `s`, made non-blocking.
    ///
    /// # Errors
    /// When the socket's mode cannot be set.
    pub fn new(s: TcpStream) -> io::Result<Self> {
        s.set_read_timeout(None)?;
        s.set_nonblocking(true)?;
        Ok(Self(s))
    }
}

impl Read for Polled {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(WAIT);
                self.0.read(buf)
            }
            r => r,
        }
    }
}

impl Write for Polled {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            match self.0.write(buf) {
                Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(WAIT),
                r => return r,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Transport for Polled {
    fn close(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}
