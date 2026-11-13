// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder: portmap 2, rpcbind 3 and 4 (RFC 1833).
//!
//! - [`binder`]: the [`Binder`] program over the registration table.
//! - [`table`]: the table and the counts of `GETSTAT`.
//! - [`uaddr`]: universal and transport addresses, netids.
//! - [`rpcbtab`]: `/etc/RPCBTAB` as IBM writes it.
//! - [`client`]: a [`BinderClient`] to register and look up.

pub mod binder;
pub mod client;
pub mod rpcbtab;
pub mod table;
pub mod uaddr;

pub use binder::Binder;
pub use client::BinderClient;
pub use table::Table;
