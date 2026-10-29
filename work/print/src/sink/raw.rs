// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The raw port of a LAN printer (9100 by convention): the data as it
//! is, wrapped in a PJL job for the PJL drivers. Nothing is read back
//! but the close: no PJL status, no SNMP (`*IBMSNMPDRV` sends the
//! same bytes and reports no printer state).

use std::io::{Read, Write};
use std::time::Duration;

use super::{Endpoint, Receipt, Sink, SinkError, Stream};
use crate::job::PrintJob;
use crate::pjl;
use crate::transform::Output;

/// The raw port's conventional number.
pub const PORT: u16 = 9100;

/// How long to wait for the printer to close once the data is sent.
const LINGER: Duration = Duration::from_secs(2);

/// A raw-port printer.
#[derive(Clone, Debug)]
pub struct RawSink {
    /// Where it is.
    pub endpoint: Endpoint,
    /// Wrap each file in a PJL job.
    pub pjl: bool,
}

impl RawSink {
    /// A printer at `host:port`.
    #[must_use]
    pub fn new(host: &str, port: u16, pjl: bool) -> Self {
        Self {
            endpoint: Endpoint::new(host, port),
            pjl,
        }
    }

    /// The bytes sent for `doc`.
    #[must_use]
    pub fn bytes(&self, job: &PrintJob, doc: &Output) -> Vec<u8> {
        if self.pjl {
            pjl::wrap(&job.title(), doc.format.pjl(), job.copies, &doc.data)
        } else {
            doc.data.clone()
        }
    }
}

impl Sink for RawSink {
    fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError> {
        let data = self.bytes(job, doc);
        let mut s = self.endpoint.connect()?;
        s.write_all(&data).map_err(|e| SinkError::io(&e))?;
        s.flush().map_err(|e| SinkError::io(&e))?;
        s.finish().map_err(|e| SinkError::io(&e))?;
        // Let the printer take it all before the socket goes; what it
        // says (if anything) is not read.
        let _ = s.set_read_timeout(Some(LINGER));
        let mut sink = [0u8; 512];
        while matches!(s.read(&mut sink), Ok(n) if n > 0) {}
        Ok(Receipt {
            bytes: data.len() as u64,
            job_id: None,
            detail: format!(
                "{} bytes to {}:{}",
                data.len(),
                self.endpoint.host,
                self.endpoint.port
            ),
        })
    }
}
