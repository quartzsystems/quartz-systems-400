// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Open descriptions the shell passes around: the [`Stream`] trait and
//! the in-process kinds every `Sys` can use (pipes, byte sources,
//! collecting buffers, the null device).

use std::any::Any;
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// An open description: what a descriptor number refers to. Shared by
/// every descriptor that duplicates it; closed when the last `Fd` is
/// dropped.
pub trait Stream: Send + Sync {
    /// Read up to `buf.len()` bytes; 0 at end of file.
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    /// Write some of `buf`.
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    /// Is this a terminal (for `test -t` and interactive mode)?
    fn is_terminal(&self) -> bool {
        false
    }
    /// End the writing half of a connection (`shutdown(SHUT_WR)`); the
    /// reading half stays open. Nothing for other streams.
    fn shutdown_write(&self) -> io::Result<()> {
        Ok(())
    }
    /// The concrete type, for a `Sys` that hands its own streams to
    /// child jobs.
    fn as_any(&self) -> &dyn Any;
}

/// A descriptor's open description.
pub type Fd = Arc<dyn Stream>;

/// Write all of `buf`.
pub fn write_all(s: &dyn Stream, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        match s.write(buf) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => buf = &buf[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Read everything to end of file.
pub fn read_to_end(s: &dyn Stream) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match s.read(&mut buf) {
            Ok(0) => return Ok(out),
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Bytes a pipe holds before writers wait.
const PIPE_CAPACITY: usize = 64 * 1024;

#[derive(Default)]
struct PipeState {
    data: VecDeque<u8>,
    readers: usize,
    writers: usize,
}

#[derive(Default)]
struct PipeShared {
    state: Mutex<PipeState>,
    cv: Condvar,
}

/// The read end of an in-process pipe.
pub struct PipeReader(Arc<PipeShared>);

/// The write end of an in-process pipe.
pub struct PipeWriter(Arc<PipeShared>);

/// A new in-process pipe: (read end, write end).
pub fn pipe() -> (Fd, Fd) {
    let shared = Arc::new(PipeShared::default());
    {
        let mut st = lock(&shared.state);
        st.readers = 1;
        st.writers = 1;
    }
    (
        Arc::new(PipeReader(Arc::clone(&shared))),
        Arc::new(PipeWriter(shared)),
    )
}

impl Stream for PipeReader {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut st = lock(&self.0.state);
        loop {
            if !st.data.is_empty() {
                let n = buf.len().min(st.data.len());
                for (i, b) in st.data.drain(..n).enumerate() {
                    buf[i] = b;
                }
                self.0.cv.notify_all();
                return Ok(n);
            }
            if st.writers == 0 {
                return Ok(0);
            }
            st = self
                .0
                .cv
                .wait(st)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn write(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(super::SysError::new(super::Errno::EBADF)))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        let mut st = lock(&self.0.state);
        st.readers -= 1;
        st.data.clear();
        self.0.cv.notify_all();
    }
}

impl Stream for PipeWriter {
    fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(super::SysError::new(super::Errno::EBADF)))
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut st = lock(&self.0.state);
        loop {
            if st.readers == 0 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            if st.data.len() < PIPE_CAPACITY {
                let n = buf.len().min(PIPE_CAPACITY - st.data.len());
                st.data.extend(&buf[..n]);
                self.0.cv.notify_all();
                return Ok(n);
            }
            st = self
                .0
                .cv
                .wait(st)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        let mut st = lock(&self.0.state);
        st.writers -= 1;
        self.0.cv.notify_all();
    }
}

/// A read-only stream over fixed bytes (here-documents, test input).
pub struct Bytes {
    data: Vec<u8>,
    pos: Mutex<usize>,
}

impl Bytes {
    /// A stream over `data`.
    pub fn fd(data: impl Into<Vec<u8>>) -> Fd {
        Arc::new(Self {
            data: data.into(),
            pos: Mutex::new(0),
        })
    }
}

impl Stream for Bytes {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut pos = lock(&self.pos);
        let rest = &self.data[(*pos).min(self.data.len())..];
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        *pos += n;
        Ok(n)
    }

    fn write(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(super::SysError::new(super::Errno::EBADF)))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A write-only stream that keeps everything written (command
/// substitution, captured output in tests).
#[derive(Clone, Default)]
pub struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    /// A new empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// A descriptor writing into this buffer.
    pub fn fd(&self) -> Fd {
        Arc::new(self.clone())
    }

    /// Everything written so far.
    pub fn contents(&self) -> Vec<u8> {
        lock(&self.0).clone()
    }

    /// Everything written so far as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.contents()).into_owned()
    }

    /// Take and clear the contents.
    pub fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *lock(&self.0))
    }
}

impl Stream for Buffer {
    fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        lock(&self.0).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One end of a two-way connection made of two pipes (a socket
/// stand-in): reads come from the other end's writes.
pub struct Duplex {
    rx: Fd,
    tx: Mutex<Option<Fd>>,
}

/// A connected pair of [`Duplex`] ends.
pub fn duplex() -> (Fd, Fd) {
    let (r1, w1) = pipe();
    let (r2, w2) = pipe();
    (
        Arc::new(Duplex {
            rx: r1,
            tx: Mutex::new(Some(w2)),
        }),
        Arc::new(Duplex {
            rx: r2,
            tx: Mutex::new(Some(w1)),
        }),
    )
}

impl Stream for Duplex {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.rx.read(buf)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let tx = lock(&self.tx).clone();
        match tx {
            Some(tx) => tx.write(buf),
            None => Err(io::ErrorKind::BrokenPipe.into()),
        }
    }

    fn shutdown_write(&self) -> io::Result<()> {
        lock(&self.tx).take();
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The null device: reads end at once, writes vanish.
pub struct Null;

impl Null {
    /// A descriptor on the null device.
    pub fn fd() -> Fd {
        Arc::new(Null)
    }
}

impl Stream for Null {
    fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_eof_and_broken() {
        let (r, w) = pipe();
        write_all(&*w, b"hi").unwrap();
        drop(w);
        assert_eq!(read_to_end(&*r).unwrap(), b"hi");
        let (r, w) = pipe();
        drop(r);
        assert_eq!(w.write(b"x").unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
