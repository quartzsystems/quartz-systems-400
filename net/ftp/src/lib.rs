// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FTP for QS/400 (docs/FTP.md): the `*FTP` server's protocol, as
//! IBM's server speaks it, and the `FTP` CL client.
//!
//! - [`session`]: one control connection's state and the answer to
//!   each command: sign-on (with `QIBM_QTMF_SVR_LOGON`), `NAMEFMT 0`
//!   and `1`, `LISTFMT 0` and `1`, `SITE`, `RCMD`, transfers of stream
//!   files, members (with CCSID conversion and records), save files,
//!   files made on `STOR`, RFC 2228 TLS, RFC 2428 and RFC 3659.
//! - [`host`]: what a session needs from its job ([`host::Host`]).
//! - [`net`]: the connections ([`net::Net`]) over sockets or in
//!   memory; [`server`] runs a session over a socket.
//! - [`names`], [`objects`], [`xlate`], [`list`]: names and paths,
//!   reading and writing objects, the data representations, listing
//!   lines; [`store`]: objects in a transfer's form, for the server
//!   and the client.
//! - [`client`]: the `FTP` command's subcommands, against any server.
//! - [`reply`]: replies and reading them; [`block`]: block mode.
//! - [`memhost`]: a host in memory, for tests and the transcript
//!   replay.

#![deny(missing_docs)]

pub mod block;
pub mod client;
pub mod host;
pub mod list;
pub mod memhost;
pub mod names;
pub mod net;
pub mod objects;
pub mod reply;
pub mod server;
pub mod session;
pub mod store;
pub mod xlate;

pub use host::{CmdFail, DbFile, Host, Profile, Refused};
pub use reply::Reply;
pub use session::{Config, Flow, Session};
