// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer's configuration (`docs/NETSERVER.md` sections 4 and the
//! APIs): the server attributes with current and pending values, file
//! and print shares and their rules ([`model`]), the `QAZLSCFG` and
//! `QAZLSSHR` files ([`file`]), the `ZLSL*` and `ZLSS*` formats
//! ([`fmt`]), and the `QZLS*` APIs over their parameters' bytes
//! ([`api`]) against a [`api::Host`] the storage service implements.

#![deny(missing_docs)]
// Names like NetServer and NetBIOS read better without backticks.
#![allow(
    clippy::doc_markdown,
    clippy::many_single_char_names,
    clippy::struct_excessive_bools
)]

pub mod api;
pub mod file;
pub mod fmt;
pub mod model;

pub use model::attrs::{Attributes, Dialect, Settings};
pub use model::config::Config;
pub use model::session::{Connection, Counter, Session, SessionTable, Stats};
pub use model::share::{FileShare, Kind, Permission, PrintShare, Qualified, Share, SpoolType};
