// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS for QS/400 (docs/NFS.md), without the job, the store or the
//! service:
//!
//! - [`xdr`]: the encoder and decoder the generated types use.
//! - [`types`]: every protocol type, generated from `x/*.x`.
//! - [`rpc`]: ONC RPC messages, record marking, authentication, the
//!   client transport and the server loop.
//! - [`rpcb`]: the binder (portmap 2, rpcbind 3 and 4) and `/etc/RPCBTAB`.
//! - [`mount`], [`v3`], [`v4`], [`nlm`], [`nsm`]: each protocol, server
//!   and client side.
//! - [`exports`]: `/etc/exports`, `/etc/netgroup`, the option lists of
//!   `EXPORTFS` and `MOUNT`, host matching.
//! - [`server`]: the `Vfs` trait, file handles, credentials, state.
//! - [`client`]: a mounted export as a remote file system.
//! - [`registry`]: the `NFSv4` identity mapping.
//! - [`fuzzing`]: the entry points of the fuzz targets.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod client;
pub mod exports;
pub mod fuzzing;
pub mod mount;
pub mod nlm;
pub mod nsm;
pub mod registry;
pub mod rpc;
pub mod rpcb;
pub mod server;
pub mod types;
pub mod v3;
pub mod v4;
pub mod xdr;
