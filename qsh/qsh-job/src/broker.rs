// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tasks from the shell's threads to the job's thread. A [`Broker`] is
//! the sending end, cloned into every `JobSys`; the [`Desk`] stays on
//! the job's thread, which serves the tasks against its [`Side`] while
//! the shell runs. A task's caller waits for its answer.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use qsh::sys::{Errno, SysError};

use crate::side::Side;

/// One operation for the job's thread.
type Task = Box<dyn FnOnce(&mut dyn Side) + Send>;

/// The sending end.
#[derive(Clone)]
pub struct Broker {
    tx: Sender<Task>,
}

/// The serving end, on the job's thread.
pub struct Desk {
    rx: Receiver<Task>,
}

/// A connected pair.
#[must_use]
pub fn pair() -> (Broker, Desk) {
    let (tx, rx) = mpsc::channel();
    (Broker { tx }, Desk { rx })
}

impl Broker {
    /// Run `f` on the job's thread and wait for its answer; `EIO` when
    /// the job no longer serves.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut dyn Side) -> T + Send + 'static,
    ) -> Result<T, SysError> {
        let (back, answer) = mpsc::sync_channel(1);
        let task: Task = Box::new(move |side| {
            let _ = back.send(f(side));
        });
        self.tx
            .send(task)
            .map_err(|_| SysError::new(Errno::EIO))?;
        answer.recv().map_err(|_| SysError::new(Errno::EIO))
    }

    /// Run `f` on the job's thread without waiting (closing a file as
    /// its last descriptor goes).
    pub fn post(&self, f: impl FnOnce(&mut dyn Side) + Send + 'static) {
        let _ = self.tx.send(Box::new(f));
    }
}

impl Desk {
    /// Serve whatever is waiting, then wait up to `wait` for more;
    /// whether any task ran.
    pub fn serve(&self, side: &mut dyn Side, wait: Duration) -> bool {
        match self.rx.recv_timeout(wait) {
            Ok(task) => {
                task(side);
                // Drain what came meanwhile, then stop.
                while let Ok(t) = self.rx.try_recv() {
                    t(side);
                }
                true
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => false,
        }
    }

    /// Serve tasks until `done` says so, checking it at least every
    /// `tick`.
    pub fn serve_until(&self, side: &mut dyn Side, tick: Duration, done: &mut dyn FnMut() -> bool) {
        while !done() {
            self.serve(side, tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs::client::{IfsClient, Unavailable};

    struct S(Unavailable);

    impl Side for S {
        fn ifs(&self) -> &dyn IfsClient {
            &self.0
        }
    }

    #[test]
    fn tasks_run_on_the_serving_thread() {
        let (b, d) = pair();
        let worker = std::thread::spawn(move || {
            let a = b.call(|_s| 41 + 1).unwrap();
            b.post(|_s| {});
            a
        });
        let mut side = S(Unavailable(ifs::Errno::ENOTSUP));
        let mut finished = false;
        d.serve_until(&mut side, Duration::from_millis(10), &mut || {
            if worker.is_finished() {
                finished = true;
            }
            finished
        });
        assert_eq!(worker.join().unwrap(), 42);
    }

    #[test]
    fn a_gone_desk_is_eio() {
        let (b, d) = pair();
        drop(d);
        assert_eq!(b.call(|_s| 1).unwrap_err().errno, Errno::EIO);
    }
}
