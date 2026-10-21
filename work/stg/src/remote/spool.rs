// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's spool sink: printer files open spooled files in the
//! service, which keeps them; the job sends the SCS in chunks.

use mi::io::IoFail;
use mi::io::spool::{PrinterOpen, SpoolSink};

use crate::Shared;
use crate::proto::Client;

/// The sink over a link.
pub struct RemoteSpool {
    link: Shared,
}

impl RemoteSpool {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

/// A service failure as an I/O failure: its message identifier leads.
fn fail(e: &str, names: &[&str]) -> IoFail {
    let id = e
        .split_whitespace()
        .next()
        .filter(|w| w.len() == 7)
        .unwrap_or("CPF4101");
    IoFail::new(1299, id, names)
}

impl SpoolSink for RemoteSpool {
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<PrinterOpen, IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.prt_open(file.into(), lib.into(), actgrp.into()))
            .map(|(id, prtf)| PrinterOpen { id, prtf })
            .map_err(|e| fail(&e, &[file, lib]))
    }

    fn put(&mut self, id: u32, data: &[u8], pages: u32, records: u32) -> Result<(), IoFail> {
        if data.is_empty() {
            return Ok(());
        }
        self.link
            .borrow_mut()
            .with(|l| l.splf_put(id, data.to_vec(), pages, records))
            .map_err(|e| fail(&e, &[]))
    }

    fn close(&mut self, id: u32, hold: bool) -> Result<(), IoFail> {
        self.link
            .borrow_mut()
            .with(|l| l.splf_close(id, hold))
            .map_err(|e| fail(&e, &[]))
    }
}
