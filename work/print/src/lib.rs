// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printing over the network for QS/400. A writer job takes a spooled
//! file (SCS) and its attributes ([`job::PrintJob`]), transforms it for
//! the printer ([`transform`]: ASCII text, PCL5 for the HP Laserjet
//! family, a workstation customizing object, PDF) and hands it to a
//! sink ([`sink`]: raw port 9100 with PJL, IPP/1.1, LPR with the
//! IBM-to-IBM control file). [`deliver`] joins the two for a target the
//! writer describes. Nothing here knows the object store; the writer
//! job maps `*DEVD`, `*OUTQ` and `*SPLF` payloads onto these types.
//! `docs/PRINT.md` describes the design and what is unverified.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod deliver;
pub mod job;
pub mod pjl;
pub mod sink;
pub mod transform;
pub mod wscst;

pub use deliver::{Delivered, Driver, Target, deliver};
pub use job::{Duplex, PrintJob};
pub use sink::{Receipt, Sink, SinkError, SinkErrorKind};
pub use transform::{Model, Transform};
pub use wscst::Wscst;
