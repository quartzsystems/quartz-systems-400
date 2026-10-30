// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The peer's window for one channel, shared by the threads writing to
//! it, and the writer that turns their output into channel data.

use std::io::{self, Write};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

use super::Event;

#[derive(Default)]
struct State {
    window: u64,
    closed: bool,
}

/// How many bytes the peer will still take on a channel.
#[derive(Default)]
pub struct Credit {
    state: Mutex<State>,
    cv: Condvar,
}

impl Credit {
    /// A window of `n` bytes.
    #[must_use]
    pub fn new(n: u32) -> Arc<Self> {
        let c = Self::default();
        if let Ok(mut s) = c.state.lock() {
            s.window = u64::from(n);
        }
        Arc::new(c)
    }

    /// `WINDOW_ADJUST`.
    pub fn add(&self, n: u32) {
        if let Ok(mut s) = self.state.lock() {
            s.window = (s.window + u64::from(n)).min(u64::from(u32::MAX));
        }
        self.cv.notify_all();
    }

    /// The channel closed: writers fail from now on.
    pub fn close(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.closed = true;
        }
        self.cv.notify_all();
    }

    /// Take up to `want` bytes of window, waiting while it is empty; 0
    /// when the channel closed.
    pub fn take(&self, want: usize) -> usize {
        let Ok(mut s) = self.state.lock() else {
            return 0;
        };
        while s.window == 0 && !s.closed {
            s = match self.cv.wait(s) {
                Ok(s) => s,
                Err(_) => return 0,
            };
        }
        if s.closed {
            return 0;
        }
        let n = usize::try_from(s.window).unwrap_or(usize::MAX).min(want);
        s.window -= n as u64;
        n
    }
}

/// Output of a channel: standard output, or standard error as extended
/// data.
pub struct ChannelWriter {
    pub(super) tx: Sender<Event>,
    pub(super) credit: Arc<Credit>,
    pub(super) chan: u32,
    pub(super) ext: bool,
    pub(super) max_packet: usize,
}

impl Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let n = self.credit.take(data.len().min(self.max_packet));
        if n == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.tx
            .send(Event::Data {
                chan: self.chan,
                ext: self.ext,
                data: data[..n].to_vec(),
            })
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
