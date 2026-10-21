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

use crate::Shared;
use crate::proto::Client;

/// How long a waiting job sleeps between tries.
pub const RETRY: Duration = Duration::from_millis(50);

/// Data areas and data queues.
pub struct RemoteData {
    link: Shared,
}

impl RemoteData {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl DataSource for RemoteData {
    fn dtaara_lock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.da_lock(name.into(), lib.into()))
    }

    fn dtaara_unlock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.da_unlock(name.into(), lib.into()))
    }

    fn dtaara_read(&mut self, name: &str, lib: &str) -> Result<DataAreaValue, IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.da_read(name.into(), lib.into()))
    }

    fn dtaara_write(&mut self, name: &str, lib: &str, value: &DataAreaValue) -> Result<(), IoFail> {
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
        let until = (wait > 0).then(|| {
            Instant::now() + Duration::from_secs(u64::try_from(wait).unwrap_or(0))
        });
        loop {
            let got = self
                .link
                .borrow_mut()
                .with(|l| l.dq_receive(name.into(), lib.into()))?;
            if got.is_some() {
                return Ok(got);
            }
            match until {
                Some(t) if Instant::now() < t => std::thread::sleep(RETRY),
                // A negative wait is for ever.
                None if wait < 0 => std::thread::sleep(RETRY),
                _ => return Ok(None),
            }
        }
    }
}

/// Files: the open data paths live in the service.
pub struct RemoteIo {
    link: Shared,
}

impl RemoteIo {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl IoSource for RemoteIo {
    fn open(&mut self, spec: &OpenSpec) -> Result<(u32, FileInfo), IoFail> {
        self.link.borrow_mut().with(|l| l.open(spec.clone()))
    }

    fn io(&mut self, handle: u32, req: &IoRequest<'_>) -> IoResult {
        let req = OwnedRequest::from_request(req);
        self.link.borrow_mut().with(|l| l.io(handle, req))
    }

    fn close(&mut self, handle: u32) -> IoResult {
        self.link.borrow_mut().with(|l| l.close_file(handle))
    }
}
