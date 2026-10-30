// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An in-memory pipe: the channel's standard input, and a byte stream
//! pair for connecting a client and server in one process. The reader
//! can report what it consumed (the channel's window).

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};

type Notify = Box<dyn Fn(usize) + Send + Sync>;

#[derive(Default)]
struct State {
    buf: VecDeque<u8>,
    writer_closed: bool,
    reader_closed: bool,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

/// The reading end.
pub struct PipeReader {
    shared: Arc<Shared>,
    notify: Option<Notify>,
}

/// The writing end; dropping it is end of file for the reader.
pub struct PipeWriter {
    shared: Arc<Shared>,
}

/// A new pipe; `notify` is told how many bytes each read consumed.
#[must_use]
pub fn pipe(notify: Option<Notify>) -> (PipeWriter, PipeReader) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        cv: Condvar::new(),
    });
    (
        PipeWriter {
            shared: shared.clone(),
        },
        PipeReader { shared, notify },
    )
}

/// Two connected byte streams: each side's (reader, writer).
#[must_use]
pub fn duplex() -> ((PipeReader, PipeWriter), (PipeReader, PipeWriter)) {
    let (aw, br) = pipe(None);
    let (bw, ar) = pipe(None);
    ((ar, aw), (br, bw))
}

impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut st = self
            .shared
            .state
            .lock()
            .map_err(|_| io::Error::other("pipe poisoned"))?;
        while st.buf.is_empty() && !st.writer_closed {
            st = self
                .shared
                .cv
                .wait(st)
                .map_err(|_| io::Error::other("pipe poisoned"))?;
        }
        let n = out.len().min(st.buf.len());
        for (o, b) in out.iter_mut().zip(st.buf.drain(..n)) {
            *o = b;
        }
        drop(st);
        self.shared.cv.notify_all();
        if n > 0
            && let Some(f) = &self.notify
        {
            f(n);
        }
        Ok(n)
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        if let Ok(mut st) = self.shared.state.lock() {
            st.reader_closed = true;
            st.buf.clear();
        }
        self.shared.cv.notify_all();
    }
}

impl PipeWriter {
    /// End of file for the reader.
    pub fn close(&self) {
        if let Ok(mut st) = self.shared.state.lock() {
            st.writer_closed = true;
        }
        self.shared.cv.notify_all();
    }
}

impl Write for PipeWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut st = self
            .shared
            .state
            .lock()
            .map_err(|_| io::Error::other("pipe poisoned"))?;
        if st.reader_closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if st.writer_closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        st.buf.extend(data);
        drop(st);
        self.shared.cv.notify_all();
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn eof_notify_and_broken_pipe() {
        let seen = Arc::new(AtomicUsize::new(0));
        let s2 = seen.clone();
        let (mut w, mut r) = pipe(Some(Box::new(move |n| {
            s2.fetch_add(n, Ordering::SeqCst);
        })));
        w.write_all(b"hello").unwrap();
        drop(w);
        let mut out = String::new();
        r.read_to_string(&mut out).unwrap();
        assert_eq!(out, "hello");
        assert_eq!(seen.load(Ordering::SeqCst), 5);
        let (mut w, r) = pipe(None);
        drop(r);
        assert!(w.write(b"x").is_err());
    }
}
