// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data areas, data queues and files from the service. A wait happens
//! here, in the job, between tries: the service never blocks on one
//! job's behalf.

use std::time::{Duration, Instant};

use mi::io::{
    DataAreaValue, DataSource, FileInfo, IoFail, IoRequest, IoResult, IoSource, OpenSpec,
    OwnedRequest,
};

use wm::job::Status;

use crate::Shared;
use crate::proto::Client;

/// How long a waiting job sleeps between tries.
pub const RETRY: Duration = Duration::from_millis(50);

/// The job's local data area: 1024 bytes, blanks at the start.
pub type Lda = std::rc::Rc<std::cell::RefCell<Vec<u8>>>;

/// A fresh `*LDA`.
#[must_use]
pub fn new_lda() -> Lda {
    std::rc::Rc::new(std::cell::RefCell::new(vec![0x40; LDA_LEN]))
}

/// The `*LDA`'s length.
pub const LDA_LEN: usize = 1024;

/// Data areas and data queues. `*LDA` is the job's own and never
/// leaves the process.
pub struct RemoteData {
    link: Shared,
    lda: Lda,
}

impl RemoteData {
    /// Over a link, with the job's `*LDA`.
    #[must_use]
    pub fn new(link: Shared, lda: Lda) -> Self {
        Self { link, lda }
    }
}

impl DataSource for RemoteData {
    fn dtaara_lock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        if name == "*LDA" || name == "*GDA" {
            return Ok(());
        }
        self.link
            .borrow_mut()
            .with(|l| l.da_lock(name.into(), lib.into()))
    }

    fn dtaara_unlock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        if name == "*LDA" || name == "*GDA" {
            return Ok(());
        }
        self.link
            .borrow_mut()
            .with(|l| l.da_unlock(name.into(), lib.into()))
    }

    fn dtaara_read(&mut self, name: &str, lib: &str) -> Result<DataAreaValue, IoFail> {
        if name == "*GDA" {
            let b = self
                .link
                .borrow_mut()
                .with(Client::gda_read)
                .map_err(|_| IoFail::new(1299, "CPF1311", &[]))?;
            let mut v = DataAreaValue::chars(u32::try_from(b.len()).unwrap_or(512));
            v.value = b;
            return Ok(v);
        }
        if name == "*LDA" {
            let mut v = DataAreaValue::chars(1024);
            v.value.clone_from(&self.lda.borrow());
            return Ok(v);
        }
        self.link
            .borrow_mut()
            .with(|l| l.da_read(name.into(), lib.into()))
    }

    fn dtaara_write(&mut self, name: &str, lib: &str, value: &DataAreaValue) -> Result<(), IoFail> {
        if name == "*GDA" {
            let mut b = value.value.clone();
            b.resize(512, 0x40);
            return self
                .link
                .borrow_mut()
                .with(|l| l.gda_write(1, b))
                .map_err(|_| IoFail::new(1299, "CPF1311", &[]));
        }
        if name == "*LDA" {
            let mut b = value.value.clone();
            b.resize(LDA_LEN, 0x40);
            *self.lda.borrow_mut() = b;
            return Ok(());
        }
        self.link
            .borrow_mut()
            .with(|l| l.da_write(name.into(), lib.into(), value.clone()))
    }

    fn dtaq_send(&mut self, name: &str, lib: &str, data: &[u8]) -> Result<(), IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.dq_send(name.into(), lib.into(), data.to_vec()))
    }

    fn dtaq_receive(
        &mut self,
        name: &str,
        lib: &str,
        wait: i64,
    ) -> Result<Option<Vec<u8>>, IoFail> {
        let until = (wait > 0)
            .then(|| Instant::now() + Duration::from_secs(u64::try_from(wait).unwrap_or(0)));
        let mut waiting = false;
        let r = loop {
            let got = self
                .link
                .borrow_mut()
                .with(|l| l.dq_receive(name.into(), lib.into()));
            if !matches!(got, Ok(None)) {
                break got;
            }
            let more = match until {
                Some(t) => Instant::now() < t,
                // A negative wait is for ever.
                None => wait < 0,
            };
            if !more {
                break got;
            }
            if !waiting {
                waiting = true;
                status(&self.link, Status::Deqw, &format!("DEQW-{name}"));
            }
            std::thread::sleep(RETRY);
        };
        if waiting {
            status(&self.link, Status::Run, "");
        }
        r
    }
}

thread_local! {
    /// The function the job last reported, kept across a display wait.
    static FUNCTION: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Tell the service the job waits at its display (its activity level
/// goes to another job) or runs again, its function unchanged.
pub fn display_wait(link: &Shared, waiting: bool) {
    let function = FUNCTION.with(|f| f.borrow().clone());
    status(link, if waiting { Status::Dspw } else { Status::Run }, &function);
}

/// Tell the service what the job is doing. A runnable job waits here,
/// ineligible, until its pool has a free activity level.
pub fn status(link: &Shared, s: Status, function: &str) {
    FUNCTION.with(|f| function.clone_into(&mut f.borrow_mut()));
    loop {
        let granted = link
            .borrow_mut()
            .with(|l| l.set_status(s.byte(), function.to_string()));
        if granted || !s.runnable() {
            return;
        }
        std::thread::sleep(RETRY);
    }
}

/// Files: the open data paths live in the service. A record another job
/// holds is waited for here, a try every 50 ms, as `WAITRCD` says.
pub struct RemoteIo {
    link: Shared,
    waits: std::collections::HashMap<u32, i64>,
}

impl RemoteIo {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self {
            link,
            waits: std::collections::HashMap::new(),
        }
    }
}

/// The status of a record another job holds.
const LOCKED: u16 = 1218;

impl IoSource for RemoteIo {
    fn open(&mut self, spec: &OpenSpec) -> Result<(u32, FileInfo), IoFail> {
        let r = self.link.borrow_mut().with(|l| l.open(spec.clone()));
        if let Ok((h, info)) = &r {
            self.waits.insert(*h, info.wait_ms);
        }
        r
    }

    fn io(&mut self, handle: u32, req: &IoRequest<'_>) -> IoResult {
        let req = OwnedRequest::from_request(req);
        let wait = self.waits.get(&handle).copied().unwrap_or(0);
        let until = Instant::now()
            + Duration::from_millis(u64::try_from(wait).unwrap_or(0));
        let mut waited = false;
        loop {
            let r = self.link.borrow_mut().with(|l| l.io(handle, req.clone()));
            let more = wait < 0 || Instant::now() < until;
            if r.status != LOCKED || wait == 0 || !more {
                if waited {
                    status(&self.link, Status::Run, "");
                }
                return r;
            }
            if !waited {
                status(&self.link, Status::Lckw, "");
                waited = true;
            }
            std::thread::sleep(RETRY);
        }
    }

    fn close(&mut self, handle: u32) -> IoResult {
        self.link.borrow_mut().with(|l| l.close_file(handle))
    }
}
