// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP servers framework (docs/SERVERS.md), without the store or
//! the job runtime:
//!
//! - [`registry`]: every server `STRTCPSVR` names, its jobs with IBM's
//!   names and subsystems, its ports, attributes command, certificate
//!   application ID and exit points, and whether this release runs it.
//! - [`framework`]: the [`TcpServer`] hooks a server implements, the
//!   [`Env`] the job gives them (start and end jobs, read attributes),
//!   the [`Framework`] that answers `STRTCPSVR`/`ENDTCPSVR` and, as a
//!   `tcpcfg::server::ServerStarter`, `STRTCP`/`ENDTCP`.
//! - [`attrs`]: the servers' attributes in the `QUSRSYS` server table
//!   (`CHGTELNA`, `CHGFTPA`, the HTTP administration ports).
//! - [`listen`]: binding, the accept loop with its stop flag and TLS.
//! - [`http`]: the minimal HTTP/1.1 administration listener's page.
//! - [`msg`]: the message identifiers.

#![deny(missing_docs)]

pub mod attrs;
pub mod framework;
pub mod http;
pub mod listen;
pub mod msg;
pub mod registry;

pub use framework::{Env, Framework, Instance, JobStart, Refusal, Request, TcpServer};
pub use registry::{JobKind, JobSpec, Readiness, ServerSpec};
