// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service for NetServer's threads. The job's link belongs
//! to its own thread, and the SMB server calls sign-on, the spooler and
//! the session table from its runtime's thread, so one thread of the job
//! holds a link of its own (`hello_thread`: it shares the job's security
//! session, so a sign-on's swap is the job's) and runs the calls sent to
//! it.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Mutex, PoisonError, mpsc};
use std::thread;

use ipc::chan::Chan;
use stg::{Client, Link, Shared};

type Call = Box<dyn FnOnce(&Shared) + Send>;

/// A link to the service on a thread of its own.
pub struct Svc {
    tx: Mutex<mpsc::Sender<Call>>,
}

/// A thread's link of job `number` in process `pid`.
pub fn thread_link(number: u32, pid: u32) -> Result<Shared, String> {
    let chan = Chan::connect(Path::new(stg::SOCKET)).map_err(|e| e.to_string())?;
    let mut link = Link::over(chan);
    link.with(|l| l.hello_thread(number, pid))?;
    Ok(Rc::new(RefCell::new(link)))
}

impl Svc {
    /// Starts the thread for job `number`.
    ///
    /// # Errors
    ///
    /// The link could not be made.
    pub fn start(number: u32) -> Result<Self, String> {
        let pid = std::process::id();
        let (tx, rx) = mpsc::channel::<Call>();
        let (ok_tx, ok_rx) = mpsc::channel();
        thread::Builder::new()
            .name("netserver-svc".into())
            .spawn(move || {
                let link = match thread_link(number, pid) {
                    Ok(l) => {
                        let _ = ok_tx.send(Ok(()));
                        l
                    }
                    Err(e) => {
                        let _ = ok_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(call) = rx.recv() {
                    call(&link);
                }
            })
            .map_err(|e| e.to_string())?;
        ok_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { tx: Mutex::new(tx) })
    }

    /// Runs `f` with the thread's link and waits for its answer; `None`
    /// when the thread is gone.
    pub fn call<T: Send + 'static>(&self, f: impl FnOnce(&Shared) -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        let call: Call = Box::new(move |l| {
            let _ = tx.send(f(l));
        });
        self.tx.lock().unwrap_or_else(PoisonError::into_inner).send(call).ok()?;
        rx.recv().ok()
    }
}
