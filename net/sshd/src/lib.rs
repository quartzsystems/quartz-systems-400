// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 SSH server (`*SSHD`): SSH-2 written here over graviola's
//! pure-Rust primitives, an `sshd_config` subset at IBM i's path, host
//! keys made at first start, password sign-on against the security
//! store and keys from `~/.ssh/authorized_keys`, QSH sessions, and SFTP
//! v3 and scp over the IFS.
//!
//! - [`wire`]: SSH data types, message numbers, base64, PEM, DER bits.
//! - [`transport`]: identification, `KEXINIT`, key exchange, packets.
//! - [`keys`]: public and private keys, `authorized_keys`, host keys.
//! - [`config`]: `sshd_config` and the IBM i paths.
//! - [`auth`]: [`auth::Accounts`], the access rules, `ssh-userauth`.
//! - [`session`]: [`session::SessionHost`], the QSH host, transfers.
//! - [`sftp`], [`scp`]: file transfer over [`ifs::client::IfsClient`].
//! - [`conn`]: one connection's channels and forwarding.
//! - [`server`]: [`server::Server`], the entry point per connection;
//!   [`daemon`]: the listener `STRTCPSVR *SSHD` runs.
//! - [`client`]: a small client for tests and tools.
//!
//! `docs/SSHD.md` is the reference.

#![deny(missing_docs)]
// Protocol code uses short names for messages, readers and writers;
// channels and options carry several flags.
#![allow(
    clippy::many_single_char_names,
    clippy::struct_excessive_bools,
    clippy::match_same_arms
)]

pub mod auth;
pub mod client;
pub mod config;
pub mod conn;
pub mod daemon;
pub mod error;
pub mod io;
pub mod keys;
pub mod scp;
pub mod server;
pub mod session;
pub mod sftp;
pub mod transport;
pub mod wire;

pub use daemon::Daemon;
pub use error::{SshError, SshResult};
pub use server::{Deps, Server, StartError};
