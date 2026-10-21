// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A blocking channel of frames over a Unix stream socket.
//!
//! A parent hands a child its end of a [`Chan::pair`] as the child's
//! standard input; the child takes it back with [`Chan::from_stdin`].
//! No raw descriptor numbers cross `exec`, so nothing here is unsafe.

use std::os::fd::{AsFd, AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::frame::{read_frame, write_frame};
use crate::{Wire, WireError, decode, encode};

/// One end of a connection.
pub struct Chan {
    stream: UnixStream,
}

impl Chan {
    /// Over a connected stream.
    #[must_use]
    pub fn new(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Connect to a listening socket.
    pub fn connect(path: &Path) -> Result<Self, WireError> {
        Ok(Self::new(UnixStream::connect(path)?))
    }

    /// A connected pair.
    pub fn pair() -> Result<(Self, Self), WireError> {
        let (a, b) = UnixStream::pair()?;
        Ok((Self::new(a), Self::new(b)))
    }

    /// Over a descriptor.
    #[must_use]
    pub fn from_fd(fd: OwnedFd) -> Self {
        Self::new(UnixStream::from(fd))
    }

    /// The socket a parent passed as this process's standard input.
    pub fn from_stdin() -> Result<Self, WireError> {
        let fd = std::io::stdin().as_fd().try_clone_to_owned()?;
        Ok(Self::from_fd(fd))
    }

    /// Send one value.
    pub fn send<T: Wire>(&mut self, v: &T) -> Result<(), WireError> {
        write_frame(&mut self.stream, &encode(v))
    }

    /// Send already encoded bytes as one frame.
    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        write_frame(&mut self.stream, bytes)
    }

    /// Receive one value.
    pub fn recv<T: Wire>(&mut self) -> Result<T, WireError> {
        decode(&read_frame(&mut self.stream)?)
    }

    /// Receive one frame's bytes.
    pub fn recv_raw(&mut self) -> Result<Vec<u8>, WireError> {
        read_frame(&mut self.stream)
    }

    /// Bound how long a receive waits; `None` waits for ever.
    pub fn set_timeout(&self, t: Option<Duration>) -> Result<(), WireError> {
        self.stream.set_read_timeout(t)?;
        Ok(())
    }

    /// A second handle on the same socket, for a reader thread.
    pub fn try_clone(&self) -> Result<Self, WireError> {
        Ok(Self::new(self.stream.try_clone()?))
    }

    /// The socket.
    #[must_use]
    pub fn stream(&self) -> &UnixStream {
        &self.stream
    }

    /// Give up the socket.
    #[must_use]
    pub fn into_fd(self) -> OwnedFd {
        self.stream.into()
    }

    /// The descriptor number, for `poll`.
    #[must_use]
    pub fn raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair() {
        let (mut a, mut b) = Chan::pair().unwrap();
        a.send(&(1u32, "x".to_string())).unwrap();
        assert_eq!(b.recv::<(u32, String)>().unwrap(), (1, "x".into()));
        drop(a);
        assert!(matches!(b.recv::<u32>(), Err(WireError::Closed)));
    }
}
