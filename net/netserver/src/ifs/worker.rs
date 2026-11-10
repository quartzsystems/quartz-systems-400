// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS on a thread of its own. An [`IfsClient`] is not `Send` (the
//! job's link to the service belongs to one thread), so one thread makes
//! it and runs every call; the async side sends closures and awaits the
//! answers.

use std::sync::mpsc;
use std::thread;

use ifs::client::IfsClient;
use ifs::{Errno, Result};
use tokio::sync::oneshot;

type Job = Box<dyn FnOnce(&dyn IfsClient) + Send>;

/// Makes the IFS client on the worker thread.
pub type MakeClient = Box<dyn FnOnce() -> Box<dyn IfsClient> + Send>;

/// The thread that owns the IFS client.
pub struct IfsWorker {
    tx: mpsc::Sender<Job>,
}

impl IfsWorker {
    /// Starts the thread; `make` builds the client on it.
    ///
    /// # Panics
    ///
    /// When the thread cannot be started.
    #[must_use]
    pub fn start(make: MakeClient) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("netserver-ifs".into())
            .spawn(move || {
                let client = make();
                while let Ok(job) = rx.recv() {
                    job(&*client);
                }
            })
            .expect("IFS thread");
        Self { tx }
    }

    /// Runs `f` with the client and returns its answer; `EIO` when the
    /// thread is gone.
    pub async fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn IfsClient) -> Result<T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let job: Job = Box::new(move |c| {
            let _ = tx.send(f(c));
        });
        self.tx.send(job).map_err(|_| Errno::EIO)?;
        rx.await.unwrap_or(Err(Errno::EIO))
    }

    /// Runs `f` without waiting for it (closing descriptors on drop).
    pub fn post(&self, f: impl FnOnce(&dyn IfsClient) + Send + 'static) {
        let _ = self.tx.send(Box::new(f));
    }
}
