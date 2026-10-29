// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A writer's destination and the one call that prints to it: the
//! transform the destination asks for, then its sink. A LAN printer
//! (`CRTDEVPRT DEVCLS(*LAN) LANATTACH(*IP)`) is reached by its
//! `SYSDRVPGM`; a remote output queue (`CRTOUTQ RMTSYS() CNNTYPE(*IP)`)
//! by LPR.

use std::sync::Arc;

use crate::job::PrintJob;
use crate::sink::ipp::{IppSink, Uri};
use crate::sink::lpr::{DestOpt, DestType, LprSink};
use crate::sink::raw::{self, RawSink};
use crate::sink::{Receipt, Sink, SinkError, TlsConnector};
use crate::transform::{Format, Transform};

/// `SYSDRVPGM`: how a LAN printer is driven.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Driver {
    /// `*HPPJLDRV`: the raw port, a PJL job round each file.
    #[default]
    HpPjl,
    /// `*IBMPJLDRV`: the same for IBM's PJL printers.
    IbmPjl,
    /// `*IBMSNMPDRV`: the raw port, no PJL; SNMP status is not read.
    IbmSnmp,
    /// `*NETSTNDRV`: the raw port as it is.
    NetStn,
    /// `*IBMIPPDRV`: IPP/1.1.
    Ipp,
}

impl Driver {
    /// From `SYSDRVPGM`; `*IPPDRV` is taken for `*IBMIPPDRV`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "*HPPJLDRV" => Self::HpPjl,
            "*IBMPJLDRV" => Self::IbmPjl,
            "*IBMSNMPDRV" => Self::IbmSnmp,
            "*NETSTNDRV" => Self::NetStn,
            "*IBMIPPDRV" | "*IPPDRV" => Self::Ipp,
            _ => return None,
        })
    }

    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::HpPjl => "*HPPJLDRV",
            Self::IbmPjl => "*IBMPJLDRV",
            Self::IbmSnmp => "*IBMSNMPDRV",
            Self::NetStn => "*NETSTNDRV",
            Self::Ipp => "*IBMIPPDRV",
        }
    }

    /// The port used when `PORT` is not given.
    #[must_use]
    pub fn default_port(self) -> u16 {
        match self {
            Self::Ipp => crate::sink::ipp::PORT,
            _ => raw::PORT,
        }
    }
}

/// Where a writer prints.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A LAN printer device.
    Lan {
        /// `RMTLOCNAME`: host name, address, or for IPP a printer URI.
        host: String,
        /// `PORT`; 0 for the driver's.
        port: u16,
        /// `SYSDRVPGM`.
        driver: Driver,
        /// `SECURECNN(*YES)`: IPP over TLS.
        secure: bool,
        /// `TRANSFORM` and `MFRTYPMDL`.
        transform: Transform,
    },
    /// A remote output queue.
    Remote {
        /// `RMTSYS`, or `INTNETADR` with `RMTSYS(*INTNETADR)`.
        host: String,
        /// The LPD port; 0 for 515.
        port: u16,
        /// `RMTPRTQ`, resolved.
        queue: String,
        /// `DESTTYPE`.
        desttype: DestType,
        /// `DESTOPT`.
        destopt: String,
        /// `SEPPAGE`.
        seppage: bool,
        /// `TRANSFORM` and `MFRTYPMDL`; ignored for an IBM i receiver.
        transform: Transform,
        /// This system's host name.
        local_host: String,
    },
}

/// What [`deliver`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivered {
    /// The sink's receipt.
    pub receipt: Receipt,
    /// The format sent.
    pub format: Format,
}

/// `RMTPRTQ` as sent: `*USER` is the spooled file's owner, `*SYSTEM`
/// the receiver's default queue (`QPRINT`), anything else as given.
#[must_use]
pub fn queue_name(rmtprtq: &str, job: &PrintJob) -> String {
    match rmtprtq.trim().to_ascii_uppercase().as_str() {
        "" | "*USER" => job.job.1.clone(),
        "*SYSTEM" => "QPRINT".into(),
        _ => rmtprtq.trim().to_string(),
    }
}

impl Target {
    /// The transform it applies.
    #[must_use]
    pub fn transform(&self) -> Transform {
        match self {
            Self::Lan { transform, .. } => transform.clone(),
            Self::Remote {
                desttype,
                transform,
                ..
            } => {
                if desttype.ibm_i() {
                    Transform::Raw
                } else {
                    transform.clone()
                }
            }
        }
    }

    /// The sink it sends through.
    pub fn sink(&self, job: &PrintJob, tls: Arc<dyn TlsConnector>) -> Result<Box<dyn Sink>, SinkError> {
        Ok(match self {
            Self::Lan {
                host,
                port,
                driver,
                secure,
                ..
            } => {
                let port = if *port == 0 { driver.default_port() } else { *port };
                match driver {
                    Driver::Ipp => {
                        let mut uri = Uri::parse(host, *secure)?;
                        if !host.contains("://") {
                            uri.port = port;
                        }
                        Box::new(IppSink::new(uri, tls))
                    }
                    Driver::HpPjl | Driver::IbmPjl => Box::new(RawSink::new(host, port, true)),
                    Driver::IbmSnmp | Driver::NetStn => Box::new(RawSink::new(host, port, false)),
                }
            }
            Self::Remote {
                host,
                port,
                queue,
                desttype,
                destopt,
                seppage,
                local_host,
                ..
            } => {
                let mut s = LprSink::new(host, &queue_name(queue, job), local_host);
                if *port != 0 {
                    s.endpoint.port = *port;
                }
                s.desttype = *desttype;
                s.destopt = DestOpt::parse(destopt);
                s.seppage = *seppage;
                Box::new(s)
            }
        })
    }
}

/// Transform `scs` and send it through `sink`.
pub fn deliver_with(
    sink: &mut dyn Sink,
    transform: &Transform,
    job: &PrintJob,
    scs: &[u8],
) -> Result<Delivered, SinkError> {
    let doc = transform.apply(job, scs);
    let receipt = sink.send(job, &doc)?;
    Ok(Delivered {
        receipt,
        format: doc.format,
    })
}

/// Print `scs`, the data of `job`, at `target`.
pub fn deliver(
    target: &Target,
    job: &PrintJob,
    scs: &[u8],
    tls: Arc<dyn TlsConnector>,
) -> Result<Delivered, SinkError> {
    let mut sink = target.sink(job, tls)?;
    deliver_with(sink.as_mut(), &target.transform(), job, scs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Model;

    #[test]
    fn drivers_and_queues() {
        assert_eq!(Driver::parse("*ippdrv"), Some(Driver::Ipp));
        assert_eq!(Driver::Ipp.name(), "*IBMIPPDRV");
        assert_eq!(Driver::HpPjl.default_port(), 9100);
        let job = PrintJob::default();
        assert_eq!(queue_name("*USER", &job), "QUSER");
        assert_eq!(queue_name("*SYSTEM", &job), "QPRINT");
        assert_eq!(queue_name(" lp0 ", &job), "lp0");
        let t = Target::Remote {
            host: "h".into(),
            port: 0,
            queue: "*USER".into(),
            desttype: DestType::Os400,
            destopt: String::new(),
            seppage: true,
            transform: Transform::Pcl(Model::Hp4),
            local_host: "me".into(),
        };
        assert_eq!(t.transform(), Transform::Raw);
    }
}
