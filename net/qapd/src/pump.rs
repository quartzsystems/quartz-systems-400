// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The thread that owns the connection: it reads frames, holds the
//! outstanding requests (refusing a duplicate ID, and the 65th with
//! `Busy`), marks cancels, writes what the connection's thread sends
//! and closes the connection after [`Config::idle_close`] without a
//! frame either way while nothing is outstanding.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use qap_proto::{Envelope, Kind, Status};

use crate::conn::lock;
use crate::frames::{FrameReader, frame};
use crate::watch::Watch;
use crate::{Closed, Config, Transport};

/// From the pump to the connection's thread.
#[derive(Debug)]
pub enum In {
    /// A request to answer.
    Request(Envelope),
    /// The connection is gone.
    Closed(Closed),
}

/// From the connection's thread to the pump.
#[derive(Debug)]
pub enum Out {
    /// Frames to write in order.
    Frames(Vec<Envelope>),
    /// Write nothing more; close.
    Close,
}

/// Requests the client cancelled, by ID.
pub type Cancels = Arc<Mutex<HashSet<u32>>>;

/// The request the connection's thread is running, and what ends it.
#[derive(Default)]
pub struct Running {
    /// Its ID.
    pub id: Mutex<Option<u32>>,
    /// The host's canceller.
    pub cancel: Option<crate::host::Canceller>,
}

/// Whether request `id` was cancelled.
pub fn cancelled(c: &Cancels, id: u32) -> bool {
    c.lock().unwrap_or_else(PoisonError::into_inner).contains(&id)
}

/// What the pump runs with.
pub struct Ends {
    /// To the dispatcher.
    pub tx: Sender<In>,
    /// From the connection's threads.
    pub rx: Receiver<Out>,
    /// The cancels.
    pub cancels: Cancels,
    /// The command running.
    pub running: Arc<Running>,
    /// The outstanding requests, for the close line too.
    pub watch: Arc<Watch>,
}

struct Pump<T: Transport> {
    t: T,
    cfg: Config,
    e: Ends,
    /// When a frame last went either way.
    last: Instant,
}

impl<T: Transport> Pump<T> {
    fn send_frame(&mut self, env: &Envelope) -> Result<(), Closed> {
        let b = frame(env).map_err(|e| Closed::Broken(format!("framing a reply: {e}")))?;
        self.t
            .write_all(&b)
            .and_then(|()| self.t.flush())
            .map_err(|e| Closed::Broken(format!("write: {e}")))?;
        self.last = Instant::now();
        Ok(())
    }

    fn write(&mut self, env: &Envelope) -> Result<(), Closed> {
        self.send_frame(env)?;
        if env.kind == Kind::Reply && !env.more() {
            lock(&self.e.watch.outstanding).remove(&env.request_id);
            lock(&self.e.cancels).remove(&env.request_id);
        }
        Ok(())
    }

    /// A frame from the client; `Err` when the connection cannot go on.
    fn take(&mut self, env: Envelope) -> Result<(), Closed> {
        match env.kind {
            Kind::Request => {
                let refuse = {
                    let mut out = lock(&self.e.watch.outstanding);
                    if env.request_id == 0 || out.contains(&env.request_id) {
                        Some(Status::InvalidRequest)
                    } else if out.len() >= self.cfg.max_outstanding {
                        Some(Status::Busy)
                    } else {
                        out.insert(env.request_id);
                        None
                    }
                };
                // A refusal is not counted: its reply goes out at once.
                if let Some(status) = refuse {
                    self.send_frame(&Envelope::reply(&env, status, None, Vec::new()))
                } else {
                    // A dispatcher that ended has told the connection's
                    // thread why: its `Close` comes after what it sent.
                    let _ = self.e.tx.send(In::Request(env));
                    Ok(())
                }
            }
            Kind::Cancel => {
                if lock(&self.e.watch.outstanding).contains(&env.request_id) {
                    lock(&self.e.cancels).insert(env.request_id);
                    // Running now: end it where it is.
                    let now = *lock(&self.e.running.id);
                    if now == Some(env.request_id)
                        && let Some(c) = &self.e.running.cancel
                    {
                        c();
                    }
                }
                Ok(())
            }
            // Replies and events are the server's to send.
            Kind::Reply | Kind::Event => Ok(()),
        }
    }

    /// Write what is waiting; `Ok(false)` once told to close.
    fn drain(&mut self) -> Result<bool, Closed> {
        loop {
            match self.e.rx.try_recv() {
                Ok(Out::Frames(v)) => {
                    for env in &v {
                        self.write(env)?;
                    }
                }
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => return Ok(false),
                Err(TryRecvError::Empty) => return Ok(true),
            }
        }
    }

    /// Read what has come; `Err` when the connection is over.
    fn read(&mut self, frames: &mut FrameReader, buf: &mut [u8]) -> Result<(), Closed> {
        match self.t.read(buf) {
            Ok(0) => Err(Closed::Peer),
            Ok(n) => {
                self.last = Instant::now();
                frames.push(&buf[..n]);
                while let Some(env) = frames.next_frame().map_err(|_| Closed::BadFrame)? {
                    self.take(env)?;
                }
                Ok(())
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => Ok(()),
            Err(e) => Err(Closed::Broken(format!("read: {e}"))),
        }
    }

    /// Idle: no frame either way for the limit, and no request waiting
    /// for its reply (a long command is not idleness).
    fn idle(&self) -> bool {
        self.last.elapsed() >= self.cfg.idle_close && lock(&self.e.watch.outstanding).is_empty()
    }

    fn run(mut self) {
        let mut frames = FrameReader::new();
        let mut buf = vec![0u8; 64 * 1024];
        let why = loop {
            match self.drain() {
                Ok(true) => {}
                Ok(false) => {
                    self.t.close();
                    return;
                }
                Err(why) => break why,
            }
            if let Err(why) = self.read(&mut frames, &mut buf) {
                break why;
            }
            if self.idle() {
                break Closed::Idle;
            }
        };
        self.t.close();
        let _ = self.e.tx.send(In::Closed(why));
    }
}

/// Run the pump over `t` until the connection closes.
pub fn run<T: Transport>(t: T, cfg: Config, e: Ends) {
    Pump {
        t,
        cfg,
        e,
        last: Instant::now(),
    }
    .run();
}
