// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's link to the service: requests out, answers back, one at a
//! time. A lost link ends the job: without the service it can do
//! nothing, and the service has already cleaned up after it.

use std::cell::RefCell;
use std::rc::Rc;

use ipc::WireError;

use crate::proto::{Client, Request, Service, dispatch};

type Transport = Box<dyn FnMut(&Request) -> Result<Vec<u8>, WireError>>;

/// One connection to the service.
pub struct Link {
    transport: Transport,
    on_lost: fn(&WireError) -> !,
    calls: u64,
}

/// A link the job's sources share.
pub type Shared = Rc<RefCell<Link>>;

impl Link {
    /// Over a socket.
    #[cfg(unix)]
    #[must_use]
    pub fn over(mut chan: ipc::chan::Chan) -> Self {
        Self::new(Box::new(move |req: &Request| {
            chan.send(req)?;
            chan.recv_raw()
        }))
    }

    /// Straight into a service in this process, for tests.
    #[must_use]
    pub fn local(mut service: impl Service + 'static) -> Self {
        Self::new(Box::new(move |req: &Request| {
            let back: Request = ipc::decode(&ipc::encode(req))?;
            Ok(dispatch(&mut service, back))
        }))
    }

    fn new(transport: Transport) -> Self {
        Self {
            transport,
            on_lost: lost,
            calls: 0,
        }
    }

    /// Share it.
    #[must_use]
    pub fn shared(self) -> Shared {
        Rc::new(RefCell::new(self))
    }

    /// What to do when the service is gone; the default ends the
    /// process.
    pub fn on_lost(&mut self, f: fn(&WireError) -> !) {
        self.on_lost = f;
    }

    /// Requests sent.
    #[must_use]
    pub fn calls(&self) -> u64 {
        self.calls
    }

    /// Run a client call, ending the job if the link fails.
    pub fn with<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, WireError>) -> T {
        match f(self) {
            Ok(v) => v,
            Err(e) => (self.on_lost)(&e),
        }
    }
}

impl Client for Link {
    fn call(&mut self, req: &Request) -> Result<Vec<u8>, WireError> {
        self.calls += 1;
        (self.transport)(req)
    }
}

/// The default for a lost link: the job cannot go on.
fn lost(e: &WireError) -> ! {
    eprintln!("qsjob: storage service lost: {e}");
    std::process::exit(70)
}
