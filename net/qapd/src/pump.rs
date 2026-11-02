// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The thread that owns the connection: it reads frames, holds the
//! outstanding requests (refusing a duplicate ID, and the 65th with
//! `Busy`), marks cancels, writes what the connection's thread sends
//! and closes the connection after [`Config::idle_close`] without a
//! frame.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use qap_proto::{Envelope, Kind, Status};

use crate::frames::{FrameReader, frame};
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

/// Whether request `id` was cancelled.
pub fn cancelled(c: &Cancels, id: u32) -> bool {
    c.lock().unwrap_or_else(PoisonError::into_inner).contains(&id)
}

struct Pump<T: Transport> {
    t: T,
    cfg: Config,
    tx: Sender<In>,
    cancels: Cancels,
    outstanding: HashSet<u32>,
}

impl<T: Transport> Pump<T> {
    fn write(&mut self, env: &Envelope) -> bool {
        let Ok(b) = frame(env) else {
            return false;
        };
        if self.t.write_all(&b).and_then(|()| self.t.flush()).is_err() {
            return false;
        }
        if env.kind == Kind::Reply && !env.more() {
            self.outstanding.remove(&env.request_id);
            self.cancels.lock().unwrap_or_else(PoisonError::into_inner).remove(&env.request_id);
        }
        true
    }

    /// A frame from the client; false when the connection cannot go on.
    fn take(&mut self, env: Envelope) -> bool {
        match env.kind {
            Kind::Request => {
                let refuse = if env.request_id == 0 || self.outstanding.contains(&env.request_id) {
                    Some(Status::InvalidRequest)
                } else if self.outstanding.len() >= self.cfg.max_outstanding {
                    Some(Status::Busy)
                } else {
                    None
                };
                // A refusal is not counted: its reply goes out at once.
                if let Some(status) = refuse {
                    let b = frame(&Envelope::reply(&env, status, None, Vec::new()));
                    b.is_ok_and(|b| self.t.write_all(&b).and_then(|()| self.t.flush()).is_ok())
                } else {
                    self.outstanding.insert(env.request_id);
                    self.tx.send(In::Request(env)).is_ok()
                }
            }
            Kind::Cancel => {
                if self.outstanding.contains(&env.request_id) {
                    self.cancels
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(env.request_id);
                }
                true
            }
            // Replies and events are the server's to send.
            Kind::Reply | Kind::Event => true,
        }
    }

    fn run(mut self, rx: &Receiver<Out>) {
        let mut frames = FrameReader::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut last = Instant::now();
        loop {
            loop {
                match rx.try_recv() {
                    Ok(Out::Frames(v)) => {
                        for e in &v {
                            if !self.write(e) {
                                let _ = self.tx.send(In::Closed(Closed::Broken));
                                return;
                            }
                        }
                    }
                    Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                        self.t.close();
                        return;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            match self.t.read(&mut buf) {
                Ok(0) => {
                    let _ = self.tx.send(In::Closed(Closed::Peer));
                    return;
                }
                Ok(n) => {
                    last = Instant::now();
                    frames.push(&buf[..n]);
                    loop {
                        match frames.next_frame() {
                            Ok(Some(env)) => {
                                if !self.take(env) {
                                    let _ = self.tx.send(In::Closed(Closed::Broken));
                                    return;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => {
                                let _ = self.tx.send(In::Closed(Closed::BadFrame));
                                self.t.close();
                                return;
                            }
                        }
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                Err(_) => {
                    let _ = self.tx.send(In::Closed(Closed::Broken));
                    return;
                }
            }
            if last.elapsed() >= self.cfg.idle_close {
                let _ = self.tx.send(In::Closed(Closed::Idle));
                self.t.close();
                return;
            }
        }
    }
}

/// Run the pump over `t` until the connection closes.
pub fn run<T: Transport>(t: T, cfg: Config, tx: Sender<In>, rx: &Receiver<Out>, cancels: Cancels) {
    Pump {
        t,
        cfg,
        tx,
        cancels,
        outstanding: HashSet::new(),
    }
    .run(rx);
}
